//! Track conditions: the weather keyframes in `Track/Sky/<track>.bin`, the car-lighting header of
//! `Track/<track>.map`, the night light map in `Render/night.psh`, and whether a track `.crp` asks
//! for night lighting. See `docs/provenance/weather.md` for the evidence behind each field.
//!
//! The sky `.bin` (version 9, 0x71DC bytes) holds, after the horizon header that `sky.rs` reads:
//! * a table of (slice, value) pairs at 0xDDC: how far the sky has moved from weather region A to B;
//! * a table of 48-byte keyframes at 0x1A5C: fog for the sky and for the world, rain, lightning and
//!   snow, each pinned to a road slice and blended between neighbouring keyframes;
//! * a table of (slice, flag) pairs at 0x655C.
//!
//! Each table holds up to 400 entries and ends at the first entry whose slice is negative.

/// Size of a version-9 sky `.bin`.
pub const SKY_BIN_SIZE: usize = 0x71DC;
const BLEND_TABLE: usize = 0xDDC;
const KEY_TABLE: usize = 0x1A5C;
const FLAG_TABLE: usize = 0x655C;
const MAX_ENTRIES: usize = 400;
const KEY_SIZE: usize = 0x30;
/// Below this (after cubing) a fog density counts as "no fog".
pub const FOG_EPSILON: f32 = 3e-6;

/// One weather keyframe, as stored.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WeatherKey {
    /// Road slice the keyframe sits on.
    pub slice: i32,
    /// Fog used while the sky is drawn, stored as the cube root of the density.
    pub sky_fog: f32,
    /// Fog used for the track, cars and scenery, stored as the cube root of the density.
    pub world_fog: f32,
    /// Fog colours as `0xAARRGGBB`.
    pub sky_fog_colour: u32,
    pub world_fog_colour: u32,
    /// How hard it rains, 0..1.
    pub rain: f32,
    /// How stormy it is, 0..1: picks how often lightning flashes.
    pub lightning: f32,
    /// How hard it snows, 0..1.
    pub snow: f32,
}

/// The conditions at one point of the lap, blended from the keyframes either side of it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Conditions {
    /// Exponential fog densities (per metre), already cubed.
    pub sky_fog_density: f32,
    pub world_fog_density: f32,
    /// Fog colours as 0..1 RGB.
    pub sky_fog_colour: [f32; 3],
    pub world_fog_colour: [f32; 3],
    pub rain: f32,
    pub lightning: f32,
    pub snow: f32,
}

/// The weather tables of one sky `.bin`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SkyWeather {
    pub version: u32,
    /// Weather region blend per slice (0 = region A, 1 = region B).
    pub region_blend: Vec<(i32, f32)>,
    pub keys: Vec<WeatherKey>,
    pub flags: Vec<(i32, bool)>,
    /// Header floats at 0x3C..0x48 and 0xB3C..0xB48 (the first is an angle in turns); kept raw.
    pub header_a: [f32; 4],
    pub header_b: [f32; 4],
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn f32_at(b: &[u8], o: usize) -> f32 {
    f32::from_bits(u32_at(b, o))
}

/// 0xAARRGGBB to 0..1 RGB.
pub fn argb_rgb(c: u32) -> [f32; 3] {
    [((c >> 16) & 0xFF) as f32 / 255.0, ((c >> 8) & 0xFF) as f32 / 255.0, (c & 0xFF) as f32 / 255.0]
}

impl SkyWeather {
    /// Reads a version-9 sky `.bin`. Older layouts (`default.bin`, `Monaco.bin`, version 7) and the
    /// unused `*_lightning.bin` files give `None`; retail fills those with built-in defaults.
    pub fn parse(b: &[u8]) -> Option<Self> {
        if b.len() < SKY_BIN_SIZE || u32_at(b, 0) != 9 {
            return None;
        }
        let mut region_blend = Vec::new();
        for i in 0..MAX_ENTRIES {
            let o = BLEND_TABLE + i * 8;
            let slice = u32_at(b, o) as i32;
            if slice < 0 {
                break;
            }
            region_blend.push((slice, f32_at(b, o + 4)));
        }
        let mut keys = Vec::new();
        for i in 0..MAX_ENTRIES {
            let o = KEY_TABLE + i * KEY_SIZE;
            let slice = u32_at(b, o) as i32;
            if slice < 0 {
                break;
            }
            keys.push(WeatherKey {
                slice,
                sky_fog: f32_at(b, o + 0x04),
                world_fog: f32_at(b, o + 0x08),
                sky_fog_colour: u32_at(b, o + 0x10),
                world_fog_colour: u32_at(b, o + 0x14),
                rain: f32_at(b, o + 0x18),
                lightning: f32_at(b, o + 0x1C),
                snow: f32_at(b, o + 0x20),
            });
        }
        let mut flags = Vec::new();
        for i in 0..MAX_ENTRIES {
            let o = FLAG_TABLE + i * 8;
            let slice = u32_at(b, o) as i32;
            if slice < 0 {
                break;
            }
            flags.push((slice, b[o + 4] != 0));
        }
        // Retail replaces a table with fewer than two entries by a flat default.
        if keys.len() < 2 {
            keys = vec![WeatherKey::default(), WeatherKey { slice: 4, ..Default::default() }];
        }
        let header = |o: usize| [f32_at(b, o), f32_at(b, o + 4), f32_at(b, o + 8), f32_at(b, o + 12)];
        Some(Self { version: 9, region_blend, keys, flags, header_a: header(0x3C), header_b: header(0xB3C) })
    }

