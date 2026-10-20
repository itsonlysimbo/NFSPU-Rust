//! Bevy side of the in-race HUD: loads the layout, words and dials from the install, paints the
//! HUD each frame into a window-sized transparent image (`hud_render`) and shows it over the 3D
//! view as a full-window UI image.
//!
//! `HudPlugin(state)` shows the HUD while the app is in `state`, when a `HudFeed` resource exists
//! on entering it (the game inserts one for every race from the menu).
//!
//! Environment switches (all optional):
//! - `NFSPU_HUD=1` turns the HUD on for a race started without the menu (and the free viewer);
//! - `NFSPU_HUD_CONFIG=1..4` picks the stock layout ("Config 1" by default);
//! - `NFSPU_HUD_SCALE=<f>` draws it bigger (1 = retail pixel sizes);
//! - `NFSPU_HUD_MPH=1` for miles per hour, `NFSPU_HUD_NIGHT=1` for the night dial art;
//! - `NFSPU_HUD_CAPTURE=1` saves `logs/hud_capture.png` (whole window) and `logs/hud_layer.png`
//!   (HUD only) after a few seconds, then quits. `NFSPU_HUD_CAPTURE_AT=<seconds>` sets when;
//!   `NFSPU_HUD_TEST_SPEED=<km/h>` replaces the car's speed (and the made-up rpm and gear) so a
//!   capture can check the needles at a known value.
//!
//! Each frame, whoever runs the race writes `HudFeed` (`input` and `car_model`).

use crate::hud::{self, Dials, HudInput, HudSettings, SplitTime};
use crate::hud_render::{paint, DashArt, HudCanvas};
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::window::PrimaryWindow;
use nfspu_formats::hudcfg;
use std::io::Write as _;
use std::path::PathBuf;

/// What the HUD should show this frame.
#[derive(Resource, Default, Debug, Clone)]
pub struct HudFeed {
    pub input: HudInput,
    /// Model file name of the player's car (`993`), to pick its dials.
    pub car_model: String,
}

/// Shows the HUD while the app is in the given state (set up on entering it when a `HudFeed`
/// exists, taken down on leaving it).
pub struct HudPlugin<S>(pub S);

/// `NFSPU_HUD=1`: the HUD for a race started without the menu.
pub fn enabled() -> bool {
    std::env::var("NFSPU_HUD").map(|v| v.trim() == "1").unwrap_or(false)
}

impl<S: States> Plugin for HudPlugin<S> {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(self.0.clone()), setup.run_if(resource_exists::<HudFeed>))
            .add_systems(OnExit(self.0.clone()), teardown)
            .add_systems(PostUpdate, draw.run_if(in_state(self.0.clone())))
            .add_systems(Last, capture.run_if(in_state(self.0.clone())));
    }
}

/// Removes the HUD's picture and state (its node is despawned here too).
fn teardown(world: &mut World) {
    let nodes: Vec<(Entity, Handle<Image>)> = world.query_filtered::<(Entity, &ImageNode), With<HudImage>>().iter(world).map(|(e, n)| (e, n.image.clone())).collect();
    for (e, image) in nodes {
        world.resource_mut::<Assets<Image>>().remove(&image);
        world.despawn(e);
    }
    world.remove_non_send::<HudState>();
    world.remove_resource::<HudFeed>();
}

#[derive(Component)]
struct HudImage;

/// Loaded data and the reusable canvas.
struct HudState {
    game_dir: PathBuf,
    layouts: Vec<hudcfg::HudLayout>,
    switches: Vec<hudcfg::HudSwitches>,
    config: String,
    settings: Option<HudSettings>,
    /// The record's saved screen and the screen the boxes were laid out for.
    saved: [i32; 2],
    screen: [u32; 2],
    boxes: Option<hud::Boxes>,
    cars: Vec<nfspu_formats::cardash::CarDash>,
    car_model: String,
    art: DashArt,
    canvas: HudCanvas,
    started: f32,
    capture: Option<(f32, bool)>,
}

fn env_f32(name: &str) -> Option<f32> {
    std::env::var(name).ok()?.trim().parse().ok()
}

fn env_on(name: &str) -> bool {
    std::env::var(name).map(|v| v.trim() == "1").unwrap_or(false)
}

