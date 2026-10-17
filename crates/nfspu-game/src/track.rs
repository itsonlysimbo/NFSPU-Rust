//! Extracts the drivable surface and the start position of a track from the player's install.
//!
//! Road pieces are articles named `RD####C (... PRIMARY)` and the terrain beside the road is made of
//! `CNK####L/R (...)` chunks (see `docs/provenance/formats.md`). Triangles that face up under the
//! file's clockwise (Direct3D) winding count as ground. Some road runs over articles with other
//! names (Monaco's casino square, industrial's street blocks, alps' side roads); those articles
//! are found from the road slices (`SimD`) that lie on them and are loaded like road pieces.

use crate::ground::Tri;
use crate::MIRROR_X;
use bevy::math::Vec3;
use nfspu_formats::{crp, refpack, sim};
use std::collections::HashMap;
use std::{env, fs, path::PathBuf};

pub struct GroundData {
    pub tris: Vec<Tri>,
    pub road_triangles: usize,
    pub terrain_triangles: usize,
    /// Articles with other names that carry the road (found from the road slices).
    pub road_carriers: usize,
    /// Near-vertical faces (embankments, tunnel sides, buildings): not drivable, but solid.
    pub walls: Vec<Tri>,
    pub start_pos: Vec3,
    pub start_yaw: f32,
    /// Centres of the main road pieces in driving order (used by offline tests and later by AI).
    pub centerline: Vec<Vec3>,
    /// How the start position was found (for the log).
    pub start_note: String,
}

pub fn track_dir() -> Result<PathBuf, String> {
    let dir = env::var("NFSPU_GAME_DIR").map_err(|_| "NFSPU_GAME_DIR is not set".to_string())?;
    Ok(PathBuf::from(dir).join("GameData").join("Track"))
}

/// Steepest surface the car may drive on. Up-facing faces steeper than this are solid walls
/// instead, so cliffsides cannot be climbed or driven along.
pub const DRIVE_SLOPE_COS: f32 = 0.766; // 40 degrees

/// How a track face behaves: drivable ground, a solid wall, a nearly flat face that faces down,
/// or ignored (degenerate).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    Ground,
    Wall,
    /// Faces down and is nearly flat: a ceiling or underside, or a road face stored with the
    /// opposite winding (autobahn has some). Never a wall: a flat face cannot block sideways.
    Flipped,
    Skip,
}

/// Classifies a face from the file's clockwise (Direct3D) winding. Up-facing faces have a
/// downward normal; steep up-facing faces (over `DRIVE_SLOPE_COS`) are walls, as are faces
/// that lean sideways. Nearly flat faces that face down are `Flipped`.
pub fn classify(t: &Tri) -> Face {
    let u = [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]];
    let v = [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]];
    let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len <= 1e-6 {
        return Face::Skip;
    }
    if n[1] < -DRIVE_SLOPE_COS * len {
        Face::Ground
    } else if n[1] > DRIVE_SLOPE_COS * len {
        Face::Flipped
    } else {
        // sideways or steep downward faces are walls, and so are up-facing faces steeper than the limit
        Face::Wall
    }
}

/// "RD0008C (1 PRIMARY)" -> Some(8)
fn road_piece_number(name: &str) -> Option<u32> {
    let rest = name.strip_prefix("RD")?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !rest[digits.len()..].starts_with('C') || !name.contains("(1 PRIMARY)") {
        return None;
    }
    digits.parse().ok()
}

/// How far (metres) above or below a road slice point a drivable face may be and still be taken
/// as the road there. Slice heights differ from the road mesh by up to about 7 m in places
/// (autobahn ramps, industrial), so the closest face within this band wins.
const ROAD_SEARCH_DY: f32 = 8.0;
/// Fraction of each half-width used for the probe points either side of a slice centre.
const EDGE_PROBE: f32 = 0.5;

