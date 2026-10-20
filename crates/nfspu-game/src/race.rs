//! Race rules, from the grid to the results: countdown, cars held until GO, laps, positions,
//! race clock, lap and best-lap times, finish. Pure logic (no Bevy systems); the game feeds it one
//! 64 Hz tick at a time with each car's lap fraction.
//!
//! Retail behaviour this follows (from the decomp, `docs/provenance/race.md`):
//! - The race clock counts sim ticks at 64 per second from 0 when the race loads. The countdown
//!   shows "3" from tick 256, "2" from 320, "1" from 384, and the race starts at tick 448 (7 s);
//!   a countdown sound event fires at each of those four ticks. Cars are held until 448.
//! - Times are measured in ticks from 448 and shown as `mm:ss.hh` with hundredths = ticks % 64 *
//!   100 / 64.
//! - Circuit laps: a car's progress is `laps + fraction - owed`. When the fraction wraps from over
//!   0.5 to under 0.1 the car crossed the line forwards: it first pays back any lap it owes
//!   (from crossing backwards), otherwise its lap count goes up. A wrap from under 0.1 to over 0.5
//!   is a backwards crossing and adds an owed lap. A new lap count records the lap time; reaching
//!   the set number of laps finishes the car.
//! - Point-to-point roads finish when the car crosses the finish trigger line.

