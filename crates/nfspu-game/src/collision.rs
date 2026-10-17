//! Collision with steep track faces (embankments, tunnel walls, buildings).
//!
//! The drivable ground holds only up-facing triangles, so near-vertical faces are kept
//! separately here in a uniform XZ grid. A car is two horizontal circles (front and rear
//! axle); each circle is pushed out of any overlapping wall. Pure Rust, no engine types.
use crate::ground::Tri;
use std::collections::HashMap;

pub struct WallSet {
    cell: f32,
    tris: Vec<Tri>,
    grid: HashMap<(i32, i32), Vec<u32>>,
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Closest point on the triangle to p (Ericson 5.1.5, vertex-region checks first).
fn closest_point(t: &Tri, p: [f32; 3]) -> [f32; 3] {
    let (a, b, c) = (t[0], t[1], t[2]);
    let (ab, ac, ap) = (sub(b, a), sub(c, a), sub(p, a));
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return [a[0] + v * ab[0], a[1] + v * ab[1], a[2] + v * ab[2]];
    }
    let cp = sub(p, c);
    let (d5, d6) = (dot(ab, cp), dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return [a[0] + w * ac[0], a[1] + w * ac[1], a[2] + w * ac[2]];
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        let bc = sub(c, b);
        return [b[0] + w * bc[0], b[1] + w * bc[1], b[2] + w * bc[2]];
    }
    let denom = 1.0 / (va + vb + vc).max(1e-9);
    let (v, w) = (vb * denom, vc * denom);
    [a[0] + ab[0] * v + ac[0] * w, a[1] + ab[1] * v + ac[1] * w, a[2] + ab[2] * v + ac[2] * w]
}

impl WallSet {
    pub fn new(tris: Vec<Tri>, cell: f32) -> Self {
        let mut grid: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (i, t) in tris.iter().enumerate() {
            let (mut x0, mut x1, mut z0, mut z1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
            for v in t {
                x0 = x0.min(v[0]);
                x1 = x1.max(v[0]);
                z0 = z0.min(v[2]);
                z1 = z1.max(v[2]);
            }
            for cx in (x0 / cell).floor() as i32..=(x1 / cell).floor() as i32 {
                for cz in (z0 / cell).floor() as i32..=(z1 / cell).floor() as i32 {
                    grid.entry((cx, cz)).or_default().push(i as u32);
                }
            }
        }
        WallSet { cell, tris, grid }
    }

    pub fn triangle_count(&self) -> usize {
        self.tris.len()
    }

    /// Push-out in XZ that moves the circle at (x, z) out of any overlapping wall, or
    /// `None` when clear. Only triangles whose height range meets the body's
    /// `[y_min, y_max]` count, so bridges and high signs are ignored.
    pub fn resolve(&self, x: f32, z: f32, y_min: f32, y_max: f32, radius: f32) -> Option<[f32; 2]> {
        let (cx0, cx1) = (
            ((x - radius) / self.cell).floor() as i32,
            ((x + radius) / self.cell).floor() as i32,
        );
        let (cz0, cz1) = (
            ((z - radius) / self.cell).floor() as i32,
            ((z + radius) / self.cell).floor() as i32,
        );
        let cy = (y_min + y_max) * 0.5;
        let mut best: Option<([f32; 2], f32)> = None;
        for cx in cx0..=cx1 {
            for cz in cz0..=cz1 {
                let Some(list) = self.grid.get(&(cx, cz)) else { continue };
                for &i in list {
                    let t = &self.tris[i as usize];
                    let (top, bottom) = (
                        t[0][1].max(t[1][1]).max(t[2][1]),
                        t[0][1].min(t[1][1]).min(t[2][1]),
                    );
                    if top < y_min || bottom > y_max {
                        continue;
                    }
                    let q = closest_point(t, [x, cy, z]);
                    let (dx, dz) = (x - q[0], z - q[2]);
                    let dist = (dx * dx + dz * dz).sqrt();
                    if dist >= radius {
                        continue;
                    }
                    let depth = radius - dist;
                    let push = if dist > 1e-6 {
                        [dx / dist * depth, dz / dist * depth]
                    } else {
                        // centre straight above the closest point: shove along +X; rare, keeps us unstuck
                        [depth, 0.0]
                    };
                    if best.map_or(true, |(_, d)| depth > d) {
                        best = Some((push, depth));
                    }
                }
            }
        }
        best.map(|(push, _)| push)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wall in the plane x = 0, spanning y 0..3, z -5..5.
    fn wall() -> WallSet {
        WallSet::new(
            vec![
                [[0.0, 0.0, -5.0], [0.0, 0.0, 5.0], [0.0, 3.0, -5.0]],
                [[0.0, 0.0, 5.0], [0.0, 3.0, 5.0], [0.0, 3.0, -5.0]],
            ],
            4.0,
        )
    }

    #[test]
    fn pushes_a_circle_out_of_a_wall() {
        let w = wall();
        let push = w.resolve(0.5, 1.0, 0.0, 1.5, 1.0).expect("should hit");
        assert!((push[0] - 0.5).abs() < 1e-3, "push {push:?}");
        assert!(push[1].abs() < 1e-3, "push {push:?}");
        assert!(w.resolve(3.0, 1.0, 0.0, 1.5, 1.0).is_none());
    }

    #[test]
    fn ignores_geometry_above_or_below_the_body() {
        let w = wall(); // wall spans y 0..3
        assert!(w.resolve(0.5, 1.0, 4.0, 5.5, 1.0).is_none()); // car on a bridge above
        assert!(w.resolve(0.5, 1.0, -3.0, -0.5, 1.0).is_none()); // car in a dip below
    }

    #[test]
    fn empty_set_never_hits() {
        let w = WallSet::new(vec![], 4.0);
        assert!(w.resolve(0.0, 0.0, 0.0, 1.0, 1.0).is_none());
    }
}
