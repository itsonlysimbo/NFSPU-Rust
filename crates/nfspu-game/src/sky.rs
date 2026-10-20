//! Reading each track's own sky: `GameData/Track/Sky/<track>.{txt,fsh,bin}`.
//!
//! The `.env` files next to the tracks are not cube maps (see `formats.md`); the real sky is built
//! from three files per track:
//!
//! * `<track>.fsh` - pictures. `horz` (256x256) is a painted panorama of the mountains that sit
//!   behind the track, `hzcl` a 16x2 horizon haze colour, `st1a`/`st1b` the sky maps for weather
//!   region A and B, `sc1a`/`sc1b` the matching sky colours, and `cc1a`/`cm1a` white cloud layers.
//! * `<track>.txt` - which of those to use: the sky level type, two weather regions, and per layer
//!   the scale factors, how fast it scrolls, and the wind speed and direction.
//! * `<track>.bin` - verified atlas layout, then the weather tables (region blend, fog/rain/snow
//!   keyframes per road slice, flags), read by `nfspu_formats::weather`. The "colour records"
//!   below are those keyframes read the old way; they are fog colours, not horizon colours.

use nfspu_formats::shpi;

/// Superseded: the records are weather keyframes keyed by road slice (see
/// `nfspu_formats::weather`), not directions, and their colours are fog colours stored as
/// 0xAARRGGBB. Kept only for the old tests; the viewer no longer uses it.
/// Record positions have NOT been verified as directions. This blends nearby records and
/// falls back to their average; neither this mapping nor the blend radius is retail-verified.
/// Atlas geometry and UV validation do not establish the sky-colour interpretation.
pub fn horizon_tint(horizon: &[HorizonColour], angle: f32) -> [f32; 3] {
    if horizon.is_empty() {
        return [0.6, 0.7, 0.85];
    }
    // Guessed blend reach, retained independently of the verified atlas change.
    const REACH: f32 = 3.0 / 16.0;
    let a = angle - angle.floor();
    let mut sum = [0f32; 3];
    let mut weight = 0.0;
    for h in horizon {
        // How far apart two directions are round the horizon, the short way.
        let mut d = (h.angle - a).abs();
        if d > 0.5 {
            d = 1.0 - d;
        }
        if d > REACH {
            continue;
        }
        let w = 1.0 - d / REACH;
        let [r, g, b] = h.rgb();
        sum[0] += r * w;
        sum[1] += g * w;
        sum[2] += b * w;
        weight += w;
    }
    if weight <= f32::EPSILON {
        for h in horizon {
            let [r, g, b] = h.rgb();
            sum[0] += r;
            sum[1] += g;
            sum[2] += b;
        }
        let n = horizon.len() as f32;
        return [sum[0] / n, sum[1] / n, sum[2] / n];
    }
    [sum[0] / weight, sum[1] / weight, sum[2] / weight]
}

/// One weather region from `<track>.txt`: a sky map, a sky colour and up to three cloud layers.
#[derive(Debug, Clone, Default)]
pub struct Region {
    /// Picture name of the sky map (`st1a`), empty when the region has none.
    pub sky_map: String,
    /// Picture name of the second layer's colour (`sc1a`), empty when the region has none.
    pub sky_colour: String,
    /// Picture name of the third layer's colour (`sc2a`), empty or `none` when there is none.
    pub third: String,
    /// Picture names of the cloud layers, in draw order; `flat` and `none` mean no picture.
    pub clouds: Vec<String>,
    /// How much the layers are repeated across the sky, per axis, in the file's order.
    pub scale: [Vec<f32>; 2],
    /// How fast the layers scroll. Larger is slower: the file says 400 is slow, 50 is fast.
    pub move_ratio: Vec<f32>,
    /// Wind speed, 0 for none and 10 for a lot.
    pub wind_speed: Vec<f32>,
    /// Wind direction in degrees, 0 = north and 90 = east.
    pub wind_direction: Vec<f32>,
}

