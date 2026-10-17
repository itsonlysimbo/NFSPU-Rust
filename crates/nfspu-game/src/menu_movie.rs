//! Plays the front end's movies (`FEData/Movies/*.mad`) frame by frame for the menu compositor.
//!
//! A movie control names a source (`othersource`). `demoloop` is the retail main menu's loop: a
//! playlist starting with `ch6`, to which the era clips `er1pc-av`..`er4pc-av` are added as the
//! selected player's career progress unlocks them (verified in the executable; see `playlist`).

use crate::menu::{Draw, Rect};
use nfspu_formats::ffn::Font;
use nfspu_formats::mad::{chunks, Decoder};
use std::path::{Path, PathBuf};

/// The two fonts the overlay's lettering picks from (small and large).
pub const OVERLAY_FONTS: [&str; 2] = ["Univers7b.ffn", "Univers11b.ffn"];
/// The front end's clock runs at 128 ticks per second (the retail timer is started at 0x80 Hz);
/// every overlay timing below is in these ticks.
pub const TICKS_PER_SECOND: f32 = 128.0;
/// The overlay's markers: `circles.fsh` frames 0-4 (8 to 128 pixels), 16 ticks each.
const CIRCLES: &str = "circles.fsh";
const CIRCLE_FRAMES: usize = 5;
const CIRCLE_FRAME_TICKS: f32 = 16.0;
/// Colours from the retail overlay routine (the white set; an amber set is used in a game mode
/// not identified yet).
const LINE_ALPHA: f32 = 0x4C as f32 / 255.0;
const CIRCLE_ALPHA: f32 = 0x30 as f32 / 255.0;
const TEXT_ALPHA: f32 = 0xA0 as f32 / 255.0;
const LETTERING: &str = "NEED FOR SPEED";

/// A small repeatable random source (the retail game uses the C library's `rand`; the exact
/// sequence does not matter, only the ranges).
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 >> 1
    }
    /// 0..n
    fn below(&mut self, n: u32) -> u32 {
        if n == 0 { 0 } else { self.next() % n }
    }
    fn coin(&mut self) -> bool {
        self.next() & 1 == 1
    }
}

/// A line across the movie that drifts and bounces, with an occasional fast streak along it.
struct Line {
    /// Horizontal lines move up and down; vertical ones left and right.
    horizontal: bool,
    pos: f32,
    min: f32,
    max: f32,
    ticks_per_pixel: f32,
    forward: bool,
    /// Streak: position of its start along the line, direction (+1/-1) and speed (px/tick).
    streak: Option<(f32, f32, f32)>,
    /// Time until the next streak starts, in ticks.
    streak_wait: f32,
}

/// A marker that plays where a horizontal and a vertical line cross.
struct Marker {
    playing: Option<(usize, usize, f32)>,
    wait: f32,
}

/// One "NEED FOR SPEED" appearance.
struct Lettering {
    showing: Option<LetteringShow>,
    wait: f32,
}

struct LetteringShow {
    elapsed: f32,
    duration: f32,
    big: bool,
    vertical: bool,
    /// 0: starts at the point; 1: centred on it; 2: ends at it.
    anchor: u32,
    at: [f32; 2],
}

/// The animated lines, markers and lettering the retail movie circle draws over its movie.
///
/// Structure, counts, colours and timings are from the retail routine, in 1/128 s ticks: 3
/// horizontal and 3 vertical lines (1 pixel every 5-9 ticks, bouncing; a streak a fifth of the
/// span long at 3-5 px/tick after a 0-127 tick wait); 3 markers (after 0-511 ticks, `circles.fsh`
/// frames at a random crossing, 16 ticks each); 4 letterings (after 0-383 ticks, shown 256-511
/// ticks, random font, direction, anchor and point). The lettering's letter-spacing growth and fade were floating-point code that did
/// not survive decompilation; here spacing stays 1 for the first 20% then grows to 3, and alpha
/// fades out over the same 80% (**approximation**). The streak's bright colour is also a guess.
pub struct Overlay {
    rect: Rect,
    rng: Rng,
    lines: Vec<Line>,
    markers: Vec<Marker>,
    letterings: Vec<Lettering>,
}

