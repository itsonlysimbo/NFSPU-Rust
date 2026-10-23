//! Body lean log: steady corners (several speeds, held steer) and hard braking from 150 km/h on
//! flat asphalt; prints lateral/longitudinal g against the drawn lean and pitch (degrees).
//! Run: `cargo run -p nfspu-game --example lean_check [car ids...]` (needs NFSPU_GAME_DIR).
//! Log: logs/lean_check.txt

use bevy::math::Vec3;
use nfspu_game::car::{Car, Input};
use nfspu_game::car_load::{self, Pick};
use nfspu_game::collision::WallSet;
use nfspu_game::ground::{Ground, Tri};
use std::fmt::Write as _;
use std::{env, fs, path::PathBuf};

fn flat() -> Ground {
    let (a, b) = (-9000.0f32, 9000.0f32);
    let t: Vec<Tri> = vec![[[a, 0.0, a], [b, 0.0, a], [a, 0.0, b]], [[b, 0.0, a], [b, 0.0, b], [a, 0.0, b]]];
    Ground::new(t, 64.0)
}

fn main() {
    let game = PathBuf::from(env::var("NFSPU_GAME_DIR").expect("set NFSPU_GAME_DIR"));
    let cars = car_load::catalogue(&game);
    let mut ids: Vec<u16> = Vec::new();
    for a in env::args().skip(1) {
        let w = a.to_lowercase();
        match a.parse() {
            Ok(id) => ids.push(id),
            Err(_) => ids.extend(cars.iter().filter(|c| c.kind == 0 && (c.name.to_lowercase().contains(&w) || c.short.to_lowercase().contains(&w))).take(1).map(|c| c.id)),
        }
    }
    if ids.is_empty() {
        for w in ["356 1100 Coup", "911 Carrera Coupé (993)", "'87 959"] {
            if let Some(c) = cars.iter().find(|c| c.kind == 0 && c.name.contains(w)) {
                ids.push(c.id);
            }
        }
    }
    let (ground, walls) = (flat(), WallSet::new(vec![], 8.0));
    let dt = 1.0 / 64.0;
    let mut out = String::from("body lean log: flat dry asphalt. g = acceleration / 9.8; lean/pitch are the drawn body angles (deg), sim = before the look scale\n");
    for id in ids {
        let Some(c) = cars.iter().find(|c| c.id == id) else { continue };
        let Ok(l) = car_load::load(&game, Pick::Id(id), false) else { continue };
        let base = Car::with_spec(Vec3::ZERO, 0.0, l.spec, Default::default());
        let _ = writeln!(out, "\n== {} (wheelbase {:.2}, spring {:.4}, damper {:.4})", c.name, base.sim.spec.wheelbase, base.sim.spec.lean_spring, base.sim.spec.lean_damper);
        for kmh in [60.0f32, 100.0, 150.0] {
            for steer in [0.3f32, 0.6, 1.0] {
                let mut car = base.reset(Vec3::new(0.0, nfspu_game::car::RIDE_HEIGHT, 0.0), 0.0);
                let target = kmh / 3.6;
                car.set_speed(target);
                let gear = car.sim.gear_for_speed(target, 0.75);
                car.sim.put_in_gear(gear, target);
                let mut peak = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
                for i in 0..(5.0 / dt) as usize {
                    let s = if i > 64 { steer } else { 0.0 };
                    car.step(Input { throttle: car.speed < target, steer: s, ..Default::default() }, dt, &ground, &walls);
                }
                let (sp, sr) = car.sim.body_lean();
                let (dp, dr) = car.lean_tilt();
                peak = (car.sim.accel()[0] / 9.8, sr.to_degrees(), dr.to_degrees(), dp.to_degrees());
                let _ = sp;
                let _ = writeln!(out, "corner {kmh:5.0} km/h steer {steer:.1}: lat {:+.2} g  roll sim {:+.2} drawn {:+.2}  pitch drawn {:+.2}", peak.0, peak.1, peak.2, peak.3);
            }
        }
        let mut car = base.reset(Vec3::new(0.0, nfspu_game::car::RIDE_HEIGHT, 0.0), 0.0);
        car.set_speed(150.0 / 3.6);
        let gear = car.sim.gear_for_speed(150.0 / 3.6, 0.75);
        car.sim.put_in_gear(gear, 150.0 / 3.6);
        for i in 0..(4.0 / dt) as usize {
            car.step(Input { brake: true, ..Default::default() }, dt, &ground, &walls);
            if i % 16 == 0 {
                let (sp, _) = car.sim.body_lean();
                let (dp, _) = car.lean_tilt();
                let _ = writeln!(out, "brake t {:.2}s v {:5.1} km/h: long {:+.2} g  pitch sim {:+.2} drawn {:+.2}", i as f32 * dt, car.speed * 3.6, car.sim.accel()[1] / 9.8, sp.to_degrees(), dp.to_degrees());
            }
        }
    }
    let _ = fs::create_dir_all("logs");
    let _ = fs::write("logs/lean_check.txt", &out);
    print!("{out}");
}
