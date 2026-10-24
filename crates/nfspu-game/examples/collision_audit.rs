//! Collision audit: sweeps the road of every track (the geometry is the same forward and
//! backward, and the sweep covers both directions) across the full road width plus shoulders at
//! fine steps and reports, per road slice:
//!  - `HOLE`    no ground within the road width (a car there falls through)
//!  - `STEP`    ground jumps more than 0.6 m between neighbouring samples (across or along the road)
//!  - `OPEN`    the outer edge has no ground and no wall within 4 m beyond it (a car can leave the road)
//!  - `PINCH`   walls leave less than 2.2 m of free width on the road (a car can wedge)
//! Run: `cargo run -p nfspu-game --example collision_audit --release -j 2 -- [track]`
//! Log: logs/collision_audit.log
use nfspu_game::collision::WallSet;
use nfspu_game::ground::Ground;
use nfspu_game::race_path::{self, Direction};
use nfspu_game::track;
use std::fmt::Write as _;

const RIDE: f32 = nfspu_game::car::RIDE_HEIGHT;
const LATERAL_STEP: f32 = 0.5;
const ALONG_STEP: f32 = 1.0;
const SHOULDER: f32 = 3.0;
/// Free width a car (about 1.9 m wide) needs, plus a little room.
const CAR_WIDTH: f32 = 2.2;

struct Counts {
    bridged: usize,
    offset: usize,
    hole: usize,
    step: usize,
    open: usize,
    pinch: usize,
}

