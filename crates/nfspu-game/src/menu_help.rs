//! Front-end help tips: the small translucent box with a one-line description that appears when
//! the pointer rests on a control ("Select player profile and continue playing").
//!
//! Every layout control may carry `helptext=<string id>` (0 or empty means none). The retail
//! control's draw step shows the tip itself; the behaviour below is described from the retail
//! executable and matched against a retail let's play frame (see `docs/provenance/fe.md`, "Help
//! tips"):
//!
//! * The control must be hovered, enabled and visible. The hover clock restarts every frame it is
//!   not. After **128 ticks** (1 s of the 128 Hz front-end clock) the tip appears and fades in:
//!   alpha = `min(255, (ticks - 128) * 4)`, so fully shown after another 64 ticks (0.5 s).
//! * Text: font `Univers8b.ffn`, word-wrapped to lines narrower than **310** pixels. Each line
//!   adds its measured height + 2 (see [`text_height`]).
//! * Box: text width + 8 by text height + 4. Its bottom-left corner is the pointer position at the
//!   moment the tip first appears, and it stays there while shown. If the box would pass x = 640 it
//!   moves left to `636 - text width`; its top never goes above 0.
//! * Fill: white at `alpha * 0xC0 / 255`; a 1-pixel black outline at `alpha`; black text at
//!   `alpha`, 4 pixels in and 2 down from the box corner.
//!
//! [`tooltip`] is the pure part (text + timing + anchor -> draws); [`HelpTimer`] keeps the hover
//! clock and the anchor between frames; [`target`] reads a control's help string and hover area
//! while a screen is built.

use crate::menu::{Draw, Rect};
use nfspu_formats::fe::{Block, Locale};
use nfspu_formats::ffn::Font;

/// The font retail loads for help tips.
pub const FONT: &str = "Univers8b.ffn";
/// Hover time before a tip appears, in 128 Hz ticks (1 s).
pub const DELAY_TICKS: f32 = 128.0;
/// Lines are broken so each stays narrower than this many pixels.
pub const WRAP_WIDTH: u32 = 310;
/// The front-end clock rate.
pub const TICKS_PER_SECOND: f32 = 128.0;

/// A control's help: where the pointer must rest and what the tip says.
#[derive(Debug, Clone, PartialEq)]
pub struct Help {
    pub rect: Rect,
    pub text: String,
}

/// The help a control carries, from its `helptext=` string id, with the hover area taken from the
/// draw commands the control just produced (its largest rectangle or click target). `None` when the
/// control has no help (id 0 or empty, an unknown id) or drew nothing to hover over.
pub fn target(block: &Block, drawn: &[Draw], locale: &Locale, language: u8) -> Option<Help> {
    let id: u32 = block.get("helptext")?.trim().parse().ok().filter(|id| *id != 0)?;
    let text = locale.text(id, language)?.trim().to_string();
    if text.is_empty() {
        return None;
    }
    // The largest rectangle: a button also draws 1-unit outline strips and stems.
    let rect = drawn
        .iter()
        .filter_map(|d| match d {
            Draw::Quad { rect, .. } | Draw::Target { rect, .. } if rect[2] > 0.0 && rect[3] > 0.0 => Some(*rect),
            _ => None,
        })
        .fold(None, |best: Option<Rect>, r| if best.is_none_or(|b| r[2] * r[3] > b[2] * b[3]) { Some(r) } else { best })?;
    Some(Help { rect, text })
}

/// The help under a point among the targets of one screen; earlier targets are on top.
pub fn at(help: &[Help], p: [f32; 2]) -> Option<&Help> {
    help.iter().find(|h| {
        let [x, y, w, h] = h.rect;
        p[0] >= x && p[0] < x + w && p[1] >= y && p[1] < y + h
    })
}

