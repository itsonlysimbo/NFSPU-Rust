//! The in-race pause (`docs/provenance/race.md`, "Pause"). Esc (or P) during a race stops it and
//! shows retail's pause menu (`hud_pause`): continue, race again, quit. While paused the virtual
//! clock stands still, so the 64 Hz race clock, the AI and every effect that runs on frame time
//! stop; the player's car, the camera keys and the race sound systems do not run at all, and every
//! sound that was playing is paused and picked up again on continue.
//!
//! `NFSPU_PAUSE_TEST=<race seconds>,<capture|resume|restart|quit>` pauses by itself once the race
//! clock passes that time, checks that the clock stays put, then does the action and logs it.

use bevy::audio::{AudioSink, AudioSinkPlayback, SpatialAudioSink};
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::window::PrimaryWindow;
use nfspu_game::hud_pause::{self, PauseAction, PausePage, PauseView};
use nfspu_game::hud_plugin::HudFeed;
use nfspu_game::race::RaceStatus;

use super::flow::{self, AppState, ReturnToMenu};
use super::log_line;

/// Whether the race is paused, and the menu's state.
#[derive(Resource, Default)]
pub struct Pause {
    pub on: bool,
    pub view: PauseView,
    /// Sounds this pause stopped, to start again on continue.
    held: Vec<Entity>,
}

/// Run condition: the race is not paused.
pub fn running(pause: Res<Pause>) -> bool {
    !pause.on
}

pub struct PausePlugin;

impl Plugin for PausePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Pause::default())
            .insert_resource(PauseTest::from_env())
            .add_systems(
                Update,
                (pause_input.after(super::drive_car).after(super::console::apply), show_pause.after(super::feed_hud), pause_test.before(pause_input))
                    .run_if(in_state(AppState::Race)),
            )
            .add_systems(OnExit(AppState::Race), clear);
    }
}

/// Stops the race clock and every playing sound.
fn freeze(pause: &mut Pause, time: &mut Time<Virtual>, sinks: &mut Query<(Entity, &mut AudioSink)>, spatial: &mut Query<(Entity, &mut SpatialAudioSink)>, status: &RaceStatus) {
    pause.on = true;
    pause.view = PauseView::default();
    time.pause();
    pause.held.clear();
    for (e, s) in sinks.iter_mut() {
        if !s.is_paused() {
            s.pause();
            pause.held.push(e);
        }
    }
    for (e, s) in spatial.iter_mut() {
        if !s.is_paused() {
            s.pause();
            pause.held.push(e);
        }
    }
    log_line(&format!("pause: on at race time {:.3}s, {} sounds held", status.race_time, pause.held.len()));
}

/// Starts the clock and the held sounds again.
fn thaw(pause: &mut Pause, time: &mut Time<Virtual>, sinks: &mut Query<(Entity, &mut AudioSink)>, spatial: &mut Query<(Entity, &mut SpatialAudioSink)>, status: &RaceStatus) {
    pause.on = false;
    time.unpause();
    for e in pause.held.drain(..) {
        if let Ok((_, s)) = sinks.get_mut(e) {
            s.play();
        } else if let Ok((_, s)) = spatial.get_mut(e) {
            s.play();
        }
    }
    log_line(&format!("pause: off at race time {:.3}s", status.race_time));
}

/// Does what a pause menu button says.
#[allow(clippy::too_many_arguments)]
fn act(
    action: PauseAction,
    pause: &mut Pause,
    time: &mut Time<Virtual>,
    sinks: &mut Query<(Entity, &mut AudioSink)>,
    spatial: &mut Query<(Entity, &mut SpatialAudioSink)>,
    status: &RaceStatus,
    commands: &mut Commands,
    back: &ReturnToMenu,
    next: &mut NextState<AppState>,
    exit: &mut MessageWriter<AppExit>,
) {
    log_line(&format!("pause: {action:?}"));
    match action {
        PauseAction::Continue => thaw(pause, time, sinks, spatial, status),
        PauseAction::RaceAgain => {
            commands.insert_resource(flow::RestartRace);
            next.set(AppState::Loading);
        }
        PauseAction::Quit => pause.view = PauseView { page: PausePage::Quit, selected: 0 },
        PauseAction::Cancel => pause.view = PauseView { page: PausePage::Main, selected: 2 },
        PauseAction::ExitToMenu => flow::leave_race(back, next, exit),
        PauseAction::ExitToWindows => {
            exit.write(AppExit::Success);
        }
    }
}

