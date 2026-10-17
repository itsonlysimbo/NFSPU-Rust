//! Decodes a `.mad` movie and compares every frame with a reference decode (raw yuv420p, e.g.
//! from FFmpeg: `ffmpeg -i ch6.mad -f rawvideo -pix_fmt yuv420p ref.yuv`). Prints a summary and
//! writes per-frame numbers to `logs/mad_check.txt`.
//!
//! `cargo run -p nfspu-formats --example mad_check --release -- <file.mad> <ref.yuv>`

use nfspu_formats::mad::{chunks, Decoder};
use std::fmt::Write as _;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let movie = std::fs::read(&args[1]).expect("read movie");
    let reference = args.get(2).map(|p| std::fs::read(p).expect("read reference"));

    let mut decoder = Decoder::new();
    let mut log = String::new();
    let (mut frames, mut exact, mut worst, mut sum_mse) = (0usize, 0usize, 0u8, 0f64);
    let mut first_bad: Option<usize> = None;
    for chunk in chunks(&movie) {
        let chunk = chunk.expect("chunk");
        if !chunk.is_video() {
            continue;
        }
        let tag = String::from_utf8_lossy(&chunk.tag).to_string();
        let pic = match decoder.decode(&chunk) {
            Ok(p) => p.clone(),
            Err(e) => {
                let _ = writeln!(log, "frame {frames} {tag}: DECODE ERROR {e}");
                println!("frame {frames} {tag}: decode error {e}");
                frames += 1;
                continue;
            }
        };
        if let Some(r) = &reference {
            let size = pic.y.len() + pic.cb.len() + pic.cr.len();
            let Some(want) = r.get(frames * size..(frames + 1) * size) else { break };
            let ours: Vec<u8> = pic.y.iter().chain(&pic.cb).chain(&pic.cr).copied().collect();
            let (mut max, mut differ, mut se) = (0u8, 0usize, 0f64);
            for (a, b) in ours.iter().zip(want) {
                let d = a.abs_diff(*b);
                max = max.max(d);
                differ += (d != 0) as usize;
                se += (d as f64) * (d as f64);
            }
            let mse = se / size as f64;
            sum_mse += mse;
            worst = worst.max(max);
            exact += (max == 0) as usize;
            if max > 4 && first_bad.is_none() {
                first_bad = Some(frames);
            }
            let _ = writeln!(log, "frame {frames:4} {tag} max {max:3} differing {differ:6} mse {mse:.3}");
        }
        frames += 1;
    }
    let psnr = if sum_mse > 0.0 { 10.0 * (255.0f64 * 255.0 / (sum_mse / frames as f64)).log10() } else { f64::INFINITY };
    let summary = format!("frames {frames}, exact {exact}, worst sample difference {worst}, mean PSNR {psnr:.1} dB, first frame with difference > 4: {first_bad:?}\n");
    print!("{summary}");
    log.insert_str(0, &summary);
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../logs/mad_check.txt");
    std::fs::write(out, log).expect("write log");
}
