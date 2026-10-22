//! Loads every catalogue car's race sounds from the install (engine set, `.spl`, opponent loops),
//! the countdown voice and each track's ambience, and writes counts to `logs/sound_check.txt`.
//! Run: `cargo run -p nfspu-game --example sound_check` with `NFSPU_GAME_DIR` set.

use nfspu_game::race_sound as rs;
use std::fmt::Write as _;
use std::path::PathBuf;

fn main() {
    let dir = PathBuf::from(std::env::var("NFSPU_GAME_DIR").expect("set NFSPU_GAME_DIR"));
    let mut out = String::new();
    let mut fails = 0;
    let cars = nfspu_game::car_load::catalogue(&dir);
    let opp = rs::find_file(&rs::sounds_dir(&dir), "zzzwzzz.viv").and_then(|p| rs::Archive::open(&p)).expect("zzzwzzz.viv");
    let mut seen = std::collections::BTreeSet::new();
    for c in &cars {
        match rs::load_engine(&dir, &c.sound, &c.sim) {
            Ok(e) => {
                if e.name != c.sound.to_ascii_lowercase() && e.name != c.sound {
                    let _ = writeln!(out, "{} '{}': sound '{}' missing, used {}", c.id, c.name, c.sound, e.name);
                }
                if seen.insert(e.name.clone()) {
                    let keys: Vec<u8> = e.layers.iter().map(|l| l.pcm.key).collect();
                    let _ = writeln!(out, "engine {}: {} layers, keys {keys:?}, shift sound {}", e.name, e.layers.len(), e.shift.is_some());
                }
                if e.layers.is_empty() {
                    fails += 1;
                }
            }
            Err(err) => {
                fails += 1;
                let _ = writeln!(out, "{} '{}': {err}", c.id, c.name);
            }
        }
        if rs::load_opponent(&opp, &c.sound).is_none() {
            fails += 1;
            let _ = writeln!(out, "{} '{}': no opponent loops", c.id, c.name);
        }
    }
    let speech = rs::speech_archive(&dir).expect("speech archive");
    for n in 0..=3 {
        let e = rs::countdown_entry(n);
        let pcm = speech.get(&e).and_then(|b| rs::decode(b, 0));
        let _ = writeln!(out, "countdown {e}: {:?}", pcm.as_ref().map(|p| (p.samples.len(), p.rate)));
        fails += pcm.is_none() as usize;
    }
    let track = rs::find_file(&rs::sounds_dir(&dir), "track.viv").and_then(|p| rs::Archive::open(&p)).expect("track.viv");
    for t in ["alps", "autobahn", "canyon", "castle", "coastal", "farmland", "foothills", "forest", "monaco1", "skidpad"] {
        let a = rs::ambient_file(&dir, t).and_then(|p| std::fs::read_to_string(p).ok()).map(|s| rs::parse_ambient(&s)).unwrap_or_default();
        let found = a.iter().filter(|a| track.entries.iter().any(|e| rs::track_sound_number(&e.name) == Some(a.sound))).count();
        let _ = writeln!(out, "ambience {t}: {} sounds, {found} found in track.viv", a.len());
    }
    let _ = writeln!(out, "{} cars, {} engine sets, {fails} failures", cars.len(), seen.len());
    let _ = writeln!(out, "{}", if fails == 0 { "ALL OK" } else { "FAILED" });
    print!("{out}");
    let log = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../logs/sound_check.txt");
    let _ = std::fs::create_dir_all(log.parent().unwrap());
    let _ = std::fs::write(log, out);
}
