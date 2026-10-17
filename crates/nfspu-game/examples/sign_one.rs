//! Prints the triangles of one placed track object (default: the first curve sign on alps).
use nfspu_formats::{crp, refpack};
fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "alps".to_string());
    let want = std::env::args().nth(2).unwrap_or_else(|| "OBJ0461R".to_string());
    let lod: u8 = std::env::args().nth(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    let dir = if lod > 0 { nfspu_game::car_model::car_dir().unwrap() } else { std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").unwrap()).join("GameData").join("Track") };
    let raw = std::fs::read(dir.join(format!("{name}.crp"))).unwrap();
    let data = if refpack::is_refpack(&raw) { refpack::decompress(&raw).unwrap() } else { raw };
    let c = crp::parse(&data).unwrap();
    let mats = c.materials();
    for a in 0..c.articles.len() {
        let n = c.article_name(a).unwrap_or_default();
        if !n.starts_with(&want) { continue }
        let Some(m) = c.article_mesh(a, lod) else { continue };
        println!("{n}");
        for b in &m.batches {
            let tex = mats.iter().find(|x| x.index == b.material).and_then(|x| x.texture.clone()).unwrap_or_default();
            for t in 0..b.vertex_idx.len() / 3 {
                let p: Vec<[f32; 3]> = (0..3).map(|q| m.positions[b.vertex_idx[3 * t + q] as usize]).collect();
                let uv: Vec<[f32; 2]> = (0..3).map(|q| m.uvs[b.uv_idx[3 * t + q] as usize]).collect();
                let (e1, e2) = ([p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]], [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]]);
                let cr = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
                let out = [-cr[0], -cr[1], -cr[2]];
                println!("  mat {:>3} tex {:<6} front normal (file) ({:+.2},{:+.2},{:+.2})  pts {:?}  uv {:?}", b.material, tex, out[0], out[1], out[2],
                    p.iter().map(|v| format!("({:.2},{:.2},{:.2})", v[0], v[1], v[2])).collect::<Vec<_>>(),
                    uv.iter().map(|v| format!("({:.2},{:.2})", v[0], v[1])).collect::<Vec<_>>());
            }
        }
    }
}
