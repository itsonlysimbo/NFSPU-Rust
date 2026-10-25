//! One program, one window: the front end, the loading screen and the race are states of one Bevy
//! app (`AppState`), entered and left without restarting the program.
//!
//! - `Menu`: the front end (`menu_mode`). Its choices, history and music survive a race.
//! - `Loading`: the retail loading screen while the track is read (`race_load`); the race scene is
//!   built behind it. Without a loading screen (the free viewer, `NFSPU_RACE` alone) the scene is
//!   built at once and the state moves straight on.
//! - `Race`: driving, the countdown, the race and its results panel (`race_mode`). Closing the
//!   results goes back to the menu at the quick-race screen, or quits when the race was started
//!   from the environment without a menu.
//!
//! What a race needs (track, car, opponents, laps) is a `RaceConfig`, built either from the menu's
//! quick-race choices or from the `NFSPU_*` variables and command-line arguments, and applied as
//! resources before `Loading` starts. Everything the race spawns is removed when `Race` ends.

use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use nfspu_game::car_model::CarOptions;
use nfspu_game::hud_plugin::HudFeed;
use nfspu_game::race_setup::{self, RaceSetup as QuickRaceSetup};
use std::{env, fs, path::PathBuf};

use nfspu_game::knockout;

use super::race_mode::{RaceHold, RaceRequest};
use super::{log_line, log_path};

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    /// The retail front end.
    Menu,
    /// The loading screen; the race scene is built behind it.
    #[default]
    Loading,
    /// Driving, the race and its results.
    Race,
}

/// True while the race scene exists or is being built (loading screen and race).
pub fn in_race_scene(state: Res<State<AppState>>) -> bool {
    matches!(state.get(), AppState::Loading | AppState::Race)
}

/// Where closing the race's results goes: back to the menu (`true`) or out of the program.
#[derive(Resource)]
pub struct ReturnToMenu(pub bool);

/// The menu's quick-race setup for the race being run, when there is one (player car's catalogue
/// id, body style, paint, gearbox and aids).
#[derive(Resource, Default)]
pub struct QuickSetup(pub Option<QuickRaceSetup>);

/// Everything one race (or the free viewer) needs before its scene is built.
#[derive(Debug, Clone)]
pub struct RaceConfig {
    pub track: String,
    pub car: String,
    pub style: usize,
    pub driver: u8,
    pub paint: usize,
    pub mirrored: bool,
    /// The loading screen's setup; none for the free viewer or a bare `NFSPU_RACE`.
    pub loading: Option<nfspu_game::loading::RaceSetup>,
    /// The race rules; none for the free viewer.
    pub race: Option<RaceRequest>,
    pub quick: Option<QuickRaceSetup>,
    pub hud: bool,
}

impl RaceConfig {
    /// The start-up race from the environment and arguments:
    /// `nfspu-game [track [car [style [driver [paint]]]]]`, `NFSPU_LOADING`, `NFSPU_RACE`,
    /// `NFSPU_OPPONENTS`, `NFSPU_HUD`. With none of them: the free viewer on alps in the 993.
    pub fn from_env() -> RaceConfig {
        let race = RaceRequest::from_env();
        let arg = |n: usize| env::args().nth(n);
        let track = race.as_ref().map(|r| r.track.clone()).or_else(|| arg(1)).unwrap_or_else(|| "alps".to_string());
        let car = arg(2).unwrap_or_else(|| "993".to_string());
        let style: usize = arg(3).and_then(|v| v.parse().ok()).unwrap_or(1);
        let driver: u8 = arg(4).and_then(|v| v.parse().ok()).unwrap_or(1);
        let paint: usize = arg(5).and_then(|v| v.parse().ok()).unwrap_or(CarOptions::default().paint);
        // `NFSPU_LOADING=<track>[,<car>]` loads that race behind the retail loading screen.
        let loading = super::race_load::from_env();
        let track = loading.as_ref().map_or(track, |s| s.track.clone());
        let car = loading.as_ref().map_or(car, |s| s.car.clone());
        // A test run with `NFSPU_LOADING` may name the menu's setup in `logs/race_setup.txt`
        // (only used when it names the car being loaded).
        let quick = env::var_os("NFSPU_LOADING").and_then(|_| read_setup_file()).filter(|s| s.car_model.eq_ignore_ascii_case(&car));
        let (style, paint) = quick.as_ref().map_or((style, paint), |s| (s.car_options().style, s.car_options().paint));
        // Mirrored races mirror the track and race geometry; either variable is enough.
        let mirrored = loading.as_ref().is_some_and(|s| s.mirrored) || race.as_ref().is_some_and(|r| r.mirrored);
        RaceConfig { track, car, style, driver, paint, mirrored, loading, race, quick, hud: nfspu_game::hud_plugin::enabled() }
    }