fn logs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("logs")
}

/// Appends a line to `logs/hud.log`.
fn log(msg: &str) {
    let dir = logs_dir();
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("hud.log")) {
        let _ = writeln!(f, "{msg}");
    }
}

fn setup(world: &mut World) {
    let game_dir = PathBuf::from(std::env::var("NFSPU_GAME_DIR").unwrap_or_default());
    let _ = std::fs::remove_file(logs_dir().join("hud.log"));
    let data = game_dir.join("FEData").join("Data");
    let layouts = std::fs::read(data.join("nfs5-1.hud")).map(|d| hudcfg::parse_layouts(&d)).unwrap_or_default();
    let switches = std::fs::read(data.join("nfs5-2.hud")).map(|d| hudcfg::parse_switches(&d)).unwrap_or_default();
    let cars = std::fs::read(data.join("nfs5.car")).map(|d| nfspu_formats::cardash::parse(&d)).unwrap_or_default();
    let words = std::fs::read(game_dir.join("FEData").join("Locale").join("hud.loc")).ok().and_then(|d| nfspu_formats::loc::parse(&d, 0)).unwrap_or_default();
    let n = std::env::var("NFSPU_HUD_CONFIG").ok().and_then(|v| v.trim().parse::<u32>().ok()).unwrap_or(1).clamp(1, 4);
    let config = format!("Config {n}");
    log(&format!("hud: {} layout records, {} switch records, {} cars, {} words, layout '{config}'", layouts.len(), switches.len(), cars.len(), words.len()));

    let settings = hudcfg::find(&layouts, &switches, &config, [640, 480]).map(|i| HudSettings {
        layout: layouts[i].clone(),
        switches: switches[i].clone(),
        dials: Dials::CARRERA_993,
        mph: env_on("NFSPU_HUD_MPH"),
        night: env_on("NFSPU_HUD_NIGHT"),
        split: SplitTime::BestLap,
        // retail's "opponent list" option: 1 = metres (the user's fe_out.txt), 2 = time
        gaps: if std::env::var("NFSPU_HUD_GAPS").is_ok_and(|v| v.trim() == "time") { hud::GapMode::Time } else { hud::GapMode::Distance },
        words,
        scale: env_f32("NFSPU_HUD_SCALE").unwrap_or(1.0).clamp(0.5, 8.0),
    });
    if settings.is_none() {
        log(&format!("hud: no 640x480 record named '{config}', HUD not drawn"));
    }
    let capture = env_on("NFSPU_HUD_CAPTURE").then(|| (env_f32("NFSPU_HUD_CAPTURE_AT").unwrap_or(6.0), false));
    let started = world.resource::<Time>().elapsed_secs();
    let state = HudState {
        art: DashArt::new(&game_dir),
        game_dir,
        layouts,
        switches,
        config,
        settings,
        saved: [640, 480],
        screen: [0, 0],
        boxes: None,
        cars,
        car_model: String::new(),
        canvas: HudCanvas::new(1, 1),
        started,
        capture,
    };
    world.insert_non_send(state);
    world.spawn((
        HudImage,
        Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
        ImageNode::default(),
        GlobalZIndex(50),
    ));
}

fn rgba_image(width: u32, height: u32, rgba: Vec<u8>) -> Image {
    Image::new(Extent3d { width, height, depth_or_array_layers: 1 }, TextureDimension::D2, rgba, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default())
}

