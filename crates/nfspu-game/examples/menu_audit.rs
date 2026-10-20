//! Loads every front-end screen and reports what the renderer can draw and what it cannot yet.
//! No window. Writes `logs/menu_audit.txt`.
//!
//! Run: `$env:NFSPU_GAME_DIR="C:\Users\CB\Downloads\NFS5"; cargo run -p nfspu-game --example menu_audit -j 2`

use nfspu_formats::fe::Locale;
use nfspu_game::menu::{load_screen, load_templates, Draw};
use std::{collections::BTreeMap, env, fs, io::Write, path::PathBuf};

fn main() {
    let mut out = String::new();
    let mut say = |line: String| {
        println!("{line}");
        out.push_str(&line);
        out.push('\n');
    };

    let Ok(dir) = env::var("NFSPU_GAME_DIR") else {
        eprintln!("set NFSPU_GAME_DIR to the game folder");
        return;
    };
    let root = PathBuf::from(dir);
    let templates = load_templates(&root);
    let locale = fs::read(root.join("FEData").join("Locale").join("festrings.csv"))
        .map(|b| Locale::parse_csv_bytes(&b))
        .unwrap_or_default();

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

    // Optional single screen: print exactly what would be drawn, so a screen can be checked
    // without opening a window.
    if let Some(wanted) = env::args().nth(1) {
        let Some(screen) = load_screen(&root, &wanted.to_ascii_lowercase(), &locale, 0, &templates) else {
            eprintln!("no screen '{wanted}'");
            return;
        };
        say(format!("{}  {}x{}  background {:.2},{:.2},{:.2}", screen.name, screen.size[0], screen.size[1], screen.background[0], screen.background[1], screen.background[2]));
        for draw in &screen.draw {
            match draw {
                Draw::Quad { rect, colour, hover, link, back } => say(format!(
                    "  quad  [{:>5.1},{:>5.1} {:>5.1}x{:<5.1}] rgba {:.2},{:.2},{:.2},{:.2}{}{}{}",
                    rect[0], rect[1], rect[2], rect[3], colour[0], colour[1], colour[2], colour[3],
                    hover.map(|h| format!("  hover {:.2},{:.2},{:.2}", h[0], h[1], h[2])).unwrap_or_default(),
                    link.as_deref().map(|l| format!("  -> {l}")).unwrap_or_default(),
                    if *back { "  (back)" } else { "" },
                )),
                Draw::Text { text, pos, size, .. } => say(format!("  text  [{:>5.1},{:>5.1}] size {size:<4.0} \"{text}\"", pos[0], pos[1])),
                Draw::Gradient { corners, colours, .. } => say(format!(
                    "  grad  UL {:?} UR {:?} BL {:?} BR {:?}  alpha {:.2}/{:.2}/{:.2}/{:.2}",
                    corners[0], corners[1], corners[2], corners[3], colours[0][3], colours[1][3], colours[2][3], colours[3][3],
                )),
                Draw::Picture { file, index, pos, flip, .. } => say(format!("  pic   [{:>5.1},{:>5.1}] {file} #{index} flip {flip}", pos[0], pos[1])),
                Draw::Anim { file, idle, hover, down, pos, .. } => say(format!(
                    "  anim  [{:>5.1},{:>5.1}] {file} idle {}..{}/{} hover {}..{}/{} down {}..{}/{}",
                    pos[0], pos[1], idle.start, idle.end, idle.ticks, hover.start, hover.end, hover.ticks, down.start, down.end, down.ticks,
                )),
                Draw::Movie { rect, source, stencil } => say(format!("  movie [{:>5.1},{:>5.1} {:>5.1}x{:<5.1}] {source} ({} stencil pieces)", rect[0], rect[1], rect[2], rect[3], stencil.len())),
                Draw::Shown { when, .. } => say(format!("  shown {when:?}")),
                Draw::Clipped { draws, .. } => say(format!("  clipped group of {}", draws.len())),
                Draw::Target { rect, hit } => say(format!("  hit   [{:>5.1},{:>5.1} {:>5.1}x{:<5.1}] {hit:?}", rect[0], rect[1], rect[2], rect[3])),
            }
        }
        for id in &screen.skipped {
            say(format!("  skipped: {id}"));
        }
        return;
    }

    say(format!("templates: {}, locale strings: {}", templates.len(), locale.len()));
    say(format!("screens: {}", names.len()));
    say(String::new());
    say(format!("  {:<24} {:>6} {:>6} {:>6} {:>7}  {}", "screen", "quads", "texts", "links", "skipped", "background"));

    let mut skipped: BTreeMap<String, usize> = BTreeMap::new();
    let (mut total_draw, mut total_skip, mut missing) = (0usize, 0usize, 0usize);
    for name in &names {
        let Some(screen) = load_screen(&root, name, &locale, 0, &templates) else {
            missing += 1;
            say(format!("  {name:<24}  COULD NOT LOAD"));
            continue;
        };
        let quads = screen.draw.iter().filter(|d| matches!(d, Draw::Quad { .. })).count();
        let texts = screen.draw.iter().filter(|d| matches!(d, Draw::Text { .. })).count();
        let links = screen.draw.iter().filter(|d| matches!(d, Draw::Quad { link: Some(_), .. })).count();
        for s in &screen.skipped {
            *skipped.entry(s.clone()).or_default() += 1;
        }
        total_draw += screen.draw.len();
        total_skip += screen.skipped.len();
        say(format!(
            "  {:<24} {:>6} {:>6} {:>6} {:>7}  {:.2},{:.2},{:.2}",
            name, quads, texts, links, screen.skipped.len(), screen.background[0], screen.background[1], screen.background[2]
        ));
    }

    say(String::new());
    say(format!("draw commands: {total_draw}, controls not drawn yet: {total_skip}, screens that failed to load: {missing}"));
    say(String::new());
    say("Control types not drawn yet (count of screens using them)".into());
    let mut types: Vec<(&String, &usize)> = skipped.iter().collect();
    types.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (id, n) in types.iter().take(40) {
        say(format!("  {:<30} {n:>4}", id));
    }

    let _ = fs::create_dir_all("logs");
    match fs::File::create("logs/menu_audit.txt").and_then(|mut f| f.write_all(out.as_bytes())) {
        Ok(()) => println!("\nwrote logs/menu_audit.txt"),
        Err(e) => eprintln!("could not write logs/menu_audit.txt: {e}"),
    }
}
