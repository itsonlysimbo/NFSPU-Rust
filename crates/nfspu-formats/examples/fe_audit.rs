//! Audits the front end's data in the player's install: every `.lay` screen, every `.ctr`/`.scr`
//! control template and the locale table, and checks that they line up. Console output only, plus
//! `logs/fe_audit.txt`.
//!
//! Run: `$env:NFSPU_GAME_DIR="C:\Users\CB\Downloads\NFS5"; cargo run -p nfspu-formats --example fe_audit -j 2`

use nfspu_formats::fe::{decode_latin1, parse_symbol_constants, ControlTemplate, Lay, Locale};
use std::{collections::BTreeMap, env, fs, io::Write, path::Path, path::PathBuf};

/// The front end's text files are ISO-8859-1, so read them as bytes and decode one-to-one.
fn read_text(path: &Path) -> Option<String> {
    fs::read(path).ok().map(|b| decode_latin1(&b))
}

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
    let layouts = root.join("FEData").join("Layouts");
    let controls = root.join("FEData").join("Controls");
    let locale_dir = root.join("FEData").join("Locale");

    // ---- screens ----
    let mut screens: Vec<(String, Lay)> = Vec::new();
    for entry in fs::read_dir(&layouts).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() != Some("lay") {
            continue;
        }
        let Some(text) = read_text(&path) else { continue };
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        screens.push((name, Lay::parse(&text)));
    }
    screens.sort_by(|a, b| a.0.cmp(&b.0));

    // ---- control templates (.ctr and .scr describe the same thing) ----
    let mut templates: BTreeMap<String, String> = BTreeMap::new();
    let (mut ctr_files, mut scr_files, mut empty_files, mut bad_files) = (0, 0, 0, 0);
    for entry in fs::read_dir(&controls).into_iter().flatten().flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
        let is_ctr = ext.as_deref() == Some("ctr");
        let is_scr = ext.as_deref() == Some("scr");
        if !is_ctr && !is_scr {
            continue;
        }
        if is_ctr {
            ctr_files += 1;
        } else {
            scr_files += 1;
        }
        // A few shipped files are deliberately emptied placeholders (one even says so in a comment),
        // so an empty or comment-only file is a normal finding, not a parse failure.
        let text = read_text(&path).unwrap_or_default();
        if text.lines().all(|l| l.trim().is_empty() || l.trim_start().starts_with(';') || l.trim_start().starts_with('#')) {
            empty_files += 1;
            continue;
        }
        match ControlTemplate::parse(&text) {
            Some(t) => {
                templates.insert(t.id, t.description);
            }
            None => bad_files += 1,
        }
    }

    // ---- locale ----
    let locale = fs::read(locale_dir.join("festrings.csv")).map(|b| Locale::parse_csv_bytes(&b)).unwrap_or_default();
    let symbols = {
        let mut s = String::new();
        for f in ["festrings.h", "hudstrings.h"] {
            s.push_str(&read_text(&locale_dir.join(f)).unwrap_or_default());
        }
        parse_symbol_constants(&s)
    };

    // ---- cross-checks ----
    let mut used: BTreeMap<&str, usize> = BTreeMap::new();
    let mut blocks = 0usize;
    let (mut string_refs, mut string_ok) = (0usize, 0usize);
    let mut unresolved: Vec<u32> = Vec::new();
    for (_, lay) in &screens {
        blocks += lay.blocks.len();
        for b in &lay.blocks {
            if let Some(sid) = b.string_id() {
                string_refs += 1;
                if locale.text(sid, 0).is_some() {
                    string_ok += 1;
                } else if !unresolved.contains(&sid) {
                    unresolved.push(sid);
                }
            }
            if !b.is_screen_header() {
                if let Some(id) = b.control() {
                    *used.entry(id).or_default() += 1;
                }
            }
        }
    }
    let undefined: Vec<&&str> = used.keys().filter(|id| !templates.contains_key(**id)).collect();
    let unused: Vec<&String> = templates.keys().filter(|id| !used.contains_key(id.as_str())).collect();
    let locale_symbols: Vec<&str> = locale.strings.iter().map(|s| s.symbol.as_str()).filter(|s| !s.is_empty()).collect();
    let header_symbols: Vec<&str> = symbols.iter().map(|(n, _)| n.as_str()).collect();
    let symbols_in_header = locale_symbols.iter().filter(|s| header_symbols.contains(*s)).count();

    say("Front-end data audit (FEData)".into());
    say(format!(
        "  layouts      : {} file(s), {} blocks, {} control placements",
        screens.len(),
        blocks,
        used.values().sum::<usize>()
    ));
    say(format!(
        "  control types: {} distinct ids in {} .ctr + {} .scr ({} empty placeholder(s), {} unparseable)",
        templates.len(),
        ctr_files,
        scr_files,
        empty_files,
        bad_files
    ));
    say(format!(
        "  cross-check  : {} used id(s) with no template, {} template(s) never used by a layout",
        undefined.len(),
        unused.len()
    ));
    say(format!(
        "  locale       : {} string(s), {} symbol(s); layout stringID refs {}/{} resolved",
        locale.len(),
        locale_symbols.len(),
        string_ok,
        string_refs
    ));
    say(format!(
        "  locale names : {}/{} symbols appear in the generated headers (festrings.h/hudstrings.h)",
        symbols_in_header,
        locale_symbols.len()
    ));
    if !undefined.is_empty() {
        say(format!("  UNDEFINED IDS: {}", undefined.iter().map(|s| **s).collect::<Vec<_>>().join(", ")));
    }
    if !unresolved.is_empty() {
        let mut list = unresolved.clone();
        list.sort_unstable();
        say(format!("  UNRESOLVED STRING IDS ({}): {:?}", list.len(), list));
    }

    say(String::new());
    say("Screens".into());
    say(format!("  {:<28} {:<24} {:>8}  first string", "file", "screen id", "controls"));
    for (name, lay) in &screens {
        let screen_id = lay.screen().and_then(|b| b.control()).unwrap_or("-");
        let first = lay
            .blocks
            .iter()
            .filter_map(|b| b.string_id())
            .find_map(|sid| locale.text(sid, 0))
            .map(|t| t.split_whitespace().take(4).collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        say(format!("  {:<28} {:<24} {:>8}  {}", name, screen_id, lay.controls().count(), first));
    }

    say(String::new());
    say("Most used control types".into());
    let mut by_use: Vec<(&str, usize)> = used.iter().map(|(k, v)| (*k, *v)).collect();
    by_use.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    for (id, n) in by_use.iter().take(10) {
        let desc = templates.get(*id).map(String::as_str).unwrap_or("?");
        say(format!("  {:<28} {n:>4}  {desc}", id));
    }

    if !unused.is_empty() {
        say(String::new());
        say(format!("Templates not used by any shipped layout ({})", unused.len()));
        for id in &unused {
            say(format!("  {:<28} {}", id, templates.get(*id).map(String::as_str).unwrap_or("?")));
        }
    }

    let _ = fs::create_dir_all("logs");
    match fs::File::create("logs/fe_audit.txt").and_then(|mut f| f.write_all(out.as_bytes())) {
        Ok(()) => println!("\nwrote logs/fe_audit.txt"),
        Err(e) => eprintln!("could not write logs/fe_audit.txt: {e}"),
    }
}