impl Overlay {
    pub fn new(rect: Rect, seed: u32) -> Overlay {
        let mut rng = Rng(seed.max(1));
        let [x, y, w, h] = rect;
        let mut lines = Vec::new();
        for horizontal in [true, false] {
            for _ in 0..3 {
                let (min, max) = if horizontal { (y, y + h) } else { (x, x + w) };
                lines.push(Line {
                    horizontal,
                    pos: min + rng.below((max - min) as u32) as f32,
                    min,
                    max,
                    ticks_per_pixel: (rng.below(5) + 5) as f32,
                    forward: rng.coin(),
                    streak: None,
                    streak_wait: rng.below(128) as f32,
                });
            }
        }
        let markers = (0..3).map(|_| Marker { playing: None, wait: rng.below(512) as f32 }).collect();
        let letterings = (0..4).map(|_| Lettering { showing: None, wait: rng.below(384) as f32 }).collect();
        Overlay { rect, rng, lines, markers, letterings }
    }

    /// Moves everything on by `elapsed_ms` of real time.
    pub fn advance(&mut self, elapsed_ms: f32) {
        // Work in the retail clock's ticks; cap a long stall at a quarter second.
        let ticks = elapsed_ms.min(250.0) * TICKS_PER_SECOND / 1000.0;
        let [x, y, w, h] = self.rect;
        for line in &mut self.lines {
            // Retail also moves at least 1 pixel per drawn frame, which ties speed to frame rate;
            // here it is the tick rate alone, smoothly.
            let step = ticks / line.ticks_per_pixel;
            line.pos += if line.forward { step } else { -step };
            if line.pos < line.min {
                line.pos = line.min;
                line.forward = true;
            }
            if line.pos > line.max - 1.0 {
                line.pos = line.max - 1.0;
                line.forward = false;
            }
            let (start, end) = if line.horizontal { (x, x + w) } else { (y, y + h) };
            let len = (end - start) / 5.0;
            match &mut line.streak {
                None => {
                    line.streak_wait -= ticks;
                    if line.streak_wait <= 0.0 {
                        let speed = (self.rng.below(3) + 3) as f32;
                        line.streak = Some(if self.rng.coin() { (start - len, 1.0, speed) } else { (end, -1.0, speed) });
                    }
                }
                Some((at, dir, speed)) => {
                    *at += *dir * *speed * ticks;
                    if (*dir > 0.0 && *at > end) || (*dir < 0.0 && *at < start - len) {
                        line.streak = None;
                        line.streak_wait = self.rng.below(128) as f32;
                    }
                }
            }
        }
        let (horizontal, vertical) = (3usize, 3usize);
        for marker in &mut self.markers {
            match &mut marker.playing {
                None => {
                    marker.wait -= ticks;
                    if marker.wait <= 0.0 {
                        let h_line = self.rng.below(horizontal as u32) as usize;
                        let v_line = horizontal + self.rng.below(vertical as u32) as usize;
                        marker.playing = Some((h_line, v_line, 0.0));
                    }
                }
                Some((_, _, t)) => {
                    *t += ticks;
                    if *t >= CIRCLE_FRAMES as f32 * CIRCLE_FRAME_TICKS {
                        marker.playing = None;
                        marker.wait = self.rng.below(512) as f32;
                    }
                }
            }
        }
        for lettering in &mut self.letterings {
            match &mut lettering.showing {
                None => {
                    lettering.wait -= ticks;
                    if lettering.wait <= 0.0 {
                        lettering.showing = Some(LetteringShow {
                            elapsed: 0.0,
                            duration: (self.rng.below(256) + 256) as f32,
                            big: self.rng.coin(),
                            vertical: self.rng.coin(),
                            anchor: self.rng.below(3),
                            at: [x + self.rng.below(w as u32) as f32, y + self.rng.below(h as u32) as f32],
                        });
                    }
                }
                Some(show) => {
                    show.elapsed += ticks;
                    if show.elapsed >= show.duration {
                        lettering.showing = None;
                        lettering.wait = self.rng.below(384) as f32;
                    }
                }
            }
        }
    }