    /// The race the menu's RACE button asks for: the quick-race choices, with the loading screen,
    /// the race rules, the opponents' cars and the HUD.
    pub fn from_menu(setup: QuickRaceSetup) -> RaceConfig {
        use race_setup::Direction as D;
        let backward = matches!(setup.direction, D::Backward | D::BackwardMirrored);
        let mirrored = matches!(setup.direction, D::ForwardMirrored | D::BackwardMirrored);
        let loading = format!("{},{}{}{}", setup.track, setup.car_model, if backward { ",reverse" } else { "" }, if mirrored { ",mirror" } else { "" });
        let rules = format!("{},{},{},{},{}", setup.track, setup.direction.word(), setup.laps, setup.opponents.min(7), setup.difficulty.min(2));
        let opponents: Vec<String> = opponent_cars(&setup).iter().map(|id| id.to_string()).collect();
        log_line(&format!("flow: race loading={loading} rules={rules} opponents={} traffic={}", opponents.join(","), setup.traffic));
        let mut race = RaceRequest::parse(&rules);
        if let Some(r) = &mut race {
            r.opponent_cars = opponents;
            r.damage = setup.damage;
            r.traffic = setup.traffic.min(3) as u8;
            // pursuit has no retail menu screen in this install: NFSPU_PURSUIT=1 turns it on
            r.pursuit = std::env::var("NFSPU_PURSUIT").as_deref() == Ok("1");
        }
        let options = setup.car_options();
        RaceConfig {
            track: setup.track.clone(),
            car: setup.car_model.clone(),
            style: options.style,
            driver: 1,
            paint: options.paint,
            mirrored,
            loading: nfspu_game::loading::RaceSetup::parse(&loading),
            race,
            quick: Some(setup),
            hud: true,
        }
    }

    /// Puts the race's resources in place (replacing the last race's) and sets the world's mirror
    /// flag; call before `Loading` starts.
    pub fn apply(self, world: &mut World) {
        world.insert_resource(LastRace(self.clone()));
        nfspu_game::set_world_mirrored(self.mirrored);
        if self.mirrored {
            log_line("race: mirrored track layout");
        }
        if let Some(s) = &self.quick {
            log_line(&format!(
                "player car from the menu: '{}' (id {}) model '{}' style {} paint section {} roof {}",
                s.car_name, s.car_id, s.car_model, self.style, self.paint, if s.roof_up { "up" } else { "down" }
            ));
        }
        world.insert_resource(super::TrackName(self.track));
        world.insert_resource(super::CarName(self.car));
        world.insert_resource(super::CarStyle(self.style));
        world.insert_resource(super::CarDriver(self.driver));
        world.insert_resource(super::CarPaint(self.paint));
        world.insert_resource(QuickSetup(self.quick));
        // With a race the car is held from the start (also behind the loading screen).
        world.insert_resource(RaceHold(self.race.is_some()));
        match self.race {
            Some(r) => world.insert_resource(r),
            None => {
                world.remove_resource::<RaceRequest>();
            }
        }
        match self.loading {
            Some(l) => world.insert_resource(l),
            None => {
                world.remove_resource::<nfspu_game::loading::RaceSetup>();
            }
        }
        if self.hud {
            world.insert_resource(HudFeed::default());
        } else {
            world.remove_resource::<HudFeed>();
        }
    }
}

/// `logs/race_setup.txt`: the last quick-race setup the menu wrote (a log, read only by tests).
fn read_setup_file() -> Option<QuickRaceSetup> {
    let text = fs::read_to_string(log_path().parent()?.join("race_setup.txt")).ok()?;
    QuickRaceSetup::parse(&text).map_err(|e| log_line(&format!("race setup not used: {e}"))).ok()
}

