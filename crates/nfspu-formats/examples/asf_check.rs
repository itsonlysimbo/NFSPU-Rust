//! Decodes one song from a `.viv` music archive with our decoder and compares it sample by sample
//! with a reference WAV (16-bit, same rate and channels, e.g. FFmpeg's decode of the same stream).
//! Lists the archive when no song name is given.
//!
//! `cargo run -p nfspu-formats --example asf_check --release -- <zzzymus.viv> [song name prefix] [reference.wav]`

use nfspu_formats::{asf, viv};
use std::io::Read;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut file = std::fs::File::open(&args[1]).expect("open archive");
    let mut first = [0u8; 16];
    file.read_exact(&mut first).expect("read");
    let mut head = vec![0u8; viv::header_size(&first).expect("BIGF")];
    std::io::Seek::rewind(&mut file).unwrap();
    file.read_exact(&mut head).expect("read index");
    let entries = viv::index(&head).expect("index");
    let Some(prefix) = args.get(2) else {
        for e in &entries {
            println!("{:10} {:9} {}", e.offset, e.size, e.name);
        }
        return;
    };
    let entry = entries.iter().find(|e| e.name.starts_with(prefix.as_str())).expect("no such song");
    let data = viv::read_entry(&mut file, entry).expect("read song");
    let t = std::time::Instant::now();
    let audio = asf::decode(&data).expect("decode");
    println!(
        "{}: {} ch, {} Hz, {} frames ({:.1} s), decoded in {:.0} ms",
        entry.name,
        audio.channels,
        audio.sample_rate,
        audio.samples.len() / audio.channels as usize,
        audio.samples.len() as f64 / audio.channels as f64 / audio.sample_rate as f64,
        t.elapsed().as_secs_f64() * 1000.0
    );
    let Some(reference) = args.get(3) else { return };
    let w = std::fs::read(reference).expect("read wav");
    let mut at = 12;
    let pcm: Vec<i16> = loop {
        let size = u32::from_le_bytes(w[at + 4..at + 8].try_into().unwrap()) as usize;
        if &w[at..at + 4] == b"data" {
            break w[at + 8..(at + 8 + size).min(w.len())].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
        }
        at += 8 + size;
    };
    let n = pcm.len().min(audio.samples.len());
    let (mut differ, mut worst) = (0usize, 0i32);
    for i in 0..n {
        let d = (pcm[i] as i32 - audio.samples[i] as i32).abs();
        if d != 0 {
            differ += 1;
        }
        worst = worst.max(d);
    }
    println!("reference {} samples, ours {}; compared {n}: {differ} differ, worst difference {worst}", pcm.len(), audio.samples.len());
}
