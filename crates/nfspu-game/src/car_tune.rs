//! The car setup screen's sliders (`carsetup.lay`): nine settings, each 0..=100 on a slider
//! (the control's `scrollmax` is 100, its `scrollpos` starts at 50), and what they do to the car.
//!
//! **Verified**: the accessor names and their order on the screen (`RideHeight`, `ShockRate`,
//! `Suspension`, `SteeringToe`, `BrakeBias`, `FDownForce`/`RDownForce`, `FTirePressure`/
//! `RTirePressure`), the slider range and the default of 50; the footage shows downforce at 50/50
//! and tyre pressure at 30/30 on a stock car. **Guess**: every effect below (retail's constants
//! are not decoded); they are small, centred so the defaults leave the stock car unchanged.
//! Our saved setups live in `settings/car_tune/<car>.txt` (never in the install).

use crate::car_sim::Spec;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The screen's accessors, in the order of [`Tune::values`].
pub const ACCESSORS: [&str; 9] = ["RideHeight", "ShockRate", "Suspension", "SteeringToe", "BrakeBias", "FDownForce", "RDownForce", "FTirePressure", "RTirePressure"];
pub const MAX: u32 = 100;

/// Slider positions 0..=100 by [`ACCESSORS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tune {
    pub values: [u32; 9],
}

impl Default for Tune {
    fn default() -> Self {
        Tune { values: [50, 50, 50, 50, 50, 50, 50, 30, 30] }
    }
}

impl Tune {
    pub fn index(accessor: &str) -> Option<usize> {
        ACCESSORS.iter().position(|a| a.eq_ignore_ascii_case(accessor))
    }

    pub fn get(&self, accessor: &str) -> Option<u32> {
        Self::index(accessor).map(|i| self.values[i])
    }

    /// Sets a slider (clamped to 0..=100); false for an unknown accessor.
    pub fn set(&mut self, accessor: &str, v: u32) -> bool {
        let Some(i) = Self::index(accessor) else { return false };
        self.values[i] = v.min(MAX);
        true
    }

    /// Moves a slider by a button's step (the buttons step by 1 or -1; we move 5 per click so the
    /// range is reachable in a few clicks, **guess**).
    pub fn step(&mut self, accessor: &str, step: i32) -> bool {
        let Some(i) = Self::index(accessor) else { return false };
        self.values[i] = (self.values[i] as i32 + step * 5).clamp(0, MAX as i32) as u32;
        true
    }

    /// Text form of the saved file.
    pub fn to_text(&self) -> String {
        let mut s = String::from("# NFSPU-Rust car setup, sliders 0-100\n");
        for (a, v) in ACCESSORS.iter().zip(self.values) {
            s.push_str(&format!("{a} = {v}\n"));
        }
        s
    }

    pub fn parse(text: &str) -> Tune {
        let mut t = Tune::default();
        for line in text.lines() {
            if let Some((k, v)) = line.split_once('=') {
                if let Ok(v) = v.trim().parse() {
                    t.set(k.trim(), v);
                }
            }
        }
        t
    }

    pub fn dir() -> PathBuf {
        std::env::var_os("NFSPU_SETTINGS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("settings"))
            .join("car_tune")
    }

    pub fn path_in(dir: &Path, car: &str) -> PathBuf {
        let safe: String = car.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
        dir.join(format!("{safe}.txt"))
    }

    pub fn save_to(&self, dir: &Path, car: &str) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        std::fs::write(Self::path_in(dir, car), self.to_text())
    }

    pub fn load_from(dir: &Path, car: &str) -> Option<Tune> {
        std::fs::read_to_string(Self::path_in(dir, car)).ok().map(|t| Tune::parse(&t))
    }

