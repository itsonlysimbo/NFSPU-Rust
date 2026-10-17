//! Decompresses one data file from the player's install into the gitignored `extracted/` folder
//! so it can be inspected with other tools. The path is relative to `GameData/`; a bare file name
//! is looked up in `GameData/Track/`.
//! Usage: `cargo run -p nfspu-formats --example dump --release -- alps.crp`
//!        `cargo run -p nfspu-formats --example dump --release -- CarModel/993.crp`

use nfspu_formats::refpack;
use std::{env, fs, path::PathBuf};

fn main() {
    let game = env::var("NFSPU_GAME_DIR").expect("set NFSPU_GAME_DIR");
    let arg = env::args().nth(1).expect("give a file, e.g. alps.crp or CarModel/993.crp");
    let rel = if arg.contains('/') || arg.contains('\\') { PathBuf::from(&arg) } else { PathBuf::from("Track").join(&arg) };
    let data = fs::read(PathBuf::from(game).join("GameData").join(&rel)).expect("cannot read file");
    let compressed = refpack::is_refpack(&data);
    let out = if compressed { refpack::decompress(&data).expect("decode failed") } else { data };
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("extracted");
    fs::create_dir_all(&dir).unwrap();
    let name = rel.file_name().unwrap().to_string_lossy().to_string();
    let path = dir.join(format!("{name}.bin"));
    fs::write(&path, &out).unwrap();
    println!("{} ({}) -> {} bytes in {}", name, if compressed { "RefPack" } else { "stored raw" }, out.len(), path.display());
}
