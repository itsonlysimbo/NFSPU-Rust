//! Offline check of the race data on all 15 tracks, both directions: road slices, route graph,
//! circuit or point-to-point, lap length, start slice, grid, finish line, the render keys and the
//! opponents' racing line (`.lsp`).
//! Run: `cargo run -p nfspu-game --example race_audit --release`  (writes logs/race_audit.txt too)

use nfspu_formats::keys;
use nfspu_game::race_path::{self, grid_spot, Direction};
use nfspu_game::ground::Ground;
use nfspu_game::track;
use std::fmt::Write as _;

fn main() {
    if std::env::var("NFSPU_MIRROR").as_deref() == Ok("1") {
        nfspu_game::set_world_mirrored(true);
    }
    let dir = track::track_dir().expect("set NFSPU_GAME_DIR");
    let mut names: Vec<String> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok())
        .filter_map(|e| { let p = e.path(); (p.extension()? == "crp").then(|| p.file_stem().unwrap().to_string_lossy().to_string()) }).collect();
    names.sort();
    let mut out = String::new();
    let _ = writeln!(out, "{:<11} {:<4} {:>6} {:>4} {:>8} {:>8} {:>5} {:>6} {:>6} {:>9} {:>8}  notes", "track", "dir", "slices", "seg", "kind", "lap km", "trig", "start", "f@start", "f@finish", "keys");
    let mut problems = 0;
    let mut ground_notes = String::new();
    let mut line_notes = String::new();
    for n in &names {
        // How much of the road path has no drivable ground from our loader under it (a car there
        // falls). This checks the ground loader, not the race data. Slice heights differ from the
        // road mesh by up to ~7 m in places (and side-road stubs dip ~10 m under the terrain), so
        // ground 3-8 m below or up to 15 m above is counted separately as an offset, not a hole.
        if let (Ok(g), Ok(t)) = (track::load_ground(n), race_path::load(n, Direction::Forward)) {
            let carriers = g.road_carriers;
            let ground = Ground::new(g.tris, 16.0);
            let (mut holes, mut offset) = (Vec::new(), 0usize);
            for i in 0..t.path.slices.len() {
                let s = t.path.slices[i].pos;
                if ground.height_at(s.x, s.z, s.y + 2.0, 0.0).map_or(false, |h| (h - s.y).abs() <= 3.0) {
                    continue;
                }
                // ground up to 8 m below or 15 m above (a slice under the mesh) still carries a car
                if ground.surface_near(s.x, s.z, 0.0, s.y - 8.0, s.y + 15.0).is_some() {
                    offset += 1;
                } else {
                    holes.push(i);
                }
            }
            let _ = writeln!(ground_notes, "{n:<11} slices with no ground: {:>4} of {:>5}; ground 3+ m off: {offset:>4}; road-carrying articles: {carriers:>3}{}",
                holes.len(), t.path.slices.len(),
                if holes.is_empty() { String::new() } else { format!("  (first {:?})", &holes[..holes.len().min(8)]) });
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
            // racing line (.lsp): one point per slice, inside the road's half-widths
            match race_path::load_racing_line(n, d) {
                None => notes.push("no .lsp racing line".into()),
                Some(l) if l.len() != p.slices.len() => {
                    notes.push(format!(".lsp has {} points for {} slices", l.len(), p.slices.len()));
                    problems += 1;
                }
                Some(l) => {
                    let line = nfspu_game::ai::racing_line(p, Some(&l));
                    let (mut outside, mut sum, mut worst, mut along) = (0usize, 0.0f32, 0.0f32, 0.0f32);
                    for (s, q) in p.slices.iter().zip(&line) {
                        let off = (*q - s.pos).dot(s.right);
                        along = along.max((*q - s.pos).dot(s.forward).abs());
                        sum += off.abs();
                        worst = worst.max(off.abs());
                        if off < -s.left_width - 0.5 || off > s.right_width + 0.5 {
                            outside += 1;
                        }
                    }
                    let _ = writeln!(line_notes, "{n:<11} {:<4} line: mean {:.2} m off centre, worst {worst:.1} m, max {along:.1} m along the slice, {outside} points off the road",
                        if d == Direction::Forward { "fwd" } else { "back" }, sum / line.len().max(1) as f32);
                }
            }
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

{line_notes}
{ground_notes}");
    print!("{out}");
    let logs = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("logs");
    let _ = std::fs::create_dir_all(&logs);
    let _ = std::fs::write(logs.join("race_audit.txt"), &out);
}
