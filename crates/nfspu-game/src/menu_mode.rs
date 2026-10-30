//! The retail front end, drawn in the viewer: a stack of `.lay` screens, scaled from their 640x480
//! space to the window, with hover and click navigation.
//!
//! Turned on with `NFSPU_MENU=<screen>` (empty means `singleplayer`, the screen `GameSetup` links
//! to). It is the `AppState::Menu` part of the one program (`flow.rs`): RACE moves on to the
//! loading screen, and the front end's state (page, history, choices, music) waits for the race to
//! end. This is milestone M6; control types the renderer does not draw yet are listed in
//! `logs/m0.log` and counted in `logs/menu_audit.txt`.
//!
//! Layers, bottom to top (see `docs/provenance/fe.md`):
//! 1. room (`room=ID_GARAGE`, a 3D scene) - drawn by `menu_garage` behind a see-through image;
//!    other rooms (`ID_TRACK`) are not drawn yet and clear to black;
//! 2. frame (`screen=ID_MOVIEOVAL` -> `screenmovieoval.lay`) - verified mapping;
//! 3. page (the screen asked for);
//! 4. popup (a screen with `room=ID_NONE` opened over a page, like `selectname` over `GameSetup`)
//!    - confirmed by a retail recording (`singleplayer` over `GameSetup`);
//! 5. `bottombar.lay` on framed pages - loaded by the retail front end; when exactly it shows is a
//!    guess.
//!
//! The stack is composed in software at 640x480 (`nfspu_game::menu_canvas`), blending in screen
//! space like retail, and shown as one image scaled to the window. It is recomposed only when the
//! screens or the hovered control change.

use bevy::asset::RenderAssetUsages;
use bevy::input::mouse::MouseButton;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::window::PrimaryWindow;
use nfspu_formats::fe::{ControlTemplate, Locale};
use nfspu_game::menu::{self, Draw, Hit, Screen};
use crate::flow::{self, AppState};
use nfspu_game::audio_settings::{self, AudioSettings};
use nfspu_game::career;
use nfspu_game::menu_canvas;
use std::{collections::BTreeMap, path::PathBuf};

#[path = "menu_garage.rs"]
mod menu_garage;
#[path = "menu_location.rs"]
mod menu_location;

/// The layout's own coordinate space: the retail front end is authored at 640x480.
const SCREEN_WIDTH: f32 = 640.0;
const SCREEN_HEIGHT: f32 = 480.0;
/// The bar of back / options / exit buttons along the bottom of framed screens.
const BOTTOM_BAR: &str = "bottombar";
/// The Options tab with the sound levels.
const AUDIO_PANEL: &str = "ID_audio_panel";

/// Everything the front end needs to move between screens.
#[derive(Resource)]
struct FrontEnd {
    game_dir: PathBuf,
    locale: Locale,
    templates: BTreeMap<String, ControlTemplate>,
    /// Language column (0 English .. 6 Portuguese), from the reference build's settings.
    language: u8,
    /// The page and the popup over it (if any).
    page: Screen,
    popup: Option<Screen>,
    history: Vec<(Screen, Option<Screen>)>,
    /// The layers as drawn, bottom first, and which of them take clicks.
    layers: Vec<Screen>,
    interactive: Vec<bool>,
    /// Window size the image was last placed for, in logical pixels.
    laid_out_for: Vec2,
    /// Decoded art and fonts for the compositor.
    art: menu_canvas::ArtCache,
    /// The hover state the current image was composed for, and whether the screens changed since.
    hover_key: Option<Vec<(usize, usize)>>,
    dirty: bool,
    /// Recent compose times (ms), logged in batches.
    compose_ms: Vec<f32>,
    /// Saved players for the user list (read-only; selecting a row is not written back).
    profiles: menu::Profiles,
    save_dir: PathBuf,
    /// Settings and dropdown state for the screens (difficulty from the selected save).
    session: menu::Session,
    /// A dropdown list sliding: elapsed ms and whether it is opening.
    combo_slide: Option<(f32, bool)>,
    /// What the pointer was last over (for the hover sound).
    last_hover: Option<Hit>,
    /// The Options > Audio levels (our `settings/audio.txt`), copied to the `AudioSettings`
    /// resource when `audio_changed`; the level bar being dragged (accessor, left, width).
    audio: AudioSettings,
    audio_changed: bool,
    dragging: Option<(String, i32, i32)>,
    /// Help tips: the hover clock and the tip last drawn (`nfspu_game::menu_help`).
    help: nfspu_game::menu_help::HelpTimer,
    tip: Vec<Draw>,
    /// The quick-race choices (cars, tracks, options), read from the catalogue and the save.
    quick: nfspu_game::quick_race::QuickRace,
    /// A screen change being animated (`transon` / `transoff`).
    transition: Option<Transition>,
    /// The frames the animated icons showed in the last picture, and the front-end clock (ms).
    anim_key: Vec<usize>,
    clock_ms: f64,
    /// The Evolution career (`nfspu_game::career`), when the player entered it.
    career: CareerMode,
    /// Factory Driver (`crate::menu_factory`), when the player entered it.
    factory: crate::menu_factory::FactoryMode,
}

/// Evolution career state of the front end.
struct CareerMode {
    /// The career screens are in use (game type 4): the account shows and RACE starts a career race.
    on: bool,
    /// The tournaments of `nfs5.trn` (eras 521..=523).
    data: Vec<career::Tournament>,
    /// The selected player's career, loaded from `career_dir` or newly made.
    career: Option<career::Career>,
    dir: PathBuf,
    /// The tournament id of the race that was started, until its result is paid.
    pending: Option<u32>,
    /// The entry fee paid for the race being run (refunded when the race is not finished).
    fee_paid: i64,
    /// `NFSPU_CAREER_PLACE=n`: pay this place when a race ends unfinished (for tests).
    forced_place: Option<usize>,
    /// The shop screens' state (dealership, used cars, parts ...) and the page they are showing.
    ui: nfspu_game::career_ui::ShopUi,
    page: String,
    /// A race just finished the era: the era-change message opens when the menu comes back.
    era_message: bool,
}

impl CareerMode {
    fn load(game_dir: &std::path::Path) -> CareerMode {
        let mut data = std::fs::read(game_dir.join("FEData").join("Data").join("nfs5.trn")).map(|b| career::parse_tournaments(&b)).unwrap_or_default();
        let tracks = std::fs::read(game_dir.join("FEData").join("Data").join("nfs5.trk")).map(|b| nfspu_game::catalog::parse_tracks(&b)).unwrap_or_default();
        career::assign_routes(&mut data, &tracks);
        let forced_place = std::env::var("NFSPU_CAREER_PLACE").ok().and_then(|v| v.trim().parse().ok());
        CareerMode { on: false, data, career: None, dir: career::Career::dir(), pending: None, forced_place, ui: nfspu_game::career_ui::ShopUi::load(game_dir), page: String::new(), era_message: false, fee_paid: 0 }
    }
}

/// How long one half of a screen change takes: retail's menus run their 0..512 transition
/// progress over half the timer rate, 64 ticks of the 128 Hz front-end clock (0.5 s), after a
/// 3-frame wait. The old screen goes off first, then the new one comes on.
const TRANSITION_MS: f32 = 64.0 * 1000.0 / 128.0;
const TRANSITION_WAIT_FRAMES: u8 = 3;

/// A screen change in progress. While going off, the layers that leave are drawn from `leaving`
/// (the picture before the change); then the layers named in `coming` come on.
struct Transition {
    leaving: Vec<(Screen, bool, bool)>,
    coming: Vec<String>,
    going_off: bool,
    wait: u8,
    ms: f32,
}

impl FrontEnd {
    fn load(&self, name: &str) -> Option<Screen> {
        self.load_as(name, false)
    }

    fn load_as(&self, name: &str, disabled: bool) -> Option<Screen> {
        menu::load_screen_with(&self.game_dir, name, &self.locale, self.language, &self.templates, &self.profiles, disabled, &self.session)
    }

    /// Copies the quick-race choices into the session the screens are built from.
    fn sync_quick(&mut self) {
        self.quick.difficulty = self.session.difficulty;
        self.session.choices = self.quick.choices(&self.locale, self.language);
        self.session.hidden = self.quick.hidden();
        self.session.watch = self.quick.watches(&self.locale, self.language);
        self.session.car_logo = self.quick.car_logo();
        self.session.track_art = self.quick.track_art();
        // Options: only the chosen tab's panel shows; the Audio page's playlist panel (the CD
        // player) is not built yet.
        let panel = self.session.panel.clone();
        self.session.hidden.extend(menu::OPTION_PANELS.iter().filter(|p| !p.eq_ignore_ascii_case(&panel)).map(|p| p.to_string()));
        self.session.hidden.push("ID_PLAYLISTPANEL".to_string());
        // Showcase: only the chosen statistics panel shows (the general one first).
        let shown = menu::showcase_panel(&panel);
        self.session.hidden.extend(menu::SHOWCASE_PANELS.iter().filter(|p| **p != shown).map(|p| p.to_string()));
        for a in audio_settings::ACCESSORS {
            let level = self.audio.level(a).unwrap_or(audio_settings::MAX_LEVEL);
            self.session.levels.insert(a.to_string(), level);
            // Retail's `(PERCENT)` view of a 0..=127 level (rounding is a **guess**).
            self.session.watch.insert(format!("{a}(PERCENT)"), format!("{}%", (level * 100 + 63) / 127));
        }
        self.session.career = self.career.on;
        if self.factory.on {
            let player = self.player_name();
            self.session.watch.extend(self.factory.watches(&self.locale, self.language, &player));
        }
        if let (true, Some(c)) = (self.career.on, &self.career.career) {
            self.session.choices.extend(c.choices(&self.career.data, &self.quick.cars, &self.quick.tracks, &self.locale, self.language));
            self.session.watch.extend(c.watches(&self.career.data, &self.quick.tracks, &self.locale, self.language));
            // The pre-race screen follows the track picked among the remaining ones: its name, map
            // and record time (a tournament race runs the career's laps, so the record is the lap
            // record times the laps; the footage shows 02:44.00 for Cote d'Azur, twice 01:22).
            let open = c.open_tournaments(&self.career.data);
            let remaining: Vec<i16> = open
                .get(c.chosen_tournament.min(open.len().saturating_sub(1)))
                .map(|t| career::Career::race_tracks(t, &self.career.data).into_iter().skip(c.next_race(t)).collect())
                .unwrap_or_default();
            let pick = self.career.ui.remaining.min(remaining.len().saturating_sub(1));
            if let Some(track) = remaining.get(pick).and_then(|id| self.quick.tracks.iter().find(|k| k.id == *id)) {
                if let Some(at) = self.quick.track_list.iter().position(|&i| self.quick.tracks[i].id == track.id) {
                    self.quick.track = at;
                    self.session.track_art = self.quick.track_art();
                }
                let upper = self.locale.text(track.name_string + 60, self.language).map(|t| t.trim().to_string()).unwrap_or_default();
                self.session.watch.insert("Tournament.Tournament(LOCATION)".into(), upper);
                let record = self.quick.record_of(track.id).map(|t| nfspu_game::catalog::format_ticks(t * career::CAREER_LAPS as u32)).unwrap_or_else(|| "--:--.--".into());
                self.session.watch.insert("Tournament.Tournament(RECORDTIME)".into(), record);
                if let Some(choice) = self.session.choices.get_mut("Tournament.RemainingTracks") {
                    choice.selected = pick;
                    if let Some(menu::Row::Item { text, .. }) = choice.rows.get(pick) {
                        choice.closed = text.clone();
                    }
                }
            }
            let page = if self.career.page.is_empty() { self.page.name.clone() } else { self.career.page.clone() };
            let open_ids: Vec<i16> = c.open_tournaments(&self.career.data).iter().flat_map(|t| career::Career::race_tracks(t, &self.career.data)).collect();
            let view = self.career.ui.view(c, &self.quick.cars, &self.quick.tracks, &open_ids, &self.locale, self.language, &page);
            self.session.choices.extend(view.choices);
            self.session.watch.extend(view.watch);
            self.session.lists = view.lists;
            self.session.hidden.extend(view.hidden);
            self.quick.showcase = view.showcase;
            self.quick.showcase_tweak = view.showcase_tweak;
            self.session.car_logo = self.quick.car_logo();
        } else {
            self.quick.showcase = None;
        }
    }

