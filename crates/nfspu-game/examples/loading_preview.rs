//! Composes the race loading screen for one choice without opening a window, and writes stages
//! 0, 5 and 9 to `logs/loading_<track>_stageNN.png`.
//!
//! `cargo run -p nfspu-game --example loading_preview -j 2 -- <track>[,<car>][,reverse][,mirror] [language]`

use nfspu_game::loading::{capture_path, save_png, LoadingArt, RaceSetup};

fn main() {
    let spec = std::env::args().nth(1).unwrap_or_else(|| "alps".into());
    let mut setup = RaceSetup::parse(&spec).expect("a track name");
    setup.language = std::env::args().nth(2).and_then(|v| v.parse().ok()).unwrap_or(0);
    let dir = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").expect("NFSPU_GAME_DIR"));
    let art = LoadingArt::load(&dir, &setup);
    for miss in &art.misses {
        println!("missing: {miss}");
    }
    let count = |s: &Vec<Option<nfspu_game::loading::Sprite>>| s.iter().filter(|s| s.is_some()).count();
    println!(
        "{setup:?}: plain background {}/{} tiles, track background {}/{}, track art {}/{}, translated label {}",
        count(&art.plain),
        art.plain.len(),
        count(&art.track_bg),
        art.track_bg.len(),
        count(&art.track_art),
        art.track_art.len(),
        art.load_text.is_some()
    );
    for stage in [0, 5, 9] {
        let path = capture_path(&setup.track, stage);
        save_png(&art.compose(&setup, stage), &path);
        println!("wrote {}", path.display());
    }
}
