//! For one car and level, lists every batch's material page (the material's field at 0x28), its
//! material kind, the article and the UV box, so the page each part really uses can be checked.
//! Run: `cargo run -p nfspu-game --example car_page_survey -- 993 1`
use nfspu_formats::{crp, refpack};
use std::collections::BTreeMap;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let name = a.get(1).cloned().unwrap_or_else(|| "993".to_string());
    let lod: u8 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(3);
    let dir = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").unwrap()).join("GameData").join("CarModel");
    let raw = std::fs::read(dir.join(format!("{name}.crp"))).unwrap();
    let data = if refpack::is_refpack(&raw) { refpack::decompress(&raw).unwrap() } else { raw };
    let c = crp::parse(&data).unwrap();
    let mats = c.materials();
    // (page, kind, article) -> (triangles, uv box, sum of x, corners)
    let mut rows: BTreeMap<(u32, String, String), (usize, [f32; 4], f32, usize)> = BTreeMap::new();
    for art in 0..c.articles.len() {
        let n = c.article_name(art).unwrap_or_default();
        let Some(m) = c.article_mesh(art, lod) else { continue };
        let ty = c.article_base(art).map_or(255, |b| b.type_index);
        for b in &m.batches {
            let mt = mats.iter().find(|x| x.index == b.material);
            let key = (mt.map_or(999, |x| x.page), mt.map_or("?".into(), |x| x.kind.clone()), format!("{n} t{ty}"));
            let e = rows.entry(key).or_insert((0, [f32::MAX, f32::MAX, f32::MIN, f32::MIN], 0.0, 0));
            e.0 += b.vertex_idx.len() / 3;
            let matrix = c.article_transform(art, lod);
            for &i in &b.vertex_idx {
                let p = m.positions[i as usize];
                let p = matrix.map_or(p, |mx| nfspu_game::car_model::apply_matrix(&mx, p));
                e.2 += p[0];
                e.3 += 1;
            }
            for &i in &b.uv_idx {
                let [u, v] = m.uvs[i as usize];
                e.1 = [e.1[0].min(u), e.1[1].min(v), e.1[2].max(u), e.1[3].max(v)];
            }
        }
    }
    for ((page, kind, art), (tris, uv, sx, n)) in rows {
        // mean x in the file's own frame (+x is the driver's side)
        println!("page {page:>2} {kind:<10} {art:<24} {tris:>4} tris  u {:>6.3}..{:<6.3} v {:>6.3}..{:<6.3}  mean x {:>6.2}", uv[0], uv[2], uv[1], uv[3], sx / n.max(1) as f32);
    }
}
