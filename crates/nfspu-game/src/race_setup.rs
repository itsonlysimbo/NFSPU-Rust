//! What the quick-race menu hands to the race: a plain value with every choice the player made,
//! written as `key = value` lines so it can be passed through a file (`logs/race_setup.txt`) and
//! read back by the loading screen and the race.
//!
//! The fields follow the retail quick-race screens (`QuickRace.lay`, `location.lay`,
//! `settings.lay`); see `docs/provenance/fe.md`, "Quick race".

use std::fmt::Write as _;

/// Track direction (`Track1.Direction`; strings 410-413).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Forward,
    Backward,
    ForwardMirrored,
    BackwardMirrored,
}

impl Direction {
    pub const ALL: [Direction; 4] = [Direction::Forward, Direction::Backward, Direction::ForwardMirrored, Direction::BackwardMirrored];

    /// The dropdown index (0..=3), clamped.
    pub fn from_index(i: usize) -> Direction {
        Direction::ALL[i.min(3)]
    }

    pub fn index(self) -> usize {
        Direction::ALL.iter().position(|d| *d == self).unwrap_or(0)
    }

    pub fn backward(self) -> bool {
        matches!(self, Direction::Backward | Direction::BackwardMirrored)
    }

    pub fn mirrored(self) -> bool {
        matches!(self, Direction::ForwardMirrored | Direction::BackwardMirrored)
    }

    /// The setup file / `NFSPU_RACE` word.
    pub fn word(self) -> &'static str {
        match self {
            Direction::Forward => "forward",
            Direction::Backward => "backward",
            Direction::ForwardMirrored => "forward_mirrored",
            Direction::BackwardMirrored => "backward_mirrored",
        }
    }
}

/// What opponents drive (`OpponentCar.Car`): one of five rules, or one specific car.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpponentCars {
    SameCar,
    SameEra,
    SameClass,
    SameEraAndClass,
    Random,
    /// A specific catalogue car: its id and model file.
    Car { id: u16, model: String },
    /// One catalogue car per opponent in turn (a career tournament's opponent list).
    Cars(Vec<u16>),
}

impl Default for OpponentCars {
    fn default() -> Self {
        OpponentCars::SameCar
    }
}

/// Everything a quick race needs. Indices are the retail dropdown positions; `None`-free so a
/// reader never has to guess a default.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RaceSetup {
    /// Track file stem in `GameData/Track` (`coastal`), its `nfs5.trk` id and shown name.
    pub track: String,
    pub track_id: i16,
    pub track_name: String,
    pub direction: Direction,
    /// 1..=8.
    pub laps: u8,
    /// 0 none, 1 light, 2 medium, 3 heavy (`Track1.Traffic`).
    pub traffic: u8,
    /// 0..=7 (`Game.NumOpponents`).
    pub opponents: u8,
    pub opponent_cars: OpponentCars,
    /// The player's car: catalogue id, model file stem in `GameData/CarModel`, shown name, and
    /// whether it is one the player owns (career garage) rather than factory stock.
    pub car_id: u16,
    pub car_model: String,
    pub car_name: String,
    pub owned_car: bool,
    /// Paint: the colour list's position (`GenericCar.Colour`, counted from 0) ...
    pub paint: u8,
    /// ... and the `.clr` section it names (`[sectionN]`), which is what the car model takes
    /// (`car_model::CarOptions::paint`). Both are written so the race needs no catalogue lookup.
    pub paint_section: usize,
    /// A custom body colour picked in the colour list's palette (replaces the section's `color1`),
    /// or `None` for the section's own colours. Written as `custom_colour = r,g,b` or `none`.
    pub custom_colour: Option<[u8; 3]>,
    /// Body style in the model's `.tpg` (`[styleN]`, 1-based; `car_model::CarOptions::style`),
    /// from the catalogue record: the 356 1100 Cabriolet is 2 ("Cabrio").
    pub car_style: usize,
    /// `settings.lay`: transmission (false manual, true automatic), ABS, traction control, roof
    /// up, damage.
    pub automatic: bool,
    pub abs: bool,
    pub traction_control: bool,
    pub roof_up: bool,
    pub damage: bool,
    /// 0 beginner, 1 advanced, 2 expert (`Game.Difficutly`).
    pub difficulty: u8,
    /// The save the choices came from (`3Greg`), empty without one.
    pub player: String,
    /// Career part effects on the car's handling (stock when missing).
    pub tuning: crate::car_sim::Tuning,
}