impl Region {
    /// One colour for the whole region: the first layer's picture (opaque, the sky itself) with
    /// the second and third layers' colours laid over it by their average alpha, halved for the
    /// cloud masks that leave gaps. Retail draws these as textured, scrolling layers; one colour
    /// is our simplification.
    pub fn tint(&self, pictures: &Pictures) -> [f32; 3] {
        let mut c = pictures
            .average(&self.sky_map)
            .or_else(|| pictures.average(&self.sky_colour))
            .unwrap_or([0.55, 0.62, 0.72]);
        for name in [&self.sky_colour, &self.third] {
            if let Some((rgb, a)) = pictures.average_with_alpha(name) {
                let k = (a * 0.5).clamp(0.0, 1.0);
                for i in 0..3 {
                    c[i] += (rgb[i] - c[i]) * k;
                }
            }
        }
        c
    }
}

/// The pictures from `<track>.fsh`, decoded to RGBA8.
#[derive(Debug, Default)]
pub struct Pictures {
    entries: Vec<(String, Vec<u8>, usize, usize)>,
}

impl Pictures {
    /// Decodes every picture in an SHPI file. Names that repeat (`st1a` on two tracks) are kept as
    /// they are; `find` returns the first.
    pub fn parse(blob: &[u8]) -> Option<Self> {
        let archive = shpi::parse(blob).ok()?;
        let mut entries = Vec::new();
        for e in &archive.entries {
            let Some(rgba) = shpi::decode_rgba8(blob, e) else { continue };
            entries.push((e.name.trim().to_string(), rgba, e.width as usize, e.height as usize));
        }
        (!entries.is_empty()).then_some(Self { entries })
    }

    /// The picture with this name, with its pixels, width and height.
    pub fn find(&self, name: &str) -> Option<(&[u8], usize, usize)> {
        self.entries
            .iter()
            .find(|(n, _, _, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, px, w, h)| (px.as_slice(), *w, *h))
    }

    /// The average colour of a picture, skipping fully transparent pixels so a mostly empty
    /// picture does not drag the average towards black.
    pub fn average(&self, name: &str) -> Option<[f32; 3]> {
        let (px, w, h) = self.find(name)?;
        if w * h == 0 {
            return None;
        }
        let mut sum = [0f32; 3];
        let mut n = 0f32;
        for i in 0..w * h {
            let c = &px[i * 4..i * 4 + 4];
            if c[3] < 8 {
                continue;
            }
            for k in 0..3 {
                sum[k] += c[k] as f32 / 255.0;
            }
            n += 1.0;
        }
        (n > 0.0).then(|| [sum[0] / n, sum[1] / n, sum[2] / n])
    }

    /// The average colour of the visible pixels and the average alpha of all pixels.
    pub fn average_with_alpha(&self, name: &str) -> Option<([f32; 3], f32)> {
        let rgb = self.average(name)?;
        let (px, w, h) = self.find(name)?;
        let a = (0..w * h).map(|i| px[i * 4 + 3] as f32 / 255.0).sum::<f32>() / (w * h) as f32;
        Some((rgb, a))
    }

    /// How many pictures there are, for the log line.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The picture's bands of content: runs of rows that carry a mountain range.
    ///
    /// `horz` is not one mountain range but two, one above the other with a gap of transparent
    /// sky between them (on alps, rows 1..126 and 139..254). This is an inspection helper;
    /// rendering uses the verified atlas header rather than splitting content runs.
    ///
    /// A range starts at the first row with **any** opaque pixel, not the first fully covered one.
    /// The topmost rows of a range are its peaks, which are opaque only in a few columns; requiring
    /// solid coverage to start a range cuts the peaks off and leaves a flat-topped ridge.
    /// Returns `None` if the picture has no name or is entirely transparent.
    pub fn content_runs(&self, name: &str) -> Option<Vec<(usize, usize)>> {
        let (px, w, h) = self.find(name)?;
        if w == 0 || h == 0 {
            return None;
        }
        let covers = |y: usize| (0..w).any(|x| px[(y * w + x) * 4 + 3] > 8);
        // Walk the rows, opening a run on the first content row and closing it after two
        // non-content rows in a row, so a single hole in the artwork does not split a range.
        let mut runs: Vec<(usize, usize)> = Vec::new();
        let mut start: Option<usize> = None;
        let mut gap = 0usize;
        for y in 0..=h {
            let on = y < h && covers(y);
            match (on, start) {
                (true, None) => start = Some(y),
                (true, Some(_)) => gap = 0,
                (false, Some(s)) => {
                    gap += 1;
                    if gap >= 2 || y == h {
                        runs.push((s, y - gap));
                        start = None;
                        gap = 0;
                    }
                }
                (false, None) => gap = 0,
            }
        }
        (!runs.is_empty()).then_some(runs)
    }

