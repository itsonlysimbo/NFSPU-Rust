//! Police cars for pursuit mode. Findings and guesses: `docs/provenance/pursuit.md`.
//!
//! Retail reads `Track/Cops/<track>.cop`: a list of records, each a cop waiting at a road slice
//! (`nfspu_formats::aidata::parse_cop`). A waiting cop is idle until the pursued car comes near;
//! then it takes the chase state (retail `AIState_Chase`), ramming when close. After a while on a
//! chase the police call a roadblock ahead of the player.
//!
//! Ours: the same placement from the file and a simple driver on the shared car physics (the 993's
//! for every cop). Wake-up distance, speeds, ramming and roadblock timing are guesses.

use crate::ai::Rng;
use crate::car::{collide_cars, Car, Input};
use crate::collision::WallSet;
use crate::ground::Ground;
use crate::race_path::{Direction, RoadPath};
use crate::traffic::flat_dist;
use bevy::math::Vec3;
use nfspu_formats::aidata::CopSpot;

/// A waiting cop wakes when the player comes this near (m, guess; retail starts a chase between
/// 50 and 300 m).
pub const WAKE_DIST: f32 = 150.0;
/// A cop this far from the player gives up (m, guess).
pub const GIVE_UP_DIST: f32 = 450.0;
/// The most a cop drives (m/s, guess).
pub const MAX_SPEED: f32 = 55.0;
/// Inside this distance the cop rams: it aims at the player's car, not the road (m).
pub const RAM_DIST: f32 = 14.0;
/// Seconds of chasing before the police call a roadblock, and between roadblocks (guess).
pub const ROADBLOCK_AFTER: f32 = 45.0;
pub const ROADBLOCK_EVERY: f32 = 90.0;
/// How far ahead of the player the roadblock goes (m, guess).
pub const ROADBLOCK_AHEAD: f32 = 260.0;
/// A parked roadblock is dropped when the player is this far past it (m).
pub const ROADBLOCK_CLEAR: f32 = 120.0;
/// Seconds without moving before a cop reverses out (guess).
const STUCK_SECONDS: f32 = 2.0;
const HZ: f32 = 64.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum State {
    /// Parked at its spot, not yet woken.
    Waiting,
    Chasing,
    /// Holding off after a ticket.
    Backing,
    /// A parked roadblock car.
    Block,
    /// Gave up or was passed; takes no further part.
    Out,
}

#[derive(Debug, Clone)]
pub struct Cop {
    pub car: Car,
    pub state: State,
    pub slice: usize,
    /// Which cop car (index into the templates given to `Squad::new`).
    pub variant: usize,
    pub rams: u32,
    still: u32,
    reverse: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Woke { cop: usize, slice: usize, dist: f32 },
    Rammed { cop: usize, impact: f32 },
    Roadblock { slice: usize, cars: usize },
    RoadblockHit { impact: f32 },
    GaveUp { cop: usize, dist: f32 },
}

pub struct Squad {
    pub cops: Vec<Cop>,
    rng: Rng,
    chase_ticks: u32,
    last_block: Option<u32>,
    pub roadblocks: u32,
    pub rams: u32,
}

