//! The in-race HUD as retail draws it: layout and draw list only (no window, no GPU). The pixels are
//! painted by `hud_render`, and `hud_plugin` puts them over the 3D view.
//!
//! Where things come from (details and evidence in `docs/provenance/hud.md`):
//! - Element boxes: the HUD layout files (`nfs5-1.hud` / `nfs5-2.hud`, read by
//!   `nfspu_formats::hudcfg`). Sizes are pixels; positions are fractions of the screen the record
//!   was saved for.
//! - Other screen sizes: retail does **not** scale the in-race HUD. It keeps every element at its
//!   640x480 pixel size and moves it: elements on the right half shift right by `width - 640`,
//!   those on the bottom half shift down by `height - 480`, and an element straddling the middle
//!   column is re-centred (`anchor`). `HudSettings::scale` can enlarge everything for big screens;
//!   1.0 is retail.
//! - Dials: face, needle and angles per car from `nfs5.car` (`nfspu_formats::cardash`).
//! - Words ("gear", "time", "lap", ...): `FEData/Locale/hud.loc`.

use nfspu_formats::cardash::DialSpec;
use nfspu_formats::hudcfg::{Argb, HudBox, HudLayout, HudSwitches, Slot};
use std::collections::BTreeMap;

/// Everything the HUD shows. The race-rules code fills this each frame.
#[derive(Debug, Clone, PartialEq)]
pub struct HudInput {
    /// Road speed in km/h (sign ignored).
    pub speed_kmh: f32,
    pub rpm: f32,
    /// -1 = reverse, 0 = neutral, 1.. = forward gears.
    pub gear: i32,
    /// Current lap, counting from 1.
    pub lap: u32,
    pub total_laps: u32,
    /// Race position, 1 = leading.
    pub position: u32,
    pub car_count: u32,
    /// Seconds since the start.
    pub race_time: f32,
    /// Seconds into the current lap.
    pub lap_time: f32,
    pub best_lap: Option<f32>,
    /// Seconds left before the start, while the 3-2-1 countdown runs; `None` once racing.
    pub countdown: Option<f32>,
    /// Set once the player has crossed the finish line: finishing place and seconds since.
    pub finished: Option<(u32, f32)>,
}

impl Default for HudInput {
    fn default() -> Self {
        HudInput {
            speed_kmh: 0.0,
            rpm: 0.0,
            gear: 0,
            lap: 1,
            total_laps: 1,
            position: 1,
            car_count: 1,
            race_time: 0.0,
            lap_time: 0.0,
            best_lap: None,
            countdown: None,
            finished: None,
        }
    }
}

/// Which time the multi-purpose time box shows (retail's "split times" option).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitTime {
    BestLap,
    LastLap,
}

/// The car's three dials.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dials {
    pub speedo_kmh: DialSpec,
    pub speedo_mph: DialSpec,
    pub tach: DialSpec,
}

impl Dials {
    /// The '96 911 Carrera (993) dials as `nfs5.car` gives them, for when the file is missing.
    pub const CARRERA_993: Dials = Dials {
        speedo_kmh: DialSpec { face: 4, units: 300, needle: 2, start_angle: -40, millidegrees_per_unit: 910, pivot: 18 },
        speedo_mph: DialSpec { face: 5, units: 200, needle: 2, start_angle: -38, millidegrees_per_unit: 1353, pivot: 18 },
        tach: DialSpec { face: 7, units: 760, needle: 2, start_angle: -22, millidegrees_per_unit: 310, pivot: 18 },
    };
}

/// Fixed choices for a race: the layout, units and look.
#[derive(Debug, Clone, PartialEq)]
pub struct HudSettings {
    pub layout: HudLayout,
    pub switches: HudSwitches,
    pub dials: Dials,
    pub mph: bool,
    /// Night versions of the dial art (`...N.fsh`).
    pub night: bool,
    pub split: SplitTime,
    /// HUD words by `hud.loc` id; English fallbacks are used for missing ids.
    pub words: BTreeMap<u16, String>,
    /// 1.0 = retail (pixel sizes as stored). 2.0 draws everything twice as big.
    pub scale: f32,
}