/// The opponents' cars (catalogue ids) for a quick race, by the menu's rule (`ai::pick_cars`)
/// over the installed road cars of `nfs5.car`.
fn opponent_cars(setup: &QuickRaceSetup) -> Vec<u16> {
    use nfspu_game::catalog;
    let game = PathBuf::from(env::var("NFSPU_GAME_DIR").unwrap_or_default());
    let cars = catalog::parse_cars(&fs::read(game.join("FEData").join("Data").join("nfs5.car")).unwrap_or_default());
    let models = catalog::file_names(&game.join("GameData").join("CarModel"));
    let list: Vec<catalog::Car> = catalog::opponent_cars(&cars, |m| models.contains(&format!("{m}.crp").to_ascii_lowercase())).into_iter().map(|i| cars[i].clone()).collect();
    let player = cars.iter().find(|c| c.id == setup.car_id).cloned().unwrap_or_else(|| catalog::Car { id: setup.car_id, model: setup.car_model.clone(), ..Default::default() });
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.subsec_nanos());
    nfspu_game::ai::pick_cars(&setup.opponent_cars, &player, &list, setup.opponents.min(7) as usize, &mut nfspu_game::ai::Rng::new(seed))
}

/// The menu's RACE: the race's resources go in place and the loading screen starts next frame.
/// On a knockout screen (`knockout::set_pending`) this starts a knockout run instead.
pub fn start_race_from_menu(setup: QuickRaceSetup, commands: &mut Commands, next: &mut NextState<AppState>) {
    let pending = knockout::take_pending();
    commands.queue(move |world: &mut World| {
        world.remove_resource::<KnockoutRun>();
        match pending {
            Some(p) => start_knockout(world, p, setup),
            None => RaceConfig::from_menu(setup).apply(world),
        }
    });
    next.set(AppState::Loading);
}

/// A knockout being played, and the menu setup its races are built from.
#[derive(Resource)]
pub struct KnockoutRun {
    pub run: knockout::Run,
    pub base: QuickRaceSetup,
}

/// The race for the run's next round: one lap per opponent with the last car of each lap out
/// (quick knockout), or the next track of the series with the survivors.
fn knockout_config(run: &knockout::Run, base: &QuickRaceSetup) -> RaceConfig {
    use knockout::Mode;
    let mut s = base.clone();
    s.opponents = run.ko.opponents_left().min(7) as u8;
    match run.ko.mode {
        Mode::Quick => {
            s.laps = run.quick_laps().clamp(1, 8) as u8;
            // laps need a circuit; a road would run once with nobody knocked out
            if !knockout::is_circuit(&s.track) {
                log_line(&format!("knockout: '{}' is a road, quick knockout uses monaco1 instead", s.track));
                s.track = "monaco1".to_string();
                s.track_name = "monaco1".to_string();
            }
        }
        Mode::Series => {
            if let Some(t) = run.track() {
                s.track = t.to_string();
                s.track_name = t.to_string();
            }
        }
    }
    let mut config = RaceConfig::from_menu(s);
    if let Some(r) = &mut config.race {
        if run.ko.mode == Mode::Quick {
            r.knockout = true;
            r.laps = run.quick_laps();
        }
        if !run.cars.is_empty() {
            r.opponent_cars = run.opponent_cars();
        }
    }
    config
}

fn start_knockout(world: &mut World, p: knockout::Pending, base: QuickRaceSetup) {
    let mut run = knockout::Run::new(p.mode, p.opponents, p.tracks);
    let config = knockout_config(&run, &base);
    run.cars = config.race.as_ref().map(|r| r.opponent_cars.clone()).unwrap_or_default();
    log_line(&format!(
        "knockout: {:?}, {} opponent(s), {} round(s), cars {:?}, tracks {:?}",
        run.ko.mode, run.ko.total, run.ko.total, run.cars, run.tracks
    ));
    config.apply(world);
    world.insert_resource(KnockoutRun { run, base });
}

/// Closes the race: back to the menu, or quits when there is no menu to go back to.
pub fn leave_race(back: &ReturnToMenu, next: &mut NextState<AppState>, exit: &mut MessageWriter<AppExit>) {
    if back.0 {
        log_line("flow: race over, back to the menu");
        next.set(AppState::Menu);
    } else {
        log_line("flow: race over, no menu to go back to, quitting");
        exit.write(AppExit::Success);
    }
}