    /// Every picture's name and size, for the `sky` console command.
    pub fn describe(&self) -> Vec<String> {
        self.entries.iter().map(|(n, _, w, h)| format!("{n} {w}x{h}")).collect()
    }
}

/// One weather keyframe from `<track>.bin`, read by the old pattern scan (superseded by
/// `nfspu_formats::weather::SkyWeather`).
#[derive(Debug, Clone, Copy)]
pub struct HorizonColour {
    /// Unconfirmed position field, currently guessed to be a direction in turns.
    pub angle: f32,
    /// Packed colour as `0xFFBBGGRR`, which is how the file stores it.
    pub colour: u32,
}

impl HorizonColour {
    /// The colour as 0..1 RGB.
    pub fn rgb(&self) -> [f32; 3] {
        [
            (self.colour & 0xFF) as f32 / 255.0,
            ((self.colour >> 8) & 0xFF) as f32 / 255.0,
            ((self.colour >> 16) & 0xFF) as f32 / 255.0,
        ]
    }
}

/// Verified atlas/geometry header. Colour interpretation is deliberately separate.
#[derive(Debug, Clone)]
pub struct HorizonLayout {
    pub zero_turn: f32,
    /// Used by retail colour lookup, not by the atlas order or geometry.
    pub colour_start: u32,
    pub columns: u32,
    pub rows: u32,
    pub bottom: f32,
    pub height: f32,
    pub cells: [u8; 16],
    pub flipped: [bool; 16],
}

impl HorizonLayout {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let header = bytes.get(..60)?;
        let word = |offset| u32::from_le_bytes(header[offset..offset + 4].try_into().unwrap());
        if !matches!(word(0), 7 | 9) { return None; }
        let zero_turn = f32::from_bits(word(4));
        let colour_start = word(8);
        let columns = word(12);
        let rows = word(16);
        let bottom = f32::from_bits(word(20));
        let height = f32::from_bits(word(24));
        let count = columns.checked_mul(rows)?;
        if columns == 0 || rows == 0 || count > 256 || colour_start >= 16
            || !zero_turn.is_finite() || !bottom.is_finite() || !height.is_finite()
            || height <= 0.0 || !(bottom + height).is_finite() { return None; }
        let mut cells = [0; 16];
        let mut flipped = [false; 16];
        for i in 0..16 {
            cells[i] = header[28 + i * 2];
            let flag = header[29 + i * 2];
            if u32::from(cells[i]) >= count || flag > 1 { return None; }
            flipped[i] = flag != 0;
        }
        Some(Self { zero_turn, colour_start, columns, rows, bottom, height, cells, flipped })
    }

    /// Verified retail vertex assignment from `hzcl`: first pixel row at the top, second
    /// at the bottom. The start index rotates colours only; cell order stays independent.
    pub fn vertex_colours(&self, pictures: &Pictures) -> Option<Vec<[u8; 4]>> {
        let (rgba, width, height) = pictures.find("hzcl")?;
        if (width, height) != (16, 2) || rgba.len() != 128 { return None; }
        let pixel = |row: usize, column: usize| {
            let offset = (row * 16 + column) * 4;
            <[u8; 4]>::try_from(&rgba[offset..offset + 4]).unwrap()
        };
        let mut colours = Vec::with_capacity(64);
        for segment in 0..16 {
            let current = (self.colour_start as usize + segment) % 16;
            let next = (current + 1) % 16;
            colours.extend([pixel(0, current), pixel(0, next), pixel(1, next), pixel(1, current)]);
        }
        Some(colours)
    }

    /// Clockwise ring in original coordinates, reflected on X like the track import.
    /// Scale preserves retail's height-to-radius ratio (reference radius is 400).
    pub fn quads(&self, radius: f32, mirror_x: f32) -> Vec<([[f32; 3]; 4], [[f32; 2]; 4])> {
        let point = |edge: usize, y: f32| {
            let angle = (self.zero_turn + (edge % 16) as f32 / 16.0) * std::f32::consts::TAU;
            [angle.sin() * radius * mirror_x, y * radius / 400.0, angle.cos() * radius]
        };
        (0..16).map(|i| {
            let cell = u32::from(self.cells[i]);
            let col = (cell % self.columns) as f32;
            let row = (cell / self.columns) as f32;
            let mut left = col / self.columns as f32;
            let mut right = (col + 1.0) / self.columns as f32;
            if self.flipped[i] { std::mem::swap(&mut left, &mut right); }
            let top = (row + 0.01) / self.rows as f32;
            let bottom = (row + 0.98) / self.rows as f32;
            ([point(i, self.bottom + self.height), point(i + 1, self.bottom + self.height),
              point(i + 1, self.bottom), point(i, self.bottom)],
             [[left, top], [right, top], [right, bottom], [left, bottom]])
        }).collect()
    }
}