    /// The conditions at road slice `slice` of a track with `slice_count` slices.
    pub fn sample(&self, slice: i32, slice_count: i32) -> Conditions {
        let (i, j, w) = bracket(self.keys.iter().map(|k| k.slice), self.keys.len(), slice, slice_count);
        let (a, b) = (&self.keys[i], &self.keys[j]);
        let mix = |x: f32, y: f32| w * x + (1.0 - w) * y;
        let mix_colour = |x: u32, y: u32| {
            let (x, y) = (argb_rgb(x), argb_rgb(y));
            [mix(x[0], y[0]), mix(x[1], y[1]), mix(x[2], y[2])]
        };
        let sky = mix(a.sky_fog, b.sky_fog);
        let world = mix(a.world_fog, b.world_fog);
        // Colours only blend towards a neighbour that has fog of its own; otherwise the current
        // keyframe's colour holds.
        let sky_colour = if b.sky_fog <= FOG_EPSILON { argb_rgb(a.sky_fog_colour) } else { mix_colour(a.sky_fog_colour, b.sky_fog_colour) };
        let world_colour = if b.world_fog > FOG_EPSILON { mix_colour(a.world_fog_colour, b.world_fog_colour) } else { argb_rgb(a.world_fog_colour) };
        Conditions {
            sky_fog_density: sky * sky * sky,
            world_fog_density: world * world * world,
            sky_fog_colour: sky_colour,
            world_fog_colour: world_colour,
            rain: mix(a.rain, b.rain),
            lightning: mix(a.lightning, b.lightning),
            snow: mix(a.snow, b.snow),
        }
    }

    /// Region A/B blend at a slice (0 when the table is empty).
    pub fn region_blend_at(&self, slice: i32, slice_count: i32) -> f32 {
        if self.region_blend.len() < 2 {
            return self.region_blend.first().map_or(0.0, |e| e.1);
        }
        let (i, j, w) = bracket(self.region_blend.iter().map(|e| e.0), self.region_blend.len(), slice, slice_count);
        w * self.region_blend[i].1 + (1.0 - w) * self.region_blend[j].1
    }

    /// The largest rain, lightning and snow values anywhere on the lap.
    pub fn peaks(&self) -> (f32, f32, f32) {
        self.keys.iter().fold((0.0f32, 0.0f32, 0.0f32), |(r, l, s), k| (r.max(k.rain), l.max(k.lightning), s.max(k.snow)))
    }
}

/// Finds the keyframes either side of `slice` and the weight of the first, the way retail does:
/// keyframes are in slice order, the last one blends into the first round the lap, and distances
/// wrap at `count` slices.
fn bracket(slices: impl Iterator<Item = i32> + Clone, n: usize, slice: i32, count: i32) -> (usize, usize, f32) {
    let s: Vec<i32> = slices.collect();
    if n < 2 {
        return (0, 0, 1.0);
    }
    let mut i = n - 1;
    for k in 0..n - 1 {
        if s[k] <= slice && slice < s[k + 1] {
            i = k;
            break;
        }
    }
    let j = if i + 1 >= n { 0 } else { i + 1 };
    if s[i] == s[j] {
        return (i, j, 1.0);
    }
    let wrap = |v: i32| if count > 0 { v.rem_euclid(count) } else { v };
    let span = wrap(s[j] - s[i]);
    let along = wrap(slice - s[i]);
    if span == 0 {
        return (i, j, 1.0);
    }
    (i, j, (1.0 - along as f32 / span as f32).clamp(0.0, 1.0))
}