    /// Notes the page about to show (the shop screens pick their 3D car and logo by it).
    fn note_page(&mut self, name: &str) {
        self.career.page = name.to_string();
        self.career.ui.page = name.to_string();
        self.sync_quick();
    }

    /// Saves the career after a shop action.
    fn save_career(&mut self) {
        if let Some(c) = self.career.career.as_ref() {
            if let Err(e) = c.save_to(&self.career.dir) {
                crate::log_line(&format!("career: could not save: {e}"));
            }
        }
    }

    /// The selected player's name.
    fn player_name(&self) -> String {
        self.profiles.selected.and_then(|i| self.profiles.names.get(i)).map(|n| menu::display_name(n).to_string()).unwrap_or_else(|| "Player".to_string())
    }

    /// Enters Factory Driver for the selected player; true for a new player (retail plays the
    /// welcome screens first).
    fn enter_factory(&mut self) -> bool {
        let player = self.player_name();
        self.career.on = false;
        let new = self.factory.enter(&player);
        self.sync_quick();
        new
    }

    /// Back from a mission race: records the verdict, and opens the promotion screen on a rank-up.
    fn settle_factory(&mut self, v: nfspu_game::factory_live::Verdict) {
        let player = self.player_name();
        let settled = self.factory.settle(&v, &self.locale, self.language, &player);
        self.sync_quick();
        if settled.promoted {
            if let Some(next) = self.load("levelvictory") {
                let snapshot = (self.page.clone(), self.popup.clone());
                self.history.push(snapshot);
                self.page = next;
                self.popup = None;
            }
        }
    }

    /// Enters the Evolution career for the selected player: loads their career file from our own
    /// folder, or starts a new one (written at once). The install and its saves are not touched.
    fn enter_career(&mut self) {
        let player = self.profiles.selected.and_then(|i| self.profiles.names.get(i)).map(|n| menu::display_name(n).to_string()).unwrap_or_else(|| "Player".to_string());
        let dir = self.career.dir.clone();
        let career = match career::Career::load_from(&dir, &player) {
            Some(c) => {
                crate::log_line(&format!("career: loaded '{player}': {} cash, year {}, {} car(s), {} tournament(s) started", career::money(c.cash), c.year, c.cars.len(), c.progress.len()));
                c
            }
            None => {
                // a retail save of this player continues its year, cash and cars (read-only)
                let retail = self.profiles.selected.and_then(|i| self.profiles.names.get(i)).and_then(|stem| std::fs::read(self.save_dir.join(format!("{stem}.sav"))).ok()).and_then(|b| nfspu_game::save_import::read(&b, &self.quick.cars));
                let c = match &retail {
                    Some(p) => {
                        crate::log_line(&format!("career: continuing the retail save of '{player}': year {}, cash {}, {} car(s)", p.year, career::money(p.cash), p.cars.len()));
                        let mut c = career::Career::from_retail(&player, p, &self.quick.cars);
                        c.apply_retail_progress(p, &self.career.data);
                        crate::log_line(&format!("career: retail progress: {} tournament(s) finished, {} mission record(s) done", p.tournaments_done, p.missions.iter().filter(|m| m.done).count()));
                        c
                    }
                    None => career::Career::new(&player),
                };
                match c.save_to(&dir) {
                    Ok(()) => crate::log_line(&format!("career: new profile '{player}' saved to {}", career::Career::path_in(&dir, &player).display())),
                    Err(e) => crate::log_line(&format!("career: could not save {}: {e}", dir.display())),
                }
                c
            }
        };
        self.career.career = Some(career);
        self.career.on = true;
        self.career.pending = None;
        self.sync_quick();
    }

    /// The chosen tournament's next race as a quick-race setup: the track, the chosen owned car,
    /// seven opponents in the tournament's first car (**guess**), the career lap count.
    fn start_career_race(&mut self) -> Option<nfspu_game::race_setup::RaceSetup> {
        let c = self.career.career.as_ref()?;
        let open = c.open_tournaments(&self.career.data);
        let t = (*open.get(c.chosen_tournament.min(open.len().saturating_sub(1)))?).clone();
        let tracks = career::Career::race_tracks(&t, &self.career.data);
        let track_id = *tracks.get(c.next_race(&t))?;
        // The first race of a tournament costs its entry fee.
        let fee = if c.next_race(&t) == 0 { t.entry_fee as i64 } else { 0 };
        if c.cash < fee {
            crate::log_line(&format!("career: not enough credits to enter tournament {} (fee {}, cash {})", t.id, career::money(fee), career::money(c.cash)));
            return None;
        }
        let track = self.quick.tracks.iter().find(|k| k.id == track_id)?.clone();
        let owned = c.cars.get(c.chosen_car.min(c.cars.len().saturating_sub(1)))?.clone();
        let car = self.quick.cars.iter().find(|k| k.id == owned.id)?.clone();
        // The tournament's car rule (**guess**: retail blocks entry with a message; here the race
        // simply does not start and the log says why).
        let rule = self.locale.text(t.restriction, 0).unwrap_or_default();
        if !t.allows(&rule, &car) {
            crate::log_line(&format!("career: '{}' does not meet the rule '{}' of tournament {}", car.name, rule.trim(), t.id));
            return None;
        }
        let mut setup = self.quick.setup(&self.locale, self.language);
        setup.track = track.file.clone();
        setup.track_id = track.id;
        setup.track_name = self.locale.text(track.name_string, self.language).unwrap_or_default().trim().to_string();
        setup.direction = nfspu_game::race_setup::Direction::Forward;
        setup.laps = career::CAREER_LAPS;
        setup.opponents = 7;
        // Career races have damage on (**guess**; the repair screens only make sense with it).
        setup.damage = true;
        // Each opponent has its own car in the tournament record (seven slots).
        setup.opponent_cars = if t.opponent_cars.is_empty() { Default::default() } else { nfspu_game::race_setup::OpponentCars::Cars(t.opponent_cars.clone()) };
        setup.car_id = car.id;
        setup.car_model = car.model.clone();
        setup.car_name = car.name.clone();
        setup.car_style = car.style;
        setup.owned_car = true;
        setup.paint_section = owned.paint;
        setup.custom_colour = None;
        setup.tweak = owned.tweak;
        setup.difficulty = track.difficulty.clamp(0, 2) as u8;
        let at = c.chosen_car.min(c.cars.len().saturating_sub(1));
        setup.tuning = c.tuning_of(at, &career::Shop { cars: &self.quick.cars, parts: &self.career.ui.parts });
        self.career.pending = Some(t.id);
        if fee > 0 {
            if let Some(c) = self.career.career.as_mut() {
                c.cash -= fee;
            }
            self.career.fee_paid = fee;
            crate::log_line(&format!("career: entry fee {} paid", career::money(fee)));
        }
        let c = self.career.career.as_ref()?;
        crate::log_line(&format!("career: race {} of {} in tournament {} on '{}' in '{}'", c.next_race(&t) + 1, tracks.len(), t.id, track.file, car.name));
        Some(setup)
    }

    /// A test drive from the shop screens as a race setup: the car and paint the screen showed, the
    /// chosen location, no rivals and no traffic, the longest lap count (retail's own test drive
    /// could not be used: it gave no control of the car). Nothing is paid or recorded.
    fn start_test_drive(&mut self, page: &str) -> Option<nfspu_game::race_setup::RaceSetup> {
        let c = self.career.career.as_ref()?;
        let open_ids: Vec<i16> = c.open_tournaments(&self.career.data).iter().flat_map(|t| career::Career::race_tracks(t, &self.career.data)).collect();
        let drive = self.career.ui.test_drive(c, &self.quick.cars, &self.quick.tracks, &open_ids, page)?;
        let track = self.quick.tracks.iter().find(|k| k.id == drive.track)?.clone();
        let car = self.quick.cars.get(drive.car)?.clone();
        let tuning = drive.owned.map(|at| c.tuning_of(at, &career::Shop { cars: &self.quick.cars, parts: &self.career.ui.parts }));
        let mut setup = self.quick.setup(&self.locale, self.language);
        setup.track = track.file.clone();
        setup.track_id = track.id;
        setup.track_name = self.locale.text(track.name_string, self.language).unwrap_or_default().trim().to_string();
        setup.direction = nfspu_game::race_setup::Direction::Forward;
        setup.laps = 8;
        setup.opponents = 0;
        setup.traffic = 0;
        setup.car_id = car.id;
        setup.car_model = car.model.clone();
        setup.car_name = car.name.clone();
        setup.car_style = car.style;
        setup.owned_car = drive.owned.is_some();
        setup.paint_section = drive.paint;
        setup.custom_colour = None;
        setup.difficulty = track.difficulty.clamp(0, 2) as u8;
        setup.tuning = tuning.unwrap_or_default();
        // Nothing is paid for a test drive.
        self.career.pending = None;
        // The popup that asked closes, so the screen comes back as it was left.
        self.popup = None;
        crate::log_line(&format!("career: test drive of '{}' on '{}' from {page}", car.name, track.file));
        Some(setup)
    }

    /// Opens the era-change message (`VictoryMessage`) over the current page.
    fn open_era_message(&mut self) {
        self.sync_quick();
        let Some(next) = self.load("VictoryMessage") else { return };
        let snapshot = (self.page.clone(), self.popup.clone());
        self.history.push(snapshot);
        self.popup = Some(next);
        crate::log_line("career: era change message opened");
    }