/// Closing the results: with a knockout going, the survivors race again (the next round) until
/// the run is won or the player is out; otherwise as `leave_race`. A race left unfinished
/// (pause menu) ends the run.
pub fn leave_results(
    knock: Option<&mut KnockoutRun>,
    status: &nfspu_game::race::RaceStatus,
    commands: &mut Commands,
    back: &ReturnToMenu,
    next: &mut NextState<AppState>,
    exit: &mut MessageWriter<AppExit>,
) {
    let Some(k) = knock else { return leave_race(back, next, exit) };
    if !status.finished {
        log_line("knockout: race left unfinished, the run is over");
        commands.remove_resource::<KnockoutRun>();
        return leave_race(back, next, exit);
    }
    let step = k.run.after_race(&status.order);
    log_line(&format!("knockout: round {} of {} over, order {:?} -> {step:?}", k.run.ko.round, k.run.ko.total, status.order));
    if step == knockout::Step::Next {
        let config = knockout_config(&k.run, &k.base);
        log_line(&format!("knockout: next race on {} with {} opponent(s)", config.track, k.run.ko.opponents_left()));
        commands.queue(move |world: &mut World| {
            world.insert_resource(LastRace(config));
            world.insert_resource(RestartRace);
        });
        next.set(AppState::Loading);
    } else {
        log_line(&format!("knockout: over ({})", if step == knockout::Step::Won { "the player won" } else { "the player is out" }));
        commands.remove_resource::<KnockoutRun>();
        leave_race(back, next, exit);
    }
}

/// How the player's last race ended, recorded as the race scene goes down: the finishing place
/// (1-based) or 0 when the race was left unfinished. The Evolution career pays by it.
#[derive(Resource, Default)]
pub struct LastResult {
    pub place: usize,
}

fn record_result(status: Res<nfspu_game::race::RaceStatus>, mut result: ResMut<LastResult>) {
    result.place = if status.finished { status.position } else { 0 };
    log_line(&format!("flow: race result, finished {}, place {}", status.finished, result.place));
}

/// The last race set up, for "race again" on the pause menu.
#[derive(Resource)]
pub struct LastRace(pub RaceConfig);

/// Inserted when the race is left to be run again: once the scene is down (`end_race`, on leaving
/// `Race`), entering `Loading` sets the last race up again and builds it afresh.
#[derive(Resource)]
pub struct RestartRace;

fn restart_race(world: &mut World) {
    if world.remove_resource::<RestartRace>().is_none() {
        return;
    }
    let Some(config) = world.get_resource::<LastRace>().map(|l| l.0.clone()) else { return };
    log_line(&format!("flow: race again on {}", config.track));
    config.apply(world);
}

pub struct FlowPlugin;

impl Plugin for FlowPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(FlowTest::from_env())
            .init_resource::<LastResult>()
            .add_systems(OnExit(AppState::Race), (record_result, end_race).chain())
            .add_systems(OnEnter(AppState::Loading), restart_race.before(super::race_load::enter))
            .add_systems(OnEnter(AppState::Menu), note_menu_entry)
            .add_systems(Update, (log_state_changes, report_after_race, flow_test_race.run_if(in_state(AppState::Race))));
    }
}

/// Takes the race scene down: its entities (track, sky, cars, cameras, UI), its resources, the
/// captured mouse and the mirror flag. Entities marked `DespawnOnExit(AppState::Race)` go by
/// themselves; the track's are listed in `TrackEntities`.
#[allow(clippy::too_many_arguments)]
fn end_race(
    mut commands: Commands,
    track: Option<Res<super::TrackEntities>>,
    mut captured: ResMut<super::MouseCaptured>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut console: ResMut<super::console::Console>,
    mut status: ResMut<nfspu_game::race::RaceStatus>,
    mut hold: ResMut<RaceHold>,
) {
    for e in track.map(|t| t.0.clone()).unwrap_or_default() {
        commands.entity(e).try_despawn();
    }
    commands.remove_resource::<super::TrackEntities>();
    commands.remove_resource::<super::GroundRes>();
    commands.remove_resource::<super::WallRes>();
    commands.remove_resource::<super::CarRes>();
    commands.remove_resource::<super::CarModelEntity>();
    commands.remove_resource::<super::SkyState>();
    commands.remove_resource::<RaceRequest>();
    commands.remove_resource::<nfspu_game::loading::RaceSetup>();
    commands.run_system_cached(nfspu_game::weather_fx::clear);
    *status = Default::default();
    hold.0 = false;
    console.open = false;
    if captured.0 {
        captured.0 = false;
        if let Ok(mut c) = cursor.single_mut() {
            c.grab_mode = CursorGrabMode::None;
            c.visible = true;
        }
    }
    nfspu_game::set_world_mirrored(false);
    log_line("flow: race scene removed");
}

