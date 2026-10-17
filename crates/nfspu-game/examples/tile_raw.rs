//! Writes a 24-bit (0x7F) tile of a car's embedded archive as two .ppm files, reading the bytes as
//! B,G,R and as R,G,B, to tell the channel order by eye.
//! Run: `cargo run -p nfspu-game --example tile_raw -- eas1 3 tdri`
use nfspu_formats::{crp, refpack, shpi};
use nfspu_game::car_model;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (name, idx, tile) = (&a[1], a[2].parse::<u16>().unwrap(), &a[3]);
    let raw = std::fs::read(car_model::car_dir().unwrap().join(format!("{name}.crp"))).unwrap();
    let data = if refpack::is_refpack(&raw) { refpack::decompress(&raw).unwrap() } else { raw };
    let c = crp::parse(&data).unwrap();
    let (_, blob) = c.embedded_archives().into_iter().find(|(i, _)| *i == idx).unwrap();
    let ar = shpi::parse(blob).unwrap();
    let e = ar.find(tile).unwrap();
    let n = e.width as usize * e.height as usize;
    let px = &blob[e.offset + 16..e.offset + 16 + n * 3];
    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../extracted");
    for (tag, order) in [("bgr", [2usize, 1, 0]), ("rgb", [0, 1, 2])] {
        let mut f = format!("P6\n{} {}\n255\n", e.width, e.height).into_bytes();
        for p in px.chunks_exact(3) {
            f.extend_from_slice(&[p[order[0]], p[order[1]], p[order[2]]]);
        }
        std::fs::write(out.join(format!("{name}_{tile}_{tag}.ppm")), f).unwrap();
    }
    println!("{} x {}", e.width, e.height);
}
