//! Game logic that does not need a window: ground lookup, wall collision, the placeholder car and track loading.

pub mod ai;
pub mod car;
pub mod car_load;
pub mod car_sim;
pub mod car_model;
pub mod collision;
pub mod garage;
pub mod ground;
pub mod catalog;
pub mod chase_cam;
pub mod hud;
pub mod hud_plugin;
pub mod hud_render;
pub mod loading;
pub mod menu;
pub mod menu_canvas;
pub mod menu_help;
pub mod menu_movie;
pub mod menu_music;
pub mod quick_race;
pub mod race;
pub mod race_path;
pub mod race_setup;
pub mod refbuild;
pub mod sky;
pub mod track;
pub mod weather;
pub mod weather_fx;

/// The game files use a left-handed world (Direct3D style) and Bevy is right-handed, so loading the
/// numbers as they are would mirror everything left to right: the steering wheel would end up on
/// the right and text would read backwards. Multiplying x by this value (-1) when loading puts the
/// world the right way round. Evidence: the car's `.tpg` file puts the driver camera at x = -0.38
/// while the model's steering wheel and driver sit at x = +0.4. Set to 1.0 to undo (and flip the
/// cabin culling in the viewer, which depends on it).
///
/// This is the factor for **models** (cars, garage rooms), which are never mirrored by a race.
/// Track and race geometry use [`world_mirror_x`], which is this value or its opposite when the
/// race direction is "mirrored".
pub const MIRROR_X: f32 = -1.0;

/// File-to-world x factor for the track and race geometry: -1 normally, +1 for a mirrored race
/// (`Track1.Direction` "forward/mirrored" or "backward/mirrored"; the same flag retail's loading
/// screen uses). Set once at start-up before the track is read; see [`set_world_mirrored`].
static WORLD_MIRROR_X: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(MIRROR_X.to_bits());

/// Makes the track and race geometry a mirror image of the usual layout. Call before loading the
/// track (the race child process knows the choice from `NFSPU_RACE` / `NFSPU_LOADING`).
pub fn set_world_mirrored(mirrored: bool) {
    let x = if mirrored { -MIRROR_X } else { MIRROR_X };
    WORLD_MIRROR_X.store(x.to_bits(), std::sync::atomic::Ordering::Relaxed);
}

/// The current [`WORLD_MIRROR_X`] value.
pub fn world_mirror_x() -> f32 {
    f32::from_bits(WORLD_MIRROR_X.load(std::sync::atomic::Ordering::Relaxed))
}
