//! Lists what the viewer leaves out of a track: articles skipped as origin templates, articles with
//! no mesh at LOD 0, and how many LODs each has. Used to explain missing geometry such as a lamp
//! that appears to float because its support is not drawn.
//! Run: `cargo run -p nfspu-game --example skipped -- alps`
use nfspu_formats::{crp, refpack};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let name = a.get(1).cloned().unwrap_or_else(|| "alps".to_string());
    let dir = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").unwrap()).join("GameData");
    let raw = std::fs::read(dir.join("Track").join(format!("{name}.crp"))).expect("crp");
    let data = if refpack::is_refpack(&raw) { refpack::decompress(&raw).unwrap() } else { raw };
    let c = crp::parse(&data).expect("crp");

    let (mut skipped, mut no_mesh, mut drawn) = (Vec::new(), 0usize, 0usize);
    let mut lod_counts: std::collections::HashMap<usize, usize> = Default::default();
    for i in 0..c.articles.len() {
        let an = c.article_name(i).unwrap_or_else(|| format!("<article {i}>"));
        // how many LODs does this article have?
        let lods = (0u8..8).filter(|l| c.article_mesh(i, *l).is_some()).count();
        *lod_counts.entry(lods).or_default() += 1;
        if c.article_mesh(i, 0).is_none() {
            no_mesh += 1;
            continue;
        }
        match c.article_transform(i, 0) {
            Some(t) if (t[12] * t[12] + t[14] * t[14]).sqrt() < 50.0 => {
                skipped.push((i, an, t[13]));
            }
            _ => drawn += 1,
        }
    }
    println!("{name}: {} articles", c.articles.len());
    println!("  {drawn} drawn, {} skipped as origin templates, {no_mesh} with no LOD 0 mesh", skipped.len());
    println!("  articles by number of LODs: {lod_counts:?}");
    println!("\n  the {len} skipped templates:", len = skipped.len());
    for (i, an, y) in &skipped {
        println!("    article {i:<5} {an:<44} y {y:>8.1}");
    }
}