/// Esc or P pauses; on the menu, up/down and the mouse pick a button, Enter, Space or a click
/// press it, and Esc goes back (to the race, or from the quit page to the menu).
#[allow(clippy::too_many_arguments)]
fn pause_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    console: Res<super::console::Console>,
    status: Res<RaceStatus>,
    mut pause: ResMut<Pause>,
    mut time: ResMut<Time<Virtual>>,
    mut sinks: Query<(Entity, &mut AudioSink)>,
    mut spatial: Query<(Entity, &mut SpatialAudioSink)>,
    mut commands: Commands,
    back: Res<ReturnToMenu>,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
) {
    if console.open {
        return;
    }
    if !pause.on {
        // the results panel has Esc of its own; retail pauses from the grid on
        if status.active && !status.finished && keys.any_just_pressed([KeyCode::Escape, KeyCode::KeyP]) {
            freeze(&mut pause, &mut time, &mut sinks, &mut spatial, &status);
        }
        return;
    }
    let mut pressed = None;
    if keys.any_just_pressed([KeyCode::Escape, KeyCode::KeyP]) {
        pressed = Some(if pause.view.page == PausePage::Quit { PauseAction::Cancel } else { PauseAction::Continue });
    }
    if keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) {
        pause.view.selected = (pause.view.selected + 1) % 3;
    }
    if keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) {
        pause.view.selected = (pause.view.selected + 2) % 3;
    }
    if let Ok(window) = windows.single() {
        let screen = [window.physical_width().max(1), window.physical_height().max(1)];
        if let Some(at) = window.physical_cursor_position() {
            if let Some(i) = hud_pause::hit(screen, hud_scale(), [at.x, at.y]) {
                pause.view.selected = i;
                if mouse.just_pressed(MouseButton::Left) {
                    pressed = Some(pause.view.page.actions()[i]);
                }
            }
        }
    }
    if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) {
        pressed = Some(pause.view.page.actions()[pause.view.selected]);
    }
    if let Some(action) = pressed {
        act(action, &mut pause, &mut time, &mut sinks, &mut spatial, &status, &mut commands, &back, &mut next, &mut exit);
    }
}

/// The HUD's scale (`NFSPU_HUD_SCALE`, as `hud_plugin` reads it), for hit tests.
fn hud_scale() -> f32 {
    std::env::var("NFSPU_HUD_SCALE").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(1.0f32).clamp(0.5, 8.0)
}

/// Hands the menu to the HUD (after the race has written this frame's HUD input).
fn show_pause(pause: Res<Pause>, feed: Option<ResMut<HudFeed>>) {
    if let Some(mut feed) = feed {
        feed.input.pause = pause.on.then_some(pause.view);
    }
}

/// Leaving the race (quit, race again or the results) always leaves the clock running.
fn clear(mut pause: ResMut<Pause>, mut time: ResMut<Time<Virtual>>) {
    if pause.on {
        log_line("pause: race left while paused");
    }
    pause.on = false;
    pause.held.clear();
    pause.view = PauseView::default();
    time.unpause();
}

/// `NFSPU_PAUSE_TEST`: when to pause and what to do then.
#[derive(Resource, Default)]
struct PauseTest {
    at: Option<f32>,
    action: String,
    /// Real time the test paused, and the race time it read then.
    paused: Option<(f32, f32)>,
    /// Real time the test resumed (resume test).
    resumed: Option<f32>,
    done: bool,
    /// A restart was asked for; the next race's start is checked.
    restarted: bool,
}

