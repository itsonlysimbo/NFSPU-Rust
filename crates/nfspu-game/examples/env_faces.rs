//! Dumps the six cube-map faces of a track's .env sky to `extracted/<track>_sky*.ppm`.
//! Run: `cargo run -p nfspu-game --example env_faces -- alps`
use nfspu_formats::shpi;

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "alps".to_string());
    let dir = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").unwrap()).join("GameData").join("Track");
    let blob = std::fs::read(dir.join(format!("{name}.env"))).expect("read .env");
    let a = shpi::parse(&blob).expect("SHPI");
    println!("{} entries", a.entries.len());
    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../extracted");
    for e in &a.entries {
        let rgba = shpi::decode_rgba8(&blob, e);
        println!("  {:<6} code {:#04x} {:>3}x{:<3} at ({:>3},{:>3}) decoded: {}", e.name, e.format, e.width, e.height, e.x, e.y, rgba.is_some());
        let Some(rgba) = rgba else { continue };
        let mut f = format!("P6\n{} {}\n255\n", e.width, e.height).into_bytes();
        for px in rgba.chunks_exact(4) {
            f.extend_from_slice(&px[..3]);
        }
        std::fs::write(out.join(format!("{}_sky_{}.ppm", name, e.name.trim())), f).unwrap();
    }
}
