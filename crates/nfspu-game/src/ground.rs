//! Ground height queries over a soup of triangles, using a uniform grid in the XZ plane.
//! Pure Rust (no engine types) so it can be tested without opening a window.

use std::collections::HashMap;

pub type Tri = [[f32; 3]; 3];

pub struct Ground {
    cell: f32,
    tris: Vec<Tri>,
    grid: HashMap<(i32, i32), Vec<u32>>,
}

/// Height of the triangle's plane at (x, z), if that point lies inside it when seen from above.
fn height_on_tri(t: &Tri, x: f32, z: f32) -> Option<f32> {
    let (ax, az) = (t[0][0], t[0][2]);
    let (bx, bz) = (t[1][0], t[1][2]);
    let (cx, cz) = (t[2][0], t[2][2]);
    let det = (bz - cz) * (ax - cx) + (cx - bx) * (az - cz);
    if det.abs() < 1e-6 {
        return None; // edge-on or degenerate when seen from above
    }
    let l1 = ((bz - cz) * (x - cx) + (cx - bx) * (z - cz)) / det;
    let l2 = ((cz - az) * (x - cx) + (ax - cx) * (z - cz)) / det;
    let l3 = 1.0 - l1 - l2;
    let eps = -1e-4;
    if l1 < eps || l2 < eps || l3 < eps {
        return None;
    }
    Some(l1 * t[0][1] + l2 * t[1][1] + l3 * t[2][1])
}

/// Closest point of the segment `a -> b` to `p`, as a fraction along the segment.
fn segment_fraction(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dz * dz;
    if len2 < 1e-12 {
        return 0.0;
    }
    (((p[0] - a[0]) * dx + (p[1] - a[1]) * dz) / len2).clamp(0.0, 1.0)
}

/// Height of the triangle at (x, z): the plane height when the point is inside, otherwise the
/// height at the closest point of the triangle's outline. Also returns the XZ distance to the
/// triangle. Small gaps between track pieces are bridged this way for a rigid car.
fn nearest_height(t: &Tri, x: f32, z: f32) -> (f32, f32) {
    if let Some(y) = height_on_tri(t, x, z) {
        return (0.0, y);
    }
    let p = [x, z];
    let mut best = (f32::MAX, 0.0f32);
    for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
        let a = [t[i][0], t[i][2]];
        let b = [t[j][0], t[j][2]];
        let s = segment_fraction(p, a, b);
        let (hx, hz) = (a[0] + (b[0] - a[0]) * s, a[1] + (b[1] - a[1]) * s);
        let d = ((x - hx) * (x - hx) + (z - hz) * (z - hz)).sqrt();
        if d < best.0 {
            best = (d, t[i][1] + (t[j][1] - t[i][1]) * s);
        }
    }
    best
}