/// Breaks `text` into lines that each measure less than `width` pixels, at spaces. A single word
/// wider than that keeps a line to itself.
pub fn wrap(text: &str, width: u32, measure: impl Fn(&str) -> u32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split(' ').filter(|w| !w.is_empty()) {
        let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if line.is_empty() || measure(&candidate) < width {
            line = candidate;
        } else {
            lines.push(std::mem::replace(&mut line, word.to_string()));
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// The tip's fade: 0 before the delay, then up 4 per tick to 255.
pub fn alpha(ticks: f32) -> u32 {
    let t = ticks.floor();
    if t < DELAY_TICKS {
        return 0;
    }
    (((t - DELAY_TICKS) * 4.0) as u32).min(255)
}

/// A line's height: from the highest glyph top to the lowest glyph bottom. Univers8b gives 11
/// for "Select player profile and continue playing", which matches the retail box (17 = 11 + 2 +
/// 4); the font's own line height (13) would give 19. **Guess** that retail measures each line
/// this way (one retail frame fits; "line height - 2" would fit it too).
pub fn text_height(font: &Font, line: &str) -> Option<u32> {
    let glyphs = line.chars().filter(|c| *c != ' ').filter_map(|c| font.glyph(c)).filter(|g| g.width > 0 && g.height > 0);
    let (top, bottom) = glyphs.fold((i32::MAX, i32::MIN), |(t, b), g| (t.min(g.y_offset as i32), b.max(g.y_offset as i32 + g.height as i32)));
    (bottom > top).then(|| (bottom - top) as u32)
}

/// The tip's box for text of `size` (width, height) with its bottom-left at `anchor`.
pub fn place(size: [f32; 2], anchor: [f32; 2]) -> Rect {
    let [w, h] = size;
    let mut x = anchor[0];
    if x + w + 8.0 > 640.0 {
        x = 636.0 - w;
    }
    let y = (anchor[1] - h - 4.0).max(0.0);
    [x, y, w + 8.0, h + 4.0]
}

/// What to draw for a tip: `ticks` is how long the control has been hovered (128 Hz), `anchor`
/// where the pointer was when the tip appeared. Topmost first, like `Screen::draw`. Without the
/// font (tests) lines are measured at 5 pixels a character and 9 pixels high.
pub fn tooltip(text: &str, ticks: f32, anchor: [f32; 2], font: Option<&Font>) -> Vec<Draw> {
    let a = alpha(ticks);
    if a == 0 {
        return Vec::new();
    }
    let measure = |s: &str| font.map(|f| f.measure(s)).unwrap_or(s.chars().count() as u32 * 5);
    let lines = wrap(text, WRAP_WIDTH, measure);
    // Each line adds its height + 2; lines are drawn that last step apart (as retail keeps it).
    let heights: Vec<f32> = lines.iter().map(|l| font.and_then(|f| text_height(f, l)).unwrap_or(9) as f32 + 2.0).collect();
    let line_step = heights.last().copied().unwrap_or(11.0);
    let width = lines.iter().map(|l| measure(l)).max().unwrap_or(0) as f32;
    let [x, y, w, h] = place([width, heights.iter().sum()], anchor);

    let text_alpha = a as f32 / 255.0;
    let fill_alpha = (a * 0xC0 / 255) as f32 / 255.0;
    let black = [0.0, 0.0, 0.0, text_alpha];
    let mut draws: Vec<Draw> = lines
        .into_iter()
        .enumerate()
        .map(|(i, line)| Draw::Text { text: line, pos: [x + 4.0, y + 2.0 + i as f32 * line_step], size: 8.0, colour: black, font: FONT.to_string() })
        .collect();
    // The outline as four 1-pixel strips (they overlap at the corners), over the fill.
    for rect in [[x + w - 1.0, y, 1.0, h], [x, y, 1.0, h], [x, y + h - 1.0, w, 1.0], [x, y, w, 1.0]] {
        draws.push(Draw::Quad { rect, colour: black, hover: None, link: None, back: false });
    }
    draws.push(Draw::Quad { rect: [x, y, w, h], colour: [1.0, 1.0, 1.0, fill_alpha], hover: None, link: None, back: false });
    draws
}

/// The hover clock for help tips between frames.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HelpTimer {
    /// The help being hovered (which layer's target and its text), how long, and where the tip was
    /// anchored when it first appeared.
    current: Option<(usize, Help)>,
    ticks: f32,
    anchor: Option<[f32; 2]>,
}

impl HelpTimer {
    /// Moves the clock on by `ms` with the pointer at `pointer` over `hovered` (the layer index and
    /// help of the control under it, if any), and returns the tip to draw (topmost first).
    pub fn update(&mut self, hovered: Option<(usize, &Help)>, pointer: [f32; 2], ms: f32, font: Option<&Font>) -> Vec<Draw> {
        let same = match (&self.current, hovered) {
            (Some((layer, help)), Some((l, h))) => *layer == l && help == h,
            _ => false,
        };
        if !same {
            self.current = hovered.map(|(l, h)| (l, h.clone()));
            self.ticks = 0.0;
            self.anchor = None;
            return Vec::new();
        }
        self.ticks += ms * TICKS_PER_SECOND / 1000.0;
        // Retail fixes the tip's place once the delay has passed, even while it is still clear.
        if self.ticks.floor() < DELAY_TICKS {
            return Vec::new();
        }
        let anchor = *self.anchor.get_or_insert(pointer);
        let Some((_, help)) = &self.current else { return Vec::new() };
        tooltip(&help.text, self.ticks, anchor, font)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quads(draws: &[Draw]) -> Vec<(Rect, [f32; 4])> {
        draws
            .iter()
            .filter_map(|d| match d {
                Draw::Quad { rect, colour, .. } => Some((*rect, *colour)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn nothing_before_one_second_then_a_half_second_fade() {
        assert_eq!(alpha(0.0), 0);
        assert_eq!(alpha(127.9), 0);
        assert_eq!(alpha(128.0), 0);
        assert_eq!(alpha(129.0), 4);
        assert_eq!(alpha(160.0), 128);
        assert_eq!(alpha(191.0), 252);
        assert_eq!(alpha(192.0), 255);
        assert_eq!(alpha(10_000.0), 255);
        assert!(tooltip("Play a quick race", 100.0, [300.0, 200.0], None).is_empty());
    }

    #[test]
    fn box_sits_above_and_right_of_the_pointer_with_retail_colours() {
        // "Play a quick race": 17 characters at 5 px = 85 wide, one 11 px line.
        let draws = tooltip("Play a quick race", 400.0, [300.0, 200.0], None);
        let q = quads(&draws);
        let (fill, fill_colour) = q.last().copied().unwrap();
        assert_eq!(fill, [300.0, 200.0 - 11.0 - 4.0, 93.0, 15.0]);
        assert_eq!(fill_colour, [1.0, 1.0, 1.0, 192.0 / 255.0]);
        // Outline strips: black, opaque at full fade, on the box edges.
        assert_eq!(q.len(), 5);
        assert!(q[..4].iter().all(|(_, c)| *c == [0.0, 0.0, 0.0, 1.0]));
        assert!(q.iter().any(|(r, _)| *r == [300.0, 185.0, 93.0, 1.0]));
        assert!(q.iter().any(|(r, _)| *r == [392.0, 185.0, 1.0, 15.0]));
        match &draws[0] {
            Draw::Text { text, pos, colour, font, .. } => {
                assert_eq!(text, "Play a quick race");
                assert_eq!(*pos, [304.0, 187.0]);
                assert_eq!(*colour, [0.0, 0.0, 0.0, 1.0]);
                assert_eq!(font, FONT);
            }
            other => panic!("expected text first, got {other:?}"),
        }
    }

    #[test]
    fn fading_scales_fill_outline_and_text_together() {
        let draws = tooltip("x", 160.0, [10.0, 100.0], None);
        let q = quads(&draws);
        assert_eq!(q.last().unwrap().1[3], (128 * 0xC0 / 255) as f32 / 255.0);
        assert_eq!(q[0].1[3], 128.0 / 255.0);
    }

    #[test]
    fn box_stays_on_screen() {
        // Near the right edge it moves left to 636 - width; near the top it stops at 0.
        assert_eq!(place([100.0, 11.0], [600.0, 100.0]), [536.0, 85.0, 108.0, 15.0]);
        assert_eq!(place([100.0, 11.0], [100.0, 5.0]), [100.0, 0.0, 108.0, 15.0]);
    }

    #[test]
    fn long_text_wraps_below_310_pixels() {
        let measure = |s: &str| s.chars().count() as u32 * 5;
        let text = "Set up car parameters such as driving aids, transmission, damage & convertible top up/down";
        let lines = wrap(text, WRAP_WIDTH, measure);
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|l| measure(l) < WRAP_WIDTH));
        assert_eq!(lines.join(" "), text);
        // Each line steps down by height + 2, and the box grows to fit.
        let draws = tooltip(text, 300.0, [10.0, 200.0], None);
        let texts: Vec<f32> = draws.iter().filter_map(|d| if let Draw::Text { pos, .. } = d { Some(pos[1]) } else { None }).collect();
        assert_eq!(texts.len(), lines.len());
        assert_eq!(texts[1] - texts[0], 11.0);
        let fill = quads(&draws).last().unwrap().0;
        assert_eq!(fill[3], 11.0 * lines.len() as f32 + 4.0);
        assert_eq!(fill[1] + fill[3], 200.0);
    }

    #[test]
    fn a_long_word_keeps_its_own_line() {
        assert_eq!(wrap("a bbbbbbbbbb c", 20, |s| s.len() as u32 * 5), vec!["a", "bbbbbbbbbb", "c"]);
        assert!(wrap("", 20, |s| s.len() as u32).is_empty());
    }

    #[test]
    fn timer_restarts_on_a_new_control_and_keeps_its_anchor() {
        let a = Help { rect: [0.0, 0.0, 100.0, 20.0], text: "first".into() };
        let b = Help { rect: [0.0, 30.0, 100.0, 20.0], text: "second".into() };
        let mut timer = HelpTimer::default();
        // 900 ms in 100 ms steps: not yet (1 s needed).
        assert!(timer.update(Some((1, &a)), [10.0, 10.0], 0.0, None).is_empty());
        for _ in 0..9 {
            assert!(timer.update(Some((1, &a)), [10.0, 10.0], 100.0, None).is_empty());
        }
        // Past one second the tip shows at the pointer; moving within the control does not move it.
        let first = timer.update(Some((1, &a)), [10.0, 10.0], 200.0, None);
        assert!(!first.is_empty());
        let later = timer.update(Some((1, &a)), [50.0, 15.0], 100.0, None);
        assert_eq!(quads(&later).last().unwrap().0[0], 10.0);
        // Another control (or the same help on another layer) starts over.
        assert!(timer.update(Some((1, &b)), [10.0, 40.0], 100.0, None).is_empty());
        // (The frame that switches restarts the clock and does not count.)
        assert!(timer.update(Some((2, &b)), [10.0, 40.0], 2000.0, None).is_empty());
        assert!(!timer.update(Some((2, &b)), [10.0, 40.0], 2000.0, None).is_empty());
        // Leaving resets.
        assert!(timer.update(None, [10.0, 40.0], 5000.0, None).is_empty());
        assert!(timer.update(Some((2, &b)), [10.0, 40.0], 100.0, None).is_empty());
    }

    #[test]
    fn targets_come_from_helptext_and_the_largest_drawn_rectangle() {
        let lay = nfspu_formats::fe::Lay::parse("[section.0]\nid=REAL.BLACK_640x480\n[section.1]\nid=NFS5.GOTOBUTTON\nhelptext=1007\n[section.2]\nid=NFS5.GOTOBUTTON\nhelptext=0\n");
        let locale = Locale::parse_csv("String ID,ENGLISH_IDS\n1007,Play a quick race\n");
        let blocks: Vec<&Block> = lay.controls().collect();
        let drawn = vec![
            Draw::Text { text: "quick race".into(), pos: [0.0, 0.0], size: 11.0, colour: [1.0; 4], font: "x".into() },
            Draw::Quad { rect: [10.0, 20.0, 1.0, 18.0], colour: [0.0; 4], hover: None, link: None, back: false },
            Draw::Quad { rect: [10.0, 20.0, 150.0, 18.0], colour: [1.0; 4], hover: None, link: None, back: false },
            Draw::Quad { rect: [10.0, 20.0, 150.0, 1.0], colour: [0.0; 4], hover: None, link: None, back: false },
        ];
        let help = target(blocks[0], &drawn, &locale, 0).unwrap();
        assert_eq!(help, Help { rect: [10.0, 20.0, 150.0, 18.0], text: "Play a quick race".into() });
        assert!(target(blocks[1], &drawn, &locale, 0).is_none());
        assert!(target(blocks[0], &[], &locale, 0).is_none());
        assert_eq!(at(&[help.clone()], [20.0, 25.0]), Some(&help));
        assert_eq!(at(&[help], [20.0, 40.0]), None);
    }
}
