//! The quick-race choices behind `QuickRace.lay`, `location.lay` and `settings.lay`: which cars,
//! tracks and options each dropdown lists, which one is chosen, and the [`RaceSetup`] they make.
//!
//! The menu renderer (`menu.rs`) draws whatever [`QuickRace::choices`] returns; the viewer
//! (`menu_mode.rs`) feeds clicks back through [`QuickRace::pick`]. Evidence for every list is in
//! `docs/provenance/fe.md`, "Quick race".

use crate::catalog::{self, Car, Paint, Track};
use crate::menu::{Choice, Row};
use crate::race_setup::{Direction, OpponentCars, RaceSetup};
use nfspu_formats::fe::Locale;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Locale strings the lists use (all checked in `FEStrings.csv` and against the executable's
/// string tables).
const OWNED: u32 = 570;
const FACTORY_STOCK: u32 = 571;
const TRAFFIC: [u32; 4] = [403, 402, 401, 400];
const DIRECTION: [u32; 4] = [410, 411, 412, 413];
/// The closed direction control shows a short form: forward, bkw, fwd/mir, bkw/mir.
const DIRECTION_SHORT: [u32; 4] = [410, 416, 417, 418];
const LAPS_FIRST: u32 = 420;
pub const DIFFICULTY_FIRST: u32 = 450;
/// Opponent rules in list order, with the closed form of the fourth ("era/class").
const OPPONENT_RULES: [u32; 5] = [640, 642, 643, 644, 641];
const OPPONENT_RULES_SHORT: [u32; 5] = [640, 642, 643, 645, 641];
const OFF_ON: [u32; 2] = [1500, 1501];
const ON: u32 = 704;
const OFF: u32 = 705;
const UP_DOWN: [u32; 2] = [1509, 1510];
const MANUAL_AUTOMATIC: [u32; 2] = [560, 561];

/// Accessor names as the layouts write them (without a `(SHORT)` suffix).
pub const CAR: &str = "zMergedList.Car";
pub const COLOUR: &str = "GenericCar.Colour";
pub const TRACK: &str = "Track1.Track";
pub const DIRECTION_KEY: &str = "Track1.Direction";
pub const TRAFFIC_KEY: &str = "Track1.Traffic";
pub const LAPS: &str = "Track1.Laps";
pub const OPPONENT_CAR: &str = "OpponentCar.Car";
pub const OPPONENTS: &str = "Game.NumOpponents";
pub const DIFFICULTY: &str = "Game.Difficutly";
pub const TRANSMISSION: &str = "Car.Transmission";
pub const ABS: &str = "Car.ABS";
pub const TRACTION: &str = "Car.Traction";
pub const ROOF: &str = "Car.Roof";
pub const DAMAGE: &str = "Game.Damage";

/// Layout control names that are shown for one kind of car only: an owned car has a garage
/// button, a factory car a colour picker, at the same place (seen in a retail recording).
pub const GARAGE_BUTTON: &str = "ID_MERGEDLISTCARSETUP";
pub const COLOUR_BOX: &str = "ID_MERGEDLISTCOLOR";

/// Everything the quick-race screens choose from and what is chosen.
#[derive(Debug, Clone, Default)]
pub struct QuickRace {
    game_dir: PathBuf,
    pub cars: Vec<Car>,
    pub tracks: Vec<Track>,
    /// Indices into `cars`: the player's own cars, then factory stock (the car dropdown lists
    /// both, in that order), and the cars opponents can be given.
    pub owned: Vec<usize>,
    /// Each owned car's paint section from the save, parallel to `owned` (0 when unread).
    pub owned_paint: Vec<u16>,
    /// Each owned car's body colour for the list swatch, parallel to `owned`.
    pub owned_swatch: Vec<Option<[u8; 3]>>,
    pub factory: Vec<usize>,
    pub opponent_list: Vec<usize>,
    /// Indices into `tracks` that the location dropdown offers.
    pub track_list: Vec<usize>,
    /// The save folder's `nfs5.trk` (best times).
    records: Vec<u8>,
    /// Paints of the chosen car's model.
    pub paints: Vec<Paint>,
    paints_for: String,
    pub player: String,
    // Choices, as dropdown positions.
    pub car: usize,
    pub colour: usize,
    pub track: usize,
    pub direction: usize,
    pub laps: usize,
    pub traffic: usize,
    pub opponents: usize,
    pub opponent_car: usize,
    pub difficulty: usize,
    pub transmission: usize,
    pub abs: usize,
    pub traction: usize,
    pub roof: usize,
    pub damage: usize,
}

/// The custom colours offered after the model's own paints (not in retail): twelve hues at three
/// strengths, then six greys from white to black. Picking one keeps the model's first paint scheme
/// for stripes and cabin and replaces only its body colour.
pub fn custom_palette() -> Vec<[u8; 3]> {
    let mut out = Vec::new();
    for v in [255u8, 176, 96] {
        for hue in 0..12u32 {
            out.push(crate::car_model::hsv_to_rgb((hue * 255 / 12) as u8, 255, v));
        }
    }
    out.extend([255u8, 204, 153, 102, 51, 0].map(|g| [g, g, g]));
    out
}

/// A save value clamped to a list length (0 when absent).
fn saved(save: Option<&[u8]>, name: &str, len: usize, default: usize) -> usize {
    save.and_then(|s| crate::menu::save_value(s, name)).map(|v| v as usize).unwrap_or(default).min(len.saturating_sub(1))
}

/// The swatch index for a paint value kept in a save: its `[sectionN]` number when the model has
/// that section, else the value read as a 1-based list position. Both readings agree on Greg's
/// owned 356 (19 = orange section 19); which one retail means is not settled (`fe.md`).
fn paint_index(paints: &[Paint], value: u16) -> Option<usize> {
    if let Some(i) = paints.iter().position(|p| p.section == usize::from(value)) {
        return Some(i);
    }
    let pos = usize::from(value).checked_sub(1)?;
    paints.get(pos).map(|_| pos)
}

impl QuickRace {
    /// Reads the catalogue from the game folder and the player's last choices from their save
    /// (read-only). `career_year` opens tournament tracks (1950 for a new player).
    pub fn load(game_dir: &Path, save: Option<&[u8]>, career_year: i32, player: &str) -> QuickRace {
        let data = game_dir.join("FEData").join("Data");
        let cars = catalog::parse_cars(&std::fs::read(data.join("nfs5.car")).unwrap_or_default());
        let tracks = catalog::parse_tracks(&std::fs::read(data.join("nfs5.trk")).unwrap_or_default());
        let tournaments = catalog::parse_tournaments(&std::fs::read(data.join("nfs5.trn")).unwrap_or_default());
        let models = catalog::file_names(&game_dir.join("GameData").join("CarModel"));
        let track_files = catalog::file_names(&game_dir.join("GameData").join("Track"));
        let records = std::fs::read(game_dir.join("SaveData").join("nfs5.trk")).unwrap_or_default();
        let owned_cars = save.map(|s| catalog::owned_cars(s, &cars)).unwrap_or_default();
        let owned: Vec<usize> = owned_cars.iter().map(|o| o.index).collect();
        let owned_paint: Vec<u16> = owned_cars.iter().map(|o| o.paint).collect();
        let owned_swatch: Vec<Option<[u8; 3]>> = owned_cars
            .iter()
            .map(|o| {
                let paints = catalog::load_paints(game_dir, &cars[o.index].model);
                paint_index(&paints, o.paint).map(|i| paints[i].body)
            })
            .collect();
        let mut q = QuickRace {
            game_dir: game_dir.to_path_buf(),
            owned,
            owned_paint,
            owned_swatch,
            factory: catalog::quick_race_cars(&cars),
            opponent_list: catalog::opponent_cars(&cars, |m| models.contains(&format!("{m}.crp").to_ascii_lowercase())),
            track_list: catalog::quick_race_tracks(&tracks, &tournaments, career_year, |f| track_files.contains(&format!("{f}.crp").to_ascii_lowercase())),
            cars,
            tracks,
            records,
            player: player.to_string(),
            ..Default::default()
        };
        // Retail resets the car to the top of the list when quick race is chosen (the "quick
        // race" button's game-type routine sets `zMergedList.Car` to 0), so the saved value is not
        // used. The others come from the save as the accessors load them.
        q.car = 0;
        q.colour = saved(save, COLOUR, usize::MAX, 0);
        q.track = saved(save, TRACK, q.track_list.len(), 0);
        q.direction = saved(save, DIRECTION_KEY, 4, 0);
        q.laps = saved(save, LAPS, 8, 1);
        q.traffic = saved(save, TRAFFIC_KEY, 4, 0);
        q.opponents = saved(save, OPPONENTS, 8, 4);
        q.opponent_car = saved(save, OPPONENT_CAR, 5 + q.opponent_list.len(), 0);
        q.difficulty = saved(save, DIFFICULTY, 3, 1);
        q.transmission = saved(save, TRANSMISSION, 2, 1);
        q.abs = saved(save, ABS, 2, 0);
        q.traction = saved(save, TRACTION, 2, 0);
        q.roof = saved(save, ROOF, 2, 0);
        q.damage = saved(save, DAMAGE, 2, 1);
        q.refresh_paints();
        q
    }

    /// The chosen car: (catalogue index, owned).
    pub fn chosen_car(&self) -> Option<(usize, bool)> {
        if self.car < self.owned.len() {
            Some((self.owned[self.car], true))
        } else {
            self.factory.get(self.car - self.owned.len()).map(|i| (*i, false))
        }
    }

    fn refresh_paints(&mut self) {
        let chosen = self.chosen_car();
        let model = chosen.map(|(i, _)| self.cars[i].model.clone()).unwrap_or_default();
        if model != self.paints_for {
            self.paints = catalog::load_paints(&self.game_dir, &model);
            self.paints_for = model;
        }
        // An owned car shows the paint kept in the save. Its colour box is replaced by the garage
        // button, so this choice is not the player's dropdown pick.
        if let Some((_, true)) = chosen {
            if let Some(pos) = self.owned_paint.get(self.car).copied().and_then(|value| paint_index(&self.paints, value)) {
                self.colour = pos;
            }
        }
        self.colour = self.colour.min((self.paints.len() + custom_palette().len()).saturating_sub(1));
    }

    /// The custom body colour picked from the palette after the model's paints, if any.
    pub fn custom_colour(&self) -> Option<[u8; 3]> {
        self.colour.checked_sub(self.paints.len()).and_then(|i| custom_palette().get(i).copied())
    }

    pub fn chosen_track(&self) -> Option<&Track> {
        self.track_list.get(self.track).map(|i| &self.tracks[*i])
    }

    /// The control names to hide: the garage button for a factory car, the colour picker for an
    /// owned one.
    pub fn hidden(&self) -> Vec<String> {
        match self.chosen_car() {
            Some((_, true)) => vec![COLOUR_BOX.to_string()],
            _ => vec![GARAGE_BUTTON.to_string()],
        }
    }

    /// Sets a dropdown to an item. Returns false for an accessor this screen does not own.
    pub fn pick(&mut self, accessor: &str, item: usize) -> bool {
        let slot = match accessor {
            CAR => &mut self.car,
            COLOUR => &mut self.colour,
            TRACK => &mut self.track,
            DIRECTION_KEY => &mut self.direction,
            TRAFFIC_KEY => &mut self.traffic,
            LAPS => &mut self.laps,
            OPPONENT_CAR => &mut self.opponent_car,
            OPPONENTS => &mut self.opponents,
            DIFFICULTY => &mut self.difficulty,
            TRANSMISSION => &mut self.transmission,
            ABS => &mut self.abs,
            TRACTION => &mut self.traction,
            ROOF => &mut self.roof,
            DAMAGE => &mut self.damage,
            _ => return false,
        };
        *slot = item;
        self.refresh_paints();
        true
    }

    /// The location arrows (`TRACK1INC` / `TRACK1DEC`) step through the tracks and wrap.
    pub fn step_track(&mut self, delta: i32) {
        let n = self.track_list.len() as i32;
        if n > 0 {
            self.track = (self.track as i32 + delta).rem_euclid(n) as usize;
        }
    }

    /// Every dropdown's rows, chosen item and closed text, keyed by accessor.
    pub fn choices(&self, locale: &Locale, language: u8) -> BTreeMap<String, Choice> {
        let s = |id: u32| locale.text(id, language).map(|t| t.trim().to_string()).unwrap_or_default();
        let plain = |ids: &[u32], selected: usize, short: &[u32]| Choice {
            rows: ids.iter().map(|id| Row::item(s(*id))).collect(),
            selected,
            closed: short.get(selected).map(|id| s(*id)).unwrap_or_default(),
            closed_swatch: None,
        };
        let mut out = BTreeMap::new();

        // Car: "OWNED" and "FACTORY STOCK" sections, each grouped by era and class headers.
        let mut rows = Vec::new();
        let owned_items: Vec<(usize, Option<[u8; 3]>)> = self.owned.iter().enumerate().map(|(n, i)| (*i, self.owned_swatch.get(n).copied().flatten())).collect();
        let factory_items: Vec<(usize, Option<[u8; 3]>)> = self.factory.iter().map(|i| (*i, None)).collect();
        for (title, list, divider) in [(OWNED, &owned_items, true), (FACTORY_STOCK, &factory_items, false)] {
            if list.is_empty() {
                continue;
            }
            rows.push(Row::Header(s(title)));
            let mut last: Option<(u32, u32)> = None;
            for (i, swatch) in list.iter() {
                let car = &self.cars[*i];
                if last.map(|l| l.0) != Some(car.era) {
                    rows.push(Row::Header(s(car.era)));
                }
                if last != Some((car.era, car.class)) {
                    rows.push(Row::Header(s(car.class)));
                }
                last = Some((car.era, car.class));
                let (year, model) = car.year_and_model();
                // An owned row's swatch is that car's own paint, read from the save at load.
                rows.push(Row::Item { lead: Some(year), text: model, swatch: *swatch, divider });
            }
        }
        let closed = self.chosen_car().map(|(i, _)| self.cars[i].short.clone()).unwrap_or_default();
        out.insert(CAR.to_string(), Choice { rows, selected: self.car, closed, closed_swatch: None });

        // Paint: one swatch per `.clr` section, then the custom palette under its own title.
        let swatch = |rgb: [u8; 3]| Row::Item { lead: None, text: String::new(), swatch: Some(rgb), divider: false };
        let mut rows: Vec<Row> = self.paints.iter().map(|p| swatch(p.body)).collect();
        rows.push(Row::Header("CUSTOM".into()));
        rows.extend(custom_palette().into_iter().map(swatch));
        let closed_swatch = self.custom_colour().or_else(|| self.paints.get(self.colour).map(|p| p.body));
        out.insert(COLOUR.to_string(), Choice { rows, selected: self.colour, closed: String::new(), closed_swatch });

        // Location: the full name in the list, the short name when closed.
        let rows = self.track_list.iter().map(|i| Row::item(s(self.tracks[*i].name_string))).collect();
        let closed = self.chosen_track().map(|t| s(t.name_string + 40)).unwrap_or_default();
        out.insert(TRACK.to_string(), Choice { rows, selected: self.track, closed, closed_swatch: None });

        out.insert(DIRECTION_KEY.to_string(), plain(&DIRECTION, self.direction, &DIRECTION_SHORT));
        out.insert(TRAFFIC_KEY.to_string(), plain(&TRAFFIC, self.traffic, &TRAFFIC));
        let laps: Vec<u32> = (LAPS_FIRST..LAPS_FIRST + 8).collect();
        out.insert(LAPS.to_string(), plain(&laps, self.laps, &laps));

        // Opponents' cars: five rules, then every installed road car.
        let mut rows: Vec<Row> = OPPONENT_RULES.iter().map(|id| Row::item(s(*id))).collect();
        rows.extend(self.opponent_list.iter().map(|i| Row::item(self.cars[*i].name.clone())));
        let closed = if self.opponent_car < 5 { s(OPPONENT_RULES_SHORT[self.opponent_car]) } else { self.opponent_list.get(self.opponent_car - 5).map(|i| self.cars[*i].short.clone()).unwrap_or_default() };
        out.insert(OPPONENT_CAR.to_string(), Choice { rows, selected: self.opponent_car, closed, closed_swatch: None });

        let counts: Vec<Row> = (0..8).map(|n| Row::item(n.to_string())).collect();
        out.insert(OPPONENTS.to_string(), Choice { rows: counts, selected: self.opponents, closed: self.opponents.to_string(), closed_swatch: None });

        let difficulty: Vec<u32> = (DIFFICULTY_FIRST..DIFFICULTY_FIRST + 3).collect();
        out.insert(DIFFICULTY.to_string(), plain(&difficulty, self.difficulty, &difficulty));
        out.insert(TRANSMISSION.to_string(), plain(&MANUAL_AUTOMATIC, self.transmission, &MANUAL_AUTOMATIC));
        out.insert(ABS.to_string(), plain(&OFF_ON, self.abs, &OFF_ON));
        out.insert(TRACTION.to_string(), plain(&OFF_ON, self.traction, &OFF_ON));
        out.insert(ROOF.to_string(), plain(&UP_DOWN, self.roof, &UP_DOWN));
        out.insert(DAMAGE.to_string(), plain(&[OFF, ON], self.damage, &[OFF, ON]));
        out
    }

    /// The text of the `NFS5.AccessorWatch` labels on the location screens, keyed by accessor as
    /// the layout writes it.
    pub fn watches(&self, locale: &Locale, language: u8) -> BTreeMap<String, String> {
        let s = |id: u32| locale.text(id, language).map(|t| t.trim().to_string()).unwrap_or_default();
        let mut out = BTreeMap::new();
        if let Some(t) = self.chosen_track() {
            let name = s(t.name_string + 60);
            let difficulty = s(DIFFICULTY_FIRST + t.difficulty.clamp(0, 2) as u32);
            let record = catalog::record_ticks(&self.records, t.id).map(catalog::format_ticks).unwrap_or_else(|| "--:--.--".into());
            for prefix in ["Track1.Track", "LoopedTrack.Track"] {
                out.insert(prefix.to_string(), name.clone());
                out.insert(format!("{prefix}(DIFFICULTY)"), difficulty.clone());
                out.insert(format!("{prefix}(RECORDTIME)"), record.clone());
            }
        }
        out
    }

    /// How the chosen car looks: (model file, `.clr` paint section, `.tpg` body style). The garage
    /// shows exactly this and the race spawns exactly this (through `RaceSetup`), so the two can
    /// not differ. The paint is the colour list's choice; an owned car's own colour (kept in the
    /// save) is not read yet. A model without a `.clr` keeps the car model's default paint.
    pub fn car_look(&self) -> Option<(String, usize, usize)> {
        let (i, _) = self.chosen_car()?;
        let car = &self.cars[i];
        Some((car.model.clone(), self.paint_section_or(crate::car_model::CarOptions::default().paint), car.style.max(1)))
    }

    /// The chosen car's logo (`NFS5.CarLogo`): archive file and logo number (two entries each).
    pub fn car_logo(&self) -> Option<(String, usize)> {
        let (i, _) = self.chosen_car()?;
        let car = &self.cars[i];
        (!car.logo_file.is_empty()).then(|| (format!("{}.fsh", car.logo_file), car.logo_index as usize))
    }

    /// The location screen's picture of the chosen track (a stand-in for retail's 3D track room).
    pub fn track_art(&self) -> Option<String> {
        self.chosen_track().map(|t| format!("TrackArt/{}.fsh", t.art[0])).filter(|a| a != "TrackArt/.fsh")
    }

    /// Renderer paint identifier (`[sectionN]`), distinct from the colour dropdown's file-order index.
    pub fn paint_section(&self) -> usize {
        self.paint_section_or(4)
    }

    /// The chosen `.clr` section; a custom colour uses the model's first section underneath.
    fn paint_section_or(&self, default: usize) -> usize {
        let at = if self.custom_colour().is_some() { 0 } else { self.colour };
        self.paints.get(at).map_or(default, |p| p.section)
    }

    /// The race these choices make.
    pub fn setup(&self, locale: &Locale, language: u8) -> RaceSetup {
        let s = |id: u32| locale.text(id, language).map(|t| t.trim().to_string()).unwrap_or_default();
        let (car, owned) = self.chosen_car().map(|(i, o)| (self.cars[i].clone(), o)).unwrap_or_default();
        let (_, paint_section, car_style) = self.car_look().unwrap_or((String::new(), crate::car_model::CarOptions::default().paint, 1));
        let track = self.chosen_track().cloned().unwrap_or_default();
        let opponent_cars = match self.opponent_car {
            0 => OpponentCars::SameCar,
            1 => OpponentCars::SameEra,
            2 => OpponentCars::SameClass,
            3 => OpponentCars::SameEraAndClass,
            4 => OpponentCars::Random,
            n => self.opponent_list.get(n - 5).map(|i| OpponentCars::Car { id: self.cars[*i].id, model: self.cars[*i].model.clone() }).unwrap_or_default(),
        };
        RaceSetup {
            track: track.file.clone(),
            track_id: track.id,
            track_name: s(track.name_string),
            direction: Direction::from_index(self.direction),
            laps: self.laps as u8 + 1,
            traffic: self.traffic as u8,
            opponents: self.opponents as u8,
            opponent_cars,
            car_id: car.id,
            car_model: car.model.clone(),
            car_name: car.name.clone(),
            owned_car: owned,
            paint: self.colour.min(255) as u8,
            paint_section,
            custom_colour: self.custom_colour(),
            car_style,
            automatic: self.transmission == 1,
            abs: self.abs == 1,
            traction_control: self.traction == 1,
            roof_up: self.roof == 0,
            damage: self.damage == 1,
            difficulty: self.difficulty as u8,
            player: self.player.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> QuickRace {
        let car = |id: u16, name: &str, short: &str, model: &str, era: u32, class: u32| Car { id, name: name.into(), short: short.into(), model: model.into(), logo_file: "finallogos2".into(), logo_index: 4, era, class, ..Default::default() };
        let track = |id: i16, file: &str, name: u32| Track { id, name_string: name, file: file.into(), art: [file.into(), String::new(), String::new()], ..Default::default() };
        QuickRace {
            cars: vec![car(76, "'50 356 1100 Cabriolet", "356", "356_1", 521, 501), car(108, "'60 356 B 1600 Coupé", "356 B", "356b", 521, 501), car(120, "'65 911 Coupé", "911", "901", 522, 502)],
            tracks: vec![track(8, "coastal", 303), track(10, "foothills", 304)],
            owned: vec![0],
            factory: vec![0, 1, 2],
            opponent_list: vec![0, 1, 2],
            track_list: vec![0, 1],
            laps: 1,
            opponents: 7,
            transmission: 1,
            damage: 1,
            ..Default::default()
        }
    }

    fn locale() -> Locale {
        Locale::parse_csv(
            "String ID,ENGLISH_IDS\n303,Côte d'Azur\n343,Côte d'Azur\n363,COTE D'AZUR\n304,Pyrénées\n344,Pyrénées\n501,CLASS 3\n502,CLASS 2\n521,CLASS ERA\n522,GOLDEN ERA\n570,OWNED\n571,FACTORY STOCK\n640,same car\n641,random car\n642,same era\n643,same class\n644,same era/class\n645,era/class\n410,forward\n411,backward\n416,bkw\n421,2\n",
        )
    }

    #[test]
    fn the_car_list_has_owned_and_factory_sections_with_era_and_class_headers() {
        let q = sample();
        let choices = q.choices(&locale(), 0);
        let car = &choices[CAR];
        let shape: Vec<String> = car.rows.iter().map(|r| match r { Row::Header(h) => format!("[{h}]"), Row::Item { text, .. } => text.clone() }).collect();
        assert_eq!(shape, vec!["[OWNED]", "[CLASS ERA]", "[CLASS 3]", "356 1100 Cabriolet", "[FACTORY STOCK]", "[CLASS ERA]", "[CLASS 3]", "356 1100 Cabriolet", "356 B 1600 Coupé", "[GOLDEN ERA]", "[CLASS 2]", "911 Coupé"]);
        assert_eq!(car.closed, "356", "the owned car first, by its short name");
        assert_eq!(q.hidden(), vec![COLOUR_BOX.to_string()]);
        let mut q = q;
        q.pick(CAR, 2);
        assert_eq!(q.choices(&locale(), 0)[CAR].closed, "356 B");
        assert_eq!(q.hidden(), vec![GARAGE_BUTTON.to_string()], "factory cars pick a colour instead");
    }

    #[test]
    fn closed_values_use_the_short_forms() {
        let mut q = sample();
        q.pick(DIRECTION_KEY, 1);
        q.pick(OPPONENT_CAR, 3);
        let c = q.choices(&locale(), 0);
        assert_eq!(c[DIRECTION_KEY].closed, "bkw");
        assert_eq!(c[DIRECTION_KEY].rows.len(), 4);
        assert_eq!(c[OPPONENT_CAR].closed, "era/class");
        assert_eq!(c[OPPONENT_CAR].rows.len(), 5 + 3);
        assert_eq!(c[TRACK].closed, "Côte d'Azur");
        assert_eq!(c[OPPONENTS].closed, "7");
        assert_eq!(c[LAPS].closed, "2");
        q.step_track(-1);
        assert_eq!(q.track, 1, "the arrows wrap");
        assert_eq!(q.watches(&locale(), 0).get("Track1.Track").map(String::as_str), Some(""), "no upper-case name for this track in the sample");
    }

    #[test]
    fn the_setup_carries_every_choice() {
        let mut q = sample();
        q.pick(CAR, 3);
        q.pick(TRACK, 1);
        q.pick(OPPONENT_CAR, 6);
        let s = q.setup(&locale(), 0);
        assert_eq!((s.car_id, s.car_model.as_str(), s.owned_car), (120, "901", false));
        assert_eq!((s.track.as_str(), s.track_id, s.track_name.as_str()), ("foothills", 10, "Pyrénées"));
        assert_eq!(s.laps, 2, "dropdown position 1 is two laps");
        assert_eq!(s.opponents, 7);
        assert_eq!(s.opponent_cars, OpponentCars::Car { id: 108, model: "356b".into() });
        assert!(s.automatic && s.damage && s.roof_up && !s.abs);
    }

    #[test]
    fn the_palette_after_the_paints_is_a_custom_colour() {
        let mut q = sample();
        q.paints = catalog::parse_paints("[section1]
color1.r=252
[section5]
color1.r=100
");
        q.paints_for = q.chosen_car().map(|(i, _)| q.cars[i].model.clone()).unwrap_or_default();
        q.colour = 1;
        assert_eq!(q.custom_colour(), None);
        assert_eq!(q.paint_section(), 5);
        q.colour = 2; // the first palette swatch: pure red
        assert_eq!(q.custom_colour(), Some([255, 0, 0]));
        assert_eq!(q.paint_section(), 1, "stripes and cabin come from the first scheme");
        q.pick(COLOUR, 2 + custom_palette().len() + 10);
        assert_eq!(q.custom_colour(), Some([0, 0, 0]), "clamped to the last swatch (black)");
        assert_eq!(custom_palette().len(), 42);
    }

    #[test]
    fn race_paint_keeps_the_selected_section_not_the_list_index() {
        let mut q = sample();
        q.paints = catalog::parse_paints("[section1]\ncolor1.r=252\n[section5]\ncolor1.r=100\n[section4]\ncolor1.r=255\n");
        for (index, section) in [1, 5, 4].into_iter().enumerate() {
            q.colour = index;
            let setup = q.setup(&locale(), 0);
            assert_eq!(setup.paint_section, section);
            assert_eq!(RaceSetup::parse(&setup.to_text()).unwrap().paint_section, section);
        }
    }

    #[test]
    fn a_saved_paint_selects_the_section_not_the_list_position() {
        let paints = catalog::parse_paints("[section1]\ncolor1.r=10\n[section5]\ncolor1.r=50\n[section4]\ncolor1.r=40\n");
        assert_eq!(paint_index(&paints, 4), Some(2), "section 4 is third in file order");
        assert_eq!(paint_index(&paints, 5), Some(1));
        assert_eq!(paint_index(&paints, 3), Some(2), "a missing section falls back to a 1-based list position");
        assert_eq!(paint_index(&paints, 0), None);
        assert_eq!(paint_index(&paints, 9), None);
    }

    #[test]
    fn an_owned_car_uses_its_saved_paint_swatch() {
        let mut q = sample();
        q.paints = catalog::parse_paints("[section1]\ncolor1.r=10\n[section5]\ncolor1.r=50\n[section4]\ncolor1.r=40\n");
        q.paints_for = "356_1".into();
        q.owned_paint = vec![4];
        q.colour = 0;
        q.pick(CAR, 0);
        assert_eq!(q.colour, 2, "the save's section 4 is the third swatch");
        assert_eq!(q.paint_section(), 4);
    }
}
