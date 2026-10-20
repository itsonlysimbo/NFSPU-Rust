//! Audits the decoded weather tables in the track sky `.bin` files, offline.
//!
//! `GameData/Track/Sky/<track>.bin` holds a 0..1 blend table at +0xddc (400 records of
//! `i32 slice, f32 value`) and a precipitation table at +0x1a5c (400 records of 0x30 bytes,
//! `i32 slice, f32 rain, f32 snow, ..., u32 rain colour, u32 snow colour, three f32`). The layout
//! is in `docs/provenance/formats.md`. This check reads every shipped track, compares the keys
//! with the track's slice count and prints the colour/precipitation ranges.
//!
//! Run: `$env:NFSPU_GAME_DIR="..."; cargo run -p nfspu-game --example sky_bin -j 2`
//! (or pass a track name: `... -- alps`). Read-only.

use nfspu_game::{race_path, sky};
use std::path::PathBuf;

fn main() {
    let game = PathBuf::from(std::env::var("NFSPU_GAME_DIR").expect("set NFSPU_GAME_DIR"));
    let sky_dir = game.join("GameData").join("Track").join("Sky");
    let only = std::env::args().nth(1);
    let mut names: Vec<String> = std::fs::read_dir(&sky_dir)
        .expect("read the Sky folder")
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            (p.extension()? == "bin").then(|| p.file_stem().unwrap().to_string_lossy().to_string())
        })
        .filter(|n| only.as_ref().map_or(true, |o| o.eq_ignore_ascii_case(n)))
        .collect();
    names.sort();
    assert!(!names.is_empty(), "no sky .bin files found");

    let mut with_tables = 0;
    for name in &names {
        let Some(data) = sky::load(&game, name) else {
            println!("{name:<12} no sky folder entry");
            continue;
        };
        let slices = race_path::load(name, race_path::Direction::Forward).ok().map(|r| r.path.slices.len()).unwrap_or(0);
        let first = |keys: &[i32]| keys.first().copied();
        let last = |keys: &[i32]| keys.last().copied();
        let blend: Vec<i32> = data.blend.iter().map(|k| k.slice).collect();
        let weather: Vec<i32> = data.weather.iter().map(|k| k.slice).collect();
        check_sorted("blend", &blend, slices, name);
        check_sorted("weather", &weather, slices, name);
        if data.blend.is_empty() && data.weather.is_empty() {
            println!("{name:<12} no tables (short or flat-colour .bin, {} bytes)", std::fs::metadata(sky_dir.join(format!("{name}.bin"))).map(|m| m.len()).unwrap_or(0));
            continue;
        }
        with_tables += 1;
        let rain = range(data.weather.iter().map(|k| k.rain));
        let snow = range(data.weather.iter().map(|k| k.snow));
        let cold = data.weather.iter().filter(|k| sky::unpack_colour(k.rain_colour)[2] > sky::unpack_colour(k.rain_colour)[0]).count();
        println!(
            "{name:<12} {slices:>5} slices | blend {} keys {}..{} | weather {} keys {}..{} | rain {} | snow {} | {cold}/{} cold colours",
            data.blend.len(),
            fmt(first(&blend)),
            fmt(last(&blend)),
            data.weather.len(),
            fmt(first(&weather)),
            fmt(last(&weather)),
            rain,
            snow,
            data.weather.len()
        );
    }
    println!("\n{} of {} sky sets carry weather tables", with_tables, names.len());

    // With a track name, also show how the sampling moves along its lap: the warm/cold sweep and the
    // precipitation the sky system will use.
    if let Some(name) = only {
        let data = sky::load(&game, &name).expect("sky entry");
        let slices = race_path::load(&name, race_path::Direction::Forward).map(|r| r.path.slices.len()).unwrap_or(0);
        println!("\n{name}: sampled every {} of {slices} slices", (slices / 12).max(1));
        for slice in (0..slices).step_by((slices / 12).max(1)) {
            let blend = sky::sample_blend(&data.blend, slice, slices).unwrap_or(0.0);
            match sky::sample_weather(&data.weather, slice, slices) {
                Some(w) => {
                    let c = w.colour();
                    println!(
                        "  {slice:5}  blend {blend:.2}  rain {:.2} snow {:.2}  rgb {:3},{:3},{:3}",
                        w.rain,
                        w.snow,
                        (c[0] * 255.0).round(),
                        (c[1] * 255.0).round(),
                        (c[2] * 255.0).round()
                    );
                }
                None => println!("  {slice:5}  blend {blend:.2}  no weather table"),
            }
        }
    }
}

fn fmt(v: Option<i32>) -> String {
    v.map_or_else(|| "-".to_string(), |v| v.to_string())
}

fn range(values: impl Iterator<Item = f32>) -> String {
    let v: Vec<f32> = values.filter(|f| f.is_finite()).collect();
    match (v.iter().copied().reduce(f32::min), v.iter().copied().reduce(f32::max)) {
        (Some(a), Some(b)) => format!("{a:.3}..{b:.3}"),
        _ => "none".to_string(),
    }
}

/// The tables must be sorted and, when the track's slice count is known, stay inside the lap.
fn check_sorted(what: &str, keys: &[i32], slices: usize, track: &str) {
    for pair in keys.windows(2) {
        assert!(pair[0] <= pair[1], "{track}: {what} keys are not sorted at {pair:?}");
    }
    if let (Some(first), Some(last), true) = (keys.first(), keys.last(), slices > 0) {
        assert!(*first >= 0 && (*last as usize) < slices, "{track}: {what} key {last} outside the {slices} slices");
    }
    assert!(keys.len() <= sky::TABLE_RECORDS, "{track}: {what} has more than {} records", sky::TABLE_RECORDS);
}
