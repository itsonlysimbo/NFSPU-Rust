//! Invisible walls along road edges that drop into void.
//!
//! Retail keeps cars on the road with its road-edge polygons; we only have the track's steep
//! faces. Where the mesh has no ground and no wall within a few metres past the road's outer
//! edge, a car can drive off and fall. This adds a plain wall strip along such stretches, found
//! with the same test `collision_audit` reports as `OPEN`.
use crate::collision::WallSet;
use crate::ground::{Ground, Tri};
use crate::race_path::RoadPath;

/// How far past the edge we look for something that already holds the car.
const LOOK: f32 = 4.0;
/// The strip stands just outside the nominal edge.
const OFFSET: f32 = 0.4;
/// Wall extent below and above the road surface.
const DOWN: f32 = 1.5;
const UP: f32 = 3.0;
/// Slices further apart than this are separate road pieces, not one stretch.
const MAX_GAP: f32 = 20.0;

fn is_open(ground: &Ground, walls: &WallSet, path: &RoadPath, i: usize, side: f32) -> bool {
    let s = &path.slices[i];
    let edge = if side < 0.0 { s.left_width } else { s.right_width };
    if edge <= 0.5 {
        return false;
    }
    for m in 1..=(LOOK / 0.5) as i32 {
        let p = s.pos + s.right * (side * (edge + m as f32 * 0.5));
        if ground.surface_near(p.x, p.z, 6.0, s.pos.y - 5.0, s.pos.y + 1.0).is_some()
            || walls.resolve(p.x, p.z, s.pos.y - 0.3, s.pos.y + crate::car::RIDE_HEIGHT + 1.0, 1.0).is_some()
        {
            return false;
        }
    }
    true
}

/// Wall triangles for every open outer edge of `path`, given the track's own ground and walls.
pub fn open_edge_walls(path: &RoadPath, ground: &Ground, walls: &WallSet) -> Vec<Tri> {
    let mut out = Vec::new();
    for seg in &path.segments {
        for i in seg.first..seg.last {
            let (a, b) = (&path.slices[i], &path.slices[i + 1]);
            if a.pos.distance(b.pos) > MAX_GAP {
                continue;
            }
            for side in [-1.0f32, 1.0] {
                if !(is_open(ground, walls, path, i, side) || is_open(ground, walls, path, i + 1, side)) {
                    continue;
                }
                let edge = |s: &crate::race_path::PathSlice| {
                    let w = if side < 0.0 { s.left_width } else { s.right_width } + OFFSET;
                    s.pos + s.right * (side * w)
                };
                let (pa, pb) = (edge(a), edge(b));
                let v = |p: bevy::math::Vec3, dy: f32| [p.x, p.y + dy, p.z];
                out.push([v(pa, -DOWN), v(pb, -DOWN), v(pb, UP)]);
                out.push([v(pa, -DOWN), v(pb, UP), v(pa, UP)]);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use nfspu_formats::sim::{SimData, Slice};

    fn straight(n: usize) -> RoadPath {
        let slices = (0..n).map(|i| Slice { pos: [0.0, 0.0, -(i as f32) * 5.0], up: [0.0, 1.0, 0.0], forward: [0.0, 0.0, -1.0], right: [1.0, 0.0, 0.0], kind: 0, lanes: [0, 0], extra_widths: [0.0; 2], left_width: 5.0, right_width: 5.0 }).collect();
        RoadPath::new(&SimData { slices, groups: vec![(0, n)] })
    }

    #[test]
    fn void_beside_the_road_gets_a_strip_and_ground_does_not() {
        let path = straight(4);
        let none = WallSet::new(vec![], 4.0);
        let empty = Ground::new(vec![], 4.0);
        // nothing anywhere: both edges open, two triangles per slice pair per side
        assert_eq!(open_edge_walls(&path, &empty, &none).len(), 3 * 2 * 2);
        // a wide slab of ground under and around the road: nothing to add
        let slab: Vec<Tri> = vec![[[-30.0, 0.0, -60.0], [30.0, 0.0, -60.0], [30.0, 0.0, 30.0]], [[-30.0, 0.0, -60.0], [30.0, 0.0, 30.0], [-30.0, 0.0, 30.0]]];
        assert!(open_edge_walls(&path, &Ground::new(slab, 4.0), &none).is_empty());
    }
}
