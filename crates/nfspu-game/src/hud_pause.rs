//! The in-race pause menu: a 500x325 panel titled "Pause Menu" with three buttons, continue,
//! race again and quit. Retail has no front-end layout for it; it is the in-race `HPause` billboard
//! (`docs/provenance/race.md`, "Pause"). From the executable: the panel size, the title and
//! button words, the button size (142x19), their column (centred) and rows (the middle one on the
//! screen's centre, 1.6 button heights apart). Guesses: the panel's colours (the race summary's),
//! the button look and highlight, and the quit confirmation page (retail's strings, our layout).

use crate::hud::{word, Align, HudDraw, HudSettings, Rect, AMBER, WHITE};

/// Which page of the pause menu is up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PausePage {
    /// continue / race again / quit...
    #[default]
    Main,
    /// "Are you sure you want to exit?": exit to game menu / exit to windows / cancel.
    Quit,
}

/// What a button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseAction {
    Continue,
    RaceAgain,
    Quit,
    ExitToMenu,
    ExitToWindows,
    Cancel,
}

/// What the HUD draws while the race is paused.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PauseView {
    pub page: PausePage,
    /// Highlighted button, 0..3.
    pub selected: usize,
}

const PANEL: [f32; 2] = [500.0, 325.0];
/// Retail's button size.
pub const BUTTON: [f32; 2] = [142.0, 19.0];
/// Row step in button heights (retail 1.6).
const STEP: f32 = 1.6;

impl PausePage {
    /// The page's buttons, top to bottom.
    pub fn actions(self) -> [PauseAction; 3] {
        match self {
            PausePage::Main => [PauseAction::Continue, PauseAction::RaceAgain, PauseAction::Quit],
            PausePage::Quit => [PauseAction::ExitToMenu, PauseAction::ExitToWindows, PauseAction::Cancel],
        }
    }
}

fn label(action: PauseAction) -> u16 {
    match action {
        PauseAction::Continue => word::CONTINUE,
        PauseAction::RaceAgain => word::RACE_AGAIN,
        PauseAction::Quit => word::QUIT,
        PauseAction::ExitToMenu => word::EXIT_FRONTEND,
        PauseAction::ExitToWindows => word::EXIT_WINDOWS,
        PauseAction::Cancel => word::CANCEL,
    }
}

/// The three button rectangles on a `screen`-sized window at HUD scale `s`.
pub fn buttons(screen: [u32; 2], s: f32) -> [Rect; 3] {
    let (cx, cy) = (screen[0] as f32 / 2.0, screen[1] as f32 / 2.0);
    let (w, h) = (BUTTON[0] * s, BUTTON[1] * s);
    let step = h * STEP;
    let top = cy - step * 3.0 * 0.5;
    [0, 1, 2].map(|i| [(cx - w / 2.0).floor(), (top + step * i as f32).floor(), w, h])
}

/// The button under a window point, if any.
pub fn hit(screen: [u32; 2], s: f32, at: [f32; 2]) -> Option<usize> {
    buttons(screen, s).iter().position(|r| at[0] >= r[0] && at[0] < r[0] + r[2] && at[1] >= r[1] && at[1] < r[1] + r[3])
}

/// Draws the pause panel over whatever is already in `out`.
pub fn draw(out: &mut Vec<HudDraw>, settings: &HudSettings, view: &PauseView, screen: [u32; 2]) {
    let s = settings.scale;
    let (cx, cy) = (screen[0] as f32 / 2.0, screen[1] as f32 / 2.0);
    let panel: Rect = [cx - PANEL[0] * s / 2.0, cy - PANEL[1] * s / 2.0, PANEL[0] * s, PANEL[1] * s];
    // Footage (2026-10-23): a pale, see-through panel, yellow at the top fading to grey-teal.
    out.push(HudDraw::Gradient { rect: panel, top: [0.95, 0.84, 0.45, 0.62], bottom: [0.66, 0.80, 0.80, 0.62] });
    let bar = 24.0 * s;
    out.push(HudDraw::Gradient { rect: [panel[0], panel[1], panel[2], bar], top: [0.0, 0.0, 0.0, 0.9], bottom: [0.0, 0.0, 0.0, 1.0] });
    // Footage: the bottom strip is thinner than the title bar (about 5% vs 8% of the panel).
    let foot = 16.0 * s;
    out.push(HudDraw::Gradient { rect: [panel[0], panel[1] + panel[3] - foot, panel[2], foot], top: [0.0, 0.0, 0.0, 1.0], bottom: [0.0, 0.0, 0.0, 0.9] });

    let font = if settings.layout.fonts[1].is_empty() { settings.layout.fonts[0].clone() } else { settings.layout.fonts[1].clone() };
    let pad = 12.0 * s;
    let title = match view.page {
        PausePage::Main => word::PAUSE_MENU,
        PausePage::Quit => word::CONFIRMATION,
    };
    out.push(HudDraw::Text { font: font.clone(), text: settings.word(title), pos: [panel[0] + panel[2] - 4.0 * s, panel[1] + 5.0 * s], colour: WHITE, align: Align::Right, scale: s });
    let rects = buttons(screen, s);
    if view.page == PausePage::Quit {
        let prompt_y = rects[0][1] - 30.0 * s;
        out.push(HudDraw::Text { font: font.clone(), text: settings.word(word::EXIT_PROMPT), pos: [cx, prompt_y], colour: WHITE, align: Align::Centre, scale: s });
    }
    for (i, (rect, action)) in rects.iter().zip(view.page.actions()).enumerate() {
        let on = i == view.selected;
        // Retail's red button: black outline, red fill, light gloss on top; yellow when hovered.
        let fill = if on { [AMBER[0], AMBER[1], AMBER[2], 1.0] } else { [0.74, 0.0, 0.0, 1.0] };
        out.push(HudDraw::Fill { rect: *rect, colour: [0.0, 0.0, 0.0, 1.0] });
        let inner = [rect[0] + s, rect[1] + s, rect[2] - 2.0 * s, rect[3] - 2.0 * s];
        out.push(HudDraw::Fill { rect: inner, colour: fill });
        if !on {
            out.push(HudDraw::Gradient { rect: [inner[0], inner[1], inner[2], inner[3] / 2.0], top: [1.0, 1.0, 1.0, 0.3], bottom: [1.0, 1.0, 1.0, 0.0] });
        }
        let text_colour = WHITE;
        out.push(HudDraw::Text { font: font.clone(), text: settings.word(label(action)), pos: [rect[0] + rect[2] / 2.0, rect[1] + 2.0 * s], colour: text_colour, align: Align::Centre, scale: s });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_sit_on_retail_rows() {
        let b = buttons([640, 480], 1.0);
        assert_eq!(b[0], [249.0, 194.0, 142.0, 19.0]);
        assert_eq!(b[1][1], 224.0);
        assert_eq!(b[2][1], 255.0);
        assert_eq!(hit([640, 480], 1.0, [320.0, 230.0]), Some(1));
        assert_eq!(hit([640, 480], 1.0, [100.0, 230.0]), None);
    }

    #[test]
    fn pages_list_their_actions() {
        assert_eq!(PausePage::Main.actions()[1], PauseAction::RaceAgain);
        assert_eq!(PausePage::Quit.actions()[2], PauseAction::Cancel);
    }
}