    /// The pieces to draw now, in screen positions, topmost first. `fonts` are `OVERLAY_FONTS`.
    pub fn draws(&self, fonts: &[Option<&Font>]) -> Vec<Draw> {
        let [x, y, w, h] = self.rect;
        let mut out = Vec::new();
        // Lettering on top, then markers, then lines (retail draws lines first).
        for show in self.letterings.iter().filter_map(|l| l.showing.as_ref()) {
            let font_name = OVERLAY_FONTS[show.big as usize];
            let Some(font) = fonts.get(show.big as usize).copied().flatten() else { continue };
            let t = show.elapsed / show.duration;
            let late = ((t - 0.2) / 0.8).clamp(0.0, 1.0);
            let spacing = 1.0 + 2.0 * late;
            let alpha = TEXT_ALPHA * (1.0 - late);
            let steps: Vec<f32> = LETTERING
                .chars()
                .map(|c| if show.vertical { font.line_height.max(8) as f32 } else { font.glyph(c).map(|g| g.advance as f32).unwrap_or(4.0) } * spacing)
                .collect();
            let total: f32 = steps.iter().sum();
            let shift = match show.anchor {
                1 => -total / 2.0,
                2 => -total,
                _ => 0.0,
            };
            let mut pen = shift;
            for (c, step) in LETTERING.chars().zip(steps) {
                if c != ' ' {
                    let pos = if show.vertical { [show.at[0], show.at[1] + pen] } else { [show.at[0] + pen, show.at[1]] };
                    out.push(Draw::Text { text: c.to_string(), pos, size: 0.0, colour: [1.0, 1.0, 1.0, alpha], font: font_name.to_string() });
                }
                pen += step;
            }
        }
        for (h_line, v_line, t) in self.markers.iter().filter_map(|m| m.playing) {
            let frame = (t / CIRCLE_FRAME_TICKS) as usize;
            if frame >= CIRCLE_FRAMES {
                continue;
            }
            // Centred on the crossing; circle sizes per frame from `circles.fsh`.
            let size = [8.0, 32.0, 64.0, 92.0, 128.0][frame];
            let (cx, cy) = (self.lines[v_line].pos, self.lines[h_line].pos);
            out.push(Draw::Picture { file: CIRCLES.into(), index: frame, pos: [cx - size / 2.0, cy - size / 2.0], flip: 0, alpha: CIRCLE_ALPHA });
        }
        let white = |a: f32| [1.0, 1.0, 1.0, a];
        for line in &self.lines {
            if let Some((at, dir, _)) = line.streak {
                let len = if line.horizontal { w / 5.0 } else { h / 5.0 };
                // Bright at the leading end, clear at the tail.
                let (lead, tail) = (white(0.5), white(0.0));
                let (c0, c1) = if dir > 0.0 { (tail, lead) } else { (lead, tail) };
                let corners = if line.horizontal {
                    [[at, line.pos], [at + len, line.pos], [at, line.pos + 1.0], [at + len, line.pos + 1.0]]
                } else {
                    [[line.pos, at], [line.pos + 1.0, at], [line.pos, at + len], [line.pos + 1.0, at + len]]
                };
                let colours = if line.horizontal { [c0, c1, c0, c1] } else { [c0, c0, c1, c1] };
                out.push(Draw::Gradient { corners, colours, when: crate::menu::Show::Always });
            }
            let rect = if line.horizontal { [x, line.pos, w, 1.0] } else { [line.pos, y, 1.0, h] };
            out.push(Draw::Quad { rect, colour: white(LINE_ALPHA), hover: None, link: None, back: false });
        }
        out
    }
}

