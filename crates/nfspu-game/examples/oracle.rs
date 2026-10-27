//! The behaviour oracle runner (`docs/ORACLE.md`): runs every case in `oracle/cases/*.toml`
//! headless, compares with its reference and ends with `ORACLE PASS` or `ORACLE FAIL`.
//!
//! Two kinds of case:
//! - `format`: a value read from the player's install checked against a number written down in
//!   `docs/provenance/` (the `source` key names where).
//! - `sim`: a scripted drive on flat ground in our car sim, compared with `oracle/ref/<name>.csv`.
//!   While `reference = "self"` the CSV is our own earlier trace (a regression check, **not**
//!   retail); retail traces will say `reference = "retail"`.
//!
//! Run: `cargo run -p nfspu-game --example oracle` (needs NFSPU_GAME_DIR).
//! `ORACLE_RECORD=1` (re)writes the self-reference CSVs of the cases it runs;
//! `ORACLE_UPDATE=1` rewrites `oracle/verified.txt` from the cases that passed.
//! A case listed in `verified.txt` that fails is a REGRESSION. Log: logs/oracle.txt.

use bevy::math::Vec3;
use nfspu_game::car::{Car, Input};
use nfspu_game::car_load::{self, Pick};
use nfspu_game::car_sim::Aids;
use nfspu_game::collision::WallSet;
use nfspu_game::ground::{Ground, Tri};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// A case file: flat `key = value` lines (strings in quotes, numbers, `["a", "b"]` lists).
struct Case {
    file: String,
    keys: BTreeMap<String, String>,
}

