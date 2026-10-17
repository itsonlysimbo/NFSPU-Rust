//! Offline ground audit. Finds enclosed holes in the drivable surface (cells surrounded by
//! ground triangles), reports their size and what geometry the file has near them, and prints
//! the slope distribution of up-facing faces so a drive limit can be chosen.
//! Run: `cargo run -p nfspu-game --example ground_audit -j 2 -- [track]`
use nfspu_formats::{crp, refpack};
use nfspu_game::track;
use std::collections::VecDeque;

type Tri = [[f32; 3]; 3];

fn normal(t: &Tri) -> Option<[f32; 3]> {
    let u = [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]];
    let v = [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]];
    let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    (len > 1e-6).then(|| [n[0] / len, n[1] / len, n[2] / len])
}

/// Height of the triangle's plane at (x, z) if the point lies inside it (as seen from above).
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

/// Cells per metre of the coverage raster.
fn audit(name: &str, cell: f32) {
    let g = match track::load_ground(name) {
        Ok(g) => g,
        Err(e) => {
            println!("{name}: ERROR {e}");
            return;
        }
    };
    let dir = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").expect("NFSPU_GAME_DIR"))
        .join("GameData")
        .join("Track");
    let raw = std::fs::read(dir.join(format!("{name}.crp"))).expect("read track");
    let data = if refpack::is_refpack(&raw) { refpack::decompress(&raw).expect("refpack") } else { raw };
    let c = crp::parse(&data).expect("parse");
    let mut all: Vec<(String, Tri, bool)> = Vec::new();
    let mut slopes: Vec<f32> = Vec::new();
    for a in 0..c.articles.len() {
        let Some(an) = c.article_name(a) else { continue };
        if !an.starts_with("RD") && !an.starts_with("CNK") {
            continue;
        }
        let Some(m) = c.article_mesh(a, 0) else { continue };
        for b in &m.batches {
            for t in 0..b.vertex_idx.len() / 3 {
                let tri: Tri = [0, 1, 2].map(|q| m.positions[b.vertex_idx[3 * t + q] as usize]);
                let Some(n) = normal(&tri) else { continue };
                let up = n[1] < -0.25; // the file's winding; up-facing has a downward normal
                let tri: Tri = tri.map(|v| [v[0] * -1.0, v[1], v[2]]);
                if up {
                    slopes.push(n[1].abs().clamp(0.0, 1.0).acos().to_degrees());
                }
                all.push((an.clone(), tri, up));
            }
        }
    }
    slopes.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let pct = |p: f32| slopes.get(((slopes.len() as f32 - 1.0) * p) as usize).copied().unwrap_or(0.0);
    let steep = slopes.iter().filter(|s| **s > 40.0).count();
    // Road faces that are nearly horizontal but wound the other way: the old classifier called
    // these "walls", so the car fell through them.
    let mut inverted: Vec<(String, [f32; 3])> = Vec::new();
    for (an, t, up) in &all {
        if *up || !an.starts_with("RD") {
            continue;
        }
        let Some(n) = normal(t) else { continue };
        if n[1] > 0.766 {
            let c = [(t[0][0] + t[1][0] + t[2][0]) / 3.0, (t[0][1] + t[1][1] + t[2][1]) / 3.0, (t[0][2] + t[1][2] + t[2][2]) / 3.0];
            inverted.push((an.clone(), c));
        }
    }
    println!(
        "{name}: {} ground tris, slope p50 {:.0} p90 {:.0} p99 {:.0} max {:.0} deg; {steep} over 40 deg ({:.0}%); {} inverted-winding road tris",
        slopes.len(), pct(0.5), pct(0.9), pct(0.99), slopes.last().copied().unwrap_or(0.0),
        100.0 * steep as f32 / slopes.len().max(1) as f32,
        inverted.len()
    );
    for (an, c) in inverted.iter().take(5) {
        println!("      inverted: {an:<44} at ({:.0}, {:.0}, {:.0})", c[0], c[1], c[2]);
    }

    // dense raster of the drivable surface
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for (_, t, _) in &all {
        for v in t {
            lo[0] = lo[0].min(v[0]);
            lo[1] = lo[1].min(v[2]);
            hi[0] = hi[0].max(v[0]);
            hi[1] = hi[1].max(v[2]);
        }
    }
    let (ox, oz) = (lo[0] - 2.0, lo[1] - 2.0);
    let (nx, nz) = (((hi[0] - ox) / cell).ceil() as usize + 4, ((hi[1] - oz) / cell).ceil() as usize + 4);
    println!("  raster {nx} x {nz} cells of {cell} m");
    let idx = |x: f32, z: f32| (((x - ox) / cell) as isize, ((z - oz) / cell) as isize);
    let mut cover = vec![f32::MIN; nx * nz];
    let at = |ix: isize, iz: isize| -> Option<usize> {
        (ix >= 0 && iz >= 0 && (ix as usize) < nx && (iz as usize) < nz).then(|| iz as usize * nx + ix as usize)
    };
    for (_, t, up) in &all {
        if !*up {
            continue;
        }
        let (mut x0, mut x1, mut z0, mut z1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for v in t {
            x0 = x0.min(v[0]);
            x1 = x1.max(v[0]);
            z0 = z0.min(v[2]);
            z1 = z1.max(v[2]);
        }
        let (i0, j0) = idx(x0, z0);
        let (i1, j1) = idx(x1, z1);
        for iz in j0..=j1 {
            for ix in i0..=i1 {
                let Some(k) = at(ix, iz) else { continue };
                let (px, pz) = (ox + (ix as f32 + 0.5) * cell, oz + (iz as f32 + 0.5) * cell);
                if let Some(y) = height_on(t, px, pz) {
                    cover[k] = cover[k].max(y);
                }
            }
        }
    }
    // flood fill from the border to mark the outside
    let mut outside = vec![false; nx * nz];
    let mut queue: VecDeque<(isize, isize)> = VecDeque::new();
    for ix in 0..nx as isize {
        for iz in [0isize, nz as isize - 1] {
            if let Some(k) = at(ix, iz) {
                if cover[k] == f32::MIN && !outside[k] {
                    outside[k] = true;
                    queue.push_back((ix, iz));
                }
            }
        }
    }
    for iz in 0..nz as isize {
        for ix in [0isize, nx as isize - 1] {
            if let Some(k) = at(ix, iz) {
                if cover[k] == f32::MIN && !outside[k] {
                    outside[k] = true;
                    queue.push_back((ix, iz));
                }
            }
        }
    }
    while let Some((ix, iz)) = queue.pop_front() {
        for (dx, dz) in [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)] {
            let (jx, jz) = (ix + dx, iz + dz);
            if let Some(k) = at(jx, jz) {
                if cover[k] == f32::MIN && !outside[k] {
                    outside[k] = true;
                    queue.push_back((jx, jz));
                }
            }
        }
    }
    // hole cells: empty, not outside, touching ground
    let mut cells: Vec<(isize, isize)> = Vec::new();
    for iz in 0..nz as isize {
        for ix in 0..nx as isize {
            let Some(k) = at(ix, iz) else { continue };
            if cover[k] != f32::MIN || outside[k] {
                continue;
            }
            let touches = [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .any(|(dx, dz)| at(ix + dx, iz + dz).is_some_and(|m| cover[m] != f32::MIN));
            if touches {
                cells.push((ix, iz));
            }
        }
    }
    // group by proximity, then keep only holes a road triangle sits next to
    let mut groups: Vec<Vec<(isize, isize)>> = Vec::new();
    'cell: for cell in cells {
        for grp in groups.iter_mut() {
            if grp.iter().any(|c| (c.0 - cell.0).abs() <= 2 && (c.1 - cell.1).abs() <= 2) {
                grp.push(cell);
                continue 'cell;
            }
        }
        groups.push(vec![cell]);
    }
    let road_near = |x: f32, z: f32| {
        all.iter().any(|(an, t, up)| {
            *up
                && an.starts_with("RD")
                && height_on(t, x, z).is_none()
                && {
                    let (tx, tz) = ((t[0][0] + t[1][0] + t[2][0]) / 3.0, (t[0][2] + t[1][2] + t[2][2]) / 3.0);
                    (tx - x).powi(2) + (tz - z).powi(2) < 64.0
                }
        })
    };
    let mut reported = 0;
    groups.sort_by_key(|g| std::cmp::Reverse(g.len()));
    let total: usize = groups.iter().map(|g| g.len()).sum();
    println!("  {} hole groups, {total} hole cells", groups.len());
    for grp in &groups {
        let (cx, cz) = (
            ox + (grp.iter().map(|c| c.0 as f32).sum::<f32>() / grp.len() as f32 + 0.5) * cell,
            oz + (grp.iter().map(|c| c.1 as f32).sum::<f32>() / grp.len() as f32 + 0.5) * cell,
        );
        if !road_near(cx, cz) {
            continue;
        }
        reported += 1;
        if reported > 6 {
            continue;
        }
        println!("  hole {} cells at ({cx:.1}, {cz:.1}); nearest tris:", grp.len());
        let mut near: Vec<(f32, String, bool, f32)> = Vec::new();
        for (an, t, up) in &all {
            let (tx, tz) = ((t[0][0] + t[1][0] + t[2][0]) / 3.0, (t[0][2] + t[1][2] + t[2][2]) / 3.0);
            let surface = height_on(t, cx, cz).unwrap_or((t[0][1] + t[1][1] + t[2][1]) / 3.0);
            near.push((((tx - cx).powi(2) + (tz - cz).powi(2)).sqrt(), an.clone(), *up, surface));
        }
        near.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        near.dedup_by(|a, b| a.1 == b.1);
        for (dist, an, up, y) in near.iter().take(3) {
            println!("      {dist:>6.1} m  {:<44} {} y {y:.1}", an, if *up { "up  " } else { "DOWN" });
        }
    }
    if reported > 6 {
        println!("  ... and {} more road-adjacent hole groups", reported - 6);
    }
}

fn main() {
    let only = std::env::args().nth(1);
    let tracks = [
        "alps", "autobahn", "canyon", "castle", "coastal", "farmland", "foothills", "forest",
        "industrial", "monaco1", "monaco2", "monaco3", "monaco4", "monaco5", "skidpad",
    ];
    for name in tracks {
        if let Some(o) = &only {
            if o != name {
                continue;
            }
        }
        audit(name, 1.0);
    }
}