/// Everything one track's sky needs.
#[derive(Debug, Default)]
pub struct TrackSky {
    pub pictures: Pictures,
    pub layout: Option<HorizonLayout>,
    /// The sky level type from the file, e.g. `MULTINORMAL`.
    pub level: String,
    /// Weather region A, then B.
    pub regions: Vec<Region>,
    /// The colour table, in file order.
    pub horizon: Vec<HorizonColour>,
    /// The horizon's zero point, as a fraction of a turn; see `parse_start_angle`.
    pub start_angle: f32,
    /// Which capability block the file's first `BEGIN` block was (HIGH or MEDIUM).
    pub capability: String,
}

/// Reads `<track>.txt`, `.fsh` and `.bin` for one track. `game_dir` is the install folder, the one
/// `NFSPU_GAME_DIR` points at. Returns `None` if the track has no sky folder at all.
pub fn load(game_dir: &std::path::Path, track: &str) -> Option<TrackSky> {
    let dir = game_dir.join("GameData").join("Track").join("Sky");
    if !dir.is_dir() {
        return None;
    }
    let pictures = std::fs::read(dir.join(format!("{track}.fsh")))
        .ok()
        .and_then(|b| Pictures::parse(&b))
        .unwrap_or_default();
    let txt = std::fs::read_to_string(dir.join(format!("{track}.txt"))).ok();
    let bin = std::fs::read(dir.join(format!("{track}.bin"))).ok();
    let layout = bin.as_deref().and_then(HorizonLayout::parse);
    let horizon = bin.as_deref().map(parse_horizon).unwrap_or_default();
    let start_angle = bin.as_deref().map(parse_start_angle).unwrap_or(0.0);

    let (capability, level, regions) = match &txt {
        Some(t) => parse_txt(t),
        None => (String::new(), String::new(), Vec::new()),
    };
    Some(TrackSky { pictures, layout, level, regions, horizon, start_angle, capability })
}

/// The horizon's zero point: the second dword of `<track>.bin` as a float. The engine subtracts this
/// from the camera's angle before working out which sixteenth of the horizon is in view
/// (`SkyHorizon::vfn01_0045a1a0`), so it lines the sky up with the track's start line. On alps it
/// is 0.8621.
pub fn parse_start_angle(bin: &[u8]) -> f32 {
    bin.get(4..8).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).unwrap_or(0.0)
}

/// Parses the first `BEGIN` block of a sky `.txt`. The file has one block per graphics capability
/// (HIGH first, then MEDIUM); we take the first, which is the one the game uses on a modern card.
fn parse_txt(text: &str) -> (String, String, Vec<Region>) {
    let mut capability = String::new();
    let mut level = String::new();
    let mut regions: Vec<Region> = Vec::new();
    // Only look inside the first BEGIN..END block.
    let body = text
        .split_once("BEGIN")
        .map(|(_, rest)| rest)
        .and_then(|rest| rest.split_once("END").map(|(b, _)| b))
        .unwrap_or(text);

    // The file is a comment hash or a value, one per line, with a comment naming each group of
    // values underneath it. So: read the comment to learn what the next values mean, then take
    // them. Groups of three (the sky map, the scales, the wind) end after three values; the rest
    // run until the next comment. Groups we do not recognise are skipped.
    let mut key: Option<&str> = None;
    let mut values: Vec<String> = Vec::new();
    for line in body.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(c) = t.strip_prefix('#') {
            // A comment ends the group above it, whatever it had collected so far.
            apply(key, &values, &mut capability, &mut level, &mut regions);
            values.clear();
            let c = c.trim().to_ascii_lowercase();
            key = if c.contains("computer capability") {
                Some("capability")
            } else if c.contains("sky level type") {
                Some("level")
            } else if c.contains("sky map") {
                Some("sky")
            } else if c.contains("x scale") {
                Some("scale_x")
            } else if c.contains("y scale") {
                Some("scale_y")
            } else if c.contains("move ratio") {
                Some("move")
            } else if c.contains("wind speed") {
                Some("wind_speed")
            } else if c.contains("wind direction") {
                Some("wind_dir")
            } else if c.contains("cloud textures") {
                Some("clouds")
            } else {
                None
            };
            continue;
        }
        values.push(t.to_string());
        // Three-value groups are complete; single-value groups after one.
        let done = match key {
            Some("capability") | Some("level") => values.len() >= 1,
            Some("sky") | Some("clouds") => values.len() >= 3,
            Some(_) => values.len() >= 3,
            None => false,
        };
        if done {
            apply(key, &values, &mut capability, &mut level, &mut regions);
            values.clear();
            key = None;
        }
    }
    apply(key, &values, &mut capability, &mut level, &mut regions);
    (capability, level, regions)
}

