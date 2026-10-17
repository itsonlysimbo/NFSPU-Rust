//! Prints what `sky::load` makes of each track's sky files: the pictures, the weather regions and
//! the horizon colour table. Run: `cargo run -p nfspu-game --example sky_info`
use nfspu_game::sky;

fn main() {
    let dir = match std::env::var("NFSPU_GAME_DIR") {
        Ok(d) => std::path::PathBuf::from(d),
        Err(_) => {
            eprintln!("set NFSPU_GAME_DIR to the game's install folder");
            return;
        }
    };
    let sky_dir = dir.join("GameData").join("Track").join("Sky");
    let mut names: Vec<String> = std::fs::read_dir(&sky_dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter_map(|e| e.path().file_stem()?.to_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names.dedup();

    for name in &names {
        let Some(s) = sky::load(&dir, name) else { continue };
        println!("== {name}");
        println!("   capability {}  level {}", s.capability, s.level);
        println!("   pictures ({}): {}", s.pictures.len(), s.pictures.describe().join(", "));
        if let Some(runs) = s.pictures.content_runs("horz") { println!("   horz content runs: {runs:?}"); }
        for (i, r) in s.regions.iter().enumerate() {
            let tint = r.tint(&s.pictures);
            println!(
                "   region {}: sky map {:?}, colour {:?} -> tint {:.2},{:.2},{:.2}, clouds {:?}",
                if i == 0 { "A" } else { "B" },
                r.sky_map,
                r.sky_colour,
                tint[0],
                tint[1],
                tint[2],
                r.clouds
            );
        }
        if s.horizon.is_empty() {
            println!("   no horizon colour table");
        } else {
            let warm = s.horizon.iter().filter(|h| h.rgb()[0] > h.rgb()[2] + 0.03).count();
            let cold = s.horizon.iter().filter(|h| h.rgb()[2] > h.rgb()[0] + 0.03).count();
            println!(
                "   horizon colours: {} ({} warm, {} cold), position field {:.3}..{:.3} (unconfirmed)",
                s.horizon.len(),
                warm,
                cold,
                s.horizon.iter().map(|h| h.angle).fold(f32::MAX, f32::min),
                s.horizon.iter().map(|h| h.angle).fold(f32::MIN, f32::max),
            );
            for h in s.horizon.iter().take(6) {
                let [r, g, b] = h.rgb();
                println!(
                    "      pos {:.3}  rgb {:>3.0},{:>3.0},{:>3.0}  {}",
                    h.angle,
                    r * 255.0,
                    g * 255.0,
                    b * 255.0,
                    if r > b + 0.03 {
                        "warm"
                    } else if b > r + 0.03 {
                        "cold"
                    } else {
                        "neutral"
                    }
                );
            }
        }
        println!();
    }
}