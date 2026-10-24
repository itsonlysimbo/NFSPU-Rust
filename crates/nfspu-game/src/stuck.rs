//! Putting a lost player car back on the road. Retail resets a car that is stuck for 320 ticks
//! (5 s at 64 Hz), that has left the world (35 m past the road edge) or that moves impossibly
//! fast (decomp notes in `docs/provenance/physics.md` section 10). The crash timer in `car.rs`
//! covers big hits; this covers the rest. Pure logic so the race and `race_drive` share it.

use crate::car::Car;
use crate::race_path::{Direction, RoadPath};

/// Retail: stuck for this many 64 Hz ticks.
pub const STUCK_TICKS: u32 = 320;
/// Retail: the car counts as out of bounds this far past the road edge (metres).
pub const OUT_OF_BOUNDS: f32 = 35.0;
/// A car this far below the road slice it is nearest has fallen through the world (own rule).
pub const FALL_DEPTH: f32 = 30.0;
/// Retail: a speed above this (m/s) is a glitch and resets the car.
pub const MAX_SPEED: f32 = 115.0;
/// Slower than this along the road counts as not moving (m/s, own rule; the AI uses the same idea).
const MOVING: f32 = 1.5;
/// Airborne this long (64 Hz ticks, 4 s) means no ground is coming: the car left the world (own rule).
pub const AIR_TICKS: u32 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    Stuck,
    Fell,
    OffRoad,
    TooFast,
}

impl Why {
    pub fn text(self) -> &'static str {
        match self {
            Why::Stuck => "stuck",
            Why::Fell => "fell through the world",
            Why::OffRoad => "out of bounds",
            Why::TooFast => "impossible speed",
        }
    }
}

#[derive(Default, Clone)]
pub struct Watch {
    still: u32,
    airborne: u32,
}

impl Watch {
    /// One 64 Hz tick with the car's nearest road slice. Returns why the car must be put back.
    pub fn tick(&mut self, car: &Car, path: &RoadPath, slice: usize, dir: Direction) -> Option<Why> {
        let s = &path.slices[slice];
        let heading = car.heading();
        let road = s.forward * dir.sign();
        let along = car.speed * heading.dot(road);
        self.still = if along > MOVING { 0 } else { self.still + 1 };
        self.airborne = if car.on_ground { 0 } else { self.airborne + 1 };
        if car.pos.y < s.pos.y - FALL_DEPTH || self.airborne > AIR_TICKS {
            return Some(Why::Fell);
        }
        // out of bounds only when far from the tracked slice and from the nearest slice anywhere
        // (the tracked one can sit on a side stub while the car is on the main road)
        let outside = |s: &crate::race_path::PathSlice| {
            let lateral = (car.pos - s.pos).dot(s.right);
            let width = if lateral < 0.0 { s.left_width } else { s.right_width };
            lateral.abs() > width + OUT_OF_BOUNDS || (car.pos - s.pos).length() > OUT_OF_BOUNDS * 3.0
        };
        if outside(s) && outside(&path.slices[path.nearest(car.pos)]) {
            return Some(Why::OffRoad);
        }
        if car.speed.abs() > MAX_SPEED {
            return Some(Why::TooFast);
        }
        (self.still > STUCK_TICKS).then_some(Why::Stuck)
    }

    /// Call after the car has been put back.
    pub fn reset(&mut self) {
        self.still = 0;
        self.airborne = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::race_path::RoadPath;
    use nfspu_formats::sim::{Slice, SimData};
    use bevy::math::Vec3;

    fn straight() -> RoadPath {
        let slices = (0..10).map(|i| Slice { pos: [0.0, 0.0, -(i as f32) * 5.0], up: [0.0, 1.0, 0.0], forward: [0.0, 0.0, -1.0], right: [1.0, 0.0, 0.0], kind: 0, lanes: [0, 0], extra_widths: [0.0; 2], left_width: 6.0, right_width: 6.0 }).collect();
        RoadPath::new(&SimData { slices, groups: vec![(0, 10)] })
    }

    #[test]
    fn a_car_sitting_still_is_reset_after_the_stuck_time() {
        let path = straight();
        let car = Car::new(Vec3::new(0.0, 0.45, -10.0), 0.0);
        let mut w = Watch::default();
        let mut out = None;
        for _ in 0..400 {
            out = out.or(w.tick(&car, &path, 2, Direction::Forward));
        }
        assert!(matches!(out, Some(Why::Stuck) | Some(Why::Fell)), "{out:?}");
    }

    #[test]
    fn a_car_far_below_or_off_the_road_is_reset_at_once() {
        let path = straight();
        let mut w = Watch::default();
        assert_eq!(w.tick(&Car::new(Vec3::new(0.0, -50.0, -10.0), 0.0), &path, 2, Direction::Forward), Some(Why::Fell));
        assert_eq!(w.tick(&Car::new(Vec3::new(60.0, 0.45, -10.0), 0.0), &path, 2, Direction::Forward), Some(Why::OffRoad));
    }
}
