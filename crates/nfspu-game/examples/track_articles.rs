//! Lists a track's articles that are not road or terrain chunks: name, triangles, size, position,
//! and whether they carry a transform. Run: `cargo run -p nfspu-game --example track_articles -- alps`

use nfspu_formats::{crp, refpack};

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "alps".to_string());
    let dir = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").expect("NFSPU_GAME_DIR")).join("GameData").join("Track");
    let raw = std::fs::read(dir.join(format!("{name}.crp"))).expect("read track");
    let data = if refpack::is_refpack(&raw) { refpack::decompress(&raw).expect("refpack") } else { raw };
    let c = crp::parse(&data).expect("parse");
    let mut kinds: std::collections::BTreeMap<String, usize> = Default::default();
    for a in 0..c.articles.len() {
        let n = c.article_name(a).unwrap_or_default();
        let prefix: String = n.chars().take_while(|ch| ch.is_ascii_alphabetic()).collect();
        *kinds.entry(prefix).or_default() += 1;
        if n.starts_with("RD") || n.starts_with("CNK") {
            continue;
        }
        let Some(m) = c.article_mesh(a, 0) else { println!("{n:<36} (no level-0 mesh)"); continue };
        if n.starts_with("WATER") {
            for p in &c.articles[a].parts {
                let tag = String::from_utf8_lossy(&p.tag).trim_end().to_string();
                let bytes = c.part_data(p);
                let u = |at: usize| bytes.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).unwrap_or(0);
                let mode = bytes.get(0..2).map(|b| u16::from_le_bytes([b[0], b[1]])).unwrap_or(0);
                println!("  WATER part {tag:?} len {} count {} mode {mode} nif {} nid {}", bytes.len(), p.count, u(0x28), u(0x2C));
            }
        }
        let tris: usize = m.batches.iter().map(|b| b.vertex_idx.len() / 3).sum();
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in &m.positions {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let tr = (0..=5u8).filter(|&l| c.article_transform(a, l).is_some()).map(|l| l.to_string()).collect::<Vec<_>>().join(",");
        let t0 = c.article_transform(a, 0).map(|m| format!("diag {:.2} {:.2} {:.2} at ({:.0},{:.0},{:.0})", m[0], m[5], m[10], m[12], m[13], m[14])).unwrap_or_default();
        println!(
            "{n:<36} tris {tris:>4} size {:>6.1} x {:>6.1} x {:>6.1} centre ({:>7.0},{:>6.0},{:>7.0}) tr[{tr}] {t0}",
            hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2], (lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0
        );
    }
    println!("article kinds: {kinds:?}");
}
