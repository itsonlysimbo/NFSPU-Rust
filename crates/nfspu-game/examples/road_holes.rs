//! Offline probe for road slices with no loaded ground within 3 m of them: lists which articles
//! (any name, any level of detail) have a face straight above or below each such slice within
//! 6 m, and how that face is classified, so missing road surfaces can be traced to their names.
//! Run: `cargo run -p nfspu-game --example road_holes --release -- [track]`

use nfspu_formats::{crp, refpack};
use nfspu_game::ground::{Ground, Tri};
use nfspu_game::race_path::{self, Direction};
use nfspu_game::track::{self, classify};
use nfspu_game::MIRROR_X;
use std::collections::BTreeMap;

fn height_on(t: &Tri, x: f32, z: f32) -> Option<f32> {
    let (ax, az) = (t[0][0], t[0][2]);
    let (bx, bz) = (t[1][0], t[1][2]);
    let (cx, cz) = (t[2][0], t[2][2]);
    let det = (bz - cz) * (ax - cx) + (cx - bx) * (az - cz);
    if det.abs() < 1e-9 {
        return None;
    }
    let l1 = ((bz - cz) * (x - cx) + (cx - bx) * (z - cz)) / det;
    let l2 = ((cz - az) * (x - cx) + (ax - cx) * (z - cz)) / det;
    let l3 = 1.0 - l1 - l2;
    (l1 >= -1e-5 && l2 >= -1e-5 && l3 >= -1e-5).then(|| l1 * t[0][1] + l2 * t[1][1] + l3 * t[2][1])
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = track::track_dir().expect("set NFSPU_GAME_DIR");
    let names: Vec<String> = if args.is_empty() {
        ["autobahn", "industrial", "monaco1", "monaco2", "monaco3", "monaco4"].iter().map(|s| s.to_string()).collect()
    } else {
        args
    };
    for n in &names {
        let g = track::load_ground(n).expect("ground");
        let t = race_path::load(n, Direction::Forward).expect("race");
        let ground = Ground::new(g.tris, 16.0);
        let bad: Vec<usize> = (0..t.path.slices.len())
            .filter(|&i| {
                let s = t.path.slices[i].pos;
                ground.height_at(s.x, s.z, s.y + 2.0, 0.0).map_or(true, |h| (h - s.y).abs() > 3.0)
            })
            .collect();
        let raw = std::fs::read(dir.join(format!("{n}.crp"))).expect("read");
        let data = if refpack::is_refpack(&raw) { refpack::decompress(&raw).expect("refpack") } else { raw };
        let c = crp::parse(&data).expect("parse");
        // every face of every article at every level of detail, mirrored like the loader does
        let mut all: Vec<(String, u8, Tri, track::Face, bool)> = Vec::new();
        for a in 0..c.articles.len() {
            let name = c.article_name(a).unwrap_or_default();
            for lod in 0..4u8 {
                let Some(m) = c.article_mesh(a, lod) else { continue };
                let has_tr = c.article_transform(a, lod).is_some();
                for b in &m.batches {
                    for k in 0..b.vertex_idx.len() / 3 {
                        let tri: Tri = [0, 1, 2].map(|q| m.positions[b.vertex_idx[3 * k + q] as usize]);
                        let f = classify(&tri);
                        all.push((name.clone(), lod, tri.map(|v| [v[0] * MIRROR_X, v[1], v[2]]), f, has_tr));
                    }
                }
            }
        }
        println!("== {n}: {} bad slices of {}", bad.len(), t.path.slices.len());
        let mut summary: BTreeMap<String, usize> = BTreeMap::new();
        for (j, &i) in bad.iter().enumerate() {
            let s = t.path.slices[i].pos;
            let mut hits: Vec<String> = Vec::new();
            for (name, lod, tri, f, has_tr) in &all {
                if let Some(y) = height_on(tri, s.x, s.z) {
                    if (y - s.y).abs() < 6.0 {
                        hits.push(format!("{name} L{lod} {f:?} dy={:.1}{}", y - s.y, if *has_tr { " TR" } else { "" }));
                        let key = format!("{} L{lod} {f:?}", name.split(' ').next().unwrap_or("").trim_end_matches(|c: char| c.is_ascii_digit() || c == 'C' || c == 'L' || c == 'R'));
                        *summary.entry(key).or_default() += 1;
                    }
                }
            }
            if j < 12 || hits.is_empty() {
                println!("  slice {i} at ({:.1},{:.1},{:.1}): {}", s.x, s.y, s.z, if hits.is_empty() { "NOTHING".into() } else { hits.join(" | ") });
            }
        }
        println!("  summary: {summary:?}");
    }
}