    /// Back from a race: pays the place and moves the tournament on. `place` 0: not finished.
    fn settle_career_race(&mut self, place: usize, damage: [f32; nfspu_game::car_damage::ZONES]) {
        let Some(id) = self.career.pending.take() else { return };
        let place = if place == 0 { self.career.forced_place.unwrap_or(0) } else { place };
        let Some(t) = self.career.data.iter().find(|t| t.id == id).cloned() else { return };
        let Some(c) = self.career.career.as_mut() else { return };
        // An unfinished race gives the entry fee back.
        let fee = std::mem::take(&mut self.career.fee_paid);
        if place == 0 && fee > 0 {
            c.cash += fee;
            crate::log_line(&format!("career: race not finished, entry fee {} refunded", career::money(fee)));
        }
        // The raced car keeps the damage it took (repaired in the garage's repair screens).
        let raced = c.chosen_car.min(c.cars.len().saturating_sub(1));
        c.keep_race_damage(raced, &damage);
        let payout = c.finish_race(&t, &self.career.data, place);
        crate::log_line(&format!(
            "career: race result place {} paid {} (tournament {} {}, year {}, cash {})",
            payout.place,
            career::money(payout.amount as i64),
            id,
            if payout.tournament_done { "complete" } else { "continues" },
            c.year,
            career::money(c.cash)
        ));
        if let Err(e) = c.save_to(&self.career.dir) {
            crate::log_line(&format!("career: could not save: {e}"));
        }
        if payout.new_year.is_some() {
            c.chosen_tournament = 0;
            self.career.era_message = true;
        }
        self.sync_quick();
    }

    /// Sets a level bar from the pointer's x and keeps it (our settings file, never the save).
    fn drag_level(&mut self, accessor: &str, left: i32, width: i32, x: f32) {
        let t = ((x - left as f32) / width.max(1) as f32).clamp(0.0, 1.0);
        let level = (t * audio_settings::MAX_LEVEL as f32).round() as u32;
        if self.audio.level(accessor) == Some(level) {
            return;
        }
        self.audio.set_level(accessor, level);
        self.audio_changed = true;
        self.reload();
        self.stack();
        self.dirty = true;
    }

    /// Reloads the page and popup, so controls that show the profiles pick up a change.
    fn reload(&mut self) {
        self.sync_quick();
        if let Some(page) = self.load(&self.page.name) {
            self.page = page;
        }
        if let Some(name) = self.popup.as_ref().map(|p| p.name.clone()) {
            self.popup = self.load(&name);
        }
    }

    /// Scrolls the open list by whole rows, kept within its rows (all quick-race lists are at most
    /// 160 units tall, `selectionboxmaxheight`).
    fn scroll_combo(&mut self, accessor: &str, rows: i32) {
        let Some(choice) = self.session.choices.get(accessor) else { return };
        let last = menu::max_scroll(&choice.rows, 160.0) as i32;
        let next = (self.session.scroll as i32 + rows).clamp(0, last) as usize;
        if next != self.session.scroll || rows == 0 {
            self.session.scroll = next;
            self.reload();
            self.stack();
            self.dirty = true;
        }
    }

    /// Starts the open dropdown's list sliding open (or closed); `present` runs it.
    fn slide_combo(&mut self, opening: bool) {
        self.combo_slide = Some((0.0, opening));
        self.session.combo_reveal = if opening { 0.0 } else { 1.0 };
        self.reload();
        self.stack();
        self.dirty = true;
    }

    /// Moves a dropdown slide on by `ms`: 16 ticks of the 128 Hz clock (125 ms) end to end.
    fn advance_slide(&mut self, ms: f32) {
        let Some((elapsed, opening)) = self.combo_slide else { return };
        let elapsed = elapsed + ms;
        let t = (elapsed / (16.0 * 1000.0 / nfspu_game::menu_movie::TICKS_PER_SECOND)).min(1.0);
        self.session.combo_reveal = if opening { t } else { 1.0 - t };
        if t >= 1.0 {
            self.combo_slide = None;
            if !opening {
                self.session.open_combo = None;
            }
            self.session.combo_reveal = 1.0;
        } else {
            self.combo_slide = Some((elapsed, opening));
        }
        self.reload();
        self.stack();
        self.dirty = true;
    }

    /// Reads the selected player's career progress for the demo loop (read-only).
    fn update_progress(&mut self) {
        let save = self
            .profiles
            .selected
            .and_then(|i| self.profiles.names.get(i))
            .and_then(|stem| std::fs::read(self.save_dir.join(format!("{stem}.sav"))).ok());
        // The player's difficulty setting (0 beginner, 1 advanced, 2 expert); default advanced.
        self.session.difficulty = save.as_deref().and_then(|d| menu::save_value(d, "Game.Difficutly")).map(|v| (v as usize).min(2)).unwrap_or(1);
        self.session.open_combo = None;
        // `NFSPU_MUSIC_LEVEL=0..127` overrides it for testing (the save is never written).
        let forced = std::env::var("NFSPU_MUSIC_LEVEL").ok().and_then(|v| v.trim().parse().ok());
        // Our settings file wins; without one the levels start from the player's save.
        self.audio = AudioSettings::load().unwrap_or_else(|| save.as_deref().map(AudioSettings::from_save).unwrap_or_default());
        if let Some(level) = forced {
            self.audio.set_level("Audio.MusicLevel", level);
        }
        self.audio_changed = true;
        crate::log_line(&format!("front end: audio levels {:?} ({})", self.audio, AudioSettings::path().display()));
        let progress = save.as_deref().and_then(nfspu_game::menu_movie::CareerProgress::from_save);
        // Quick-race lists and last choices (tracks open with the career year; 1950 at the start).
        let player = self.profiles.selected.and_then(|i| self.profiles.names.get(i)).cloned().unwrap_or_default();
        self.quick = nfspu_game::quick_race::QuickRace::load(&self.game_dir, save.as_deref(), progress.map(|p| p.year).unwrap_or(1950), &player);
        self.quick.difficulty = self.session.difficulty;
        let q = &self.quick;
        crate::log_line(&format!(
            "front end: quick race for '{player}': {} owned + {} factory car(s), {} opponent car(s), {} track(s) {:?}, choices car {} track {} direction {} laps {} traffic {} opponents {} rule {}",
            q.owned.len(),
            q.factory.len(),
            q.opponent_list.len(),
            q.track_list.len(),
            q.track_list.iter().map(|i| q.tracks[*i].file.as_str()).collect::<Vec<_>>(),
            q.car,
            q.track,
            q.direction,
            q.laps + 1,
            q.traffic,
            q.opponents,
            q.opponent_car
        ));
        crate::log_line(&format!("front end: career progress {progress:?}, demo loop {:?}, difficulty {}", nfspu_game::menu_movie::playlist("demoloop", progress), self.session.difficulty));
        self.art.set_progress(progress);
    }

    /// A screen that has no room of its own opens as a popup over the page (matches the retail
    /// start-up, where `selectname` sits over `GameSetup`, and a retail recording of
    /// `singleplayer` opening over it).
    fn is_popup(screen: &Screen) -> bool {
        screen.room.eq_ignore_ascii_case("ID_NONE")
    }

    /// A page with a 3D room behind it (`ID_GARAGE`, `ID_TRACK`). The retail recording shows the
    /// bottom bar on the quick-race garage page, which has no frame; drawing the bar on every room
    /// page is a **guess** beyond that.
    fn has_room(screen: &Screen) -> bool {
        let room = screen.room.trim();
        !room.is_empty() && !room.eq_ignore_ascii_case("ID_NONE")
    }

    /// The RACE button: builds the race from the quick-race choices and logs it (also to
    /// `logs/race_setup.txt`, a log only: the race gets the setup in memory).
    fn start_race(&self) -> nfspu_game::race_setup::RaceSetup {
        let setup = self.quick.setup(&self.locale, self.language);
        self.note_knockout();
        let text = setup.to_text();
        crate::log_line(&format!("front end: RACE pressed, setup: {}", text.lines().filter(|l| !l.starts_with('#')).collect::<Vec<_>>().join("; ")));
        if let Some(path) = crate::log_path().parent().map(|d| d.join("race_setup.txt")) {
            match std::fs::write(&path, text) {
                Ok(()) => crate::log_line(&format!("front end: wrote {}", path.display())),
                Err(e) => crate::log_line(&format!("front end: could not write {}: {e}", path.display())),
            }
        }
        setup
    }

    /// RACE on a knockout screen starts a knockout run: `knockout` (Series) or `quickknockout`
    /// (Quick). `NFSPU_KNOCKOUT=quick|series[,opponents]` forces one from any screen (tests).
    fn note_knockout(&self) {
        use nfspu_game::knockout::{self, Mode, Pending};
        let page = self.popup.as_ref().unwrap_or(&self.page).name.to_ascii_lowercase();
        let env = std::env::var("NFSPU_KNOCKOUT").ok();
        let mut env_parts = env.as_deref().unwrap_or("").split(',');
        let mode = match (page.as_str(), env_parts.next().unwrap_or("")) {
            ("quickknockout", _) | (_, "quick") => Some(Mode::Quick),
            ("knockout", _) | (_, "series") => Some(Mode::Series),
            _ => None,
        };
        let Some(mode) = mode else { return knockout::set_pending(None) };
        let opponents = env_parts.next().and_then(|n| n.parse().ok()).unwrap_or_else(|| self.quick.knockout_opponents());
        // Series: the chosen track first, then random others, each different from the last
        // (the screen's "make random" fills the rest; the 7 are a guess).
        let files: Vec<String> = self.quick.track_list.iter().map(|&i| self.quick.tracks[i].file.clone()).collect();
        let mut tracks = vec![files.get(self.quick.track).cloned().unwrap_or_default()];
        let mut seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(7, |d| d.subsec_nanos() as usize);
        while tracks.len() < 7 && files.len() > 1 {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            let pick = &files[(seed >> 8) % files.len()];
            // knockout races are on roads ("linear tracks"); circuits belong to quick knockout
            if tracks.last() != Some(pick) && !knockout::is_circuit(pick) {
                tracks.push(pick.clone());
            }
        }
        if let Ok(list) = std::env::var("NFSPU_KO_TRACKS") {
            tracks = list.split(',').map(|t| t.trim().to_string()).collect();
        }
        crate::log_line(&format!("front end: knockout {mode:?}, {opponents} opponent(s)"));
        knockout::set_pending(Some(Pending { mode, opponents, tracks }));
    }

    /// The top screen offers RACE: the bottom bar is up and the screen allows playing.
    fn can_race(&self) -> bool {
        self.layers.iter().any(|l| l.name.eq_ignore_ascii_case(BOTTOM_BAR)) && self.popup.as_ref().unwrap_or(&self.page).can_play
    }

