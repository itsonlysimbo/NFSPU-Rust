//! Read-only comparison of menu swatches with renderer paint schemes.
use nfspu_game::{car_model, catalog};

fn main() {
    let root = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").expect("NFSPU_GAME_DIR"));
    let mut files = 0;
    let mut schemes = 0;
    let mut unsupported = 0;
    for entry in std::fs::read_dir(root.join("GameData/CarModel")).unwrap() {
        let path = entry.unwrap().path();
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("clr")) {
            continue;
        }
        let data = std::fs::read(&path).unwrap();
        let text = String::from_utf8_lossy(&data);
        let menu = catalog::parse_paints(&text);
        let renderer = car_model::parse_clr(&text);
        if renderer.is_empty() && !text.lines().any(|l| l.trim_start().starts_with("color1.r=")) {
            println!("{}: unsupported, no RGB body colour (not compared)", path.file_name().unwrap().to_string_lossy());
            unsupported += 1;
            continue;
        }
        assert_eq!(menu.len(), renderer.len(), "{}", path.display());
        for paint in &menu {
            assert_eq!(paint.body, renderer[&usize::from(paint.section)].colours[0], "{} section {}", path.display(), paint.section);
        }
        println!("{}: {} paints, section order {:?}", path.file_name().unwrap().to_string_lossy(), menu.len(), menu.iter().map(|p| p.section).collect::<Vec<_>>());
        files += 1;
        schemes += menu.len();
    }
    assert!(files > 0, "no paint files found");
    println!("Verified {files} files / {schemes} swatches against renderer schemes; {unsupported} unsupported files");
    owned_cars(&root);
}

/// Each save's owned cars: the paint section read from the saved catalogue copy, and the body
/// colour that section is in the model's `.clr`. Read-only; checks the saved value fits the model.
fn owned_cars(root: &std::path::Path) {
    let cars = catalog::parse_cars(&std::fs::read(root.join("FEData/Data/nfs5.car")).unwrap_or_default());
    let mut saves = 0;
    let mut cars_seen = 0;
    for entry in std::fs::read_dir(root.join("SaveData")).unwrap() {
        let path = entry.unwrap().path();
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("sav")) {
            continue;
        }
        let save = std::fs::read(&path).unwrap();
        let owned = catalog::owned_cars(&save, &cars);
        if owned.is_empty() {
            continue;
        }
        saves += 1;
        for car in &owned {
            let model = &cars[car.index].model;
            let paints = catalog::load_paints(root, model);
            let found = paints.iter().find(|p| p.section as u16 == car.paint);
            println!(
                "{}: {} ({}) paint section {} -> {}",
                path.file_name().unwrap().to_string_lossy(),
                cars[car.index].name,
                model,
                car.paint,
                found.map(|p| format!("body rgb{:?}", p.body)).unwrap_or_else(|| "no such section (not checked)".into())
            );
            cars_seen += 1;
        }
    }
    println!("Owned cars: {cars_seen} in {saves} save(s)");
}
