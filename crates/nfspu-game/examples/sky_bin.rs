//! Dumps a track sky `.bin` file record by record: the leading count, then the floats and dwords in
//! groups, so its layout can be worked out. Run: `cargo run -p nfspu-game --example sky_bin -- alps`
use std::path::PathBuf;

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "alps".to_string());
    let dir = PathBuf::from(std::env::var("NFSPU_GAME_DIR").unwrap())
        .join("GameData")
        .join("Track")
        .join("Sky");
    let b = std::fs::read(dir.join(format!("{name}.bin"))).expect("read .bin");
    let count = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    println!("{}: {} bytes, first dword {count}", name, b.len());
    println!("bytes 4..40: {:?}", &b[4..40.min(b.len())]);

    // floats, with their byte offset, skipping runs of exact zeros
    let n = (b.len() - 4) / 4;
    let f = |i: usize| f32::from_le_bytes([b[4 + i * 4], b[5 + i * 4], b[6 + i * 4], b[7 + i * 4]]);
    println!("\n{count} records over {n} floats ({:.2} floats per record)", n as f32 / count as f32);
    let per = n / count.max(1) as usize;
    for r in 0..count as usize {
        let start = r * per;
        let end = if r + 1 == count as usize { n } else { start + per };
        let vals: Vec<String> = (start..end).map(|i| format!("{:.4}", f(i))).collect();
        let dwords: Vec<String> = (start..end)
            .map(|i| {
                let o = 4 + i * 4;
                format!("{:#x}", u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]))
            })
            .collect();
        println!("  record {r:>2}: {}", vals.join(" "));
        println!("           as dwords: {}", dwords.join(" "));
    }
}