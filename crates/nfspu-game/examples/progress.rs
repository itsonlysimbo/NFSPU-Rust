//! Rewrite progress as a treemap: `cargo run -p nfspu-game --example progress`.
//!
//! Three panels, each a squarified treemap of groups with one cell per item (green = done):
//! - **Retail functions**: every function in the decomp's function list, grouped by class family
//!   (or by address block for unnamed ones). A function counts as covered when our code or notes
//!   cite an address inside it (`0x45A8E0`, `FUN_0045a8e0`, `0045a8e0`), i.e. it was studied and
//!   its behaviour recreated or written down.
//! - **Game files**: every file in the install, grouped by folder. A file counts when our code
//!   names its extension, i.e. we have a reader for that format.
//! - **Features**: the hand-kept table in `docs/progress.md`, ten cells per feature filled by its
//!   percent.
//!
//! Inputs are read only. Writes `logs/progress/progress.svg` and `progress.json`, and prints one
//! line per panel. `NFSPU_GAME_DIR` picks the install; `NFSPU_DECOMP_DIR` the decomp folder.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

const PANEL_W: f64 = 495.0;
const PANEL_H: f64 = 280.0;
const HEADER_H: f64 = 30.0;
const GAP: f64 = 10.0;
const DONE: &str = "#2ea043";
const LEFT: &str = "#6e7681";
const BG: &str = "#0d1117";
const FEATURE_CELLS: usize = 10;

struct Group {
    name: String,
    done: usize,
    total: usize,
    note: String,
}

struct Panel {
    title: String,
    groups: Vec<Group>,
}

impl Panel {
    fn totals(&self) -> (usize, usize) {
        self.groups.iter().fold((0, 0), |(d, t), g| (d + g.done, t + g.total))
    }
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let game = std::env::var("NFSPU_GAME_DIR").unwrap_or_else(|_| r"C:\Users\CB\Downloads\NFS5 - Copy".into());
    let decomp = std::env::var("NFSPU_DECOMP_DIR").unwrap_or_else(|_| r"C:\Users\CB\Desktop\NFS5 Decomp".into());

    let mut sources = Vec::new();
    for dir in ["crates", "docs"] {
        collect(&root.join(dir), &mut |p| {
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext == "rs" || ext == "md" {
                if let Ok(s) = fs::read_to_string(p) {
                    sources.push((p.to_path_buf(), s));
                }
            }
        });
    }

    let mut panels = Vec::new();
    match functions_panel(&Path::new(&decomp).join("decomp/functions.csv"), &sources) {
        Some(p) => panels.push(p),
        None => eprintln!("functions: no {decomp}\\decomp\\functions.csv, panel skipped"),
    }
    match files_panel(Path::new(&game), &sources) {
        Some(p) => panels.push(p),
        None => eprintln!("files: no install at {game}, panel skipped"),
    }
    match features_panel(&root.join("docs/progress.md")) {
        Some(p) => panels.push(p),
        None => eprintln!("features: docs/progress.md missing or empty, panel skipped"),
    }

    let out = root.join("logs/progress");
    fs::create_dir_all(&out).expect("create logs/progress");
    fs::write(out.join("progress.svg"), svg(&panels)).expect("write svg");
    fs::write(out.join("progress.json"), json(&panels)).expect("write json");
    for p in &panels {
        let (d, t) = p.totals();
        println!("{:<18} {d}/{t} ({:.2}%)", p.title.to_lowercase(), pct(d, t));
    }
    println!("wrote {}", out.join("progress.svg").display());
}

fn pct(done: usize, total: usize) -> f64 {
    if total == 0 { 0.0 } else { 100.0 * done as f64 / total as f64 }
}

fn collect(dir: &Path, f: &mut dyn FnMut(&Path)) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            collect(&p, f);
        } else {
            f(&p);
        }
    }
}

