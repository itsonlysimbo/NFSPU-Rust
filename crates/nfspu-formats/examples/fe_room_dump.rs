//! Lists what is inside a front-end room model (`FEData/Models/<name>.crp`): articles with their
//! part tags, Base bytes, transforms and triangle counts, then materials and embedded archives.
//! Run: `cargo run -p nfspu-formats --example fe_room_dump -- garage`

use nfspu_formats::{crp, refpack, shpi};

fn tag(t: &[u8; 4]) -> String {
    t.iter().map(|&b| if (32..127).contains(&b) { b as char } else { '.' }).collect::<String>() + &format!("({:02x}{:02x})", t[0], t[1])
}

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "garage".to_string());
    let verbose = std::env::args().any(|a| a == "-v");
    let dir = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").expect("NFSPU_GAME_DIR")).join("FEData").join("Models");
    // a path under the game folder (`GameData/CarModel/356_1.crp`) reads that file instead
    let file = if name.contains('/') { std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").unwrap()).join(&name) } else { dir.join(format!("{name}.crp")) };
    let raw = std::fs::read(file).expect("read model");
    let data = if refpack::is_refpack(&raw) { refpack::decompress(&raw).expect("refpack") } else { raw };
    println!("{name}.crp: {} bytes decompressed, magic {:?}", data.len(), String::from_utf8_lossy(&data[0..4]));
    let c = crp::parse(&data).expect("parse");
    for a in 0..c.articles.len() {
        let n = c.article_name(a).unwrap_or_default();
        let base = c.article_base(a);
        let lods: Vec<String> = (0..=5u8)
            .filter_map(|l| c.article_mesh(a, l).map(|m| format!("L{l}: {}v {}t mats {:?}", m.positions.len(), m.batches.iter().map(|b| b.vertex_idx.len() / 3).sum::<usize>(), m.batches.iter().map(|b| b.material).collect::<std::collections::BTreeSet<_>>())))
            .collect();
        println!("[{a}] {n:<28} base {:?} slot {}  {}", base.map(|b| (b.geometry, b.type_index)), nfspu_formats::fe_room::animation_slot(&c, a), lods.join(" | "));
        for l in 0..=5u8 {
            if let Some(t) = c.article_transform(a, l) {
                println!("     transform L{l}: row0 {:?} row1 {:?} row2 {:?} pos {:?}", &t[0..3], &t[4..7], &t[8..11], &t[12..15]);
            }
        }
        if verbose {
            if let Some(p) = c.articles[a].parts.iter().find(|p| &p.tag == b"esaB") {
                let hex: Vec<String> = c.part_data(p).iter().map(|b| format!("{b:02x}")).collect();
                println!("     base: {}", hex.join(""));
            }
            let tags: Vec<String> = c.articles[a].parts.iter().map(|p| format!("{} {}B x{}", tag(&p.tag), p.length, p.count)).collect();
            println!("     parts: {}", tags.join(", "));
            if let Some(m) = c.article_mesh(a, 1).or_else(|| c.article_mesh(a, 0)) {
                let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                for p in &m.positions {
                    for k in 0..3 {
                        lo[k] = lo[k].min(p[k]);
                        hi[k] = hi[k].max(p[k]);
                    }
                }
                println!("     bounds {lo:?} .. {hi:?}");
            }
        }
    }
    println!("misc parts:");
    for p in &c.misc {
        println!("  {} {}B x{}", tag(&p.tag), p.length, p.count);
        if verbose && p.tag[2] != b'f' {
            for (row, chunk) in c.part_data(p).chunks(16).enumerate().take(24) {
                let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
                let txt: String = chunk.iter().map(|&b| if (32..127).contains(&b) { b as char } else { '.' }).collect();
                println!("    {:04x}: {:<48} {txt}", row * 16, hex.join(" "));
            }
        }
    }
    for m in c.materials() {
        println!("material {} kind {:?} texture {:?}", m.index, m.kind, m.texture);
    }
    for (i, arc) in c.embedded_archives() {
        match shpi::parse(arc) {
            Ok(s) => println!("archive {i}: {} entries {:?}", s.entries.len(), s.entries.iter().map(|e| format!("{} {}x{}", e.name, e.width, e.height)).collect::<Vec<_>>()),
            Err(e) => println!("archive {i}: {} bytes, not SHPI: {e}", arc.len()),
        }
    }
}