impl PauseTest {
    fn from_env() -> PauseTest {
        let Ok(v) = std::env::var("NFSPU_PAUSE_TEST") else { return PauseTest::default() };
        let mut parts = v.split(',');
        let at = parts.next().and_then(|t| t.trim().parse().ok());
        let action = parts.next().unwrap_or("capture").trim().to_string();
        PauseTest { at, action, ..Default::default() }
    }
}

#[allow(clippy::too_many_arguments)]
fn pause_test(
    mut test: ResMut<PauseTest>,
    real: Res<Time<Real>>,
    status: Res<RaceStatus>,
    mut pause: ResMut<Pause>,
    mut time: ResMut<Time<Virtual>>,
    mut sinks: Query<(Entity, &mut AudioSink)>,
    mut spatial: Query<(Entity, &mut SpatialAudioSink)>,
    mut commands: Commands,
    back: Res<ReturnToMenu>,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(at) = test.at else { return };
    let now = real.elapsed_secs();
    if test.restarted {
        // the new race: its clock starts again from zero
        if status.active && status.race_time > 0.5 {
            log_line(&format!("pause test: restart ok, new race running, race time {:.3}s, intentional exit", status.race_time));
            exit.write(AppExit::Success);
            test.at = None;
        }
        return;
    }
    if test.done {
        return;
    }
    match test.paused {
        None => {
            if status.active && status.race_time >= at && !pause.on {
                freeze(&mut pause, &mut time, &mut sinks, &mut spatial, &status);
                test.paused = Some((now, status.race_time));
            }
        }
        Some((since, race_time)) => {
            if let Some(r) = test.resumed {
                if now - r > 1.0 {
                    let moved = status.race_time - race_time;
                    log_line(&format!("pause test: after continue the race clock moved {moved:.3}s ({})", if moved > 0.5 { "OK" } else { "FAILED" }));
                    test.done = true;
                    exit.write(AppExit::Success);
                }
                return;
            }
            if now - since < 1.5 {
                return;
            }
            let held = status.race_time == race_time;
            log_line(&format!("pause test: race clock {:.3}s after 1.5 s paused ({})", status.race_time, if held { "frozen, OK" } else { "MOVED, FAILED" }));
            let action = test.action.clone();
            match action.as_str() {
                "resume" => {
                    act(PauseAction::Continue, &mut pause, &mut time, &mut sinks, &mut spatial, &status, &mut commands, &back, &mut next, &mut exit);
                    test.resumed = Some(now);
                }
                "restart" => {
                    act(PauseAction::RaceAgain, &mut pause, &mut time, &mut sinks, &mut spatial, &status, &mut commands, &back, &mut next, &mut exit);
                    test.restarted = true;
                }
                "quit" => {
                    act(PauseAction::Quit, &mut pause, &mut time, &mut sinks, &mut spatial, &status, &mut commands, &back, &mut next, &mut exit);
                    act(PauseAction::ExitToMenu, &mut pause, &mut time, &mut sinks, &mut spatial, &status, &mut commands, &back, &mut next, &mut exit);
                    test.done = true;
                }
                page => {
                    // capture (or capture_quit for the quit page): the paused screen to logs/
                    if page == "capture_quit" {
                        pause.view = PauseView { page: PausePage::Quit, selected: 0 };
                    }
                    test.done = true;
                    let path = super::log_path().with_file_name(if page == "capture_quit" { "pause_quit_capture.png" } else { "pause_capture.png" });
                    commands.spawn(Screenshot::primary_window()).observe(move |captured: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                        let result = captured.image.clone().try_into_dynamic().map_err(|e| format!("image conversion: {e}")).and_then(|image| image.to_rgb8().save(&path).map_err(|e| e.to_string()));
                        match result {
                            Ok(()) => log_line(&format!("pause test: saved {}, intentional exit", path.display())),
                            Err(e) => log_line(&format!("pause test: capture FAILED: {e}")),
                        }
                        exit.write(AppExit::Success);
                    });
                }
            }
        }
    }
}