/// String ids in `hud.loc` (English text in the comments).
pub mod word {
    pub const GEAR: u16 = 10; // "gear"
    pub const KMH: u16 = 11; // "km/h"
    pub const POSITION: u16 = 17; // "position"
    pub const TIME: u16 = 18; // "time"
    pub const LAP: u16 = 19; // "lap"
    pub const MPH: u16 = 20; // "mph"
    pub const BEST_LAP: u16 = 23; // "best lap"
    pub const LAST_LAP: u16 = 24; // "last lap"
    pub const RPM: u16 = 18144; // "rpm"
    pub const REVERSE: u16 = 18145; // "R"
    pub const NEUTRAL: u16 = 18146; // "N"
}

impl HudSettings {
    fn word(&self, id: u16) -> String {
        if let Some(w) = self.words.get(&id) {
            return w.clone();
        }
        match id {
            word::GEAR => "gear",
            word::KMH => "km/h",
            word::POSITION => "position",
            word::TIME => "time",
            word::LAP => "lap",
            word::MPH => "mph",
            word::BEST_LAP => "best lap",
            word::LAST_LAP => "last lap",
            word::RPM => "rpm",
            word::REVERSE => "R",
            word::NEUTRAL => "N",
            _ => "",
        }
        .to_string()
    }
}

/// A rectangle in screen pixels: x, y, width, height.
pub type Rect = [f32; 4];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
    Centre,
}

/// One thing to paint, in order (later items on top). Art is named by a `GameData/DashHud` file
/// and an entry: its 4-letter name, `#n` for the n-th entry, or empty for the first. The painter
/// multiplies the art by `tint` (per channel, alpha included).
#[derive(Debug, Clone, PartialEq)]
pub enum HudDraw {
    Fill { rect: Rect, colour: [f32; 4] },
    /// Art stretched to `rect`.
    Image { file: String, entry: String, rect: Rect, tint: [f32; 4] },
    /// Art at its own size times `scale`, centred on `centre`.
    Sprite { file: String, entry: String, centre: [f32; 2], scale: f32, tint: [f32; 4] },
    /// A picture stretched onto four corners: top-left, top-right, bottom-right, bottom-left of
    /// the picture, in screen pixels.
    Quad { file: String, entry: String, corners: [[f32; 2]; 4], tint: [f32; 4] },
    /// `pos` is the top of the line; `align` says which end of the text sits at `pos[0]`.
    Text { font: String, text: String, pos: [f32; 2], colour: [f32; 4], align: Align, scale: f32 },
}

const WHITE: [f32; 4] = [1.0; 4];

/// Retail's rule for moving a 640x480 box onto a bigger screen (see the module notes). Positions
/// and sizes are whole pixels like the game's.
pub fn anchor(x: i32, y: i32, w: i32, h: i32, screen: [i32; 2]) -> [i32; 2] {
    let [sw, sh] = screen;
    let (mut x, mut y) = (x, y);
    if sw > 640 {
        let bottom = y > 240;
        if x < 320 && x + w > 320 {
            x = sw / 2 - w / 2;
        } else if x > 320 {
            x += sw - 640;
        }
        if bottom {
            y += sh - 480;
        }
    }
    if x + w > sw {
        x = sw - w;
    }
    if y + h > sh {
        y = sh - h;
    }
    [x, y]
}

/// Element boxes in screen pixels for one screen size.
#[derive(Debug, Clone, PartialEq)]
pub struct Boxes {
    slots: Vec<(Slot, Rect)>,
    pub multi_time: Rect,
    pub tach_digital: Rect,
}

impl Boxes {
    pub fn slot(&self, slot: Slot) -> Rect {
        self.slots.iter().find(|(s, _)| *s == slot).map(|(_, r)| *r).unwrap_or([0.0; 4])
    }
}

