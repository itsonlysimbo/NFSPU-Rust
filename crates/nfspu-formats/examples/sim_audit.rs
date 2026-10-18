//! Reads every car simulation file (`GameData/Simulation/CarData/*.sim`) from the player's install
//! and logs the numbers, one block per car, plus the ranges of each field.
//! Run: `cargo run -p nfspu-formats --example sim_audit` (needs NFSPU_GAME_DIR). Log: logs/sim_audit.txt

use nfspu_formats::carsim;
use std::fmt::Write as _;
use std::{env, fs, path::PathBuf};

fn main() {
    let game = PathBuf::from(env::var("NFSPU_GAME_DIR").expect("set NFSPU_GAME_DIR"));
    let dir = game.join("GameData").join("Simulation").join("CarData");
    let mut files: Vec<_> = fs::read_dir(&dir)
        .expect("CarData folder")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("sim")))
        .collect();
    files.sort();
    let mut out = String::new();
    let mut sims = Vec::new();
    for f in &files {
        let stem = f.file_stem().unwrap().to_string_lossy().to_string();
        let raw = fs::read(f).unwrap();
        match carsim::parse(&raw) {
            Ok(s) => sims.push((stem, raw.len(), s)),
            Err(e) => {
                let _ = writeln!(out, "{stem}: ERROR {e}");
            }
        }
    }
    let _ = writeln!(out, "{} .sim files in {}\n", sims.len(), dir.display());
    for (stem, len, s) in &sims {
        let (peak, at) = s.peak_torque();
        let gears: Vec<String> = s.ratios[2..2 + s.forward_gears].iter().map(|r| format!("{r:.3}")).collect();
        let _ = writeln!(out, "{stem} ({len} bytes) \"{}\"", s.name);
        let _ = writeln!(
            out,
            "  mass {:.0} kg, wheelbase {:.0} mm, {:?}, front drive {:.0}%, front weight {:.1}%",
            s.mass_kg, s.wheelbase_mm, s.induction, s.front_drive_pct, s.front_weight_pct
        );
        let _ = writeln!(
            out,
            "  engine: redline {:.0} rpm, peak {:.0} lb-ft at {:.0} rpm, torque scale {:.0}%, engine brake {:.0}%",
            s.redline_rpm, peak, at, s.torque_scale_pct, s.engine_brake_pct
        );
        let _ = writeln!(
            out,
            "  gears ({}): R {:.3}, {} ; final {:.3}; shift {} ticks",
            s.forward_gears,
            s.ratios[0],
            gears.join(" "),
            s.final_drive,
            s.shift_ticks
        );
        let _ = writeln!(
            out,
            "  tyre {:.0}/{:.0} R{:.0} (circumference {:.3} m), grip {:.2}, steer lock {:.1}/256 turn, toe {}",
            s.tyre_width_mm,
            s.tyre_aspect_pct,
            s.rim_in,
            s.tyre_circumference_m(),
            s.grip,
            s.steer_lock,
            s.toe
        );
        let _ = writeln!(
            out,
            "  brakes {:.1} m/s^2, front share {:.0}%; top speed {:.1} mph; transfer height {:.0}% stiffness {:.2} split {:.2}",
            s.brake_decel, s.front_brake_pct, s.top_speed_mph, s.transfer_height_pct, s.transfer_stiffness, s.lateral_split
        );
        let _ = writeln!(
            out,
            "  downforce {:.2}/{:.2}, suspension {:.1}%/{:.1}% {:?}, unknown_48 {}, tail {:?}",
            s.downforce_front, s.downforce_rear, s.suspension_front_pct, s.suspension_rear_pct, s.suspension, s.unknown_48, s.tail
        );
        let torque: Vec<String> = s.torque_lbft.iter().map(|t| format!("{t:.0}")).collect();
        let _ = writeln!(out, "  torque/500rpm: {}\n", torque.join(" "));
    }
    // ranges per field, to see which ones vary at all
    let fields: [(&str, fn(&carsim::SimFile) -> f32); 18] = [
        ("mass_kg", |s| s.mass_kg),
        ("wheelbase_mm", |s| s.wheelbase_mm),
        ("shift_ticks", |s| s.shift_ticks as f32),
        ("torque_scale_pct", |s| s.torque_scale_pct),
        ("front_drive_pct", |s| s.front_drive_pct),
        ("top_speed_mph", |s| s.top_speed_mph),
        ("redline_rpm", |s| s.redline_rpm),
        ("engine_brake_pct", |s| s.engine_brake_pct),
        ("front_weight_pct", |s| s.front_weight_pct),
        ("grip", |s| s.grip),
        ("transfer_height_pct", |s| s.transfer_height_pct),
        ("steer_lock", |s| s.steer_lock),
        ("toe", |s| s.toe),
        ("brake_decel", |s| s.brake_decel),
        ("front_brake_pct", |s| s.front_brake_pct),
        ("transfer_stiffness", |s| s.transfer_stiffness),
        ("lateral_split", |s| s.lateral_split),
        ("downforce_front", |s| s.downforce_front),
    ];
    let _ = writeln!(out, "field ranges over {} files:", sims.len());
    for (name, get) in fields {
        let vals: Vec<f32> = sims.iter().map(|(_, _, s)| get(s)).collect();
        let lo = vals.iter().cloned().fold(f32::MAX, f32::min);
        let hi = vals.iter().cloned().fold(f32::MIN, f32::max);
        let _ = writeln!(out, "  {name:<20} {lo:>9.3} .. {hi:>9.3}");
    }
    let tails: std::collections::BTreeSet<String> = sims.iter().map(|(_, _, s)| format!("{:?}", s.tail)).collect();
    let _ = writeln!(out, "  distinct tail values: {tails:?}");
    let logs = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("logs");
    let _ = fs::create_dir_all(&logs);
    fs::write(logs.join("sim_audit.txt"), &out).unwrap();
    print!("{out}");
}