fn audit(name: &str, log: &mut String) -> Result<Counts, String> {
    let g = track::load_ground(name)?;
    let walls = WallSet::new(g.walls.clone(), 4.0);
    let ground = Ground::new(g.tris, 4.0);
    let t = race_path::load(name, Direction::Forward)?;
    let slices = &t.path.slices;
    let mut c = Counts { bridged: 0, offset: 0, hole: 0, step: 0, open: 0, pinch: 0 };
    let note = |log: &mut String, kind: &str, slice: usize, along: f32, lat: f32, text: String| {
        let _ = writeln!(log, "{name} {kind} slice {slice} +{along:.1} m lateral {lat:+.1}: {text}");
    };
    // consecutive slices of one segment, sampled every ALONG_STEP between them
    for seg in &t.path.segments {
        let mut prev_row: Option<(f32, Vec<Option<f32>>)> = None;
        for i in seg.first..seg.last {
            let (a, b) = (&slices[i], &slices[i + 1]);
            let len = a.pos.distance(b.pos).max(0.01);
            if len > 20.0 {
                prev_row = None; // a jump between separate road pieces, not a stretch of road
                continue;
            }
            let n = (len / ALONG_STEP).ceil().max(1.0) as usize;
            for k in 0..n {
                let f = k as f32 / n as f32;
                let along = f * len;
                let pos = a.pos.lerp(b.pos, f);
                let right = a.right.lerp(b.right, f).normalize_or_zero();
                let (lw, rw) = (a.left_width + (b.left_width - a.left_width) * f, a.right_width + (b.right_width - a.right_width) * f);
                let base = a.pos.y + (b.pos.y - a.pos.y) * f;
                let cols = ((lw + rw + 2.0 * SHOULDER) / LATERAL_STEP).ceil() as usize;
                // lateral position: negative is left of the centre
                let lat_of = |q: usize| -lw - SHOULDER + q as f32 * LATERAL_STEP;
                let mut row: Vec<Option<f32>> = Vec::with_capacity(cols + 1);
                let mut y_ref = base;
                for q in 0..=cols {
                    let l = lat_of(q);
                    let p = pos + right * l;
                    // surfaces near the slice height; the nearest to the previous sample wins
                    let h = ground.surface_near(p.x, p.z, 0.0, y_ref - 3.0, y_ref + 3.0);
                    if let Some(h) = h {
                        y_ref = h;
                    }
                    row.push(h);
                }
                // holes and steps across the road
                // a gap with a surface 3..15 m off the slice is a slice/mesh offset (unused stub or
                // under-road slice), not a hole
                // the road surface level in this row (the mesh can sit above the slice)
                let local = row.iter().flatten().next().copied().unwrap_or(base);
                let gap = |c: &mut Counts, log: &mut String, i: usize, along: f32, s: f32, e: f32| {
                    let p = pos + right * ((s + e) * 0.5);
                    // the car's own support tiers (car.rs PROBE/NEAR/FAR): it is held up here
                    let held = [(1.5f32, 1.5f32), (3.0, 2.0), (6.0, 5.0)].iter().any(|&(r, d)| ground.surface_near(p.x, p.z, r, local - d, local + 1.0).is_some());
                    if held {
                        c.bridged += 1;
                    } else if ground.surface_near(p.x, p.z, 0.0, base - 15.0, base + 15.0).is_some() {
                        c.offset += 1;
                        note(log, "OFFSET", i, along, (s + e) * 0.5, format!("road surface 3+ m off the slice over {:.1}..{:.1} m", s, e));
                    } else {
                        c.hole += 1;
                        note(log, "HOLE", i, along, (s + e) * 0.5, format!("no ground over {:.1}..{:.1} m of the road", s, e));
                    }
                };
                let mut hole_run: Option<(f32, f32)> = None;
                for q in 0..=cols {
                    let l = lat_of(q);
                    let on_road = l >= -lw && l <= rw;
                    match (row[q], on_road) {
                        (None, true) => hole_run = Some(hole_run.map_or((l, l), |(s, _)| (s, l))),
                        _ => {
                            if let Some((s, e)) = hole_run.take() {
                                gap(&mut c, log, i, along, s, e);
                            }
                        }
                    }
                    if q > 0 {
                        if let (Some(h0), Some(h1)) = (row[q - 1], row[q]) {
                            if (h1 - h0).abs() > 0.6 && on_road {
                                c.step += 1;
                                note(log, "STEP", i, along, l, format!("{:.2} m across the road", h1 - h0));
                            }
                        }
                    }
                }
                if let Some((s, e)) = hole_run.take() {
                    gap(&mut c, log, i, along, s, e);
                }
                if let Some((_, prow)) = &prev_row {
                    if prow.len() == row.len() {
                        for q in 0..row.len() {
                            if let (Some(h0), Some(h1)) = (prow[q], row[q]) {
                                let l = lat_of(q);
                                if (h1 - h0).abs() > 0.6 && l >= -lw && l <= rw {
                                    c.step += 1;
                                    note(log, "STEP", i, along, l, format!("{:.2} m along the road", h1 - h0));
                                }
                            }
                        }
                    }
                }
                prev_row = Some((len, row.clone()));
                // wall pinch: free lateral width where the car's body circle meets no wall
                let free = |l: f32, yb: f32| -> bool {
                    let p = pos + right * l;
                    walls.resolve(p.x, p.z, yb - 0.3, yb + RIDE + 1.0, 1.0).is_none()
                };
                let mut best = 0.0f32;
                let mut run = 0.0f32;
                let mut l = -lw;
                while l <= rw {
                    if free(l, base) {
                        run += LATERAL_STEP;
                        best = best.max(run);
                    } else {
                        run = 0.0;
                    }
                    l += LATERAL_STEP;
                }
                // the free run counts circle centres (they are 1 m from a wall); a car needs 0 of them
                if best < 0.01 && lw + rw > CAR_WIDTH {
                    c.pinch += 1;
                    note(log, "PINCH", i, along, 0.0, format!("walls block the whole {:.1} m road", lw + rw));
                }
                // outer edges: a car driven off must meet ground or a wall within 4 m
                for (side, edge) in [(-1.0f32, lw), (1.0, rw)] {
                    let mut safe = false;
                    for m in 1..=8 {
                        let l = side * (edge + m as f32 * 0.5);
                        let p = pos + right * l;
                        if ground.surface_near(p.x, p.z, 6.0, base - 5.0, base + 1.0).is_some() || !free(l, base) {
                            safe = true;
                            break;
                        }
                    }
                    if !safe && edge > 0.5 {
                        c.open += 1;
                        note(log, "OPEN", i, along, side * edge, "no ground or wall within 4 m beyond the edge".to_string());
                    }
                }
            }
        }
    }
    Ok(c)
}

