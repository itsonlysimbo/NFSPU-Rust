//! Prints the coastal water batches: material, texture, colours, UVs.
use nfspu_formats::{crp, refpack, shpi};
fn main() {
    let dir = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").unwrap()).join("GameData").join("Track");
    let cd = refpack::decompress(&std::fs::read(dir.join("coastal.crp")).unwrap()).unwrap();
    let c = crp::parse(&cd).unwrap();
    let f = refpack::decompress(&std::fs::read(dir.join("coastal.fsh")).unwrap()).unwrap();
    let arch = shpi::parse(&f).unwrap();
    let mats = c.materials();
    for a in 0..c.articles.len() {
        let n = c.article_name(a).unwrap_or_default();
        if !n.starts_with("WATER") { continue; }
        let m = c.article_mesh(a, 0).unwrap();
        for b in &m.batches {
            let mat = mats.iter().find(|x| x.index == b.material);
            let tex = mat.and_then(|x| x.texture.clone());
            let info = tex.as_ref().and_then(|t| arch.find(t)).map(|e| {
                let rgba = shpi::decode_rgba8(&f, e).unwrap();
                let n = rgba.len() / 4;
                let avg: Vec<u32> = (0..4).map(|k| rgba.chunks_exact(4).map(|p| p[k] as u32).sum::<u32>() / n as u32).collect();
                format!("{}x{} avg {:?}", e.width, e.height, avg)
            });
            println!("{n} mat {} {:?} tex {:?} {:?}\n  colors {:?}\n  uv {:?}\n  pos {:?}", b.material, mat.map(|x| x.index), tex, info, &b.colors[..4.min(b.colors.len())], &b.uv_idx.iter().take(6).map(|&i| m.uvs[i as usize]).collect::<Vec<_>>(), &b.vertex_idx.iter().take(3).map(|&i| m.positions[i as usize]).collect::<Vec<_>>());
        }
    }
}