    /// Rebuilds the layer list from the page and popup.
    fn stack(&mut self) {
        let mut layers = Vec::new();
        let mut interactive = Vec::new();
        if let Some(frame) = self.page.frame.clone().and_then(|f| self.load(&f)) {
            layers.push(frame);
            interactive.push(false);
        }
        // Under a popup the page is drawn in its disabled state: faint buttons that take no clicks.
        // Matches retail captures; the executable's disabled check also counts a control as
        // disabled when its menu is inactive, which is the likely mechanism.
        let page = if self.popup.is_some() { self.load_as(&self.page.name, true).unwrap_or_else(|| self.page.clone()) } else { self.page.clone() };
        layers.push(page);
        interactive.push(self.popup.is_none());
        if let Some(popup) = &self.popup {
            layers.push(popup.clone());
            interactive.push(true);
        }
        if self.page.frame.is_some() || FrontEnd::has_room(&self.page) {
            // "back" is grey at the root screen; RACE follows the top screen's `canplaygame`.
            let can_play = self.popup.as_ref().unwrap_or(&self.page).can_play;
            let session = menu::Session { can_go_back: !self.history.is_empty(), can_play, ..self.session.clone() };
            if let Some(bar) = menu::load_screen_with(&self.game_dir, BOTTOM_BAR, &self.locale, self.language, &self.templates, &self.profiles, false, &session) {
                layers.push(bar);
                interactive.push(true);
            }
        }
        for layer in &layers {
            log_screen(layer);
        }
        self.layers = layers;
        self.interactive = interactive;
    }

    /// Starts the animation from the layers shown before a change (`before`) to the current ones:
    /// layers that are no longer there go off, new ones come on (by name; a page kept under a
    /// new popup stays put, and so does the bottom bar). Nothing to animate: no transition.
    /// `NFSPU_MENU_NOTRANS=1` turns transitions off.
    fn begin_transition(&mut self, before: Vec<(Screen, bool)>) {
        if std::env::var("NFSPU_MENU_NOTRANS").as_deref() == Ok("1") {
            return;
        }
        let now: Vec<&str> = self.layers.iter().map(|l| l.name.as_str()).collect();
        let was: Vec<String> = before.iter().map(|(l, _)| l.name.clone()).collect();
        let leaving: Vec<(Screen, bool, bool)> = before.into_iter().map(|(l, on)| {
            let goes = !now.iter().any(|n| n.eq_ignore_ascii_case(&l.name));
            (l, on, goes)
        }).collect();
        let coming: Vec<String> = now.iter().filter(|n| !was.iter().any(|w| w.eq_ignore_ascii_case(n))).map(|n| n.to_string()).collect();
        let any_off = leaving.iter().any(|(_, _, goes)| *goes);
        if !any_off && coming.is_empty() {
            return;
        }
        crate::log_line(&format!(
            "menu: transition off {:?} on {:?}",
            leaving.iter().filter(|l| l.2).map(|l| l.0.name.as_str()).collect::<Vec<_>>(),
            coming
        ));
        self.transition = Some(Transition { leaving, coming, going_off: any_off, wait: TRANSITION_WAIT_FRAMES, ms: 0.0 });
    }

    /// Moves a screen change on by one frame of `ms`. True while one is running.
    fn advance_transition(&mut self, ms: f32) -> bool {
        let Some(t) = &mut self.transition else { return false };
        if t.wait > 0 {
            t.wait -= 1;
        } else {
            t.ms += ms;
        }
        if t.ms >= TRANSITION_MS {
            if t.going_off && !t.coming.is_empty() {
                t.going_off = false;
                t.wait = TRANSITION_WAIT_FRAMES;
                t.ms = 0.0;
            } else {
                self.transition = None;
            }
        }
        self.dirty = true;
        true
    }

    /// The layers to draw now: during a screen change, the leaving or the coming screens with
    /// each moving layer marked (`Screen::moving`); otherwise the current ones.
    fn shown_layers(&self) -> Vec<(Screen, bool)> {
        let Some(t) = &self.transition else {
            return self.layers.iter().cloned().zip(self.interactive.iter().copied()).collect();
        };
        let away = (t.ms / TRANSITION_MS).clamp(0.0, 1.0);
        if t.going_off {
            t.leaving.iter().map(|(l, on, goes)| (Screen { moving: goes.then_some((true, away)), ..l.clone() }, *on)).collect()
        } else {
            self.layers
                .iter()
                .zip(&self.interactive)
                .map(|(l, on)| (Screen { moving: t.coming.iter().any(|c| c.eq_ignore_ascii_case(&l.name)).then_some((false, 1.0 - away)), ..l.clone() }, *on))
                .collect()
        }
    }

    /// What a click does: the topmost interactive layer with an action under the pointer.
    fn hit_at(&self, x: f32, y: f32) -> Option<Hit> {
        self.layers.iter().zip(&self.interactive).rev().filter(|(_, on)| **on).find_map(|(s, _)| s.hit_at(x, y))
    }

    /// The colour around the 640x480 image (window bars): black while the room is not drawn,
    /// otherwise the page's.
    fn clear_colour(&self) -> Color {
        let room = self.page.room.trim();
        if room.is_empty() || room.eq_ignore_ascii_case("ID_NONE") {
            to_color(self.page.background)
        } else {
            Color::BLACK
        }
    }

    /// The colour the 640x480 image starts from, by the same rule; see-through over a room that
    /// is drawn (the garage), so the 3D scene shows wherever the page draws nothing.
    fn backdrop(&self) -> [f32; 4] {
        let room = self.page.room.trim();
        if room.is_empty() || room.eq_ignore_ascii_case("ID_NONE") {
            self.page.background
        } else if nfspu_game::garage::room_for(room).is_some() {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            [0.0, 0.0, 0.0, 1.0]
        }
    }
}

/// The single node that shows the composed 640x480 front end.
#[derive(Component)]
struct MenuVisual;

/// Front-end sound effects from `GameData/Sounds/fesfx.bnk`. The retail sound call's number n
/// plays **slot n + 1**: matched against a retail recording with sound, where the click sound
/// (call 5, buttons and dropdown choices) is slot 6 at all eight clicks. Seen in the code: 2 a
/// dropdown opens, 3 it closes from a click elsewhere (both **not confirmed** by ear: too quiet
/// under the music in the recording), 5 a button or a dropdown choice.
#[derive(Resource, Default)]
struct Sounds {
    bank: Option<Vec<u8>>,
    loaded: std::collections::HashMap<usize, Option<Handle<AudioSource>>>,
    /// The Options > Audio effects level (0..1) the menu sounds play at.
    level: f32,
}

/// Sound call numbers (bank slot = number + 1). From two retail recordings: 0 screen change
/// (slot 1, three times), 2 dropdown opens (slot 3, twice), 5 click (slot 6, every click), 6 the
/// pointer moves onto another control (slot 7, 41 times). 3 (close from a click elsewhere) is
/// from the code only.
const SOUND_SCREEN: usize = 0;
const SOUND_OPEN: usize = 2;
const SOUND_CLOSE: usize = 3;
const SOUND_SELECT: usize = 5;
const SOUND_HOVER: usize = 6;

impl Sounds {
    fn load(game_dir: &std::path::Path) -> Sounds {
        let bank = std::fs::read(game_dir.join("GameData").join("Sounds").join("fesfx.bnk")).ok();
        if bank.is_none() {
            crate::log_line("front end: fesfx.bnk not found, no menu sounds");
        }
        Sounds { bank, loaded: Default::default(), level: 1.0 }
    }

    /// Plays slot `n` once (decoded on first use).
    fn play(&mut self, n: usize, commands: &mut Commands, assets: &mut Assets<AudioSource>) {
        let bank = &self.bank;
        let handle = self.loaded.entry(n).or_insert_with(|| {
            let data = bank.as_ref()?;
            let patch = nfspu_formats::bnk::parse(data).ok()?.get(n + 1)?.clone()?;
            let pcm = nfspu_formats::bnk::decode(data, &patch, nfspu_formats::bnk::Interleave::Planar);
            let wav = nfspu_formats::bnk::wav(&pcm, patch.channels.max(1), patch.sample_rate);
            Some(assets.add(AudioSource { bytes: wav.into() }))
        });
        if let Some(handle) = handle.clone() {
            commands.spawn((AudioPlayer(handle), PlaybackSettings::DESPAWN.with_volume(bevy::audio::Volume::Linear(self.level))));
        }
    }
}

/// Marks the entity playing the current song.
#[derive(Component)]
struct MusicTrack;

/// The front-end music player: `nfspu_game::menu_music` decides what plays, this streams it from
/// `GameData/Music/zzzymus.viv` (decoded whole when a song starts).
#[derive(Resource)]
struct Music {
    archive: PathBuf,
    entries: Vec<nfspu_formats::viv::Entry>,
    jukebox: nfspu_game::menu_music::Jukebox,
    /// The screen the music was last chosen for.
    screen: String,
    playing: Option<Entity>,
    /// A fade in progress: elapsed ms, total ms, and the song (archive index) that follows it.
    fade: Option<(f32, f32, Option<usize>)>,
    /// A song was spawned this frame (its entity exists only once commands apply).
    just_started: bool,
}

impl Music {
    fn load(game_dir: &std::path::Path) -> Music {
        use std::io::Read;
        let archive = game_dir.join("GameData").join("Music").join("zzzymus.viv");
        let entries = (|| {
            let mut file = std::fs::File::open(&archive).ok()?;
            let mut first = [0u8; 16];
            file.read_exact(&mut first).ok()?;
            let mut head = vec![0u8; nfspu_formats::viv::header_size(&first).ok()?];
            std::io::Seek::rewind(&mut file).ok()?;
            file.read_exact(&mut head).ok()?;
            nfspu_formats::viv::index(&head).ok()
        })()
        .unwrap_or_default();
        crate::log_line(&format!("front end: {} song(s) in {}", entries.len(), archive.display()));
        let names = entries.iter().map(|e| e.name.clone()).collect();
        Music { archive, entries, jukebox: nfspu_game::menu_music::Jukebox::new(names), screen: String::new(), playing: None, fade: None, just_started: false }
    }

    /// Decodes song `n` and starts it at `volume` (linear).
    fn start(&mut self, n: usize, volume: f32, commands: &mut Commands, assets: &mut Assets<AudioSource>) {
        let Some(entry) = self.entries.get(n) else { return };
        let t = std::time::Instant::now();
        let audio = std::fs::File::open(&self.archive)
            .ok()
            .and_then(|mut f| nfspu_formats::viv::read_entry(&mut f, entry).ok())
            .and_then(|data| nfspu_formats::asf::decode(&data).ok());
        let Some(audio) = audio else {
            crate::log_line(&format!("front end: could not decode song '{}'", entry.name));
            return;
        };
        crate::log_line(&format!(
            "front end: music '{}' ({:.1} s, {} Hz), decoded in {:.0} ms, volume {volume:.2}",
            entry.name,
            audio.samples.len() as f64 / audio.channels.max(1) as f64 / audio.sample_rate as f64,
            audio.sample_rate,
            t.elapsed().as_secs_f64() * 1000.0
        ));
        let wav = nfspu_formats::bnk::wav(&audio.samples, audio.channels, audio.sample_rate);
        let handle = assets.add(AudioSource { bytes: wav.into() });
        let settings = PlaybackSettings::DESPAWN.with_volume(bevy::audio::Volume::Linear(volume));
        self.playing = Some(commands.spawn((AudioPlayer(handle), settings, MusicTrack)).id());
        self.just_started = true;
    }
}

