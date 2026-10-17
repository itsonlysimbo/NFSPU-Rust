//! Offline check of the race data on all 15 tracks, both directions: road slices, route graph,
//! circuit or point-to-point, lap length, start slice, grid, finish line, and the render keys.
//! Run: `cargo run -p nfspu-game --example race_audit --release`  (writes logs/race_audit.txt too)

use nfspu_formats::keys;
use nfspu_game::race_path::{self, grid_spot, Direction};
use nfspu_game::ground::Ground;
use nfspu_game::track;
use std::fmt::Write as _;

fn main() {
    let dir = track::track_dir().expect("set NFSPU_GAME_DIR");
    let mut names: Vec<String> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok())
        .filter_map(|e| { let p = e.path(); (p.extension()? == "crp").then(|| p.file_stem().unwrap().to_string_lossy().to_string()) }).collect();
    names.sort();
    let mut out = String::new();
    let _ = writeln!(out, "{:<11} {:<4} {:>6} {:>4} {:>8} {:>8} {:>5} {:>6} {:>6} {:>9} {:>8}  notes", "track", "dir", "slices", "seg", "kind", "lap km", "trig", "start", "f@start", "f@finish", "keys");
    let mut problems = 0;
    let mut ground_notes = String::new();
    for n in &names {
        // How much of the road path has no drivable ground from our loader under it (a car there
        // falls). This checks the ground loader, not the race data.
        if let (Ok(g), Ok(t)) = (track::load_ground(n), race_path::load(n, Direction::Forward)) {
            let ground = Ground::new(g.tris, 16.0);
            let bad: Vec<usize> = (0..t.path.slices.len()).filter(|&i| {
                let s = t.path.slices[i].pos;
                ground.height_at(s.x, s.z, s.y + 2.0, 0.0).map_or(true, |h| (h - s.y).abs() > 3.0)
            }).collect();
            let _ = writeln!(ground_notes, "{n:<11} slices without ground within 3 m: {:>5} of {:>5}{}", bad.len(), t.path.slices.len(),
                if bad.is_empty() { String::new() } else { format!("  (first {:?})", &bad[..bad.len().min(8)]) });
        }
        for d in [Direction::Forward, Direction::Backward] {
            let t = match race_path::load(n, d) {
                Ok(t) => t,
                Err(e) => { let _ = writeln!(out, "{n:<11} ERROR {e}"); problems += 1; continue; }
            };
            let p = &t.path;
            let f_start = p.fraction(t.start_slice, d);
            let mut notes = Vec::new();
            let f_finish = match t.finish {
                Some((a, b)) => {
                    let s = p.nearest((a + b) * 0.5);
                    let f = p.fraction(s, d);
                    if f < 0.9 { notes.push(format!("finish line only {:.0}% along", f * 100.0)); problems += 1; }
                    format!("{:.3}", f)
                }
                None => "-".into(),
            };
            if t.point_to_point() && t.finish.is_none() { notes.push("point-to-point but no finish line".into()); problems += 1; }
            if f_start > 0.1 && !(t.point_to_point() && f_start < 0.2) { notes.push(format!("start {:.0}% along", f_start * 100.0)); }
            // grid: 8 cars must all be on distinct slices ahead of the start, near the road centre
            let spots: Vec<_> = (0..8).map(|g| grid_spot(p, t.start_slice, g, 8, d)).collect();
            for (g, s) in spots.iter().enumerate() {
                let off = s.pos.distance(p.slices[s.slice].pos);
                if off > 6.0 { notes.push(format!("grid {g} {off:.1} m off centre")); problems += 1; }
            }
            let pole_ahead = p.fraction(spots[0].slice, d) > p.fraction(spots[7].slice, d) || (p.fraction(spots[0].slice, d) - p.fraction(spots[7].slice, d)).abs() > 0.5;
            if !pole_ahead { notes.push("pole is not ahead of the last car".into()); problems += 1; }
            let key_name = format!("{n}_{}.key", if d == Direction::Forward { "forward" } else { "backwards" }).to_ascii_lowercase();
            let key = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).find(|e| e.file_name().to_string_lossy().to_ascii_lowercase() == key_name)
                .and_then(|e| keys::parse(&std::fs::read(e.path()).ok()?));
            let keys_col = match &key { Some(k) => format!("{}", k.keys.len()), None => "none".into() };
            let _ = writeln!(out, "{:<11} {:<4} {:>6} {:>4} {:>8} {:>8.2} {:>5} {:>6} {:>7.3} {:>9} {:>8}  {}",
                n, if d == Direction::Forward { "fwd" } else { "back" }, p.slices.len(), p.segments.len(),
                if t.point_to_point() { "p2p" } else { "circuit" }, t.lap_metres() / 1000.0, t.triggers.len(), t.start_slice, f_start, f_finish, keys_col, notes.join("; "));
        }
    }
    let _ = writeln!(out, "problems: {problems}

{ground_notes}");
    print!("{out}");
    let logs = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("logs");
    let _ = std::fs::create_dir_all(&logs);
    let _ = std::fs::write(logs.join("race_audit.txt"), &out);
}
