//! Lists every front-end control with a help tip: its screen, hover area, text and the tip box
//! retail would draw for it (pointer at the area's centre). No window. Writes `logs/help_audit.txt`.
//!
//! Run: `$env:NFSPU_GAME_DIR="C:\Users\CB\Downloads\NFS5 - Copy"; cargo run -p nfspu-game --example help_audit -j 2`

use nfspu_formats::fe::Locale;
use nfspu_game::menu::{load_font, load_screen, load_templates, Draw};
use nfspu_game::menu_help::{tooltip, FONT};
use std::{env, fs, path::PathBuf};

fn main() {
    let Ok(dir) = env::var("NFSPU_GAME_DIR") else {
        eprintln!("set NFSPU_GAME_DIR to the game folder");
        return;
    };
    let root = PathBuf::from(dir);
    let templates = load_templates(&root);
    let locale = fs::read(root.join("FEData").join("Locale").join("festrings.csv")).map(|b| Locale::parse_csv_bytes(&b)).unwrap_or_default();
    let font = load_font(&root, FONT);
    let mut out = format!("font {FONT}: {}\n", font.as_ref().map(|f| format!("line height {}", f.line_height)).unwrap_or_else(|| "missing".into()));

    let mut names: Vec<String> = fs::read_dir(root.join("FEData").join("Layouts"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            (p.extension()?.to_str()?.eq_ignore_ascii_case("lay")).then(|| p.file_stem()?.to_str().map(str::to_string))?
        })
        .collect();
    names.sort();
    let wanted = env::args().nth(1).map(|w| w.to_ascii_lowercase());

    let (mut screens, mut tips) = (0, 0);
    for name in names.iter().filter(|n| wanted.as_ref().is_none_or(|w| n.to_ascii_lowercase() == *w)) {
        let Some(screen) = load_screen(&root, name, &locale, 0, &templates) else { continue };
        if screen.help.is_empty() {
            continue;
        }
        screens += 1;
        out.push_str(&format!("{name}: {} tip(s)\n", screen.help.len()));
        for help in &screen.help {
            tips += 1;
            let [x, y, w, h] = help.rect;
            let centre = [x + w / 2.0, y + h / 2.0];
            let draws = tooltip(&help.text, 1000.0, centre, font.as_ref());
            let lines = draws.iter().filter(|d| matches!(d, Draw::Text { .. })).count();
            let boxed = draws.iter().rev().find_map(|d| if let Draw::Quad { rect, .. } = d { Some(*rect) } else { None }).unwrap_or_default();
            out.push_str(&format!(
                "  [{x:>5.1},{y:>5.1} {w:>5.1}x{h:<5.1}] box [{:>5.1},{:>5.1} {:>5.1}x{:<4.1}] {lines} line(s)  \"{}\"\n",
                boxed[0], boxed[1], boxed[2], boxed[3], help.text
            ));
        }
    }
    out.push_str(&format!("{tips} tip(s) on {screens} screen(s)\n"));
    print!("{out}");
    let _ = fs::create_dir_all("logs");
    let _ = fs::write("logs/help_audit.txt", out);
}
