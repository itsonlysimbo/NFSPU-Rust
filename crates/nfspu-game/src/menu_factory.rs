//! Factory Driver in the front end: the player's mission progress, the text the mission screens
//! show (`Factory.Data(...)` accessors) and the race a mission starts. The rules are in
//! `nfspu_game::factory`; the race side is `race_mission`.

use nfspu_game::catalog::Track;
use nfspu_game::factory::{self, Mission, Progress, Status};
use nfspu_game::factory_live::{self, Active, Verdict};
use nfspu_game::quick_race::QuickRace;
use nfspu_game::race_setup::{Direction, RaceSetup};
use nfspu_formats::fe::Locale;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct FactoryMode {
    /// The Factory Driver screens are in use (game type 5).
    pub on: bool,
    pub missions: Vec<Mission>,
    pub tracks: Vec<Track>,
    pub progress: Option<Progress>,
    pub dir: PathBuf,
    game_dir: PathBuf,
    /// What the last mission said (pass or fail speech), for the log and the screens.
    pub last_speech: String,
    /// The rank just reached, until its promotion screen has been shown.
    pub promoted: bool,
}

/// What settling a verdict did.
#[derive(Debug, PartialEq)]
pub struct Settled {
    pub passed: bool,
    pub promoted: bool,
}

impl FactoryMode {
    pub fn load(game_dir: &Path) -> FactoryMode {
        let tracks = std::fs::read(game_dir.join("FEData").join("Data").join("nfs5.trk")).map(|b| nfspu_game::catalog::parse_tracks(&b)).unwrap_or_default();
        FactoryMode { on: false, missions: factory::load_missions(game_dir), tracks, progress: None, dir: Progress::dir(), game_dir: game_dir.to_path_buf(), last_speech: String::new(), promoted: false }
    }

    /// Loads or starts the player's progress; returns true for a new player.
    pub fn enter(&mut self, player: &str) -> bool {
        self.on = true;
        let loaded = Progress::load_from(&self.dir, player).filter(|p| p.done.len() == self.missions.len());
        let new = loaded.is_none();
        let progress = loaded.unwrap_or_else(|| Progress::new(player, self.missions.len()));
        crate::log_line(&format!("factory: '{player}': {} of {} missions done, next {:?}", progress.done.iter().flatten().count(), self.missions.len(), progress.current()));
        self.progress = Some(progress);
        // `NFSPU_FACTORY_MISSION=n` jumps to mission n (1-based), like retail's mission cheat
        if let Some(n) = std::env::var("NFSPU_FACTORY_MISSION").ok().and_then(|v| v.trim().parse().ok()) {
            if let Some(p) = self.progress.as_mut() {
                p.jump_to(n);
                crate::log_line(&format!("factory: jumped to mission {n}"));
            }
        }
        new
    }

    pub fn save(&self) {
        if let Some(p) = &self.progress {
            if let Err(e) = p.save_to(&self.dir) {
                crate::log_line(&format!("factory: could not save: {e}"));
            }
        }
    }

    pub fn current(&self) -> Option<&Mission> {
        self.progress.as_ref()?.current().and_then(|i| self.missions.get(i))
    }

    fn text(locale: &Locale, language: u8, id: u32, player: &str) -> String {
        locale.text(id, language).map(|t| t.trim().replace("%s", player)).unwrap_or_default()
    }

