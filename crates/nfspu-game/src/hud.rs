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
    /// Every car in the race by race index (empty outside a race): for the map and opponent list.
    pub racers: Vec<HudRacer>,
    /// Race index of the player in `racers`.
    pub player: usize,
    /// Race indices in running order, leader first.
    pub order: Vec<usize>,
    /// Road centre lines as (x, z) polylines in world metres, for the map.
    pub road: std::sync::Arc<Vec<Vec<[f32; 2]>>>,
    /// Length of one lap in metres (turns progress into the distance gaps of the opponent list).
    pub lap_metres: f32,
    /// The race summary table (`hud_results`): empty until it should be on screen.
    pub summary: Vec<crate::hud_results::SummaryRow>,
    /// Seconds the summary has been up (drives its opening animation).
    pub summary_since: f32,
    /// Whether the summary has the BEST LAP column (a race with laps).
    pub summary_laps: bool,
}

/// One car as the map and the opponent list need it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HudRacer {
    pub name: String,
    /// Body colour.
    pub colour: [u8; 3],
    /// World position (x, z) in metres and yaw in radians (the car faces (-sin yaw, -cos yaw) in x, z).
    pub place: [f32; 3],
    /// Race progress in laps.
    pub progress: f32,
    /// Road speed in m/s (for time gaps).
    pub speed: f32,
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
            racers: Vec::new(),
            player: 0,
            order: Vec::new(),
            road: Default::default(),
            lap_metres: 0.0,
            summary: Vec::new(),
            summary_since: 0.0,
            summary_laps: true,
        }
    }
}

/// What the opponent list writes after each name (retail's "opponent list" option).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GapMode {
    /// Metres ahead (+) or behind (-) the player, "-----" for the player (option value 1).
    Distance,
    /// Time ahead or behind, `-mm:ss.cc` (option value 2).
    Time,
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
    pub gaps: GapMode,
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
    pub const NAME: u16 = 18111; // "NAME"
    pub const CAR: u16 = 18112; // "CAR"
    pub const BEST_LAP_CAPS: u16 = 18113; // "BEST LAP"
    pub const TOTAL_TIME: u16 = 18114; // "TIME"
    pub const RACE_SUMMARY: u16 = 18130; // "Race Summary"
    pub const SINGLE_RACE: u16 = 18129; // "Single Race"
    pub const ESC_TO_EXIT: u16 = 18100; // "Esc to exit"
    pub const SPACE_TO_PROCEED: u16 = 18101; // "Spacebar to proceed"
    pub const RPM: u16 = 18144; // "rpm"
    pub const REVERSE: u16 = 18145; // "R"
    pub const NEUTRAL: u16 = 18146; // "N"
}