/// Articles not named as road or terrain that carry the road: for every road slice point (centre
/// and half way to each edge), the drivable face closest in height is found among all articles;
/// the articles owning those faces are the road surface there. Works in file space.
fn road_carriers(sim: &sim::SimData, meshes: &[Option<(String, crp::ArticleMesh)>]) -> Vec<bool> {
    // drivable faces of every article, as (article, triangle), on a grid in the XZ plane
    let mut faces: Vec<(usize, Tri)> = Vec::new();
    for (a, m) in meshes.iter().enumerate() {
        let Some((_, mesh)) = m else { continue };
        for b in &mesh.batches {
            for t in 0..b.vertex_idx.len() / 3 {
                let tri: Tri = [0, 1, 2].map(|q| mesh.positions[b.vertex_idx[3 * t + q] as usize]);
                if classify(&tri) == Face::Ground {
                    faces.push((a, tri));
                }
            }
        }
    }
    let cell = 16.0f32;
    let mut grid: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
    for (i, (_, t)) in faces.iter().enumerate() {
        let (x0, x1) = (t[0][0].min(t[1][0]).min(t[2][0]), t[0][0].max(t[1][0]).max(t[2][0]));
        let (z0, z1) = (t[0][2].min(t[1][2]).min(t[2][2]), t[0][2].max(t[1][2]).max(t[2][2]));
        for cx in (x0 / cell).floor() as i32..=(x1 / cell).floor() as i32 {
            for cz in (z0 / cell).floor() as i32..=(z1 / cell).floor() as i32 {
                grid.entry((cx, cz)).or_default().push(i as u32);
            }
        }
    }
    let mut carrier = vec![false; meshes.len()];
    for s in &sim.slices {
        let at = |k: f32| [s.pos[0] + s.right[0] * k, s.pos[1] + s.right[1] * k, s.pos[2] + s.right[2] * k];
        for p in [s.pos, at(-s.left_width * EDGE_PROBE), at(s.right_width * EDGE_PROBE)] {
            let Some(list) = grid.get(&((p[0] / cell).floor() as i32, (p[2] / cell).floor() as i32)) else { continue };
            let mut best: Option<(f32, usize)> = None;
            for &i in list {
                let (a, t) = &faces[i as usize];
                if let Some(y) = crate::ground::height_on_tri(t, p[0], p[2]) {
                    let d = (y - p[1]).abs();
                    if d <= ROAD_SEARCH_DY && best.map_or(true, |(bd, _)| d < bd) {
                        best = Some((d, *a));
                    }
                }
            }
            if let Some((_, a)) = best {
                carrier[a] = true;
            }
        }
    }
    // road and terrain articles are loaded anyway; only the others are reported as carriers
    for (a, m) in meshes.iter().enumerate() {
        if let Some((name, _)) = m {
            if name.starts_with("RD") || name.starts_with("CNK") {
                carrier[a] = false;
            }
        }
    }
    carrier
}

