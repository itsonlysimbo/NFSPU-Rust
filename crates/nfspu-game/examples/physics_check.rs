//! Drives retail cars through scripted tests on flat ground and compares them with the game's own
//! showroom numbers (`FEData/Locale/<car>.loc`: 0-60 mph time and top speed):
//! - standing start (revved on the grid, then full throttle): 0-60 mph, 0-100 km/h, 0-160 km/h, quarter mile, top speed;
//! - braking from 100 km/h with and without ABS;
//! - steady cornering on a 50 m circle (highest speed that holds the line, lateral g);
//! - the automatic's shift points.
//! Run: `cargo run -p nfspu-game --example physics_check --release [car ids...]` (needs
//! NFSPU_GAME_DIR). Without ids it checks a fixed list. Log: logs/physics_check.txt

use nfspu_game::car_load::{self, Pick};
use nfspu_game::car_sim::{Aids, CarSim, Contact, Controls, Spec, SpecOptions, TICK_HZ};
use std::fmt::Write as _;
use std::{env, fs, path::PathBuf};

const MPH: f32 = 0.447_04;
const KMH: f32 = 1.0 / 3.6;

struct Run {
    times: Vec<(f32, Option<f32>)>,
    quarter: Option<(f32, f32)>,
    top: f32,
    top_at: f32,
    shifts: Vec<(f32, i32, f32, f32)>,
}

/// Full throttle from a standstill for `seconds`, straight ahead.
fn launch(spec: &Spec, aids: Aids, seconds: f32, marks: &[f32]) -> Run {
    let mut sim = CarSim::new(spec.clone(), aids);
    let mut times: Vec<(f32, Option<f32>)> = marks.iter().map(|m| (*m, None)).collect();
    let (mut dist, mut quarter, mut top, mut top_at) = (0.0f32, None, 0.0f32, 0.0f32);
    let mut shifts = Vec::new();
    let mut gear = sim.gear();
    let dt = 1.0 / TICK_HZ;
    // on the grid: two seconds held at the line with the throttle down (the engine revs in
    // neutral), then the start
    sim.held = true;
    for _ in 0..(2.0 * TICK_HZ) as usize {
        sim.tick(0.0, Controls { throttle: 1.0, ..Default::default() }, Contact::default());
    }
    sim.held = false;
    let trace = env::var_os("NFSPU_TRACE").is_some() && aids.automatic;
    for i in 0..(seconds * TICK_HZ) as usize {
        let t = i as f32 * dt;
        // a manual driver shifts up at 97% of the redline and engages first at once
        let want_up = !aids.automatic && (sim.gear() == 0 || (sim.drive.rpm > spec.redline * 0.97 && sim.drive.shift_left == 0));
        // hold the button for two force ticks, then let go for two
        let press = want_up && (i / 4) % 2 == 0;
        let ctl = Controls { throttle: 1.0, gear_up: press, ..Default::default() };
        sim.tick(0.0, ctl, Contact::default());
        let v = sim.forward_speed(0.0);
        dist += v * dt;
        for m in times.iter_mut() {
            if m.1.is_none() && v >= m.0 {
                m.1 = Some(t + dt);
            }
        }
        if quarter.is_none() && dist >= 402.336 {
            quarter = Some((t + dt, v));
        }
        if v > top + 0.01 {
            top = v;
            top_at = t;
        }
        if trace && i % 16 == 0 && t < 12.0 {
            let w = &sim.wheels;
            println!(
                "  t {t:5.2} v {:5.1} km/h gear {} rpm {:5.0} shift {} a_fwd {:5.2} loads {:.2} {:.2} {:.2} {:.2} long {:.2} {:.2} {:.2} {:.2} slide {:.2} {:.2}",
                v * 3.6, sim.gear(), sim.drive.rpm, sim.drive.shift_left, sim.accel()[1],
                w[0].load, w[1].load, w[2].load, w[3].load, w[0].force[1], w[1].force[1], w[2].force[1], w[3].force[1], w[2].slide, w[3].slide
            );
        }
        if sim.gear() != gear {
            shifts.push((t, sim.gear(), sim.drive.rpm, v));
            gear = sim.gear();
        }
    }
    Run { times, quarter, top, top_at, shifts }
}

