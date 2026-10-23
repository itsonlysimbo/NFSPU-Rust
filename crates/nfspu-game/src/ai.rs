//! Opponent drivers: who races (drivers and their cars), and how each one drives the road slices.
//! Pure logic (no Bevy systems); the race calls `Opponent::think` once per 64 Hz tick and feeds
//! the returned input to the same car physics the player uses.
//!
//! Retail behaviour this follows (from the decomp and the data files, `docs/provenance/race.md`,
//! "Opponents"); the code is our own:
//! - Target speed: the driver looks a number of slices ahead that grows with speed (the car's
//!   `.ais` lookahead table, indexed by `speed * 0.2`, max 23). Over those slices it finds the
//!   largest bend to the left and to the right, in degrees between the slice directions, and adds
//!   them. That sum (max 115) divided by 3 indexes the `.ais` speed table.
//! - Skill: each driver (`nfs5.per`) has a skill and a range for sharp bends, medium bends, fast
//!   road and a default. Whenever the kind of road ahead changes, a new factor is drawn: with
//!   probability `1 - skill` it is uniform in `skill +- range` of that kind of road, otherwise
//!   uniform in the default `skill +- range`. The table speed is multiplied by it.
//! - Narrow road: below 12 m total width the speed is scaled down by `width / 12`.
//! - Lead slowdown: an opponent more than a set part of a lap ahead of the player drives slower,
//!   only on advanced and expert, and only while the player keeps up a lap rate (see
//!   `lead_factor`). No speed-up for opponents behind was found.
//! - Steering: toward the average of four racing-line points (`.lsp`) a little way ahead, as a
//!   steer value proportional to the sideways part of that direction, full lock at about 30 deg.
//! - Throttle when slower than the target, brake when faster.
//! - Stuck: slower than 2 m/s for 160 ticks, or facing backwards under 10 m/s, starts a recovery.
//! - Junctions: the route is picked by `ai_route` (retail rules) and every look along the road
//!   follows it.
//!
//! Ours (not read from retail, marked "guess" where used): how far ahead the steering looks, the
//! recovery manoeuvre and the reset after 10 s stuck, the avoidance of other cars, the yaw-rate damping of the steering, and lifting off at
//! full lock or while sliding.

use crate::ai_route::{Choice, Routes};
use crate::car::{Car, Input};
use crate::catalog;
use crate::race_path::{Direction, RoadPath};
use crate::race_setup::OpponentCars;
use bevy::math::Vec3;
use nfspu_formats::aidata::{self, AiSpec, Driver, Stretch};
use std::path::{Path, PathBuf};

/// The largest bend sum used to index the speed table (retail clamps at 115 degrees).
pub const MAX_BEND: f32 = 115.0;
/// Road narrower than this (both half-widths together, metres) slows the driver down.
pub const FULL_WIDTH: f32 = 12.0;
/// Below this speed (m/s) for `STUCK_TICKS` ticks a driver counts as stuck (retail: 2.0, 160).
pub const STUCK_SPEED: f32 = 2.0;
pub const STUCK_TICKS: u32 = 160;
/// Recovery: reverse this many ticks (guess: 1.5 s).
pub const REVERSE_TICKS: u32 = 96;
/// Without getting above `MOVING_SPEED` for this long, the car is put back on the road (guess).
pub const RESET_TICKS: u32 = 640;
pub const MOVING_SPEED: f32 = 5.0;

/// Lead slowdown by difficulty (beginner, advanced, expert): laps ahead of the player before it
/// starts, and the speed factor then. Beginner's 1.0 lap with factor 1.0 means none.
const LEAD_LAPS: [f32; 3] = [1.0, 0.07, 0.03];
const LEAD_FACTOR: [f32; 3] = [1.0, 0.98, 0.95];
/// The player has to make this much progress: `laps gained * 640000 / race ticks` over 35, i.e.
/// a lap in under about 286 s.
const LEAD_PLAYER_RATE: f32 = 35.0;

/// A small deterministic random source (xorshift), so a race can be replayed from its seed.
#[derive(Debug, Clone)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Rng(seed.max(1))
    }

    pub fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// Uniform in 0..1.
    pub fn unit(&mut self) -> f32 {
        (self.next() >> 8) as f32 / (1u32 << 24) as f32
    }

    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            self.next() as usize % n
        }
    }
}

