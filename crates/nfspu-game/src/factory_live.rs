//! The hand-over between the front end and a race for a Factory Driver mission: the menu names the
//! mission before the race starts, the race plays it and leaves the verdict here for the menu.
//! Two process-wide slots, like `knockout::set_pending`.

use crate::factory::{Mission, Outcome, Scene, Status};
use std::sync::Mutex;

/// A mission about to be driven (or being driven).
#[derive(Debug, Clone)]
pub struct Active {
    pub mission: Mission,
    pub scene: Option<Scene>,
}

impl Active {
    /// A mission judged by the race itself (place, time, damage) rather than by gates: the race
    /// missions with opponents and the timed laps on a circuit.
    pub fn is_race(&self) -> bool {
        self.mission.min_place.is_some() || self.scene.is_none()
    }
}

/// What a finished mission left behind.
#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    pub index: usize,
    pub status: Status,
    pub outcome: Outcome,
}

static ACTIVE: Mutex<Option<Active>> = Mutex::new(None);
static VERDICT: Mutex<Option<Verdict>> = Mutex::new(None);

pub fn set_active(a: Option<Active>) {
    *ACTIVE.lock().unwrap() = a;
}

/// The mission the race in progress is for.
pub fn active() -> Option<Active> {
    ACTIVE.lock().unwrap().clone()
}

/// Whether a gate mission (a scene the car is driven through, off the road's rules) is running.
pub fn gate_mission_active() -> bool {
    ACTIVE.lock().unwrap().as_ref().is_some_and(|a| !a.is_race())
}

pub fn set_verdict(v: Verdict) {
    *VERDICT.lock().unwrap() = Some(v);
}

/// The verdict of the mission just driven, once (the menu settles the profile with it).
pub fn take_verdict() -> Option<Verdict> {
    VERDICT.lock().unwrap().take()
}