/// A colour multiplier from the `.map` header, as stored: alpha then red, green, blue.
pub type Argb = [f32; 4];

/// One `[sectionN]` of a `.map`: the car's reflection textures over a range of slices, in the open
/// and under cover.
#[derive(Debug, Clone, PartialEq)]
pub struct ReflectionSection {
    pub start_slice: i32,
    pub end_slice: i32,
    /// Texture names in the track's `.env`, (open, covered) for layers 1..3.
    pub textures: [(String, String); 3],
    pub texture_size: f32,
    pub aspect: f32,
}

/// The `.map` file: per-track multipliers for how the cars are lit, and reflection sections.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackLighting {
    pub tint: Argb,
    pub ambient: Argb,
    pub interior: Argb,
    pub interior_diffuse: Argb,
    pub light_decal: Argb,
    pub sections: Vec<ReflectionSection>,
}

impl Default for TrackLighting {
    /// Retail's defaults when a key is missing: ambient 0.3, everything else 1.
    fn default() -> Self {
        Self {
            tint: [1.0; 4],
            ambient: [1.0, 0.3, 0.3, 0.3],
            interior: [1.0; 4],
            interior_diffuse: [1.0; 4],
            light_decal: [1.0; 4],
            sections: Vec::new(),
        }
    }
}

/// Four characters stored as a little-endian number, e.g. 0x30564E45 = "ENV0".
fn fourcc(v: u32) -> String {
    v.to_le_bytes().iter().map(|&b| if (32..127).contains(&b) { b as char } else { '?' }).collect::<String>().trim_end().to_string()
}

impl TrackLighting {
    pub fn parse(text: &str) -> Self {
        let mut out = TrackLighting::default();
        let mut section: Option<std::collections::HashMap<String, String>> = None;
        let mut header = std::collections::HashMap::new();
        let mut in_header = false;
        let finish = |s: std::collections::HashMap<String, String>, out: &mut TrackLighting| {
            let int = |k: &str, d: i32| s.get(k).and_then(|v| v.parse().ok()).unwrap_or(d);
            let num = |k: &str, d: f32| s.get(k).and_then(|v| v.parse().ok()).unwrap_or(d);
            let tex = |k: &str| fourcc(s.get(k).and_then(|v| v.parse::<i64>().ok()).map_or(0x3056_4E45, |v| v as u32));
            out.sections.push(ReflectionSection {
                start_slice: int("startslice", 0),
                end_slice: int("endslice", 0),
                textures: [
                    (tex("texture1.open"), tex("texture1.covered")),
                    (tex("texture2.open"), tex("texture2.covered")),
                    (tex("texture3.open"), tex("texture3.covered")),
                ],
                texture_size: num("texturesize", 6.0),
                aspect: num("aspect", 1.0),
            });
        };
        for line in text.lines() {
            let t = line.trim();
            if t.starts_with('[') {
                if let Some(s) = section.take() {
                    finish(s, &mut out);
                }
                in_header = t.eq_ignore_ascii_case("[header]");
                if !in_header {
                    section = Some(Default::default());
                }
                continue;
            }
            let Some((k, v)) = t.split_once('=') else { continue };
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim().to_string());
            if in_header {
                header.insert(k, v);
            } else if let Some(s) = section.as_mut() {
                // The first entry of a key wins (section 0 repeats some keys).
                s.entry(k).or_insert(v);
            }
        }
        if let Some(s) = section.take() {
            finish(s, &mut out);
        }
        let argb = |k: &str, d: Argb| -> Argb {
            header.get(k).and_then(|v| {
                let n: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                (n.len() == 4).then(|| [n[0], n[1], n[2], n[3]])
            }).unwrap_or(d)
        };
        let d = TrackLighting::default();
        out.tint = argb("tint", d.tint);
        out.ambient = argb("ambient", d.ambient);
        out.interior = argb("interior", d.interior);
        out.interior_diffuse = argb("interiordiffuse", d.interior_diffuse);
        out.light_decal = argb("lightdecal", d.light_decal);
        out
    }
}

/// Layer ids that retail's layer factory creates as headlight vertex-light layers. Creating any of
/// them switches the race into night mode.
pub const HEADLIGHT_LAYERS: [u32; 3] = [0x8002, 0x8003, 0x8011];