/// Slices to look ahead at `speed` m/s.
pub fn lookahead_slices(spec: &AiSpec, speed: f32) -> usize {
    let i = (speed.abs() * 0.2).min(23.0).round() as usize;
    spec.lookahead[i].round().max(0.0) as usize
}

/// Sum of the sharpest bend left and the sharpest bend right (degrees) between the slice a car is
/// on and the slices 2..=`ahead` in front of it, along the route `walk` takes.
pub fn bend_degrees(path: &RoadPath, slice: usize, ahead: usize, walk: impl Fn(usize, i32) -> usize) -> f32 {
    let here = path.slices[slice].forward;
    let (mut left, mut right) = (0.0f32, 0.0f32);
    for k in 2..=ahead {
        let s = &path.slices[walk(slice, k as i32)];
        let angle = s.forward.dot(here).clamp(-1.0, 1.0).acos().to_degrees();
        if s.right.dot(here) >= 0.0 {
            left = left.max(angle);
        } else {
            right = right.max(angle);
        }
    }
    left + right
}

/// The `.ais` table speed for a bend sum.
pub fn table_speed(spec: &AiSpec, bend: f32) -> f32 {
    let i = (bend.clamp(0.0, MAX_BEND) / 3.0).round() as usize;
    spec.speeds[i.min(spec.speeds.len() - 1)]
}

/// What kind of road a bend sum is: over 60 degrees sharp, over 30 medium, else fast.
pub fn stretch_for(bend: f32) -> Stretch {
    let steps = bend.min(MAX_BEND) / 3.0;
    if steps > 20.0 {
        Stretch::Slow
    } else if steps > 10.0 {
        Stretch::Medium
    } else {
        Stretch::Fast
    }
}

/// A fresh skill factor for a kind of road (see the module notes).
pub fn draw_skill(driver: &Driver, stretch: Stretch, rng: &mut Rng) -> f32 {
    let (skill, range) = driver.skill(stretch);
    let use_stretch = rng.unit() > skill;
    let (centre, spread) = if use_stretch { (skill, range) } else { driver.skill(Stretch::Default) };
    centre - spread + rng.unit() * 2.0 * spread
}

/// Speed factor for an opponent `lead` laps ahead of the player (negative = behind), with the
/// player's lap rate `laps gained * 640000 / race ticks`.
pub fn lead_factor(difficulty: u8, lead: f32, player_rate: f32) -> f32 {
    let d = (difficulty as usize).min(2);
    if lead > LEAD_LAPS[d] && player_rate > LEAD_PLAYER_RATE {
        LEAD_FACTOR[d]
    } else {
        1.0
    }
}

/// The player's lap rate as the lead slowdown measures it.
pub fn player_rate(progress_gained: f32, race_ticks: u32) -> f32 {
    if race_ticks == 0 {
        0.0
    } else {
        progress_gained * 640_000.0 / race_ticks as f32
    }
}

/// The racing line in Bevy space, one point per slice, as the file has it. `lsp` is the file's
/// (x, z) per slice; without a matching file the slice centres are used.
pub fn racing_line(path: &RoadPath, lsp: Option<&[[f32; 2]]>) -> Vec<Vec3> {
    match lsp {
        Some(p) if p.len() == path.slices.len() => path.slices.iter().zip(p).map(|(s, q)| Vec3::new(q[0] * crate::world_mirror_x(), s.pos.y, q[1])).collect(),
        _ => path.slices.iter().map(|s| s.pos).collect(),
    }
}

/// The racing line the drivers follow: `racing_line` with every point moved onto its slice's
/// cross-line and kept at least `EDGE_MARGIN` inside the slice's half-widths. Some file points
/// lie well outside them (up to 36 m on foothills, `race_audit`); keeping the line on the road is
/// ours.
pub fn driving_line(path: &RoadPath, lsp: Option<&[[f32; 2]]>) -> Vec<Vec3> {
    racing_line(path, lsp)
        .into_iter()
        .zip(&path.slices)
        .map(|(q, s)| {
            let side = (q - s.pos).dot(s.right);
            let lo = (-s.left_width + EDGE_MARGIN).min(0.0);
            let hi = (s.right_width - EDGE_MARGIN).max(0.0);
            s.pos + s.right * side.clamp(lo, hi)
        })
        .collect()
}

/// How far inside the road edge the driving line stays (metres, ours).
pub const EDGE_MARGIN: f32 = 1.0;