/// Places a stored box: `saved` is the screen the record was saved for. When it differs from
/// `screen`, the box is first put on the 640x480 screen and then moved by `anchor`.
fn place(b: HudBox, saved: [i32; 2], screen: [i32; 2]) -> [i32; 4] {
    // The game turns fractions into pixels by truncation.
    let to_px = |f: f32, size: i32| (f as f64 * size as f64) as i32;
    if saved == screen {
        return [to_px(b.x, screen[0]), to_px(b.y, screen[1]), b.width, b.height];
    }
    let base = [to_px(b.x, saved[0]), to_px(b.y, saved[1])];
    // A record saved for another size than 640x480 is first brought back to 640x480 the same way.
    let base = if saved == [640, 480] { base } else { unanchor(base, b.width, saved) };
    let [x, y] = anchor(base[0], base[1], b.width, b.height, screen);
    [x, y, b.width, b.height]
}

/// The reverse of `anchor` for a box saved on a bigger screen (an approximation: centred boxes
/// come back centred).
fn unanchor([x, y]: [i32; 2], w: i32, saved: [i32; 2]) -> [i32; 2] {
    let (dx, dy) = (saved[0] - 640, saved[1] - 480);
    let cx = if (x + w / 2 - saved[0] / 2).abs() <= 1 { 320 - w / 2 } else if x > 320 + dx { x - dx } else { x };
    let cy = if y > 240 + dy { y - dy } else { y };
    [cx, cy]
}

/// Lays out every element for a screen of `screen` pixels. With `scale` > 1 the layout is made for
/// a screen `scale` times smaller and then blown up, so elements keep their relative places.
pub fn layout(settings: &HudSettings, saved: [i32; 2], screen: [u32; 2]) -> Boxes {
    let s = settings.scale.max(0.25);
    let virt = [(screen[0] as f32 / s).round() as i32, (screen[1] as f32 / s).round() as i32];
    let rect = |b: HudBox| {
        let [x, y, w, h] = place(b, saved, virt);
        [x as f32 * s, y as f32 * s, w as f32 * s, h as f32 * s]
    };
    Boxes {
        slots: Slot::ALL.iter().map(|&slot| (slot, rect(settings.layout.slot(slot)))).collect(),
        multi_time: rect(settings.switches.multi_time),
        tach_digital: rect(settings.switches.tach_digital),
    }
}

/// Needle angle in degrees (0 = right, counter-clockwise) for `value` on a dial. The top of the
/// scale (`units - 1`) sits at `start_angle`; each unit below it turns the needle further
/// counter-clockwise, so the needle sweeps clockwise as the value rises.
pub fn needle_angle(spec: &DialSpec, value: f32) -> f32 {
    let top = (spec.units.max(1) - 1) as f32;
    let v = value.max(0.0).floor().min(top);
    spec.start_angle as f32 + (top - v) * spec.millidegrees_per_unit as f32 * 0.001
}

/// Width and length of needle picture `index` (the same table for every needle file).
pub fn needle_size(index: u16) -> [i32; 2] {
    match index {
        1 => [19, 60],
        2 => [18, 65],
        3 => [17, 71],
        4 => [17, 64],
        _ => [5, 74],
    }
}

/// The needle's four corners on screen (picture top-left, top-right, bottom-right, bottom-left;
/// the picture's top is the tip). `centre` is the dial centre in screen pixels.
pub fn needle_quad(spec: &DialSpec, value: f32, centre: [f32; 2], scale: f32) -> [[f32; 2]; 4] {
    let [w, len] = needle_size(spec.needle);
    let a = needle_angle(spec, value).to_radians();
    let (dir_x, dir_y) = (a.cos(), a.sin());
    let half = ((w + 1) / 2) as f32;
    // Sideways half-width, and the back end / tip along the needle (y up).
    let (side_x, side_y) = (dir_y * half, -dir_x * half);
    let back = -(spec.pivot as f32);
    let tip = (len - spec.pivot as i32) as f32;
    let at = |along: f32, side: f32| [centre[0] + (along * dir_x + side * side_x) * scale, centre[1] - (along * dir_y + side * side_y) * scale];
    [at(tip, -1.0), at(tip, 1.0), at(back, 1.0), at(back, -1.0)]
}

