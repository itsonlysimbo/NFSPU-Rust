//! Finds track glows: `.crp` parts whose data starts with type 2 (retail's article-bundle "flare"
//! entry, exe 0x450D10) and names a `Render/flare.dat` record. Prints, per track, how many there
//! are, which records they use, and a few raw entries, so the layout can be checked.
//!
//! `cargo run -p nfspu-game --example flare_parts -j 2 -- [track ...]`

use nfspu_formats::{crp, refpack};

fn main() {
    let dir = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").expect("NFSPU_GAME_DIR")).join("GameData");
    let flare = std::fs::read(dir.join("Render").join("flare.dat")).expect("flare.dat");
    // Record names as the engine compares them: the first dword of each 140-byte record.
    let names: Vec<u32> = flare.chunks_exact(140).map(|r| u32::from_le_bytes([r[0], r[1], r[2], r[3]])).collect();
    let text = |v: u32| v.to_be_bytes().iter().map(|&b| if b.is_ascii_graphic() { b as char } else { '.' }).collect::<String>();
    let mut tracks: Vec<String> = std::env::args().skip(1).collect();
    if tracks.is_empty() {
        tracks = ["alps", "autobahn", "canyon", "castle", "coastal", "farmland", "foothills", "forest", "industrial", "monaco1", "monaco3", "skidpad"].iter().map(|s| s.to_string()).collect();
    }
    for t in tracks {
        let Ok(raw) = std::fs::read(dir.join("Track").join(format!("{t}.crp"))) else { println!("{t}: no crp"); continue };
        let data = if refpack::is_refpack(&raw) { refpack::decompress(&raw).expect("refpack") } else { raw };
        let c = crp::parse(&data).expect("crp");
        let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
        let mut tags: std::collections::BTreeMap<String, usize> = Default::default();
        let mut shown = 0;
        let parts = c.articles.iter().enumerate().flat_map(|(i, a)| a.parts.iter().map(move |p| (Some(i), p))).chain(c.misc.iter().map(|p| (None, p)));
        for (article, p) in parts {
            let d = c.part_data(p);
            if d.len() < 40 || u32::from_le_bytes([d[0], d[1], d[2], d[3]]) != 2 {
                continue;
            }
            // type, pad, then x y z w, three floats, tag, phase (guessed from 0x451940's reads)
            let w = |k: usize| u32::from_le_bytes([d[4 * k], d[4 * k + 1], d[4 * k + 2], d[4 * k + 3]]);
            let f = |k: usize| f32::from_bits(w(k));
            let tag = w(9);
            if !names.contains(&tag) {
                continue;
            }
            *counts.entry(text(tag)).or_default() += 1;
            *tags.entry(String::from_utf8_lossy(&p.tag).to_string()).or_default() += 1;
            if shown < 4 {
                shown += 1;
                let name = article.and_then(|a| c.article_name(a)).unwrap_or_default();
                let tr = article.and_then(|a| c.article_transform(a, 0));
                println!(
                    "  {t} part '{}' article {:?} '{name}' len {}: pos ({:.2}, {:.2}, {:.2}, {:.2}) v ({:.3}, {:.3}, {:.3}) tag {} phase {} | article at {:?}",
                    String::from_utf8_lossy(&p.tag),
                    article,
                    d.len(),
                    f(2), f(3), f(4), f(5), f(6), f(7), f(8),
                    text(tag),
                    w(10) as i32,
                    tr.map(|m| [m[12], m[13], m[14]])
                );
            }
        }
        println!("{t}: {} glows {:?} in parts {:?}", counts.values().sum::<usize>(), counts, tags);
        // Whole-file scan: any flare tag with the type-2 dword 36 bytes before it.
        let mut raw_hits = 0;
        for o in (36..data.len().saturating_sub(4)).step_by(4) {
            let v = u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
            if names.contains(&v) && u32::from_le_bytes([data[o - 36], data[o - 35], data[o - 34], data[o - 33]]) == 2 {
                raw_hits += 1;
            }
        }
        println!("{t}: {raw_hits} raw type-2 entries naming a flare record");
    }
}
