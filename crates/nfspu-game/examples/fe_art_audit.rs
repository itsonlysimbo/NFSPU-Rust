//! Front-end art audit: decodes every image of every `FEData/Art/*.fsh` and reports which pixel
//! formats the menu renderer can and cannot draw yet. Writes `logs/fe_art_audit.txt`.
//!
//! `$env:NFSPU_GAME_DIR="C:\path\to\game"; cargo run -p nfspu-game --example fe_art_audit -j 2`

use nfspu_game::menu::{decode_shape, load_art, ShapeError};
use std::collections::BTreeMap;
use std::fmt::Write as _;

fn main() {
    let game_dir = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").expect("set NFSPU_GAME_DIR"));
    let art = game_dir.join("FEData").join("Art");
    let mut files: Vec<_> = std::fs::read_dir(&art).expect("FEData/Art").flatten().map(|e| e.path()).collect();
    files.retain(|p| p.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("fsh")).unwrap_or(false));
    files.sort();

    let mut out = String::new();
    let (mut ok, mut failed) = (0usize, 0usize);
    let mut failed_formats: BTreeMap<String, usize> = BTreeMap::new();
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let data = match load_art(&game_dir, &name) {
            Ok(d) => d,
            Err(e) => {
                let _ = writeln!(out, "{name:<28} LOAD FAILED {e:?}");
                failed += 1;
                continue;
            }
        };
        let mut sizes = Vec::new();
        for i in 0.. {
            match decode_shape(&data, i) {
                Ok(s) => {
                    ok += 1;
                    sizes.push(format!("{}x{}", s.width, s.height));
                }
                Err(ShapeError::NoEntry { .. }) => break,
                Err(ShapeError::Format(f)) => {
                    failed += 1;
                    *failed_formats.entry(format!("0x{f:02X}")).or_default() += 1;
                    sizes.push(format!("fmt 0x{f:02X}"));
                }
                Err(e) => {
                    failed += 1;
                    sizes.push(format!("{e:?}"));
                    break;
                }
            }
        }
        let _ = writeln!(out, "{name:<28} {}", sizes.join(" "));
    }
    let summary = format!("files {}, images decoded {ok}, not decoded {failed}, undecoded formats {failed_formats:?}\n", files.len());
    out.insert_str(0, &summary);
    print!("{summary}");
    let log = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../logs/fe_art_audit.txt");
    std::fs::write(&log, out).expect("write log");
}