/// True when a track container's render-method parts (tag `..MR`) use a headlight layer, which is
/// what puts retail into night mode for that track.
pub fn crp_wants_night(c: &crate::crp::Crp) -> bool {
    c.misc.iter().chain(c.articles.iter().flat_map(|a| a.parts.iter())).filter(|p| &p.tag[2..] == b"MR").any(|p| {
        c.part_data(p).chunks_exact(4).any(|w| HEADLIGHT_LAYERS.contains(&u32::from_le_bytes([w[0], w[1], w[2], w[3]])))
    })
}

/// The headlight light map: an 8-bit 128x128 picture, row = distance ahead, column = sideways.
#[derive(Debug, Clone, PartialEq)]
pub struct LightMap {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

/// Rows per metre ahead of the car and columns per metre sideways, from the exe.
pub const LIGHT_MAP_ROWS_PER_M: f32 = 1.28;
pub const LIGHT_MAP_COLS_PER_M: f32 = 3.62;
/// Brightness lost per metre above or below the light's plane, in map units (of 255).
pub const LIGHT_MAP_FALLOFF_PER_M: f32 = 32.0;

impl LightMap {
    /// Reads the first picture of an `SHPP` file (`night.psh`): format 0x40, 8 bits per pixel.
    pub fn parse_psh(b: &[u8]) -> Option<Self> {
        if b.get(0..4)? != b"SHPP" || u32_at(b, 8) < 1 {
            return None;
        }
        let off = u32_at(b, 20) as usize;
        let h = b.get(off..off + 16)?;
        if h[0] != 0x40 && h[0] != 0x41 {
            return None;
        }
        let (width, height) = (u16::from_le_bytes([h[4], h[5]]) as usize, u16::from_le_bytes([h[6], h[7]]) as usize);
        let pixels = b.get(off + 16..off + 16 + width * height)?.to_vec();
        Some(Self { width, height, pixels })
    }

