//! Lists the placed objects in a track whose name suggests a lamp, light or post, with where they
//! sit and how big they are, so a floating one can be spotted.
//! Run: `cargo run -p nfspu-game --example track_lamps -- alps`
use nfspu_formats::{crp, refpack};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let name = a.get(1).cloned().unwrap_or_else(|| "alps".to_string());
    let dir = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").unwrap()).join("GameData");
    let raw = std::fs::read(dir.join("Track").join(format!("{name}.crp"))).expect("crp");
    let data = if refpack::is_refpack(&raw) { refpack::decompress(&raw).unwrap() } else { raw };
    let c = crp::parse(&data).expect("crp");
    println!("{}: {} articles", name, c.articles.len());

    let want = |n: &str| {
        let u = n.to_ascii_uppercase();
        ["LAMP", "LIGHT", "POST", "LITERN", "LANTERN", "POSTE"].iter().any(|k| u.contains(k))
    };

    let mut rows: Vec<String> = Vec::new();
    let mut placed = 0usize;
    for i in 0..c.articles.len() {
        let Some(an) = c.article_name(i) else { continue };
        let Some(t) = c.article_transform(i, 0) else { continue };
        let (x, z) = (t[12], t[14]);
        if (x * x + z * z).sqrt() < 50.0 {
            continue; // template at the origin
        }
        placed += 1;
        let all = std::env::var("ALL").is_ok();
        if !all && !want(&an) {
            continue;
        }
        // how big is it, and how high off the ground does it sit?
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        if let Some(m) = c.article_mesh(i, 0) {
            for p in &m.positions {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
        }
        let size: Vec<String> = (0..3).map(|k| format!("{:.2}", hi[k] - lo[k])).collect();
        let ytop = lo[1] * t[5] + t[13] + (hi[1] - lo[1]);
        rows.push(format!(
            "  article {i:<5} {an:<28} at ({:>8.1},{:>8.1},{:>8.1})  size {}  top y {ytop:.2}",
            x,
            t[13],
            z,
            size.join(" x ")
        ));
    }
    println!("{placed} placed objects (transform more than 50 m from the origin)");
    println!("{} of them look like lamps or posts:", rows.len());
    for r in &rows {
        println!("{r}");
    }
}