/// Where a reset car goes: on the racing line at `slice`, facing the race direction.
pub fn reset_pose(path: &RoadPath, line: &[Vec3], slice: usize, dir: Direction) -> (Vec3, f32) {
    let s = &path.slices[slice];
    let h = s.forward * dir.sign();
    (line[slice] + s.up * 1.0, (-h.x).atan2(-h.z))
}

/// What a driver needs to know about the race this tick.
pub struct View<'a> {
    pub path: &'a RoadPath,
    pub routes: &'a Routes,
    pub line: &'a [Vec3],
    pub dir: Direction,
    /// Cars must stay on the grid.
    pub held: bool,
    /// This car has finished: it brakes to a stop (ours; retail's after-finish driving not read).
    pub finished: bool,
    pub difficulty: u8,
    /// This car's progress minus the player's, in laps.
    pub lead: f32,
    /// See `player_rate`.
    pub player_rate: f32,
}

/// Another car near the driver.
#[derive(Debug, Clone, Copy)]
pub struct Other {
    pub pos: Vec3,
    pub speed: f32,
    /// Which way the car points (flat), so oncoming traffic is told from traffic going our way.
    pub heading: Vec3,
}

impl Other {
    pub fn of(car: &Car) -> Self {
        Other { pos: car.pos, speed: car.speed, heading: car.heading() }
    }
}

/// Room a car needs to pass beside another: a car width plus a margin (m, guess).
pub const PASS_ROOM: f32 = 3.2;
/// How far ahead, in seconds of closing speed, a driver reacts to a car in its path (guess).
pub const REACT_SECONDS: f32 = 2.2;

/// Keeping clear of cars ahead, racers and traffic alike: returns a sideways shift for the aim
/// point (overtake or change lane on the side of the road with more room) and a speed cap
/// (follow behind, or brake for a car that can't be passed). `along_road` is the slice the car is
/// on. Retail's opponents dodge traffic too, but its avoidance code was not read: the rules here
/// are ours. A car going the other way closes at both speeds added, so it is seen sooner.
pub fn avoid(car: &Car, slice: &crate::race_path::PathSlice, others: &[Other]) -> (Vec3, f32) {
    let heading = car.heading();
    let right = car_right(car);
    let speed = car.speed;
    // the road edges measured along our own right, from the slice centre
    let flip = right.dot(slice.right) < 0.0;
    let (left_edge, right_edge) = if flip { (-slice.right_width, slice.left_width) } else { (-slice.left_width, slice.right_width) };
    let me = (car.pos - slice.pos).dot(right);
    let mut shift = 0.0f32;
    let mut cap = f32::INFINITY;
    for o in others {
        let rel = o.pos - car.pos;
        let along = rel.dot(heading);
        let side = rel.dot(right);
        let their = o.speed * o.heading.dot(heading);
        let closing = speed - their;
        let reach = 11.0 + closing.max(0.0) * REACT_SECONDS;
        if along <= 0.0 || along > reach || side.abs() > 2.4 + (closing > 15.0) as u8 as f32 * 0.6 {
            continue;
        }
        if closing < 1.0 && along > 9.0 {
            continue; // pulling away or matched speed: nothing to do yet
        }
        // room between the other car and each road edge (1 m kept from the edge)
        let ob = me + side;
        let room_left = ob - left_edge - 1.0;
        let room_right = right_edge - ob - 1.0;
        let go_left = if side.abs() > 0.6 && (if side > 0.0 { room_left } else { room_right }) >= PASS_ROOM {
            side > 0.0 // already off to one side: keep going that way
        } else {
            room_left > room_right
        };
        let room = if go_left { room_left } else { room_right };
        // urgency: 1 when about to touch, less further out
        let urgency = (1.0 - along / reach).clamp(0.35, 1.0);
        if room >= PASS_ROOM {
            let want = if go_left { ob - PASS_ROOM } else { ob + PASS_ROOM } - me;
            let push = want.clamp(-5.0, 5.0) * urgency;
            if push.abs() > shift.abs() {
                shift = push;
            }
            if along < 7.0 {
                cap = cap.min(their.max(0.0) + 3.0); // +3: still able to pull away from the grid
            }
        } else {
            // no gap: brake to arrive behind it at its speed (oncoming: as slow as we can)
            let gap = (along - 7.0).max(0.0);
            cap = cap.min((their.max(0.0) * their.max(0.0) + 2.0 * 6.0 * gap).sqrt().max(their.max(0.0)));
        }
    }
    (right * shift, cap)
}

