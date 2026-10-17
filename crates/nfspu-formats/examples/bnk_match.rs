//! Finds which bank sounds play in a recording: every sound of the given `.bnk` files is decoded,
//! mixed to mono at 22050 Hz and cross-correlated (FFT) with the recording (a mono 16-bit WAV at
//! 22050 Hz, e.g. from `ffmpeg -i rec.mp4 -vn -ac 1 -ar 22050 -c:a pcm_s16le audio.wav`).
//! Prints, per sound, the strongest normalised correlation peaks and their times.
//!
//! `cargo run -p nfspu-formats --example bnk_match --release -- <audio.wav> <a.bnk> [b.bnk ...]`

use nfspu_formats::bnk::{decode, parse, Interleave};

const RATE: f64 = 22050.0;
/// Peaks below this normalised correlation are not listed (`NFSPU_MATCH_MIN` overrides).
static MIN_SCORE: std::sync::LazyLock<f64> = std::sync::LazyLock::new(|| std::env::var("NFSPU_MATCH_MIN").ok().and_then(|v| v.parse().ok()).unwrap_or(0.5));

/// Minimal complex FFT (radix 2, in place). `inverse` uses the conjugate transform without scaling.
fn fft(re: &mut [f64], im: &mut [f64], inverse: bool) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = 2.0 * std::f64::consts::PI / len as f64 * if inverse { 1.0 } else { -1.0 };
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0, 0.0);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[b] * cr - im[b] * ci, re[b] * ci + im[b] * cr);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let ncr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = ncr;
            }
        }
        len <<= 1;
    }
}

fn read_wav_mono16(path: &str) -> Vec<f64> {
    let d = std::fs::read(path).expect("read wav");
    // Find the "data" chunk.
    let mut at = 12;
    while at + 8 <= d.len() {
        let size = u32::from_le_bytes(d[at + 4..at + 8].try_into().unwrap()) as usize;
        if &d[at..at + 4] == b"data" {
            return d[at + 8..(at + 8 + size).min(d.len())].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f64).collect();
        }
        at += 8 + size;
    }
    panic!("no data chunk");
}

/// Mono, resampled (linear) to 22050 Hz.
fn to_mono_22k(pcm: &[i16], channels: usize, rate: u32) -> Vec<f64> {
    let ch = channels.max(1);
    let mono: Vec<f64> = pcm.chunks(ch).map(|f| f.iter().map(|s| *s as f64).sum::<f64>() / ch as f64).collect();
    let step = rate as f64 / RATE;
    let n = (mono.len() as f64 / step) as usize;
    (0..n)
        .map(|i| {
            let p = i as f64 * step;
            let k = p as usize;
            let f = p - k as f64;
            let a = mono[k.min(mono.len() - 1)];
            let b = mono[(k + 1).min(mono.len() - 1)];
            a * (1.0 - f) + b * f
        })
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let rec = read_wav_mono16(&args[1]);
    let n = (rec.len() + 4 * 22050).next_power_of_two();
    let (mut rr, mut ri) = (vec![0.0; n], vec![0.0; n]);
    rr[..rec.len()].copy_from_slice(&rec);
    fft(&mut rr, &mut ri, false);
    // Prefix sums of squares, for the recording's local energy under each placement.
    let mut sq = vec![0.0; rec.len() + 1];
    for i in 0..rec.len() {
        sq[i + 1] = sq[i] + rec[i] * rec[i];
    }
    for bank_path in &args[2..] {
        let data = std::fs::read(bank_path).expect("read bank");
        let name = std::path::Path::new(bank_path).file_stem().unwrap().to_string_lossy().to_string();
        let Ok(slots) = parse(&data) else { continue };
        for (i, slot) in slots.iter().enumerate() {
            let Some(p) = slot else { continue };
            let pcm = decode(&data, p, Interleave::Frames);
            // Use at most the first 0.5 s: enough to identify, and less smeared by the music.
            let mut s = to_mono_22k(&pcm, p.channels as usize, p.sample_rate);
            s.truncate(11025);
            let energy: f64 = s.iter().map(|v| v * v).sum();
            if s.len() < 200 || energy < 1.0 {
                continue;
            }
            let (mut sr, mut si) = (vec![0.0; n], vec![0.0; n]);
            sr[..s.len()].copy_from_slice(&s);
            fft(&mut sr, &mut si, false);
            // Correlation = IFFT(R * conj(S)).
            let (mut cr, mut ci) = (vec![0.0; n], vec![0.0; n]);
            for k in 0..n {
                cr[k] = rr[k] * sr[k] + ri[k] * si[k];
                ci[k] = ri[k] * sr[k] - rr[k] * si[k];
            }
            fft(&mut cr, &mut ci, true);
            let mut peaks: Vec<(f64, usize)> = Vec::new();
            for t in 0..rec.len().saturating_sub(s.len()) {
                let local = sq[t + s.len()] - sq[t];
                if local <= 0.0 {
                    continue;
                }
                let score = cr[t] / n as f64 / (energy.sqrt() * local.sqrt());
                if score > *MIN_SCORE {
                    // Keep one peak per 0.3 s.
                    if let Some(last) = peaks.last_mut() {
                        if t - last.1 < 6615 {
                            if score > last.0 {
                                *last = (score, t);
                            }
                            continue;
                        }
                    }
                    peaks.push((score, t));
                }
            }
            peaks.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
            peaks.truncate(4);
            peaks.sort_by_key(|p| p.1);
            if !peaks.is_empty() {
                let list: Vec<String> = peaks.iter().map(|(s, t)| format!("{:.2}s ({s:.2})", *t as f64 / RATE)).collect();
                println!("{name} slot {i:2}: {}", list.join(", "));
            }
        }
    }
}
