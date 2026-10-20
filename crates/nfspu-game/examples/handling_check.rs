//! Keyboard handling check: scripted step-steer and lane-change manoeuvres at 100 and 150 km/h on
//! flat dry asphalt for a few retail cars, counting spin-outs. Inputs are digital like a keyboard
//! (steer -1/0/1, throttle on/off), fed through `Car::step` so the steering filter applies.
//! A run counts as a spin when the slip angle (velocity against heading) passes 35 degrees or the
//! yaw rate passes 2.5 rad/s; "slide" is a slip angle over 15 degrees.
//! Part two drives real tracks like a keyboard player: digital full-lock taps toward a look-ahead
//! point on the road (no yaw damping), full throttle up to a corner speed, braking for corners,
//! for 90 s per track, and counts spins (slip over 35 degrees above 10 m/s, or rolling backwards).
//! Run: `cargo run -p nfspu-game --example handling_check [car ids...]` (needs NFSPU_GAME_DIR).
//! Log: logs/handling_check.txt

use bevy::math::Vec3;
use nfspu_game::car::{Car, Input};
use nfspu_game::car_load::{self, Pick};
use nfspu_game::car_sim::Aids;
use nfspu_game::collision::WallSet;
use nfspu_game::ground::{Ground, Tri};
use nfspu_game::race_path::{self, grid_spot, Direction};
use nfspu_game::track;
use std::fmt::Write as _;
use std::{env, fs, path::PathBuf};

const SPIN_SLIP_DEG: f32 = 35.0;
const SPIN_YAW: f32 = 2.5;
const SLIDE_DEG: f32 = 15.0;

/// A manoeuvre: steering keys over time (seconds from the start) and whether the throttle is held
/// to keep speed or released (lift-off).
struct Manoeuvre {
    name: &'static str,
    /// (until time, steer) segments; after the last one the wheel is straight.
    steer: &'static [(f32, f32)],
    lift: bool,
}

const MANOEUVRES: &[Manoeuvre] = &[
    Manoeuvre { name: "step steer 1.0 s", steer: &[(0.5, 0.0), (1.5, 1.0)], lift: false },
    Manoeuvre { name: "step steer 2.0 s, lift", steer: &[(0.5, 0.0), (2.5, 1.0)], lift: true },
    Manoeuvre { name: "lane change 0.4/0.8/0.4 s", steer: &[(0.5, 0.0), (0.9, 1.0), (1.7, -1.0), (2.1, 1.0)], lift: false },
    Manoeuvre { name: "lane change 0.6/1.2/0.6 s", steer: &[(0.5, 0.0), (1.1, 1.0), (2.3, -1.0), (2.9, 1.0)], lift: false },
    Manoeuvre { name: "lane change, lift", steer: &[(0.5, 0.0), (0.9, 1.0), (1.7, -1.0), (2.1, 1.0)], lift: true },
];

struct Result {
    max_slip: f32,
    max_yaw: f32,
    spun: bool,
    slid: bool,
}

fn flat() -> Ground {
    let (a, b) = (-5000.0f32, 5000.0f32);
    let t: Vec<Tri> = vec![[[a, 0.0, a], [b, 0.0, a], [a, 0.0, b]], [[b, 0.0, a], [b, 0.0, b], [a, 0.0, b]]];
    Ground::new(t, 64.0)
}

