//! Writes a car (as built by the real loader) to `extracted/<car>.obj` so it can be inspected offline.
//! Groups: paint, trim, glass, and one group per wheel (placed at its offset).
//! Run: `cargo run -p nfspu-game --example car_obj -- 993`

use nfspu_game::car_model::{self, Piece};
use std::fmt::Write;

fn dump(out: &mut String, base: &mut usize, group: &str, piece: &Piece, offset: [f32; 3], names: &[String]) {
    if piece.positions.is_empty() { return; }
    writeln!(out, "g {group}").unwrap();
    for p in &piece.positions { writeln!(out, "v {} {} {}", p[0] + offset[0], p[1] + offset[1], p[2] + offset[2]).unwrap(); }
    for uv in &piece.uvs { writeln!(out, "vt {} {}", uv[0], uv[1]).unwrap(); }
    for t in 0..piece.positions.len() / 3 {
        writeln!(out, "# article {}", names.get(piece.articles[3 * t] as usize).map(String::as_str).unwrap_or("?").replace(' ', "_")).unwrap();
        writeln!(out, "usemtl m{}", piece.materials[3 * t]).unwrap();
        writeln!(out, "f {a}/{a} {b}/{b} {c}/{c}", a = *base + 3 * t + 1, b = *base + 3 * t + 2, c = *base + 3 * t + 3).unwrap();
    }
    *base += piece.positions.len();
}

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "993".to_string());
    let m = car_model::load_car(&name, &car_model::CarOptions { style: std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(1), driver: std::env::args().nth(3).and_then(|s| s.parse().ok()).unwrap_or(1), paint: std::env::args().nth(4).and_then(|s| s.parse().ok()).unwrap_or(4), ..Default::default() }).expect("load car");
    let (mut s, mut base) = (String::new(), 0usize);
    dump(&mut s, &mut base, "paint", &m.paint, [0.0; 3], &m.article_names);
    dump(&mut s, &mut base, "trim", &m.trim, [0.0; 3], &m.article_names);
    dump(&mut s, &mut base, "glass", &m.glass, [0.0; 3], &m.article_names);
    dump(&mut s, &mut base, "interior", &m.interior, [0.0; 3], &m.article_names);
    dump(&mut s, &mut base, "interior_outside", &m.interior_outside, [0.0; 3], &m.article_names);
    dump(&mut s, &mut base, "paint_inner", &m.paint_inner, [0.0; 3], &m.article_names);
    dump(&mut s, &mut base, "trim_solid", &m.trim_solid, [0.0; 3], &m.article_names);
    dump(&mut s, &mut base, "cabrio_glass", &m.cabrio_glass, [0.0; 3], &m.article_names);
    dump(&mut s, &mut base, "headlights", &m.headlights, [0.0; 3], &m.article_names);
    dump(&mut s, &mut base, "taillights", &m.taillights, [0.0; 3], &m.article_names);
    dump(&mut s, &mut base, "driver_suit", &m.driver_suit, [0.0; 3], &m.article_names);
    dump(&mut s, &mut base, "driver_head", &m.driver_head, [0.0; 3], &m.article_names);
    for (i, w) in m.wheels.iter().enumerate() { dump(&mut s, &mut base, &format!("wheel{i}"), &w.piece, w.offset, &m.article_names); }
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("extracted");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.obj"));
    std::fs::write(&path, s).unwrap();
    println!("wrote {} ({} triangles, {} wheels)", path.display(), m.triangles(), m.wheels.len());
}
