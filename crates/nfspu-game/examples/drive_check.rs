//! Offline check (no window): for every track, put the car at the start and hold the throttle straight
//! ahead for 3 seconds. Reports whether the car stays on the ground and how far it got.
//! Run: `cargo run -p nfspu-game --example drive_check --release`

use nfspu_game::car::{Car, Input, RIDE_HEIGHT};
use nfspu_game::collision::WallSet;
use nfspu_game::ground::Ground;
use nfspu_game::track;

fn main() {
    let dir = track::track_dir().expect("set NFSPU_GAME_DIR");
    let mut names: Vec<String> = std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok())
        .filter_map(|e| { let p = e.path(); (p.extension()? == "crp").then(|| p.file_stem().unwrap().to_string_lossy().to_string()) }).collect();
    names.sort();
    println!("{:<11} {:>8} {:>7} {:>8}  {:<40} {:>9} {:>9} {:>9}", "track", "ground", "road", "terrain", "start found by", "on ground", "end speed", "ground y");
    for n in names {
        let g = match track::load_ground(&n) { Ok(g) => g, Err(e) => { println!("{n:<11} ERROR {e}"); continue } };
        let ground = Ground::new(g.tris, 16.0);
        let walls = WallSet::new(g.walls, 4.0);
        let mut car = Car::new(g.start_pos, g.start_yaw);
        let mut grounded = 0usize;
        let steps = (3.0f32 / 0.016) as usize;
        for _ in 0..steps {
            car.step(Input { throttle: true, ..Default::default() }, 0.016, &ground, &walls);
            if car.on_ground { grounded += 1; }
        }
        let note: String = g.start_note.chars().take(40).collect();
        println!("{:<11} {:>8} {:>7} {:>8}  {:<40} {:>8.0}% {:>7.1}m/s {:>9.1}",
            n, ground.triangle_count(), g.road_triangles, g.terrain_triangles, note, 100.0 * grounded as f32 / steps as f32, car.speed, car.pos.y - RIDE_HEIGHT);
    }
}