/// PLACEHOLDER until the car model has an engine and gearbox: rpm and gear made up from road
/// speed (six gears, each spanning 50 km/h, 2500 to 7000 rpm). Not retail behaviour.
pub fn placeholder_engine(speed_kmh: f32) -> (f32, i32) {
    if speed_kmh < -1.0 {
        return (1500.0 + -speed_kmh * 60.0, -1);
    }
    let v = speed_kmh.max(0.0);
    if v < 1.0 {
        return (900.0, 0);
    }
    let gear = ((v / 50.0).floor() as i32 + 1).min(6);
    let into = (v - (gear - 1) as f32 * 50.0) / 50.0;
    (2500.0 + into.min(1.2) * 4500.0, gear)
}

/// `mm:ss.cc`, as the HUD writes times (e.g. `02:44.00`).
pub fn format_time(seconds: f32) -> String {
    let hundredths = (seconds.max(0.0) * 100.0).floor() as u64;
    let (m, s, c) = (hundredths / 6000, hundredths / 100 % 60, hundredths % 100);
    format!("{:02}:{:02}.{:02}", m.min(99), s, c)
}

fn argb(c: Argb) -> [f32; 4] {
    c.to_f32()
}

/// A value-over-label pair in a box (gear, time, speed): retail's combo text. The value is in the
/// value colour, right-aligned on the top half; the label is grey, left-aligned on the bottom half.
fn combo(out: &mut Vec<HudDraw>, settings: &HudSettings, rect: Rect, value: String, label: String) {
    let s = settings.scale;
    let bg = settings.layout.background;
    if bg.alpha() > 0 {
        out.push(HudDraw::Fill { rect, colour: argb(bg) });
    }
    let [pad_x, pad_y] = settings.layout.pad.map(|p| p as f32 * s);
    let font = settings.layout.fonts[0].clone();
    out.push(HudDraw::Text {
        font: font.clone(),
        text: value,
        pos: [rect[0] + rect[2] - pad_x, rect[1] + pad_y],
        colour: argb(settings.switches.value_colour()),
        align: Align::Right,
        scale: s,
    });
    out.push(HudDraw::Text { font, text: label, pos: [rect[0] + pad_x, rect[1] + rect[3] / 2.0], colour: argb(settings.layout.label), align: Align::Left, scale: s });
}

/// One analog dial: optional dark disc, face, needle, and the centre cap for the plain needle.
fn dial(out: &mut Vec<HudDraw>, settings: &HudSettings, rect: Rect, face_file: &str, needle_file: &str, spec: &DialSpec, value: f32) {
    let s = settings.scale;
    if settings.layout.background.alpha() > 0 {
        out.push(HudDraw::Image { file: "circletach.fsh".into(), entry: String::new(), rect, tint: WHITE });
    }
    out.push(HudDraw::Image { file: face_file.into(), entry: format!("#{}", spec.face), rect, tint: WHITE });
    // The game uses whole-pixel centres.
    let centre = [rect[0] + (rect[2] / s / 2.0).floor() * s, rect[1] + (rect[3] / s / 2.0).floor() * s];
    out.push(HudDraw::Quad { file: needle_file.into(), entry: format!("#{}", spec.needle), corners: needle_quad(spec, value, centre, s), tint: WHITE });
    if spec.needle == 0 {
        let cap = 20.0 * s;
        out.push(HudDraw::Image { file: needle_file.into(), entry: "#5".into(), rect: [centre[0] - cap / 2.0, centre[1] - cap / 2.0, cap, cap], tint: WHITE });
    }
}

/// Amber of the countdown and finish circles.
const AMBER: [f32; 3] = [0xFA as f32 / 255.0, 0xB4 as f32 / 255.0, 0.0];

