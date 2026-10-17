//! For each batch of the named articles of a car, prints its UV box and which tiles on every
//! embedded page contain it (to find where a part's artwork really is).
//! Run: `cargo run -p nfspu-game --example uv_where -- 944 3 Bottom`  (car, level, articles...)

use nfspu_formats::{crp, refpack, shpi};
use nfspu_game::car_model;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let name = &a[1];
    let level: u8 = a[2].parse().unwrap();
    let dir = car_model::car_dir().unwrap();
    let raw = std::fs::read(dir.join(format!("{name}.crp"))).unwrap();
    let data = if refpack::is_refpack(&raw) { refpack::decompress(&raw).unwrap() } else { raw };
    let c = crp::parse(&data).unwrap();
    let tpg = std::fs::read_to_string(dir.join(format!("{name}.tpg"))).unwrap_or_default();
    let names = car_model::tpg_file_names(&tpg);
    let mats = c.materials();
    let archives: Vec<(u16, shpi::ShpiArchive)> = c.embedded_archives().into_iter().filter_map(|(i, b)| shpi::parse(b).ok().map(|x| (i, x))).collect();
    for art in 0..c.articles.len() {
        let n = c.article_name(art).unwrap_or_default();
        if !a[3..].iter().any(|w| n.starts_with(w.as_str())) {
            continue;
        }
        let base = c.article_base(art);
        let Some(m) = c.article_mesh(art, level) else { continue };
        for b in &m.batches {
            let (mut u0, mut v0, mut u1, mut v1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for &i in &b.uv_idx {
                let uv = m.uvs[i as usize];
                u0 = u0.min(uv[0]);
                v0 = v0.min(uv[1]);
                u1 = u1.max(uv[0]);
                v1 = v1.max(uv[1]);
            }
            let kind = mats.iter().find(|x| x.index == b.material).map_or("?".to_string(), |x| x.kind.clone());
            println!(
                "{n} (geo {:?} type {:?}) material {} {kind}: {} tris, uv u[{u0:.3}..{u1:.3}] v[{v0:.3}..{v1:.3}]",
                base.as_ref().map(|x| x.geometry),
                base.as_ref().map(|x| x.type_index),
                b.material,
                b.vertex_idx.len() / 3
            );
            for (i, ar) in &archives {
                // page size: the largest tile extent, rounded up to a power of two
                let extent = ar.entries.iter().map(|e| (e.x as u32 + e.width as u32).max(e.y as u32 + e.height as u32)).max().unwrap_or(1);
                let size = extent.next_power_of_two() as f32;
                let fits: Vec<String> = ar
                    .entries
                    .iter()
                    .filter(|e| {
                        let s = 1.5;
                        u0 * size >= e.x as f32 - s && v0 * size >= e.y as f32 - s && u1 * size <= e.x as f32 + e.width as f32 + s && v1 * size <= e.y as f32 + e.height as f32 + s
                    })
                    .map(|e| e.name.clone())
                    .collect();
                if !fits.is_empty() {
                    println!("    page {i} {:?} (~{size} px): {}", names.get(i), fits.join(" "));
                }
            }
        }
    }
}
