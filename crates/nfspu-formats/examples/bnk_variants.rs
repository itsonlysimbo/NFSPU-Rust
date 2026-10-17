//! Tries decoder variants for EA "XA" bank audio against a recording of the game: for each
//! variant (nibble order, scale rule, stereo layout) every sound of the bank is decoded and
//! cross-correlated with the recording; a right decoder makes the sounds that really play show
//! up as strong peaks. Prints the best peaks per variant.
//!
//! `cargo run -p nfspu-formats --example bnk_variants --release -- <audio.wav 22050 mono> <bank.bnk>`

use nfspu_formats::bnk::parse;

const RATE: f64 = 22050.0;
const COEF1: [i32; 4] = [0, 240, 460, 392];
const COEF2: [i32; 4] = [0, 0, -208, -220];

#[derive(Clone, Copy, Debug)]
struct Variant {
    low_first: bool,
    inverted_scale: bool,
    planar: bool,
}

fn decode_channel(data: &[u8], mut at: usize, stride: usize, n: usize, v: Variant) -> Vec<f64> {
    let (mut h1, mut h2) = (0i32, 0i32);
    let mut out = Vec::with_capacity(n);
    while out.len() < n {
        let Some(f) = data.get(at..at + 15) else { break };
        let idx = (f[0] >> 4) as usize & 3;
        let scale = (f[0] & 15) as i32;
        let shift = if v.inverted_scale { 20 - scale } else { scale + 8 };
        for i in 0..28 {
            if out.len() >= n {
                break;
            }
            let b = f[1 + i / 2];
            let high = i % 2 == 0;
            let nib = if high != v.low_first { b >> 4 } else { b & 15 } as i32;
            let delta = if shift >= 0 { (nib << 28) >> shift.min(31) } else { 0 };
            let s = ((delta + COEF1[idx] * h1 + COEF2[idx] * h2 + 128) >> 8).clamp(-32768, 32767);
            h2 = h1;
            h1 = s;
            out.push(s as f64);
        }
        at += stride;
    }
    out
}

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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let wav = std::fs::read(&args[1]).unwrap();
    let mut at = 12;
    let rec: Vec<f64> = loop {
        let size = u32::from_le_bytes(wav[at + 4..at + 8].try_into().unwrap()) as usize;
        if &wav[at..at + 4] == b"data" {
            break wav[at + 8..(at + 8 + size).min(wav.len())].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f64).collect();
        }
        at += 8 + size;
    };
    let n = (rec.len() + 4 * 22050).next_power_of_two();
    let (mut rr, mut ri) = (vec![0.0; n], vec![0.0; n]);
    rr[..rec.len()].copy_from_slice(&rec);
    fft(&mut rr, &mut ri, false);
    let mut sq = vec![0.0; rec.len() + 1];
    for i in 0..rec.len() {
        sq[i + 1] = sq[i] + rec[i] * rec[i];
    }
    let bank = std::fs::read(&args[2]).unwrap();
    let slots = parse(&bank).unwrap();
    let all = std::env::var("NFSPU_ALL_VARIANTS").is_ok();
    let min_score: f64 = std::env::var("NFSPU_MATCH_MIN").ok().and_then(|v| v.parse().ok()).unwrap_or(0.45);
    for low_first in if all { vec![false, true] } else { vec![false] } {
        for inverted_scale in if all { vec![false, true] } else { vec![false] } {
            for planar in if all { vec![false, true] } else { vec![true] } {
                let v = Variant { low_first, inverted_scale, planar };
                let mut best: Vec<(f64, usize, f64)> = Vec::new();
                for (i, slot) in slots.iter().enumerate() {
                    let Some(p) = slot else { continue };
                    let ch = p.channels.max(1) as usize;
                    let total = p.samples as usize;
                    let frames = total.div_ceil(28);
                    let chans: Vec<Vec<f64>> = (0..ch)
                        .map(|c| {
                            let (start, stride) = if planar { (p.data_offset + c * frames * 15, 15) } else { (p.data_offset + c * 15, 15 * ch) };
                            decode_channel(&bank, start, stride, total, v)
                        })
                        .collect();
                    let len = chans.iter().map(Vec::len).min().unwrap_or(0);
                    let mono: Vec<f64> = (0..len).map(|k| chans.iter().map(|c| c[k]).sum::<f64>() / ch as f64).collect();
                    // Resample to 22050, first 0.4 s.
                    let step = p.sample_rate as f64 / RATE;
                    let m = ((mono.len() as f64 / step) as usize).min(8820);
                    let s: Vec<f64> = (0..m).map(|k| mono[((k as f64 * step) as usize).min(mono.len() - 1)]).collect();
                    let energy: f64 = s.iter().map(|x| x * x).sum();
                    if s.len() < 200 || energy < 1.0 {
                        continue;
                    }
                    let (mut sr, mut si) = (vec![0.0; n], vec![0.0; n]);
                    sr[..s.len()].copy_from_slice(&s);
                    fft(&mut sr, &mut si, false);
                    let (mut cr, mut ci) = (vec![0.0; n], vec![0.0; n]);
                    for k in 0..n {
                        cr[k] = rr[k] * sr[k] + ri[k] * si[k];
                        ci[k] = ri[k] * sr[k] - rr[k] * si[k];
                    }
                    fft(&mut cr, &mut ci, true);
                    let mut top = (0.0, 0usize);
                    let mut peaks: Vec<(f64, usize)> = Vec::new();
                    for t in 0..rec.len().saturating_sub(s.len()) {
                        let local = sq[t + s.len()] - sq[t];
                        if local > 0.0 {
                            let score = cr[t] / n as f64 / (energy.sqrt() * local.sqrt());
                            if score > top.0 {
                                top = (score, t);
                            }
                            if score > min_score {
                                match peaks.last_mut() {
                                    Some(last) if t - last.1 < 6615 => { if score > last.0 { *last = (score, t) } }
                                    _ => peaks.push((score, t)),
                                }
                            }
                        }
                    }
                    // Optional: best score inside given time windows (NFSPU_WINDOWS=10.5-11.7,12.7-13.6).
                    if let Ok(spec) = std::env::var("NFSPU_WINDOWS") {
                        let mut parts = Vec::new();
                        let mut worst = f64::MAX;
                        for w in spec.split(',') {
                            let Some((a, b)) = w.split_once('-') else { continue };
                            let (a, b) = ((a.parse::<f64>().unwrap() * RATE) as usize, (b.parse::<f64>().unwrap() * RATE) as usize);
                            let mut m = (0.0f64, 0usize);
                            for t in a..b.min(rec.len().saturating_sub(s.len())) {
                                let local = sq[t + s.len()] - sq[t];
                                if local > 0.0 {
                                    let score = cr[t] / n as f64 / (energy.sqrt() * local.sqrt());
                                    if score > m.0 { m = (score, t) }
                                }
                            }
                            worst = worst.min(m.0);
                            parts.push(format!("{:.2}@{:.2}s", m.0, m.1 as f64 / RATE));
                        }
                        println!("   window slot {i:2} worst {worst:.2}: {}", parts.join(" "));
                        continue;
                    }
                    if !peaks.is_empty() {
                        let list: Vec<String> = peaks.iter().map(|(sc, t)| format!("{:.2}s({sc:.2})", *t as f64 / RATE)).collect();
                        println!("   {v:?} slot {i}: {}", list.join(" "));
                    }
                    best.push((top.0, i, top.1 as f64 / RATE));
                }
                best.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
                let list: Vec<String> = best.iter().take(6).map(|(s, i, t)| format!("slot {i} {s:.2} @{t:.2}s")).collect();
                println!("{v:?}: {}", list.join(", "));
            }
        }
    }
}