/// The selected player's evolution-career progress, as the demo loop uses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CareerProgress {
    /// The career year (1950 at the start; raised as eras are won).
    pub year: i32,
    /// Set once the career has been started.
    pub started: bool,
}

impl CareerProgress {
    /// Reads progress from a `.sav`. The save is a run of named sections; the executable's loaders
    /// read the career year as the first 32-bit value after the `Tournament` name, and a flag byte
    /// after the `PlayerInfo` name. Which byte is the flag was matched by comparing three saves
    /// (1 only in the profile that owns a car; **likely**, not traced byte for byte).
    pub fn from_save(data: &[u8]) -> Option<CareerProgress> {
        let after = |name: &[u8]| {
            let mut tag = name.to_vec();
            tag.push(0);
            data.windows(tag.len()).position(|w| w == tag.as_slice()).map(|p| p + tag.len())
        };
        let year_at = after(b"Tournament")?;
        let year = i32::from_le_bytes(data.get(year_at..year_at + 4)?.try_into().ok()?);
        let started = after(b"PlayerInfo").and_then(|p| data.get(p)).map(|b| *b != 0).unwrap_or(false);
        Some(CareerProgress { year, started })
    }
}

/// The clips a movie source plays, in order.
///
/// `demoloop` (verified in the executable): `ch6` always, then `er1pc-av` once the career has
/// started, `er2pc-av` from 1970, `er3pc-av` from 1989, `er4pc-av` from 2000 - each clip only if
/// every earlier one is in, since the loop runs up to the highest unlocked slot. Without a player
/// the loop is `ch6` alone.
pub fn playlist(source: &str, progress: Option<CareerProgress>) -> Vec<&'static str> {
    match source {
        "demoloop" => {
            let mut highest = 0;
            if let Some(p) = progress {
                if p.started {
                    highest = 1;
                }
                for (slot, year) in [(2, 1970), (3, 1989), (4, 2000)] {
                    if p.year >= year {
                        highest = slot;
                    }
                }
            }
            ["ch6", "er1pc-av", "er2pc-av", "er3pc-av", "er4pc-av"][..=highest].to_vec()
        }
        "careerfirst" => vec!["er1pc-av"],
        _ => Vec::new(),
    }
}

/// One clip being decoded: its bytes, where its video chunks are, and the decoder state.
struct Clip {
    data: Vec<u8>,
    frames: Vec<(usize, usize)>,
}

/// A looping playlist with the current frame as RGBA8.
pub struct MoviePlayer {
    movies_dir: PathBuf,
    names: Vec<&'static str>,
    clip_index: usize,
    clip: Option<Clip>,
    frame: usize,
    decoder: Decoder,
    /// Time left on the current frame, in milliseconds.
    remaining_ms: f32,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// Problems to log (missing or undecodable files).
    pub problems: Vec<String>,
}

impl MoviePlayer {
    /// A player for `source` reading `<game>/FEData/Movies`. `None` when the source has no clips.
    pub fn new(game_dir: &Path, source: &str, progress: Option<CareerProgress>) -> Option<MoviePlayer> {
        let names = playlist(source, progress);
        if names.is_empty() {
            return None;
        }
        let mut player = MoviePlayer {
            movies_dir: game_dir.join("FEData").join("Movies"),
            names,
            clip_index: 0,
            clip: None,
            frame: 0,
            decoder: Decoder::new(),
            remaining_ms: 0.0,
            width: 0,
            height: 0,
            rgba: Vec::new(),
            problems: Vec::new(),
        };
        player.open(0);
        player.show_next();
        Some(player)
    }

    fn open(&mut self, index: usize) {
        self.clip_index = index % self.names.len();
        self.frame = 0;
        self.decoder = Decoder::new();
        let name = self.names[self.clip_index];
        let path = self.movies_dir.join(format!("{name}.mad"));
        self.clip = match std::fs::read(&path) {
            Ok(data) => {
                let mut frames = Vec::new();
                let mut at = 0usize;
                for c in chunks(&data) {
                    match c {
                        Ok(c) => {
                            if c.is_video() {
                                frames.push((at, c.data.len()));
                            }
                            at += c.data.len();
                        }
                        Err(e) => {
                            self.problems.push(format!("movie {name}: {e}"));
                            break;
                        }
                    }
                }
                Some(Clip { data, frames })
            }
            Err(e) => {
                self.problems.push(format!("movie {}: {e}", path.display()));
                None
            }
        };
    }