/// Logs every state change.
fn log_state_changes(state: Res<State<AppState>>) {
    if state.is_changed() {
        log_line(&format!("flow: state {:?}", state.get()));
    }
}

/// When the menu came back, wait until the removed race's assets are freed (a few frames), then
/// log what is left, so a leak shows in `logs/m0.log`.
#[derive(Resource)]
struct MenuEntry {
    at: f32,
    logged: bool,
}

fn note_menu_entry(mut commands: Commands, time: Res<Time>) {
    commands.insert_resource(MenuEntry { at: time.elapsed_secs(), logged: false });
}

#[allow(clippy::too_many_arguments)]
fn report_after_race(
    time: Res<Time>,
    entry: Option<ResMut<MenuEntry>>,
    meshes: Res<Assets<Mesh>>,
    images: Res<Assets<Image>>,
    materials: Res<Assets<StandardMaterial>>,
    audio: Res<Assets<AudioSource>>,
    entities: Query<()>,
    cameras: Query<&Camera>,
    mesh3d: Query<(), With<Mesh3d>>,
    nodes: Query<(), With<Node>>,
) {
    let Some(mut entry) = entry else { return };
    if entry.logged || time.elapsed_secs() - entry.at < 3.0 {
        return;
    }
    entry.logged = true;
    let active = cameras.iter().filter(|c| c.is_active).count();
    log_line(&format!(
        "flow: on the menu: {} entities ({} meshes drawn, {} UI nodes, {} cameras, {} active); assets: {} meshes, {} images, {} materials, {} sounds",
        entities.iter().count(),
        mesh3d.iter().count(),
        nodes.iter().count(),
        cameras.iter().count(),
        active,
        meshes.len(),
        images.len(),
        materials.len(),
        audio.len()
    ));
}

/// `NFSPU_FLOW_TEST=<races>[,<seconds>]`: a scripted run for tests. The menu presses RACE by
/// itself (a different track each time), each race is left `seconds` (default 20) after it
/// starts as if its results were closed, and after the last race the menu quits.
#[derive(Resource, Default)]
pub struct FlowTest {
    pub races: u32,
    pub done: u32,
    pub race_secs: f32,
    /// When the current state was entered (s since start).
    pub since: Option<f32>,
}

impl FlowTest {
    fn from_env() -> FlowTest {
        let Ok(v) = env::var("NFSPU_FLOW_TEST") else { return FlowTest::default() };
        let mut parts = v.split(',').map(str::trim);
        let races = parts.next().and_then(|n| n.parse().ok()).unwrap_or(1);
        let race_secs = parts.next().and_then(|n| n.parse().ok()).unwrap_or(20.0);
        log_line(&format!("flow test: {races} race(s) of {race_secs} s each"));
        FlowTest { races, done: 0, race_secs, since: None }
    }

    pub fn on(&self) -> bool {
        self.races > 0
    }
}

fn flow_test_race(
    time: Res<Time>,
    mut test: ResMut<FlowTest>,
    status: Res<nfspu_game::race::RaceStatus>,
    back: Res<ReturnToMenu>,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
) {
    if !test.on() {
        return;
    }
    let now = time.elapsed_secs();
    let since = *test.since.get_or_insert(now);
    if now - since < test.race_secs {
        return;
    }
    test.since = None;
    test.done += 1;
    log_line(&format!(
        "flow test: race {} of {} left after {:.0} s (race tick {}, lap {}/{}, P{})",
        test.done, test.races, test.race_secs, status.tick, status.current_lap, status.total_laps, status.position
    ));
    leave_race(&back, &mut next, &mut exit);
}