/// Builds the draw list for one frame on a `screen`-sized window.
pub fn draws(input: &HudInput, settings: &HudSettings, boxes: &Boxes, screen: [u32; 2]) -> Vec<HudDraw> {
    let mut out = Vec::new();
    let sw = &settings.switches;
    let s = settings.scale;
    let night = if settings.night { "N" } else { "" };

    if let Some(left) = input.countdown {
        countdown(&mut out, left, screen, s);
        return out;
    }

    // Dials.
    if sw.tachometer == 2 {
        dial(&mut out, settings, boxes.slot(Slot::Tach), &format!("tach{night}.fsh"), &format!("needle{night}.fsh"), &settings.dials.tach, input.rpm / 10.0);
    }
    let speed = if settings.mph { input.speed_kmh.abs() / 1.609_344 } else { input.speed_kmh.abs() };
    if sw.speedometer == 2 {
        let (face, spec) = if settings.mph { (format!("speedM{night}.fsh"), settings.dials.speedo_mph) } else { (format!("speedK{night}.fsh"), settings.dials.speedo_kmh) };
        dial(&mut out, settings, boxes.slot(Slot::Speedometer), &face, &format!("needle2{night}.fsh"), &spec, speed);
    }
    // Digital readouts.
    if sw.speedometer == 1 {
        let unit = settings.word(if settings.mph { word::MPH } else { word::KMH });
        combo(&mut out, settings, boxes.slot(Slot::SpeedDigital), format!("{}", speed as i32), unit);
    }
    if sw.tachometer == 1 {
        combo(&mut out, settings, boxes.tach_digital, format!("{}", input.rpm.max(0.0) as i32), settings.word(word::RPM));
    }
    if sw.gear == 1 {
        let g = match input.gear {
            i32::MIN..=-1 => settings.word(word::REVERSE),
            0 => settings.word(word::NEUTRAL),
            n => n.to_string(),
        };
        combo(&mut out, settings, boxes.slot(Slot::Gear), g, settings.word(word::GEAR));
    }

    // Times.
    combo(&mut out, settings, boxes.slot(Slot::Time), format_time(input.race_time), settings.word(word::TIME));
    if sw.multi_time_on != 0 {
        let (label, value) = match settings.split {
            SplitTime::BestLap => (word::BEST_LAP, input.best_lap),
            SplitTime::LastLap => (word::LAST_LAP, None),
        };
        combo(&mut out, settings, boxes.multi_time, value.map(format_time).unwrap_or_else(|| "--:--.--".into()), settings.word(label));
    }

    // Position: the last row of the opponent-list box.
    if sw.opponent_list != 0 {
        let row = boxes.slot(Slot::Position);
        line(&mut out, settings, row, settings.word(word::POSITION), format!("{}/{}", input.position, input.car_count));
    }
    // Lap: retail writes it with the track map, along the map box's top.
    if sw.map != 0 {
        let map = boxes.slot(Slot::Map);
        let row = [map[0], map[1], map[2], 15.0 * s];
        line(&mut out, settings, row, settings.word(word::LAP), format!("{}/{}", input.lap.min(input.total_laps.max(1)), input.total_laps));
    }

    if let Some((place, since)) = input.finished {
        finish(&mut out, place, since, screen, s);
    }
    out
}

/// A one-line label + value row (position, lap), in the small font.
fn line(out: &mut Vec<HudDraw>, settings: &HudSettings, rect: Rect, label: String, value: String) {
    let s = settings.scale;
    let bg = settings.layout.background;
    if bg.alpha() > 0 {
        out.push(HudDraw::Fill { rect, colour: argb(bg) });
    }
    let font = if settings.layout.fonts[1].is_empty() { settings.layout.fonts[0].clone() } else { settings.layout.fonts[1].clone() };
    let pad = settings.layout.pad[0] as f32 * s;
    out.push(HudDraw::Text { font: font.clone(), text: label, pos: [rect[0] + pad, rect[1] + s], colour: argb(settings.layout.label), align: Align::Left, scale: s });
    out.push(HudDraw::Text { font, text: value, pos: [rect[0] + rect[2] - pad, rect[1] + s], colour: argb(settings.switches.value_colour()), align: Align::Right, scale: s });
}

fn centred(screen: [u32; 2], w: f32, h: f32) -> Rect {
    [((screen[0] as f32 - w) / 2.0).floor(), ((screen[1] as f32 - h) / 2.0).floor(), w, h]
}

