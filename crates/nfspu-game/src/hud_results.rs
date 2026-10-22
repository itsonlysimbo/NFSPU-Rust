//! The race summary shown after the player finishes: a 500x325 panel with a table of the eight
//! places. Retail has no front-end layout for it; it is the in-race `HRaceSummary` over `HTable`
//! (`docs/provenance/hud.md`, "Race summary"). The column widths, row height, row count, header
//! strings and the player-row highlight come from the executable; the panel's own colours, its
//! screen position, the title position and the opening speed are guesses.

use crate::hud::{argb, word, Align, HudDraw, HudSettings, Rect, AMBER, WHITE};

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

/// Draws the summary panel; `since` is the seconds since it appeared.
pub fn summary(out: &mut Vec<HudDraw>, settings: &HudSettings, rows: &[SummaryRow], since: f32, screen: [u32; 2]) {
    let s = settings.scale;
    // Retail's screen is 640x480 scaled by `s`; the panel sits in its centre (guess).
    let (cx, cy) = (screen[0] as f32 / 2.0, screen[1] as f32 / 2.0);
    let open = (since / OPEN_SECS).clamp(0.0, 1.0);
    let full = PANEL[1] * s;
    let h = (40.0 * s + (full - 40.0 * s) * open).min(full);
    let panel: Rect = [cx - PANEL[0] * s / 2.0, cy - h / 2.0, PANEL[0] * s, h];
    out.push(HudDraw::Gradient { rect: panel, top: [0.0, 0.0, 0.0, 0.85], bottom: [0.0, 0.0, 0.0, 0.6] });
    let amber = [AMBER[0], AMBER[1], AMBER[2], 1.0];
    out.push(HudDraw::Fill { rect: [panel[0], panel[1], panel[2], s], colour: amber });
    out.push(HudDraw::Fill { rect: [panel[0], panel[1] + panel[3] - s, panel[2], s], colour: amber });
    if open < 1.0 {
        return;
    }

    let font = if settings.layout.fonts[1].is_empty() { settings.layout.fonts[0].clone() } else { settings.layout.fonts[1].clone() };
    let title_font = settings.layout.fonts[0].clone();
    out.push(HudDraw::Text { font: title_font, text: settings.word(word::RACE_SUMMARY), pos: [cx, panel[1] + 12.0 * s], colour: amber, align: Align::Centre, scale: s });

    // The table: centred in the panel, a header row and eight body rows.
    let width: f32 = WIDTHS.iter().sum::<f32>() * s;
    let left = panel[0] + (panel[2] - width) / 2.0;
    let top = panel[1] + ((PANEL[1] - (ROWS as f32 + 1.0) * ROW) / 2.0).floor() * s;
    out.push(HudDraw::Fill { rect: [left, top, width, ROW * s], colour: [AMBER[0], AMBER[1], AMBER[2], 1.0] });
    out.push(HudDraw::Fill { rect: [left, top + ROW * s, width, ROW * s * ROWS as f32], colour: [1.0, 1.0, 1.0, 64.0 / 255.0] });
    if let Some(p) = rows.iter().position(|r| r.is_player) {
        out.push(HudDraw::Fill { rect: [left, top + ROW * s * (p as f32 + 1.0), width, ROW * s], colour: [AMBER[0], AMBER[1], AMBER[2], 0.5] });
    }
    // column starts
    let mut x = left;
    let starts: Vec<f32> = WIDTHS
        .iter()
        .map(|w| {
            let at = x;
            x += w * s;
            at
        })
        .collect();
    let black = [0.0, 0.0, 0.0, 1.0];
    let heads = ["".to_string(), settings.word(word::NAME), settings.word(word::CAR), settings.word(word::BEST_LAP_CAPS), settings.word(word::TOTAL_TIME)];
    let text_y = |row: usize| top + ROW * s * row as f32 + 3.0 * s;
    for (i, head) in heads.into_iter().enumerate() {
        out.push(HudDraw::Text { font: font.clone(), text: head, pos: [starts[i] + 4.0 * s, text_y(0)], colour: black, align: Align::Left, scale: s });
    }
    for (i, r) in rows.iter().take(ROWS).enumerate() {
        let cells = [
            ordinal(r.place).to_string(),
            clip(&r.name, 15),
            clip(&r.car, 15),
            r.best_lap.map_or_else(|| NO_TIME.to_string(), crate::hud::format_time),
            r.time.map_or_else(|| NO_TIME.to_string(), crate::hud::format_time),
        ];
        for (c, text) in cells.into_iter().enumerate() {
            let colour = if r.is_player { WHITE } else { [0.9, 0.9, 0.9, 1.0] };
            let clipped = HudDraw::Clip { rect: [starts[c], text_y(i + 1) - 3.0 * s, WIDTHS[c] * s, ROW * s], draws: vec![HudDraw::Text { font: font.clone(), text, pos: [starts[c] + 4.0 * s, text_y(i + 1)], colour, align: Align::Left, scale: s }] };
            out.push(clipped);
        }
    }
    // column dividers
    for st in starts.iter().skip(1) {
        out.push(HudDraw::Line { from: [*st, top], to: [*st, top + ROW * s * (ROWS as f32 + 1.0)], colour: [1.0, 1.0, 1.0, 0.25], width: s });
    }
    // the prompt
    let prompt = format!("{}   {}", settings.word(word::SPACE_TO_PROCEED), settings.word(word::ESC_TO_EXIT));
    out.push(HudDraw::Text { font, text: prompt, pos: [cx, panel[1] + panel[3] - 22.0 * s], colour: argb(settings.layout.label), align: Align::Centre, scale: s });
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
