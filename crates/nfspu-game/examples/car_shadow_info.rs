//! Lists a car's wheel-shadow articles: type, level, triangle count, the material and its page,
//! the bounds of the quad (model space, before the viewer's x mirror), its UV range and, at
//! level 1, each corner's height and UV. Also prints the shadow page (page 6) as a coarse grid of
//! alpha (0-9) and the mean colour of its covered texels. Console output only.
//! Run: `cargo run -p nfspu-game --example car_shadow_info -- 993 901`
use nfspu_formats::{crp, refpack};

fn main() {
    let names: Vec<String> = std::env::args().skip(1).collect();
    let dir = nfspu_game::car_model::car_dir().expect("car folder");
    for name in if names.is_empty() { vec!["993".to_string()] } else { names } {
        let raw = std::fs::read(dir.join(format!("{name}.crp"))).expect("read car");
        let data = if refpack::is_refpack(&raw) { refpack::decompress(&raw).expect("unpack") } else { raw };
        let c = crp::parse(&data).expect("parse");
        let mats: Vec<_> = c.materials();
        println!("{name}:");
        // the shadow page (page 6) as a coarse grid: alpha 0-9 and the mean colour
        if let Ok(m) = nfspu_game::car_model::load_car(&name, &nfspu_game::car_model::CarOptions::default()) {
            if let Some(Some(pg)) = m.pages.get(6) {
                let n = pg.size as usize;
                let (mut rgb, mut cnt) = ([0u64; 3], 0u64);
                for y in (0..n).step_by((n / 16).max(1)) {
                    let row: String = (0..n).step_by((n / 32).max(1)).map(|x| {
                        let a = pg.rgba[(y * n + x) * 4 + 3] as usize;
                        char::from(b'0' + (a * 9 / 255) as u8)
                    }).collect();
                    println!("    {row}");
                }
                for px in pg.rgba.chunks(4) {
                    if px[3] > 0 {
                        for k in 0..3 {
                            rgb[k] += px[k] as u64;
                        }
                        cnt += 1;
                    }
                }
                println!("  shadow page {n} px, mean colour of covered texels {:?}", rgb.map(|v| v / cnt.max(1)));
            }
        }
        for a in 0..c.articles.len() {
            let Some(art) = c.article_name(a) else { continue };
            if !art.starts_with("WheelShadow") {
                continue;
            }
            let base = c.article_base(a);
            for lvl in 1..=4u8 {
                let Some(mesh) = c.article_mesh(a, lvl) else { continue };
                let m = c.article_transform(a, lvl);
                for b in &mesh.batches {
                    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                    for &i in &b.vertex_idx {
                        let p = mesh.positions[i as usize];
                        let p = m.as_ref().map_or(p, |m| nfspu_game::car_model::apply_matrix(m, p));
                        for k in 0..3 {
                            lo[k] = lo[k].min(p[k]);
                            hi[k] = hi[k].max(p[k]);
                        }
                    }
                    let (mut u0, mut u1) = ([f32::MAX; 2], [f32::MIN; 2]);
                    for &i in &b.uv_idx {
                        let uv = mesh.uvs[i as usize];
                        for k in 0..2 {
                            u0[k] = u0[k].min(uv[k]);
                            u1[k] = u1[k].max(uv[k]);
                        }
                    }
                    if lvl == 1 {
                        let corners: Vec<String> = (0..b.vertex_idx.len()).map(|k| {
                            let p = mesh.positions[b.vertex_idx[k] as usize];
                            let p = m.as_ref().map_or(p, |m| nfspu_game::car_model::apply_matrix(m, p));
                            let uv = mesh.uvs[b.uv_idx[k] as usize];
                            format!("(y {:.2} z {:.2} uv {:.2},{:.2})", p[1], p[2], uv[0], uv[1])
                        }).collect();
                        println!("    corners {}", corners.join(" "));
                    }
                    let mat = mats.iter().find(|x| x.index == b.material);
                    println!(
                        "  {art} type {:?} level {lvl}: {} tris, material {} {:?} page {:?}, x {:.2}..{:.2} y {:.3}..{:.3} z {:.2}..{:.2}, uv {:.3?}..{:.3?}",
                        base.map(|b| (b.geometry, b.type_index)),
                        b.vertex_idx.len() / 3,
                        b.material,
                        mat.map(|m| m.kind.clone()),
                        mat.map(|m| m.page),
                        lo[0], hi[0], lo[1], hi[1], lo[2], hi[2], u0, u1
                    );
                }
            }
        }
    }
}