    /// Decodes the next frame (moving to the next clip at the end) into `rgba`.
    fn show_next(&mut self) {
        for _ in 0..2 {
            let Some(clip) = &self.clip else { return };
            if self.frame >= clip.frames.len() {
                let next = self.clip_index + 1;
                self.open(next);
                continue;
            }
            let (start, len) = clip.frames[self.frame];
            let bytes = &clip.data[start..start + len];
            let chunk = nfspu_formats::mad::Chunk { tag: [bytes[0], bytes[1], bytes[2], bytes[3]], data: bytes };
            self.frame += 1;
            match self.decoder.decode(&chunk) {
                Ok(pic) => {
                    self.width = pic.width as u32;
                    self.height = pic.height as u32;
                    self.rgba = pic.to_rgba();
                    self.remaining_ms += pic.frame_ms.max(1) as f32;
                }
                Err(e) => {
                    self.problems.push(format!("movie {} frame {}: {e}", self.names[self.clip_index], self.frame - 1));
                    self.remaining_ms += 66.0;
                }
            }
            return;
        }
    }

    /// Moves the movie on by `elapsed_ms`. Returns true when the picture changed.
    pub fn advance(&mut self, elapsed_ms: f32) -> bool {
        self.remaining_ms -= elapsed_ms;
        let mut changed = false;
        // Catch up at most a few frames after a stall rather than racing through the clip.
        let mut steps = 0;
        while self.remaining_ms <= 0.0 && steps < 4 {
            self.show_next();
            changed = true;
            steps += 1;
        }
        if self.remaining_ms <= 0.0 {
            self.remaining_ms = 0.0;
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_demo_loop_grows_with_career_progress() {
        let p = |year, started| Some(CareerProgress { year, started });
        assert_eq!(playlist("demoloop", None), vec!["ch6"]);
        assert_eq!(playlist("demoloop", p(1950, false)), vec!["ch6"]);
        assert_eq!(playlist("demoloop", p(1950, true)), vec!["ch6", "er1pc-av"]);
        assert_eq!(playlist("demoloop", p(1975, false)), vec!["ch6", "er1pc-av", "er2pc-av"]);
        assert_eq!(playlist("demoloop", p(2000, true)).len(), 5);
    }

    #[test]
    fn progress_comes_from_named_sections() {
        let mut save = b"Missions\0junkTournament\0".to_vec();
        save.extend_from_slice(&1975i32.to_le_bytes());
        save.extend_from_slice(b"morePlayerInfo\0\x01");
        assert_eq!(CareerProgress::from_save(&save), Some(CareerProgress { year: 1975, started: true }));
        assert_eq!(CareerProgress::from_save(b"nothing here"), None);
    }

    #[test]
    fn overlay_lines_stay_inside_and_everything_eventually_shows() {
        let rect = [112.0, 100.0, 416.0, 348.0];
        let mut overlay = Overlay::new(rect, 7);
        let (mut saw_marker, mut saw_streak) = (false, false);
        for _ in 0..600 {
            overlay.advance(16.0);
            for line in &overlay.lines {
                assert!(line.pos >= line.min && line.pos <= line.max - 1.0);
            }
            saw_marker |= overlay.markers.iter().any(|m| m.playing.is_some());
            saw_streak |= overlay.lines.iter().any(|l| l.streak.is_some());
        }
        assert!(saw_marker && saw_streak);
        let draws = overlay.draws(&[None, None]);
        let lines = draws.iter().filter(|d| matches!(d, Draw::Quad { .. })).count();
        assert_eq!(lines, 6, "three horizontal and three vertical lines");
    }
}