/// Copies changed menu levels into the `AudioSettings` resource the sound systems read.
fn sync_audio(mut state: ResMut<FrontEnd>, mut settings: ResMut<AudioSettings>, sounds: Option<ResMut<Sounds>>) {
    if std::mem::take(&mut state.audio_changed) && *settings != state.audio {
        *settings = state.audio;
    }
    if let Some(mut sounds) = sounds {
        if sounds.level != settings.sfx {
            sounds.level = settings.sfx;
        }
    }
}

/// A fresh random number for song picks (retail mixes the clock in too).
fn random_u32() -> u32 {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let mut x = (nanos as u64) ^ 0x9E37_79B9_7F4A_7C15;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    (x >> 32) as u32
}

/// Changes the music when the active screen asks for different songs, fades, and moves on to the
/// next song when one ends.
fn play_music(
    mut music: ResMut<Music>,
    state: Res<FrontEnd>,
    app_state: Res<State<AppState>>,
    time: Res<Time>,
    mut commands: Commands,
    mut assets: ResMut<Assets<AudioSource>>,
    mut sinks: Query<&mut AudioSink, With<MusicTrack>>,
    tracks: Query<(), With<MusicTrack>>,
    settings: Res<AudioSettings>,
) {
    use bevy::audio::{AudioSinkPlayback, Volume};
    use nfspu_game::menu_music::Cue;
    // How retail maps a level to loudness is not known; linear level/127.
    let volume = settings.music;
    let top = state.popup.as_ref().unwrap_or(&state.page);
    // While the race loads the music only finishes its fade (`leave_menu`).
    // Options is `NONE`, but its audio tab plays the CD player's selected song, the archive's
    // first (retail recording; the playlist itself is not built, and the other tabs staying
    // silent is a **guess**).
    let audio_tab = top.name.eq_ignore_ascii_case("Options") && state.session.panel == AUDIO_PANEL;
    let key = if audio_tab { format!("{}/{}", top.name, AUDIO_PANEL) } else { top.name.clone() };
    if *app_state.get() == AppState::Menu && key != music.screen {
        music.screen = key;
        let main_menu = top.name.eq_ignore_ascii_case("GameSetup");
        let pattern = if audio_tab { music.jukebox.name(0).to_string() } else { top.music.clone() };
        if let Some(Cue::Change { fade_ms, next }) = music.jukebox.enter_screen(&pattern, main_menu, time.elapsed_secs_f64(), random_u32()) {
            crate::log_line(&format!("front end: screen '{}' music '{pattern}' -> {:?}", top.name, next.map(|n| music.jukebox.name(n).to_string())));
            if music.playing.is_some() {
                let elapsed = music.fade.map(|f| f.0).unwrap_or(0.0);
                music.fade = Some((elapsed, fade_ms, next));
            } else if let Some(n) = next {
                music.start(n, volume, &mut commands, &mut assets);
            }
        }
    }
    if let Some((elapsed, total, next)) = music.fade {
        let elapsed = elapsed + time.delta_secs() * 1000.0;
        if elapsed >= total || music.playing.is_none() {
            if let Some(e) = music.playing.take() {
                if let Ok(mut entity) = commands.get_entity(e) {
                    entity.despawn();
                }
            }
            music.fade = None;
            if let Some(n) = next {
                music.start(n, volume, &mut commands, &mut assets);
            }
        } else {
            for mut sink in &mut sinks {
                sink.set_volume(Volume::Linear(volume * (1.0 - elapsed / total)));
            }
            music.fade = Some((elapsed, total, next));
        }
        return;
    }
    // The music slider changes the playing song at once.
    if settings.is_changed() {
        for mut sink in &mut sinks {
            sink.set_volume(Volume::Linear(volume));
        }
    }
    // A finished song despawns itself: play the next one.
    if std::mem::take(&mut music.just_started) {
        return;
    }
    if let Some(e) = music.playing {
        if tracks.get(e).is_err() {
            music.playing = None;
            if let Some(n) = music.jukebox.song_ended() {
                music.start(n, volume, &mut commands, &mut assets);
            }
        }
    }
}

/// The window title while the front end shows.
pub const TITLE: &str = "NFS Porsche Unleashed (Rust rewrite) - front end - M6";

/// The front end: with `Some(screen)` its screens are read at start-up and the app starts there.
pub struct MenuPlugin(pub Option<String>);

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(menu_garage::MenuGarage)
            .add_systems(OnEnter(AppState::Menu), enter_menu.run_if(resource_exists::<FrontEnd>))
            .add_systems(OnExit(AppState::Menu), leave_menu.run_if(resource_exists::<FrontEnd>))
            .add_systems(OnEnter(AppState::Race), stop_music.run_if(resource_exists::<Music>))
            .add_systems(Update, (click, present, capture, flow_test_menu, quit_on_escape).chain().run_if(in_state(AppState::Menu)).run_if(resource_exists::<FrontEnd>))
            .add_systems(Update, (sync_audio, play_music).chain().run_if(resource_exists::<FrontEnd>).run_if(menu_or_loading));
        let Some(start) = &self.0 else { return };
        let game_dir = PathBuf::from(std::env::var("NFSPU_GAME_DIR").unwrap_or_default());
        match front_end(game_dir, start) {
            Some(state) => {
                app.insert_resource(ClearColor(state.clear_colour()))
                    .insert_resource(Capture::from_env())
                    .insert_resource(Sounds::load(&state.game_dir))
                    .insert_resource(Music::load(&state.game_dir))
                    .insert_resource(state);
            }
            None => {
                app.add_systems(Startup, |mut exit: MessageWriter<AppExit>| {
                    exit.write(AppExit::error());
                });
            }
        }
    }
}

/// The menu music plays on the menu and finishes its fade while the race loads.
fn menu_or_loading(state: Res<State<AppState>>) -> bool {
    matches!(state.get(), AppState::Menu | AppState::Loading)
}

/// Reads the front end's data and builds its first screen (`start`: `page` or `page+popup`).
fn front_end(game_dir: PathBuf, start: &str) -> Option<FrontEnd> {
    let locale = std::fs::read(game_dir.join("FEData").join("Locale").join("festrings.csv"))
        .map(|b| Locale::parse_csv_bytes(&b))
        .unwrap_or_default();
    let templates = menu::load_templates(&game_dir);
    let language = nfspu_game::refbuild::Settings::parse(
        &std::fs::read(game_dir.join("nfs5.ini")).map(|b| String::from_utf8_lossy(&b).to_string()).unwrap_or_default(),
    )
    .language
    .id();

    crate::log_line(&format!(
        "front end: {} templates, {} locale strings, language column {language}, starting at '{start}'",
        templates.len(),
        locale.len()
    ));

    // Saved players: the portable build keeps them in `<game>\savedata\`; `NFSPU_SAVE_DIR` points
    // elsewhere (for a test copy, so the install is never written).
    let save_dir = std::env::var_os("NFSPU_SAVE_DIR").map(PathBuf::from).unwrap_or_else(|| game_dir.join("SaveData"));
    let profiles = menu::Profiles::load(&save_dir);
    crate::log_line(&format!("front end: {} saved player(s) in {}, selected {:?}", profiles.names.len(), save_dir.display(), profiles.selected));

    // `page+popup` starts with a popup open over the page (`GameSetup+singleplayer`); `+none`
    // starts with no popup even where one opens by default.
    let (page_name, popup_name) = match start.split_once('+') {
        Some((page, popup)) => (page.to_string(), Some(popup.to_string())),
        None => (start.to_string(), None),
    };
    let Some(screen) = menu::load_screen_with(&game_dir, &page_name, &locale, language, &templates, &profiles, false, &menu::Session::default()) else {
        eprintln!("could not load screen '{page_name}' (is NFSPU_GAME_DIR a game folder?)");
        crate::log_line(&format!("front end: could not load screen '{page_name}', quitting"));
        return None;
    };

    let mut state = FrontEnd {
        game_dir: game_dir.clone(),
        locale,
        templates,
        language,
        page: screen,
        popup: None,
        history: Vec::new(),
        layers: Vec::new(),
        interactive: Vec::new(),
        laid_out_for: Vec2::ZERO,
        art: menu_canvas::ArtCache::new(game_dir.clone()),
        hover_key: None,
        dirty: true,
        compose_ms: Vec::new(),
        profiles,
        save_dir: save_dir.clone(),
        session: menu::Session::default(),
        combo_slide: None,
        last_hover: None,
        audio: AudioSettings::default(),
        audio_changed: true,
        dragging: None,
        help: Default::default(),
        tip: Vec::new(),
        quick: Default::default(),
        transition: None,
        anim_key: Vec::new(),
        clock_ms: 0.0,
        career: CareerMode::load(&game_dir),
        factory: crate::menu_factory::FactoryMode::load(&game_dir),
    };
    // Read the selected player's save first: screens built below show its settings, and the
    // start page (built before it) is rebuilt.
    state.update_progress();
    // The career screens can be started directly (tests and captures): `NFSPU_MENU=tournament`.
    // `NFSPU_CAREER=1` does the same for any page (the shop screens); `NFSPU_CAREER_CASH=n` and
    // `NFSPU_CAREER_YEAR=y` set the account and the year for such a test.
    // The Factory Driver screens can be started directly: `NFSPU_MENU=factorydriver`.
    if ["factorydriver", "missionbrief", "missionmap", "factfirst", "factwelcome", "levelvictory"].iter().any(|n| page_name.eq_ignore_ascii_case(n)) {
        state.enter_factory();
        state.reload();
    }
    if ["tournament", "prerace"].iter().any(|n| page_name.eq_ignore_ascii_case(n)) || std::env::var("NFSPU_CAREER").as_deref() == Ok("1") {
        state.enter_career();
        state.career.page = page_name.to_string();
        if let Some(c) = state.career.career.as_mut() {
            if let Some(cash) = std::env::var("NFSPU_CAREER_CASH").ok().and_then(|v| v.trim().parse().ok()) {
                c.cash = cash;
            }
            if let Some(year) = std::env::var("NFSPU_CAREER_YEAR").ok().and_then(|v| v.trim().parse().ok()) {
                c.year = year;
            }
        }
        // `NFSPU_CAREER_DO=a;b;c` presses shop buttons in order on the start page (a test hook):
        // `BUYCAR`, `SELLCAR`, `PURCHASECALLBACK` ..., `pick:Accessor=n` for a dropdown choice and
        // `row:Accessor=n` for a list row.
        if let Ok(script) = std::env::var("NFSPU_CAREER_DO") {
            for step in script.split(';').map(str::trim).filter(|x| !x.is_empty()) {
                let st = &mut state;
                let Some(c) = st.career.career.as_mut() else { break };
                if step == "ERA" {
                    // Completes every regular tournament of the era (test hook for era progression).
                    let data = st.career.data.clone();
                    let era = career::era_of_year(c.year);
                    for t in data.iter().filter(|t| t.era == era && !t.bonus) {
                        for _ in 0..career::Career::race_tracks(t, &data).len() {
                            let p = c.finish_race(t, &data, 1);
                            if let Some(y) = p.new_year {
                                crate::log_line(&format!("career: era advance to {y} (cash {})", career::money(c.cash)));
                                st.career.era_message = true;
                            }
                        }
                    }
                    continue;
                }
                if let Some((kind, rest)) = step.split_once(':') {
                    let Some((accessor, n)) = rest.split_once('=') else { continue };
                    let n = n.trim().parse().unwrap_or(0);
                    let done = if kind == "pick" { st.career.ui.pick(accessor.trim(), n, c, &st.quick.cars) } else { st.career.ui.list_pick(accessor.trim(), n) };
                    crate::log_line(&format!("career: test step {step} -> {done}"));
                } else {
                    let line = st.career.ui.callback(step, &page_name, c, &st.quick.cars);
                    crate::log_line(&format!("career: test step {step} -> {}", line.unwrap_or_else(|| "not a shop button".into())));
                }
            }
            state.save_career();
        }
        state.reload();
        if std::mem::take(&mut state.career.era_message) {
            state.open_era_message();
        }
    }
    state.reload();
    match popup_name.as_deref() {
        Some(name) if name.eq_ignore_ascii_case("none") => {}
        Some(name) => {
            state.popup = state.load(name);
            if state.career.on {
                state.note_page(name);
                state.reload();
            }
        }
        // Default: the main menu with the player picker over it, as retail shows at first launch
        // (per the author). Later start-ups open the main menu alone (retail recording); the
        // picker is also linked from "load game". `GameSetup+none` starts without it.
        None if state.page.name.eq_ignore_ascii_case("GameSetup") => state.popup = state.load("selectname"),
        None => {}
    }
    // For captures: `NFSPU_MENU_COMBO=<accessor>` starts with that dropdown open.
    if let Ok(accessor) = std::env::var("NFSPU_MENU_COMBO") {
        state.session.open_combo = Some(accessor);
        state.reload();
    }
    // For captures: `NFSPU_MENU_PICK=<accessor>=<item>[;...]` makes choices before the first
    // frame (`zMergedList.Car=5;Track1.Track=2`).
    if let Ok(picks) = std::env::var("NFSPU_MENU_PICK") {
        for pick in picks.split(';') {
            if let Some((accessor, item)) = pick.split_once('=') {
                let ok = item.trim().parse().map(|i| state.quick.pick(accessor.trim(), i)).unwrap_or(false);
                crate::log_line(&format!("front end: pick {pick}: {}", if ok { "ok" } else { "not a quick-race choice" }));
            }
        }
        state.reload();
    }
    state.stack();
    Some(state)
}