/// Files one completed group of values into the right place. Unknown groups are ignored, which is
/// how the warp settings and the "does this layer block the sun" flag are skipped.
fn apply(
    key: Option<&str>,
    values: &[String],
    capability: &mut String,
    level: &mut String,
    regions: &mut Vec<Region>,
) {
    if values.is_empty() {
        return;
    }
    let nums = |n: usize| -> Vec<f32> { values.iter().take(n).filter_map(|s| s.parse().ok()).collect() };
    match key {
        Some("capability") => *capability = values[0].clone(),
        Some("level") => *level = values[0].clone(),
        // Each "sky map" group starts a weather region; the third value is the first cloud entry.
        Some("sky") => regions.push(Region {
            sky_map: values[0].clone(),
            sky_colour: values.get(1).cloned().unwrap_or_default(),
            third: values.get(2).cloned().unwrap_or_default(),
            ..Default::default()
        }),
        // The cloud textures are listed once for the whole block, not per region.
        Some("clouds") => {
            let clouds: Vec<String> = values.iter().take(3).cloned().collect();
            for r in regions.iter_mut() {
                r.clouds = clouds.clone();
            }
        }
        Some(k @ ("scale_x" | "scale_y" | "move" | "wind_speed" | "wind_dir")) => {
            let v = nums(3);
            if let Some(r) = regions.last_mut() {
                match k {
                    "scale_x" => r.scale[0] = v,
                    "scale_y" => r.scale[1] = v,
                    "move" => r.move_ratio = v,
                    "wind_speed" => r.wind_speed = v,
                    _ => r.wind_direction = v,
                }
            }
        }
        _ => {}
    }
}

