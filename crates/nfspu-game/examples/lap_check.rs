//! Offline check (no window): an autopilot drives each track's road pieces in order, as a stand-in
//! for a driver. Reports whether the car ever loses the ground or falls off the track.
//! Run: `cargo run -p nfspu-game --example lap_check`   (optionally a track name as argument)

use nfspu_game::car::{Car, Input};
use nfspu_game::collision::WallSet;
use nfspu_game::ground::Ground;
use nfspu_game::track;

fn wrap(a: f32) -> f32 {
    let two_pi = std::f32::consts::TAU;
    (a + std::f32::consts::PI).rem_euclid(two_pi) - std::f32::consts::PI
}

fn main() {
    let only = std::env::args().nth(1);
    let dir = track::track_dir().expect("set NFSPU_GAME_DIR");
    let mut names: Vec<String> = std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok())
        .filter_map(|e| { let p = e.path(); (p.extension()? == "crp").then(|| p.file_stem().unwrap().to_string_lossy().to_string()) }).collect();
    names.sort();
    println!("{:<11} {:>10} {:>9} {:>9} {:>11} {:>10}  result", "track", "waypoints", "time", "airborne", "worst gap", "lowest y");
    for n in names {
        if let Some(o) = &only { if &n != o { continue; } }
        let g = match track::load_ground(&n) { Ok(g) => g, Err(e) => { println!("{n:<11} ERROR {e}"); continue } };
        let wps = g.centerline.clone();
        let ground = Ground::new(g.tris, 16.0);
        let walls = WallSet::new(g.walls, 4.0);
        let mut car = Car::new(g.start_pos, g.start_yaw);
        let dt = 0.016f32;
        let (mut wp, mut t, mut air, mut steps, mut gap, mut worst_gap, mut lowest) = (1usize, 0.0f32, 0usize, 0usize, 0.0f32, 0.0f32, f32::MAX);
        let mut wall_hits = 0usize;
        let mut last_progress = 0.0f32; // last time a waypoint was reached
        let mut backing = 0.0f32; // seconds left reversing out of a pin
        let start_y = g.start_pos.y;
        let mut episode_start = (car.pos, wp);
        let mut result = "completed";
        while wp < wps.len() {
            let to = wps[wp] - car.pos;
            if to.x * to.x + to.z * to.z < 12.0 * 12.0 { wp += 1; last_progress = t; continue; }
            let want = (-to.x).atan2(-to.z);
            let diff = wrap(want - car.yaw);
            let target = if diff.abs() > 0.5 { 9.0 } else { 17.0 };
            let mut input = Input { throttle: car.speed < target, brake: car.speed > target + 5.0, steer: (diff * 2.5).clamp(-1.0, 1.0), ..Default::default() };
            if backing > 0.0 {
                // backing out of a pin like a driver would, then the normal logic resumes
                backing -= dt;
                input = Input { brake: true, steer: -(diff * 2.5).clamp(-1.0, 1.0), ..Default::default() };
            } else if t - last_progress > 5.0 {
                backing = 2.0;
                last_progress = t;
            }
            car.step(input, dt, &ground, &walls);
            t += dt; steps += 1;
            if car.hit_wall { wall_hits += 1; }
            if !car.on_ground {
                if gap == 0.0 { episode_start = (car.pos, wp); }
                air += 1; gap += dt; worst_gap = worst_gap.max(gap);
            } else {
                if gap > 0.5 && only.is_some() {
                    println!("   airborne {:.2}s near waypoint {} : left ({:.0}, {:.1}, {:.0}), landed ({:.0}, {:.1}, {:.0})", gap, episode_start.1, episode_start.0.x, episode_start.0.y, episode_start.0.z, car.pos.x, car.pos.y, car.pos.z);
                }
                gap = 0.0;
            }
            lowest = lowest.min(car.pos.y);
            if car.pos.y < start_y - 300.0 { result = "FELL OFF THE TRACK"; break; }
            if t > 3000.0 { result = "gave up (stuck or lost)"; break; }
        }
        println!("{:<11} {:>4}/{:<5} {:>8.0}s {:>8.1}% {:>10.2}s {:>10.1}  {} (wall {:>5.1}%)", n, wp, wps.len(), t, 100.0 * air as f32 / steps.max(1) as f32, worst_gap, lowest, result, 100.0 * wall_hits as f32 / steps.max(1) as f32);
        if result != "completed" {
            println!("   stuck at ({:.1}, {:.1}, {:.1}) yaw {:.2}, next wp {} at ({:.1}, {:.1}, {:.1})",
                car.pos.x, car.pos.y, car.pos.z, car.yaw, wp, wps[wp.min(wps.len()-1)].x, wps[wp.min(wps.len()-1)].y, wps[wp.min(wps.len()-1)].z);
        }
    }
}
