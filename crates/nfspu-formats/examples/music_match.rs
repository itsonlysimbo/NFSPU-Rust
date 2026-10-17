//! Finds which song (and where in it) a recording is playing: every song of a `.viv` music archive
//! is decoded, mixed to mono at 11025 Hz, and short clips of the recording (a mono 16-bit WAV at
//! 11025 Hz) are cross-correlated (FFT) against it. Prints the best song and offset per clip.
//!
//! `cargo run -p nfspu-formats --example music_match --release -- <zzzymus.viv> <rec.wav>`
//! `NFSPU_SONGS=menu` limits the songs by name prefix; `NFSPU_CLIP=2` sets the clip length and
//! `NFSPU_STEP=4` the spacing of clips (s).

use nfspu_formats::{asf, viv};
use std::io::Read;

const RATE: f64 = 11025.0;

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
    let mut file = std::fs::File::open(&args[1]).expect("open archive");
    let mut first = [0u8; 16];
    file.read_exact(&mut first).unwrap();
    let mut head = vec![0u8; viv::header_size(&first).unwrap()];
    std::io::Seek::rewind(&mut file).unwrap();
    file.read_exact(&mut head).unwrap();
    let w = std::fs::read(&args[2]).expect("read wav");
    let mut at = 12;
    let rec: Vec<f64> = loop {
        let size = u32::from_le_bytes(w[at + 4..at + 8].try_into().unwrap()) as usize;
        if &w[at..at + 4] == b"data" {
            break w[at + 8..(at + 8 + size).min(w.len())].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f64).collect();
        }
        at += 8 + size;
    };
    let filter = std::env::var("NFSPU_SONGS").unwrap_or_default();
    let clip_s: f64 = std::env::var("NFSPU_CLIP").ok().and_then(|v| v.parse().ok()).unwrap_or(2.0);
    let clip = (clip_s * RATE) as usize;
    let step_s: f64 = std::env::var("NFSPU_STEP").ok().and_then(|v| v.parse().ok()).unwrap_or(4.0);
    let starts: Vec<usize> = (0..).map(|k| (k as f64 * step_s * RATE) as usize).take_while(|s| s + clip <= rec.len()).collect();
    // Per clip: (score, song, offset s).
    let mut best: Vec<(f64, String, f64)> = vec![(0.0, String::new(), 0.0); starts.len()];
    for e in viv::index(&head).unwrap() {
        if !e.name.starts_with(filter.as_str()) {
            continue;
        }
        let a = asf::decode(&viv::read_entry(&mut file, &e).unwrap()).unwrap();
        let ch = a.channels as usize;
        let frames = a.samples.len() / ch;
        let step = a.sample_rate as f64 / RATE;
        let m = (frames as f64 / step) as usize;
        let song: Vec<f64> = (0..m)
            .map(|i| {
                let k = ((i as f64 * step) as usize).min(frames - 1);
                (0..ch).map(|c| a.samples[k * ch + c] as f64).sum::<f64>() / ch as f64
            })
            .collect();
        let n = (song.len() + clip).next_power_of_two();
        let (mut sr, mut si) = (vec![0.0; n], vec![0.0; n]);
        sr[..song.len()].copy_from_slice(&song);
        fft(&mut sr, &mut si, false);
        let mut sq = vec![0.0; song.len() + 1];
        for i in 0..song.len() {
            sq[i + 1] = sq[i] + song[i] * song[i];
        }
        for (ci, &s0) in starts.iter().enumerate() {
            let c = &rec[s0..s0 + clip];
            let energy: f64 = c.iter().map(|v| v * v).sum();
            if energy < 1.0 {
                continue;
            }
            let (mut cr, mut cim) = (vec![0.0; n], vec![0.0; n]);
            cr[..clip].copy_from_slice(c);
            fft(&mut cr, &mut cim, false);
            let (mut xr, mut xi) = (vec![0.0; n], vec![0.0; n]);
            for k in 0..n {
                xr[k] = sr[k] * cr[k] + si[k] * cim[k];
                xi[k] = si[k] * cr[k] - sr[k] * cim[k];
            }
            fft(&mut xr, &mut xi, true);
            for t in 0..song.len().saturating_sub(clip) {
                let local = sq[t + clip] - sq[t];
                if local > 0.0 {
                    let score = xr[t] / n as f64 / (energy.sqrt() * local.sqrt());
                    if score > best[ci].0 {
                        best[ci] = (score, e.name.clone(), t as f64 / RATE);
                    }
                }
            }
        }
        eprintln!("checked {}", e.name);
    }
    for (ci, (score, name, off)) in best.iter().enumerate() {
        let t = starts[ci] as f64 / RATE;
        println!("recording {t:5.1} s -> {name} at {off:6.2} s (score {score:.2}; song start = recording {:.2} s)", t - off);
    }
}
