//! Font check: writes a font's atlas and a sample line drawn with it to the ignored `extracted/`
//! folder, and prints the header and a few glyph records.
//!
//! `cargo run -p nfspu-game --example ffn_dump -j 2 -- <path to .ffn> [sample text]`

use nfspu_formats::ffn::Font;

fn save(path: &std::path::Path, width: u32, height: u32, grey: &[u8]) {
    let image = bevy::image::Image::new(
        bevy::render::render_resource::Extent3d { width, height, depth_or_array_layers: 1 },
        bevy::render::render_resource::TextureDimension::D2,
        grey.iter().flat_map(|&v| [v, v, v, 255]).collect(),
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::default(),
    );
    image.try_into_dynamic().expect("image").to_rgba8().save(path).expect("save");
}

fn main() {
    let path = std::path::PathBuf::from(std::env::args().nth(1).expect("path to an .ffn"));
    let sample = std::env::args().nth(2).unwrap_or_else(|| "options... Driver Greg (Ag|py)".to_string());
    let font = Font::parse(&std::fs::read(&path).expect("read")).expect("not an FFN font");
    println!("line height {}, {} glyphs, atlas {}x{}", font.line_height, font.glyphs.len(), font.atlas_width, font.atlas_height);
    for c in ['A', 'g', 'p', ' ', '.', 'é'] {
        println!("  {c:?} {:?}", font.glyph(c));
    }

    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../extracted/ffn_dump");
    std::fs::create_dir_all(&out).expect("mkdir");
    let stem = path.file_stem().unwrap().to_string_lossy().to_string();
    save(&out.join(format!("{stem}_atlas.png")), font.atlas_width, font.atlas_height, &font.coverage);

    // The sample line, 2 px margin, glyph tops at pen y + y offset.
    let width = font.measure(&sample) + 4;
    let height = font.line_height as u32 + 4;
    let mut line = vec![0u8; (width * height) as usize];
    let mut pen = 2i32;
    for c in sample.chars() {
        let Some(g) = font.glyph(c) else { continue };
        for gy in 0..g.height as i32 {
            for gx in 0..g.width as i32 {
                let src = ((g.y as i32 + gy) * font.atlas_width as i32 + g.x as i32 + gx) as usize;
                let (dx, dy) = (pen + g.x_offset as i32 + gx, 2 + g.y_offset as i32 + gy);
                if dx >= 0 && dy >= 0 && (dx as u32) < width && (dy as u32) < height {
                    let dst = (dy as u32 * width + dx as u32) as usize;
                    line[dst] = line[dst].max(font.coverage[src]);
                }
            }
        }
        pen += g.advance as i32;
    }
    save(&out.join(format!("{stem}_sample.png")), width, height, &line);
    println!("wrote {}", out.display());
}
