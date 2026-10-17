//! Saves frames of a `.mad` movie, decoded by our own decoder, as PNGs in the ignored
//! `extracted/mad_dump/<name>/` folder.
//!
//! `cargo run -p nfspu-game --example mad_dump --release -- <file.mad> [every_nth=15] [first] [last]`

use nfspu_formats::mad::{chunks, Decoder};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = std::path::PathBuf::from(&args[1]);
    let every: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(15);
    let first: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    let last: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);
    let data = std::fs::read(&path).expect("read movie");
    let name = path.file_stem().unwrap().to_string_lossy().to_string();
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../extracted/mad_dump").join(&name);
    std::fs::create_dir_all(&out).expect("mkdir");
    let mut decoder = Decoder::new();
    let mut frame = 0usize;
    let mut saved = 0usize;
    for chunk in chunks(&data) {
        let chunk = chunk.expect("chunk");
        if !chunk.is_video() {
            continue;
        }
        let pic = decoder.decode(&chunk).expect("decode");
        if frame >= first && frame <= last && (frame - first) % every == 0 {
            let image = bevy::image::Image::new(
                bevy::render::render_resource::Extent3d { width: pic.width as u32, height: pic.height as u32, depth_or_array_layers: 1 },
                bevy::render::render_resource::TextureDimension::D2,
                pic.to_rgba(),
                bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                bevy::asset::RenderAssetUsages::default(),
            );
            image.try_into_dynamic().expect("image").to_rgba8().save(out.join(format!("f_{frame:05}.png"))).expect("save");
            saved += 1;
        }
        frame += 1;
    }
    println!("{name}: {frame} frames, saved {saved} to {}", out.display());
}
