//! Prints the briefing of each race mission of Factory Driver (reading aid; needs NFSPU_GAME_DIR).
fn main() {
    let game = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").unwrap_or_default());
    let ms = nfspu_game::factory::load_missions(&game);
    let loc = nfspu_formats::fe::Locale::parse_csv_bytes(&std::fs::read(game.join("FEData").join("Locale").join("festrings.csv")).unwrap_or_default());
    for m in ms.iter().filter(|m| m.min_place.is_some() || m.scene == 0 || m.index == 5) {
        println!("{} {}: {}\n", m.index, m.stem, loc.text(m.brief, 0).unwrap_or_default().replace('\n', " "));
    }
}
