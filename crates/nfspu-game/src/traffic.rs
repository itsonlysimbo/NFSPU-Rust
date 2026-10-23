//! Road traffic: ordinary cars that drive the road slices in both directions around the racers,
//! appear ahead of the player and vanish far behind. Findings and guesses: `docs/provenance/traffic.md`.
//!
//! Retail (decomp `AIHigh_Traffic`, `AIState_NonActive/Normal/Purgatory`):
//! - a traffic car is either waiting (hidden) or driving;
//! - it appears 60 to 65 slices ahead of (or behind) a random racer, driving either way along the
//!   road (an even choice), in one of four lanes;
//! - it is taken off when it is more than 400 m from its racer (flat distance, the larger axis plus a
//!   quarter of the smaller) and waits a random time before appearing again;
//! - it cruises near 16.5 m/s, scaled by a per-car factor of 1.0, 0.9, 0.8 or 0.7.
//!
//! Ours: the same placement and removal rules, the same speeds, the same car physics as every other
//! car (so hits move both cars), and a simple driver that follows its lane and slows for a car in
//! front. The wait time table, the lane widths, the car counts per density and the braking and
//! honking rules are guesses.

use crate::ai::Rng;
use crate::car::{collide_cars, Car, Input};
use crate::collision::WallSet;
use crate::ground::Ground;
use crate::race_path::{Direction, RoadPath};
use bevy::math::Vec3;

/// A traffic car further than this from the racer it follows is taken off (retail `0x5b25c4`, m).
pub const CULL_DIST: f32 = 400.0;
/// Slices ahead of (or behind) the racer where a traffic car appears: 60 plus 0 to 5 (retail).
pub const SPAWN_SLICES: (i32, i32) = (60, 65);
/// Retail's wait table, indexed by the number of traffic cars: a new random wait in 1..=value ticks.
pub const WAIT_TABLE: [u32; 6] = [25, 50, 75, 100, 200, 400];
/// Cruise speed of traffic going with the race (retail `16.5` m/s).
pub const CRUISE: f32 = 16.5;
/// Per-car speed factors (retail table at `0x5b24a8`-ish: 1.0, 0.9, 0.8, 0.7).
pub const SPEED_FACTORS: [f32; 4] = [1.0, 0.9, 0.8, 0.7];
/// Lane centres as a share of the half road width on the car's own side (guess; retail has four
/// lane values per vehicle type that were not decoded).
pub const LANES: [f32; 4] = [0.25, 0.4, 0.55, 0.7];
/// A car this near another car at spawn time blocks the spawn (m, guess).
pub const SPAWN_CLEAR: f32 = 30.0;
/// Seconds with almost no speed before a traffic car is taken off the road (guess).
pub const STUCK_SECONDS: f32 = 6.0;

/// How many traffic cars a density setting runs: 0 none, 1 light, 2 medium, 3 heavy. Retail's
/// count comes from the track data; this table is a guess.
pub fn cars_for_density(density: u8) -> usize {
    match density {
        0 => 0,
        1 => 4,
        2 => 8,
        _ => 12,
    }
}

/// Retail's distance measure: the larger flat axis plus a quarter of the smaller.
pub fn flat_dist(a: Vec3, b: Vec3) -> f32 {
    let (dx, dz) = ((a.x - b.x).abs(), (a.z - b.z).abs());
    if dz <= dx {
        dz * 0.25 + dx
    } else {
        dx * 0.25 + dz
    }
}

