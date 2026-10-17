//! Game logic that does not need a window: ground lookup, wall collision, the placeholder car and track loading.

pub mod car;
pub mod car_model;
pub mod collision;
pub mod ground;
pub mod hud;
pub mod hud_plugin;
pub mod hud_render;
pub mod loading;
pub mod menu;
pub mod menu_canvas;
pub mod menu_help;
pub mod menu_movie;
pub mod menu_music;
pub mod race;
pub mod race_path;
pub mod refbuild;
pub mod sky;
pub mod track;

/// The game files use a left-handed world (Direct3D style) and Bevy is right-handed, so loading the
/// numbers as they are would mirror everything left to right: the steering wheel would end up on
/// the right and text would read backwards. Multiplying x by this value (-1) when loading puts the
/// world the right way round. Evidence: the car's `.tpg` file puts the driver camera at x = -0.38
/// while the model's steering wheel and driver sit at x = +0.4. Set to 1.0 to undo (and flip the
/// cabin culling in the viewer, which depends on it).
pub const MIRROR_X: f32 = -1.0;
