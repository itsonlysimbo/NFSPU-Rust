//! Writes the assembled texture pages of a car (as the viewer would build them) to `extracted/` as
//! .ppm images (transparent texels shown magenta) so they can be looked at.
//! Run: `cargo run -p nfspu-game --example car_pages -- 993 [style] [driver] [paint]`

use nfspu_game::car_model::{self, CarOptions, Page};

fn save(name: &str, tag: &str, page: &Option<Page>) {
    let Some(p) = page else { return };
    let mut out = format!("P6\n{} {}\n255\n", p.size, p.size).into_bytes();
    for px in p.rgba.chunks_exact(4) {
        if px[3] < 128 { out.extend_from_slice(&[255, 0, 255]); } else { out.extend_from_slice(&px[0..3]); }
    }
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("extracted");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}_page_{tag}.ppm"));
    std::fs::write(&path, out).unwrap();
    println!("wrote {} ({}x{})", path.file_name().unwrap().to_string_lossy(), p.size, p.size);
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let name = a.get(1).cloned().unwrap_or_else(|| "993".to_string());
    let get = |i: usize, d: usize| a.get(i).and_then(|s| s.parse().ok()).unwrap_or(d);
    let o = CarOptions { style: get(2, 1), driver: get(3, 1) as u8, paint: get(4, 4), ..Default::default() };
    let m = car_model::load_car(&name, &o).expect("load car");
    println!("style '{}', {} triangles", m.style_name, m.triangles());
    save(&name, "exterior", &m.exterior);
    save(&name, "trim", &m.extint);
    save(&name, "glass", &m.glass_page);
    save(&name, "suit", &m.suit_page);
    save(&name, "head", &m.head_page);
    println!("headlights {} tris, taillights {} tris, paint {} tris", m.headlights.positions.len() / 3, m.taillights.positions.len() / 3, m.paint.positions.len() / 3);
}