/// One opponent's driving state.
#[derive(Debug, Clone)]
pub struct Opponent {
    pub spec: AiSpec,
    pub driver: Driver,
    /// Slice the car is on (kept up to date by `think`).
    pub slice: usize,
    stretch: Option<Stretch>,
    pub skill: f32,
    /// Speed aimed for this tick (m/s), for the log.
    pub target_speed: f32,
    slow_ticks: u32,
    reverse_ticks: u32,
    since_moving: u32,
    pub recoveries: u32,
    /// The junction last chosen at and the route taken there.
    pub route: Option<Choice>,
    /// Set when a route was chosen this tick (for logs); the caller may take it.
    pub new_choice: Option<Choice>,
}

fn car_right(car: &Car) -> Vec3 {
    Vec3::new(car.yaw.cos(), 0.0, -car.yaw.sin())
}

impl Opponent {
    pub fn new(spec: AiSpec, driver: Driver, slice: usize) -> Self {
        Opponent { spec, driver, slice, stretch: None, skill: 1.0, target_speed: 0.0, slow_ticks: 0, reverse_ticks: 0, since_moving: 0, recoveries: 0, route: None, new_choice: None }
    }

    /// True once the car has been stuck long enough to be put back on the road (guess).
    pub fn wants_reset(&self) -> bool {
        self.since_moving > RESET_TICKS
    }

    /// Call after putting the car back on the road.
    pub fn was_reset(&mut self, slice: usize) {
        self.slice = slice;
        self.since_moving = 0;
        self.slow_ticks = 0;
        self.reverse_ticks = 0;
    }

    /// The speed this driver wants at its slice.
    pub fn target(&mut self, speed: f32, v: &View, rng: &mut Rng) -> f32 {
        let ahead = lookahead_slices(&self.spec, speed);
        let route = self.route;
        let bend = bend_degrees(v.path, self.slice, ahead, |s, k| v.routes.walk(s, k, v.dir, route));
        let stretch = stretch_for(bend);
        if self.stretch != Some(stretch) {
            self.stretch = Some(stretch);
            self.skill = draw_skill(&self.driver, stretch, rng);
        }
        let s = &v.path.slices[self.slice];
        let width = ((s.left_width + s.right_width) / FULL_WIDTH).min(1.0);
        table_speed(&self.spec, bend) * self.skill * width * lead_factor(v.difficulty, v.lead, v.player_rate)
    }

    /// One tick of driving: where to steer and whether to accelerate or brake.
    pub fn think(&mut self, car: &Car, v: &View, others: &[Other], rng: &mut Rng) -> Input {
        self.slice = v.path.track(self.slice, car.pos);
        // route choice: once per junction, when it comes within the look-ahead
        if let Some(j) = v.routes.junction_ahead(self.slice, lookahead_slices(&self.spec, car.speed), v.dir) {
            if self.route.is_none_or(|c| c.junction != j) {
                let r = rng.unit();
                if let Some(c) = v.routes.choose(j, v.difficulty, r, v.dir) {
                    self.route = Some(c);
                    self.new_choice = Some(c);
                }
            }
        }
        if v.held {
            return Input::default();
        }
        if v.finished {
            return Input { brake: car.speed > 0.5, ..Default::default() };
        }
        let speed = car.speed;
        let mut target_speed = self.target(speed, v, rng);
        let heading = car.heading();
        let right = car_right(car);

        // steering point: average of four racing-line points, about 0.6 s ahead (guess)
        let lead = ((speed.max(0.0) * 0.6 / 6.0).round() as i32).clamp(2, 12);
        let mut aim = Vec3::ZERO;
        for i in 0..4 {
            aim += v.line[v.routes.walk(self.slice, lead + i, v.dir, self.route)];
        }
        aim /= 4.0;

        // keep clear of cars ahead: racers and traffic
        let (shift, cap) = avoid(car, &v.path.slices[self.slice], others);
        aim += shift;
        target_speed = target_speed.min(cap);
        self.target_speed = target_speed;

        let mut to = aim - car.pos;
        to.y = 0.0;
        // damped with the yaw rate so the retail tyre model does not swing the car round (ours)
        let steer = (-(to.normalize_or_zero().dot(right) * 3.0) - 0.25 * car.sim.yaw_rate).clamp(-1.0, 1.0);
        let sliding = car.sim.lateral_speed(car.yaw).abs() > 3.0;

        // stuck: too slow for too long, or facing back down the road
        let road = v.path.slices[self.slice].forward * v.dir.sign();
        let backwards = heading.dot(road) < -0.707 && speed.abs() < 10.0;
        self.slow_ticks = if speed.abs() < STUCK_SPEED { self.slow_ticks + 1 } else { 0 };
        // moving = going along the road; rocking back and forth against a wall does not count
        self.since_moving = if speed * heading.dot(road) > MOVING_SPEED { 0 } else { self.since_moving + 1 };
        if self.reverse_ticks == 0 && (self.slow_ticks > STUCK_TICKS || backwards) {
            self.reverse_ticks = REVERSE_TICKS;
            self.slow_ticks = 0;
            self.recoveries += 1;
        }
        if self.reverse_ticks > 0 {
            self.reverse_ticks -= 1;
            return Input { brake: true, steer: -steer, ..Default::default() };
        }
        // no throttle near full lock at speed or while sliding: the car cannot hold a tight line
        // there (ours; below 15 m/s it may, or a car off the line on the grid never starts)
        Input { throttle: speed < target_speed && (steer.abs() < 0.7 || speed < 15.0) && !sliding, brake: speed > target_speed + 2.0, steer, ..Default::default() }
    }
}