/// Brakes from `from` m/s in top-ish gear until stopped: (distance, time, locked ticks).
fn stop(spec: &Spec, aids: Aids, from: f32) -> (f32, f32, usize) {
    let mut sim = CarSim::new(spec.clone(), aids);
    sim.vel = [0.0, -from];
    sim.drive.slot = (spec.forward_gears + 1).min(4);
    sim.drive.rpm = from * spec.rpm_per_ms[sim.drive.slot];
    let (mut dist, mut t, mut locked) = (0.0f32, 0.0f32, 0usize);
    let dt = 1.0 / TICK_HZ;
    while sim.forward_speed(0.0) > 0.05 && t < 20.0 {
        sim.tick(0.0, Controls { brake: 1.0, ..Default::default() }, Contact::default());
        dist += sim.forward_speed(0.0).max(0.0) * dt;
        t += dt;
        locked += sim.wheels.iter().filter(|w| w.locked).count();
    }
    (dist, t, locked)
}

/// Holds a circle of radius `r` at a target speed with a simple steering controller; returns the
/// lateral acceleration reached and whether the car held the line (radius error under 15%).
fn circle(spec: &Spec, aids: Aids, r: f32, speed: f32) -> (f32, bool, f32) {
    let mut sim = CarSim::new(spec.clone(), aids);
    let (mut x, mut z, mut yaw) = (r, 0.0f32, 0.0f32);
    sim.vel = [0.0, -speed];
    sim.drive.slot = 3;
    let dt = 1.0 / TICK_HZ;
    let mut worst = 0.0f32;
    let mut lat = 0.0f32;
    for i in 0..(20.0 * TICK_HZ) as usize {
        // centre at the origin, driving counter-clockwise seen from above (turning left)
        let dist = (x * x + z * z).sqrt();
        let err = dist - r;
        let tangent = [z / dist, -x / dist]; // travel direction on a left-hand circle round the origin
        let h = [-yaw.sin(), -yaw.cos()];
        let heading_err = h[0] * tangent[1] - h[1] * tangent[0];
        let steer = (0.9 * spec.wheelbase / r + 0.08 * err - 0.8 * heading_err).clamp(-1.0, 1.0);
        let v = sim.forward_speed(yaw);
        let throttle = ((speed - v) * 0.5 + 0.35).clamp(0.0, 1.0);
        sim.tick(yaw, Controls { throttle, steer, ..Default::default() }, Contact::default());
        yaw += sim.yaw_rate * dt;
        x += sim.vel[0] * dt;
        z += sim.vel[1] * dt;
        if i as f32 > 8.0 * TICK_HZ {
            worst = worst.max((err / r).abs());
            lat = lat.max(sim.accel()[0].abs());
        }
    }
    let v = sim.speed();
    (lat / 9.8, worst < 0.15, v)
}

/// The showroom numbers from the car's locale file (named by the catalogue text at 0x4d,
/// `1987959Coupe29`): 0-60 mph seconds, top speed mph, weight text.
fn loc_specs(game: &PathBuf, long_name: &str) -> (Option<f32>, Option<f32>, String) {
    let want = format!("{long_name}.loc").to_ascii_lowercase();
    let file = fs::read_dir(game.join("FEData").join("Locale"))
        .ok()
        .and_then(|rd| rd.filter_map(|e| e.ok()).find(|e| e.file_name().to_string_lossy().to_ascii_lowercase() == want).map(|e| e.path()));
    let Some(map) = file.and_then(|f| fs::read(f).ok()).and_then(|d| nfspu_formats::loc::parse(&d, 0)) else { return (None, None, String::new()) };
    // the strings in file order: 7 is the 0-60 time, 8 the top speed, 11 the weight
    let strings: Vec<&String> = map.values().collect();
    let num = |s: Option<&&String>| s.and_then(|s| s.split_whitespace().next()).and_then(|w| w.parse::<f32>().ok());
    (num(strings.get(7)), num(strings.get(8)), strings.get(11).map(|s| s.to_string()).unwrap_or_default())
}

