//! Lists a car's embedded texture archives: index, the .tpg file name, and every tile.
//! Run: `cargo run -p nfspu-game --example car_archives -- eas1`
use nfspu_formats::{crp, refpack, shpi};
use nfspu_game::car_model;
fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "eas1".to_string());
    let dir = car_model::car_dir().unwrap();
    let path = std::env::args().nth(2).map(|_| dir.join(format!("{name}.crp"))).unwrap_or_else(|| dir.join(format!("{name}.crp")));
    let raw = std::fs::read(path).unwrap();
    let data = if refpack::is_refpack(&raw) { refpack::decompress(&raw).unwrap() } else { raw };
    let c = crp::parse(&data).unwrap();
    let tpg = std::fs::read_to_string(dir.join(format!("{name}.tpg"))).unwrap_or_default();
    let names = car_model::tpg_file_names(&tpg);
    for (index, blob) in c.embedded_archives() {
        let Ok(a) = shpi::parse(blob) else { println!("archive {index}: not SHPI"); continue };
        println!("archive {index} ({:?}): {} tiles", names.get(&index), a.entries.len());
        for e in &a.entries {
            let code = blob.get(e.offset as usize).copied().unwrap_or(0);
            println!("  {:<6} code {code:#04x} {:>3}x{:<3} at ({:>3},{:>3})", e.name, e.width, e.height, e.x, e.y);
        }
    }
}