/// Picks `n` drivers of a difficulty at random, without repeats while the pool lasts (whether
/// retail avoids repeats was not read).
pub fn pick_drivers(drivers: &[Driver], difficulty: u8, n: usize, rng: &mut Rng) -> Vec<Driver> {
    let mut pool: Vec<&Driver> = drivers.iter().filter(|d| d.difficulty == difficulty as u16).collect();
    if pool.is_empty() {
        pool = drivers.iter().collect();
    }
    let mut left = pool.clone();
    (0..n)
        .filter_map(|_| {
            if left.is_empty() {
                left = pool.clone();
            }
            (!left.is_empty()).then(|| left.remove(rng.below(left.len())).clone())
        })
        .collect()
}

/// Opponent cars (catalogue ids) by the menu's rule. Same era / class / both: retail draws `k`
/// over the whole list and takes the `k`-th match counting round the list, i.e.
/// `matches[k % matches]`; random: any car of the list. Falls back to the player's car when
/// nothing matches.
pub fn pick_cars(rule: &OpponentCars, player: &catalog::Car, list: &[catalog::Car], n: usize, rng: &mut Rng) -> Vec<u16> {
    let matches: Vec<&catalog::Car> = list
        .iter()
        .filter(|c| match rule {
            OpponentCars::SameEra => c.era == player.era,
            OpponentCars::SameClass => c.class == player.class,
            OpponentCars::SameEraAndClass => c.era == player.era && c.class == player.class,
            _ => true,
        })
        .collect();
    (0..n)
        .map(|_| match rule {
            OpponentCars::SameCar => player.id,
            OpponentCars::Car { id, .. } => *id,
            _ if matches.is_empty() => player.id,
            OpponentCars::Random => matches[rng.below(matches.len())].id,
            _ => matches[rng.below(list.len().max(1)) % matches.len()].id,
        })
        .collect()
}

/// The `.ais` file stem for a car's AI-data name (`nfs5.car`, e.g. `993T`): the file of that
/// name in any letter case, else `default`, as retail falls back to `default.ais`.
pub fn ais_stem(ai_name: &str, stems: &[String]) -> String {
    stems.iter().find(|s| !ai_name.is_empty() && s.eq_ignore_ascii_case(ai_name)).cloned().unwrap_or_else(|| "default".to_string())
}

/// The retail drivers (`FEData/Data/nfs5.per`) and their names by string id
/// (`FEData/Locale/festrings.loc`, English). Empty when missing.
pub fn load_drivers(game: &Path) -> (Vec<Driver>, std::collections::BTreeMap<u16, String>) {
    let fe = game.join("FEData");
    let drivers = std::fs::read(fe.join("Data").join("nfs5.per")).map(|d| aidata::parse_per(&d)).unwrap_or_default();
    let names = std::fs::read(fe.join("Locale").join("festrings.loc")).ok().and_then(|d| nfspu_formats::loc::parse(&d, 0)).unwrap_or_default();
    (drivers, names)
}

