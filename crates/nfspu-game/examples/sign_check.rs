//! Checks that placed track objects (articles with a transform away from the origin) land next to
//! the road: horizontal and vertical distance to the nearest road vertex.
//! Run: `cargo run -p nfspu-game --example sign_check -- alps`

use nfspu_formats::{crp, refpack};

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "alps".to_string());
    let dir = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").expect("NFSPU_GAME_DIR")).join("GameData").join("Track");
    let raw = std::fs::read(dir.join(format!("{name}.crp"))).expect("read track");
    let data = if refpack::is_refpack(&raw) { refpack::decompress(&raw).expect("refpack") } else { raw };
    let c = crp::parse(&data).expect("parse");
    let mut road: Vec<[f32; 3]> = Vec::new();
    let mut pieces: std::collections::BTreeMap<u32, [f32; 3]> = Default::default();
    for a in 0..c.articles.len() {
        if c.article_name(a).is_some_and(|n| n.starts_with("RD")) {
            if let Some(m) = c.article_mesh(a, 0) {
                road.extend(m.positions.iter().copied());
                let n = c.article_name(a).unwrap_or_default();
                if let Ok(k) = n[2..].chars().take_while(|ch| ch.is_ascii_digit()).collect::<String>().parse::<u32>() {
                    let s = m.positions.iter().fold([0.0f32; 3], |s, p| [s[0] + p[0], s[1] + p[1], s[2] + p[2]]);
                    let l = m.positions.len().max(1) as f32;
                    pieces.insert(k, [s[0] / l, s[1] / l, s[2] / l]);
                }
            }
        }
    }
    let (mut placed, mut far) = (0, 0);
    let (mut align_rows, mut align_cols, mut counted) = (0.0f32, 0.0f32, 0);
    let (mut facing_driver, mut facing_away) = (0, 0);
    let (mut readable_rows, mut readable_cols, mut counted_read) = (0, 0, 0);
    let (mut wind_agree, mut wind_disagree) = (0, 0);
    let mut worst: Vec<(f32, String)> = Vec::new();
    for a in 0..c.articles.len() {
        let Some(t) = c.article_transform(a, 0) else { continue };
        if (t[12] * t[12] + t[14] * t[14]).sqrt() < 50.0 {
            continue;
        }
        let n = c.article_name(a).unwrap_or_default();
        let (mut best, mut dy) = (f32::MAX, 0.0);
        for r in &road {
            let d = ((r[0] - t[12]).powi(2) + (r[2] - t[14]).powi(2)).sqrt();
            if d < best {
                best = d;
                dy = t[13] - r[1];
            }
        }
        // which way does the sign face? its artwork lies in the local x-y plane, so the facing is
        // local +Z; compare both readings of the rotation with the road direction nearby (PCA)
        let near: Vec<&[f32; 3]> = road.iter().filter(|r| ((r[0] - t[12]).powi(2) + (r[2] - t[14]).powi(2)).sqrt() < 20.0).collect();
        if near.len() > 3 {
            let (mx, mz) = (near.iter().map(|r| r[0]).sum::<f32>() / near.len() as f32, near.iter().map(|r| r[2]).sum::<f32>() / near.len() as f32);
            let (mut sxx, mut sxz, mut szz) = (0.0, 0.0, 0.0);
            for r in &near {
                let (dx, dz) = (r[0] - mx, r[2] - mz);
                sxx += dx * dx;
                sxz += dx * dz;
                szz += dz * dz;
            }
            let ang = 0.5 * (2.0 * sxz).atan2(sxx - szz);
            let road_dir = [ang.cos(), ang.sin()];
            let rows = [t[8], t[10]]; // p * M: local +Z maps to row 2
            let cols = [t[2], t[10]]; // transposed: local +Z maps to column 2
            let along = |v: [f32; 2]| (v[0] * road_dir[0] + v[1] * road_dir[1]).abs() / (v[0].hypot(v[1])).max(1e-6);
            align_rows += along(rows);
            align_cols += along(cols);
            counted += 1;
        }
        // front of the sign by winding (D3D clockwise front: outward = -((b-a) x (c-a))), in world
        // space via p * M, against the driving direction (road pieces in increasing number)
        if let Some(m) = c.article_mesh(a, 0) {
            let w = |p: [f32; 3]| [p[0] * t[0] + p[1] * t[4] + p[2] * t[8], p[0] * t[1] + p[1] * t[5] + p[2] * t[9], p[0] * t[2] + p[1] * t[6] + p[2] * t[10]];
            let b0 = &m.batches[0];
            let (pa, pb, pc) = (w(m.positions[b0.vertex_idx[0] as usize]), w(m.positions[b0.vertex_idx[1] as usize]), w(m.positions[b0.vertex_idx[2] as usize]));
            let (u, v) = ([pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]], [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]]);
            let front = [-(u[1] * v[2] - u[2] * v[1]), -(u[2] * v[0] - u[0] * v[2]), -(u[0] * v[1] - u[1] * v[0])];
            let k = pieces.iter().min_by(|x, y| {
                let dx = (x.1[0] - t[12]).hypot(x.1[2] - t[14]);
                let dy = (y.1[0] - t[12]).hypot(y.1[2] - t[14]);
                dx.total_cmp(&dy)
            }).map(|(k, _)| *k).unwrap_or(0);
            if let (Some(prev), Some(next)) = (pieces.range(..k).next_back(), pieces.range(k + 1..).next()) {
                let travel = [next.1[0] - prev.1[0], next.1[2] - prev.1[2]];
                let d = front[0] * travel[0] + front[2] * travel[1];
                if d < 0.0 { facing_driver += 1 } else { facing_away += 1 }
            }
        }
        // which side does the artwork read correctly from? In the mirrored (right-handed) world the
        // readable side's normal is U x Up, where U is the direction u grows along the sign.
        if let Some(m) = c.article_mesh(a, 0) {
            let k = pieces.iter().min_by(|x, y| (x.1[0] - t[12]).hypot(x.1[2] - t[14]).total_cmp(&(y.1[0] - t[12]).hypot(y.1[2] - t[14]))).map(|(k, _)| *k).unwrap_or(0);
            let travel = match (pieces.range(..k).next_back(), pieces.range(k + 1..).next()) {
                (Some(prev), Some(next)) => [-(next.1[0] - prev.1[0]), next.1[2] - prev.1[2]], // mirrored x
                _ => [0.0, 0.0],
            };
            for (conv, counter) in [(0, &mut readable_rows), (1, &mut readable_cols)] {
                let w = |p: [f32; 3]| {
                    let q = if conv == 0 {
                        [p[0] * t[0] + p[1] * t[4] + p[2] * t[8], p[0] * t[1] + p[1] * t[5] + p[2] * t[9], p[0] * t[2] + p[1] * t[6] + p[2] * t[10]]
                    } else {
                        [p[0] * t[0] + p[1] * t[1] + p[2] * t[2], p[0] * t[4] + p[1] * t[5] + p[2] * t[6], p[0] * t[8] + p[1] * t[9] + p[2] * t[10]]
                    };
                    [-q[0], q[1], q[2]] // mirrored, see MIRROR_X
                };
                let mut vote = 0.0f32;
                for b in &m.batches {
                    for tri in 0..b.vertex_idx.len() / 3 {
                        let pts: Vec<[f32; 3]> = (0..3).map(|q| w(m.positions[b.vertex_idx[3 * tri + q] as usize])).collect();
                        let uvs: Vec<[f32; 2]> = (0..3).map(|q| m.uvs[b.uv_idx[3 * tri + q] as usize]).collect();
                        let e1 = [pts[1][0] - pts[0][0], pts[1][1] - pts[0][1], pts[1][2] - pts[0][2]];
                        let e2 = [pts[2][0] - pts[0][0], pts[2][1] - pts[0][1], pts[2][2] - pts[0][2]];
                        let (du1, dv1, du2, dv2) = (uvs[1][0] - uvs[0][0], uvs[1][1] - uvs[0][1], uvs[2][0] - uvs[0][0], uvs[2][1] - uvs[0][1]);
                        let det = du1 * dv2 - du2 * dv1;
                        if det.abs() < 1e-6 {
                            continue;
                        }
                        // direction in which u grows on this triangle
                        let uu = [(e1[0] * dv2 - e2[0] * dv1) / det, (e1[1] * dv2 - e2[1] * dv1) / det, (e1[2] * dv2 - e2[2] * dv1) / det];
                        if uu[1].abs() > 0.5 * uu[0].hypot(uu[2]) {
                            continue; // u runs up the post, not across the artwork
                        }
                        // readable normal = U x Up = (-uu.z, 0, uu.x) (x and z components)
                        let n = [-uu[2], uu[0]];
                        if conv == 0 {
                            // the file's front face in the mirrored world: the mirror flips the winding, so
                            // the outward normal is +((b-a) x (c-a)) here
                            let cr = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
                            if cr[0] * n[0] + cr[2] * n[1] > 0.0 { wind_agree += 1 } else { wind_disagree += 1 }
                        }
                        let d = n[0] * travel[0] + n[1] * travel[1];
                        vote += if d < 0.0 { 1.0 } else { -1.0 };
                    }
                }
                if vote > 0.0 { *counter += 1 }
                if conv == 0 {
                    // side of the road for a driver going in increasing piece order (mirrored world: x negated)
                    let pc = pieces.get(&k).copied().unwrap_or([0.0; 3]);
                    let off = [-(t[12] - pc[0]), t[14] - pc[2]];
                    // right of travel direction (dx, dz) in a right-handed, +Y up world is (-dz, dx)
                    let right = [-travel[1], travel[0]];
                    let side = if off[0] * right[0] + off[1] * right[1] > 0.0 { "right" } else { "left" };
                    println!("SIGN {} {} {}", if vote > 0.0 { "fwd" } else { "rev" }, side, c.article_name(a).unwrap_or_default());
                }
            }
            counted_read += 1;
        }
        placed += 1;
        if best > 30.0 {
            far += 1;
        }
        worst.push((best, format!("{n:<36} at ({:.0},{:.0},{:.0}) nearest road vertex {best:.1} m away, height above it {dy:+.1} m", t[12], t[13], t[14])));
    }
    println!("signs facing along the road (1 = exactly, 0 = sideways), mean over {counted}: rows (p*M) {:.2}, transposed {:.2}", align_rows / counted as f32, align_cols / counted as f32);
    println!("front side (by winding) faces the oncoming driver: {facing_driver}, faces away: {facing_away}");
    println!("artwork readable by the oncoming driver: rows (p*M) {readable_rows} of {counted_read}, transposed {readable_cols} of {counted_read}");
    println!("sign triangles whose front face (by winding) is the readable side: {wind_agree}, the back: {wind_disagree}");
    worst.sort_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((k, pc)) = pieces.iter().min_by(|x, y| (x.1[0] + 131.0).hypot(x.1[2] - 464.0).total_cmp(&(y.1[0] + 131.0).hypot(y.1[2] - 464.0))) {
        let next = pieces.range(k + 1..).next().map(|(n, q)| (*n, *q));
        println!("START piece {k} at ({:.0},{:.0}); next piece {:?}: increasing order heads {}", pc[0], pc[2], next.map(|n| n.0), next.map_or("?", |n| if n.1[2] < pc[2] { "-Z (the way the car starts)" } else { "+Z (opposite to the car)" }));
    }
    println!("PIECES {} numbered road pieces, numbers {:?}..{:?}", pieces.len(), pieces.keys().next(), pieces.keys().last());
    for (k, q) in pieces.iter().filter(|(_, q)| q[2] > 200.0) { println!("PIECE {k} at ({:.0},{:.1},{:.0})", q[0], q[1], q[2]); }
    println!("{placed} placed objects, {far} more than 30 m from the road");
    for (_, l) in worst.iter().take(3) { println!("  closest: {l}"); }
    for (_, l) in worst.iter().rev().take(3) { println!("  farthest: {l}"); }
    let mid = worst.len() / 2;
    if let Some((_, l)) = worst.get(mid) { println!("  median: {l}"); }
}