/// Reads the colour table out of a sky `.bin`.
///
/// Records are 12 dwords: a distance or index, then two floats of which the first is the direction
/// the colour belongs to (a fraction of a full turn), then the colour twice. We look for the first
/// dword where that pattern holds and read forwards while it does, which skips the header and the
/// scrolling parameters without needing their layout.
pub fn parse_horizon(bin: &[u8]) -> Vec<HorizonColour> {
    let dwords: Vec<u32> = bin
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    let dw = dwords.as_slice();
    let is_colour = |v: u32| (v >> 24) == 0xFF && v != 0xFFFF_FFFF && v & 0xFF00_0000 != 0xCB00_0000;

    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 12 <= dw.len() {
        if is_colour(dw[i + 4]) && is_colour(dw[i + 5]) {
            out.push(HorizonColour { angle: f32::from_bits(dw[i + 1]), colour: dw[i + 4] });
            i += 12;
        } else {
            i += 1;
        }
        // A colour table is never thousands of records long; stop if we have clearly run off it.
        if out.len() > 512 {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `.bin`-shaped blob with a header, then three colour records, then filler.
    fn fake_bin() -> Vec<u8> {
        let mut v: Vec<u32> = vec![9, 0x3f5c_b08d, 0, 4, 2, 0xc2ca_9d48, 0x43ad_5b31];
        v.extend_from_slice(&[0; 40]);
        for (d, a, b, c) in
            [(0u32, 0.14f32, 0.10f32, 0xff8b_6757u32), (436, 0.5, 0.6, 0xff8b_6757), (u32::MAX, 0.9, 0.0, 0xff7b_615b)]
        {
            v.push(d);
            v.push(a.to_bits());
            v.push(b.to_bits());
            v.push(0);
            v.push(c);
            v.push(c);
            v.extend_from_slice(&[0; 6]);
        }
        v.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    #[test]
    fn horizon_colours_are_read_in_order() {
        let got = parse_horizon(&fake_bin());
        assert_eq!(got.len(), 3, "three colour records, header skipped: {got:?}");
        assert!((got[0].angle - 0.14).abs() < 0.001, "angle was {}", got[0].angle);
        assert!((got[1].angle - 0.5).abs() < 0.001, "angle was {}", got[1].angle);
    }

    #[test]
    fn the_start_angle_is_the_second_dword() {
        // alps' second dword is 0x3f5cb08d = 0.8621, which the engine subtracts from the camera angle.
        assert!((parse_start_angle(&fake_bin()) - 0.8621).abs() < 0.0001);
    }

    #[test]
    fn packed_colours_come_back_as_rgb() {
        let got = parse_horizon(&fake_bin());
        // 0xff8b6757 is stored as 0xFFBBGGRR, so R = 0x57, G = 0x67, B = 0x8b.
        let [r, g, b] = got[0].rgb();
        assert!((r - 87.0 / 255.0).abs() < 0.01, "red was {r}");
        assert!((g - 103.0 / 255.0).abs() < 0.01, "green was {g}");
        assert!((b - 139.0 / 255.0).abs() < 0.01, "blue was {b}");
    }

    #[test]
    fn a_file_with_no_colour_table_gives_nothing() {
        assert!(parse_horizon(&[0u8; 64]).is_empty());
    }

    const ALPS_TXT: &str = "BEGIN\n\
        #Computer capability\n\
        \tHIGH\n\
        #sky level type\n\
        \tMULTINORMAL\n\
        #Does this layer block the sun\n\
        \tNO\n\
        #\n\
        #  WEATHER REGION A\n\
        #\n\
        # sky map RGB (or ARGB) cannot choose \"noise\n\
        \tst1a\n\
        \tsc1a\n\
        \tnone\n\
        #X Scale Factors\n\
        \t1\n\
        \t2\n\
        \t0\n\
        #Y Scale Factors \n\
        \t1\n\
        \t2\n\
        \t0\n\
        #move ratio set to 0 for no movement at all\n\
        \t400\n\
        #wind speed 0 for none, 10 for lots\n\
        \t0\n\
        \t2\n\
        \t0\n\
        #wind direction 0-360\n\
        \t90\n\
        \t40\n\
        \t0\n\
        #warp speed\n\
        \t0\n\
        #warp amount\n\
        \t0\n\
        #  WEATHER REGION B\n\
        # sky map RGB (or ARGB) cannot choose \"noise\n\
        \tst1b\n\
        \tsc1b\n\
        \tnone\n\
        #X Scale Factors\n\
        \t1\n\
        \t2\n\
        \t0\n\
        #Y Scale Factors \n\
        \t1\n\
        \t2\n\
        \t0\n\
        #move ratio\n\
        \t400\n\
        #wind speed\n\
        \t0\n\
        \t2\n\
        \t0\n\
        #wind direction\n\
        \t90\n\
        \t40\n\
        \t0\n\
        #warp speed\n\
        \t0\n\
        #warp amount\n\
        \t0\n\
        ############# note that the number of textures determines how many layers you have\n\
        # CLOUD TEXTURES (you must always have three entries, enter NONE, or none for no texture)\n\
        \tflat\n\
        \tcc1a\n\
        \tnone\n\
        END\n";

    #[test]
    fn both_weather_regions_are_read() {
        let (cap, level, regions) = parse_txt(ALPS_TXT);
        assert_eq!(cap, "HIGH");
        assert_eq!(level, "MULTINORMAL");
        assert_eq!(regions.len(), 2, "region A and region B: {regions:?}");
        assert_eq!(regions[0].sky_map, "st1a");
        assert_eq!(regions[0].sky_colour, "sc1a");
        assert_eq!(regions[1].sky_map, "st1b");
        assert_eq!(regions[1].sky_colour, "sc1b");
    }

    #[test]
    fn cloud_layers_come_from_the_last_group() {
        let (_, _, regions) = parse_txt(ALPS_TXT);
        assert_eq!(regions[1].clouds, vec!["flat", "cc1a", "none"]);
    }

    #[test]
    fn an_empty_file_gives_no_regions() {
        let (cap, level, regions) = parse_txt("");
        assert!(cap.is_empty() && level.is_empty() && regions.is_empty());
    }

    #[test]
    fn a_missing_track_folder_gives_none() {
        assert!(load(std::path::Path::new("Z:/no/such/place"), "nosuchtrack").is_none());
    }

    /// Two records: a cold one facing 0.0 and a warm one half a turn away. The colour is packed as
    /// 0xFFBBGGRR, so the cold one has a high blue byte and the warm one a high red byte.
    fn two_colours() -> Vec<HorizonColour> {
        vec![
            HorizonColour { angle: 0.0, colour: 0xffc0_4020 },
            HorizonColour { angle: 0.5, colour: 0xff20_40c0 },
        ]
    }

    #[test]
    fn the_tint_facing_one_way_is_the_cold_colour() {
        let [r, g, b] = horizon_tint(&two_colours(), 0.0);
        assert!(b > r + 0.2, "expected blue that way, got {r},{g},{b}");
    }

    #[test]
    fn the_tint_facing_the_other_way_is_the_warm_colour() {
        let [r, g, b] = horizon_tint(&two_colours(), 0.5);
        assert!(r > b + 0.2, "expected warm that way, got {r},{g},{b}");
    }

    #[test]
    fn a_direction_wraps_around_the_horizon() {
        // 1.25 of a turn is the same direction as 0.25, and so is -0.75.
        let a = horizon_tint(&two_colours(), 1.25);
        let b = horizon_tint(&two_colours(), -0.75);
        assert!((a[0] - b[0]).abs() < 0.001 && (a[2] - b[2]).abs() < 0.001);
    }

    #[test]
    fn a_direction_far_from_everything_blends_to_the_average() {
        let t = horizon_tint(&two_colours(), 0.25);
        let (r, b) = (t[0], t[2]);
        assert!((r - b).abs() < 0.25, "expected a blend, got red {r} blue {b}");
    }

    #[test]
    fn an_empty_table_gives_a_neutral_sky() {
        assert_eq!(horizon_tint(&[], 0.25), [0.6, 0.7, 0.85]);
    }

    /// Builds a `Pictures` from raw RGBA, so `content_runs` can be tested without the game files.
    /// Every pixel's alpha is set to `alpha`.
    fn picture(w: usize, h: usize, alpha: u8) -> Pictures {
        let mut px = vec![0u8; w * h * 4];
        for p in px.chunks_exact_mut(4) {
            p[3] = alpha;
        }
        Pictures { entries: vec![("test".to_string(), px, w, h)] }
    }

    #[test]
    fn one_solid_band_is_a_single_run() {
        let runs = picture(4, 4, 255).content_runs("test").expect("some content");
        assert_eq!(runs, vec![(0, 3)], "all four rows are one run");
    }

    #[test]
    fn two_ranges_with_a_gap_come_back_as_two_runs() {
        // rows 0-3 opaque, 4-5 clear, 6-9 opaque: this is the shape of alps' `horz`.
        let mut p = picture(4, 10, 0);
        let px = &mut p.entries[0].1;
        for y in [0, 1, 2, 3, 6, 7, 8, 9] {
            for x in 0..4 {
                px[(y * 4 + x) * 4 + 3] = 255;
            }
        }
        let runs = p.content_runs("test").expect("some content");
        assert_eq!(runs, vec![(0, 3), (6, 9)], "the gap splits it in two");
    }

    #[test]
    fn a_single_hole_does_not_split_a_range() {
        let mut p = picture(4, 6, 0);
        let px = &mut p.entries[0].1;
        for y in [0, 1, 2, 4, 5] {
            for x in 0..4 {
                px[(y * 4 + x) * 4 + 3] = 255;
            }
        }
        let runs = p.content_runs("test").expect("some content");
        assert_eq!(runs, vec![(0, 5)], "one clear row is a hole, not a gap");
    }

    #[test]
    fn a_range_starts_at_its_peaks_even_where_they_are_sparse() {
        // Row 0 has a single opaque pixel out of twenty: a mountain peak. Row 1 onwards is solid.
        // The peak must be kept, because trimming to solid coverage leaves a flat-topped ridge.
        let (w, h) = (20usize, 5usize);
        let mut p = picture(w, h, 0);
        let px = &mut p.entries[0].1;
        for y in 1..h {
            for x in 0..w {
                px[(y * w + x) * 4 + 3] = 255;
            }
        }
        px[3] = 255;
        let runs = p.content_runs("test").expect("some content");
        assert_eq!(runs, vec![(0, 4)], "the peak row is part of the range");
    }

    #[test]
    fn a_fully_transparent_picture_has_no_runs() {
        assert!(picture(4, 4, 0).content_runs("test").is_none());
    }

    #[test]
    fn a_missing_picture_has_no_runs() {
        assert!(Pictures::default().content_runs("nope").is_none());
    }
}
#[cfg(test)]
mod atlas_tests {
    use super::*;
    fn header() -> Vec<u8> {
        let mut b = vec![0; 60];
        for (offset, value) in [(0, 9u32), (4, 0.25f32.to_bits()), (8, 5), (12, 4), (16, 2),
                                (20, (-100f32).to_bits()), (24, 300f32.to_bits())] {
            b[offset..offset+4].copy_from_slice(&value.to_le_bytes());
        }
        for i in 0..16 { b[28+i*2] = (i % 8) as u8; }
        b[31] = 1;
        b
    }
    #[test]
    fn atlas_rows_flips_and_seams() {
        let layout = HorizonLayout::parse(&header()).unwrap();
        let q = layout.quads(400.0, -1.0);
        assert_eq!(q.len(), 16);
        assert_eq!(q[0].1, [[0.0,0.005],[0.25,0.005],[0.25,0.49],[0.0,0.49]]);
        assert_eq!(q[1].1[0][0], 0.5);
        assert_eq!(q[1].1[1][0], 0.25);
        assert_eq!(q[4].1[0][1], 0.505);
        assert_eq!(q[4].1[2][1], 0.99);
        assert!((q[0].0[0][0] + 400.0).abs() < 0.001);
        assert_eq!(q[0].0[0][1], 200.0);
        assert_eq!(q[0].0[3][1], -100.0);
        for i in 0..16 {
            assert_eq!(q[i].0[1], q[(i+1)%16].0[0]);
            assert_eq!(q[i].0[2], q[(i+1)%16].0[3]);
        }
        let mut other = layout.clone(); other.colour_start = 0;
        assert_eq!(q, other.quads(400.0, -1.0));
    }
    #[test]
    fn horizon_palette_rows_start_index_and_wrap_are_independent_of_cells() {
        let layout = HorizonLayout::parse(&header()).unwrap();
        let rgba: Vec<u8> = (0..32u8).flat_map(|i| [i, 100, 200, 255]).collect();
        let pictures = Pictures { entries: vec![("hzcl".into(), rgba, 16, 2)] };
        let colours = layout.vertex_colours(&pictures).unwrap();
        assert_eq!(colours.len(), 64);
        assert_eq!(colours[..4].iter().map(|c| c[0]).collect::<Vec<_>>(), [5,6,22,21]);
        assert_eq!(colours[40..44].iter().map(|c| c[0]).collect::<Vec<_>>(), [15,0,16,31]);
        assert_eq!(colours[61], colours[0]);
        let mut changed = layout.clone(); changed.cells = [7; 16]; changed.flipped = [true; 16];
        assert_eq!(colours, changed.vertex_colours(&pictures).unwrap());
        assert!(layout.vertex_colours(&Pictures::default()).is_none());
        let invalid = Pictures { entries: vec![("hzcl".into(), vec![0;128], 32, 1)] };
        assert!(layout.vertex_colours(&invalid).is_none());
    }
    #[test]
    fn invalid_atlas_headers_are_rejected() {
        let b = header();
        for size in 0..60 { assert!(HorizonLayout::parse(&b[..size]).is_none()); }
        for (offset, value) in [(0, 944u32),(8,16),(12,0),(16,u32::MAX),
                                (4,f32::NAN.to_bits()),(24,0)] {
            let mut bad=b.clone(); bad[offset..offset+4].copy_from_slice(&value.to_le_bytes());
            assert!(HorizonLayout::parse(&bad).is_none());
        }
        for (offset,value) in [(28,8),(29,2)] {
            let mut bad=b.clone(); bad[offset]=value;
            assert!(HorizonLayout::parse(&bad).is_none());
        }
    }
}