impl RaceSetup {
    /// The player's car model options in the race: the setup's body style and paint section (the
    /// ones the garage showed), exterior detail, a driver in the seat.
    pub fn car_options(&self) -> crate::car_model::CarOptions {
        crate::car_model::CarOptions { style: self.car_style.max(1), paint: self.paint_section, body: self.custom_colour, roof_up: self.roof_up, ..Default::default() }
    }

    /// `key = value` lines, one per field, in a fixed order.
    pub fn to_text(&self) -> String {
        let mut s = String::from("# quick race setup, written by the front end (nfspu-game race_setup.rs)\n");
        let mut line = |k: &str, v: String| {
            let _ = writeln!(s, "{k} = {v}");
        };
        line("track", self.track.clone());
        line("track_id", self.track_id.to_string());
        line("track_name", self.track_name.clone());
        line("direction", self.direction.word().to_string());
        line("laps", self.laps.to_string());
        line("traffic", self.traffic.to_string());
        line("opponents", self.opponents.to_string());
        line(
            "opponent_cars",
            match &self.opponent_cars {
                OpponentCars::SameCar => "same_car".to_string(),
                OpponentCars::SameEra => "same_era".to_string(),
                OpponentCars::SameClass => "same_class".to_string(),
                OpponentCars::SameEraAndClass => "same_era_class".to_string(),
                OpponentCars::Random => "random".to_string(),
                OpponentCars::Car { id, model } => format!("car {id} {model}"),
                OpponentCars::Cars(ids) => format!("cars {}", ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",")),
            },
        );
        line("car_id", self.car_id.to_string());
        line("car_model", self.car_model.clone());
        line("car_name", self.car_name.clone());
        line("owned_car", self.owned_car.to_string());
        line("paint", self.paint.to_string());
        line("paint_section", self.paint_section.to_string());
        line("custom_colour", self.custom_colour.map_or("none".to_string(), |[r, g, b]| format!("{r},{g},{b}")));
        line("car_style", self.car_style.to_string());
        line("automatic", self.automatic.to_string());
        line("abs", self.abs.to_string());
        line("traction_control", self.traction_control.to_string());
        line("roof_up", self.roof_up.to_string());
        line("damage", self.damage.to_string());
        line("difficulty", self.difficulty.to_string());
        line("player", self.player.clone());
        line("tuning", format!("{},{},{},{}", self.tuning.power, self.tuning.grip, self.tuning.brake, self.tuning.mass));
        s
    }

    /// Reads [`RaceSetup::to_text`] back. Unknown keys and `#` comments are ignored; a missing or
    /// unreadable key is an error naming it.
    pub fn parse(text: &str) -> Result<RaceSetup, String> {
        let map: std::collections::BTreeMap<&str, &str> = text
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.trim(), v.trim()))
            .collect();
        let get = |k: &str| map.get(k).copied().ok_or_else(|| format!("race setup: missing '{k}'"));
        fn num<T: std::str::FromStr>(k: &str, v: &str) -> Result<T, String> {
            v.parse().map_err(|_| format!("race setup: bad '{k}' value '{v}'"))
        }
        let flag = |k: &str| -> Result<bool, String> { num(k, get(k)?) };
        let direction = match get("direction")? {
            "forward" => Direction::Forward,
            "backward" => Direction::Backward,
            "forward_mirrored" => Direction::ForwardMirrored,
            "backward_mirrored" => Direction::BackwardMirrored,
            other => return Err(format!("race setup: bad direction '{other}'")),
        };
        let opponent_cars = match get("opponent_cars")? {
            "same_car" => OpponentCars::SameCar,
            "same_era" => OpponentCars::SameEra,
            "same_class" => OpponentCars::SameClass,
            "same_era_class" => OpponentCars::SameEraAndClass,
            "random" => OpponentCars::Random,
            other => {
                let mut parts = other.split_whitespace();
                match (parts.next(), parts.next(), parts.next()) {
                    (Some("cars"), Some(ids), None) => OpponentCars::Cars(ids.split(',').map(|i| num("opponent_cars", i)).collect::<Result<_, _>>()?),
                    (Some("car"), Some(id), Some(model)) => OpponentCars::Car { id: num("opponent_cars", id)?, model: model.to_string() },
                    _ => return Err(format!("race setup: bad opponent_cars '{other}'")),
                }
            }
        };
        Ok(RaceSetup {
            track: get("track")?.to_string(),
            track_id: num("track_id", get("track_id")?)?,
            track_name: get("track_name")?.to_string(),
            direction,
            laps: num("laps", get("laps")?)?,
            traffic: num("traffic", get("traffic")?)?,
            opponents: num("opponents", get("opponents")?)?,
            opponent_cars,
            car_id: num("car_id", get("car_id")?)?,
            car_model: get("car_model")?.to_string(),
            car_name: get("car_name")?.to_string(),
            owned_car: flag("owned_car")?,
            paint: num("paint", get("paint")?)?,
            paint_section: num("paint_section", get("paint_section")?)?,
            // older setups have no custom colour line
            custom_colour: get("custom_colour").ok().and_then(|v| {
                let c: Vec<u8> = v.split(',').filter_map(|n| n.trim().parse().ok()).collect();
                (c.len() == 3).then(|| [c[0], c[1], c[2]])
            }),
            car_style: num("car_style", get("car_style")?)?,
            automatic: flag("automatic")?,
            abs: flag("abs")?,
            traction_control: flag("traction_control")?,
            roof_up: flag("roof_up")?,
            damage: flag("damage")?,
            difficulty: num("difficulty", get("difficulty")?)?,
            player: map.get("player").copied().unwrap_or("").to_string(),
            tuning: map
                .get("tuning")
                .map(|v| v.split(',').filter_map(|n| n.trim().parse::<f32>().ok()).collect::<Vec<_>>())
                .filter(|t| t.len() == 4)
                .map_or_else(Default::default, |t| crate::car_sim::Tuning { power: t[0], grip: t[1], brake: t[2], mass: t[3] }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> RaceSetup {
        RaceSetup {
            track: "coastal".into(),
            track_id: 8,
            track_name: "Côte d'Azur".into(),
            direction: Direction::BackwardMirrored,
            laps: 2,
            traffic: 2,
            opponents: 7,
            opponent_cars: OpponentCars::Car { id: 120, model: "901".into() },
            car_id: 76,
            car_model: "356_1".into(),
            car_name: "'50 356 1100 Cabriolet".into(),
            owned_car: true,
            paint: 20,
            paint_section: 21,
            custom_colour: Some([12, 200, 99]),
            car_style: 2,
            automatic: true,
            abs: true,
            traction_control: false,
            roof_up: true,
            damage: false,
            difficulty: 1,
            player: "3Greg".into(),
            tuning: crate::car_sim::Tuning { power: 1.1, grip: 1.05, brake: 1.0, mass: 0.98 },
        }
    }

    #[test]
    fn custom_colour_round_trips_and_may_be_absent() {
        let s = sample();
        assert_eq!(RaceSetup::parse(&s.to_text()).unwrap().custom_colour, Some([12, 200, 99]));
        let none = RaceSetup { custom_colour: None, ..sample() };
        assert_eq!(RaceSetup::parse(&none.to_text()).unwrap().custom_colour, None);
        let old: String = s.to_text().lines().filter(|l| !l.starts_with("custom_colour")).map(|l| format!("{l}
")).collect();
        assert_eq!(RaceSetup::parse(&old).unwrap().custom_colour, None, "setups written before the field still load");
        assert_eq!(s.car_options().body, Some([12, 200, 99]));
    }

    #[test]
    fn text_round_trips() {
        let s = sample();
        assert_eq!(RaceSetup::parse(&s.to_text()), Ok(s.clone()));
        for rule in [OpponentCars::SameCar, OpponentCars::SameEra, OpponentCars::SameClass, OpponentCars::SameEraAndClass, OpponentCars::Random] {
            let s = RaceSetup { opponent_cars: rule, ..sample() };
            assert_eq!(RaceSetup::parse(&s.to_text()), Ok(s));
        }
    }

    #[test]
    fn missing_or_bad_values_are_named() {
        let text = sample().to_text().replace("laps = 2", "laps = two");
        assert_eq!(RaceSetup::parse(&text), Err("race setup: bad 'laps' value 'two'".into()));
        let text = sample().to_text().replace("track = coastal\n", "");
        assert_eq!(RaceSetup::parse(&text), Err("race setup: missing 'track'".into()));
    }

    #[test]
    fn directions_map_to_dropdown_rows() {
        assert_eq!(Direction::from_index(2), Direction::ForwardMirrored);
        assert_eq!(Direction::from_index(9), Direction::BackwardMirrored);
        assert!(Direction::BackwardMirrored.backward() && Direction::BackwardMirrored.mirrored());
        assert!(!Direction::Forward.backward());
        assert_eq!(Direction::Backward.index(), 1);
        assert_eq!(Direction::ForwardMirrored.word(), "forward_mirrored");
        assert_eq!(Direction::BackwardMirrored.word(), "backward_mirrored");
    }
}
