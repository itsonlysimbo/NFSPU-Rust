//! Dumps every decodable image of one `.fsh` (packed or not) to PNGs in the ignored `extracted/`
//! folder, and prints each entry's name, format and size.
//!
//! `cargo run -p nfspu-game --example fsh_dump -j 2 -- <path to .fsh>`

use bevy::image::Image;

fn main() {
    let path = std::path::PathBuf::from(std::env::args().nth(1).expect("path to an .fsh"));
    let raw = std::fs::read(&path).expect("read");
    let data = if nfspu_formats::refpack::is_refpack(&raw) { nfspu_formats::refpack::decompress(&raw).expect("refpack") } else { raw };
    let archive = nfspu_formats::shpi::parse(&data).expect("shpi");
    let stem = path.file_stem().unwrap().to_string_lossy().to_string();
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../extracted/fsh_dump").join(&stem);
    std::fs::create_dir_all(&out).expect("mkdir");
    println!("{stem}: directory '{}', {} entries", archive.directory_id, archive.entries.len());
    for (i, entry) in archive.entries.iter().enumerate() {
        let decoded = nfspu_formats::shpi::decode_rgba8(&data, entry);
        println!("  #{i} '{}' format 0x{:02X} {}x{}{}", entry.name, entry.format, entry.width, entry.height, if decoded.is_some() { "" } else { " (not decoded)" });
        if let Some(rgba) = decoded {
            let image = Image::new(
                bevy::render::render_resource::Extent3d { width: entry.width as u32, height: entry.height as u32, depth_or_array_layers: 1 },
                bevy::render::render_resource::TextureDimension::D2,
                rgba,
                bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                bevy::asset::RenderAssetUsages::default(),
            );
            if let Ok(dynamic) = image.try_into_dynamic() {
                let _ = dynamic.to_rgba8().save(out.join(format!("{i:02}_{}.png", entry.name.replace(['/', '\\', '?'], "_"))));
            }
        }
    }
}
