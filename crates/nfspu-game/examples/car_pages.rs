//! Writes every assembled texture page of a car (as the viewer builds them, `tpage1`, ...) to `extracted/` as
//! .png images (transparent texels shown magenta) so they can be looked at.
//! Run: `cargo run -p nfspu-game --example car_pages -- 993 [style] [driver] [paint]`

use nfspu_game::car_model::{self, CarOptions, Page};

fn save(name: &str, tag: &str, page: &Option<Page>) {
    let Some(p) = page else { return };
    // transparent texels shown magenta
    let rgba: Vec<u8> = p.rgba.chunks_exact(4).flat_map(|px| if px[3] < 128 { [255, 0, 255, 255] } else { [px[0], px[1], px[2], 255] }).collect();
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("extracted");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}_page_{tag}.png"));
    let image = bevy::image::Image::new(
        bevy::render::render_resource::Extent3d { width: p.size, height: p.size, depth_or_array_layers: 1 },
        bevy::render::render_resource::TextureDimension::D2,
        rgba,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::default(),
    );
    image.try_into_dynamic().unwrap().to_rgb8().save(&path).unwrap();
    println!("wrote {} ({}x{})", path.file_name().unwrap().to_string_lossy(), p.size, p.size);
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let name = a.get(1).cloned().unwrap_or_else(|| "993".to_string());
    let get = |i: usize, d: usize| a.get(i).and_then(|s| s.parse().ok()).unwrap_or(d);
    let o = CarOptions { style: get(2, 1), driver: get(3, 1) as u8, paint: get(4, 4), ..Default::default() };
    let m = car_model::load_car(&name, &o).expect("load car");
    println!("style '{}', {} triangles", m.style_name, m.triangles());
    // every page as the .tpg numbers it (tpage1 = index 0)
    for (i, page) in m.pages.iter().enumerate() {
        save(&name, &format!("tpage{}", i + 1), page);
    }
    println!("headlights {} tris, taillights {} tris, paint {} tris", m.headlights.positions.len() / 3, m.taillights.positions.len() / 3, m.paint.positions.len() / 3);
}
