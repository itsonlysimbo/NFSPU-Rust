//! Finds which movie clip and frame a screen recording of the retail main menu shows. Each
//! recording frame's movie area (the oval's centre) is compared with every frame of every clip,
//! decoded with our own decoder, using brightness-normalised correlation of small grey thumbnails.
//! Prints the best match per recording frame.
//!
//! `cargo run -p nfspu-game --example mad_match --release -- <movies dir> <dir of 640x480 PNGs>`

use nfspu_formats::mad::{chunks, Decoder};

const TW: usize = 32;
const TH: usize = 24;
/// The part of the movie used for matching (fractions of its width/height): the centre, which the
/// oval always shows.
const U0: f32 = 0.2;
const U1: f32 = 0.8;
const V0: f32 = 0.2;
const V1: f32 = 0.8;
/// The movie rectangle in the 640x480 main menu (`NFS5.MOVIECIRCLE` on `GameSetup`).
const RECT: [f32; 4] = [112.0, 100.0, 416.0, 348.0];

/// A normalised grey thumbnail (zero mean, unit length) of a region of an RGB(A) image.
fn thumb(sample: impl Fn(f32, f32) -> f32) -> Vec<f32> {
    let mut v = Vec::with_capacity(TW * TH);
    for ty in 0..TH {
        for tx in 0..TW {
            let u = U0 + (U1 - U0) * (tx as f32 + 0.5) / TW as f32;
            let w = V0 + (V1 - V0) * (ty as f32 + 0.5) / TH as f32;
            v.push(sample(u, w));
        }
    }
    let mean = v.iter().sum::<f32>() / v.len() as f32;
    v.iter_mut().for_each(|x| *x -= mean);
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-3);
    v.iter_mut().for_each(|x| *x /= norm);
    v
}

fn grey(rgba: &[u8], width: usize, x: usize, y: usize) -> f32 {
    let at = (y * width + x) * 4;
    0.299 * rgba[at] as f32 + 0.587 * rgba[at + 1] as f32 + 0.114 * rgba[at + 2] as f32
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let movies = std::path::PathBuf::from(&args[1]);
    let shots = std::path::PathBuf::from(&args[2]);

    // Every frame of every clip, as thumbnails.
    let mut library: Vec<(String, usize, Vec<f32>)> = Vec::new();
    let mut names: Vec<_> = std::fs::read_dir(&movies).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().map(|e| e.eq_ignore_ascii_case("mad")).unwrap_or(false)).collect();
    names.sort();
    for path in names {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let data = std::fs::read(&path).unwrap();
        let mut decoder = Decoder::new();
        let mut n = 0;
        for chunk in chunks(&data).flatten().filter(|c| c.is_video()) {
            let Ok(pic) = decoder.decode(&chunk) else { continue };
            let rgba = pic.to_rgba();
            let (w, h) = (pic.width, pic.height);
            library.push((name.clone(), n, thumb(|u, v| grey(&rgba, w, ((u * w as f32) as usize).min(w - 1), ((v * h as f32) as usize).min(h - 1)))));
            n += 1;
        }
        eprintln!("{name}: {n} frames");
    }

    let mut shot_paths: Vec<_> = std::fs::read_dir(&shots).unwrap().flatten().map(|e| e.path()).filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("r_")).collect();
    shot_paths.sort();
    for path in shot_paths {
        let bytes = std::fs::read(&path).unwrap();
        let image = bevy::image::Image::from_buffer(
            &bytes,
            bevy::image::ImageType::Extension("png"),
            bevy::image::CompressedImageFormats::NONE,
            true,
            bevy::image::ImageSampler::Default,
            bevy::asset::RenderAssetUsages::default(),
        )
        .expect("png");
        let dynamic = image.try_into_dynamic().expect("image").to_rgba8();
        let (w, h) = (dynamic.width() as usize, dynamic.height() as usize);
        let raw = dynamic.into_raw();
        let [rx, ry, rw, rh] = RECT;
        let sx = w as f32 / 640.0;
        let sy = h as f32 / 480.0;
        let t = thumb(|u, v| grey(&raw, w, (((rx + u * rw) * sx) as usize).min(w - 1), (((ry + v * rh) * sy) as usize).min(h - 1)));
        let mut best = (f32::MIN, String::new(), 0usize);
        for (name, frame, lib) in &library {
            let score: f32 = t.iter().zip(lib).map(|(a, b)| a * b).sum();
            if score > best.0 {
                best = (score, name.clone(), *frame);
            }
        }
        println!("{} -> {} frame {} (score {:.2})", path.file_stem().unwrap().to_string_lossy(), best.1, best.2, best.0);
    }
}
