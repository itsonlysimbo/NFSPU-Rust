//! Prints the articles that have a part with a given tag: their name and the part's bytes.
//! Run: `cargo run -p nfspu-game --example crp_part -- Track/alps.crp minA`
use nfspu_formats::{crp, refpack};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let file = a.get(1).cloned().unwrap_or_else(|| "Track/alps.crp".to_string());
    let tag = a.get(2).cloned().unwrap_or_else(|| "minA".to_string());
    let dir = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").unwrap()).join("GameData");
    let raw = std::fs::read(dir.join(&file)).unwrap();
    let data = if refpack::is_refpack(&raw) { refpack::decompress(&raw).unwrap() } else { raw };
    let c = crp::parse(&data).expect("crp");
    let mut shown = 0;
    for i in 0..c.articles.len() {
        let Some(p) = c.articles[i].parts.iter().find(|p| String::from_utf8_lossy(&p.tag).trim_end() == tag) else {
            continue;
        };
        let bytes = c.part_data(p);
        let name = c.article_name(i).unwrap_or_default();
        let floats: Vec<f32> = bytes
            .chunks_exact(4)
            .filter(|c| c.len() == 4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .take(16)
            .collect();
        let head: Vec<String> = bytes.iter().take(16).map(|b| format!("{b:02X}")).collect();
        println!("article {i:<5} {name:<28} {} bytes  head {}  floats {:?}", bytes.len(), head.join(" "), floats);
        shown += 1;
        if shown >= 40 {
            break;
        }
    }
    println!("{shown} articles have a part tagged {tag}");
}