    /// Light at a point `side` metres right and `ahead` metres in front of the lamps, `above`
    /// metres off their plane, as 0..1 (retail adds this times the lamp colour to the vertex colour).
    pub fn intensity(&self, side: f32, ahead: f32, above: f32) -> f32 {
        let row = (ahead * LIGHT_MAP_ROWS_PER_M).round() as i32;
        let col = (side * LIGHT_MAP_COLS_PER_M).round() as i32;
        if row <= 0 || row >= 127 || col <= -63 || col >= 63 || self.width != 128 {
            return 0.0;
        }
        let v = self.pixels[(row as usize) * 128 + (col + 64) as usize] as f32 - (above * LIGHT_MAP_FALLOFF_PER_M).abs().round();
        v.max(0.0) / 256.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bin(keys: &[(i32, f32, f32, u32, u32, f32, f32, f32)]) -> Vec<u8> {
        let mut b = vec![0u8; SKY_BIN_SIZE];
        b[0..4].copy_from_slice(&9u32.to_le_bytes());
        let put = |b: &mut Vec<u8>, o: usize, v: u32| b[o..o + 4].copy_from_slice(&v.to_le_bytes());
        for t in [BLEND_TABLE, FLAG_TABLE] {
            put(&mut b, t, 0);
            put(&mut b, t + 8, u32::MAX);
        }
        for (i, k) in keys.iter().enumerate() {
            let o = KEY_TABLE + i * KEY_SIZE;
            put(&mut b, o, k.0 as u32);
            put(&mut b, o + 4, k.1.to_bits());
            put(&mut b, o + 8, k.2.to_bits());
            put(&mut b, o + 0x10, k.3);
            put(&mut b, o + 0x14, k.4);
            put(&mut b, o + 0x18, k.5.to_bits());
            put(&mut b, o + 0x1C, k.6.to_bits());
            put(&mut b, o + 0x20, k.7.to_bits());
        }
        put(&mut b, KEY_TABLE + keys.len() * KEY_SIZE, u32::MAX);
        b
    }

    #[test]
    fn keyframes_are_read_and_densities_cubed() {
        let w = SkyWeather::parse(&bin(&[(0, 0.2, 0.1, 0xFF41556F, 0xFF3F506A, 1.0, 0.135, 0.0), (100, 0.2, 0.1, 0xFF41556F, 0xFF3F506A, 0.0, 0.0, 0.0)])).unwrap();
        assert_eq!(w.keys.len(), 2);
        assert_eq!(w.region_blend, vec![(0, 0.0)]);
        let c = w.sample(0, 400);
        assert!((c.world_fog_density - 0.001).abs() < 1e-6);
        assert!((c.sky_fog_density - 0.008).abs() < 1e-6);
        // 0xAARRGGBB: 0x41 red, 0x55 green, 0x6F blue.
        assert_eq!(c.sky_fog_colour, [0x41 as f32 / 255.0, 0x55 as f32 / 255.0, 0x6F as f32 / 255.0]);
        assert_eq!(c.rain, 1.0);
    }

    #[test]
    fn values_blend_between_keyframes_and_wrap_round_the_lap() {
        let w = SkyWeather::parse(&bin(&[(10, 0.0, 0.1, 0, 0, 0.0, 0.0, 0.0), (20, 0.0, 0.1, 0, 0, 1.0, 0.0, 0.0)])).unwrap();
        assert!((w.sample(15, 100).rain - 0.5).abs() < 1e-6);
        // From 20 round to 10 is 90 slices; slice 65 is halfway.
        assert!((w.sample(65, 100).rain - 0.5).abs() < 1e-6);
        // Before the first keyframe still wraps from the last one: 5 is 85 of 90 slices along.
        assert!((w.sample(5, 100).rain - (5.0 / 90.0)).abs() < 1e-5);
    }

    #[test]
    fn a_colour_holds_when_the_next_keyframe_has_no_fog() {
        let w = SkyWeather::parse(&bin(&[(0, 0.2, 0.2, 0xFFFF0000, 0xFFFF0000, 0.0, 0.0, 0.0), (10, 0.0, 0.0, 0xFF0000FF, 0xFF0000FF, 0.0, 0.0, 0.0)])).unwrap();
        let c = w.sample(5, 100);
        assert_eq!(c.sky_fog_colour, [1.0, 0.0, 0.0]);
        assert_eq!(c.world_fog_colour, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn short_or_other_files_are_rejected() {
        assert!(SkyWeather::parse(&[0u8; 100]).is_none());
        let mut b = bin(&[]);
        b[0] = 7;
        assert!(SkyWeather::parse(&b).is_none());
        // An empty keyframe table becomes retail's flat default.
        assert_eq!(SkyWeather::parse(&bin(&[])).unwrap().keys.len(), 2);
    }

    #[test]
    fn map_header_and_sections() {
        let text = "[header]\r\ncount=2\r\ntint=1.000000,0.5,0.5,0.5\r\nambient=1.000000,1.231788,1.231788,1.231788\r\n\r\n[section0]\r\nstartslice=0\r\nendslice=0\r\ntexture1=1\r\ntexture1=2\r\n\r\n[section1]\r\nstartslice=414\r\nendslice=420\r\ntexture1.open=540103746\r\ntexturesize=5.0\r\n";
        let m = TrackLighting::parse(text);
        assert_eq!(m.tint, [1.0, 0.5, 0.5, 0.5]);
        assert!((m.ambient[1] - 1.231788).abs() < 1e-6);
        assert_eq!(m.light_decal, [1.0; 4]);
        assert_eq!(m.sections.len(), 2);
        assert_eq!(m.sections[1].start_slice, 414);
        assert_eq!(m.sections[1].textures[0].0, "BT1");
        assert_eq!(m.sections[0].textures[0].0, "ENV0");
        assert_eq!(m.sections[1].texture_size, 5.0);
        assert_eq!(TrackLighting::parse("").ambient, [1.0, 0.3, 0.3, 0.3]);
    }

    #[test]
    fn light_map_lookup() {
        let mut b = vec![0u8; 24 + 16 + 128 * 128];
        b[0..4].copy_from_slice(b"SHPP");
        b[8..12].copy_from_slice(&1u32.to_le_bytes());
        b[16..20].copy_from_slice(b"nght");
        b[20..24].copy_from_slice(&24u32.to_le_bytes());
        b[24] = 0x41;
        b[28..30].copy_from_slice(&128u16.to_le_bytes());
        b[30..32].copy_from_slice(&128u16.to_le_bytes());
        for p in &mut b[40..] {
            *p = 200;
        }
        let m = LightMap::parse_psh(&b).unwrap();
        assert_eq!((m.width, m.height), (128, 128));
        assert!((m.intensity(0.0, 10.0, 0.0) - 200.0 / 256.0).abs() < 1e-6);
        assert!((m.intensity(0.0, 10.0, 1.0) - 168.0 / 256.0).abs() < 1e-6);
        assert_eq!(m.intensity(0.0, 0.2, 0.0), 0.0, "behind the first row");
        assert_eq!(m.intensity(20.0, 10.0, 0.0), 0.0, "too far sideways");
        assert!(LightMap::parse_psh(b"SHPI").is_none());
    }
}