/// The 3-2-1 start: an amber disc in the screen centre with the solid digit on it, and a large
/// outline copy of the digit that grows out of it and fades. `left` is seconds to the start.
/// Each digit shows for one second here; the growth and fade curve is a guess from four frames
/// of retail footage (see `docs/provenance/hud.md`).
pub fn countdown(out: &mut Vec<HudDraw>, left: f32, screen: [u32; 2], s: f32) {
    if left <= 0.0 {
        return;
    }
    let digit = (left.ceil() as u32).clamp(1, 3);
    let t = 1.0 - (left - (digit - 1) as f32).clamp(0.0, 1.0); // 0 -> 1 through this digit
    let (file, solid, outline) = match digit {
        1 => ("321go2.fsh", "1Sol", "1OL"),
        2 => ("321go.fsh", "2Sol", "2OL"),
        _ => ("321go.fsh", "3Sol", "3OL"),
    };
    let disc = 128.0 * s;
    out.push(HudDraw::Image { file: "circlefull.fsh".into(), entry: String::new(), rect: centred(screen, disc, disc), tint: [AMBER[0], AMBER[1], AMBER[2], 0.5] });
    out.push(HudDraw::Image { file: "circleoutline.fsh".into(), entry: String::new(), rect: centred(screen, disc, disc), tint: [AMBER[0], AMBER[1], AMBER[2], 0.25] });
    // Digits keep the art's own size (about 45 x 58).
    let centre = [(screen[0] / 2) as f32, (screen[1] / 2) as f32];
    out.push(HudDraw::Sprite { file: file.into(), entry: solid.into(), centre, scale: s, tint: WHITE });
    let grow = 1.0 + 7.0 * t;
    let fade = 0.5 * (1.0 - t);
    out.push(HudDraw::Sprite { file: file.into(), entry: outline.into(), centre, scale: grow * s, tint: [1.0, 1.0, 1.0, fade] });
}

/// Finishing place over the amber disc, for a few seconds after the line. Not checked against
/// retail footage yet.
pub fn finish(out: &mut Vec<HudDraw>, place: u32, since: f32, screen: [u32; 2], s: f32) {
    if since > 5.0 || place == 0 {
        return;
    }
    let disc = 128.0 * s;
    out.push(HudDraw::Image { file: "circlefull.fsh".into(), entry: String::new(), rect: centred(screen, disc, disc), tint: [AMBER[0], AMBER[1], AMBER[2], 0.5] });
    out.push(HudDraw::Image { file: "congrats1.fsh".into(), entry: String::new(), rect: centred(screen, disc, disc), tint: WHITE });
    out.push(HudDraw::Image { file: "circleoutline.fsh".into(), entry: String::new(), rect: centred(screen, disc, disc), tint: [AMBER[0], AMBER[1], AMBER[2], 0.25] });
    let (file, entry) = match place {
        1 => ("congrats3.fsh", "1.tg".to_string()),
        2 => ("congrats4.fsh", "2.tg".to_string()),
        3 => ("congrats4.fsh", "3.tg".to_string()),
        4..=7 => ("congrats2.fsh", format!("{place}OL")),
        _ => ("congrats3.fsh", "8OL".to_string()),
    };
    out.push(HudDraw::Sprite { file: file.into(), entry, centre: [(screen[0] / 2) as f32, (screen[1] / 2) as f32], scale: s, tint: WHITE });
}

#[cfg(test)]
mod tests {
    use super::*;
    use nfspu_formats::hudcfg::{parse_layouts, parse_switches, LAYOUT_RECORD, SWITCH_RECORD};

