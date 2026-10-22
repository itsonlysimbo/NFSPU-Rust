//! The race summary shown after the player finishes: a 500x325 panel with a table of the eight
//! places. Retail has no front-end layout for it; it is the in-race `HRaceSummary` over `HTable`
//! (`docs/provenance/hud.md`, "Race summary"). The column widths, row height, row count, header
//! strings and the player-row highlight come from the executable; the panel's own colours, its
//! screen position, the title position and the opening speed are guesses.

use crate::hud::{word, Align, HudDraw, HudSettings, Rect, AMBER, WHITE};

/// One table row, in finishing order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SummaryRow {
    pub place: usize,
    pub name: String,
    pub car: String,
    pub best_lap: Option<f32>,
    /// Total time, `None` while the car is still racing.
    pub time: Option<f32>,
    pub is_player: bool,
    /// Body colour of the car (the bar in the CAR column).
    pub colour: [u8; 3],
}

const PANEL: [f32; 2] = [500.0, 325.0];
const ROW: f32 = 20.0;
const ROWS: usize = 8;
/// Retail's column widths for a lap race: place, name, car, best lap, time.
pub const WIDTHS: [f32; 5] = [30.0, 90.0, 140.0, 100.0, 80.0];
/// Seconds the panel takes to open to full height (guess; retail grows it a fifth per tick).
const OPEN_SECS: f32 = 0.4;

/// The ordinal text for a place (`IDS_1ST`..`IDS_8TH`).
pub fn ordinal(place: usize) -> &'static str {
    ["1st", "2nd", "3rd", "4th", "5th", "6th", "7th", "8th"].get(place.wrapping_sub(1)).copied().unwrap_or("")
}

/// Retail's text for a car that has no finish time yet.
pub const NO_TIME: &str = "--------------";

/// Cuts text to at most `n` characters, as retail's fixed-size name buffers do.
fn clip(text: &str, n: usize) -> String {
    text.chars().take(n).collect()
}

