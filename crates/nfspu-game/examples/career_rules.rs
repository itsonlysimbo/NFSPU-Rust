//! Prints every career tournament (era, name, car rule text, fee, difficulty, prizes) and the
//! cars each rule lets in (`career::car_allowed`), for checking the rule table against the text.
//! Run: `cargo run -p nfspu-game --example career_rules --release` (needs NFSPU_GAME_DIR).
//! Log: logs/career_rules.txt

use nfspu_formats::fe::Locale;
use nfspu_game::{career, catalog};
use std::fmt::Write as _;
use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(std::env::var("NFSPU_GAME_DIR").expect("set NFSPU_GAME_DIR"));
    let locale = std::fs::read(root.join("FEData").join("Locale").join("festrings.csv")).map(|b| Locale::parse_csv_bytes(&b)).unwrap_or_default();
    let trn = std::fs::read(root.join("FEData").join("Data").join("nfs5.trn")).expect("nfs5.trn");
    let cars: Vec<catalog::Car> = catalog::parse_cars(&std::fs::read(root.join("GameData").join("nfs5.car")).or_else(|_| std::fs::read(root.join("FEData").join("Data").join("nfs5.car"))).expect("nfs5.car")).into_iter().filter(|c| c.kind == 0).collect();
    let text = |id: u32| locale.text(id, 0).unwrap_or_default().trim().to_string();
    let mut out = String::new();
    for t in career::parse_tournaments(&trn) {
        let rule = text(t.restriction);
        let allowed: Vec<&catalog::Car> = cars.iter().filter(|c| career::car_allowed(&rule, c)).collect();
        let _ = writeln!(out, "{} (id {}, era {}, difficulty {}, fee {}, 1st {}): rule \"{rule}\" -> {} of {} cars", text(t.name), t.id, t.era, t.difficulty, t.entry_fee, t.prizes[0], allowed.len(), cars.len());
        let names: Vec<String> = allowed.iter().take(14).map(|c| c.name.clone()).collect();
        let _ = writeln!(out, "    e.g. {}", names.join("; "));
    }
    print!("{out}");
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("logs");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join("career_rules.txt"), out);
}
