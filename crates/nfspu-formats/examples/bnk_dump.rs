//! Decodes every sound of an EA `.bnk` with both stereo layouts, prints a smoothness score for
//! each (real audio is smooth; a wrong layout decodes to noise), and writes WAVs of the smoother
//! layout to the ignored `extracted/bnk/<bank>/` folder for listening.
//!
//! `cargo run -p nfspu-formats --example bnk_dump --release -- <file.bnk>`

use nfspu_formats::bnk::{decode, parse, wav, Interleave};

/// How much bigger the steps are at 28-sample frame boundaries than inside frames, averaged over
/// channels. A right layout gives about 1; a wrong one jumps at every boundary.
fn boundary_ratio(samples: &[i16], channels: usize) -> f64 {
    let ch = channels.max(1);
    let mut ratios = Vec::new();
    for c in 0..ch {
        let s: Vec<f64> = samples.iter().skip(c).step_by(ch).map(|v| *v as f64).collect();
        let (mut at, mut na, mut inside, mut ni) = (0.0, 0, 0.0, 0);
        for i in 1..s.len() {
            let d = (s[i] - s[i - 1]).abs();
            if i % 28 == 0 { at += d; na += 1 } else { inside += d; ni += 1 }
        }
        if na > 0 && ni > 0 && inside > 0.0 {
            ratios.push((at / na as f64) / (inside / ni as f64));
        }
    }
    if ratios.is_empty() { 1.0 } else { ratios.iter().sum::<f64>() / ratios.len() as f64 }
}

/// Mean absolute step between neighbouring samples of one channel, relative to the mean level,
/// and the fraction of clipped samples.
fn roughness(samples: &[i16], channels: usize) -> (f64, f64) {
    let ch0: Vec<i16> = samples.iter().step_by(channels.max(1)).copied().collect();
    if ch0.len() < 2 {
        return (0.0, 0.0);
    }
    let level = ch0.iter().map(|s| (*s as f64).abs()).sum::<f64>() / ch0.len() as f64;
    let step = ch0.windows(2).map(|w| (w[1] as f64 - w[0] as f64).abs()).sum::<f64>() / (ch0.len() - 1) as f64;
    let clipped = ch0.iter().filter(|s| **s == i16::MAX || **s == i16::MIN).count() as f64 / ch0.len() as f64;
    (step / level.max(1.0), clipped)
}

fn main() {
    let path = std::path::PathBuf::from(std::env::args().nth(1).expect("path to a .bnk"));
    let data = std::fs::read(&path).expect("read");
    let name = path.file_stem().unwrap().to_string_lossy().to_string();
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../extracted/bnk").join(&name);
    std::fs::create_dir_all(&out).expect("mkdir");
    let slots = parse(&data).expect("bank");
    for (i, slot) in slots.iter().enumerate() {
        let Some(p) = slot else { continue };
        let frames = decode(&data, p, Interleave::Frames);
        let planar = decode(&data, p, Interleave::Planar);
        let (rf, cf) = roughness(&frames, p.channels as usize);
        let (rp, cp) = roughness(&planar, p.channels as usize);
        let (bf, bp) = (boundary_ratio(&frames, p.channels as usize), boundary_ratio(&planar, p.channels as usize));
        // Stereo is planar (confirmed against a retail recording with sound; see bnk.rs).
        let (best, label) = (&planar, "planar");
        println!(
            "slot {i:2}: {} ch {} Hz {:6} samples ({:.2} s)  roughness frames {rf:.2} (clip {cf:.3})  planar {rp:.2} (clip {cp:.3})  boundary frames {bf:.2} planar {bp:.2}  -> {label}",
            p.channels,
            p.sample_rate,
            p.samples,
            p.samples as f64 / p.sample_rate as f64
        );
        std::fs::write(out.join(format!("slot{i:02}.wav")), wav(best, p.channels.max(1), p.sample_rate)).expect("write");
    }
    println!("wrote {}", out.display());
}
