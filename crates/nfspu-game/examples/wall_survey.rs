//! Surveys steep (wall-like) triangles inside a track's road/terrain articles: counts,
//! vertical extent, and how much of the road centerline runs near one. Offline only.
//! Run: `cargo run -p nfspu-game --example wall_survey -j 2 -- [track]`
//! With no track argument, all 15 tracks are summarized.
use nfspu_formats::{crp, refpack};
use nfspu_game::ground::Ground;
use nfspu_game::track;
use std::collections::HashSet;

fn faces_up(t: &[[f32; 3]; 3]) -> bool {
    let u = [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]];
    let v = [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]];
    let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    len > 1e-6 && n[1] < -0.25 * len
}

fn survey(name: &str) {
    let dir = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").expect("NFSPU_GAME_DIR"))
        .join("GameData")
        .join("Track");
    let raw = std::fs::read(dir.join(format!("{name}.crp"))).expect("read track");
    let data = if refpack::is_refpack(&raw) { refpack::decompress(&raw).expect("refpack") } else { raw };
    let c = crp::parse(&data).expect("parse");
    let g = track::load_ground(name).expect("ground");
    let height = Ground::new(g.tris.clone(), 16.0);

    let mut up = 0usize;
    let mut steep_road = 0usize;
    let mut steep_terrain = 0usize;
    let mut wall_tris: Vec<[[f32; 3]; 3]> = Vec::new();
    for a in 0..c.articles.len() {
        let Some(an) = c.article_name(a) else { continue };
        let is_road = an.starts_with("RD");
        if !is_road && !an.starts_with("CNK") {
            continue;
        }
        let Some(m) = c.article_mesh(a, 0) else { continue };
        for b in &m.batches {
            for t in 0..b.vertex_idx.len() / 3 {
                // file winding for the up test, then mirror like the viewer
                let raw_tri: [[f32; 3]; 3] =
                    [0, 1, 2].map(|q| m.positions[b.vertex_idx[3 * t + q] as usize]);
                if faces_up(&raw_tri) {
                    up += 1;
                    continue;
                }
                if is_road {
                    steep_road += 1;
                } else {
                    steep_terrain += 1;
                }
                let tri: [[f32; 3]; 3] = raw_tri.map(|v| [v[0] * -1.0, v[1], v[2]]);
                let top = tri[0][1].max(tri[1][1]).max(tri[2][1]);
                let bottom = tri[0][1].min(tri[1][1]).min(tri[2][1]);
                let cx = (tri[0][0] + tri[1][0] + tri[2][0]) / 3.0;
                let cz = (tri[0][2] + tri[1][2] + tri[2][2]) / 3.0;
                // a wall stands on the ground: its base near a ground surface, with real height
                if top - bottom >= 0.4 {
                    if let Some(gy) = height.height_at(cx, cz, top, 2.0) {
                        if bottom - gy <= 1.5 {
                            wall_tris.push(tri);
                        }
                    }
                }
            }
        }
    }
    // coverage: fraction of centerline points within 12 m (XZ) of a wall triangle centroid
    let mut near = 0usize;
    let mut cells: HashSet<(i32, i32)> = HashSet::new();
    for t in &wall_tris {
        let cx = (t[0][0] + t[1][0] + t[2][0]) / 3.0;
        let cz = (t[0][2] + t[1][2] + t[2][2]) / 3.0;
        cells.insert((cx as i32 / 4, cz as i32 / 4));
    }
    let has_near = |x: f32, z: f32| {
        let (bx, bz) = (x as i32 / 4, z as i32 / 4);
        for dx in -3..=3 {
            for dz in -3..=3 {
                if cells.contains(&(bx + dx, bz + dz)) {
                    return true;
                }
            }
        }
        false
    };
    for p in &g.centerline {
        if has_near(p.x, p.z) {
            near += 1;
        }
    }
    let total = g.centerline.len().max(1);
    println!(
        "{name:<10} up {up:>6} steep-road {steep_road:>5} steep-terrain {steep_terrain:>6} wall-like {wall:>5} centerline-near-wall {near}/{total}",
        wall = wall_tris.len()
    );
}

fn main() {
    let only = std::env::args().nth(1);
    let tracks = [
        "alps", "autobahn", "canyon", "castle", "coastal", "farmland", "foothills", "forest",
        "industrial", "monaco1", "monaco2", "monaco3", "monaco4", "monaco5", "skidpad",
    ];
    for t in tracks {
        if let Some(ref o) = only {
            if o != t {
                continue;
            }
        }
        survey(t);
    }
}