    /// The `Factory.Data(...)` texts of the current mission.
    pub fn watches(&self, locale: &Locale, language: u8, player: &str) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        let Some(m) = self.current().or_else(|| self.missions.last()) else { return out };
        let t = |id: u32| Self::text(locale, language, id, player);
        out.insert("Factory.Data(NARRNAME)".into(), t(m.narrator_string()));
        out.insert("Factory.Data(MISSONDESC)".into(), t(m.brief));
        // the promotion screen speaks for the mission just passed, the one before the current
        let passed = if self.promoted { m.index.checked_sub(1).and_then(|i| self.missions.get(i)).or(self.missions.last()) } else { Some(m) };
        if let Some(p) = passed {
            out.insert("Factory.Data(ENDOFLEVEL)".into(), t(p.promotion));
            if self.promoted {
                out.insert("Factory.Data(NARRNAME)".into(), t(p.narrator_string()));
            }
        }
        if let Some(track) = self.tracks.iter().find(|k| k.id == m.track) {
            out.insert("Factory.Data(TRACKNAME)".into(), t(track.name_string + 20));
        }
        out
    }

    /// The race of the current mission, and hands the mission to the race side. `None` when there is
    /// nothing to play.
    pub fn start(&mut self, quick: &QuickRace, locale: &Locale, language: u8, player: &str) -> Option<RaceSetup> {
        let m = self.current()?.clone();
        self.promoted = false;
        let scene = factory::load_scene(&self.game_dir, &self.tracks, &m);
        let race = m.min_place.is_some() || scene.is_none();
        let track = self.tracks.iter().find(|k| k.id == m.track)?.clone();
        let mut setup = quick.setup(locale, language);
        setup.track = track.file.clone();
        setup.track_id = track.id;
        setup.track_name = Self::text(locale, language, track.name_string, player);
        setup.direction = Direction::Forward;
        if race {
            // judged by the race: the drivers the record names (their cars are not matched yet),
            // three laps where the briefing says so (mission 30), else two (**guess**)
            setup.laps = if m.index == 30 { 3 } else { 2 };
            setup.opponents = m.opponents.len().min(7) as u8;
            setup.difficulty = 1;
        } else {
            // a long lap count keeps the road race from ending before the mission does
            setup.laps = 8;
            setup.opponents = 0;
        }
        setup.traffic = 0;
        if !race {
            setup.difficulty = 0;
        }
        if let Some(car) = quick.cars.iter().find(|c| c.id == m.car) {
            setup.car_id = car.id;
            setup.car_model = car.model.clone();
            setup.car_name = car.name.clone();
            setup.car_style = car.style;
            setup.owned_car = false;
            setup.paint_section = nfspu_game::car_model::CarOptions::default().paint;
            setup.custom_colour = None;
        } else {
            crate::log_line(&format!("factory: mission car {} is not in the catalogue, using the picked car", m.car));
        }
        crate::log_line(&format!("factory: mission {} ({}) on '{}' in '{}', {} gates, {} cones, race {race}", m.index, m.stem, track.file, setup.car_name, scene.as_ref().map_or(0, |s| s.gates.len()), scene.as_ref().map_or(0, |s| s.cones.len())));
        factory_live::set_active(Some(Active { mission: m, scene }));
        Some(setup)
    }

    /// Back from the race: records a pass (saved), keeps a fail for another go.
    pub fn settle(&mut self, v: &Verdict, locale: &Locale, language: u8, player: &str) -> Settled {
        factory_live::set_active(None);
        let Some(p) = self.progress.as_mut() else { return Settled { passed: false, promoted: false } };
        let promoted = p.record(&self.missions, v.index, v.status, v.outcome);
        let passed = v.status == Status::Passed;
        // one of the mission's speeches, picked by the mission number (retail picks at random)
        if let Some(m) = self.missions.get(v.index) {
            let ids = if passed { m.pass_text } else { m.fail_text };
            let ids: Vec<u32> = ids.into_iter().filter(|i| *i > 0).collect();
            if !ids.is_empty() {
                self.last_speech = Self::text(locale, language, ids[v.outcome.ticks as usize % ids.len()], player);
            }
        }
        crate::log_line(&format!("factory: mission {} {}: \"{}\"{}", v.index, if passed { "PASSED" } else { "failed" }, self.last_speech, if promoted { " (promotion)" } else { "" }));
        self.promoted = promoted;
        self.save();
        Settled { passed, promoted }
    }
}
