//! For every track: do all materials used by triangles resolve to a texture that decodes?
//! Run: `cargo run -p nfspu-formats --example textures --release`

use nfspu_formats::{crp, refpack, shpi};
use std::{collections::HashSet, env, fs, path::PathBuf};

fn main() {
    let dir = PathBuf::from(env::var("NFSPU_GAME_DIR").expect("set NFSPU_GAME_DIR")).join("GameData").join("Track");
    let mut names: Vec<_> = fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("crp")).collect();
    names.sort();
    println!("{:<12} {:>9} {:>11} {:>10} {:>9} {:>11}", "track", "materials", "used by tri", "no texture", "missing", "undecodable");
    for f in names {
        let stem = f.file_stem().unwrap().to_string_lossy().to_string();
        let crp_data = refpack::decompress(&fs::read(&f).unwrap()).unwrap();
        let fsh_data = refpack::decompress(&fs::read(dir.join(format!("{stem}.fsh"))).unwrap()).unwrap();
        let c = crp::parse(&crp_data).unwrap();
        let archive = shpi::parse(&fsh_data).unwrap();
        let mats = c.materials();
        let mut used = HashSet::new();
        for a in 0..c.articles.len() {
            if let Some(m) = c.article_mesh(a, 0) {
                for b in &m.batches { used.insert(b.material); }
            }
        }
        let (mut none, mut missing, mut bad, mut undefined) = (0, 0, 0, 0);
        for idx in &used {
            let Some(m) = mats.iter().find(|m| m.index == *idx) else { undefined += 1; continue };
            match &m.texture {
                None => none += 1,
                Some(t) => match archive.find(t) {
                    None => missing += 1,
                    Some(e) => if shpi::decode_rgba8(&fsh_data, e).is_none() { bad += 1 },
                },
            }
        }
        println!("{:<12} {:>9} {:>11} {:>10} {:>9} {:>11}   (material index undefined: {undefined})", stem, mats.len(), used.len(), none, missing, bad);
    }
}