/// Every executable address our text cites: `0x45A8E0`, `FUN_0045a8e0`, or a bare `0045a8e0`.
fn cited_addresses(sources: &[(PathBuf, String)]) -> BTreeSet<u32> {
    let mut out = BTreeSet::new();
    for (_, text) in sources {
        let b = text.as_bytes();
        let mut i = 0;
        while i < b.len() {
            let start = i;
            let prefixed = if b[i..].starts_with(b"0x") || b[i..].starts_with(b"0X") {
                i += 2;
                true
            } else if b[i..].starts_with(b"FUN_") {
                i += 4;
                true
            } else {
                false
            };
            let digits_at = i;
            while i < b.len() && b[i].is_ascii_hexdigit() {
                i += 1;
            }
            let digits = std::str::from_utf8(&b[digits_at..i]).unwrap_or("");
            let boundary_before = start == 0 || !b[start - 1].is_ascii_alphanumeric();
            let boundary_after = i >= b.len() || !b[i].is_ascii_alphanumeric();
            let ok = boundary_before && boundary_after && (prefixed || digits.len() == 8) && (6..=8).contains(&digits.len());
            if ok {
                if let Ok(v) = u32::from_str_radix(digits, 16) {
                    if (0x0040_0000..0x0080_0000).contains(&v) {
                        out.insert(v);
                    }
                }
            }
            if i == start {
                i += 1;
            }
        }
    }
    out
}