    /// A layout record with the stock "Config 1" boxes that matter here.
    fn settings() -> HudSettings {
        let mut r = vec![0u8; LAYOUT_RECORD];
        r[..8].copy_from_slice(b"Config 1");
        r[0x14..0x18].copy_from_slice(&0x8000_0080u32.to_le_bytes());
        r[0x34..0x38].copy_from_slice(&5i32.to_le_bytes());
        r[0x38..0x3C].copy_from_slice(&2i32.to_le_bytes());
        r[0x4C..0x4C + 15].copy_from_slice(b"UniversC12b.ffn");
        let mut put = |size_at: usize, pos_at: usize, w: i32, h: i32, x: f32, y: f32| {
            r[size_at..size_at + 4].copy_from_slice(&w.to_le_bytes());
            r[size_at + 4..size_at + 8].copy_from_slice(&h.to_le_bytes());
            r[pos_at..pos_at + 4].copy_from_slice(&x.to_le_bytes());
            r[pos_at + 4..pos_at + 8].copy_from_slice(&y.to_le_bytes());
        };
        put(0x94, 0x188, 85, 35, 0.8671875, 0.925); // gear (555, 444)
        put(0xC4, 0x1B8, 128, 128, 0.8, 0.6395833); // tach (512, 307)
        put(0xD4, 0x1C0, 128, 128, 0.6, 0.6645833); // speedo (384, 318)
        put(0xFC, 0x1D8, 240, 84, 0.2890625, 0.0); // rear-view mirror (185, 0)
        let mut sw = vec![0u8; SWITCH_RECORD];
        sw[0x28..0x2C].copy_from_slice(&640i32.to_le_bytes());
        sw[0x2C..0x30].copy_from_slice(&480i32.to_le_bytes());
        for (at, v) in [(0x30, 2), (0x38, 2), (0x48, 1), (0x64, 1)] {
            sw[at..at + 4].copy_from_slice(&(v as i32).to_le_bytes());
        }
        HudSettings {
            layout: parse_layouts(&r).remove(0),
            switches: parse_switches(&sw).remove(0),
            dials: Dials::CARRERA_993,
            mph: false,
            night: false,
            split: SplitTime::BestLap,
            words: BTreeMap::new(),
            scale: 1.0,
        }
    }

    #[test]
    fn anchoring_matches_the_players_saved_1440x900_layout() {
        // The player's Greg1.hud holds Config 1 re-saved by the game for 1440x900. Its boxes are
        // the 640x480 ones moved by these rules, sizes unchanged.
        let screen = [1440, 900];
        assert_eq!(anchor(512, 307, 128, 128, screen), [1312, 727]); // tach: right + bottom
        assert_eq!(anchor(384, 318, 128, 128, screen), [1184, 738]); // speedo
        assert_eq!(anchor(185, 0, 240, 84, screen), [600, 0]); // mirror: re-centred
        assert_eq!(anchor(125, 0, 59, 85, screen), [125, 0]); // leader: stays top-left
        assert_eq!(anchor(0, 358, 120, 120, screen), [0, 778]); // map: bottom-left
        assert_eq!(anchor(555, 444, 85, 35, screen), [1355, 864]); // gear
        // 640x480 itself does not move anything; a box past the edge is pulled back in.
        assert_eq!(anchor(600, 470, 85, 35, [640, 480]), [555, 445]);
    }

    #[test]
    fn layout_uses_truncated_pixels_and_optional_scale() {
        let mut s = settings();
        let b = layout(&s, [640, 480], [640, 480]);
        assert_eq!(b.slot(Slot::Tach), [512.0, 306.0, 128.0, 128.0]); // 0.6395833 * 480 = 306.99..
        let b = layout(&s, [640, 480], [1440, 900]);
        assert_eq!(b.slot(Slot::Gear), [1355.0, 864.0, 85.0, 35.0]);
        s.scale = 2.0;
        let b = layout(&s, [640, 480], [1280, 960]);
        assert_eq!(b.slot(Slot::Gear), [1110.0, 888.0, 170.0, 70.0]);
    }

    #[test]
    fn needle_sweeps_clockwise_with_speed() {
        let kmh = Dials::CARRERA_993.speedo_kmh;
        // 0 km/h: start + 299 units * 0.91 degrees = 232.09 (lower left).
        assert!((needle_angle(&kmh, 0.0) - 232.09).abs() < 0.01);
        assert!((needle_angle(&kmh, 299.0) - -40.0).abs() < 0.01);
        assert!((needle_angle(&kmh, 1000.0) - -40.0).abs() < 0.01, "clamped at the top of the scale");
        assert!(needle_angle(&kmh, 100.0) > needle_angle(&kmh, 200.0));
        let tach = Dials::CARRERA_993.tach;
        assert!((needle_angle(&tach, 0.0) - (-22.0 + 759.0 * 0.31)).abs() < 0.01);
    }