/// One car's cruise speed from its random factor (retail: at least 8.9 m/s, at most 13.4 below the base).
pub fn cruise_speed(base: f32, factor: f32) -> f32 {
    (base * factor).max(base - 13.4).max(8.9)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum State {
    /// Off the road; the ticks left before it appears again.
    Waiting(u32),
    Driving,
}

#[derive(Debug, Clone)]
pub struct TrafficCar {
    pub car: Car,
    pub state: State,
    /// Which way along the road the car drives.
    pub dir: Direction,
    /// Lane offset to the right of the driving direction, metres.
    pub lane: f32,
    pub cruise: f32,
    pub slice: usize,
    pub steer: f32,
    pub braking: bool,
    pub honks: u32,
    pub spawns: u32,
    slow_ticks: u32,
    honking: bool,
}

/// What happened this tick, for the log.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Spawned { car: usize, slice: usize, dir: Direction, speed: f32, lane: f32, from: f32 },
    Removed { car: usize, why: &'static str, dist: f32 },
    Honk { car: usize },
    Hit { car: usize, with: usize, impact: f32 },
}

pub struct Fleet {
    pub cars: Vec<TrafficCar>,
    rng: Rng,
    tick: u32,
}

fn wrap(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// The direction of travel as a vector along the road at a slice.
fn heading(path: &RoadPath, slice: usize, dir: Direction) -> Vec3 {
    path.slices[slice].forward * dir.sign()
}

/// The point of the lane at a slice: the slice centre moved to the right of the travel direction.
fn lane_point(path: &RoadPath, slice: usize, dir: Direction, lane: f32) -> Vec3 {
    let s = &path.slices[slice];
    let right = heading(path, slice, dir).cross(s.up).normalize_or_zero();
    // keep a metre from the road edge; use the narrower side so one lane value suits both ways
    let room = (s.left_width.min(s.right_width) - 1.5).max(0.0);
    s.pos + right * (room * lane).min(room)
}

fn yaw_of(h: Vec3) -> f32 {
    (-h.x).atan2(-h.z)
}

impl Fleet {
    /// `count` traffic cars, all waiting to appear (staggered by a tick each).
    pub fn new(count: usize, template: &Car, seed: u32) -> Self {
        let cars = (0..count)
            .map(|i| TrafficCar {
                car: template.reset(Vec3::new(0.0, -1000.0, 0.0), 0.0),
                state: State::Waiting(1 + i as u32),
                dir: Direction::Forward,
                lane: 0.0,
                cruise: CRUISE,
                slice: 0,
                steer: 0.0,
                braking: false,
                honks: 0,
                spawns: 0,
                slow_ticks: 0,
                honking: false,
            })
            .collect();
        Fleet { cars, rng: Rng::new(seed ^ 0x7aff_1c01), tick: 0 }
    }

    pub fn driving(&self) -> usize {
        self.cars.iter().filter(|c| c.state == State::Driving).count()
    }

    fn wait(&mut self) -> u32 {
        let n = self.cars.len().saturating_sub(1).min(WAIT_TABLE.len() - 1);
        1 + self.rng.below(WAIT_TABLE[n] as usize) as u32
    }

    /// Tries to put car `i` on the road near `reference`. Returns the event on success.
    fn spawn(&mut self, i: usize, path: &RoadPath, ground: &Ground, reference: &Car, ref_slice: usize, others: &[Vec3]) -> Option<Event> {
        // appear ahead of or behind the racer (even odds; a fast racer is always met head on from
        // the front: retail follows the racer's own direction above 30 m/s)
        let way = if reference.speed.abs() > 30.0 || self.rng.below(2) == 0 { Direction::Forward } else { Direction::Backward };
        let steps = SPAWN_SLICES.0 + self.rng.below((SPAWN_SLICES.1 - SPAWN_SLICES.0 + 1) as usize) as i32;
        let slice = path.walk(ref_slice, steps, way);
        // driving either way along the road, evenly
        let dir = if self.rng.below(2) == 0 { Direction::Forward } else { Direction::Backward };
        let lane = LANES[self.rng.below(LANES.len())];
        let factor = SPEED_FACTORS[self.rng.below(SPEED_FACTORS.len())];
        let p = lane_point(path, slice, dir, lane);
        if others.iter().any(|o| o.distance(p) < SPAWN_CLEAR) {
            return None;
        }
        let h = ground.height_at(p.x, p.z, p.y + 2.0, 0.0)?;
        if (h - p.y).abs() > 3.0 {
            return None;
        }
        let cruise = cruise_speed(CRUISE, factor);
        let yaw = yaw_of(heading(path, slice, dir));
        let t = &mut self.cars[i];
        t.car = t.car.reset(Vec3::new(p.x, h + 1.0, p.z), yaw);
        t.car.set_speed(cruise);
        t.dir = dir;
        t.lane = lane;
        t.cruise = cruise;
        t.slice = slice;
        t.state = State::Driving;
        t.slow_ticks = 0;
        t.spawns += 1;
        Some(Event::Spawned { car: i, slice, dir, speed: cruise, lane, from: flat_dist(p, reference.pos) })
    }

    /// One 64 Hz tick. `refs[0]` is the player (the car the others are placed around); the
    /// rest are the other racers. Traffic hits move both cars like any car-to-car hit.
    #[allow(clippy::too_many_arguments)]
    pub fn tick(&mut self, path: &RoadPath, ground: &Ground, walls: &WallSet, refs: &mut [&mut Car], ref_slice: usize, dt: f32, wet: bool) -> Vec<Event> {
        let mut events = Vec::new();
        self.tick += 1;
        if refs.is_empty() {
            return events;
        }
        for i in 0..self.cars.len() {
            match self.cars[i].state {
                State::Waiting(n) => {
                    if n > 1 {
                        self.cars[i].state = State::Waiting(n - 1);
                        continue;
                    }
                    let mut spots: Vec<Vec3> = refs.iter().map(|c| c.pos).collect();
                    spots.extend(self.cars.iter().filter(|c| c.state == State::Driving).map(|c| c.car.pos));
                    let reference = refs[0].clone();
                    match self.spawn(i, path, ground, &reference, ref_slice, &spots) {
                        Some(e) => events.push(e),
                        None => self.cars[i].state = State::Waiting(1),
                    }
                }
                State::Driving => self.drive(i, path, ground, walls, refs, dt, wet, &mut events),
            }
        }
        events
    }

    #[allow(clippy::too_many_arguments)]
    fn drive(&mut self, i: usize, path: &RoadPath, ground: &Ground, walls: &WallSet, refs: &mut [&mut Car], dt: f32, wet: bool, events: &mut Vec<Event>) {
        let (input, honk) = {
            let t = &self.cars[i];
            Self::think(t, path, refs)
        };
        let t = &mut self.cars[i];
        t.car.wet = wet;
        t.car.step(input, dt, ground, walls);
        t.steer = input.steer;
        t.braking = input.brake && t.car.speed > 0.5;
        t.slice = path.track(t.slice, t.car.pos);
        if honk && !t.honking {
            t.honks += 1;
            events.push(Event::Honk { car: i });
        }
        t.honking = honk;
        t.slow_ticks = if t.car.speed.abs() < 0.5 { t.slow_ticks + 1 } else { 0 };
        let dist = flat_dist(t.car.pos, refs[0].pos);
        let why = if dist > CULL_DIST {
            Some("far from the racer")
        } else if t.car.pos.y < path.slices[t.slice].pos.y - 30.0 {
            Some("fell off the road")
        } else if t.car.wants_recover {
            Some("crashed")
        } else if t.slow_ticks as f32 > STUCK_SECONDS * 64.0 {
            Some("stuck")
        } else {
            None
        };
        if let Some(why) = why {
            events.push(Event::Removed { car: i, why, dist });
            let w = self.wait();
            let t = &mut self.cars[i];
            t.state = State::Waiting(w);
            t.car = t.car.reset(Vec3::new(0.0, -1000.0, 0.0), 0.0);
            return;
        }
        for (k, r) in refs.iter_mut().enumerate() {
            if let Some(impact) = collide_cars(r, &mut self.cars[i].car) {
                if impact >= 20.0 {
                    events.push(Event::Hit { car: i, with: k, impact });
                }
            }
        }
    }

    /// The driver: steer to the lane a few metres on, hold the cruise speed, brake for a car in
    /// front in the same lane. Returns the input and whether the horn is blown (guess: a car
    /// closer than 12 m ahead and closing).
    fn think(t: &TrafficCar, path: &RoadPath, refs: &[&mut Car]) -> (Input, bool) {
        let car = &t.car;
        let look = (car.speed.abs() * 0.5).max(8.0);
        // walk to the first lane point at least `look` metres ahead
        let mut s = t.slice;
        let mut target = lane_point(path, s, t.dir, t.lane);
        for _ in 0..40 {
            if target.distance(car.pos) >= look {
                break;
            }
            let n = path.walk(s, 1, t.dir);
            if n == s {
                break;
            }
            s = n;
            target = lane_point(path, s, t.dir, t.lane);
        }
        let to = target - car.pos;
        let diff = wrap((-to.x).atan2(-to.z) - car.yaw);
        let steer = (diff * 2.5 - 0.25 * car.sim.yaw_rate).clamp(-1.0, 1.0);
        let mut want = t.cruise;
        if diff.abs() > 0.5 {
            want = want.min(8.0);
        }
        let mut honk = false;
        let (fwd, right) = (car.heading(), Vec3::new(car.yaw.cos(), 0.0, -car.yaw.sin()));
        for r in refs {
            let d = r.pos - car.pos;
            let (ahead, side) = (d.dot(fwd), d.dot(right).abs());
            // something in the way: same lane width, ahead, within a braking distance
            if ahead > 0.0 && side < 2.6 && ahead < 10.0 + car.speed.abs() * 1.6 {
                let closing = car.speed - r.speed.max(0.0);
                if closing > 0.0 || ahead < 8.0 {
                    want = want.min((r.speed * 0.8).max(0.0));
                    if ahead < 12.0 && closing > 2.0 {
                        honk = true;
                    }
                }
            }
        }
        let input = Input { throttle: car.speed < want, brake: car.speed > want + 1.0, steer, ..Default::default() };
        (input, honk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_is_the_octagon_measure() {
        let a = Vec3::ZERO;
        assert!((flat_dist(a, Vec3::new(100.0, 5.0, 0.0)) - 100.0).abs() < 1e-4);
        assert!((flat_dist(a, Vec3::new(100.0, 0.0, 100.0)) - 125.0).abs() < 1e-4);
        assert!((flat_dist(a, Vec3::new(-40.0, 0.0, 400.0)) - 410.0).abs() < 1e-4);
    }

    #[test]
    fn cruise_speeds_follow_the_retail_clamps() {
        assert!((cruise_speed(16.5, 1.0) - 16.5).abs() < 1e-4);
        assert!((cruise_speed(16.5, 0.9) - 14.85).abs() < 1e-4);
        assert!((cruise_speed(16.5, 0.7) - 11.55).abs() < 1e-4);
        // never below 8.9 m/s
        assert!((cruise_speed(10.0, 0.5) - 8.9).abs() < 1e-4);
    }

    #[test]
    fn densities_map_to_counts() {
        assert_eq!(cars_for_density(0), 0);
        assert!(cars_for_density(1) < cars_for_density(2) && cars_for_density(2) < cars_for_density(3));
    }

    #[test]
    fn waiting_cars_count_down_and_the_wait_is_bounded() {
        let mut f = Fleet::new(3, &Car::new(Vec3::ZERO, 0.0), 7);
        for _ in 0..200 {
            let w = f.wait();
            assert!((1..=WAIT_TABLE[2]).contains(&w), "wait {w}");
        }
        assert_eq!(f.driving(), 0);
    }
}