/// The front end comes up (at start-up, or back from a race at the screen RACE was pressed on):
/// its UI camera and image, the clear colour and the title (the garage's camera: `menu_garage`).
fn enter_menu(
    mut commands: Commands,
    mut state: ResMut<FrontEnd>,
    mut result: ResMut<flow::LastResult>,
    mut clear: ResMut<ClearColor>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    // UI is drawn by a camera, so one has to exist even though the screen is one image (the
    // garage room comes in as a texture, see `menu_garage`).
    commands.spawn((Camera2d::default(), IsDefaultUiCamera, DespawnOnExit(AppState::Menu)));
    let mut node = Node::default();
    if let Ok(mut window) = windows.single_mut() {
        crate::log_line(&format!("front end: window {:.0}x{:.0} logical, camera spawned", window.width(), window.height()));
        place(&mut node, &window);
        window.title = TITLE.to_string();
    }
    commands.spawn((MenuVisual, node, ImageNode::default(), DespawnOnExit(AppState::Menu)));
    // A Factory Driver mission that just ended is judged before the screens are rebuilt.
    if let Some(v) = nfspu_game::factory_live::take_verdict() {
        state.settle_factory(v);
    } else {
        nfspu_game::factory_live::set_active(None);
    }
    // A career race that just ended is paid before the screens are rebuilt.
    if state.career.pending.is_some() {
        let place = std::mem::take(&mut result.place);
        let damage = std::mem::take(&mut result.damage);
        state.settle_career_race(place, damage);
    }
    // Screens are rebuilt (the page and popup as they were) and drawn afresh.
    state.reload();
    // The era just ended: the era-change message comes up over the tournament screen.
    if std::mem::take(&mut state.career.era_message) {
        state.open_era_message();
    }
    state.stack();
    // The screens come on as on any screen change (retail's menus start in their "coming on"
    // state).
    state.begin_transition(Vec::new());
    state.laid_out_for = Vec2::ZERO;
    state.hover_key = None;
    state.dirty = true;
    clear.0 = state.clear_colour();
}

/// The front end goes for a race: the menu music fades out as on a screen with no music
/// (`menu_music`); the screens and choices stay as they are for the way back.
fn leave_menu(music: Option<ResMut<Music>>, mut images: ResMut<Assets<Image>>, visual: Query<&ImageNode, With<MenuVisual>>, time: Res<Time>) {
    for image in &visual {
        images.remove(&image.image);
    }
    let Some(mut music) = music else { return };
    music.screen = RACE_SCREEN.to_string();
    if let Some(nfspu_game::menu_music::Cue::Change { fade_ms, next }) = music.jukebox.enter_screen("NONE", false, time.elapsed_secs_f64(), random_u32()) {
        if music.playing.is_some() {
            let elapsed = music.fade.map(|f| f.0).unwrap_or(0.0);
            music.fade = Some((elapsed, fade_ms, next));
        }
        crate::log_line(&format!("front end: race starting, music fades out over {fade_ms:.0} ms"));
    }
}

/// What the music remembers as its screen during a race (no layout has this name).
const RACE_SCREEN: &str = "<race>";

/// The race is on screen: a menu song still fading out stops.
fn stop_music(mut music: ResMut<Music>, mut commands: Commands) {
    if let Some(e) = music.playing.take() {
        if let Ok(mut entity) = commands.get_entity(e) {
            entity.despawn();
        }
        crate::log_line("front end: menu music stopped for the race");
    }
    music.fade = None;
}

/// `NFSPU_FLOW_TEST`: presses RACE a few seconds after the menu shows (on the next track after the
/// first race), and quits once every race was run.
fn flow_test_menu(
    time: Res<Time>,
    mut test: ResMut<flow::FlowTest>,
    mut state: ResMut<FrontEnd>,
    mut commands: Commands,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
) {
    if !test.on() {
        return;
    }
    let now = time.elapsed_secs();
    let since = *test.since.get_or_insert(now);
    // the garage room and its car take a few seconds to appear
    if now - since < 5.0 {
        return;
    }
    test.since = None;
    if test.done >= test.races {
        // A picture of the menu as it came back, then quit.
        let path = crate::log_path().with_file_name("flow_menu_after.png");
        crate::log_line(&format!("flow test: {} race(s) done, saving {} and quitting", test.done, path.display()));
        test.races = 0;
        commands.spawn(Screenshot::primary_window()).observe(move |captured: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
            let saved = captured.image.clone().try_into_dynamic().map_err(|e| e.to_string()).and_then(|i| i.to_rgb8().save(&path).map_err(|e| e.to_string()));
            crate::log_line(&format!("flow test: menu picture {saved:?}"));
            exit.write(AppExit::Success);
        });
        return;
    }
    let test_drive = state.career.on && std::env::var("NFSPU_CAREER_TESTDRIVE").as_deref() == Ok("1");
    if !state.can_race() && !test_drive {
        crate::log_line(&format!("flow test: RACE is not offered on '{}', quitting", state.page.name));
        exit.write(AppExit::error());
        return;
    }
    if test.done > 0 && !state.career.on && !state.factory.on {
        state.quick.step_track(1);
        state.reload();
        state.stack();
    }
    crate::log_line(&format!("flow test: pressing RACE for race {}", test.done + 1));
    // `NFSPU_CAREER_TESTDRIVE=1` drives the chosen car on a test drive instead of racing.
    let setup = if state.career.on && std::env::var("NFSPU_CAREER_TESTDRIVE").as_deref() == Ok("1") {
        let page = state.career.page.clone();
        state.start_test_drive(&page)
    } else if state.career.on {
        state.start_career_race()
    } else if state.factory.on {
        let player = state.player_name();
        let st = &mut *state;
        st.factory.start(&st.quick, &st.locale, st.language, &player)
    } else {
        None
    };
    flow::start_race_from_menu(setup.unwrap_or_else(|| state.start_race()), &mut commands, &mut next);
}

fn to_color(rgba: [f32; 4]) -> Color {
    Color::srgba(rgba[0], rgba[1], rgba[2], rgba[3])
}

fn log_screen(screen: &Screen) {
    let count = |f: fn(&Draw) -> bool| screen.draw.iter().filter(|d| f(d)).count();
    let quads = count(|d| matches!(d, Draw::Quad { .. }));
    let texts = count(|d| matches!(d, Draw::Text { .. }));
    let gradients = count(|d| matches!(d, Draw::Gradient { .. }));
    let pictures = count(|d| matches!(d, Draw::Picture { .. }));
    let mut skipped: BTreeMap<&str, usize> = BTreeMap::new();
    for s in &screen.skipped {
        *skipped.entry(s.as_str()).or_default() += 1;
    }
    let list: Vec<String> = skipped.iter().map(|(k, v)| format!("{k} x{v}")).collect();
    crate::log_line(&format!(
        "screen '{}': {quads} quad(s), {texts} text(s), {gradients} gradient(s), {pictures} picture(s), {} control(s) not drawn [{}]",
        screen.name,
        screen.skipped.len(),
        list.join(", ")
    ));
}

/// How the 640x480 layout maps onto the window: a uniform scale to fit the height, centred.
fn project(window: &Window) -> (f32, Vec2) {
    let scale = window.height() / SCREEN_HEIGHT;
    let width = SCREEN_WIDTH * scale;
    let height = SCREEN_HEIGHT * scale;
    (scale, Vec2::new((window.width() - width) / 2.0, (window.height() - height) / 2.0))
}

/// The pointer in layout units. For captures without a mouse, `NFSPU_MENU_POINTER=x,y` places it.
fn pointer(window: &Window) -> Vec2 {
    let (scale, offset) = project(window);
    let cursor = window.cursor_position().unwrap_or(Vec2::new(-1000.0, -1000.0));
    std::env::var("NFSPU_MENU_POINTER")
        .ok()
        .and_then(|v| menu::eval_pair(&v, &BTreeMap::new()))
        .map(|(x, y)| Vec2::new(x, y))
        .unwrap_or((cursor - offset) / scale)
}

/// Keeps the composed image placed over the window's 640x480 area.
fn place(node: &mut Node, window: &Window) {
    let (scale, offset) = project(window);
    node.position_type = PositionType::Absolute;
    node.left = Val::Px(offset.x);
    node.top = Val::Px(offset.y);
    node.width = Val::Px(SCREEN_WIDTH * scale);
    node.height = Val::Px(SCREEN_HEIGHT * scale);
}

