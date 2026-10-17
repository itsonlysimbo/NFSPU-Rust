//! Parses every track `.crp` from the player's install and checks the geometry is self-consistent
//! (all vertex / UV indices in range, triangle counts, bounding box). Console output only.
//! Run: `cargo run -p nfspu-formats --example tracks --release`

use nfspu_formats::{crp, refpack};
use std::{env, fs, path::PathBuf};

fn main() {
    let dir = PathBuf::from(env::var("NFSPU_GAME_DIR").expect("set NFSPU_GAME_DIR")).join("GameData").join("Track");
    let mut files: Vec<_> = fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("crp")).collect();
    files.sort();
    println!("{:<14} {:>8} {:>9} {:>10} {:>8}  bounding box (x, y, z)", "track", "articles", "meshes", "triangles", "bad idx");
    for f in files {
        let raw = fs::read(&f).unwrap();
        let data = refpack::decompress(&raw).expect("refpack");
        let c = crp::parse(&data).expect("crp");
        let (mut meshes, mut tris, mut bad) = (0usize, 0usize, 0usize);
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for a in 0..c.articles.len() {
            let Some(m) = c.article_mesh(a, 0) else { continue };
            meshes += 1;
            for p in &m.positions { for k in 0..3 { lo[k] = lo[k].min(p[k]); hi[k] = hi[k].max(p[k]); } }
            for b in &m.batches {
                tris += b.vertex_idx.len() / 3;
                bad += b.vertex_idx.iter().filter(|&&i| i as usize >= m.positions.len()).count();
                bad += b.uv_idx.iter().filter(|&&i| i as usize >= m.uvs.len()).count();
            }
        }
        println!("{:<14} {:>8} {:>9} {:>10} {:>8}  [{:.0}..{:.0}] [{:.0}..{:.0}] [{:.0}..{:.0}]",
            f.file_stem().unwrap().to_string_lossy(), c.articles.len(), meshes, tris, bad, lo[0], hi[0], lo[1], hi[1], lo[2], hi[2]);
    }
}