fn draw(
    state: Option<NonSendMut<HudState>>,
    feed: Option<Res<HudFeed>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut shown: Query<&mut ImageNode, With<HudImage>>,
    mut images: ResMut<Assets<Image>>,
) {
    let (Some(mut state), Some(feed), Ok(window)) = (state, feed, windows.single()) else { return };
    let screen = [window.physical_width().max(1), window.physical_height().max(1)];
    let state = &mut *state;
    // The car's dials.
    if state.car_model != feed.car_model {
        state.car_model = feed.car_model.clone();
        if let Some(settings) = &mut state.settings {
            match nfspu_formats::cardash::by_model(&state.cars, &feed.car_model) {
                Some(c) => {
                    settings.dials = Dials { speedo_kmh: c.speedo_kmh, speedo_mph: c.speedo_mph, tach: c.tach };
                    log(&format!("hud: car '{}' ({}) dials {:?}", c.name, c.model, settings.dials));
                }
                None => log(&format!("hud: car '{}' not in nfs5.car, using the 993 dials", feed.car_model)),
            }
        }
    }
    let Some(settings) = &state.settings else { return };
    // Lay out again when the window size changes. A record saved for exactly this size is used
    // as it is, as the game does; otherwise the 640x480 one is moved onto the screen.
    if state.screen != screen || state.boxes.is_none() {
        let exact = hudcfg::find(&state.layouts, &state.switches, &state.config, [screen[0] as i32, screen[1] as i32]);
        let mut s = settings.clone();
        state.saved = [640, 480];
        if let Some(i) = exact.filter(|_| settings.scale == 1.0) {
            s.layout = state.layouts[i].clone();
            state.saved = state.switches[i].screen;
        }
        let boxes = hud::layout(&s, state.saved, screen);
        log(&format!("hud: screen {}x{}, scale {}, layout record saved for {:?}", screen[0], screen[1], s.scale, state.saved));
        for slot in hudcfg::Slot::ALL {
            log(&format!("hud:   {slot:?} at {:?}", boxes.slot(slot)));
        }
        log(&format!("hud:   multi time at {:?}, digital tach at {:?}", boxes.multi_time, boxes.tach_digital));
        state.boxes = Some(boxes);
        state.screen = screen;
        state.canvas = HudCanvas::new(screen[0], screen[1]);
        log(&format!("hud: game dir {}", state.game_dir.display()));
    }
    let Some(boxes) = &state.boxes else { return };
    let mut input = feed.input.clone();
    if let Some(kmh) = env_f32("NFSPU_HUD_TEST_SPEED") {
        input.speed_kmh = kmh;
        (input.rpm, input.gear) = hud::placeholder_engine(kmh);
    }
    let list = hud::draws(&input, settings, boxes, screen);
    state.canvas.clear();
    paint(&mut state.canvas, &list, &mut state.art);
    for miss in state.art.misses.drain(..) {
        log(&miss);
    }
    let Ok(mut node) = shown.single_mut() else { return };
    let old = std::mem::replace(&mut node.image, images.add(rgba_image(state.canvas.width, state.canvas.height, state.canvas.rgba.clone())));
    images.remove(&old);
}

/// `NFSPU_HUD_CAPTURE=1`: saves the HUD layer and a window screenshot, then exits.
fn capture(state: Option<NonSendMut<HudState>>, time: Res<Time>, feed: Option<Res<HudFeed>>, mut commands: Commands) {
    let (Some(mut state), Some(feed)) = (state, feed) else { return };
    let Some((at, waiting)) = state.capture else { return };
    if waiting || time.elapsed_secs() - state.started < at {
        return;
    }
    state.capture = Some((at, true));
    let dir = logs_dir();
    let layer = dir.join("hud_layer.png");
    let c = &state.canvas;
    let saved = image_crate_save(&layer, c.width, c.height, &c.rgba);
    log(&format!("hud capture: input {:?}, test speed {:?}", feed.input, env_f32("NFSPU_HUD_TEST_SPEED")));
    log(&format!("hud capture: layer {} ({saved})", layer.display()));
    let path = dir.join("hud_capture.png");
    commands.spawn(Screenshot::primary_window()).observe(move |captured: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
        let result = captured.image.clone().try_into_dynamic().map_err(|e| format!("image conversion: {e}")).and_then(|image| image.to_rgb8().save(&path).map_err(|e| e.to_string()));
        match result {
            Ok(()) => log(&format!("hud capture: saved {}, intentional exit", path.display())),
            Err(e) => log(&format!("hud capture FAILED: {e}")),
        }
        exit.write(AppExit::Success);
    });
}

/// Saves RGBA8 pixels as a PNG through Bevy's image conversion.
fn image_crate_save(path: &std::path::Path, width: u32, height: u32, rgba: &[u8]) -> String {
    match rgba_image(width, height, rgba.to_vec()).try_into_dynamic() {
        Ok(image) => match image.to_rgba8().save(path) {
            Ok(()) => "saved".into(),
            Err(e) => format!("not saved: {e}"),
        },
        Err(e) => format!("not converted: {e}"),
    }
}