/// Recomposes the front end when the screens or the hovered control change, and keeps it placed
/// on resize.
fn present(
    mut state: ResMut<FrontEnd>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut shown: Query<(&mut Node, &mut ImageNode), With<MenuVisual>>,
    mut images: ResMut<Assets<Image>>,
    time: Res<Time>,
    mut commands: Commands,
    mut sounds: ResMut<Sounds>,
    mut audio: ResMut<Assets<AudioSource>>,
    buttons: Res<ButtonInput<MouseButton>>,
) {
    let Ok(window) = windows.single() else { return };
    let Ok((mut node, mut image_node)) = shown.single_mut() else { return };
    state.advance_slide(time.delta_secs() * 1000.0);
    let changing = state.advance_transition(time.delta_secs() * 1000.0);
    // The front-end clock for the animated icons (128 Hz ticks) and the mouse button.
    state.clock_ms += time.delta_secs_f64() * 1000.0;
    state.art.ticks = (state.clock_ms * 128.0 / 1000.0) as u32;
    state.art.pressed = buttons.pressed(MouseButton::Left);
    // Movies run on real time; a new frame means a new picture.
    if state.art.advance_movies(time.delta_secs() * 1000.0) {
        state.dirty = true;
    }
    let size = Vec2::new(window.width(), window.height());
    if size != state.laid_out_for {
        state.laid_out_for = size;
        place(&mut node, window);
    }
    // During a screen change nothing is under the pointer (input waits for it to finish).
    let p = if changing { Vec2::new(-1000.0, -1000.0) } else { pointer(window) };
    // The hover sound plays when the pointer moves onto another control that does something.
    let hovered = state.hit_at(p.x, p.y).filter(|h| *h != Hit::Inert);
    // Over an open list nothing under it shows a tip.
    let over_list = matches!(state.hit_at(p.x, p.y), Some(Hit::Inert | Hit::ComboPick(..) | Hit::ComboScroll(..)));
    if hovered.is_some() && hovered != state.last_hover {
        sounds.play(SOUND_HOVER, &mut commands, &mut audio);
    }
    state.last_hover = hovered;
    let state = &mut *state;
    // Help tip for the control under the pointer on the topmost interactive layer, drawn above all.
    let help = if over_list { None } else { state.layers.iter().zip(&state.interactive).enumerate().rev().filter(|(_, (_, on))| **on).find_map(|(i, (s, _))| nfspu_game::menu_help::at(&s.help, [p.x, p.y]).map(|h| (i, h))) };
    let tip = state.help.update(help, [p.x, p.y], time.delta_secs() * 1000.0, menu_canvas::Art::font(&mut state.art, nfspu_game::menu_help::FONT));
    state.dirty |= tip != state.tip;
    state.tip = tip;
    let tip_layer = Screen { draw: state.tip.clone(), ..Default::default() };
    let backdrop = state.backdrop();
    let shown = state.shown_layers();
    let mut layers: Vec<(&Screen, bool)> = shown.iter().map(|(l, on)| (l, *on)).collect();
    layers.push((&tip_layer, false));
    let key = menu_canvas::hover_key(&layers, Some([p.x, p.y]));
    if state.hover_key.as_ref() != Some(&key) {
        menu_canvas::HOVER_START.store(state.art.ticks, std::sync::atomic::Ordering::Relaxed);
    }
    let frames = menu_canvas::anim_frames(&layers, Some([p.x, p.y]), (state.art.ticks, state.art.pressed));
    if frames != state.anim_key {
        state.anim_key = frames;
        state.dirty = true;
    }
    if !state.dirty && state.hover_key.as_ref() == Some(&key) {
        return;
    }
    let started = std::time::Instant::now();
    let canvas = menu_canvas::compose(&layers, Some([p.x, p.y]), backdrop, &mut state.art);
    if backdrop[3] <= 0.0 && state.compose_ms.is_empty() {
        let open = canvas.rgba.chunks_exact(4).filter(|px| px[3] < 255).count();
        crate::log_line(&format!("front end: '{}' over the 3D room: {open} of {} pixels see-through", state.page.name, canvas.width * canvas.height));
    }
    // Movies need a new picture every ~66 ms, so report how long composing takes.
    state.compose_ms.push(started.elapsed().as_secs_f32() * 1000.0);
    if state.compose_ms.len() == 300 {
        let avg = state.compose_ms.iter().sum::<f32>() / 300.0;
        let max = state.compose_ms.iter().cloned().fold(0.0, f32::max);
        crate::log_line(&format!("front end: compose time over 300 frames avg {avg:.1} ms, max {max:.1} ms"));
        state.compose_ms.clear();
    }
    for miss in state.art.misses.drain(..) {
        crate::log_line(&miss);
    }
    let old = std::mem::replace(&mut image_node.image, images.add(rgba_image(canvas.width, canvas.height, canvas.rgba)));
    images.remove(&old);
    state.hover_key = Some(key);
    state.dirty = false;
}

