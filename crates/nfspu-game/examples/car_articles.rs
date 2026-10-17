//! Lists every article in a car file: name, piece id (geometry), type, and triangles per level.
//! Run: `cargo run -p nfspu-game --example car_articles -- 928`

use nfspu_formats::{crp, refpack};

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "993".to_string());
    let dir = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").expect("NFSPU_GAME_DIR")).join("GameData").join("CarModel");
    let raw = std::fs::read(dir.join(format!("{name}.crp"))).expect("read car");
    let data = if refpack::is_refpack(&raw) { refpack::decompress(&raw).expect("refpack") } else { raw };
    let c = crp::parse(&data).expect("parse");
    println!("{:<20} {:>4} {:>4}  tris at level 1..5", "article", "geo", "type");
    for a in 0..c.articles.len() {
        let n = c.article_name(a).unwrap_or_default();
        let (g, t) = c.article_base(a).map_or((255, 255), |b| (b.geometry, b.type_index));
        let tris: Vec<String> = (1..=5u8)
            .map(|l| c.article_mesh(a, l).map_or("-".to_string(), |m| m.batches.iter().map(|b| b.vertex_idx.len() / 3).sum::<usize>().to_string()))
            .collect();
        println!("{n:<20} {g:>4} {t:>4}  {}", tris.join(" "));
    }
}