impl Case {
    fn parse(file: &str, text: &str) -> Case {
        let mut keys = BTreeMap::new();
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if let Some((k, v)) = line.split_once('=') {
                keys.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
        Case { file: file.to_string(), keys }
    }

    fn str(&self, k: &str) -> Option<String> {
        self.keys.get(k).map(|v| v.trim_matches('"').to_string())
    }

    fn num(&self, k: &str) -> Option<f64> {
        self.keys.get(k).and_then(|v| v.parse().ok())
    }

    fn list(&self, k: &str) -> Vec<String> {
        self.keys.get(k).map(|v| v.trim_matches(|c| c == '[' || c == ']').split(',').map(|s| s.trim().trim_matches('"').to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default()
    }

    fn name(&self) -> String {
        self.str("name").unwrap_or_else(|| self.file.clone())
    }
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn game_dir() -> PathBuf {
    PathBuf::from(std::env::var("NFSPU_GAME_DIR").unwrap_or_default())
}

type Outcome = Result<String, String>;

fn close(label: &str, got: f64, want: f64, tol: f64) -> Outcome {
    if (got - want).abs() <= tol {
        Ok(format!("{label} {got} (want {want} +-{tol})"))
    } else {
        Err(format!("{label} {got} but want {want} +-{tol}"))
    }
}

fn load_spec(case: &Case) -> Result<nfspu_game::car_sim::Spec, String> {
    let model = case.str("car").ok_or("no car")?;
    let manual = case.str("manual").as_deref() == Some("true");
    Ok(car_load::load(&game_dir(), Pick::Model(&model), manual)?.spec)
}

fn format_case(case: &Case) -> Outcome {
    let game = game_dir();
    let tol = case.num("tol").unwrap_or(0.0);
    let expect = case.num("expect").ok_or("no expect")?;
    let track = case.str("track").unwrap_or_default();
    match case.str("check").ok_or("no check")?.as_str() {
        "front_weight" => close("front weight share", load_spec(case)?.front_weight as f64, expect, tol),
        // the lightest and heaviest car in nfs5.car (`expect` is the lightest, `expect_max` the heaviest)
        "mass_range" => {
            let (mut lo, mut hi, mut n) = (f64::MAX, 0.0f64, 0);
            for c in car_load::catalogue(&game) {
                if let Ok(l) = car_load::load(&game, Pick::Id(c.id), false) {
                    lo = lo.min(l.spec.mass as f64);
                    hi = hi.max(l.spec.mass as f64);
                    n += 1;
                }
            }
            let max = case.num("expect_max").ok_or("no expect_max")?;
            close("lightest mass", lo, expect, tol)?;
            close("heaviest mass", hi, max, tol)?;
            Ok(format!("{n} cars, {lo:.0}..{hi:.0} kg"))
        }
        "map_header_count" | "map_sections" => {
            let text = std::fs::read_to_string(game.join("GameData").join("Track").join(format!("{track}.map"))).map_err(|e| format!("{track}.map: {e}"))?;
            if case.str("check").as_deref() == Some("map_sections") {
                close("[sectionN] blocks", text.lines().filter(|l| l.trim().starts_with("[section")).count() as f64, expect, tol)
            } else {
                let c = text.lines().find_map(|l| l.trim().strip_prefix("count=")).and_then(|v| v.trim().parse::<f64>().ok()).ok_or("no count= line")?;
                close("header count", c, expect, tol)
            }
        }
        "file_size" => {
            let rel = case.str("file").ok_or("no file")?;
            let len = std::fs::metadata(game.join(&rel)).map_err(|e| format!("{rel}: {e}"))?.len();
            close(&format!("{rel} bytes"), len as f64, expect, tol)
        }
        "track_count" => {
            let files = nfspu_game::catalog::file_names(&game.join("GameData").join("Track"));
            let n = files.iter().filter(|f| f.ends_with(".crp")).count();
            close(".crp track files", n as f64, expect, tol)
        }
        other => Err(format!("unknown check '{other}'")),
    }
}

fn flat() -> Ground {
    let (a, b) = (-20000.0f32, 20000.0f32);
    let t: Vec<Tri> = vec![[[a, 0.0, a], [b, 0.0, a], [a, 0.0, b]], [[b, 0.0, a], [b, 0.0, b], [a, 0.0, b]]];
    Ground::new(t, 256.0)
}

const HEADER: &str = "tick,speed_kmh,rpm,gear,x,z,yaw";

/// Drives the case's script and returns the sampled rows.
fn drive(case: &Case) -> Result<Vec<[f64; 7]>, String> {
    let spec = load_spec(case)?;
    let ticks = case.num("ticks").ok_or("no ticks")? as u32;
    let every = case.num("sample_every").unwrap_or(64.0) as u32;
    // script entries "from_tick,throttle,brake,steer": inputs from that tick on
    let mut script: Vec<(u32, bool, bool, f32)> = Vec::new();
    for s in case.list("script") {
        let p: Vec<&str> = s.split(':').collect();
        if p.len() != 4 {
            return Err(format!("bad script entry '{s}' (want from:throttle:brake:steer)"));
        }
        script.push((p[0].parse().map_err(|_| "bad tick")?, p[1] == "1", p[2] == "1", p[3].parse().map_err(|_| "bad steer")?));
    }
    let ground = flat();
    let walls = WallSet::new(Vec::new(), 4.0);
    let mut car = Car::with_spec(Vec3::new(0.0, nfspu_game::car::RIDE_HEIGHT, 0.0), 0.0, spec, Aids::default());
    let dt = 1.0 / 64.0;
    let mut rows = Vec::new();
    for t in 0..=ticks {
        if t % every == 0 {
            rows.push([t as f64, car.speed as f64 * 3.6, car.rpm() as f64, car.gear() as f64, car.pos.x as f64, car.pos.z as f64, car.yaw as f64]);
        }
        let (_, throttle, brake, steer) = script.iter().rev().find(|s| s.0 <= t).copied().unwrap_or((0, false, false, 0.0));
        car.step(Input { throttle, brake, steer, ..Default::default() }, dt, &ground, &walls);
    }
    Ok(rows)
}

fn csv(case: &Case, rows: &[[f64; 7]]) -> String {
    let mut s = format!("# {} trace from OUR sim (reference = {}), NOT retail unless the case says retail\n{HEADER}\n", case.name(), case.str("reference").unwrap_or_default());
    for r in rows {
        let _ = writeln!(s, "{},{:.4},{:.2},{},{:.4},{:.4},{:.5}", r[0], r[1], r[2], r[3], r[4], r[5], r[6]);
    }
    s
}

fn read_csv(path: &Path) -> Result<Vec<[f64; 7]>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("no reference {}: {e}", path.display()))?;
    let mut rows = Vec::new();
    for line in text.lines().filter(|l| !l.starts_with('#') && !l.starts_with("tick")) {
        let v: Vec<f64> = line.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        if v.len() != 7 {
            return Err(format!("bad reference line '{line}'"));
        }
        rows.push([v[0], v[1], v[2], v[3], v[4], v[5], v[6]]);
    }
    Ok(rows)
}

fn sim_case(case: &Case, record: bool) -> Outcome {
    let rows = drive(case)?;
    // the same run twice must agree exactly, or the case cannot be a regression check
    if drive(case)? != rows {
        return Err("the sim gave different numbers on a repeat run".into());
    }
    let path = root().join("oracle").join("ref").join(format!("{}.csv", case.name()));
    if record {
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&path, csv(case, &rows)).map_err(|e| e.to_string())?;
        return Ok(format!("recorded {} rows to {}", rows.len(), path.display()));
    }
    let want = read_csv(&path)?;
    if want.len() != rows.len() {
        return Err(format!("{} rows but the reference has {}", rows.len(), want.len()));
    }
    let tol = [0.0, case.num("tol_speed_kmh").unwrap_or(1.0), case.num("tol_rpm").unwrap_or(100.0), 0.0, case.num("tol_pos").unwrap_or(0.5), case.num("tol_pos").unwrap_or(0.5), case.num("tol_yaw").unwrap_or(0.02)];
    let names = ["tick", "speed_kmh", "rpm", "gear", "x", "z", "yaw"];
    // `ref_gear_offset`: the retail trace holds the raw HUD gear byte (2 = first), our sim counts
    // 1 = first, so a retail case sets the difference (1) instead of editing the trace.
    let gear_offset = case.num("ref_gear_offset").unwrap_or(0.0);
    for (r, w) in rows.iter().zip(&want) {
        for c in 1..7 {
            let w_c = if c == 3 { w[c] - gear_offset } else { w[c] };
            if (r[c] - w_c).abs() > tol[c] {
                return Err(format!("tick {}: {} is {:.3} but the reference has {:.3} (tolerance {})", r[0], names[c], r[c], w_c, tol[c]));
            }
        }
    }
    let last = rows.last().unwrap();
    Ok(format!("{} rows match; end speed {:.1} km/h, rpm {:.0}, gear {}", rows.len(), last[1], last[2], last[3]))
}

fn main() {
    let record = std::env::var("ORACLE_RECORD").is_ok();
    let update = std::env::var("ORACLE_UPDATE").is_ok();
    let dir = root().join("oracle");
    let verified: Vec<String> = std::fs::read_to_string(dir.join("verified.txt")).unwrap_or_default().lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(String::from).collect();
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir.join("cases")).map(|d| d.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "toml")).collect()).unwrap_or_default();
    files.sort();
    let mut out = String::new();
    let (mut passed, mut failed) = (Vec::new(), 0);
    if game_dir().as_os_str().is_empty() {
        let _ = writeln!(out, "NFSPU_GAME_DIR is not set");
        failed += 1;
    }
    for f in &files {
        let case = Case::parse(&f.file_stem().unwrap().to_string_lossy(), &std::fs::read_to_string(f).unwrap_or_default());
        let name = case.name();
        if case.str("pending").as_deref() == Some("true") {
            let _ = writeln!(out, "PEND {name}: waiting for a reference");
            continue;
        }
        let r = match case.str("kind").as_deref() {
            Some("format") => format_case(&case),
            Some("sim") => sim_case(&case, record),
            other => Err(format!("unknown kind {other:?}")),
        };
        match r {
            Ok(m) => {
                let _ = writeln!(out, "PASS {name}: {m}");
                passed.push(name);
            }
            Err(m) => {
                failed += 1;
                let tag = if verified.contains(&name) { "FAIL (REGRESSION)" } else { "FAIL" };
                let _ = writeln!(out, "{tag} {name}: {m}");
            }
        }
    }
    for v in &verified {
        if !files.iter().any(|f| f.file_stem().unwrap().to_string_lossy() == v.as_str()) && !passed.contains(v) {
            failed += 1;
            let _ = writeln!(out, "FAIL (REGRESSION) {v}: listed in verified.txt but no such case");
        }
    }
    if update && failed == 0 {
        let mut text = String::from("# Cases that passed (ORACLE_UPDATE=1). A case leaving this list is a regression.\n");
        for p in &passed {
            text += p;
            text.push('\n');
        }
        let _ = std::fs::write(dir.join("verified.txt"), text);
        let _ = writeln!(out, "verified.txt rewritten with {} cases", passed.len());
    }
    let _ = writeln!(out, "{} passed, {failed} failed, {} verified", passed.len(), verified.len());
    print!("{out}");
    let logs = root().join("logs");
    let _ = std::fs::create_dir_all(&logs);
    let _ = std::fs::write(logs.join("oracle.txt"), &out);
    println!("{}", if failed == 0 { "ORACLE PASS" } else { "ORACLE FAIL" });
    if failed > 0 {
        std::process::exit(1);
    }
}