    /// The car's simulation constants with the setup applied (guesses, centred on the defaults).
    pub fn apply(&self, mut spec: Spec) -> Spec {
        if *self == Tune::default() {
            return spec;
        }
        let v = |a: &str| self.get(a).unwrap_or(50) as f32;
        // 0..100 -> 0.8..1.2 (50 = 1.0)
        let around = |x: f32| 0.8 + 0.4 * x / 100.0;
        // ride height: a lower car shifts less weight sideways and lengthways
        spec.transfer_height *= around(v("RideHeight"));
        // stiffness: stiffer springs shift weight faster and lean less
        spec.transfer_stiffness *= around(v("ShockRate"));
        spec.lean_spring /= around(v("ShockRate"));
        // travel: more travel, more lean
        spec.lean_spring *= around(v("Suspension"));
        spec.lean_damper *= around(v("Suspension"));
        // toe-in: up to 5 steering units either way
        spec.toe += (v("SteeringToe") - 50.0) * 0.1;
        // brake balance: 0 rear .. 100 front, 15 % of the braking either way
        spec.front_brake = (spec.front_brake + (v("BrakeBias") - 50.0) * 0.003).clamp(0.2, 0.9);
        // downforce: more load at speed, a little more drag
        let (f, r) = (v("FDownForce"), v("RDownForce"));
        spec.downforce_front *= 0.5 + f / 100.0;
        spec.downforce_rear *= 0.5 + r / 100.0;
        spec.drag *= 1.0 + 0.1 * ((f + r) / 2.0 - 50.0) / 50.0;
        // tyre pressure: grip peaks near 60 (+4 %), -12 % at 0, stock at the default 30
        let p = (v("FTirePressure") + v("RTirePressure")) / 2.0;
        spec.grip *= 1.0 + 0.04 * (1.0 - ((p - 60.0) / 30.0).powi(2));
        spec
    }
}

static CURRENT: Mutex<Option<Tune>> = Mutex::new(None);

/// The setup the next race's car uses (set by the car setup screen; `None` = stock).
pub fn set_current(t: Option<Tune>) {
    *CURRENT.lock().unwrap() = t;
}

pub fn current() -> Option<Tune> {
    *CURRENT.lock().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_leave_the_car_alone() {
        let s = Spec::generic();
        assert_eq!(Tune::default().apply(s.clone()), s);
    }

    #[test]
    fn sliders_move_and_clamp() {
        let mut t = Tune::default();
        assert!(t.step("BrakeBias", 1));
        assert_eq!(t.get("brakebias"), Some(55));
        t.set("RideHeight", 400);
        assert_eq!(t.get("RideHeight"), Some(100));
        assert!(!t.set("Nope", 1));
        for _ in 0..30 {
            t.step("RideHeight", -1);
        }
        assert_eq!(t.get("RideHeight"), Some(0));
    }

    #[test]
    fn text_round_trips_and_files_save() {
        let mut t = Tune::default();
        t.set("SteeringToe", 70);
        t.set("RTirePressure", 41);
        assert_eq!(Tune::parse(&t.to_text()), t);
        let dir = std::env::temp_dir().join(format!("nfspu_tune_{}", std::process::id()));
        t.save_to(&dir, "97 Boxster").unwrap();
        assert_eq!(Tune::load_from(&dir, "97 Boxster"), Some(t));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn each_slider_changes_the_car_the_expected_way() {
        let base = Spec::generic();
        let with = |a: &str, v: u32| {
            let mut t = Tune::default();
            t.set(a, v);
            t.apply(base.clone())
        };
        assert!(with("BrakeBias", 90).front_brake > base.front_brake);
        assert!(with("BrakeBias", 10).front_brake < base.front_brake);
        assert!(with("FDownForce", 100).downforce_front > base.downforce_front);
        assert!(with("RDownForce", 100).drag > base.drag);
        assert!(with("ShockRate", 100).transfer_stiffness > base.transfer_stiffness);
        assert!(with("SteeringToe", 100).toe > base.toe);
        assert!(with("FTirePressure", 60).grip > base.grip && with("FTirePressure", 0).grip < base.grip);
    }
}
