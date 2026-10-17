//! Reads every track `.crp` / `.fsh` from the player's own install, decompresses it, and
//! prints sizes and the first four bytes. Output goes to the console only.
//!
//! Run: `$env:NFSPU_GAME_DIR="C:\Users\CB\Downloads\NFS5"; cargo run -p nfspu-formats --example probe --release`

use nfspu_formats::{refpack, shpi};
use std::{env, fs, path::PathBuf};

fn magic(b: &[u8]) -> String {
    b.iter()
        .take(4)
        .map(|c| if (32..127).contains(c) { *c as char } else { '.' })
        .collect()
}

fn main() {
    let game = env::var("NFSPU_GAME_DIR").expect("set NFSPU_GAME_DIR to your game folder");
    let dir = PathBuf::from(game).join("GameData").join("Track");
    let mut entries: Vec<_> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| matches!(p.extension().and_then(|e| e.to_str()), Some("crp") | Some("fsh")))
        .collect();
    entries.sort();

    let (mut ok, mut failed) = (0, 0);
    println!("{:<22} {:>11} {:>11}  magic", "file", "compressed", "decoded");
    for p in &entries {
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        let data = fs::read(p).unwrap();
        if !refpack::is_refpack(&data) {
            println!("{name:<22} {:>11} {:>11}  (not RefPack) {}", data.len(), "-", magic(&data));
            continue;
        }
        match refpack::decompress(&data) {
            Ok(out) => {
                ok += 1;
                let extra = match shpi::parse(&out) {
                    Ok(a) => {
                        let (mut w, mut h) = (0u16, 0u16);
                        for e in &a.entries { w = w.max(e.width); h = h.max(e.height); }
                        format!("  SHPI: {} textures, largest {}x{}", a.entries.len(), w, h)
                    }
                    Err(_) => String::new(),
                };
                println!("{name:<22} {:>11} {:>11}  {}{extra}", data.len(), out.len(), magic(&out));
            }
            Err(e) => {
                failed += 1;
                println!("{name:<22} {:>11} {:>11}  ERROR: {e}", data.len(), "-");
            }
        }
    }
    println!("\ndecoded OK: {ok}, failed: {failed}, total files: {}", entries.len());
}