fn run(base: &Car, kmh: f32, m: &Manoeuvre, ground: &Ground, walls: &WallSet) -> Result {
    let mut car = base.reset(Vec3::new(0.0, nfspu_game::car::RIDE_HEIGHT, 0.0), 0.0);
    let target = kmh / 3.6;
    // settle at the speed in a fitting gear first
    car.set_speed(target);
    let gear = car.sim.gear_for_speed(target, 0.75);
    car.sim.put_in_gear(gear, target);
    let dt = 1.0 / 64.0;
    for _ in 0..64 {
        let input = Input { throttle: car.speed < target, ..Default::default() };
        car.step(input, dt, ground, walls);
    }
    car.set_speed(target);
    let mut r = Result { max_slip: 0.0, max_yaw: 0.0, spun: false, slid: false };
    let end = m.steer.last().map_or(0.0, |s| s.0) + 3.0;
    let mut t = 0.0;
    while t < end {
        let steer = m.steer.iter().find(|s| t < s.0).map_or(0.0, |s| s.1);
        let lifted = m.lift && t >= m.steer[0].0;
        let input = Input { throttle: !lifted && car.speed < target, steer, ..Default::default() };
        car.step(input, dt, ground, walls);
        t += dt;
        let v_f = car.sim.forward_speed(car.yaw);
        let v_r = car.sim.lateral_speed(car.yaw);
        if car.sim.speed() > 3.0 {
            let slip = v_r.abs().atan2(v_f.max(0.0)).to_degrees();
            r.max_slip = r.max_slip.max(slip);
        }
        r.max_yaw = r.max_yaw.max(car.sim.yaw_rate.abs());
        if env::var("HANDLING_TRACE").is_ok_and(|v| v == "flat") && (t * 64.0).round() as i32 % 8 == 0 {
            println!("    t {t:4.2} steer {steer:+.0} thr {} v_f {v_f:5.1} v_r {v_r:5.2} yaw {:5.2} rate {:5.2} gear {} ground {} wheel {:.3}",
                input.throttle as u8, car.yaw, car.sim.yaw_rate, car.gear(), car.on_ground as u8, car.sim.wheels[0].angle);
        }
    }
    r.spun = r.max_slip > SPIN_SLIP_DEG || r.max_yaw > SPIN_YAW;
    r.slid = r.max_slip > SLIDE_DEG;
    r
}

