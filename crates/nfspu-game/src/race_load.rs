//! Loads a race behind the retail loading screen (`nfspu_game::loading`).
//!
//! Runs while the app is in `AppState::Loading`. Without a `RaceSetup` resource (the free viewer, a
//! bare `NFSPU_RACE`) it runs the viewer's `setup` at once and moves on to `Race`. With one, the
//! screen goes up first, the track's geometry and textures are read on a worker thread while
//! the screen stays responsive, and the viewer's own `setup` then runs as usual (it finds the
//! track already read). The stages follow retail's order:
//!
//! | stage | retail | here |
//! |---|---|---|
//! | 0 | screen up, plain era background | screen up; worker thread starts reading the track |
//! | 1-4 | audio, front end teardown, scenery setup | nothing to do yet: one frame each |
//! | 5 | track art shown, then the track is read | track art shown; wait for the worker |
//! | 6 | track read | track geometry and textures decoded |
//! | 7 | (unnamed step) | the viewer's `setup`: meshes, sky, ground, walls, car |
//! | 8-9 | cars and AI | already done in 7: one frame each |
//! | 10 | screen removed, race intro starts | screen removed, `AppState::Race` |

use crate::flow::AppState;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use nfspu_game::loading::{self, LoadingStage, RaceSetup, LAST_STAGE, TRACK_ART_STAGE};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// A track read ahead of time by the worker, picked up by `load_track`.
static PRELOADED: OnceLock<Mutex<Option<(String, Result<super::LoadedTrack, String>)>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<(String, Result<super::LoadedTrack, String>)>> {
    PRELOADED.get_or_init(|| Mutex::new(None))
}

/// The track read by the worker for `name`, if there is one (taken, so a later reload reads again).
pub(super) fn take_preloaded(name: &str) -> Option<Result<super::LoadedTrack, String>> {
    let mut slot = slot().lock().ok()?;
    match slot.take() {
        Some((n, track)) if n == name => Some(track),
        other => {
            *slot = other;
            None
        }
    }
}

/// `NFSPU_LOADING=<track>[,<car>][,reverse][,mirror]` starts the race behind the loading screen.
pub fn from_env() -> Option<RaceSetup> {
    std::env::var("NFSPU_LOADING").ok().and_then(|v| RaceSetup::parse(&v))
}

/// Builds the race scene on entering `Loading`: plain `setup` without a `RaceSetup`, the loading
/// screen flow with one.
pub struct RaceLoad;

impl Plugin for RaceLoad {
    fn build(&self, app: &mut App) {
        app.add_plugins(loading::LoadingScreenPlugin)
            .add_systems(OnEnter(AppState::Loading), enter)
            .add_systems(Update, (advance, report).chain().run_if(in_state(AppState::Loading)))
            .add_systems(OnExit(AppState::Loading), |mut commands: Commands| commands.remove_resource::<Flow>());
    }
}

/// The loading screen for a race, or for the viewer the scene at once.
fn enter(world: &mut World) {
    if world.contains_resource::<RaceSetup>() {
        if let Err(e) = world.run_system_once(begin) {
            super::log_line(&format!("loading screen: could not start: {e}"));
        }
        return;
    }
    let started = Instant::now();
    if let Err(e) = world.run_system_once(super::setup) {
        super::log_line(&format!("setup failed to run: {e}"));
    }
    super::log_line(&format!("scene built without a loading screen in {:.2}s", started.elapsed().as_secs_f32()));
    world.resource_mut::<NextState<AppState>>().set(AppState::Race);
}

/// Progress of the flow (the visible stage is `LoadingStage`).
#[derive(Resource)]
struct Flow {
    started: Instant,
    /// Frames shown at the current stage.
    frames: u32,
    worker: Option<std::thread::JoinHandle<()>>,
    /// When each stage was reached, for the log.
    times: Vec<(u8, f32)>,
}

fn begin(mut commands: Commands, setup: Res<RaceSetup>) {
    let game_dir = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").unwrap_or_default());
    for miss in loading::begin(&mut commands, &game_dir, setup.clone()) {
        super::log_line(&format!("loading screen: {miss}"));
    }
    super::log_line(&format!("loading screen: up for {:?}", *setup));
    let name = setup.track.clone();
    let worker = std::thread::spawn(move || {
        let started = Instant::now();
        let track = super::load_track(&name);
        super::log_line(&format!("loading screen: worker read track '{name}' in {:.2}s (ok: {})", started.elapsed().as_secs_f32(), track.is_ok()));
        if let Ok(mut slot) = slot().lock() {
            *slot = Some((name, track));
        }
    });
    commands.insert_resource(Flow { started: Instant::now(), frames: 0, worker: Some(worker), times: vec![(0, 0.0)] });
}

/// Moves to the next stage when its work is done. Every stage stays up for at least one frame so
/// it is drawn.
fn advance(world: &mut World) {
    let Some(stage) = world.get_resource::<LoadingStage>().map(|s| s.0) else { return };
    let Some(mut flow) = world.get_resource_mut::<Flow>() else { return };
    flow.frames += 1;
    if flow.frames < 2 || stage >= LAST_STAGE {
        return;
    }
    let ready = match stage {
        TRACK_ART_STAGE => flow.worker.as_ref().is_some_and(|w| w.is_finished()),
        _ => true,
    };
    if !ready {
        return;
    }
    if stage == TRACK_ART_STAGE {
        if let Some(worker) = flow.worker.take() {
            let _ = worker.join();
        }
    }
    let next = stage + 1;
    let at = flow.started.elapsed().as_secs_f32();
    flow.frames = 0;
    flow.times.push((next, at));
    drop(flow);
    if next == 7 {
        // The viewer's own start-up; `load_track` inside it takes the worker's result.
        let started = Instant::now();
        if let Err(e) = world.run_system_once(super::setup) {
            super::log_line(&format!("loading screen: setup failed to run: {e}"));
        }
        super::log_line(&format!("loading screen: setup (meshes, sky, ground, car) took {:.2}s", started.elapsed().as_secs_f32()));
    }
    world.insert_resource(LoadingStage(next));
}

/// Logs the stage times once the screen is gone, and starts the race.
fn report(mut finished: MessageReader<loading::LoadingFinished>, flow: Option<Res<Flow>>, mut next: ResMut<NextState<AppState>>) {
    for done in finished.read() {
        let times = flow.as_ref().map(|f| f.times.iter().map(|(s, t)| format!("{s}@{t:.2}s")).collect::<Vec<_>>().join(" ")).unwrap_or_default();
        super::log_line(&format!("loading screen: done for '{}' in {:.2}s; stages {times}", done.setup.track, done.seconds));
        next.set(AppState::Race);
    }
}