pub fn load_ground(track: &str) -> Result<GroundData, String> {
    let path = track_dir()?.join(format!("{track}.crp"));
    let raw = fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let data = refpack::decompress(&raw).map_err(|e| format!("{e}"))?;
    let crp = crp::parse(&data).map_err(|e| format!("{e}"))?;

    let mut tris: Vec<Tri> = Vec::new();
    let mut walls: Vec<Tri> = Vec::new();
    let (mut road, mut terrain) = (0usize, 0usize);
    // (piece number, centroid sum, vertex count) for the primary road pieces
    let mut pieces: Vec<(u32, [f32; 3], usize)> = Vec::new();

    let meshes: Vec<Option<(String, crp::ArticleMesh)>> =
        (0..crp.articles.len()).map(|a| Some((crp.article_name(a)?, crp.article_mesh(a, 0)?))).collect();
    // Road slices find the road surfaces in articles that are not named as road or terrain.
    let carrier = sim::read(&crp).map(|s| road_carriers(&s, &meshes)).unwrap_or_else(|_| vec![false; meshes.len()]);
    let carriers = carrier.iter().filter(|&&c| c).count();

    for (a, m) in meshes.iter().enumerate() {
        let Some((name, mesh)) = m else { continue };
        let is_road = name.starts_with("RD") || carrier[a];
        if !is_road && !name.starts_with("CNK") {
            continue;
        }
        let number = road_piece_number(name);
        let mut sum = [0.0f32; 3];
        let mut count = 0usize;
        for b in &mesh.batches {
            for t in 0..b.vertex_idx.len() / 3 {
                let tri: Tri = [0, 1, 2].map(|q| mesh.positions[b.vertex_idx[3 * t + q] as usize]);
                // the slope test uses the file's winding, so it comes before the mirror
                let face = classify(&tri);
                let tri: Tri = tri.map(|v| [v[0] * MIRROR_X, v[1], v[2]]);
                if number.is_some() {
                    for v in &tri {
                        for k in 0..3 {
                            sum[k] += v[k];
                        }
                        count += 1;
                    }
                }
                // a flat face in a road piece is road whatever its winding; elsewhere a flat
                // down-facing face is a ceiling or underside and is left out
                let face = if face == Face::Flipped && name.starts_with("RD") { Face::Ground } else { face };
                match face {
                    Face::Ground => {
                        tris.push(tri);
                        if is_road {
                            road += 1;
                        } else {
                            terrain += 1;
                        }
                    }
                    Face::Wall => walls.push(tri),
                    Face::Flipped | Face::Skip => {}
                }
            }
        }
        if let Some(n) = number {
            if count > 0 {
                pieces.push((n, sum, count));
            }
        }
    }
    if tris.is_empty() {
        return Err("no ground triangles found in this track".to_string());
    }

    pieces.sort_by_key(|p| p.0);
    let centroid = |p: &(u32, [f32; 3], usize)| Vec3::new(p.1[0] / p.2 as f32, p.1[1] / p.2 as f32, p.1[2] / p.2 as f32);
    let (start_pos, start_yaw, start_note) = if pieces.len() >= 2 {
        let c0 = centroid(&pieces[0]);
        let c1 = centroid(&pieces[1]);
        let (hx, hz) = (c1.x - c0.x, c1.z - c0.z);
        let len = (hx * hx + hz * hz).sqrt().max(1e-6);
        (
            Vec3::new(c0.x, c0.y + 2.0, c0.z),
            (-hx / len).atan2(-hz / len),
            format!("road piece {} heading toward piece {}", pieces[0].0, pieces[1].0),
        )
    } else {
        let (mut sx, mut sz, mut sy) = (0.0, 0.0, f32::MIN);
        for t in &tris {
            sx += t[0][0];
            sz += t[0][2];
            sy = sy.max(t[0][1]);
        }
        let n = tris.len() as f32;
        (Vec3::new(sx / n, sy + 2.0, sz / n), 0.0, "no named road pieces; using the middle of the ground".to_string())
    };

    let centerline: Vec<Vec3> = pieces.iter().map(|p| centroid(p)).collect();
    Ok(GroundData { tris, road_triangles: road, terrain_triangles: terrain, road_carriers: carriers, walls, start_pos, start_yaw, centerline, start_note })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn road_piece_numbers() {
        assert_eq!(road_piece_number("RD0008C (1 PRIMARY)"), Some(8));
        assert_eq!(road_piece_number("RD1136C (1 PRIMARY)"), Some(1136));
        assert_eq!(road_piece_number("RD0008C (28 PRIMARY)"), None); // surface variant, not the main piece
        assert_eq!(road_piece_number("CNK0000L (1 PRIMARY)"), None);
        assert_eq!(road_piece_number("RDX"), None);
    }

    #[test]
    fn gentle_faces_are_ground_and_steep_ones_are_walls() {
        // Measured on the real road triangles: up-facing ones have (b - a) x (c - a) pointing down (-Y).
        let flat: Tri = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        assert_eq!(classify(&flat), Face::Ground);
        let down: Tri = [flat[0], flat[2], flat[1]];
        assert_eq!(classify(&down), Face::Flipped);
        // a 20 degree slope is drivable: rises 0.36 m over 1 m
        let gentle: Tri = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.36, 1.0]];
        assert_eq!(classify(&gentle), Face::Ground);
        // a 60 degree up-facing slope is a wall: rises 1.73 m over 1 m
        let steep: Tri = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.73, 1.0]];
        assert_eq!(classify(&steep), Face::Wall);
        // a vertical wall face is a wall
        let wall: Tri = [[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]];
        assert_eq!(classify(&wall), Face::Wall);
        // a degenerate triangle is ignored
        let degenerate: Tri = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
        assert_eq!(classify(&degenerate), Face::Skip);
        // swaying a slope the wrong way makes it face down: still a wall
        let overhang: Tri = [steep[0], steep[2], steep[1]];
        assert_eq!(classify(&overhang), Face::Wall);
    }
}