fn wrap(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

const TRACKS: [&str; 6] = ["alps", "autobahn", "canyon", "coastal", "forest", "monaco1"];

/// One track with a keyboard-style driver: (spins, wall hits, metres driven, first spin notes).
fn track_run(base: &Car, name: &str, seconds: f32) -> Option<(u32, u32, f32, Vec<String>)> {
    let t = race_path::load(name, Direction::Forward).ok()?;
    let g = track::load_ground(name).ok()?;
    let ground = Ground::new(g.tris, 16.0);
    let walls = WallSet::new(g.walls, 4.0);
    let p = &t.path;
    let spot = grid_spot(p, t.start_slice, 0, 1, Direction::Forward);
    let mut car = base.reset(spot.pos + Vec3::Y, spot.yaw);
    let mut slice = spot.slice;
    let dt = 1.0 / 64.0;
    // two seconds on the line, then go
    car.sim.held = true;
    for _ in 0..128 {
        car.step(Input { throttle: true, ..Default::default() }, dt, &ground, &walls);
        slice = p.track(slice, car.pos);
    }
    car.sim.held = false;
    let (mut spins, mut walls_hit, mut dist, mut spinning, mut wall_on) = (0u32, 0u32, 0.0f32, false, false);
    let mut notes = Vec::new();
    let mut stuck_for = 0.0f32;
    let trace = env::var("HANDLING_TRACE").is_ok_and(|v| v == "track");
    let mut ring: std::collections::VecDeque<String> = Default::default();
    for i in 0..(seconds * 64.0) as usize {
        let speed = car.sim.speed();
        let ahead = ((speed * 0.8 / 6.0) as i32).clamp(3, 14);
        let aim = p.slices[p.walk(slice, ahead, Direction::Forward)].pos;
        let to = aim - car.pos;
        let err = wrap((-to.x).atan2(-to.z) - car.yaw);
        // corner speed from how much the road turns over the next ~80 m
        let a = p.slices[p.walk(slice, 2, Direction::Forward)].pos;
        let b = p.slices[p.walk(slice, 14, Direction::Forward)].pos;
        let c = p.slices[p.walk(slice, 26, Direction::Forward)].pos;
        let turn = wrap((-(c - b).x).atan2(-(c - b).z) - (-(b - a).x).atan2(-(b - a).z)).abs();
        let target = (45.0 - turn * 60.0).clamp(14.0, 45.0);
        let steer = if err > 0.04 { 1.0 } else if err < -0.04 { -1.0 } else { 0.0 };
        let mut input = Input { throttle: speed < target, brake: speed > target + 3.0, steer, ..Default::default() };
        if stuck_for > 3.0 {
            // reverse out for a moment like a player would
            input = Input { brake: true, steer: -steer, ..Default::default() };
            if stuck_for > 5.0 {
                stuck_for = 0.0;
            }
        }
        let before = car.pos;
        car.step(input, dt, &ground, &walls);
        slice = p.track(slice, car.pos);
        dist += (car.pos - before).length();
        stuck_for = if speed < 2.0 || stuck_for > 3.0 { stuck_for + dt } else { 0.0 };
        if car.hit_wall && !wall_on {
            walls_hit += 1;
        }
        wall_on = car.hit_wall;
        let (v_f, v_r) = (car.sim.forward_speed(car.yaw), car.sim.lateral_speed(car.yaw));
        let slip = v_r.abs().atan2(v_f.abs()).to_degrees();
        let now = car.sim.speed() > 10.0 && (slip > 35.0 || v_f < -2.0);
        if trace {
            let w = &car.sim.wheels;
            ring.push_back(format!(
                "        t {:.2} v_f {v_f:5.1} v_r {v_r:5.1} rate {:5.2} st {steer:+.0} thr {} gear {} rpm {:.0} ang {:.2} | load {:.1} {:.1} {:.1} {:.1} | lat {:.1} {:.1} {:.1} {:.1} | long {:.1} {:.1} {:.1} {:.1} | slide {:.2} {:.2}",
                i as f32 / 64.0, car.sim.yaw_rate, input.throttle as u8, car.gear(), car.rpm(), w[0].angle,
                w[0].load, w[1].load, w[2].load, w[3].load, w[0].force[0], w[1].force[0], w[2].force[0], w[3].force[0],
                w[0].force[1], w[1].force[1], w[2].force[1], w[3].force[1], w[2].slide, w[3].slide
            ));
            if ring.len() > 96 {
                ring.pop_front();
            }
        }
        if now && !spinning {
            spins += 1;
            if trace && spins == 1 {
                for (k, l) in ring.iter().enumerate() {
                    if k % 2 == 0 {
                        println!("{l}");
                    }
                }
            }
            if notes.len() < 3 {
                notes.push(format!(
                    "{:.1}s at ({:.0},{:.0}) v_f {v_f:.1} v_r {v_r:.1} rate {:.2} wall {} ground {} pitch {:.2} roll {:.2} steer {steer:+.0} thr {} brk {} gear {}",
                    i as f32 / 64.0, car.pos.x, car.pos.z, car.sim.yaw_rate, car.hit_wall as u8, car.on_ground as u8, car.pitch, car.roll, input.throttle as u8, input.brake as u8, car.gear()
                ));
            }
        }
        if car.sim.speed() < 3.0 || slip < 15.0 {
            spinning = false;
        }
        spinning |= now;
    }
    Some((spins, walls_hit, dist, notes))
}

/// A road along -Z whose height follows `h(distance)`, 60 m wide, in 1 m strips.
fn profile_ground(h: impl Fn(f32) -> f32) -> Ground {
    let mut t: Vec<Tri> = Vec::new();
    let (a, b) = (-30.0f32, 30.0f32);
    for i in -50..600 {
        let (d0, d1) = (i as f32, i as f32 + 1.0);
        let (z0, z1) = (-d0, -d1);
        let (h0, h1) = (h(d0), h(d1));
        t.push([[a, h0, z0], [b, h0, z0], [a, h1, z1]]);
        t.push([[b, h0, z0], [b, h1, z1], [a, h1, z1]]);
    }
    Ground::new(t, 8.0)
}

/// Crest shapes: a gentle hill top, a sharp crest and a kicker (short ramp, then a 1.6 m drop).
fn crest_height(kind: &str, d: f32) -> f32 {
    let d = d - 100.0;
    match kind {
        "hill" => if d < 0.0 { 0.0 } else if d < 40.0 { d * 0.08 } else { 3.2 - (d - 40.0) * 0.08 },
        "crest" => if d < 0.0 { 0.0 } else if d < 30.0 { d * 0.12 } else { 3.6 - (d - 30.0) * 0.18 },
        _ => if (0.0..8.0).contains(&d) { d * 0.2 } else { 0.0 },
    }
}

/// Drives over a crest at a speed: (airtime s, highest gap above the road m, worst landing speed
/// into the road m/s, metres flown).
fn crest_run(base: &Car, kind: &str, kmh: f32) -> (f32, f32, f32, f32) {
    let ground = profile_ground(|d| crest_height(kind, d));
    let walls = WallSet::new(vec![], 8.0);
    let mut car = base.reset(Vec3::new(0.0, nfspu_game::car::RIDE_HEIGHT, 0.0), 0.0);
    let target = kmh / 3.6;
    car.set_speed(target);
    let gear = car.sim.gear_for_speed(target, 0.75);
    car.sim.put_in_gear(gear, target);
    let dt = 1.0 / 64.0;
    let (mut air, mut gap, mut land, mut flown) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    let mut was_air = false;
    for _ in 0..(8.0 * 64.0) as usize {
        let input = Input { throttle: car.speed < target, ..Default::default() };
        let before = car.pos;
        car.step(input, dt, &ground, &walls);
        let road = crest_height(kind, -car.pos.z) + nfspu_game::car::RIDE_HEIGHT;
        if !car.on_ground {
            air += dt;
            flown += (car.pos - before).length();
            gap = gap.max(car.pos.y - road);
        } else if was_air {
            land = land.max(car.landing);
        }
        if !car.on_ground {
            land = land.max(0.0);
        }
        was_air = !car.on_ground;
        let _ = before;
    }
    (air, gap, land, flown)
}

fn main() {
    let game = PathBuf::from(env::var("NFSPU_GAME_DIR").expect("set NFSPU_GAME_DIR"));
    let cars = car_load::catalogue(&game);
    let mut ids: Vec<u16> = env::args().skip(1).filter_map(|a| a.parse().ok()).collect();
    if ids.is_empty() {
        for w in ["356 1100 Coup", "911 Carrera Coupé (993)", "'87 959"] {
            if let Some(c) = cars.iter().find(|c| c.kind == 0 && c.name.contains(w)) {
                ids.push(c.id);
            }
        }
    }
    let ground = flat();
    let walls = WallSet::new(vec![], 8.0);
    let mut out = String::new();
    let _ = writeln!(out, "handling check: keyboard inputs, flat dry asphalt; spin = slip > {SPIN_SLIP_DEG} deg or yaw rate > {SPIN_YAW} rad/s\n");
    let (mut spins, mut slides, mut runs) = (0, 0, 0);
    let mut track_spins = 0;
    for id in ids {
        let Some(c) = cars.iter().find(|c| c.id == id) else { continue };
        let l = match car_load::load(&game, Pick::Id(id), false) {
            Ok(l) => l,
            Err(e) => {
                let _ = writeln!(out, "{}: {e}", c.name);
                continue;
            }
        };
        let base = Car::with_spec(Vec3::ZERO, 0.0, l.spec, Aids::default());
        let _ = writeln!(out, "== {}", l.note);
        for kmh in [100.0, 150.0] {
            for m in MANOEUVRES {
                let r = run(&base, kmh, m, &ground, &walls);
                runs += 1;
                spins += r.spun as usize;
                slides += r.slid as usize;
                let _ = writeln!(
                    out,
                    "   {kmh:>3.0} km/h {:<28} max slip {:5.1} deg, max yaw rate {:4.2} rad/s  {}",
                    m.name,
                    r.max_slip,
                    r.max_yaw,
                    if r.spun { "SPIN" } else if r.slid { "slide" } else { "ok" }
                );
            }
        }
        for kind in ["hill", "crest", "kicker"] {
            for kmh in [100.0, 150.0] {
                let (air, gap, land, flown) = crest_run(&base, kind, kmh);
                let _ = writeln!(out, "   {kind:<6} {kmh:>3.0} km/h: airborne {air:.2} s over {flown:.1} m, highest {gap:.2} m above the road, landing {land:.1} m/s");
            }
        }
        if env::var_os("HANDLING_FLAT_ONLY").is_none() {
            let (mut ts, mut tw, mut td) = (0, 0, 0.0);
            for name in TRACKS {
                if let Some((sp, wh, d, notes)) = track_run(&base, name, 90.0) {
                    let _ = writeln!(out, "   track {name:<9} 90 s keyboard driver: {sp} spins, {wh} wall hits, {:.1} km", d / 1000.0);
                    for n in notes {
                        let _ = writeln!(out, "      spin {n}");
                    }
                    ts += sp;
                    tw += wh;
                    td += d;
                }
            }
            track_spins += ts;
            let _ = writeln!(out, "   tracks: {ts} spins, {tw} wall hits in {:.1} km", td / 1000.0);
        }
        let _ = writeln!(out);
    }
    let _ = writeln!(out, "total: {spins} spins, {slides} slides (over {SLIDE_DEG} deg) in {runs} flat runs; {track_spins} spins on tracks");
    let logs = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("logs");
    let _ = fs::create_dir_all(&logs);
    let name = env::var("HANDLING_LOG").unwrap_or_else(|_| "handling_check.txt".into());
    fs::write(logs.join(name), &out).unwrap();
    print!("{out}");
}