fn main() {
    let dir = track::track_dir().expect("set NFSPU_GAME_DIR");
    let arg = std::env::args().nth(1);
    if let Some(sl) = std::env::args().nth(2).and_then(|v| v.parse::<usize>().ok()) {
        // probe mode: heights of every surface in a column across one slice (wide band)
        let n = arg.clone().unwrap();
        let g = track::load_ground(&n).unwrap();
        let ceil = Ground::new(g.ceilings.clone(), 4.0);
        let ground = Ground::new(g.tris, 4.0);
        let t = race_path::load(&n, Direction::Forward).unwrap();
        let s = &t.path.slices[sl];
        println!("slice {sl} pos {:?} widths {} {}", s.pos, s.left_width, s.right_width);
        for w in &g.walls {
            let near = w.iter().any(|v| ((v[0] - s.pos.x).powi(2) + (v[2] - s.pos.z).powi(2)).sqrt() < 2.5 && (v[1] - s.pos.y).abs() < 4.0);
            if near {
                println!("wall tri (rel y {:.1}): {:?}", 0.0, w.map(|v| [v[0] - s.pos.x, v[1] - s.pos.y, v[2] - s.pos.z]));
            }
        }
        let mut l = -s.left_width - 3.0;
        while l <= s.right_width + 3.0 {
            let p = s.pos + s.right * l;
            let hs: Vec<String> = [(-40.0f32, -3.0f32), (-3.0, 3.0), (3.0, 40.0)].iter().map(|(a, b)| ground.surface_near(p.x, p.z, 0.0, s.pos.y + a, s.pos.y + b).map_or("-".into(), |h| format!("{:+.1}", h - s.pos.y))).collect();
            let c = ceil.surface_near(p.x, p.z, 0.0, s.pos.y - 40.0, s.pos.y + 40.0).map_or("-".into(), |h| format!("{:+.1}", h - s.pos.y));
            println!("lat {l:+.1}: below {} near {} above {} | flipped/ceiling {c}", hs[0], hs[1], hs[2]);
            l += 1.0;
        }
        return;
    }
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let p = e.path();
            (p.extension()? == "crp").then(|| p.file_stem().unwrap().to_string_lossy().to_string())
        })
        .collect();
    names.sort();
    if let Some(a) = arg {
        names.retain(|n| *n == a);
    }
    let mut log = String::new();
    let mut total = 0;
    for n in &names {
        let mut tl = String::new();
        match audit(n, &mut tl) {
            Ok(c) => {
                let sum = format!("{n}: bridged {} hole {} offset {} step {} open {} pinch {}", c.bridged, c.hole, c.offset, c.step, c.open, c.pinch);
                println!("{sum}");
                let _ = writeln!(log, "== {sum}");
                // group the per-metre lines into runs of consecutive slices per kind
                for kind in ["HOLE", "PINCH", "STEP", "OPEN", "OFFSET"] {
                    let mut runs: Vec<(usize, usize, f32, f32, usize)> = Vec::new(); // first, last, min lat, max lat, count
                    for l in tl.lines().filter(|l| l.split(' ').nth(1) == Some(kind)) {
                        let w: Vec<&str> = l.split(' ').collect();
                        let slice: usize = w[3].parse().unwrap_or(0);
                        let lat: f32 = w[7].trim_end_matches(':').parse().unwrap_or(0.0);
                        match runs.last_mut() {
                            Some(r) if slice <= r.1 + 2 => {
                                r.1 = r.1.max(slice);
                                r.2 = r.2.min(lat);
                                r.3 = r.3.max(lat);
                                r.4 += 1;
                            }
                            _ => runs.push((slice, slice, lat, lat, 1)),
                        }
                    }
                    for r in runs.iter().take(60) {
                        let _ = writeln!(log, "{n} {kind} slices {}..{} lateral {:+.1}..{:+.1} ({} samples)", r.0, r.1, r.2, r.3, r.4);
                    }
                    if runs.len() > 60 {
                        let _ = writeln!(log, "{n} {kind} ... {} more runs", runs.len() - 60);
                    }
                    if !runs.is_empty() {
                        let _ = writeln!(log, "{n} {kind}: {} runs", runs.len());
                    }
                }
                total += c.hole + c.pinch;
            }
            Err(e) => {
                println!("{n}: ERROR {e}");
                let _ = writeln!(log, "== {n}: ERROR {e}");
                total += 1;
            }
        }
    }
    let _ = std::fs::create_dir_all("logs");
    let _ = std::fs::write("logs/collision_audit.log", &log);
    println!("{}", if total == 0 { "AUDIT CLEAN (holes and pinches)" } else { "AUDIT FOUND HOLES/PINCHES" });
}