    #[test]
    fn needle_quad_points_up_at_ninety_degrees() {
        // A dial whose only unit sits at 90 degrees: the needle points straight up.
        let spec = DialSpec { face: 0, units: 1, needle: 0, start_angle: 90, millidegrees_per_unit: 0, pivot: 10 };
        let q = needle_quad(&spec, 0.0, [100.0, 100.0], 1.0);
        let close = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3;
        // 5 px wide -> half width 3; 74 long, 10 behind the centre.
        assert!(close(q[0], [97.0, 36.0]), "{q:?}");
        assert!(close(q[1], [103.0, 36.0]), "{q:?}");
        assert!(close(q[2], [103.0, 110.0]), "{q:?}");
        assert!(close(q[3], [97.0, 110.0]), "{q:?}");
    }

    #[test]
    fn placeholder_engine_shifts_up_with_speed() {
        assert_eq!(placeholder_engine(0.0).1, 0);
        assert_eq!(placeholder_engine(-10.0).1, -1);
        assert_eq!(placeholder_engine(30.0).1, 1);
        assert_eq!(placeholder_engine(120.0).1, 3);
        assert_eq!(placeholder_engine(400.0).1, 6);
        assert!(placeholder_engine(49.0).0 > placeholder_engine(10.0).0);
    }

    #[test]
    fn times_format_like_retail() {
        assert_eq!(format_time(164.0), "02:44.00");
        assert_eq!(format_time(13.599), "00:13.59");
        assert_eq!(format_time(-1.0), "00:00.00");
    }

    #[test]
    fn draw_list_has_dials_gear_and_position() {
        let s = settings();
        let b = layout(&s, [640, 480], [640, 480]);
        let input = HudInput { speed_kmh: 120.0, rpm: 4000.0, gear: 3, position: 2, car_count: 5, ..Default::default() };
        let d = draws(&input, &s, &b, [640, 480]);
        let images: Vec<&str> = d.iter().filter_map(|x| if let HudDraw::Image { file, .. } = x { Some(file.as_str()) } else { None }).collect();
        assert!(images.contains(&"tach.fsh") && images.contains(&"speedK.fsh") && images.contains(&"circletach.fsh"));
        let quads = d.iter().filter(|x| matches!(x, HudDraw::Quad { .. })).count();
        assert_eq!(quads, 2);
        let texts: Vec<&str> = d.iter().filter_map(|x| if let HudDraw::Text { text, .. } = x { Some(text.as_str()) } else { None }).collect();
        assert!(texts.contains(&"3") && texts.contains(&"gear") && texts.contains(&"2/5") && texts.contains(&"position"));
        // Reverse and neutral use the table's letters.
        let r = draws(&HudInput { gear: -1, ..Default::default() }, &s, &b, [640, 480]);
        assert!(r.iter().any(|x| matches!(x, HudDraw::Text { text, .. } if text == "R")));
    }

    #[test]
    fn countdown_replaces_the_hud_and_counts_down() {
        let s = settings();
        let b = layout(&s, [640, 480], [640, 480]);
        let entries = |left: f32| -> Vec<String> {
            draws(&HudInput { countdown: Some(left), ..Default::default() }, &s, &b, [640, 480])
                .into_iter()
                .filter_map(|x| match x {
                    HudDraw::Image { entry, .. } | HudDraw::Sprite { entry, .. } => Some(entry),
                    _ => None,
                })
                .collect()
        };
        assert!(entries(2.5).contains(&"3Sol".to_string()));
        assert!(entries(1.5).contains(&"2Sol".to_string()));
        assert!(entries(0.2).contains(&"1Sol".to_string()));
        assert!(!entries(2.5).iter().any(|e| e.starts_with('#')), "no dials during the countdown");
    }

    #[test]
    fn finish_shows_the_place_for_five_seconds() {
        let mut out = Vec::new();
        finish(&mut out, 2, 1.0, [640, 480], 1.0);
        assert!(out.iter().any(|x| matches!(x, HudDraw::Sprite { entry, .. } if entry == "2.tg")));
        let mut later = Vec::new();
        finish(&mut later, 2, 6.0, [640, 480], 1.0);
        assert!(later.is_empty());
    }
}