/// Split one CSV line, honouring quoted fields.
fn csv_fields(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// The family a class belongs to: its leading capitals (`FEBottomBarMenu` -> `FE`, `R3DCar` ->
/// `R3D`), or the whole name when that is a single letter.
fn family(class: &str) -> String {
    let b: Vec<char> = class.chars().collect();
    let mut n = 0;
    while n < b.len() && (b[n].is_ascii_uppercase() || b[n].is_ascii_digit()) {
        n += 1;
    }
    // `FEBottom`: the last capital starts the next word.
    if n < b.len() && n > 1 {
        n -= 1;
    }
    if n <= 1 { class.split('_').next().unwrap_or(class).to_string() } else { b[..n].iter().collect() }
}

fn functions_panel(csv: &Path, sources: &[(PathBuf, String)]) -> Option<Panel> {
    let text = fs::read_to_string(csv).ok()?;
    let cited = cited_addresses(sources);
    let mut groups: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut lines = text.lines();
    let header = csv_fields(lines.next()?);
    let col = |name: &str| header.iter().position(|h| h == name);
    let (a, ns, size, thunk, ext) = (col("address")?, col("namespace")?, col("size")?, col("is_thunk")?, col("is_external")?);
    for line in lines {
        let f = csv_fields(line);
        if f.len() < header.len() || f[thunk] == "true" || f[ext] == "true" {
            continue;
        }
        let Ok(start) = u32::from_str_radix(&f[a], 16) else { continue };
        let len = f[size].parse::<u32>().unwrap_or(1).max(1);
        let group = if f[ns].is_empty() { format!("{:X}xxxx", start >> 16) } else { family(&f[ns]) };
        let hit = cited.range(start..start + len).next().is_some();
        let e = groups.entry(group).or_default();
        e.0 += hit as usize;
        e.1 += 1;
    }
    // Families with only a few functions are lumped together so labels stay readable.
    let mut small = (0, 0);
    let mut out = Vec::new();
    for (name, (done, total)) in groups {
        if total < 8 {
            small.0 += done;
            small.1 += total;
        } else {
            out.push(Group { name, done, total, note: String::new() });
        }
    }
    if small.1 > 0 {
        out.push(Group { name: "other classes".into(), done: small.0, total: small.1, note: String::new() });
    }
    Some(Panel { title: "Retail functions".into(), groups: out })
}

fn files_panel(game: &Path, sources: &[(PathBuf, String)]) -> Option<Panel> {
    if !game.is_dir() {
        return None;
    }
    let code: String = sources.iter().filter(|(p, _)| p.extension().is_some_and(|e| e == "rs")).map(|(_, s)| s.to_lowercase()).collect::<Vec<_>>().join("\n");
    let mut known: BTreeMap<String, bool> = BTreeMap::new();
    let mut groups: BTreeMap<String, (usize, usize, BTreeSet<String>)> = BTreeMap::new();
    collect(game, &mut |p| {
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        let read = *known.entry(ext.clone()).or_insert_with(|| {
            !ext.is_empty() && [format!(".{ext}\""), format!("\"{ext}\""), format!("*.{ext}"), format!(".{ext}'")].iter().any(|n| code.contains(n.as_str()))
        });
        let rel = p.strip_prefix(game).unwrap_or(p);
        let parts: Vec<_> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
        let group = match parts.len() {
            0 | 1 => "(top folder)".to_string(),
            2 => parts[0].clone(),
            _ => format!("{}/{}", parts[0], parts[1]),
        };
        let e = groups.entry(group).or_default();
        e.0 += read as usize;
        e.1 += 1;
        if !read && !ext.is_empty() {
            e.2.insert(ext);
        }
    });
    let groups = groups
        .into_iter()
        .map(|(name, (done, total, unread))| Group { name, done, total, note: if unread.is_empty() { String::new() } else { format!("not read: {}", unread.into_iter().collect::<Vec<_>>().join(", ")) } })
        .collect();
    Some(Panel { title: "Game files".into(), groups })
}

fn features_panel(md: &Path) -> Option<Panel> {
    let text = fs::read_to_string(md).ok()?;
    let mut groups: BTreeMap<String, (usize, usize, Vec<String>)> = BTreeMap::new();
    for line in text.lines() {
        let cells: Vec<&str> = line.trim().trim_matches('|').split('|').map(str::trim).collect();
        if cells.len() < 4 {
            continue;
        }
        let Ok(percent) = cells[3].parse::<usize>() else { continue };
        let filled = (percent.min(100) * FEATURE_CELLS + 50) / 100;
        let e = groups.entry(cells[0].to_string()).or_default();
        e.0 += filled;
        e.1 += FEATURE_CELLS;
        e.2.push(format!("{} ({percent}%)", cells[1]));
    }
    if groups.is_empty() {
        return None;
    }
    let groups = groups.into_iter().map(|(name, (done, total, items))| Group { name, done, total, note: items.join("; ") }).collect();
    Some(Panel { title: "Features".into(), groups })
}

#[derive(Clone, Copy)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// Squarified treemap: lays `sizes` (largest first) into `r`, returning one rect per size.
fn squarify(sizes: &[f64], r: Rect) -> Vec<Rect> {
    let total: f64 = sizes.iter().sum();
    if sizes.is_empty() || total <= 0.0 {
        return Vec::new();
    }
    let scale = r.w * r.h / total;
    let areas: Vec<f64> = sizes.iter().map(|s| s * scale).collect();
    let mut out = Vec::with_capacity(areas.len());
    let mut rest = r;
    let mut i = 0;
    while i < areas.len() {
        let side = rest.w.min(rest.h);
        let worst = |row: &[f64]| {
            let s: f64 = row.iter().sum();
            let (mx, mn) = row.iter().fold((0.0f64, f64::MAX), |(a, b), &v| (a.max(v), b.min(v)));
            ((side * side * mx) / (s * s)).max((s * s) / (side * side * mn))
        };
        let mut j = i + 1;
        while j < areas.len() && worst(&areas[i..=j]) <= worst(&areas[i..j]) {
            j += 1;
        }
        let row = &areas[i..j];
        let s: f64 = row.iter().sum();
        if rest.w >= rest.h {
            let w = s / rest.h;
            let mut y = rest.y;
            for a in row {
                let h = a / w;
                out.push(Rect { x: rest.x, y, w, h });
                y += h;
            }
            rest = Rect { x: rest.x + w, w: rest.w - w, ..rest };
        } else {
            let h = s / rest.w;
            let mut x = rest.x;
            for a in row {
                let w = a / h;
                out.push(Rect { x, y: rest.y, w, h });
                x += w;
            }
            rest = Rect { y: rest.y + h, h: rest.h - h, ..rest };
        }
        i = j;
    }
    out
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn svg(panels: &[Panel]) -> String {
    let width = panels.len() as f64 * (PANEL_W + GAP) + GAP;
    let height = HEADER_H + PANEL_H + GAP;
    let mut s = String::new();
    let _ = write!(s, r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" font-family="Segoe UI, sans-serif"><rect width="100%" height="100%" fill="{BG}"/>"#);
    for (k, p) in panels.iter().enumerate() {
        let x0 = GAP + k as f64 * (PANEL_W + GAP);
        let (d, t) = p.totals();
        let _ = write!(s, r#"<text x="{x0}" y="21" fill="white" font-size="15">{}: {:.2}% ({d}/{t})</text>"#, esc(&p.title), pct(d, t));
        let mut order: Vec<&Group> = p.groups.iter().filter(|g| g.total > 0).collect();
        order.sort_by(|a, b| b.total.cmp(&a.total));
        let sizes: Vec<f64> = order.iter().map(|g| g.total as f64).collect();
        let rects = squarify(&sizes, Rect { x: x0, y: HEADER_H, w: PANEL_W, h: PANEL_H });
        for (g, r) in order.iter().zip(rects) {
            let tip = format!("{}: {}/{} ({:.1}%){}", g.name, g.done, g.total, pct(g.done, g.total), if g.note.is_empty() { String::new() } else { format!(" - {}", g.note) });
            let _ = write!(s, r#"<g><title>{}</title><rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="{BG}"/>"#, esc(&tip), r.x, r.y, r.w, r.h);
            cells(&mut s, g, Rect { x: r.x + 1.0, y: r.y + 1.0, w: (r.w - 2.0).max(0.0), h: (r.h - 2.0).max(0.0) });
            if r.w > 42.0 && r.h > 16.0 {
                let _ = write!(s, r#"<text x="{:.2}" y="{:.2}" fill="white" font-size="11" stroke="{BG}" stroke-width="3" paint-order="stroke">{}</text>"#, r.x + 4.0, r.y + 13.0, esc(&label(&g.name, r.w)));
            }
            s.push_str("</g>");
        }
    }
    s.push_str("</svg>\n");
    s
}

fn label(name: &str, w: f64) -> String {
    let fit = ((w - 8.0) / 6.2) as usize;
    if name.chars().count() <= fit { name.to_string() } else { name.chars().take(fit.saturating_sub(1)).collect::<String>() + "…" }
}

/// One cell per item in a grid filling `r`, done items first.
fn cells(s: &mut String, g: &Group, r: Rect) {
    if g.total == 0 || r.w <= 0.0 || r.h <= 0.0 {
        return;
    }
    let side = (r.w * r.h / g.total as f64).sqrt();
    let mut cols = (r.w / side).ceil().max(1.0) as usize;
    let mut rows = g.total.div_ceil(cols);
    while (rows as f64) * (r.w / cols as f64) > r.h + 0.01 && cols < g.total {
        cols += 1;
        rows = g.total.div_ceil(cols);
    }
    let cell = (r.w / cols as f64).min(r.h / rows as f64);
    let pad = if cell > 3.0 { 0.6 } else { 0.0 };
    for i in 0..g.total {
        let (cx, cy) = ((i % cols) as f64 * cell, (i / cols) as f64 * cell);
        let fill = if i < g.done { DONE } else { LEFT };
        let _ = write!(s, r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="{fill}"/>"#, r.x + cx, r.y + cy, (cell - pad).max(0.3), (cell - pad).max(0.3));
    }
}

fn json(panels: &[Panel]) -> String {
    let mut s = String::from("{\n");
    for (k, p) in panels.iter().enumerate() {
        let (d, t) = p.totals();
        let _ = write!(s, "  \"{}\": {{\"done\": {d}, \"total\": {t}, \"groups\": [", esc(&p.title));
        for (i, g) in p.groups.iter().enumerate() {
            let _ = write!(s, "{}{{\"name\": \"{}\", \"done\": {}, \"total\": {}}}", if i > 0 { ", " } else { "" }, g.name.replace('"', "'"), g.done, g.total);
        }
        let _ = writeln!(s, "]}}{}", if k + 1 < panels.len() { "," } else { "" });
    }
    s.push_str("}\n");
    s
}
