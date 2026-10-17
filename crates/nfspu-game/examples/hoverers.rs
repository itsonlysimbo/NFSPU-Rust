//! Finds small pieces of track geometry hanging above the ground, which is what a floating object
//! looks like. Anything whose bounding box is small enough to be an object (a lamp, a sign, a
//! bracket) but whose lowest point is well above the ground under it is a candidate.
//! Run: `cargo run -p nfspu-game --example hoverers -- alps`
use nfspu_game::ground::Ground;
use nfspu_game::track;
use nfspu_formats::{crp, refpack};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let name = a.get(1).cloned().unwrap_or_else(|| "alps".to_string());
    let dir = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").unwrap()).join("GameData");

    let raw = std::fs::read(dir.join("Track").join(format!("{name}.crp"))).expect("crp");
    let data = if refpack::is_refpack(&raw) { refpack::decompress(&raw).unwrap() } else { raw };
    let c = crp::parse(&data).expect("crp");

    let g = track::load_ground(&name).expect("ground");
    let height = Ground::new(g.tris.clone(), 16.0);

    println!("{name}: looking for small objects hanging above the ground");
    let mut found = 0;
    for i in 0..c.articles.len() {
        let an = c.article_name(i).unwrap_or_default();
        let Some(m) = c.article_mesh(i, 0) else { continue };
        let Some(t) = c.article_transform(i, 0) else { continue };
        if (t[12] * t[12] + t[14] * t[14]).sqrt() < 50.0 {
            continue; // origin template
        }
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in &m.positions {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let size: Vec<f32> = (0..3).map(|k| hi[k] - lo[k]).collect();
        // object-sized only: a lamp, sign or bracket, not a building or a hillside
        if size.iter().any(|s| *s > 6.0) {
            continue;
        }
        let x = t[12];
        let z = t[14];
        let bottom = lo[1] * t[5] + t[13];
        let Some(ground) = height.height_at(x, z, bottom, 4.0) else { continue };
        let gap = bottom - ground;
        if gap <= 1.0 {
            continue;
        }
        found += 1;
        if found <= 30 {
            let ty = t[13];
            println!(
                "  {an:<34} at ({x:>8.1},{ty:>7.1},{z:>8.1})  size {:.2} x {:.2} x {:.2}  bottom {gap:>5.2} m up",
                size[0], size[1], size[2]
            );
        }
    }
    println!("{found} small objects floating more than 1 m above the ground");
}