/// Draws the summary panel; `since` is the seconds since it appeared. `laps` says whether the
/// race has laps (retail adds the BEST LAP column only then). Look checked against retail footage
/// (2026-10-22): gold translucent panel, black title and footer bars, black table outline and
/// grid, amber header and player row, the car's colour as a bar in the CAR cell.
pub fn summary(out: &mut Vec<HudDraw>, settings: &HudSettings, rows: &[SummaryRow], since: f32, screen: [u32; 2], laps: bool) {
    let s = settings.scale;
    let (cx, cy) = (screen[0] as f32 / 2.0, screen[1] as f32 / 2.0);
    let open = (since / OPEN_SECS).clamp(0.0, 1.0);
    let full = PANEL[1] * s;
    let h = (40.0 * s + (full - 40.0 * s) * open).min(full);
    let panel: Rect = [cx - PANEL[0] * s / 2.0, cy - h / 2.0, PANEL[0] * s, h];
    let amber = [AMBER[0], AMBER[1], AMBER[2], 1.0];
    out.push(HudDraw::Fill { rect: panel, colour: [0.98, 0.76, 0.3, 0.6] });
    let bar = 24.0 * s;
    out.push(HudDraw::Gradient { rect: [panel[0], panel[1], panel[2], bar], top: [0.0, 0.0, 0.0, 0.9], bottom: [0.0, 0.0, 0.0, 1.0] });
    out.push(HudDraw::Gradient { rect: [panel[0], panel[1] + panel[3] - bar, panel[2], bar], top: [0.0, 0.0, 0.0, 1.0], bottom: [0.0, 0.0, 0.0, 0.9] });
    if open < 1.0 {
        return;
    }

    let font = if settings.layout.fonts[1].is_empty() { settings.layout.fonts[0].clone() } else { settings.layout.fonts[1].clone() };
    let bar_text = |out: &mut Vec<HudDraw>, text: String, x: f32, y: f32, align: Align| out.push(HudDraw::Text { font: font.clone(), text, pos: [x, y], colour: WHITE, align, scale: s });
    let pad = 12.0 * s;
    bar_text(out, settings.word(word::SINGLE_RACE), panel[0] + pad, panel[1] + 5.0 * s, Align::Left);
    bar_text(out, settings.word(word::RACE_SUMMARY), panel[0] + panel[2] - pad, panel[1] + 5.0 * s, Align::Right);
    bar_text(out, settings.word(word::ESC_TO_EXIT), panel[0] + pad, panel[1] + panel[3] - bar + 5.0 * s, Align::Left);
    bar_text(out, settings.word(word::SPACE_TO_PROCEED), panel[0] + panel[2] - pad, panel[1] + panel[3] - bar + 5.0 * s, Align::Right);

    // The table: centred in the panel, a header row and eight body rows.
    let widths: Vec<f32> = if laps { WIDTHS.to_vec() } else { vec![WIDTHS[0], WIDTHS[1], WIDTHS[2], WIDTHS[4]] };
    let width: f32 = widths.iter().sum::<f32>() * s;
    let left = panel[0] + (panel[2] - width) / 2.0;
    let top = panel[1] + ((PANEL[1] - (ROWS as f32 + 1.0) * ROW) / 2.0).floor() * s;
    let height = ROW * s * (ROWS as f32 + 1.0);
    out.push(HudDraw::Fill { rect: [left, top, width, ROW * s], colour: amber });
    if let Some(p) = rows.iter().position(|r| r.is_player) {
        out.push(HudDraw::Fill { rect: [left, top + ROW * s * (p as f32 + 1.0), width, ROW * s], colour: [AMBER[0], AMBER[1], AMBER[2], 0.8] });
    }
    let mut x = left;
    let starts: Vec<f32> = widths
        .iter()
        .map(|w| {
            let at = x;
            x += w * s;
            at
        })
        .collect();
    let black = [0.0, 0.0, 0.0, 1.0];
    let mut heads = vec![String::new(), settings.word(word::NAME), settings.word(word::CAR)];
    if laps {
        heads.push(settings.word(word::BEST_LAP_CAPS));
    }
    heads.push(settings.word(word::TOTAL_TIME));
    let text_y = |row: usize| top + ROW * s * row as f32 + 3.0 * s;
    for (i, head) in heads.into_iter().enumerate() {
        out.push(HudDraw::Text { font: font.clone(), text: head, pos: [starts[i] + 4.0 * s, text_y(0)], colour: black, align: Align::Left, scale: s });
    }
    for (i, r) in rows.iter().take(ROWS).enumerate() {
        let mut cells = vec![ordinal(r.place).to_string(), clip(&r.name, 15), clip(&r.car, 15)];
        if laps {
            cells.push(r.best_lap.map_or_else(|| NO_TIME.to_string(), crate::hud::format_time));
        }
        cells.push(r.time.map_or_else(|| NO_TIME.to_string(), crate::hud::format_time));
        for (c, text) in cells.into_iter().enumerate() {
            let dx = if c == 2 { 26.0 * s } else { 4.0 * s };
            if c == 2 {
                let [cr, cg, cb] = r.colour;
                out.push(HudDraw::Fill { rect: [starts[c] + 4.0 * s, text_y(i + 1) + 4.0 * s, 18.0 * s, 5.0 * s], colour: [cr as f32 / 255.0, cg as f32 / 255.0, cb as f32 / 255.0, 1.0] });
            }
            out.push(HudDraw::Clip { rect: [starts[c], text_y(i + 1) - 3.0 * s, widths[c] * s, ROW * s], draws: vec![HudDraw::Text { font: font.clone(), text, pos: [starts[c] + dx, text_y(i + 1)], colour: black, align: Align::Left, scale: s }] });
        }
    }
    // outline and grid, black
    let line = |out: &mut Vec<HudDraw>, from: [f32; 2], to: [f32; 2]| out.push(HudDraw::Line { from, to, colour: black, width: s.max(1.0) });
    for st in starts.iter().chain(std::iter::once(&(left + width))) {
        line(out, [*st, top], [*st, top + height]);
    }
    line(out, [left, top], [left + width, top]);
    line(out, [left, top + height], [left + width, top + height]);
    line(out, [left, top + ROW * s], [left + width, top + ROW * s]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_440_wide_and_ordinals_read_1st_to_8th() {
        assert_eq!(WIDTHS.iter().sum::<f32>(), 440.0);
        assert_eq!((ordinal(1), ordinal(3), ordinal(8), ordinal(9)), ("1st", "3rd", "8th", ""));
        assert_eq!(clip("Machine Gun Kelly Jr", 15).len(), 15);
    }
}