fn wrap(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

fn yaw_of(h: Vec3) -> f32 {
    (-h.x).atan2(-h.z)
}

/// A point on the road at `slice`, `frac` of the way from the centre to the right edge
/// (negative: left), kept a metre and a half inside.
fn road_point(path: &RoadPath, slice: usize, frac: f32) -> Vec3 {
    let s = &path.slices[slice];
    let room = if frac >= 0.0 { s.right_width } else { s.left_width };
    s.pos + s.right * (frac * (room - 1.5).max(0.0))
}

impl Squad {
    /// One parked cop per spot, on the right of the road for side 1 and the left for side -1
    /// (side 0: right), facing along the race direction.
    pub fn new(spots: &[CopSpot], path: &RoadPath, ground: &Ground, templates: &[Car], dir: Direction, seed: u32) -> Self {
        let mut rng = Rng::new(seed ^ 0xc0b5_0001);
        let mut cops = Vec::new();
        for sp in spots {
            let slice = (sp.slice as usize).min(path.slices.len().saturating_sub(1));
            let frac = 0.55 * if sp.side == 0 { 1.0 } else { sp.side as f32 };
            let p = road_point(path, slice, frac);
            let Some(h) = ground.height_at(p.x, p.z, p.y + 2.0, 0.0) else { continue };
            if (h - p.y).abs() > 3.0 {
                continue;
            }
            let yaw = yaw_of(path.slices[slice].forward * dir.sign());
            let variant = rng.below(templates.len().max(1));
            let Some(template) = templates.get(variant) else { continue };
            cops.push(Cop { car: template.reset(Vec3::new(p.x, h + 1.0, p.z), yaw), state: State::Waiting, slice, variant, rams: 0, still: 0, reverse: 0 });
        }
        Squad { cops, rng, chase_ticks: 0, last_block: None, roadblocks: 0, rams: 0 }
    }

    pub fn chasing(&self) -> usize {
        self.cops.iter().filter(|c| c.state == State::Chasing).count()
    }

    /// Distance from the player to the nearest chasing cop.
    pub fn nearest_chaser(&self, player: &Car) -> Option<f32> {
        self.cops.iter().filter(|c| c.state == State::Chasing).map(|c| c.car.pos.distance(player.pos)).min_by(|a, b| a.total_cmp(b))
    }

    /// One 64 Hz tick. `hold`: the cops are backing off after a ticket.
    #[allow(clippy::too_many_arguments)]
    pub fn tick(&mut self, path: &RoadPath, ground: &Ground, walls: &WallSet, player: &mut Car, player_slice: usize, dir: Direction, hold: bool, dt: f32) -> Vec<Event> {
        let mut ev = Vec::new();
        let mut any_chase = false;
        for i in 0..self.cops.len() {
            let dist = flat_dist(self.cops[i].car.pos, player.pos);
            match self.cops[i].state {
                State::Waiting => {
                    self.cops[i].car.step(Input { brake: true, ..Default::default() }, dt, ground, walls);
                    if dist < WAKE_DIST {
                        self.cops[i].state = State::Chasing;
                        ev.push(Event::Woke { cop: i, slice: self.cops[i].slice, dist });
                    }
                }
                State::Chasing | State::Backing => {
                    self.cops[i].state = if hold { State::Backing } else { State::Chasing };
                    any_chase |= !hold;
                    self.drive(i, path, ground, walls, player, player_slice, hold, dt);
                    if dist > GIVE_UP_DIST {
                        self.cops[i].state = State::Out;
                        ev.push(Event::GaveUp { cop: i, dist });
                        continue;
                    }
                    if let Some(impact) = collide_cars(player, &mut self.cops[i].car) {
                        if impact >= 8.0 {
                            self.cops[i].rams += 1;
                            self.rams += 1;
                            ev.push(Event::Rammed { cop: i, impact });
                        }
                    }
                }
                State::Block => {
                    self.cops[i].car.step(Input { brake: true, ..Default::default() }, dt, ground, walls);
                    if let Some(impact) = collide_cars(player, &mut self.cops[i].car) {
                        if impact >= 8.0 {
                            ev.push(Event::RoadblockHit { impact });
                        }
                    }
                    // passed: the player is further along the race than the block, and far from it
                    let passed = path.fraction(player_slice, dir) > path.fraction(self.cops[i].slice, dir) && dist > ROADBLOCK_CLEAR;
                    if passed {
                        self.cops[i].state = State::Out;
                    }
                }
                State::Out => {}
            }
        }
        if any_chase {
            self.chase_ticks += 1;
            let due = match self.last_block {
                None => self.chase_ticks as f32 >= ROADBLOCK_AFTER * HZ,
                Some(t) => (self.chase_ticks - t) as f32 >= ROADBLOCK_EVERY * HZ,
            };
            if due {
                if let Some(e) = self.roadblock(path, ground, player_slice, dir) {
                    self.last_block = Some(self.chase_ticks);
                    ev.push(e);
                }
            }
        }
        ev
    }

    /// Two parked cars across the road about `ROADBLOCK_AHEAD` metres ahead of the player, on the
    /// two outer sides so there is a gap in the middle.
    fn roadblock(&mut self, path: &RoadPath, ground: &Ground, player_slice: usize, dir: Direction) -> Option<Event> {
        let template = self.cops.first()?.car.clone();
        let mut slice = player_slice;
        let mut got = 0.0;
        while got < ROADBLOCK_AHEAD {
            let n = path.walk(slice, 1, dir);
            if n == slice {
                return None;
            }
            got += path.slices[n].pos.distance(path.slices[slice].pos);
            slice = n;
        }
        let yaw = yaw_of(path.slices[slice].right * if self.rng.below(2) == 0 { 1.0 } else { -1.0 });
        let mut cars = 0;
        // each car lies across the road against an edge (2.25 m = half its length); on a narrow
        // road only one side is blocked, so there is always a way through
        let sl = &path.slices[slice];
        let sides: Vec<f32> = if sl.left_width.min(sl.right_width) >= 7.0 { vec![-1.0, 1.0] } else { vec![if self.rng.below(2) == 0 { -1.0 } else { 1.0 }] };
        for side in sides {
            let room = if side > 0.0 { sl.right_width } else { sl.left_width };
            let frac = side * ((room - 2.5).max(0.0) / (room - 1.5).max(0.1));
            let p = road_point(path, slice, frac);
            let Some(h) = ground.height_at(p.x, p.z, p.y + 2.0, 0.0) else { continue };
            self.cops.push(Cop { car: template.reset(Vec3::new(p.x, h + 1.0, p.z), yaw), state: State::Block, slice, variant: 0, rams: 0, still: 0, reverse: 0 });
            cars += 1;
        }
        if cars == 0 {
            return None;
        }
        self.roadblocks += 1;
        Some(Event::Roadblock { slice, cars })
    }

    #[allow(clippy::too_many_arguments)]
    fn drive(&mut self, i: usize, path: &RoadPath, ground: &Ground, walls: &WallSet, player: &Car, player_slice: usize, backing: bool, dt: f32) {
        let c = &mut self.cops[i];
        c.slice = path.track(c.slice, c.car.pos);
        let dist = (player.pos - c.car.pos).length();
        // far away: follow the road towards the player's slice; close: aim at where the player
        // will be in half a second
        let target = if dist > 45.0 && !backing {
            let way = if path.fraction(player_slice, Direction::Forward) >= path.fraction(c.slice, Direction::Forward) { Direction::Forward } else { Direction::Backward };
            let s = path.walk(c.slice, 6, way);
            road_point(path, s, 0.0)
        } else {
            player.pos + player.heading() * player.speed * 0.5
        };
        let to = target - c.car.pos;
        let diff = wrap((-to.x).atan2(-to.z) - c.car.yaw);
        let steer = (diff * 2.5 - 0.25 * c.car.sim.yaw_rate).clamp(-1.0, 1.0);
        let mut want = if backing { 0.0 } else { (player.speed.abs() * 1.08 + 4.0).min(MAX_SPEED) };
        if dist < RAM_DIST && !backing {
            want = player.speed.abs() + 8.0;
        }
        if diff.abs() > 0.6 {
            want = want.min(10.0);
        }
        c.still = if c.car.speed.abs() < 0.8 && !backing { c.still + 1 } else { 0 };
        if c.still as f32 > STUCK_SECONDS * HZ {
            c.reverse = (1.2 * HZ) as u32;
            c.still = 0;
        }
        let input = if c.reverse > 0 {
            c.reverse -= 1;
            Input { brake: true, steer: -steer, ..Default::default() }
        } else {
            Input { throttle: c.car.speed < want, brake: c.car.speed > want + 2.0, steer, ..Default::default() }
        };
        c.car.step(input, dt, ground, walls);
    }
}

/// Reads `Track/Cops/<track>.cop` from the game folder.
pub fn load_spots(game: &std::path::Path, track: &str) -> Vec<CopSpot> {
    let p = game.join("GameData").join("Track").join("Cops").join(format!("{track}.cop"));
    std::fs::read(p).map(|d| nfspu_formats::aidata::parse_cop(&d)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_no_cops() {
        assert!(load_spots(std::path::Path::new("."), "nowhere").is_empty());
    }

    #[test]
    fn siren_alternates() {
        let phases: std::collections::BTreeSet<u8> = (0..80).map(crate::pursuit::siren_light).collect();
        assert_eq!(phases.len(), 3);
    }
}
