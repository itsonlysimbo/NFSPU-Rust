//! Prints the material (kind, texture name) of each batch of the named articles of a car.
//! Run: `cargo run -p nfspu-game --example car_materials -- 959 Handle License Body`
use nfspu_formats::{crp, refpack};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let name = a.get(1).cloned().unwrap_or_else(|| "993".to_string());
    let dir = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").unwrap()).join("GameData").join("CarModel");
    let raw = std::fs::read(dir.join(format!("{name}.crp"))).unwrap();
    let data = if refpack::is_refpack(&raw) { refpack::decompress(&raw).unwrap() } else { raw };
    let c = crp::parse(&data).unwrap();
    let mats = c.materials();
    for art in 0..c.articles.len() {
        let n = c.article_name(art).unwrap_or_default();
        if !a[2..].iter().any(|w| n == *w) { continue }
        let Some(m) = c.article_mesh(art, 3) else { continue };
        for b in &m.batches {
            let mt = mats.iter().find(|x| x.index == b.material);
            println!("{name:<8} {n:<10} material {:>3}  kind {:<10} texture {:?}  tris {}", b.material, mt.map_or("?", |x| x.kind.as_str()), mt.and_then(|x| x.texture.clone()), b.vertex_idx.len() / 3);
        }
    }
}