/// Click and key navigation.
#[allow(clippy::too_many_arguments)]
fn click(
    mut commands: Commands,
    mut sounds: ResMut<Sounds>,
    mut audio: ResMut<Assets<AudioSource>>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    wheel: Res<bevy::input::mouse::AccumulatedMouseScroll>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut state: ResMut<FrontEnd>,
    mut clear: ResMut<ClearColor>,
    mut exit: MessageWriter<AppExit>,
    mut next: ResMut<NextState<AppState>>,
) {
    let Ok(window) = windows.single() else { return };
    let pointer = pointer(window);
    // Input waits while a screen change is animating.
    if state.transition.is_some() {
        return;
    }
    // The mouse wheel scrolls an open list, three rows a notch (**guess**: the retail list scrolls
    // by its arrows; wheel support is not traced).
    if wheel.delta.y != 0.0 {
        if let Some(accessor) = state.session.open_combo.clone() {
            let rows = if wheel.delta.y > 0.0 { -3 } else { 3 };
            state.scroll_combo(&accessor, rows);
            return;
        }
    }
    // Dragging a level bar: follow the pointer, keep the level when the button comes up.
    if let Some((accessor, left, width)) = state.dragging.clone() {
        if buttons.pressed(MouseButton::Left) {
            state.drag_level(&accessor, left, width, pointer.x);
            return;
        }
        state.dragging = None;
        let level = state.audio.level(&accessor);
        match state.audio.save() {
            Ok(()) => crate::log_line(&format!("menu: {accessor} = {level:?}, saved {}", AudioSettings::path().display())),
            Err(e) => crate::log_line(&format!("menu: could not save {}: {e}", AudioSettings::path().display())),
        }
    }
    let back = keys.just_pressed(KeyCode::Backspace);
    if !buttons.just_pressed(MouseButton::Left) && !back {
        return;
    }

    let action = if back { Some(Hit::Back) } else { state.hit_at(pointer.x, pointer.y) };
    // An open dropdown closes on any click that is not on it (the click then acts as usual).
    if state.session.open_combo.is_some() && !matches!(action, Some(Hit::ComboToggle(_)) | Some(Hit::ComboPick(..)) | Some(Hit::ComboScroll(..)) | Some(Hit::Inert)) {
        sounds.play(SOUND_CLOSE, &mut commands, &mut audio);
        state.slide_combo(false);
    }
    let Some(action) = action else { return };
    if action == Hit::Inert {
        return;
    }
    let before: Vec<(Screen, bool)> = state.layers.iter().cloned().zip(state.interactive.iter().copied()).collect();
    let from = state.popup.as_ref().unwrap_or(&state.page).name.clone();
    match action {
        Hit::Goto(name) => {
            if name.eq_ignore_ascii_case(&from) {
                return;
            }
            // Choosing quick race puts the car list back at its first car (the game-type routine
            // behind `SETTYPEQUICKRACE` sets `zMergedList.Car` to 0).
            if ["quickrace", "knockout", "quickknockout"].iter().any(|n| name.eq_ignore_ascii_case(n)) {
                state.career.on = false;
                state.factory.on = false;
            }
            if name.eq_ignore_ascii_case("quickrace") && from.eq_ignore_ascii_case("singleplayer") {
                state.quick.car = 0;
                state.quick.pick(nfspu_game::quick_race::CAR, 0);
                state.reload();
            }
            sounds.play(SOUND_SELECT, &mut commands, &mut audio);
            sounds.play(SOUND_SCREEN, &mut commands, &mut audio);
            state.note_page(&name);
            let Some(next) = state.load(&name) else {
                crate::log_line(&format!("menu: '{name}' is not a screen"));
                return;
            };
            let snapshot = (state.page.clone(), state.popup.clone());
            state.history.push(snapshot);
            if FrontEnd::is_popup(&next) {
                state.popup = Some(next);
            } else {
                state.page = next;
                state.popup = None;
            }
        }
        Hit::Back => {
            let Some((page, popup)) = state.history.pop() else { return };
            state.page = page;
            state.popup = popup;
            let name = state.popup.as_ref().unwrap_or(&state.page).name.clone();
            state.note_page(&name);
            state.reload();
        }
        // Retail may ask for confirmation first; not modelled.
        Hit::Quit => {
            crate::log_line("front end: exit button pressed, quitting");
            exit.write(AppExit::Success);
            return;
        }
        Hit::ComboToggle(accessor) => {
            let open = state.session.open_combo.as_deref() == Some(accessor.as_str());
            sounds.play(if open { SOUND_CLOSE } else { SOUND_OPEN }, &mut commands, &mut audio);
            if open {
                state.slide_combo(false);
            } else {
                // Open with the chosen item in view (a few rows from the top; **guess**).
                state.session.scroll = state.session.choices.get(&accessor).map(|c| menu::item_row(&c.rows, c.selected).saturating_sub(3)).unwrap_or(0);
                state.session.open_combo = Some(accessor.clone());
                state.scroll_combo(&accessor, 0);
                state.slide_combo(true);
            }
            return;
        }
        // Session only: the choice is not written to the save (the install is read-only).
        Hit::ComboPick(accessor, index) => {
            crate::log_line(&format!("menu: {accessor} = {index}"));
            sounds.play(SOUND_SELECT, &mut commands, &mut audio);
            if accessor == nfspu_game::quick_race::DIFFICULTY {
                state.session.difficulty = index;
            }
            match (state.career.on, accessor.as_str()) {
                (true, "Tournament.Tournament") | (true, "Tournament.EventType") => {
                    if let Some(c) = state.career.career.as_mut() {
                        c.chosen_tournament = index;
                    }
                    state.sync_quick();
                }
                (true, "OwnedCars.Car") => {
                    if let Some(c) = state.career.career.as_mut() {
                        c.chosen_car = index;
                    }
                    state.sync_quick();
                }
                (true, _) if state.career.career.is_some() && {
                    let s = &mut *state;
                    s.career.ui.pick(&accessor, index, s.career.career.as_mut().expect("career"), &s.quick.cars)
                } => {
                    state.save_career();
                    state.sync_quick();
                }
                _ => {
                    state.quick.pick(&accessor, index);
                }
            }
            state.slide_combo(false);
            return;
        }
        Hit::ComboScroll(accessor, rows) => {
            state.scroll_combo(&accessor, rows);
            return;
        }
        Hit::Slider(accessor, left, width) => {
            sounds.play(SOUND_SELECT, &mut commands, &mut audio);
            state.drag_level(&accessor, left, width, pointer.x);
            state.dragging = Some((accessor, left, width));
            return;
        }
        Hit::Tab(panel) => {
            if state.session.panel == panel {
                return;
            }
            sounds.play(SOUND_SELECT, &mut commands, &mut audio);
            crate::log_line(&format!("menu: Options tab {panel}"));
            state.session.panel = panel;
            state.reload();
            state.stack();
            state.dirty = true;
            return;
        }
        Hit::Callback(name) => {
            sounds.play(SOUND_SELECT, &mut commands, &mut audio);
            match name.as_str() {
                "PLAYGAME" => {
                    crate::log_line("front end: leaving the menu for the race");
                    let setup = if state.factory.on {
                        let player = state.player_name();
                        let st = &mut *state;
                        match st.factory.start(&st.quick, &st.locale, st.language, &player) {
                            Some(setup) => Some(setup),
                            None => {
                                crate::log_line("factory: no mission started");
                                return;
                            }
                        }
                    } else if state.career.on {
                        match state.start_career_race() {
                            Some(setup) => Some(setup),
                            None => {
                                crate::log_line("career: no race started");
                                return;
                            }
                        }
                    } else {
                        None
                    };
                    flow::start_race_from_menu(setup.unwrap_or_else(|| state.start_race()), &mut commands, &mut next);
                    return;
                }
                // Evolution: load or start the player's career and open the tournament list (retail
                // plays intro movies first for a new career; not built).
                // Factory Driver: a new player sees the welcome screens first, others the mission list.
                "SETTYPEFACTORYDRIVER" => {
                    let new = state.enter_factory();
                    let Some(next) = state.load(if new { "factfirst" } else { "factorydriver" }) else { return };
                    let snapshot = (state.page.clone(), state.popup.clone());
                    state.history.push(snapshot);
                    state.page = next;
                    state.popup = None;
                }
                "SETTYPECAREERMODE" => {
                    state.factory.on = false;
                    state.enter_career();
                    let Some(next) = state.load("tournament") else { return };
                    let snapshot = (state.page.clone(), state.popup.clone());
                    state.history.push(snapshot);
                    state.page = next;
                    state.popup = None;
                }
                // Give up the tournament in progress: its races start again.
                "FORFEITTOURNY" => {
                    let state = &mut *state;
                    if let Some(c) = state.career.career.as_mut() {
                        let open = c.open_tournaments(&state.career.data);
                        let id = open.get(c.chosen_tournament.min(open.len().saturating_sub(1))).map(|t| t.id);
                        if let Some(id) = id {
                            c.progress.entry(id).or_default().races_done = 0;
                            let _ = c.save_to(&state.career.dir);
                        }
                    }
                    state.sync_quick();
                }
                // The player popup's "continue": keep the highlighted player (already applied when
                // the row was clicked; nothing is written to the install) and close the popup. At
                // start-up the popup opens straight over the main menu with no history to go back to.
                "SETNAMETHENBACK" => {
                    crate::log_line(&format!("menu: continue as player {:?}", state.profiles.selected.and_then(|i| state.profiles.names.get(i))));
                    if let Some((page, popup)) = state.history.pop() {
                        state.page = page;
                        state.popup = popup;
                    } else {
                        state.popup = None;
                    }
                }
                // The bottom bar's options button opens the Options screen, on its audio tab.
                "OPTIONS" => {
                    if from.eq_ignore_ascii_case("Options") {
                        return;
                    }
                    let Some(next) = state.load("Options") else { return };
                    let snapshot = (state.page.clone(), state.popup.clone());
                    state.history.push(snapshot);
                    state.page = next;
                    state.popup = None;
                }
                // The car-setup screen's test drive opens the location popup; its button, and the
                // dealership's and used-car lot's (reached through their own popups), start the drive.
                "TESTDRIVECHECK" => {
                    let Some(next) = state.load("testdrivecarsetup") else { return };
                    state.note_page("testdrivecarsetup");
                    let snapshot = (state.page.clone(), state.popup.clone());
                    state.history.push(snapshot);
                    state.popup = Some(next);
                }
                "TESTDRIVECARSETUP" | "TESTDRIVENEW" | "TESTDRIVEUSED" | "TESTDRIVE" if state.career.on => {
                    let page = state.popup.as_ref().unwrap_or(&state.page).name.clone();
                    // The used-car lot's own button also opens the location popup (its destination).
                    if name == "TESTDRIVE" && !page.eq_ignore_ascii_case("prerace") {
                        return;
                    }
                    // A practice run from the pre-race screen goes to the next race's track.
                    if name == "TESTDRIVE" {
                        if let Some(first) = state.career.career.as_ref().and_then(|c| {
                            let open = c.open_tournaments(&state.career.data);
                            let t = open.get(c.chosen_tournament.min(open.len().saturating_sub(1)))?;
                            career::Career::race_tracks(t, &state.career.data).get(c.next_race(t)).copied()
                        }) {
                            let open_ids = vec![first];
                            if let Some(at) = nfspu_game::career_ui::test_tracks(&state.quick.tracks, &open_ids).iter().position(|&i| state.quick.tracks[i].id == first) {
                                state.career.ui.test_track = at;
                            }
                        }
                    }
                    let Some(setup) = state.start_test_drive(&page) else {
                        crate::log_line("career: test drive: no car or location");
                        return;
                    };
                    flow::start_race_from_menu(setup, &mut commands, &mut next);
                    return;
                }
                "TRACK1INC" | "LOOPEDINC" => state.quick.step_track(1),
                "TRACK1DEC" | "LOOPEDDEC" => state.quick.step_track(-1),
                other => {
                    let st = &mut *state;
                    let page = st.popup.as_ref().unwrap_or(&st.page).name.clone();
                    let line = match st.career.career.as_mut() {
                        Some(c) if st.career.on => st.career.ui.callback(other, &page, c, &st.quick.cars),
                        _ => None,
                    };
                    match line {
                        Some(line) => {
                            crate::log_line(&line);
                            state.save_career();
                            state.sync_quick();
                        }
                        None => {
                            crate::log_line(&format!("menu: callback {other} not handled yet"));
                            return;
                        }
                    }
                }
            }
            state.reload();
        }
        Hit::ListPick(accessor, index) => {
            sounds.play(SOUND_SELECT, &mut commands, &mut audio);
            if state.career.ui.list_pick(&accessor, index) {
                state.sync_quick();
                state.reload();
                state.stack();
                state.dirty = true;
            }
            return;
        }
        Hit::Inert => return,
        Hit::Select(row) => {
            if state.profiles.selected == Some(row) {
                return;
            }
            crate::log_line(&format!("menu: select player {row} ({:?})", state.profiles.names.get(row)));
            state.profiles.selected = Some(row);
            state.update_progress();
            state.reload();
        }
    }
    let to = state.popup.as_ref().unwrap_or(&state.page).name.clone();
    crate::log_line(&format!("menu: {from} -> {to}"));
    state.stack();
    state.begin_transition(before);
    state.dirty = true;
    clear.0 = state.clear_colour();
}

/// Makes a texture from RGBA8 pixels (gamma-encoded, as the compositor writes them).
fn rgba_image(width: u32, height: u32, rgba: Vec<u8>) -> Image {
    Image::new(
        Extent3d { width, height, depth_or_array_layers: 1 },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

fn quit_on_escape(keys: Res<ButtonInput<KeyCode>>, mut exit: MessageWriter<AppExit>) {
    if keys.just_pressed(KeyCode::Escape) {
        crate::log_line("front end: Escape pressed, quitting");
        exit.write(AppExit::Success);
    }
}

/// `NFSPU_MENU_CAPTURE=1` saves a PNG of the current screen and exits, so the drawing can be
/// checked without a human at the window. Output goes to the ignored `logs/` folder.
#[derive(Resource, Default)]
struct Capture {
    enabled: bool,
    /// `NFSPU_MENU_CAPTURE_SECS`: wait at least this long first (a help tip needs 1.5 s to fade in).
    after: f32,
    frames: u32,
    waiting: bool,
    /// `NFSPU_MENU_PLAY=1`: press RACE once the picture is saved (instead of quitting).
    play: bool,
    /// The picture is saved and RACE is to be pressed now.
    go: bool,
}

impl Capture {
    fn from_env() -> Self {
        let after = std::env::var("NFSPU_MENU_CAPTURE_SECS").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(0.0);
        let play = std::env::var("NFSPU_MENU_PLAY").as_deref() == Ok("1");
        Self { enabled: std::env::var("NFSPU_MENU_CAPTURE").as_deref() == Ok("1"), after, play, ..default() }
    }
}

fn capture(mut state: ResMut<Capture>, current: Res<FrontEnd>, time: Res<Time>, mut commands: Commands, mut next: ResMut<NextState<AppState>>) {
    if std::mem::take(&mut state.go) {
        if current.can_race() {
            crate::log_line("front end: capture saved, pressing RACE (NFSPU_MENU_PLAY)");
            flow::start_race_from_menu(current.start_race(), &mut commands, &mut next);
        } else {
            crate::log_line("front end: NFSPU_MENU_PLAY, but this screen has no RACE");
        }
        return;
    }
    if !state.enabled || state.waiting {
        return;
    }
    state.frames += 1;
    // Over the 3D garage the first frames are black while the 3D shaders compile: wait 3 s.
    // A screen change still animating is waited out too.
    let after = if nfspu_game::garage::room_for(&current.page.room).is_some() { state.after.max(3.0) } else { state.after };
    if state.frames < 60 || time.elapsed_secs() < after || current.transition.is_some() {
        return;
    }
    let dir = crate::log_path().parent().map(PathBuf::from).unwrap_or_default();
    let shown = match &current.popup {
        Some(p) => format!("{}_{}", current.page.name, p.name),
        None => current.page.name.clone(),
    };
    let safe: String = shown.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    let path = dir.join(format!("menu_{safe}.png"));
    crate::log_line(&format!("front end: capture {}", path.display()));
    state.waiting = true;
    commands.spawn(Screenshot::primary_window()).observe(
        move |captured: On<ScreenshotCaptured>, mut state: ResMut<Capture>, mut exit: MessageWriter<AppExit>| {
            let result = captured
                .image
                .clone()
                .try_into_dynamic()
                .map_err(|e| format!("image conversion: {e}"))
                .and_then(|image| image.to_rgb8().save(&path).map_err(|e| e.to_string()));
            match result {
                Ok(()) => {
                    crate::log_line(&format!("front end: saved {}", path.display()));
                    state.enabled = false;
                    // `NFSPU_MENU_PLAY=1` goes on to the race (the next frame presses RACE).
                    if state.play {
                        state.go = true;
                    } else {
                        exit.write(AppExit::Success);
                    }
                }
                Err(error) => {
                    crate::log_line(&format!("front end capture FAILED: {error}"));
                    state.enabled = false;
                    exit.write(AppExit::error());
                }
            }
        },
    );
}
