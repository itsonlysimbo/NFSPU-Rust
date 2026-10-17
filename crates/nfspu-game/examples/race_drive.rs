//! Offline scripted race (no window): one car starts on the retail grid, waits for the countdown,
//! then an autopilot drives the road slices in race direction while `race::Race` counts laps.
//! Checks that the countdown holds the car, laps are counted and the race finishes.
//! Run: `cargo run -p nfspu-game --example race_drive --release -- monaco1 forward 2`
//! (track, forward|backward, laps; with no track it runs every track once). Log: logs/race_drive.txt

use nfspu_game::car::{Car, Input};
use nfspu_game::collision::WallSet;
use nfspu_game::ground::Ground;
use nfspu_game::race::{self, CarSample, Kind, Race, RaceEvent, RaceSetup};
use nfspu_game::race_path::{self, crosses, grid_spot, Direction};
use nfspu_game::track;
use std::fmt::Write as _;

fn wrap(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

fn drive(name: &str, dir: Direction, laps: u32, out: &mut String) -> bool {
    let t = match race_path::load(name, dir) {
        Ok(t) => t,
        Err(e) => { let _ = writeln!(out, "{name}: {e}"); return false; }
    };
    let g = track::load_ground(name).expect("ground");
    let ground = Ground::new(g.tris, 16.0);
    let walls = WallSet::new(g.walls, 4.0);
    let p = &t.path;
    let spot = grid_spot(p, t.start_slice, 0, 1, dir);
    let mut car = Car::new(spot.pos + bevy::math::Vec3::Y, spot.yaw);
    let kind = if t.point_to_point() { Kind::PointToPoint } else { Kind::Circuit };
    let mut slice = spot.slice;
    let mut r = Race::new(RaceSetup { kind, laps, cars: 1 }, &[p.fraction(slice, dir)]);
    let _ = writeln!(out, "{name} {dir:?}: {kind:?}, {} laps, start slice {}, grid slice {}, start fraction {:.3}", r.setup.laps, t.start_slice, slice, p.fraction(slice, dir));
    let dt = 1.0 / race::TICK_HZ as f32;
    let start_pos = car.pos;
    let mut route = slice;
    let mut route_target = p.slices[route].pos;
    let debug = std::env::var("RACE_DRIVE_DEBUG").is_ok();
    let mut moved_before_go = 0.0f32;
    let (mut wrong_ticks, mut stuck, mut backing, mut best_progress, mut last_gain) = (0u32, false, 0.0f32, 0.0f32, 0u32);
    while !r.all_finished() {
        let before = car.pos;
        let input = if r.held() {
            Input::default()
        } else {
            // the autopilot follows its own route along the main road (waypoints), separate from
            // the race's slice tracking
            while route_target.distance(car.pos) < 14.0 {
                let next = p.walk(route, 2, dir);
                if next == route { break; }
                route = next;
                route_target = p.slices[route].pos;
            }
            let to = route_target - car.pos;
            let diff = wrap((-to.x).atan2(-to.z) - car.yaw);
            let want = if diff.abs() > 0.5 { 10.0 } else { 22.0 };
            if backing > 0.0 {
                backing -= dt;
                Input { brake: true, steer: -(diff * 2.5).clamp(-1.0, 1.0), ..Default::default() }
            } else {
                Input { throttle: car.speed < want, brake: car.speed > want + 5.0, steer: (diff * 2.5).clamp(-1.0, 1.0), ..Default::default() }
            }
        };
        car.step(input, dt, &ground, &walls);
        if r.held() {
            let d = car.pos - start_pos;
            moved_before_go = moved_before_go.max((d.x * d.x + d.z * d.z).sqrt());
        }
        if debug && ((r.tick % 64 == 0 && r.tick <= 64 * 120) || r.tick % 640 == 0) {
            let _ = writeln!(out, "      t {:>5} slice {slice:>5} seg {:>3} frac {:.3} pos ({:.0},{:.0},{:.0}) yaw {:.2} speed {:.1}",
                r.tick, p.segment_of(slice), p.fraction(slice, dir), car.pos.x, car.pos.y, car.pos.z, car.yaw, car.speed);
        }
        slice = p.track(slice, car.pos);
        let crossed = t.finish.map_or(false, |(a, b)| crosses(before, car.pos, a, b));
        let vel = car.pos - before;
        if p.wrong_way(slice, vel / dt, dir) && !r.held() {
            wrong_ticks += 1;
        }
        for e in r.step(&[CarSample { fraction: p.fraction(slice, dir), crossed_finish: crossed }]) {
            let line = match e {
                RaceEvent::Countdown(n) => format!("   tick {:>5}: countdown {}", r.tick, if n == 0 { "GO".to_string() } else { n.to_string() }),
                RaceEvent::Lap { lap, ticks, .. } => format!("   tick {:>5}: lap {lap} in {} (slice {slice})", r.tick, race::format_time(ticks)),
                RaceEvent::LapOwed { .. } => format!("   tick {:>5}: crossed the line backwards (slice {slice})", r.tick),
                RaceEvent::Finished { position, ticks, .. } => format!("   tick {:>5}: FINISHED P{position} in {}, best lap {}", r.tick, race::format_time(ticks), r.best_lap(0).map_or("-".into(), race::format_time)),
            };
            let _ = writeln!(out, "{line}");
        }
        let prog = r.cars[0].progress;
        if prog > best_progress + 0.002 || r.held() { best_progress = best_progress.max(prog); last_gain = r.tick; }
        if !r.held() && r.tick - last_gain > 64 * 6 && backing <= 0.0 { backing = 2.0; last_gain = r.tick; }
        if r.race_ticks() > 64 * 60 * 25 { stuck = true; break; }
    }
    let _ = writeln!(out, "   held before GO: moved {moved_before_go:.2} m sideways; wrong-way ticks {wrong_ticks}; final progress {:.3}; {}",
        r.cars[0].progress, if stuck { "GAVE UP (stuck or too slow)" } else { "ok" });
    !stuck && moved_before_go < 0.5
}

fn log_file() -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("logs");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("race_drive.txt")
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut out = String::new();
    let mut ok = true;
    if let Some(name) = a.first() {
        let dir = if a.get(1).map_or(false, |d| d.starts_with('b')) { Direction::Backward } else { Direction::Forward };
        let laps = a.get(2).and_then(|l| l.parse().ok()).unwrap_or(2);
        ok &= drive(name, dir, laps, &mut out);
    } else {
        for n in ["alps", "autobahn", "canyon", "castle", "coastal", "farmland", "foothills", "forest", "industrial", "monaco1", "monaco2", "monaco3", "monaco4", "monaco5", "skidpad"] {
            ok &= drive(n, Direction::Forward, 2, &mut out);
            let _ = std::fs::write(log_file(), &out);
        }
    }
    print!("{out}");
    let _ = std::fs::write(log_file(), &out);
    println!("{}", if ok { "ALL OK" } else { "SOME FAILED" });
}