/// Sim ticks per second.
pub const TICK_HZ: u32 = 64;
/// Tick at which the "3" appears (the first countdown event).
pub const COUNTDOWN_FIRST: u32 = 256;
/// Ticks between countdown steps.
pub const COUNTDOWN_STEP: u32 = 64;
/// Tick at which the race starts and cars are released.
pub const GO_TICK: u32 = 448;
/// How long "GO" is reported after the start: none. The 3-2-1 overlay only runs up to tick 448,
/// and the reference let's play (10:26-10:27) shows the "1" vanish and the HUD appear with no GO
/// graphic. `RaceEvent::Countdown(0)` still fires at 448 (the fourth countdown sound event).
pub const GO_SHOWN_TICKS: u32 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Circuit,
    PointToPoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RaceSetup {
    pub kind: Kind,
    /// Laps to race (forced to 1 on point-to-point roads).
    pub laps: u32,
    pub cars: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CarState {
    Racing,
    Finished,
}

/// What the game tells the race about one car each tick.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CarSample {
    /// Lap fraction 0..1 of the car's slice, already turned for the race direction.
    pub fraction: f32,
    /// True if the car crossed the finish trigger line this tick (point-to-point only).
    pub crossed_finish: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CarRace {
    pub state: CarState,
    /// Laps completed.
    pub laps: u32,
    /// Laps owed after crossing the line backwards.
    pub owed: u32,
    pub fraction: f32,
    /// Total progress in laps (`laps + fraction - owed`), used for the running order.
    pub progress: f32,
    /// Tick the current lap started.
    pub lap_start: u32,
    pub lap_times: Vec<u32>,
    /// Race time at the finish, in ticks from GO.
    pub finish_time: Option<u32>,
    /// Finishing position (1-based).
    pub finish_position: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaceEvent {
    /// A countdown step: 3, 2, 1, then 0 for GO.
    Countdown(u8),
    /// A lap finished: car, lap number (1-based), lap time in ticks.
    Lap { car: usize, lap: u32, ticks: u32 },
    /// A car crossed the line backwards and now owes a lap.
    LapOwed { car: usize },
    Finished { car: usize, position: usize, ticks: u32 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResultRow {
    pub position: usize,
    pub car: usize,
    /// Race time in ticks, None if the car has not finished.
    pub time: Option<u32>,
    pub best_lap: Option<u32>,
    pub laps: u32,
}

pub struct Race {
    pub setup: RaceSetup,
    /// Sim ticks since the race loaded.
    pub tick: u32,
    pub cars: Vec<CarRace>,
    finished: usize,
}

/// Plain race numbers for the HUD, updated every frame while a race runs (all zero / None when
/// no race is running). Times are in seconds; use `format_secs` for the retail `mm:ss.hh` text.
#[derive(bevy::prelude::Resource, Debug, Clone, Default, PartialEq)]
pub struct RaceStatus {
    /// True while a race is set up (from the grid to the results).
    pub active: bool,
    pub track: String,
    /// true = backwards direction.
    pub backwards: bool,
    pub point_to_point: bool,
    /// Lap the player is on, 1-based (capped at `total_laps`).
    pub current_lap: u32,
    pub total_laps: u32,
    /// Player's running position, 1-based.
    pub position: usize,
    pub cars: usize,
    /// Race clock since GO (frozen when the player finishes).
    pub race_time: f32,
    /// Time of the player's current lap (frozen when the player finishes).
    pub lap_time: f32,
    pub best_lap: Option<f32>,
    /// Completed lap times of the player.
    pub lap_times: Vec<f32>,
    /// 3, 2, 1 during the countdown, None otherwise (retail shows no GO graphic).
    pub countdown: Option<u8>,
    /// Cars are held on the grid (before GO).
    pub held: bool,
    /// The player is driving against the race direction.
    pub wrong_way: bool,
    /// The player has finished.
    pub finished: bool,
    /// Player's progress through the race, 0..1 overall (laps included).
    pub progress: f32,
    pub speed_kmh: f32,
    /// Engine speed; the placeholder car has no engine model yet.
    pub rpm: Option<f32>,
    /// Final standings once the player has finished (position order).
    pub results: Vec<ResultLine>,
    /// Sim ticks (64 Hz) since the race loaded.
    pub tick: u32,
    /// Name of each car by race index ("You" for the player, the driver's name for opponents).
    pub names: Vec<String>,
    /// Car indices in running order, leader first (for the HUD's opponent list).
    pub order: Vec<usize>,
    /// Each car's race progress in laps (0..laps), by race index.
    pub car_progress: Vec<f32>,
    /// Each car's ground position (x, z), yaw and speed (m/s) by race index, for the HUD (set by the race runner).
    pub car_places: Vec<[f32; 4]>,
    /// Each car's body colour by race index (set by the race runner).
    pub car_colours: Vec<[u8; 3]>,
    /// The road's centre lines as (x, z) polylines, one per road run (set once per race).
    pub road: std::sync::Arc<Vec<Vec<[f32; 2]>>>,
    /// Length of one lap in metres.
    pub lap_metres: f32,
}

/// One line of the results table.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResultLine {
    pub position: usize,
    pub car: usize,
    pub is_player: bool,
    /// Total race time, None if the car has not finished.
    pub time: Option<f32>,
    pub best_lap: Option<f32>,
}

/// `mm:ss.hh` for a time in seconds (the HUD's form of `format_time`).
pub fn format_secs(secs: f32) -> String {
    format_time((secs.max(0.0) * TICK_HZ as f32).round() as u32)
}

/// `mm:ss.hh` as the retail game prints race times.
pub fn format_time(ticks: u32) -> String {
    format!("{:02}:{:02}.{:02}", ticks / (TICK_HZ * 60), (ticks / TICK_HZ) % 60, (ticks % TICK_HZ) * 100 / TICK_HZ)
}

pub fn ticks_to_secs(ticks: u32) -> f32 {
    ticks as f32 / TICK_HZ as f32
}

impl Race {
    pub fn new(mut setup: RaceSetup, start_fractions: &[f32]) -> Self {
        if setup.kind == Kind::PointToPoint || setup.laps == 0 {
            setup.laps = 1;
        }
        let cars = (0..setup.cars)
            .map(|i| {
                let f = start_fractions.get(i).copied().unwrap_or(0.0);
                CarRace { state: CarState::Racing, laps: 0, owed: 0, fraction: f, progress: f, lap_start: GO_TICK, lap_times: Vec::new(), finish_time: None, finish_position: None }
            })
            .collect();
        Race { setup, tick: 0, cars, finished: 0 }
    }

    /// 3, 2, 1 during the countdown, else None (see `GO_SHOWN_TICKS`).
    pub fn countdown(&self) -> Option<u8> {
        let t = self.tick;
        if t < COUNTDOWN_FIRST {
            None
        } else if t < GO_TICK {
            Some(3 - ((t - COUNTDOWN_FIRST) / COUNTDOWN_STEP) as u8)
        } else if t < GO_TICK + GO_SHOWN_TICKS {
            Some(0)
        } else {
            None
        }
    }

    /// Cars may not move before GO.
    pub fn held(&self) -> bool {
        self.tick < GO_TICK
    }

    /// Race clock in ticks (0 before GO).
    pub fn race_ticks(&self) -> u32 {
        self.tick.saturating_sub(GO_TICK)
    }

    /// Time of a car's current lap (frozen at the finish).
    pub fn current_lap_ticks(&self, car: usize) -> u32 {
        let c = &self.cars[car];
        if c.state == CarState::Finished {
            return c.lap_times.last().copied().unwrap_or(0);
        }
        self.tick.saturating_sub(c.lap_start)
    }

    /// Race time of one car (frozen at its finish).
    pub fn car_time(&self, car: usize) -> u32 {
        self.cars[car].finish_time.unwrap_or_else(|| self.race_ticks())
    }

    pub fn best_lap(&self, car: usize) -> Option<u32> {
        self.cars[car].lap_times.iter().copied().min()
    }

    /// The lap a car is on (1-based, capped at the race's laps).
    pub fn current_lap(&self, car: usize) -> u32 {
        (self.cars[car].laps + 1).min(self.setup.laps)
    }

    pub fn all_finished(&self) -> bool {
        self.finished == self.cars.len()
    }

    /// Car indices in running order: finished cars by finishing position, then by progress.
    pub fn order(&self) -> Vec<usize> {
        let mut idx: Vec<usize> = (0..self.cars.len()).collect();
        idx.sort_by(|&a, &b| {
            let (ca, cb) = (&self.cars[a], &self.cars[b]);
            match (ca.finish_position, cb.finish_position) {
                (Some(x), Some(y)) => x.cmp(&y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => cb.progress.total_cmp(&ca.progress).then(a.cmp(&b)),
            }
        });
        idx
    }

    /// 1-based running position of a car.
    pub fn position(&self, car: usize) -> usize {
        self.order().iter().position(|&c| c == car).map_or(0, |p| p + 1)
    }

    /// Fills the HUD numbers for car `player` (speed is set by the caller).
    pub fn fill_status(&self, player: usize, st: &mut RaceStatus) {
        let c = &self.cars[player];
        st.active = true;
        st.point_to_point = self.setup.kind == Kind::PointToPoint;
        st.current_lap = self.current_lap(player);
        st.total_laps = self.setup.laps;
        st.position = self.position(player);
        st.cars = self.cars.len();
        st.race_time = ticks_to_secs(self.car_time(player));
        st.lap_time = ticks_to_secs(self.current_lap_ticks(player));
        st.best_lap = self.best_lap(player).map(ticks_to_secs);
        st.lap_times = c.lap_times.iter().map(|&t| ticks_to_secs(t)).collect();
        st.countdown = self.countdown();
        st.held = self.held();
        st.finished = c.state == CarState::Finished;
        st.progress = (c.progress / self.setup.laps as f32).clamp(0.0, 1.0);
        st.tick = self.tick;
        st.order = self.order();
        st.car_progress = self.cars.iter().map(|c| c.progress).collect();
        st.results = if st.finished {
            self.results()
                .into_iter()
                .map(|r| ResultLine { position: r.position, car: r.car, is_player: r.car == player, time: r.time.map(ticks_to_secs), best_lap: r.best_lap.map(ticks_to_secs) })
                .collect()
        } else {
            Vec::new()
        };
    }

    pub fn results(&self) -> Vec<ResultRow> {
        self.order()
            .into_iter()
            .enumerate()
            .map(|(i, car)| ResultRow { position: i + 1, car, time: self.cars[car].finish_time, best_lap: self.best_lap(car), laps: self.cars[car].laps })
            .collect()
    }

    fn finish(&mut self, car: usize, events: &mut Vec<RaceEvent>) {
        self.finished += 1;
        let ticks = self.race_ticks();
        let c = &mut self.cars[car];
        c.state = CarState::Finished;
        c.finish_time = Some(ticks);
        c.finish_position = Some(self.finished);
        events.push(RaceEvent::Finished { car, position: self.finished, ticks });
    }

    /// Advances one 64 Hz tick. `samples` has one entry per car.
    pub fn step(&mut self, samples: &[CarSample]) -> Vec<RaceEvent> {
        let mut events = Vec::new();
        self.tick += 1;
        let t = self.tick;
        if (COUNTDOWN_FIRST..=GO_TICK).contains(&t) && (t - COUNTDOWN_FIRST) % COUNTDOWN_STEP == 0 {
            events.push(RaceEvent::Countdown(3 - ((t - COUNTDOWN_FIRST) / COUNTDOWN_STEP) as u8));
        }
        for car in 0..self.cars.len().min(samples.len()) {
            let s = samples[car];
            if self.cars[car].state != CarState::Racing {
                continue;
            }
            let racing = t > GO_TICK;
            let c = &mut self.cars[car];
            let (old, new) = (c.fraction, s.fraction);
            c.fraction = new;
            let mut lap_done = false;
            if racing && self.setup.kind == Kind::Circuit {
                if old > 0.5 && new < 0.1 {
                    if c.owed > 0 {
                        c.owed -= 1;
                    } else {
                        c.laps += 1;
                        lap_done = true;
                    }
                } else if old < 0.1 && new > 0.5 {
                    c.owed += 1;
                    events.push(RaceEvent::LapOwed { car });
                }
            }
            if racing && self.setup.kind == Kind::PointToPoint && s.crossed_finish {
                c.laps = 1;
                lap_done = true;
            }
            c.progress = c.laps as f32 + new - c.owed as f32;
            if lap_done {
                let ticks = t - c.lap_start;
                c.lap_times.push(ticks);
                c.lap_start = t;
                events.push(RaceEvent::Lap { car, lap: c.laps, ticks });
                if c.laps >= self.setup.laps {
                    self.finish(car, &mut events);
                }
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn circuit(laps: u32, cars: usize) -> Race {
        Race::new(RaceSetup { kind: Kind::Circuit, laps, cars }, &vec![0.02; cars])
    }

    fn run_to(r: &mut Race, tick: u32, f: f32) -> Vec<RaceEvent> {
        let mut ev = Vec::new();
        while r.tick < tick {
            ev.extend(r.step(&vec![CarSample { fraction: f, crossed_finish: false }; r.cars.len()]));
        }
        ev
    }

    #[test]
    fn countdown_follows_retail_ticks() {
        let mut r = circuit(2, 1);
        assert_eq!(r.countdown(), None);
        assert!(r.held());
        let ev = run_to(&mut r, 256, 0.02);
        assert_eq!(ev, vec![RaceEvent::Countdown(3)]);
        assert_eq!(r.countdown(), Some(3));
        run_to(&mut r, 319, 0.02);
        assert_eq!(r.countdown(), Some(3));
        assert_eq!(run_to(&mut r, 320, 0.02), vec![RaceEvent::Countdown(2)]);
        assert_eq!(run_to(&mut r, 384, 0.02), vec![RaceEvent::Countdown(1)]);
        assert_eq!(r.countdown(), Some(1));
        assert!(r.held());
        assert_eq!(run_to(&mut r, 448, 0.02), vec![RaceEvent::Countdown(0)]);
        assert!(!r.held());
        assert_eq!(r.countdown(), None, "retail shows no GO graphic");
        assert_eq!(r.race_ticks(), 0);
        run_to(&mut r, 448 + 64 + 64, 0.02);
        assert_eq!(r.countdown(), None);
        assert_eq!(r.race_ticks(), 128);
        assert_eq!(ticks_to_secs(r.race_ticks()), 2.0);
    }

    #[test]
    fn laps_count_on_forward_wraps_and_finish() {
        let mut r = circuit(2, 1);
        run_to(&mut r, 448, 0.02);
        run_to(&mut r, 700, 0.3);
        run_to(&mut r, 1000, 0.6);
        run_to(&mut r, 2000, 0.95);
        let ev = run_to(&mut r, 2001, 0.03);
        assert_eq!(ev, vec![RaceEvent::Lap { car: 0, lap: 1, ticks: 2001 - 448 }]);
        assert_eq!(r.current_lap(0), 2);
        run_to(&mut r, 2500, 0.3);
        run_to(&mut r, 3000, 0.7);
        let ev = run_to(&mut r, 3001, 0.01);
        assert_eq!(ev, vec![RaceEvent::Lap { car: 0, lap: 2, ticks: 1000 }, RaceEvent::Finished { car: 0, position: 1, ticks: 3001 - 448 }]);
        assert!(r.all_finished());
        assert_eq!(r.best_lap(0), Some(1000));
        assert_eq!(r.car_time(0), 2553);
        // the clock stops for the finished car
        run_to(&mut r, 4000, 0.2);
        assert_eq!(r.car_time(0), 2553);
        assert_eq!(r.current_lap_ticks(0), 1000);
        assert_eq!(r.cars[0].laps, 2);
    }

    #[test]
    fn backwards_crossing_owes_a_lap() {
        let mut r = circuit(3, 1);
        run_to(&mut r, 500, 0.05);
        // reverse over the line: no lap, one owed
        assert_eq!(run_to(&mut r, 501, 0.97), vec![RaceEvent::LapOwed { car: 0 }]);
        assert!(r.cars[0].progress < 0.0);
        // forward again over the line pays it back without counting a lap
        assert!(run_to(&mut r, 502, 0.04).is_empty());
        assert_eq!(r.cars[0].laps, 0);
        assert_eq!(r.cars[0].owed, 0);
    }

    #[test]
    fn nothing_counts_before_go() {
        let mut r = circuit(1, 1);
        run_to(&mut r, 300, 0.9);
        run_to(&mut r, 301, 0.02);
        assert_eq!(r.cars[0].laps, 0);
    }

    #[test]
    fn point_to_point_finishes_on_the_trigger() {
        let mut r = Race::new(RaceSetup { kind: Kind::PointToPoint, laps: 3, cars: 2 }, &[0.01, 0.012]);
        assert_eq!(r.setup.laps, 1);
        run_to(&mut r, 1000, 0.5);
        let mut s = vec![CarSample { fraction: 0.99, crossed_finish: false }; 2];
        s[1].crossed_finish = true;
        let ev = r.step(&s);
        assert_eq!(ev, vec![RaceEvent::Lap { car: 1, lap: 1, ticks: 1001 - 448 }, RaceEvent::Finished { car: 1, position: 1, ticks: 553 }]);
        assert_eq!(r.position(1), 1);
        assert_eq!(r.position(0), 2);
        let res = r.results();
        assert_eq!(res[0], ResultRow { position: 1, car: 1, time: Some(553), best_lap: Some(553), laps: 1 });
        assert_eq!(res[1].time, None);
    }

    #[test]
    fn order_by_progress() {
        let mut r = circuit(3, 3);
        run_to(&mut r, 500, 0.3);
        r.step(&[CarSample { fraction: 0.31, ..Default::default() }, CarSample { fraction: 0.5, ..Default::default() }, CarSample { fraction: 0.2, ..Default::default() }]);
        assert_eq!(r.order(), vec![1, 0, 2]);
    }

    #[test]
    fn retail_time_format() {
        assert_eq!(format_time(0), "00:00.00");
        assert_eq!(format_time(64 * 83 + 32), "01:23.50");
        assert_eq!(format_time(63), "00:00.98");
    }
}
