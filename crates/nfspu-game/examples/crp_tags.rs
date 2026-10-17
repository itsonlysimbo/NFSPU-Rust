//! Lists the part tags inside every article of a car or track .crp file, with how often each
//! appears and the total size, so the interesting ones (sky, sun, light, fog) can be spotted.
//! Run: `cargo run -p nfspu-game --example crp_tags -- Track/alps.crp`
use nfspu_formats::{crp, refpack};

fn main() {
    let arg = std::env::args().nth(1).unwrap_or_else(|| "Track/alps.crp".to_string());
    let dir = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").unwrap()).join("GameData");
    let raw = std::fs::read(dir.join(&arg)).unwrap_or_else(|e| panic!("{arg}: {e}"));
    let data = if refpack::is_refpack(&raw) { refpack::decompress(&raw).unwrap() } else { raw };
    let c = crp::parse(&data).expect("crp");
    let mut count: std::collections::HashMap<String, (usize, usize, usize)> = Default::default();
    for a in 0..c.articles.len() {
        for p in &c.articles[a].parts {
            let tag = String::from_utf8_lossy(&p.tag).trim_end().to_string();
            let e = count.entry(tag).or_default();
            e.0 += 1;
            e.1 += p.length;
            e.2 = e.2.max(a);
        }
    }
    let mut rows: Vec<_> = count.into_iter().collect();
    rows.sort_by_key(|(_, v)| std::cmp::Reverse(v.0));
    println!("{} articles; tag count / bytes / last article", c.articles.len());
    for (tag, (n, bytes, last)) in rows {
        println!("  {tag:<10} {n:>6} {bytes:>9}  article {last}");
    }
}