impl Ground {
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
        Ground { cell, tris, grid }
    }

    pub fn triangle_count(&self) -> usize {
        self.tris.len()
    }

    /// Height of the highest surface at (x, z) that is at or below `y_ref + step_up`. If every
    /// surface there is higher than that, returns the lowest one. `None` if nothing is under (x, z).
    pub fn height_at(&self, x: f32, z: f32, y_ref: f32, step_up: f32) -> Option<f32> {
        let key = ((x / self.cell).floor() as i32, (z / self.cell).floor() as i32);
        let list = self.grid.get(&key)?;
        let limit = y_ref + step_up;
        let (mut below, mut above): (Option<f32>, Option<f32>) = (None, None);
        for &i in list {
            if let Some(y) = height_on_tri(&self.tris[i as usize], x, z) {
                if y <= limit {
                    below = Some(below.map_or(y, |b: f32| b.max(y)));
                } else {
                    above = Some(above.map_or(y, |a: f32| a.min(y)));
                }
            }
        }
        below.or(above)
    }

    /// Height of the nearest surface to (x, z) within `radius` metres, restricted to the band
    /// `[y_min, y_max]`. A point that misses every triangle still finds support at the closest
    /// triangle edge, which is how a rigid car bridges the small gaps between track pieces.
    /// Surfaces straight above the point win over nearby edges; otherwise the closest wins.
    pub fn surface_near(&self, x: f32, z: f32, radius: f32, y_min: f32, y_max: f32) -> Option<f32> {
        let (cx0, cx1) = (((x - radius) / self.cell).floor() as i32, ((x + radius) / self.cell).floor() as i32);
        let (cz0, cz1) = (((z - radius) / self.cell).floor() as i32, ((z + radius) / self.cell).floor() as i32);
        let (mut inside, mut nearest): (Option<f32>, Option<(f32, f32)>) = (None, None);
        let mut seen: Vec<u32> = Vec::new();
        for cx in cx0..=cx1 {
            for cz in cz0..=cz1 {
                let Some(list) = self.grid.get(&(cx, cz)) else { continue };
                for &i in list {
                    if seen.contains(&i) {
                        continue;
                    }
                    seen.push(i);
                    let (dist, y) = nearest_height(&self.tris[i as usize], x, z);
                    if dist > radius || y < y_min || y > y_max {
                        continue;
                    }
                    if dist <= 0.0 {
                        inside = Some(inside.map_or(y, |h: f32| h.max(y)));
                    } else if nearest.map_or(true, |(d, h)| dist < d || (dist == d && y > h)) {
                        nearest = Some((dist, y));
                    }
                }
            }
        }
        inside.or(nearest.map(|(_, y)| y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(y: f32) -> Vec<Tri> {
        vec![
            [[0.0, y, 0.0], [100.0, y, 0.0], [0.0, y, 100.0]],
            [[100.0, y, 0.0], [100.0, y, 100.0], [0.0, y, 100.0]],
        ]
    }

    #[test]
    fn flat_ground_height() {
        let g = Ground::new(flat(2.0), 16.0);
        assert_eq!(g.height_at(10.0, 10.0, 5.0, 1.0), Some(2.0));
        assert_eq!(g.height_at(90.0, 90.0, 5.0, 1.0), Some(2.0));
    }

    #[test]
    fn outside_is_none() {
        let g = Ground::new(flat(2.0), 16.0);
        assert_eq!(g.height_at(-5.0, 10.0, 0.0, 1.0), None);
        assert_eq!(g.height_at(500.0, 500.0, 0.0, 1.0), None);
    }

    #[test]
    fn slope_is_interpolated() {
        // plane y = x * 0.1 over a 100 x 100 square
        let t = vec![
            [[0.0, 0.0, 0.0], [100.0, 10.0, 0.0], [0.0, 0.0, 100.0]],
            [[100.0, 10.0, 0.0], [100.0, 10.0, 100.0], [0.0, 0.0, 100.0]],
        ];
        let g = Ground::new(t, 16.0);
        let h = g.height_at(50.0, 30.0, 10.0, 1.0).unwrap();
        assert!((h - 5.0).abs() < 1e-3, "got {h}");
    }

    #[test]
    fn picks_the_surface_below_the_car_on_stacked_layers() {
        let mut t = flat(0.0);
        t.extend(flat(10.0)); // a bridge above the road
        let g = Ground::new(t, 16.0);
        assert_eq!(g.height_at(50.0, 50.0, 0.4, 1.5), Some(0.0)); // car on the road under the bridge
        assert_eq!(g.height_at(50.0, 50.0, 10.4, 1.5), Some(10.0)); // car on the bridge
    }

    #[test]
    fn triangle_spanning_many_cells_is_found_everywhere() {
        let g = Ground::new(flat(1.0), 4.0);
        for k in 0..20 {
            let p = 2.0 + k as f32 * 4.5;
            assert!(g.height_at(p, p, 3.0, 1.0).is_some(), "missing at {p}");
        }
    }
}