impl HudSettings {
    pub(crate) fn word(&self, id: u16) -> String {
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
            word::NAME => "NAME",
            word::CAR => "CAR",
            word::BEST_LAP_CAPS => "BEST LAP",
            word::TOTAL_TIME => "TIME",
            word::SINGLE_RACE => "Single Race",
            word::RACE_SUMMARY => "Race Summary",
            word::ESC_TO_EXIT => "Esc to exit",
            word::SPACE_TO_PROCEED => "Spacebar to proceed",
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
    /// A rectangle whose colour runs from `top` to `bottom`.
    Gradient { rect: Rect, top: [f32; 4], bottom: [f32; 4] },
    /// An anti-aliased line `width` pixels wide.
    Line { from: [f32; 2], to: [f32; 2], colour: [f32; 4], width: f32 },
    /// `draws`, cut to `rect`.
    Clip { rect: Rect, draws: Vec<HudDraw> },
}

/// Retail's box behind HUD text: black, fully opaque along the top edge fading to a quarter along
/// the bottom. The exe's fixed corner table is colour 000000 with alphas 63, 63, 255, 255 for its
/// corners in the order top-left, top-right, bottom-right, bottom-left, but retail footage (let's
/// play, 10:46) shows the boxes darkest at the top, so the 2D pass evidently has y pointing up; we
/// follow the footage. The layout's fill word only switches the box on (its alpha byte); its colour
/// (0x000080 in Config 1, the old blue) is never used.
fn tray(out: &mut Vec<HudDraw>, settings: &HudSettings, rect: Rect) {
    if settings.layout.background.alpha() > 0 {
        out.push(HudDraw::Gradient { rect, top: [0.0, 0.0, 0.0, 1.0], bottom: [0.0, 0.0, 0.0, 63.0 / 255.0] });
    }
}

pub(crate) const WHITE: [f32; 4] = [1.0; 4];

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

pub(crate) fn argb(c: Argb) -> [f32; 4] {
    c.to_f32()
}

/// A value-over-label pair in a box (gear, time, speed): retail's combo text. The value is in the
/// value colour, right-aligned on the top half; the label is grey, left-aligned on the bottom half.
fn combo(out: &mut Vec<HudDraw>, settings: &HudSettings, rect: Rect, value: String, label: String) {
    let s = settings.scale;
    tray(out, settings, rect);
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
pub(crate) const AMBER: [f32; 3] = [0xFA as f32 / 255.0, 0xB4 as f32 / 255.0, 0.0];

/// Builds the draw list for one frame on a `screen`-sized window.
pub fn draws(input: &HudInput, settings: &HudSettings, boxes: &Boxes, screen: [u32; 2]) -> Vec<HudDraw> {
    let mut out = Vec::new();
    let sw = &settings.switches;
    let s = settings.scale;
    let night = if settings.night { "N" } else { "" };

    // Retail footage (2026-10-22): the summary replaces the rest of the HUD.
    if !input.summary.is_empty() {
        crate::hud_results::summary(&mut out, settings, &input.summary, input.summary_since, screen, input.summary_laps);
        return out;
    }

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

    if sw.opponent_list != 0 {
        opponent_list(&mut out, input, settings, boxes.slot(Slot::Position));
    }
    if sw.map != 0 {
        let map = boxes.slot(Slot::Map);
        minimap(&mut out, input, settings, map);
        // Lap: retail writes it with the track map, along the map box's top.
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
    tray(out, settings, rect);
    let font = if settings.layout.fonts[1].is_empty() { settings.layout.fonts[0].clone() } else { settings.layout.fonts[1].clone() };
    let pad = settings.layout.pad[0] as f32 * s;
    out.push(HudDraw::Text { font: font.clone(), text: label, pos: [rect[0] + pad, rect[1] + s], colour: argb(settings.layout.label), align: Align::Left, scale: s });
    out.push(HudDraw::Text { font, text: value, pos: [rect[0] + rect[2] - pad, rect[1] + s], colour: argb(settings.switches.value_colour()), align: Align::Right, scale: s });
}

/// A car colour as the map and list draw it: very dark paints become dark grey (retail lifts any
/// colour whose channels are all below 0x14 to 0x202020 so black cars stay visible).
fn car_colour(c: [u8; 3]) -> [f32; 4] {
    let c = if c.iter().all(|&v| v < 0x14) { [0x20; 3] } else { c };
    [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0]
}

/// Gap text for a car against the player, as retail's opponent list writes it.
pub fn gap_text(mode: GapMode, car: &HudRacer, player: &HudRacer, is_player: bool, lap_metres: f32) -> String {
    let metres = (car.progress - player.progress) * lap_metres;
    if is_player {
        return "-----".into();
    }
    match mode {
        GapMode::Distance => {
            let m = metres as i32;
            if m == 0 {
                "-----".into()
            } else {
                m.to_string()
            }
        }
        GapMode::Time => {
            // Retail divides by the faster of the two speeds (or half the other one) and shows
            // nothing while the player is slower than 13.39 m/s.
            let (p, o) = (player.speed.abs(), car.speed.abs());
            if p <= 13.39 || metres == 0.0 {
                return "-----".into();
            }
            let v = if metres < 0.0 { o.max(p * 0.5) } else { p.max(o * 0.5) };
            let secs = metres / v.max(0.1);
            let t = format_time(secs.abs());
            if secs < 0.0 {
                format!("-{t}")
            } else {
                t
            }
        }
    }
}

/// Retail's opponent list (checked against the let's play at 10:46): one black box holding a row
/// per car in running order (the row height is the layout box's height), each with a square in
/// the car's colour, the driver's name in grey and the gap to the player in the value colour and
/// the italic font; under a thin grey line, "position N/M" in grey. Without race data only the
/// position row is drawn.
fn opponent_list(out: &mut Vec<HudDraw>, input: &HudInput, settings: &HudSettings, slot: Rect) {
    let s = settings.scale;
    let row_h = slot[3];
    let rows = if input.racers.get(input.player).is_some() { input.order.len() } else { 0 };
    tray(out, settings, [slot[0], slot[1], slot[2], row_h * (rows as f32 + 1.0) + 2.0 * s]);
    let small = if settings.layout.fonts[1].is_empty() { settings.layout.fonts[0].clone() } else { settings.layout.fonts[1].clone() };
    let italic = if settings.layout.fonts[2].is_empty() { small.clone() } else { settings.layout.fonts[2].clone() };
    let pad = settings.layout.pad[0] as f32 * s;
    let grey = argb(settings.layout.label);
    if let Some(p) = input.racers.get(input.player) {
        for (row, &car) in input.order.iter().enumerate() {
            let Some(r) = input.racers.get(car) else { continue };
            let y = slot[1] + row_h * row as f32;
            let sq = (7.0 * s).round();
            out.push(HudDraw::Fill { rect: [slot[0] + pad, y + ((row_h - sq) / 2.0).floor(), sq, sq], colour: car_colour(r.colour) });
            out.push(HudDraw::Text { font: small.clone(), text: r.name.clone(), pos: [slot[0] + pad + sq + 4.0 * s, y + s], colour: grey, align: Align::Left, scale: s });
            let gap = gap_text(settings.gaps, r, p, car == input.player, input.lap_metres);
            out.push(HudDraw::Text { font: italic.clone(), text: gap, pos: [slot[0] + slot[2] - pad, y + s], colour: argb(settings.switches.value_colour()), align: Align::Right, scale: s });
        }
    }
    let y = slot[1] + row_h * rows as f32 + s;
    if rows > 0 {
        out.push(HudDraw::Line { from: [slot[0] + pad, y], to: [slot[0] + slot[2] - pad, y], colour: [grey[0], grey[1], grey[2], 0.6], width: s });
    }
    let font = settings.layout.fonts[0].clone();
    out.push(HudDraw::Text { font: font.clone(), text: settings.word(word::POSITION), pos: [slot[0] + pad, y + s], colour: grey, align: Align::Left, scale: s });
    out.push(HudDraw::Text { font, text: format!("{}/{}", input.position, input.car_count), pos: [slot[0] + slot[2] - pad, y + s], colour: grey, align: Align::Right, scale: s });
}

/// Half the map's width in metres for a zoom step (retail: steps 0 and 1 = 185 m, 2 = 322 m,
/// 3 = 459 m, 4 = 600 m).
pub fn map_range(zoom: i32) -> f32 {
    let z = match zoom {
        2 => 33.0,
        3 => 66.0,
        4 => 100.0,
        _ => 0.0,
    };
    z * 4.15 + 185.0
}

/// World (x, z) to map pixels: the `centre` world point lands on `screen_centre`, the world
/// direction `up` points up the screen, `px_per_m` is the scale.
#[derive(Debug, Clone, Copy)]
pub struct MapView {
    pub centre: [f32; 2],
    pub up: [f32; 2],
    pub px_per_m: f32,
    pub screen_centre: [f32; 2],
}

impl MapView {
    pub fn project(&self, p: [f32; 2]) -> [f32; 2] {
        let d = [p[0] - self.centre[0], p[1] - self.centre[1]];
        // Seen from above in our right-handed world (y up), right of `up` is `up` turned a
        // quarter clockwise: facing -z, right is +x.
        let right = [-self.up[1], self.up[0]];
        let x = d[0] * right[0] + d[1] * right[1];
        let y = d[0] * self.up[0] + d[1] * self.up[1];
        [self.screen_centre[0] + x * self.px_per_m, self.screen_centre[1] - y * self.px_per_m]
    }
}

/// The view retail uses for map style `mode` (1 turns with the player's car, 2 keeps north up,
/// 3 shows the whole track) and zoom step `zoom`.
pub fn map_view(input: &HudInput, mode: i32, zoom: i32, rect: Rect) -> Option<MapView> {
    let p = input.racers.get(input.player)?;
    let screen_centre = [rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0];
    // North in the game files is +z; our world mirrors x only, so +z is still north.
    let north = [0.0, 1.0];
    if mode == 3 {
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for q in input.road.iter().flatten() {
            lo = [lo[0].min(q[0]), lo[1].min(q[1])];
            hi = [hi[0].max(q[0]), hi[1].max(q[1])];
        }
        if lo[0] > hi[0] {
            return None;
        }
        let half = ((hi[0] - lo[0]).max(hi[1] - lo[1]) * 0.5 + 20.0).max(1.0);
        return Some(MapView { centre: [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0], up: north, px_per_m: rect[2] / 2.0 / half, screen_centre });
    }
    let up = if mode == 1 { [-p.place[2].sin(), -p.place[2].cos()] } else { north };
    Some(MapView { centre: [p.place[0], p.place[1]], up, px_per_m: rect[2] / 2.0 / map_range(zoom), screen_centre })
}

/// Retail's track map: a dark square with a fixed grid, the road as a white line, and every car as
/// the `radar.fsh` car tinted in its colour, the player's drawn last. Everything is cut to the box.
fn minimap(out: &mut Vec<HudDraw>, input: &HudInput, settings: &HudSettings, rect: Rect) {
    let s = settings.scale;
    let mut d = Vec::new();
    if settings.layout.background.alpha() > 0 {
        // Colour and alpha not traced (the backing quad's corner colours are set at run time).
        d.push(HudDraw::Fill { rect, colour: [0.0, 0.0, 0.0, 0.5] });
    }
    // The grid stays put while the map moves: a line every `grid` pixels.
    let step = settings.layout.grid.max(4) as f32 * s;
    let grid = argb(settings.layout.extra_colours[4]);
    let mut k = step;
    while k < rect[2] - 0.5 {
        d.push(HudDraw::Line { from: [rect[0] + k, rect[1]], to: [rect[0] + k, rect[1] + rect[3]], colour: grid, width: s });
        k += step;
    }
    let mut k = step;
    while k < rect[3] - 0.5 {
        d.push(HudDraw::Line { from: [rect[0], rect[1] + k], to: [rect[0] + rect[2], rect[1] + k], colour: grid, width: s });
        k += step;
    }
    if let Some(view) = map_view(input, settings.switches.map, settings.switches.map_zoom, rect) {
        // Road: a white line, thinned to points at least a pixel apart.
        for run in input.road.iter() {
            let mut last: Option<[f32; 2]> = None;
            for (i, &q) in run.iter().enumerate() {
                let p = view.project(q);
                match last {
                    None => last = Some(p),
                    Some(a) => {
                        if (p[0] - a[0]).hypot(p[1] - a[1]) >= 1.0 || i + 1 == run.len() {
                            d.push(HudDraw::Line { from: a, to: p, colour: [1.0; 4], width: s });
                            last = Some(p);
                        }
                    }
                }
            }
        }
        // Cars: retail stretches the car picture over a quad 20 times two of the car's size fields
        // (which fields is not traced). The size here is fitted to the let's play (a car about
        // 0.65 of a grid cell long at the default zoom): 20 x (0.42 m, 1.0 m) half extents, the
        // picture's own 26:63 shape. Last car first, so the player ends on top.
        let (half_w, half_l) = (0.42 * 20.0 * view.px_per_m, 1.0 * 20.0 * view.px_per_m);
        let others = input.racers.iter().enumerate().rev().filter(|(i, _)| *i != input.player);
        for (_, r) in others.chain(input.racers.get(input.player).map(|r| (input.player, r))) {
            let c = view.project([r.place[0], r.place[1]]);
            let ahead = view.project([r.place[0] - r.place[2].sin(), r.place[1] - r.place[2].cos()]);
            let len = (ahead[0] - c[0]).hypot(ahead[1] - c[1]).max(1e-6);
            let f = [(ahead[0] - c[0]) / len, (ahead[1] - c[1]) / len];
            let rt = [-f[1], f[0]];
            let at = |a: f32, b: f32| [c[0] + f[0] * a + rt[0] * b, c[1] + f[1] * a + rt[1] * b];
            d.push(HudDraw::Quad {
                file: "radar.fsh".into(),
                entry: String::new(),
                corners: [at(half_l, -half_w), at(half_l, half_w), at(-half_l, half_w), at(-half_l, -half_w)],
                tint: car_colour(r.colour),
            });
        }
    }
    out.push(HudDraw::Clip { rect, draws: d });
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
        put(0xB4, 0x1A8, 138, 15, 0.784375, 0.0020833); // opponent list rows (502, 1)
        put(0xE4, 0x1C8, 120, 120, 0.0, 0.7458333); // track map (0, 358)
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
            gaps: GapMode::Distance,
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
    fn boxes_are_black_not_the_layout_colour() {
        // Config 1 stores 0x80000080 (would read as navy); retail draws black, faint at the bottom.
        let s = settings();
        let b = layout(&s, [640, 480], [640, 480]);
        let d = draws(&HudInput::default(), &s, &b, [640, 480]);
        let grads: Vec<_> = d.iter().filter_map(|x| if let HudDraw::Gradient { top, bottom, .. } = x { Some((*top, *bottom)) } else { None }).collect();
        assert!(!grads.is_empty());
        for (top, bottom) in grads {
            assert_eq!(top, [0.0, 0.0, 0.0, 1.0]);
            assert_eq!(&bottom[..3], &[0.0, 0.0, 0.0]);
            assert!((bottom[3] - 63.0 / 255.0).abs() < 1e-6);
        }
        assert!(!d.iter().any(|x| matches!(x, HudDraw::Fill { colour, .. } if colour[2] > 0.4 && colour[0] < 0.1)), "no blue fills");
    }

    fn racer(name: &str, progress: f32, x: f32, z: f32, yaw: f32) -> HudRacer {
        HudRacer { name: name.into(), colour: [200, 0, 0], place: [x, z, yaw], progress, speed: 30.0 }
    }

    #[test]
    fn gaps_read_like_retail() {
        let me = racer("You", 1.0, 0.0, 0.0, 0.0);
        let behind = racer("Leadfoot", 0.9, 0.0, 0.0, 0.0);
        assert_eq!(gap_text(GapMode::Distance, &me, &me, true, 3840.0), "-----");
        assert_eq!(gap_text(GapMode::Distance, &behind, &me, false, 3840.0), "-384");
        // 384 m at 30 m/s = 12.8 s
        assert_eq!(gap_text(GapMode::Time, &behind, &me, false, 3840.0), "-00:12.80");
        let slow = HudRacer { speed: 5.0, ..me.clone() };
        assert_eq!(gap_text(GapMode::Time, &behind, &slow, false, 3840.0), "-----");
    }

    #[test]
    fn opponent_list_rows_follow_running_order() {
        let mut s = settings();
        s.switches.opponent_list = 1;
        let b = layout(&s, [640, 480], [640, 480]);
        let input = HudInput {
            racers: vec![racer("You", 1.0, 0.0, 0.0, 0.0), racer("Amazon", 1.2, 0.0, 0.0, 0.0), racer("Dylan", 0.5, 0.0, 0.0, 0.0)],
            order: vec![1, 0, 2],
            position: 2,
            car_count: 3,
            lap_metres: 1000.0,
            ..Default::default()
        };
        let d = draws(&input, &s, &b, [640, 480]);
        let texts: Vec<(String, f32)> = d.iter().filter_map(|x| if let HudDraw::Text { text, pos, .. } = x { Some((text.clone(), pos[1])) } else { None }).collect();
        let y = |t: &str| texts.iter().find(|(x, _)| x == t).map(|(_, y)| *y).unwrap();
        assert!(y("Amazon") < y("You") && y("You") < y("Dylan") && y("Dylan") < y("position"));
        assert!(texts.iter().any(|(t, _)| t == "200") && texts.iter().any(|(t, _)| t == "-500") && texts.iter().any(|(t, _)| t == "2/3"));
    }

    #[test]
    fn map_puts_the_player_in_the_middle_and_turns_with_it() {
        let rect = [0.0, 358.0, 120.0, 120.0];
        // Player at (100, 50) facing -z (yaw 0); a car 92.5 m further along -z.
        let input = HudInput { racers: vec![racer("You", 0.0, 100.0, 50.0, 0.0), racer("B", 0.0, 100.0, 50.0 - 92.5, 0.0)], ..Default::default() };
        let v = map_view(&input, 1, 0, rect).unwrap();
        let close = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3;
        assert!(close(v.project([100.0, 50.0]), [60.0, 418.0]));
        // 92.5 m ahead at 185 m per 60 px = 30 px straight up.
        assert!(close(v.project([100.0, -42.5]), [60.0, 388.0]));
        // Heading-up: +x (right of a car facing -z) is to the right.
        assert!(v.project([110.0, 50.0])[0] > 60.0);
        // North-up (style 2): +z is up, so the car ahead (at -z) is below.
        let n = map_view(&input, 2, 0, rect).unwrap();
        assert!(n.project([100.0, -42.5])[1] > 418.0);
        assert!((map_range(4) - 600.0).abs() < 1e-3);
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