fn main() {
    let game = PathBuf::from(env::var("NFSPU_GAME_DIR").expect("set NFSPU_GAME_DIR"));
    let raw = fs::read(game.join("FEData").join("Data").join("nfs5.car")).expect("nfs5.car");
    let mut long_names = std::collections::HashMap::new();
    for r in raw.chunks_exact(nfspu_game::catalog::CAR_RECORD) {
        let t = &r[0x4d..0x7f];
        let end = t.iter().position(|b| *b == 0).unwrap_or(t.len());
        long_names.insert(u16::from_le_bytes([r[0], r[1]]), String::from_utf8_lossy(&t[..end]).to_string());
    }
    let cars = car_load::catalogue(&game);
    let mut ids: Vec<u16> = env::args().skip(1).filter_map(|a| a.parse().ok()).collect();
    if ids.is_empty() {
        // '50 356 1100 coupe, '95 993 Carrera 4, '95 993 Turbo, '87 959, '95 911 GT2, and a few
        // more from each era
        let wanted = ["356 1100 Coup", "911 Carrera 4 Coup", "911 Turbo 3.6 Coup", "Carrera 4 Coupé (993)", "Turbo 3.6 Coupé (993)", "'87 959", "911 GT2", "'73 911 Carrera RS", "Boxster", "550"];
        for w in wanted {
            if let Some(c) = cars.iter().find(|c| c.kind == 0 && c.name.contains(w)) {
                ids.push(c.id);
            }
        }
    }
    let mut out = String::new();
    let _ = writeln!(out, "physics check: flat dry asphalt, no wind, retail model at 64/32 Hz; showroom = the car's .loc specs\n");
    for id in ids {
        let Some(car) = cars.iter().find(|c| c.id == id) else { continue };
        let (zero_sixty, top_mph, mass) = loc_specs(&game, long_names.get(&id).map_or("", |s| s.as_str()));
        let loaded = match car_load::load(&game, Pick::Id(id), false) {
            Ok(l) => l,
            Err(e) => {
                let _ = writeln!(out, "{}: {e}", car.name);
                continue;
            }
        };
        let (sim, _) = car_load::read_sim(&game, &loaded.sim_file).unwrap();
        let manual_spec = Spec::from_sim(&sim, SpecOptions::for_car(&car.ai_name, true));
        let spec = loaded.spec;
        let _ = writeln!(out, "== {} (id {id}), {}", car.name, loaded.note);
        let _ = writeln!(
            out,
            "   showroom: 0-60 mph {} s, top speed {} mph, weight {mass}",
            zero_sixty.map_or("?".into(), |v| format!("{v:.1}")),
            top_mph.map_or("?".into(), |v| format!("{v:.0}"))
        );
        let marks = [60.0 * MPH, 100.0 * KMH, 160.0 * KMH, 200.0 * KMH];
        for (label, s, aids) in [
            ("automatic", &spec, Aids { automatic: true, ..Aids::default() }),
            ("manual   ", &manual_spec, Aids { automatic: false, ..Aids::default() }),
        ] {
            let run = launch(s, aids, 150.0, &marks);
            let fmt = |t: Option<f32>| t.map_or("  -  ".into(), |t| format!("{t:5.2}"));
            let _ = writeln!(
                out,
                "   {label}: 0-60 mph {} s, 0-100 {} s, 0-160 {} s, 0-200 {} s, 1/4 mile {}, top {:.1} km/h ({:.1} mph) after {:.0} s",
                fmt(run.times[0].1),
                fmt(run.times[1].1),
                fmt(run.times[2].1),
                fmt(run.times[3].1),
                run.quarter.map_or("-".into(), |(t, v)| format!("{t:.2} s at {:.0} km/h", v * 3.6)),
                run.top * 3.6,
                run.top / MPH,
                run.top_at
            );
            if label.starts_with("auto") {
                let shifts: Vec<String> = run.shifts.iter().take(8).map(|(t, g, rpm, v)| format!("{t:.1}s->{g} at {:.0} km/h {rpm:.0} rpm", v * 3.6)).collect();
                let _ = writeln!(out, "   automatic shifts: {}", shifts.join(", "));
            }
            if let (Some(spec060), Some(t)) = (zero_sixty, run.times[0].1) {
                let _ = writeln!(out, "     vs showroom 0-60: {:+.0}%", (t / spec060 - 1.0) * 100.0);
            }
            if let Some(top) = top_mph {
                let _ = writeln!(out, "     vs showroom top speed: {:+.1}%", (run.top / MPH / top - 1.0) * 100.0);
            }
        }
        for abs in [true, false] {
            let (d, t, locked) = stop(&spec, Aids { abs, ..Aids::default() }, 100.0 * KMH);
            let _ = writeln!(out, "   braking 100-0 km/h, ABS {}: {d:.1} m in {t:.2} s ({:.2} g average), locked wheel-ticks {locked}", if abs { "on " } else { "off" }, (100.0 * KMH) / t / 9.8);
        }
        // cornering: the fastest of these speeds that still holds a 50 m circle
        let mut best = None;
        for kmh in (40..=160).step_by(5) {
            let (g, held, v) = circle(&spec, Aids::default(), 50.0, kmh as f32 * KMH);
            if held {
                best = Some((kmh, g, v));
            }
        }
        match best {
            Some((kmh, g, v)) => {
                let _ = writeln!(out, "   50 m circle: holds {kmh} km/h (ends at {:.0} km/h), {g:.2} g lateral (grip {:.2})", v * 3.6, spec.grip);
            }
            None => {
                let _ = writeln!(out, "   50 m circle: did not hold even 40 km/h");
            }
        }
        let _ = writeln!(out);
    }
    let logs = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("logs");
    let _ = fs::create_dir_all(&logs);
    fs::write(logs.join("physics_check.txt"), &out).unwrap();
    print!("{out}");
}