/// The `.ais` tables for a car's AI-data name (`GameData/Simulation/AICarData`, see `ais_stem`),
/// with the file stem used.
pub fn load_ais(game: &Path, ai_name: &str) -> Option<(AiSpec, String)> {
    let dir = game.join("GameData").join("Simulation").join("AICarData");
    let files: Vec<(String, PathBuf)> = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("ais")))
        .filter_map(|p| Some((p.file_stem()?.to_string_lossy().to_string(), p)))
        .collect();
    let stems: Vec<String> = files.iter().map(|f| f.0.clone()).collect();
    let stem = ais_stem(ai_name, &stems);
    let path = &files.iter().find(|f| f.0.eq_ignore_ascii_case(&stem))?.1;
    Some((aidata::parse_ais(&std::fs::read(path).ok()?)?, stem))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ground::{Ground, Tri};
    use crate::collision::WallSet;
    use nfspu_formats::sim::{SimData, Slice};

    fn spec() -> AiSpec {
        let mut lookahead = [0.0; 24];
        for (i, v) in lookahead.iter_mut().enumerate() {
            *v = 8.0 + i as f32;
        }
        let mut speeds = [0.0; 40];
        for (i, v) in speeds.iter_mut().enumerate() {
            *v = 60.0 - i as f32 * 1.2;
        }
        AiSpec { name: "test".into(), lookahead, speeds }
    }

    fn driver(skill: f32) -> Driver {
        Driver { id: 1, name_id: 18000, difficulty: 2, skills: [(skill, 0.0); 4] }
    }

    fn slice(x: f32, z: f32, fwd: [f32; 3]) -> Slice {
        Slice { pos: [x, 0.0, z], up: [0.0, 1.0, 0.0], forward: fwd, right: [fwd[2], 0.0, -fwd[0]], kind: 0, lanes: [1, 1], extra_widths: [5.0; 2], left_width: 7.0, right_width: 7.0 }
    }

    /// A rounded square loop (file space): 4 straights of 40 slices 6 m apart joined by
    /// quarter-circle corners of 10 slices.
    fn track() -> RoadPath {
        RoadPath::new(&track_sim())
    }

    fn track_sim() -> SimData {
        let mut s = Vec::new();
        let (mut x, mut z, mut a) = (0.0f32, 0.0f32, 0.0f32);
        for _ in 0..4 {
            for _ in 0..40 {
                let f = [a.sin(), 0.0, a.cos()];
                s.push(slice(x, z, f));
                x += 6.0 * f[0];
                z += 6.0 * f[2];
            }
            for _ in 0..10 {
                a += std::f32::consts::FRAC_PI_2 / 10.0;
                let f = [a.sin(), 0.0, a.cos()];
                s.push(slice(x, z, f));
                x += 6.0 * f[0];
                z += 6.0 * f[2];
            }
        }
        SimData { groups: vec![(0, s.len())], slices: s }
    }

    #[test]
    fn bends_and_table_speeds() {
        let p = track();
        let sp = spec();
        assert_eq!(lookahead_slices(&sp, 0.0), 8);
        assert_eq!(lookahead_slices(&sp, 30.0), 14);
        assert_eq!(lookahead_slices(&sp, 500.0), 31, "index capped at 23");
        // middle of a straight, looking 10 slices: no bend
        assert!(bend_degrees(&p, 10, 10, |s, k| p.walk(s, k, Direction::Forward)) < 0.5);
        // ahead of a corner: about 90 degrees once the whole corner is in view
        let b = bend_degrees(&p, 35, 20, |s, k| p.walk(s, k, Direction::Forward));
        assert!((b - 90.0).abs() < 2.0, "bend {b}");
        assert_eq!(stretch_for(b), Stretch::Slow);
        assert_eq!(stretch_for(45.0), Stretch::Medium);
        assert_eq!(stretch_for(5.0), Stretch::Fast);
        assert_eq!(table_speed(&sp, 0.0), 60.0);
        assert_eq!(table_speed(&sp, 90.0), 60.0 - 30.0 * 1.2);
        assert_eq!(table_speed(&sp, 400.0), 60.0 - 38.0 * 1.2, "bend capped at 115");
        // backwards the same corner is still about 90 degrees
        let b = bend_degrees(&p, 55, 20, |s, k| p.walk(s, k, Direction::Backward));
        assert!((b - 90.0).abs() < 2.0, "backward bend {b}");
    }

    #[test]
    fn skill_draws_stay_in_range() {
        let d = Driver { id: 1, name_id: 0, difficulty: 0, skills: [(0.5, 0.05), (0.6, 0.05), (0.7, 0.05), (0.4, 0.01)] };
        let mut rng = Rng::new(7);
        let (mut slow, mut default) = (0, 0);
        for _ in 0..2000 {
            let f = draw_skill(&d, Stretch::Slow, &mut rng);
            if (0.45..=0.55).contains(&f) {
                slow += 1;
            } else {
                assert!((0.39..=0.41).contains(&f), "factor {f}");
                default += 1;
            }
        }
        // the stretch value is used with probability 1 - 0.5
        assert!((800..1200).contains(&slow), "slow {slow} default {default}");
    }

    #[test]
    fn lead_slowdown_by_difficulty() {
        assert_eq!(lead_factor(0, 0.5, 100.0), 1.0, "beginner never");
        assert_eq!(lead_factor(1, 0.1, 100.0), 0.98);
        assert_eq!(lead_factor(1, 0.05, 100.0), 1.0);
        assert_eq!(lead_factor(2, 0.05, 100.0), 0.95);
        assert_eq!(lead_factor(2, 0.05, 20.0), 1.0, "only while the player keeps up");
        assert_eq!(lead_factor(2, -0.5, 100.0), 1.0, "no catch-up boost");
        // one lap in 120 s
        assert!((player_rate(1.0, 120 * 64) - 83.33).abs() < 0.1);
    }

    #[test]
    fn drives_round_the_loop_without_help() {
        let p = track();
        let line = racing_line(&p, None);
        let tris: Vec<Tri> = vec![[[-2000.0, 0.0, -2000.0], [2000.0, 0.0, -2000.0], [-2000.0, 0.0, 2000.0]], [[2000.0, 0.0, -2000.0], [2000.0, 0.0, 2000.0], [-2000.0, 0.0, 2000.0]]];
        let ground = Ground::new(tris, 16.0);
        let walls = WallSet::new(vec![], 4.0);
        let start = 5;
        let (pos, yaw) = reset_pose(&p, &line, start, Direction::Forward);
        let mut car = Car::new(pos, yaw);
        let mut ai = Opponent::new(spec(), driver(0.9), start);
        let mut rng = Rng::new(3);
        let mut visited = vec![false; p.slices.len()];
        let mut worst = 0.0f32;
        for _ in 0..64 * 60 {
            let routes = Routes::build(&track_sim(), None);
            let v = View { path: &p, routes: &routes, line: &line, dir: Direction::Forward, held: false, finished: false, difficulty: 2, lead: 0.0, player_rate: 0.0 };
            let input = ai.think(&car, &v, &[], &mut rng);
            car.step(input, 1.0 / 64.0, &ground, &walls);
            visited[ai.slice] = true;
            let s = &p.slices[ai.slice];
            let side = (car.pos - s.pos).dot(s.right).abs();
            worst = worst.max(side);
        }
        assert!(visited.iter().all(|v| *v), "went all the way round");
        assert!(worst < 7.0, "stayed on the 14 m road, worst {worst:.1} m off centre");
        assert_eq!(ai.recoveries, 0);
    }

    #[test]
    fn driving_line_stays_on_the_road() {
        let p = track();
        // file space: slice 5 is at x 0, z 30 heading +z; its right in Bevy space is -x
        let mut lsp: Vec<[f32; 2]> = p.slices.iter().map(|s| [s.pos.x * crate::world_mirror_x(), s.pos.z]).collect();
        lsp[5][0] += 3.0;
        lsp[6][0] += 30.0;
        let raw = racing_line(&p, Some(&lsp));
        assert!((raw[6] - p.slices[6].pos).length() > 29.0);
        let line = driving_line(&p, Some(&lsp));
        let side = |i: usize| (line[i] - p.slices[i].pos).dot(p.slices[i].right);
        assert!((side(5).abs() - 3.0).abs() < 1e-3, "inside the road: kept, {}", side(5));
        assert!((side(6).abs() - 6.0).abs() < 1e-3, "outside: 1 m inside the 7 m half-width, {}", side(6));
        assert_eq!(driving_line(&p, Some(&lsp[..10])), p.slices.iter().map(|s| s.pos).collect::<Vec<_>>(), "wrong length: centres");
    }

    #[test]
    fn held_cars_do_nothing() {
        let p = track();
        let line = racing_line(&p, None);
        let mut ai = Opponent::new(spec(), driver(0.9), 5);
        let car = Car::new(p.slices[5].pos, 0.0);
        let routes = Routes::build(&track_sim(), None);
        let v = View { path: &p, routes: &routes, line: &line, dir: Direction::Forward, held: true, finished: false, difficulty: 2, lead: 0.0, player_rate: 0.0 };
        let i = ai.think(&car, &v, &[], &mut Rng::new(1));
        assert!(!i.throttle && !i.brake && i.steer == 0.0);
    }

    #[test]
    fn drivers_come_from_the_chosen_difficulty() {
        let mut ds = Vec::new();
        for i in 0..10u16 {
            ds.push(Driver { id: i, name_id: 18000 + i as u32, difficulty: i % 3, skills: [(0.5, 0.0); 4] });
        }
        let mut rng = Rng::new(11);
        let picked = pick_drivers(&ds, 1, 3, &mut rng);
        assert_eq!(picked.len(), 3);
        assert!(picked.iter().all(|d| d.difficulty == 1));
        let mut ids: Vec<u16> = picked.iter().map(|d| d.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 3, "no repeats while the pool lasts");
        assert_eq!(pick_drivers(&ds, 0, 7, &mut rng).len(), 7, "pool of 4 reused");
    }

    #[test]
    fn cars_follow_the_rule() {
        let car = |id: u16, era: u32, class: u32| catalog::Car { id, era, class, ..Default::default() };
        let list = vec![car(1, 521, 503), car(2, 521, 501), car(3, 522, 503), car(4, 523, 501), car(5, 523, 501)];
        let player = car(1, 521, 503);
        let mut rng = Rng::new(5);
        assert!(pick_cars(&OpponentCars::SameCar, &player, &list, 4, &mut rng).iter().all(|&c| c == 1));
        assert!(pick_cars(&OpponentCars::SameEra, &player, &list, 20, &mut rng).iter().all(|&c| c == 1 || c == 2));
        assert!(pick_cars(&OpponentCars::SameClass, &player, &list, 20, &mut rng).iter().all(|&c| c == 1 || c == 3));
        assert!(pick_cars(&OpponentCars::SameEraAndClass, &player, &list, 5, &mut rng).iter().all(|&c| c == 1));
        let any = pick_cars(&OpponentCars::Random, &player, &list, 50, &mut rng);
        assert!(any.contains(&5) && any.contains(&2));
        let one = OpponentCars::Car { id: 4, model: "993".into() };
        assert_eq!(pick_cars(&one, &player, &list, 2, &mut rng), vec![4, 4]);
    }

    #[test]
    fn ai_names_pick_ais_files() {
        let stems: Vec<String> = ["356", "993", "993t", "Gt1", "default"].iter().map(|s| s.to_string()).collect();
        assert_eq!(ais_stem("993T", &stems), "993t");
        assert_eq!(ais_stem("993", &stems), "993");
        assert_eq!(ais_stem("GT1", &stems), "Gt1");
        assert_eq!(ais_stem("", &stems), "default", "traffic cars have no AI name");
        assert_eq!(ais_stem("550A", &stems), "default", "missing file");
    }

    fn straight(left: f32, right: f32) -> crate::race_path::PathSlice {
        crate::race_path::PathSlice { pos: Vec3::ZERO, forward: Vec3::NEG_Z, right: Vec3::X, up: Vec3::Y, left_width: left, right_width: right }
    }

    #[test]
    fn avoid_passes_on_the_open_side_and_brakes_when_boxed_in() {
        let mut car = Car::new(Vec3::ZERO, 0.0);
        car.set_speed(25.0);
        let slow = Other { pos: Vec3::new(0.5, 0.0, -30.0), speed: 12.0, heading: Vec3::NEG_Z };
        // wide road, more room on the left of the slow car: aim left, no hard braking
        let (shift, cap) = avoid(&car, &straight(8.0, 3.0), &[slow]);
        assert!(shift.x < -1.0, "shift {shift:?}");
        assert!(cap > 20.0, "cap {cap}");
        // narrow road: no gap, slow down towards its speed
        let (_, cap) = avoid(&car, &straight(2.5, 2.5), &[slow]);
        assert!(cap < 25.0, "cap {cap}");
        // oncoming in our path is seen from further away (both speeds add up)
        let oncoming = Other { pos: Vec3::new(0.0, 0.0, -70.0), speed: 15.0, heading: Vec3::Z };
        let (shift, _) = avoid(&car, &straight(5.0, 5.0), &[oncoming]);
        assert!(shift.length() > 0.5, "shift {shift:?}");
        // a car behind or well to the side is ignored
        let behind = Other { pos: Vec3::new(0.0, 0.0, 10.0), speed: 30.0, heading: Vec3::NEG_Z };
        assert_eq!(avoid(&car, &straight(5.0, 5.0), &[behind]).0, Vec3::ZERO);
    }
}
