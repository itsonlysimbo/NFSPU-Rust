//! Milestone 3: a real track from the player's install with a placeholder car you can drive.
//! Writes `logs/m0.log` (start, heartbeat with car position and speed, panic, exit reason).
//!
//! DRIVE (default): W/Up throttle, S/Down brake and reverse, A/D or Left/Right steer, Space handbrake,
//! R reset to the start. C cycles the camera: chase, cockpit (the driver's eyes), orbit (circle the car:
//! right mouse button or a captured mouse rotates, wheel zooms). F switches to the free-fly camera and back.
//! N / B switch to the next / previous car (all cars in the install, traffic included), V to the
//! car's next body style, H toggles the headlights. F10 quits.
//! FLY: click in the window to capture the mouse and look around (Esc/Tab releases it, right-drag
//! also looks). W/A/S/D move, Q/E down/up, Shift = 5x speed, mouse wheel changes the base speed.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};

use bevy::render::render_resource::{Extent3d, Face, TextureDimension, TextureFormat};
use nfspu_formats::{crp, refpack, shpi};
use nfspu_game::car::{Car, Input as CarInput, RIDE_HEIGHT};
use nfspu_game::car_model::{self, CarOptions, Page, Piece};
use nfspu_game::collision::WallSet;
use nfspu_game::chase_cam::{self, CarFrame, ChaseCam, ChaseView};
use nfspu_game::ground::Ground;
use nfspu_game::race_path::{self, Direction, RoadPath};
use nfspu_game::{refbuild, sky, track};
use nfspu_game::MIRROR_X;
mod car_probe;
mod console;
mod flow;
mod menu_mode;
mod race_load;
mod race_mode;
mod visual_probe;

use std::collections::HashMap;
use std::{
    env, fs,
    io::{Read, Write},
    path::PathBuf,
    sync::OnceLock,
    time::Instant,
};

fn log_path() -> PathBuf {
    // Fixed location (project\logs), independent of the folder the program is started from.
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("logs").join("m0.log")
}
static START: OnceLock<Instant> = OnceLock::new();

fn log_line(msg: &str) {
    let secs = START.get().map(|s| s.elapsed().as_secs_f32()).unwrap_or(0.0);
    let path = log_path();
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "[{secs:8.2}s] {msg}");
    }
}

fn main() {
    START.get_or_init(Instant::now);
    let _ = fs::remove_file(log_path());
    log_line(&format!(
        "starting: pid {}, exe {:?}, started from {:?}",
        std::process::id(),
        env::current_exe().ok(),
        env::current_dir().ok()
    ));

    std::panic::set_hook(Box::new(|info| {
        log_line(&format!("PANIC: {info}"));
        eprintln!("PANIC: {info}
Press Enter to close this console.");
        let _ = std::io::stdin().read(&mut [0u8]);
    }));

    // Bevy exits with code 130 on Ctrl+C. Replace that with a handler that only logs it,
    // so a stray console interrupt cannot close the window, and we can see when one arrives.
    let hooked = bevy::app::ctrlc::set_handler(|| log_line("Ctrl+C / Ctrl+Break received (ignored)"));
    log_line(&format!("ctrl-c handler installed: {}", hooked.is_ok()));

    // One program for the front end and the races (`flow.rs`). `NFSPU_MENU=<screen>` starts at
    // the front end (empty means `singleplayer`); otherwise the race or free viewer named by the
    // arguments and `NFSPU_LOADING` / `NFSPU_RACE` starts directly.
    let menu_start = env::var("NFSPU_MENU").ok().map(|requested| {
        let trimmed = requested.trim();
        if trimmed.is_empty() { "singleplayer".to_string() } else { trimmed.to_ascii_lowercase() }
    });
    let config = flow::RaceConfig::from_env();
    let window = match &menu_start {
        Some(_) => Window { title: menu_mode::TITLE.to_string(), resolution: bevy::window::WindowResolution::new(1024, 768), ..default() },
        None => Window { title: format!("NFS Porsche Unleashed (Rust rewrite) - M3 - {}", config.track), ..default() },
    };
    let first = if menu_start.is_some() { flow::AppState::Menu } else { flow::AppState::Loading };
    log_line(&format!("flow: one program, starting in {first:?}"));

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.build().disable::<bevy::app::TerminalCtrlCHandlerPlugin>().set(WindowPlugin { primary_window: Some(window), ..default() }))
        .insert_state(first)
        .insert_resource(flow::ReturnToMenu(menu_start.is_some()))
        .insert_resource(ClearColor(RACE_CLEAR))
        .insert_resource(CarList(list_cars()))
        .insert_resource(LightsOn(false))
        .insert_resource(console::Console::default())
        .insert_resource(console::Pending::default())
        .insert_resource(console::catalog(&PathBuf::from(env::var("NFSPU_GAME_DIR").unwrap_or_default())))
        .insert_resource(FlySpeed(120.0))
        .insert_resource(MouseCaptured(false))
        .insert_resource(DriveMode(true))
        .insert_resource(visual_probe::Probe::from_env())
        .insert_resource(car_probe::CarProbe::from_env())
        .insert_resource(CamMode(0))
        .insert_resource(Orbit { yaw: 0.6, pitch: 0.35, radius: 6.0 })
        .insert_resource(RetailFov::from_env())
        .add_plugins(flow::FlowPlugin)
        // The front end; its screens are read now only when the program starts there.
        .add_plugins(menu_mode::MenuPlugin(menu_start.clone()))
        .add_plugins(race_mode::RacePlugin)
        // Runs `setup` directly, or behind the loading screen.
        .add_plugins(race_load::RaceLoad)
        // In-race HUD: always in a race from the menu, otherwise only with NFSPU_HUD=1.
        .add_plugins(nfspu_game::hud_plugin::HudPlugin(flow::AppState::Race))
        // Fog, rain, snow, lightning and night headlights (weather_fx.rs).
        .add_plugins(nfspu_game::weather_fx::WeatherFxPlugin)
        .add_plugins(nfspu_game::sky_fx::SkyFxPlugin)
        // The sky effects' camera, layers and glows are not tied to the race state; clear them on leaving it.
        .add_systems(OnExit(flow::AppState::Race), nfspu_game::sky_fx::clear)
        .add_systems(OnEnter(flow::AppState::Loading), |mut clear: ResMut<ClearColor>| clear.0 = RACE_CLEAR)
        .add_systems(Update, feed_weather.after(drive_car).run_if(flow::in_race_scene))
        .add_systems(Update, feed_hud.after(drive_car).run_if(flow::in_race_scene))
        // after the cars' final positions for this frame are known
        .add_systems(PostUpdate, lay_ground_shadows.after(TransformSystems::Propagate).run_if(flow::in_race_scene))
        .add_systems(Last, (visual_probe::update, car_probe::update).run_if(flow::in_race_scene))
        .add_systems(Update, (console::handle_input, console::apply).chain().run_if(flow::in_race_scene))
        .add_systems(
            Update,
            (sky_dome, heartbeat, mouse_capture, toggle_mode, camera_input, switch_car, drive_car, update_lamps, animate_wheels, driver_visibility, chase_camera, retail_fov, fly_camera, sky_tint, fps_counter, window_title)
                .chain()
                .run_if(flow::in_race_scene),
        )
        .add_systems(Update, quit_on_f10);
    // The race (or viewer) asked for at start-up; a menu start replaces it when RACE is pressed.
    config.apply(app.world_mut());
    let result = app.run();

    let msg = format!("app finished with: {result:?}");
    log_line(&msg);
    println!("{msg}");
    // A menu start ends from the menu (its launcher waits); the viewer keeps its console open.
    if menu_start.is_some() {
        return;
    }
    println!("A log was written to {}. Press Enter to close this console.", log_path().display());
    if env::var("NFSPU_SKY_CAPTURE").as_deref() != Ok("1") && env::var_os("NFSPU_CAR_CAPTURE").is_none() {
        let _ = std::io::stdin().read(&mut [0u8]);
    }
}

/// The colour behind the race's sky (the viewer's plain sky where a track has none).
const RACE_CLEAR: Color = Color::srgb(0.45, 0.62, 0.85);

#[derive(Resource)]
struct TrackName(String);

#[derive(Resource)]
struct CarName(String);

/// Every car in the install (the `.crp` files in GameData/CarModel), sorted; N / B step through it.
#[derive(Resource)]
struct CarList(Vec<String>);

/// The car body entity, the model entity under it (despawned and rebuilt when the car changes) and
/// how many body styles the current car has.
#[derive(Resource)]
struct CarModelEntity {
    root: Entity,
    model: Entity,
    style_count: usize,
}

/// Car names from the install, without `postship` (no body: wheels and a big decal only).
fn list_cars() -> Vec<String> {
    let Ok(dir) = car_model::car_dir() else { return Vec::new() };
    let mut cars: Vec<String> = fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("crp")))
                .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
                .filter(|n| !n.eq_ignore_ascii_case("postship"))
                .collect()
        })
        .unwrap_or_default();
    cars.sort_by_key(|n| n.to_ascii_lowercase());
    cars
}

/// N / B: next / previous car; V: next body style of the current car. Rebuilds the car model in place.
#[allow(clippy::too_many_arguments)]
fn switch_car(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    list: Res<CarList>,
    mut name: ResMut<CarName>,
    mut style: ResMut<CarStyle>,
    driver: Res<CarDriver>,
    paint: Res<CarPaint>,
    holder: Option<ResMut<CarModelEntity>>,
    console: Res<console::Console>,
) {
        if console.open {
            return;
        }
        let Some(mut holder) = holder else { return };
    let step: i32 = if keys.just_pressed(KeyCode::KeyN) {
        1
    } else if keys.just_pressed(KeyCode::KeyB) {
        -1
    } else {
        0
    };
    let next_style = keys.just_pressed(KeyCode::KeyV);
    if step == 0 && !next_style {
        return;
    }
    if step != 0 && !list.0.is_empty() {
        let at = list.0.iter().position(|n| n.eq_ignore_ascii_case(&name.0)).unwrap_or(0) as i32;
        let n = list.0.len() as i32;
        name.0 = list.0[((at + step).rem_euclid(n)) as usize].clone();
        style.0 = 1;
    } else if next_style {
        style.0 = if style.0 >= holder.style_count.max(1) { 1 } else { style.0 + 1 };
    }
    commands.entity(holder.model).despawn();
    let options = CarOptions { style: style.0, driver: driver.0, paint: paint.0, ..Default::default() };
    let (model, count) = spawn_car_model(&mut commands, &mut meshes, &mut materials, &mut images, holder.root, &name.0, &options);
    holder.model = model;
    holder.style_count = count;
    log_line(&format!("switched to car '{}' style {} of {}", name.0, style.0, count));
}

/// Which body style of the car to build (1 = the first style in its .tpg file, e.g. the 993 coupe).
#[derive(Resource)]
struct CarStyle(usize);

/// Which driver to seat in the car: 1 = racing suit and helmet, 2-10 = casual outfits, 0 = nobody.
#[derive(Resource)]
struct CarDriver(u8);

/// Paint scheme (section number in the car's .clr file; its first colour is the body colour).
#[derive(Resource)]
struct CarPaint(usize);

/// Headlights switched on (H). The tail lights glow dimly with them and brightly when braking.
#[derive(Resource)]
struct LightsOn(bool);

/// A headlight or tail light with its own material, so it can be made to glow.
#[derive(Component)]
struct Lamp {
    front: bool,
    material: Handle<StandardMaterial>,
}

fn update_lamps(
    keys: Res<ButtonInput<KeyCode>>,
    mut on: ResMut<LightsOn>,
    car: Option<Res<CarRes>>,
    lamps: Query<(Entity, &Lamp)>,
    new_lamps: Query<(), Added<Lamp>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut last: Local<Option<(bool, bool)>>,
    console: Res<console::Console>,
    parents: Query<&ChildOf>,
    ai: Query<&race_mode::AiBody>,
) {
    // opponents' lamps follow their own brakes, every frame (a write only when it changes)
    for (e, lamp) in &lamps {
        let Some(body) = opponent_of(e, &parents, &ai) else { continue };
        let want = lamp_colour(lamp.front, on.0, body.braking);
        if materials.get(&lamp.material).is_some_and(|m| m.base_color != want) {
            if let Some(mut m) = materials.get_mut(&lamp.material) {
                m.base_color = want;
            }
        }
    }
    if console.open {
        return;
    }
    if keys.just_pressed(KeyCode::KeyH) {
        on.0 = !on.0;
        log_line(if on.0 { "headlights on" } else { "headlights off" });
    }
    let braking = car.as_ref().is_some_and(|c| c.braking);
    // a newly built car (N / B / V) needs its lamps set even if nothing else changed
    if *last == Some((on.0, braking)) && new_lamps.is_empty() {
        return;
    }
    *last = Some((on.0, braking));
    for (e, lamp) in &lamps {
        if opponent_of(e, &parents, &ai).is_some() {
            continue;
        }
        let Some(mut m) = materials.get_mut(&lamp.material) else { continue };
        m.base_color = lamp_colour(lamp.front, on.0, braking);
    }
}

/// A lamp's colour. Values above 1.0 make the unlit texture brighter than normal, which reads as
/// glow.
fn lamp_colour(front: bool, lights_on: bool, braking: bool) -> Color {
    if front {
        if lights_on { Color::linear_rgb(2.6, 2.6, 2.0) } else { Color::WHITE }
    } else if braking {
        Color::linear_rgb(3.0, 0.35, 0.35)
    } else if lights_on {
        Color::linear_rgb(1.5, 0.4, 0.4)
    } else {
        Color::WHITE
    }
}

/// The opponent a car part belongs to (part -> model -> body), None for the player's car.
fn opponent_of(part: Entity, parents: &Query<&ChildOf>, ai: &Query<&race_mode::AiBody>) -> Option<race_mode::AiBody> {
    let model = parents.get(part).ok()?.parent();
    let body = parents.get(model).ok()?.parent();
    ai.get(body).ok().copied()
}

#[derive(Resource)]
struct FlySpeed(f32);

/// True while the mouse is captured (hidden and locked) for looking around.
#[derive(Resource)]
struct MouseCaptured(bool);

fn mouse_capture(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut captured: ResMut<MouseCaptured>,
    mut windows: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
) {
    let Ok((window, mut cursor)) = windows.single_mut() else { return };
    let was = captured.0;
    if !captured.0 && window.focused && buttons.just_pressed(MouseButton::Left) {
        captured.0 = true;
    }
    if captured.0 && (keys.just_pressed(KeyCode::Escape) || keys.just_pressed(KeyCode::Tab) || !window.focused) {
        captured.0 = false;
    }
    if captured.0 != was {
        cursor.grab_mode = if captured.0 { CursorGrabMode::Locked } else { CursorGrabMode::None };
        cursor.visible = !captured.0;
        log_line(if captured.0 { "mouse captured" } else { "mouse released" });
    }
}

#[derive(Component)]
struct FlyCamera {
    yaw: f32,
    pitch: f32,
}

fn heartbeat(time: Res<Time>, car: Option<Res<CarRes>>, mode: Res<DriveMode>, mut since_last: Local<f32>, mut frames: Local<u64>) {
    *frames += 1;
    *since_last += time.delta_secs();
    if *since_last >= 2.0 {
        let extra = match (&car, mode.0) {
            (Some(c), true) => format!(", car at ({:.0}, {:.1}, {:.0}) {:.0} km/h{}", c.car.pos.x, c.car.pos.y, c.car.pos.z, c.car.speed * 3.6, if c.car.on_ground { "" } else { " AIRBORNE" }),
            _ => String::new(),
        };
        log_line(&format!("alive, {} frames so far{extra}", *frames));
        *since_last = 0.0;
    }
}

/// Triangles that share one material, with its decoded texture (if any).
struct MaterialMesh {
    positions: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    texture: Option<TextureData>,
    /// Placed objects (signs) are drawn from the front only, so a sign's back doesn't show its
    /// artwork reversed; terrain and road stay double-sided.
    one_sided: bool,
}

#[derive(Clone)]
struct TextureData {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    has_alpha: bool,
}

struct LoadedTrack {
    groups: Vec<MaterialMesh>,
    articles: usize,
    triangles: usize,
    textured: usize,
}

/// For each batch of an object, whether it has a back-to-back partner in the same object: a batch
/// facing the opposite way over (nearly) the same area, like a sign's front and its back. Such
/// batches are drawn one-sided so each side shows its own artwork; single faces (a sign's flat post)
/// stay double-sided so they don't vanish from behind.
fn back_to_back_batches(m: &crp::ArticleMesh) -> Vec<bool> {
    let info: Vec<(Vec3, Vec3, Vec3)> = m
        .batches
        .iter()
        .map(|b| {
            let (mut n, mut lo, mut hi) = (Vec3::ZERO, Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            for t in 0..b.vertex_idx.len() / 3 {
                let p: Vec<Vec3> = (0..3).map(|q| Vec3::from(m.positions[b.vertex_idx[3 * t + q] as usize])).collect();
                n += (p[1] - p[0]).cross(p[2] - p[0]);
                for v in &p {
                    lo = lo.min(*v);
                    hi = hi.max(*v);
                }
            }
            (n.normalize_or_zero(), lo, hi)
        })
        .collect();
    let overlap = |a: &(Vec3, Vec3, Vec3), b: &(Vec3, Vec3, Vec3)| {
        // share of the larger box covered by the intersection (thin axes count as 1 cm)
        let size = |lo: Vec3, hi: Vec3| (hi - lo).max(Vec3::splat(0.01));
        let inter = size(a.1.max(b.1), a.2.min(b.2).max(a.1.max(b.1)));
        let vol = |s: Vec3| s.x * s.y * s.z;
        vol(inter) / vol(size(a.1, a.2)).max(vol(size(b.1, b.2)))
    };
    (0..info.len())
        .map(|i| (0..info.len()).any(|j| j != i && info[i].0.dot(info[j].0) < -0.9 && overlap(&info[i], &info[j]) > 0.5))
        .collect()
}

/// Append complete triangles only. A trailing corner belongs to this batch and cannot
/// become part of a triangle with the next material/article batch.
fn append_track_batch(
    g: &mut MaterialMesh,
    m: &crp::ArticleMesh,
    b: &crp::Batch,
    matrix: Option<&[f32; 16]>,
) -> usize {
    let complete = b.vertex_idx.len() / 3 * 3;
    for (k, &vi) in b.vertex_idx[..complete].iter().enumerate() {
        let p = m.positions[vi as usize];
        // row-vector transform: p * M, translation in the last row
        let p = match matrix {
            Some(t) => [
                p[0] * t[0] + p[1] * t[4] + p[2] * t[8] + t[12],
                p[0] * t[1] + p[1] * t[5] + p[2] * t[9] + t[13],
                p[0] * t[2] + p[1] * t[6] + p[2] * t[10] + t[14],
            ],
            None => p,
        };
        g.positions.push([p[0] * nfspu_game::world_mirror_x(), p[1], p[2]]); // mirrored, see world_mirror_x
        g.uvs.push(m.uvs[b.uv_idx[k] as usize]);
        // Vertex colours are stored as Direct3D colours: bytes blue, green, red, alpha. Verified
        // against retail frames: alps' snow section, autobahn and Normandie only match with
        // blue and red this way round (docs/provenance/weather.md).
        let c = b.colors[k];
        g.colors.push([
            (c[2] as f32 / 255.0).powf(2.2),
            (c[1] as f32 / 255.0).powf(2.2),
            (c[0] as f32 / 255.0).powf(2.2),
            1.0,
        ]);
    }
    b.vertex_idx.len() - complete
}

/// Reads one track from the player's install: geometry (LOD 0), materials and textures.
fn load_track(name: &str) -> Result<LoadedTrack, String> {
    if let Some(track) = race_load::take_preloaded(name) {
        return track;
    }
    let dir = env::var("NFSPU_GAME_DIR").map_err(|_| "NFSPU_GAME_DIR is not set".to_string())?;
    let base = PathBuf::from(dir).join("GameData").join("Track");
    let read = |ext: &str| -> Result<Vec<u8>, String> {
        let path = base.join(format!("{name}.{ext}"));
        let raw = fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        refpack::decompress(&raw).map_err(|e| format!("{}: {e}", path.display()))
    };
    let crp_data = read("crp")?;
    let fsh_data = read("fsh")?;
    let crp = crp::parse(&crp_data).map_err(|e| format!("{e}"))?;
    let archive = shpi::parse(&fsh_data).map_err(|e| format!("{e}"))?;

    // material index -> decoded texture
    let mut textures: HashMap<u16, TextureData> = HashMap::new();
    let mut by_name: HashMap<String, Option<(u32, u32, Vec<u8>, bool)>> = HashMap::new();
    for m in crp.materials() {
        let Some(tex) = &m.texture else { continue };
        let decoded = by_name.entry(tex.clone()).or_insert_with(|| {
            let e = archive.find(tex)?;
            let rgba = shpi::decode_rgba8(&fsh_data, e)?;
            let has_alpha = rgba.chunks_exact(4).any(|p| p[3] < 255);
            Some((e.width as u32, e.height as u32, rgba, has_alpha))
        });
        if let Some((w, h, rgba, a)) = decoded {
            textures.insert(m.index, TextureData { width: *w, height: *h, rgba: rgba.clone(), has_alpha: *a });
        }
    }

    let mut groups: HashMap<(u16, bool), MaterialMesh> = HashMap::new();
    let mut skipped_templates = 0;
    for a in 0..crp.articles.len() {
        let Some(m) = crp.article_mesh(a, 0) else { continue };
        // Articles with a transform are objects: placed ones (roadside signs `OBJ...`) carry their
        // position in it; ones whose transform sits at the origin are templates (traffic cars,
        // cones, people) that the game places by other data, not drawn here yet.
        let matrix = crp.article_transform(a, 0);
        if let Some(t) = &matrix {
            // horizontal distance only: the flagman template sits 68 m straight up
            if (t[12] * t[12] + t[14] * t[14]).sqrt() < 50.0 {
                skipped_templates += 1;
                continue;
            }
        }
        let paired = if matrix.is_some() { back_to_back_batches(&m) } else { vec![false; m.batches.len()] };
        for (bi, b) in m.batches.iter().enumerate() {
            let one_sided = paired[bi];
            let g = groups.entry((b.material, one_sided)).or_insert_with(|| MaterialMesh {
                positions: Vec::new(),
                uvs: Vec::new(),
                colors: Vec::new(),
                texture: None,
                one_sided,
            });
            let excluded = append_track_batch(g, &m, b, matrix.as_ref());
            if excluded > 0 {
                log_line(&format!("track '{name}': article {a} batch {bi} material {}: excluded {excluded} trailing corners ({} complete triangles)",
                    b.material, b.vertex_idx.len() / 3));
            }
        }
    }
    let mut out: Vec<((u16, bool), MaterialMesh)> = groups.into_iter().collect();
    out.sort_by_key(|(k, _)| *k);
    let groups: Vec<MaterialMesh> = out
        .into_iter()
        .map(|((i, _), mut g)| {
            g.texture = textures.get(&i).cloned();
            g
        })
        .collect();
    let triangles = groups.iter().map(|g| g.positions.len() / 3).sum();
    let textured = groups.iter().filter(|g| g.texture.is_some()).count();
    log_line(&format!("track '{name}': {skipped_templates} object templates at the origin not drawn"));
    Ok(LoadedTrack { groups, articles: crp.articles.len(), triangles, textured })
}

/// Shows the frame rate in a corner of the window (F3 toggles it; F is the fly camera).
fn fps_counter(
    keys: Res<ButtonInput<KeyCode>>,
    console: Res<console::Console>,
    time: Res<Time>,
    mut since: Local<f32>,
    mut frames: Local<u32>,
    mut shown: Local<bool>,
    mut text: Query<&mut Text, With<FpsText>>,
    mut node: Query<&mut Visibility, With<FpsText>>,
) {
    if keys.just_pressed(KeyCode::F3) && !console.open {
        *shown = !*shown;
        log_line(if *shown { "fps counter on" } else { "fps counter off" });
    }
    for mut v in &mut node {
        *v = if *shown { Visibility::Inherited } else { Visibility::Hidden };
    }
    *frames += 1;
    *since += time.delta_secs();
    if *since < 0.5 {
        return;
    }
    let fps = *frames as f32 / *since;
    *since = 0.0;
    *frames = 0;
    for mut t in &mut text {
        **t = format!("{fps:.0} fps");
    }
}

/// Spawns the frame-rate readout (hidden until F is pressed).
fn spawn_fps(commands: &mut Commands) {
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(6.0),
            right: Val::Px(8.0),
            padding: UiRect::all(Val::Px(3.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        FpsText,
        DespawnOnExit(flow::AppState::Race),
        Text::new(""),
        TextFont { font_size: FontSize::Px(15.0), ..default() },
        TextColor(Color::srgb(0.9, 0.95, 0.9)),
        Visibility::Hidden,
    ));
}

/// Marks the frame-rate readout.
#[derive(Component)]
struct FpsText;

/// The parts of the sky that follow the camera, so there is sky in every direction.
#[derive(Component)]
struct SkyFollows;

/// How far away the sky is, and how far the camera can see (the sky has to fit inside it).
const SKY_RADIUS: f32 = 3000.0;
const SKY_FAR: f32 = 4000.0;

/// Keeps the sky centred on the camera.
fn sky_dome(
    cam: Query<&Transform, (With<FlyCamera>, Without<SkyFollows>)>,
    mut parts: Query<&mut Transform, With<SkyFollows>>,
) {
    let Ok(cam) = cam.single() else { return };
    for mut p in &mut parts {
        p.translation = cam.translation;
    }
}

/// The sky of the loaded track, kept so `sky_tint` can blend it between its two weather regions.
#[derive(Resource, Default)]
struct SkyState {
    /// The dome's material.
    dome: Option<Handle<StandardMaterial>>,
    /// Sky colour of weather region A and B (the average of each region's sky-colour picture).
    regions: [[f32; 3]; 2],
    /// The colour the dome was last set to, so it is only touched when it changes.
    last: [f32; 3],
}

/// Blends the dome between weather region A and B by the sky `.bin`'s region table at the
/// camera's road slice (retail passes that blend to every sky layer). The region colours
/// themselves are averages of the region pictures, which is our simplification of retail's
/// textured sky layers.
fn sky_tint(
    state: Option<ResMut<SkyState>>,
    weather: Option<Res<nfspu_game::weather_fx::WeatherState>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(mut state) = state else { return };
    let t = weather.map_or(0.0, |w| w.region_blend.clamp(0.0, 1.0));
    let [a, b] = state.regions;
    let want = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
    if (0..3).all(|i| (want[i] - state.last[i]).abs() < 0.004) {
        return;
    }
    state.last = want;
    if let Some(h) = state.dome.clone() {
        if let Some(mut m) = materials.get_mut(&h) {
            m.base_color = Color::srgb(want[0], want[1], want[2]);
        }
    }
}

/// The track's sky, from `GameData/Track/Sky/<track>.{txt,fsh,bin}` (see `sky.rs`). Two parts:
/// a dome carrying the weather region's sky colour, and a band round the horizon carrying `horz`,
/// the painted panorama of the mountains that stand behind the track in the retail game. The
/// `.bin` tables then move the sky colour and the weather along the lap (`sky_tint`, `weather_fx`).
///
/// Returns the entities it spawned so a track reload can despawn them.
fn spawn_sky(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    track: &str,
) -> Vec<Entity> {
    let mut spawned = Vec::new();
    let Ok(dir) = env::var("NFSPU_GAME_DIR") else { return spawned };
    let Some(data) = sky::load(&PathBuf::from(dir), track) else {
        log_line(&format!("sky '{track}': no Sky folder, keeping the plain sky colour"));
        return spawned;
    };
    if data.pictures.is_empty() && data.horizon.is_empty() {
        log_line(&format!("sky '{track}': no sky pictures and no colour table, keeping the plain sky colour"));
        return spawned;
    }

    // Each weather region has its own sky colour picture; the dome blends between them (sky_tint).
    let base = data.regions.first().map(|r| r.tint(&data.pictures)).unwrap_or([0.55, 0.62, 0.72]);
    let region_b = data.regions.get(1).map(|r| r.tint(&data.pictures)).unwrap_or(base);
    let horizon_now = base;

    // --- the retail sky layers (sky_fx), at the horizon band's scale: radius / retail's 400 ---
    let sky_scale = data.layout.as_ref().map_or(SKY_RADIUS * 0.95, horizon_band_radius) / 400.0;
    let layers = nfspu_game::sky_fx::spawn(commands, meshes, materials, images, &data, sky_scale);
    let retail_layers = !layers.is_empty();
    spawned.extend(layers);

    // --- fallback dome when the track has no sky level: sky colour, brightest at the horizon ---
    let mut dome_handle = None;
    if !retail_layers {
        let dome_mat = materials.add(StandardMaterial {
            base_color: Color::srgb(base[0], base[1], base[2]),
            base_color_texture: Some(images.add(sky_gradient())),
            unlit: true,
            cull_mode: None,
            // The sky is not fogged: only the world is.
            fog_enabled: false,
            ..default()
        });
        spawned.push(
            commands
                .spawn((Mesh3d(meshes.add(sky_dome_mesh())), MeshMaterial3d(dome_mat.clone()), Transform::default(), SkyFollows, nfspu_game::weather_fx::SkyPart))
                .id(),
        );
        dome_handle = Some(dome_mat);
    }

    // Verified atlas and hzcl vertex assignment; overall sky interpretation is still incomplete.
    let mut band_handle = None;
    if let (Some((px, w, h)), Some(layout)) = (data.pictures.find("horz"), data.layout.as_ref()) {
        let mut image = Image::new(
            Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 },
            TextureDimension::D2,
            px.to_vec(),
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        // Atlas UVs stay in 0..1; each segment selects one configured cell.
        image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::ClampToEdge,
            address_mode_v: ImageAddressMode::ClampToEdge,
            mag_filter: ImageFilterMode::Linear,
            min_filter: ImageFilterMode::Linear,
            ..default()
        });
        let vertex_colours = layout.vertex_colours(&data.pictures);
        let verified_palette = vertex_colours.is_some();
        let mut band_mesh = horizon_band_mesh(layout);
        if let Some(colours) = vertex_colours {
            let linear: Vec<[f32; 4]> = colours.iter().map(|c| {
                Color::srgba_u8(c[0], c[1], c[2], c[3]).to_linear().to_f32_array()
            }).collect();
            band_mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, linear);
        }
        let mat = materials.add(StandardMaterial {
            base_color: if verified_palette { Color::WHITE } else { Color::srgb(horizon_now[0], horizon_now[1], horizon_now[2]) },
            base_color_texture: Some(images.add(image)),
            // The picture has sky above the mountains left transparent, so the dome shows through.
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            // Retail draws the horizon last in the sky pass, under the sky fog (sky_fx ranks).
            depth_bias: nfspu_game::sky_fx::HORIZON_DEPTH_BIAS,
            ..default()
        });
        spawned.push(
            commands
                .spawn((Mesh3d(meshes.add(band_mesh)), MeshMaterial3d(mat.clone()), Transform::default(), SkyFollows, nfspu_game::weather_fx::SkyPart))
                .id(),
        );
        log_line(&format!("sky '{track}': atlas 16 segments, {}x{}, cells {:?}, flips {:?}, zero {:.7}, colour-start {} (colour mapping unconfirmed), reference y {:.5}..{:.5}, radius 400",
            layout.columns, layout.rows, layout.cells, layout.flipped, layout.zero_turn,
            layout.colour_start, layout.bottom, layout.bottom + layout.height));
        log_line(if verified_palette { "horizon: verified hzcl 16x2 vertex colours; no guessed whole-band tint" }
            else { "horizon: hzcl unavailable/invalid; guessed whole-band tint fallback (unconfirmed)" });
        // Direction tint is retained only for the unconfirmed fallback, never over hzcl colours.
        band_handle = if verified_palette { None } else { Some(mat) };
    }

    let _ = band_handle;
    commands.insert_resource(SkyState { dome: dome_handle, regions: [base, region_b], last: base });
    log_line(&format!(
        "sky '{track}': {} level, {} pictures, {} weather regions, {} horizon colours ({})",
        if data.level.is_empty() { "unknown".to_string() } else { data.level.clone() },
        data.pictures.len(),
        data.regions.len(),
        data.horizon.len(),
        if data.horizon.is_empty() {
            "no warm/cold change".to_string()
        } else {
            let warm = data.horizon.iter().filter(|h| h.rgb()[0] > h.rgb()[2] + 0.03).count();
            let cold = data.horizon.iter().filter(|h| h.rgb()[2] > h.rgb()[0] + 0.03).count();
            format!("{warm} warm, {cold} cold")
        }
    ));
    spawned
}

/// A small picture for the dome: white at the horizon, fading to a darker sky above and to dark
/// ground below. The material's colour multiplies it, so one small texture serves every colour the
/// colour table asks for.
fn sky_gradient() -> Image {
    let (w, h) = (8usize, 64usize);
    let mut data = vec![0u8; w * h * 4];
    for y in 0..h {
        let v = y as f32 / (h - 1) as f32; // 0 at the top of the dome, 1 at the bottom
        let lum = if v < 0.5 {
            // towards the horizon: brighten, faster as it gets there
            let t = v / 0.5;
            0.45 + 0.55 * t * t
        } else {
            // below the horizon: fade away into the ground
            let t = (v - 0.5) / 0.5;
            1.0 - 0.72 * t * t
        };
        let c = (lum.clamp(0.0, 1.0) * 255.0) as u8;
        for x in 0..w {
            data[(y * w + x) * 4..][..4].copy_from_slice(&[c, c, c, 255]);
        }
    }
    let mut image = Image::new(
        Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}

/// A sphere round the camera with the picture wrapped round it: u goes all the way round, v goes
/// from the top down to the bottom.
fn sky_dome_mesh() -> Mesh {
    let (rings, sectors) = (32u32, 64u32);
    let mut positions = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    for r in 0..=rings {
        let theta = r as f32 / rings as f32 * std::f32::consts::PI;
        for s in 0..=sectors {
            let phi = s as f32 / sectors as f32 * std::f32::consts::TAU;
            let d = [theta.sin() * phi.cos(), theta.cos(), theta.sin() * phi.sin()];
            positions.push([d[0] * SKY_RADIUS, d[1] * SKY_RADIUS, d[2] * SKY_RADIUS]);
            uvs.push([s as f32 / sectors as f32, r as f32 / rings as f32]);
        }
    }
    for r in 0..rings {
        for s in 0..sectors {
            let a = r * (sectors + 1) + s;
            let b = a + sectors + 1;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    finish_mesh(positions, uvs, indices)
}

/// Independent atlas quads keep discontinuous cell/flip UVs from interpolating across seams.
/// How far out the horizon band sits: keeps even its highest and lowest corners inside the sky.
/// Uniform scaling preserves retail angular proportions; a cylindrical radius alone does not
/// bound corner distance.
fn horizon_band_radius(layout: &sky::HorizonLayout) -> f32 {
    let vertical = layout.bottom.abs().max((layout.bottom + layout.height).abs()) / 400.0;
    SKY_RADIUS * 0.95 / (1.0 + vertical * vertical).sqrt()
}

fn horizon_band_mesh(layout: &sky::HorizonLayout) -> Mesh {
    let mut positions = Vec::with_capacity(64);
    let mut uvs = Vec::with_capacity(64);
    let mut indices = Vec::with_capacity(96);
    let radius = horizon_band_radius(layout);
    for (quad, tex) in layout.quads(radius, nfspu_game::world_mirror_x()) {
        let first = positions.len() as u32;
        positions.extend(quad);
        uvs.extend(tex);
        indices.extend([first, first + 1, first + 2, first, first + 2, first + 3]);
    }
    finish_mesh(positions, uvs, indices)
}

fn finish_mesh(positions: Vec<[f32; 3]>, uvs: Vec<[f32; 2]>, indices: Vec<u32>) -> Mesh {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}
fn setup(
    mut commands: Commands,
    track: Res<TrackName>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    car_name: Res<CarName>,
    car_style: Res<CarStyle>,
    car_driver: Res<CarDriver>,
    car_paint: Res<CarPaint>,
    quick: Res<flow::QuickSetup>,
) {
    console::spawn(&mut commands);
    spawn_fps(&mut commands);
    let scene = match spawn_track(&mut commands, &mut meshes, &mut materials, &mut images, &track.0) {
        Ok(s) => s,
        Err(e) => {
            error!("could not load track '{}': {e}", track.0);
            log_line(&format!("LOAD FAILED: {e}"));
            commands.spawn((Camera3d::default(), FlyCamera { yaw: 0.0, pitch: 0.0 }, nfspu_game::weather_fx::WorldCamera, DespawnOnExit(flow::AppState::Race)));
            return;
        }
    };
    let g = scene.ground;
    log_line(&format!(
        "ground: {} up-facing triangles (road {}, terrain {}), {} wall triangles; start {:?} yaw {:.3} ({})",
        g.tris.len(), g.road_triangles, g.terrain_triangles, g.walls.len(), g.start_pos, g.start_yaw, g.start_note
    ));
    commands.insert_resource(TrackEntities(scene.entities));
    commands.insert_resource(GroundRes(Ground::new(g.tris, 16.0)));
    commands.insert_resource(WallRes(WallSet::new(g.walls, 4.0)));
    commands.insert_resource(CarRes { car: player_car(g.start_pos, g.start_yaw, &car_name.0, quick.0.as_ref()), start_pos: g.start_pos, start_yaw: g.start_yaw, steer: 0.0, braking: false });
    commands.spawn((
        Camera3d::default(),
        // the sky dome sits 3 km away, so the camera has to be able to see that far
        Projection::Perspective(PerspectiveProjection { far: SKY_FAR, ..default() }),
        // Weather fog, coloured and weighted by the track's `.bin` weather table; transparent
        // until the sky system finds precipitation.
        DistanceFog {
            color: Color::NONE,
            directional_light_color: Color::NONE,
            directional_light_exponent: 0.0,
            falloff: FogFalloff::Linear { start: 60.0, end: 900.0 },
        },
        scene.camera,
        FlyCamera { yaw: scene.yaw, pitch: scene.pitch },
        nfspu_game::weather_fx::WorldCamera,
        DespawnOnExit(flow::AppState::Race),
    ));
    let root = commands.spawn((Transform::from_translation(g.start_pos), Visibility::default(), CarBody, DespawnOnExit(flow::AppState::Race))).id();
    let options = CarOptions { style: car_style.0, driver: car_driver.0, paint: car_paint.0, ..Default::default() };
    let (model, style_count) = spawn_car_model(&mut commands, &mut meshes, &mut materials, &mut images, root, &car_name.0, &options);
    commands.insert_resource(CarModelEntity { root, model, style_count });
}

/// A loaded track: its mesh entities (despawn them to unload it), where the camera goes and the
/// drivable ground.
struct TrackScene {
    entities: Vec<Entity>,
    camera: Transform,
    yaw: f32,
    pitch: f32,
    ground: track::GroundData,
}

/// The track's mesh entities, so the console can despawn them before loading another track.
#[derive(Resource, Default)]
struct TrackEntities(Vec<Entity>);

/// Reads a track and spawns its meshes; the same path for the first load and for a console reload.
fn spawn_track(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    name: &str,
) -> Result<TrackScene, String> {
    let started = Instant::now();
    let loaded = load_track(name)?;

    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for g in &loaded.groups {
        for p in &g.positions {
            lo = lo.min(Vec3::from(*p));
            hi = hi.max(Vec3::from(*p));
        }
    }
    let centre = (lo + hi) / 2.0;
    log_line(&format!(
        "track '{}': {} articles, {} materials ({} textured), {} triangles, loaded in {:.2}s; bounds {:?} .. {:?}",
        name,
        loaded.articles,
        loaded.groups.len(),
        loaded.textured,
        loaded.triangles,
        started.elapsed().as_secs_f32(),
        lo,
        hi
    ));

    let mut entities = Vec::new();
    for g in loaded.groups {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, g.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, g.uvs);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, g.colors);
        mesh.compute_flat_normals();

        let mut material = StandardMaterial {
            base_color: Color::WHITE,
            unlit: true,
            double_sided: !g.one_sided,
            // the files' front faces are clockwise; after the x mirror Bevy sees them as its back faces
            cull_mode: if g.one_sided { Some(if nfspu_game::world_mirror_x() < 0.0 { Face::Back } else { Face::Front }) } else { None },
            ..default()
        };
        if let Some(t) = g.texture {
            let mut image = Image::new(
                Extent3d { width: t.width, height: t.height, depth_or_array_layers: 1 },
                TextureDimension::D2,
                t.rgba,
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::default(),
            );
            image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                mag_filter: ImageFilterMode::Linear,
                min_filter: ImageFilterMode::Linear,
                ..default()
            });
            material.base_color_texture = Some(images.add(image));
            if t.has_alpha {
                material.alpha_mode = AlphaMode::Mask(0.5);
            }
        }
        entities.push(commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(materials.add(material)))).id());
    }

    let ground = track::load_ground(name)?;
    entities.extend(spawn_sky(commands, meshes, materials, images, name));
    entities.extend(nfspu_game::sky_fx::spawn_glows(commands, meshes, materials, images, name));
    let weather_started = Instant::now();
    let weather = nfspu_game::weather_fx::WeatherState::load(name, SKY_RADIUS);
    log_line(&format!("{} (loaded in {:.2}s)", weather.track.describe(), weather_started.elapsed().as_secs_f32()));
    commands.insert_resource(weather);
    // Start above the middle of the track, looking along the longest horizontal direction.
    let start = Vec3::new(centre.x, hi.y + 150.0, centre.z);
    let look_at = Vec3::new(centre.x, centre.y, centre.z - 300.0);
    let camera = Transform::from_translation(start).looking_at(look_at, Vec3::Y);
    let (yaw, pitch, _) = camera.rotation.to_euler(EulerRot::YXZ);
    log_line(&format!("camera start {:?}", start));
    Ok(TrackScene { entities, camera, yaw, pitch, ground })
}

/// Unloads the current track, loads `name` instead and puts the car at its start line. Used by the
/// console's `track` command; returns false and keeps the old track if the new one fails to load.
fn reload_track(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    name: &str,
    old: Option<&TrackEntities>,
    car: Option<&mut CarRes>,
    ground: Option<&mut GroundRes>,
    walls: Option<&mut WallRes>,
    mut cams: Query<&mut Transform, With<FlyCamera>>,
) -> bool {
    // read the new track first: if it fails, nothing has been thrown away yet
    let Ok(scene) = spawn_track(commands, meshes, materials, images, name) else {
        log_line(&format!("could not load track '{name}'; keeping the current one"));
        return false;
    };
    for e in old.map(|o| o.0.clone()).unwrap_or_default() {
        commands.entity(e).despawn();
    }
    let g = scene.ground;
    log_line(&format!(
        "track '{name}' loaded: {} ground triangles, {} wall triangles; start {:?} yaw {:.3} ({})",
        g.tris.len(), g.walls.len(), g.start_pos, g.start_yaw, g.start_note
    ));
    if let Some(ground) = ground {
        ground.0 = Ground::new(g.tris, 16.0);
    }
    if let Some(walls) = walls {
        walls.0 = WallSet::new(g.walls, 4.0);
    }
    if let Some(car) = car {
        car.car = car.car.reset(g.start_pos, g.start_yaw);
        car.start_pos = g.start_pos;
        car.start_yaw = g.start_yaw;
        car.steer = 0.0;
        car.braking = false;
    }
    for mut t in &mut cams {
        *t = scene.camera;
    }
    commands.insert_resource(TrackEntities(scene.entities));
    true
}

/// Builds the car model `name` as a child of `root` (the car body entity): textured pieces, cabins,
/// lamps, driver and wheels, or a plain box if the car cannot be loaded. Returns the model entity
/// (despawn it to remove the car) and the number of body styles the car has.
fn spawn_car_model(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    root: Entity,
    name: &str,
    options: &CarOptions,
) -> (Entity, usize) {
    // the model's origin is at ground level, the car's position is its centre
    let model = commands.spawn((Transform::from_xyz(0.0, -RIDE_HEIGHT, 0.0), Visibility::default())).id();
    commands.entity(root).add_child(model);
    let m = match car_model::load_car(name, options) {
        Ok(m) => m,
        Err(e) => {
            log_line(&format!("car model '{}' failed ({e}); using a plain box", name));
            let red = materials.add(StandardMaterial { base_color: Color::srgb(0.85, 0.1, 0.1), unlit: true, ..default() });
            let box_mesh = commands.spawn((Mesh3d(meshes.add(Cuboid::new(1.8, 0.7, 4.4))), MeshMaterial3d(red), Transform::from_xyz(0.0, RIDE_HEIGHT, 0.0))).id();
            commands.entity(model).add_child(box_mesh);
            return (model, 0);
        }
    };
    log_line(&format!(
        "car '{}' style '{}': {} parts, {} triangles, {} wheels, size {:.2} x {:.2} x {:.2} m (width x height x length), {} texture pages, eye {:?} turn {:?}",
        m.name, m.style_name, m.articles_used, m.triangles(), m.wheels.len(), m.size[0], m.size[1], m.size[2], m.pages.iter().flatten().count(), m.driver_eye, m.driver_rot
    ));
    commands.entity(model).insert((
        CockpitView { eye: Vec3::from(m.driver_eye), pitch: m.driver_rot[0].to_radians(), yaw: m.driver_rot[1].to_radians() },
        CarBounds { centre_height: m.centre_height, half_length: m.size[2] / 2.0 },
    ));
    let mut looks = CarLooks::new(&m, images);
    // Faces are drawn one-sided, as the engine does: the cabin, interior glass and the inner faces of
    // the shell are separate parts that face inwards. Glass blends; the arch liners sample a texel
    // whose alpha is 0 and are drawn without cut-outs.
    let pieces = [
        (&m.paint, Look::Cut),
        (&m.paint_inner, Look::Cut),
        (&m.trim, Look::Cut),
        (&m.trim_solid, Look::Solid),
        (&m.glass, Look::Glass),
        (&m.cabrio_glass, Look::Glass),
    ];
    for (piece, look) in pieces {
        for (page, part) in by_page(piece) {
            let mat = looks.material(page, look, materials);
            let e = commands.spawn((Mesh3d(meshes.add(piece_mesh(&part, [1.0; 3], look.alpha()))), MeshMaterial3d(mat), Transform::default())).id();
            commands.entity(model).add_child(e);
        }
    }
    // two cabins: the detailed one only in the cockpit view, the simpler one (which fits inside the
    // body) in every other view
    for (piece, cockpit) in [(&m.interior, true), (&m.interior_outside, false)] {
        for (page, part) in by_page(piece) {
            let mat = looks.material(page, Look::Cut, materials);
            let e = commands.spawn((Mesh3d(meshes.add(piece_mesh(&part, [1.0; 3], 1.0))), MeshMaterial3d(mat), Transform::default(), CabinPart { cockpit })).id();
            commands.entity(model).add_child(e);
        }
    }
    // lamps get their own copy of the material so they can glow independently
    for (piece, front) in [(&m.headlights, true), (&m.taillights, false)] {
        for (page, part) in by_page(piece) {
            let shared = looks.material(page, Look::Cut, materials);
            let Some(copy) = materials.get(&shared).cloned() else { continue };
            let handle = materials.add(copy);
            let e = commands
                .spawn((Mesh3d(meshes.add(piece_mesh(&part, [1.0; 3], 1.0))), MeshMaterial3d(handle.clone()), Transform::default(), Lamp { front, material: handle }))
                .id();
            commands.entity(model).add_child(e);
        }
    }
    // the driver is hidden in the cockpit view, where the camera sits inside the head
    for piece in [&m.driver_suit, &m.driver_head] {
        for (page, part) in by_page(piece) {
            let mat = looks.material(page, Look::Cut, materials);
            let e = commands.spawn((Mesh3d(meshes.add(piece_mesh(&part, [1.0; 3], 1.0))), MeshMaterial3d(mat), Transform::default(), DriverPart)).id();
            commands.entity(model).add_child(e);
        }
    }
    // the wheel shadow quads: upright squares outside the wheels, blended from the shared 4444
    // shadow page, darken the tops of the tyres under the arches
    for (page, part) in by_page(&m.shadow) {
        let mat = looks.material(page, Look::Shadow, materials);
        let e = commands.spawn((Mesh3d(meshes.add(piece_mesh(&part, [1.0; 3], 1.0))), MeshMaterial3d(mat), Transform::default())).id();
        commands.entity(model).add_child(e);
    }
    // the ground shadow: a dark rectangle the size of the car laid on the road under it (its mesh
    // is rebuilt every frame by `lay_ground_shadows`)
    match ground_shadow_material(images, materials) {
        Ok(mat) => {
            let shadow = m.ground_shadow();
            let mesh = meshes.add(ground_shadow_mesh(shadow, car_model::SHADOW_MAX_CELLS, |p| [p[0], SHADOW_LIFT, p[1]]));
            let e = commands.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::default(), NoFrustumCulling, GroundShadowPart(shadow))).id();
            commands.entity(model).add_child(e);
        }
        Err(e) => log_line(&format!("no ground shadow: {e}")),
    }
    for w in &m.wheels {
        let wheel = commands.spawn((Transform::from_translation(Vec3::from(w.offset)), Visibility::default(), WheelComp { front: w.front, spin: 0.0 })).id();
        commands.entity(model).add_child(wheel);
        for (page, part) in by_page(&w.piece) {
            let mat = looks.material(page, Look::Cut, materials);
            let e = commands.spawn((Mesh3d(meshes.add(piece_mesh(&part, [1.0; 3], 1.0))), MeshMaterial3d(mat), Transform::default())).id();
            commands.entity(wheel).add_child(e);
        }
    }
    (model, m.style_count)
}

/// How a car part is drawn: with cut-outs where its texture is transparent, fully opaque, or as
/// see-through glass.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Look {
    Cut,
    Solid,
    Glass,
    /// The wheel shadow quads: blended by their page's alpha and seen from either side.
    Shadow,
}

/// How far (metres) the ground shadow is raised above the road so it draws over it.
const SHADOW_LIFT: f32 = 0.03;
/// The engine looks for the road under each shadow point from this high above the car's ground
/// (metres), and keeps the car's own ground where the road found is this far off it or more.
const SHADOW_PROBE_UP: f32 = 1.75;
const SHADOW_MAX_STEP: f32 = 1.5;

/// The alpha that darkens a pixel as much as `a` does when blending on screen values (as the
/// engine's renderer did). Bevy blends linear light, where a black overlay of alpha `a` changes
/// the screen value far less on a dark road, so the shadows are made stronger to match:
/// screen value * (1 - a) is about linear * (1 - a)^2.2.
fn screen_alpha(a: u8) -> u8 {
    let keep = 1.0 - a as f32 / 255.0;
    ((1.0 - keep.powf(2.2)) * 255.0).round() as u8
}

/// Marks a car's ground shadow mesh (a child of the model entity) and holds its rectangle.
#[derive(Component)]
struct GroundShadowPart(car_model::GroundShadow);

/// The shadow as a grid of `cells` x `cells`; `place` turns each model-space point (x, z) into the
/// mesh position.
fn ground_shadow_mesh(shadow: car_model::GroundShadow, cells: usize, mut place: impl FnMut([f32; 2]) -> [f32; 3]) -> Mesh {
    let (points, uvs, indices) = car_model::shadow_grid(shadow, cells);
    let positions: Vec<[f32; 3]> = points.into_iter().map(&mut place).collect();
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// The engine's `SHAD` picture, blended over the road and drawn in front of it. Its alpha is
/// converted for Bevy's linear-light blending (`screen_alpha`).
fn ground_shadow_material(images: &mut Assets<Image>, materials: &mut Assets<StandardMaterial>) -> Result<Handle<StandardMaterial>, String> {
    let (width, height, mut rgba) = car_model::shadow_picture()?;
    for px in rgba.chunks_exact_mut(4) {
        px[3] = screen_alpha(px[3]);
    }
    let mut image = Image::new(Extent3d { width, height, depth_or_array_layers: 1 }, TextureDimension::D2, rgba, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor { mag_filter: ImageFilterMode::Linear, min_filter: ImageFilterMode::Linear, ..default() });
    Ok(materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(images.add(image)),
        unlit: true,
        cull_mode: None,
        alpha_mode: AlphaMode::Blend,
        depth_bias: 1000.0,
        ..default()
    }))
}

/// Lays every car's shadow on the road, as the engine does each frame: the rectangle turns with the
/// car, is split into fewer cells the further the camera is, and each point drops onto the road
/// found under it (the car's own ground where there is none, or where it is a step away). The
/// player's shadow is not drawn in the cockpit view.
fn lay_ground_shadows(
    mut meshes: ResMut<Assets<Mesh>>,
    mut shadows: Query<(Entity, &GroundShadowPart, &Mesh3d, &ChildOf, &mut Visibility)>,
    models: Query<&GlobalTransform>,
    camera: Query<&GlobalTransform, With<FlyCamera>>,
    ground: Option<Res<GroundRes>>,
    mode: Res<CamMode>,
    drive: Res<DriveMode>,
    parents: Query<&ChildOf>,
    ai: Query<&race_mode::AiBody>,
) {
    let eye = camera.iter().next().map(|t| t.translation());
    for (e, part, mesh3d, parent, mut vis) in &mut shadows {
        let hide = drive.0 && mode.0 == 1 && opponent_of(e, &parents, &ai).is_none();
        vis.set_if_neq(if hide { Visibility::Hidden } else { Visibility::Inherited });
        let Ok(model) = models.get(parent.parent()) else { continue };
        let to_world = model.affine();
        let to_model = to_world.inverse();
        let origin = to_world.translation;
        let cells = eye.map_or(car_model::SHADOW_MAX_CELLS, |eye| car_model::shadow_cells(Vec3::from(origin).distance(eye)));
        let place = |p: [f32; 2]| {
            let world = to_world.transform_point3(Vec3::new(p[0], 0.0, p[1]));
            // the model's ground under this point, unless the road there is close to the car's ground
            let road = ground.as_ref().and_then(|g| g.0.height_at(world.x, world.z, origin.y, SHADOW_PROBE_UP)).filter(|&y| (y - world.y).abs() < SHADOW_MAX_STEP);
            let local = road.map_or(Vec3::new(p[0], 0.0, p[1]), |y| to_model.transform_point3(Vec3::new(world.x, y, world.z)));
            [local.x, local.y + SHADOW_LIFT, local.z]
        };
        if let Some(mut m) = meshes.get_mut(&mesh3d.0) {
            *m = ground_shadow_mesh(part.0, cells, place);
        }
    }
}

impl Look {
    fn alpha(self) -> f32 {
        if self == Look::Glass { 0.55 } else { 1.0 }
    }
}

/// The car's texture pages as images, and one material per (page, look), made on first use.
struct CarLooks {
    images: Vec<Option<(Handle<Image>, bool)>>,
    made: HashMap<(usize, Look), Handle<StandardMaterial>>,
    grey: Option<Handle<StandardMaterial>>,
}

impl CarLooks {
    fn new(m: &car_model::CarModel, images: &mut Assets<Image>) -> Self {
        let mut out: Vec<Option<(Handle<Image>, bool)>> = Vec::new();
        // the wheel shadow quads' page is only used by them; it is blended, see `screen_alpha`
        let shadow_pages: Vec<u8> = m.shadow.pages.clone();
        for (i, page) in m.pages.iter().enumerate() {
            // a page that shows another page's picture shares its image
            let shared = m.page_source.get(i).copied().flatten().and_then(|s| out.get(s).cloned().flatten());
            out.push(shared.or_else(|| {
                page.as_ref().map(|p| {
                    let mut image = page_image(p);
                    if shadow_pages.contains(&(i as u8)) {
                        if let Some(data) = image.data.as_mut() {
                            data.chunks_exact_mut(4).for_each(|px| px[3] = screen_alpha(px[3]));
                        }
                    }
                    (images.add(image), p.has_alpha)
                })
            }));
        }
        CarLooks { images: out, made: HashMap::new(), grey: None }
    }

    fn material(&mut self, page: u8, look: Look, materials: &mut Assets<StandardMaterial>) -> Handle<StandardMaterial> {
        let page = page as usize;
        let Some((image, has_alpha)) = self.images.get(page).cloned().flatten() else {
            // no picture for this page: flat grey
            return self.grey.get_or_insert_with(|| materials.add(StandardMaterial { base_color: Color::srgb(0.3, 0.3, 0.32), unlit: true, ..default() })).clone();
        };
        self.made
            .entry((page, look))
            .or_insert_with(|| {
                materials.add(StandardMaterial {
                    base_color: Color::WHITE,
                    base_color_texture: Some(image),
                    unlit: true,
                    // the file's front faces are clockwise; mirroring x (MIRROR_X) turns them anticlockwise
                    cull_mode: if look == Look::Shadow { None } else { Some(if MIRROR_X < 0.0 { Face::Back } else { Face::Front }) },
                    alpha_mode: match look {
                        Look::Glass | Look::Shadow => AlphaMode::Blend,
                        Look::Cut if has_alpha => AlphaMode::Mask(0.5),
                        _ => AlphaMode::Opaque,
                    },
                    ..default()
                })
            })
            .clone()
    }
}

/// Splits a piece into one piece per texture page.
fn by_page(piece: &Piece) -> Vec<(u8, Piece)> {
    let mut out: Vec<(u8, Piece)> = Vec::new();
    for t in 0..piece.positions.len() / 3 {
        let page = piece.pages.get(3 * t).copied().unwrap_or(0);
        let at = out.iter().position(|(p, _)| *p == page).unwrap_or_else(|| {
            out.push((page, Piece::default()));
            out.len() - 1
        });
        let part = &mut out[at].1;
        for k in 3 * t..3 * t + 3 {
            part.positions.push(piece.positions[k]);
            part.normals.push(piece.normals[k]);
            part.uvs.push(piece.uvs[k]);
            part.materials.push(piece.materials[k]);
            part.articles.push(piece.articles[k]);
            part.pages.push(page);
        }
    }
    out
}

/// Where the engine's car centre sits on the model (height above the ground) and half the model's
/// length, for the chase camera.
#[derive(Component, Clone, Copy)]
struct CarBounds {
    centre_height: f32,
    half_length: f32,
}

/// The driver's eye and the in-car camera's turn for the car model under this entity.
#[derive(Component, Clone, Copy)]
struct CockpitView {
    eye: Vec3,
    pitch: f32,
    yaw: f32,
}

#[derive(Resource)]
struct DriveMode(bool);

#[derive(Resource)]
struct GroundRes(Ground);

#[derive(Resource)]
struct WallRes(WallSet);

/// The player's car with its retail `.sim` physics. Which car: `NFSPU_SIM=<sim stem>` if set;
/// in a quick race the setup's catalogue car and its gearbox and aids (`flow::QuickSetup`);
/// otherwise the first road car using the shown model. Without an install: a made-up car.
fn player_car(pos: Vec3, yaw: f32, model: &str, quick: Option<&nfspu_game::race_setup::RaceSetup>) -> Car {
    use nfspu_game::car_load::{self, Pick};
    use nfspu_game::car_sim::Aids;
    let dir = PathBuf::from(env::var("NFSPU_GAME_DIR").unwrap_or_default());
    let setup = quick.filter(|s| s.car_model.eq_ignore_ascii_case(model));
    let aids = match &setup {
        Some(s) => Aids { automatic: s.automatic, abs: s.abs, traction_control: s.traction_control, keyboard_steering: true },
        None => Aids { automatic: env::var("NFSPU_MANUAL").is_err(), ..Aids::default() },
    };
    let sim = env::var("NFSPU_SIM").ok();
    let pick = match (&sim, &setup) {
        (Some(stem), _) => Pick::Sim { sim: stem, ai_name: "" },
        (None, Some(s)) => Pick::Id(s.car_id),
        (None, None) => Pick::Model(model),
    };
    match car_load::load(&dir, pick, !aids.automatic) {
        Ok(l) => {
            log_line(&format!("physics: {} ({}, ABS {}, TC {})", l.note, if aids.automatic { "automatic" } else { "manual" }, aids.abs, aids.traction_control));
            Car::with_spec(pos, yaw, l.spec, aids)
        }
        Err(e) => {
            log_line(&format!("physics: {e}; driving a made-up car"));
            Car::with_spec(pos, yaw, nfspu_game::car_sim::Spec::generic(), aids)
        }
    }
}

#[derive(Resource)]
struct CarRes {
    car: Car,
    start_pos: Vec3,
    start_yaw: f32,
    /// Last steering input (-1 right .. 1 left), used to turn the front wheels.
    steer: f32,
    /// True while the brake or handbrake is held (lights the brake lights).
    braking: bool,
}

/// A wheel of the car model; it spins with the speed and the front ones steer.
#[derive(Component)]
struct WheelComp {
    front: bool,
    spin: f32,
}

/// Wheel radius of the cars (about 0.31 m).
const WHEEL_RADIUS: f32 = 0.31;

/// Baked flat shading: base colour times a simple light, so the unlit car still looks solid.
fn piece_mesh(piece: &Piece, base: [f32; 3], alpha: f32) -> Mesh {
    let light = Vec3::new(0.35, 0.85, -0.4).normalize();
    let colors: Vec<[f32; 4]> = piece
        .normals
        .iter()
        .map(|n| {
            let s = 0.40 + 0.60 * Vec3::from(*n).dot(light).max(0.0);
            [base[0] * s, base[1] * s, base[2] * s, alpha]
        })
        .collect();
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, piece.positions.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, piece.uvs.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh
}

/// Turns a car texture page into an image (repeat sampling, smooth filtering).
fn page_image(page: &Page) -> Image {
    let mut image = Image::new(
        Extent3d { width: page.size, height: page.size, depth_or_array_layers: 1 },
        TextureDimension::D2,
        page.rgba.clone(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}

fn animate_wheels(time: Res<Time>, car: Option<Res<CarRes>>, mut wheels: Query<(Entity, &mut Transform, &mut WheelComp)>, parents: Query<&ChildOf>, ai: Query<&race_mode::AiBody>) {
    let Some(state) = car else { return };
    let dt = time.delta_secs().min(0.05);
    for (e, mut t, mut w) in &mut wheels {
        let (speed, steer) = opponent_of(e, &parents, &ai).map_or((state.car.speed, state.steer), |b| (b.speed, b.steer));
        w.spin -= speed * dt / WHEEL_RADIUS; // rolling forward turns the top toward the front
        let steer = if w.front { steer * 0.45 } else { 0.0 };
        t.rotation = Quat::from_rotation_y(steer) * Quat::from_rotation_x(w.spin);
    }
}

#[derive(Component)]
struct CarBody;

fn toggle_mode(keys: Res<ButtonInput<KeyCode>>, console: Res<console::Console>, mut mode: ResMut<DriveMode>, mut cams: Query<(&Transform, &mut FlyCamera)>) {
    if console.open {
        return;
    }
    if keys.just_pressed(KeyCode::KeyF) {
        mode.0 = !mode.0;
        log_line(if mode.0 { "mode: drive" } else { "mode: fly" });
        if !mode.0 {
            for (t, mut cam) in &mut cams {
                let (yaw, pitch, _) = t.rotation.to_euler(EulerRot::YXZ);
                cam.yaw = yaw;
                cam.pitch = pitch;
            }
        }
    }
}

/// Hands the car to the weather effects: where it is, whether its headlights are on, and the
/// ground under the headlight pool. On a night track the headlights start switched on.
fn feed_weather(
    car: Option<Res<CarRes>>,
    ground: Option<Res<GroundRes>>,
    weather: Option<Res<nfspu_game::weather_fx::WeatherState>>,
    mut lights: ResMut<LightsOn>,
    mut input: ResMut<nfspu_game::weather_fx::WeatherInput>,
    bounds: Query<&CarBounds>,
    holder: Option<Res<CarModelEntity>>,
) {
    let Some(car) = car else {
        input.car = None;
        return;
    };
    if weather.as_ref().is_some_and(|w| w.is_added() && w.night()) && !lights.0 {
        lights.0 = true;
        log_line("night track: headlights on");
    }
    let (pos, yaw) = (car.car.pos, car.car.yaw);
    input.car = Some((pos, yaw, car.car.speed));
    input.headlights = lights.0;
    input.braking = car.braking;
    input.ground_y = pos.y - RIDE_HEIGHT;
    input.half_length = holder.and_then(|h| bounds.get(h.model).ok()).map_or(2.2, |b| b.half_length);
    input.pool_heights.clear();
    if lights.0 && weather.as_ref().is_some_and(|w| w.night()) {
        if let Some(ground) = ground {
            let ground_y = pos.y - RIDE_HEIGHT;
            for (x, z) in nfspu_game::weather_fx::pool_points(pos, yaw) {
                input.pool_heights.push(ground.0.height_at(x, z, ground_y, 3.0));
            }
        }
    }
}

/// Gives the HUD the car's speed, rpm and gear and the race's numbers; outside a race (the viewer
/// with `NFSPU_HUD=1`) the race fields are PLACEHOLDERS (a 3-second countdown, lap 1/3, 1st of 6).
fn feed_hud(
    time: Res<Time>,
    car: Option<Res<CarRes>>,
    name: Res<CarName>,
    feed: Option<ResMut<nfspu_game::hud_plugin::HudFeed>>,
    race: Res<nfspu_game::race::RaceStatus>,
    mut start: Local<Option<f32>>,
    mut finished_at: Local<Option<f32>>,
) {
    let (Some(car), Some(mut feed)) = (car, feed) else { return };
    let now = time.elapsed_secs();
    let speed_kmh = car.car.speed * 3.6;
    let (rpm, gear) = (car.car.rpm(), car.car.gear());
    feed.car_model = name.0.clone();
    if race.active {
        // The race clock: GO is at tick 448 of the 64 Hz race clock (docs/provenance/race.md).
        let countdown = race.held.then(|| (448.0 - race.tick as f32).max(0.0) / 64.0);
        if race.finished && finished_at.is_none() {
            *finished_at = Some(now);
        } else if !race.finished {
            // a new race in the same program
            *finished_at = None;
        }
        feed.input = nfspu_game::hud::HudInput {
            speed_kmh,
            rpm,
            gear,
            lap: race.current_lap.max(1),
            total_laps: race.total_laps.max(1),
            position: race.position.max(1) as u32,
            car_count: race.cars.max(1) as u32,
            race_time: race.race_time,
            lap_time: race.lap_time,
            best_lap: race.best_lap,
            countdown,
            finished: finished_at.map(|t| (race.position.max(1) as u32, now - t)),
            racers: (0..race.cars)
                .map(|i| {
                    let place = race.car_places.get(i).copied().unwrap_or_default();
                    nfspu_game::hud::HudRacer {
                        name: race.names.get(i).cloned().unwrap_or_default(),
                        colour: race.car_colours.get(i).copied().unwrap_or([128; 3]),
                        place: [place[0], place[1], place[2]],
                        progress: race.car_progress.get(i).copied().unwrap_or(0.0),
                        speed: place[3],
                    }
                })
                .collect(),
            player: 0,
            order: race.order.clone(),
            road: race.road.clone(),
            lap_metres: race.lap_metres,
        };
        return;
    }
    // No race running: made-up race numbers so the HUD can be looked at in the viewer.
    let since = now - *start.get_or_insert(now);
    let race_time = (since - 3.0).max(0.0);
    feed.input = nfspu_game::hud::HudInput {
        speed_kmh,
        rpm,
        gear,
        lap: 1,
        total_laps: 3,
        position: 1,
        car_count: 6,
        race_time,
        lap_time: race_time,
        best_lap: None,
        countdown: (since < 3.0).then_some(3.0 - since),
        finished: None,
        ..Default::default()
    };
}

fn drive_car(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mode: Res<DriveMode>,
    ground: Option<Res<GroundRes>>,
    walls: Option<Res<WallRes>>,
    car: Option<ResMut<CarRes>>,
    console: Res<console::Console>,
    hold: Res<race_mode::RaceHold>,
    mut bodies: Query<&mut Transform, With<CarBody>>,
) {
    let (Some(ground), Some(walls), Some(mut state)) = (ground, walls, car) else { return };
    if console.open {
        return;
    }
    if keys.just_pressed(KeyCode::KeyR) || state.car.pos.y < state.start_pos.y - 300.0 {
        state.car = state.car.reset(state.start_pos, state.start_yaw);
        log_line("car reset to the start");
    }
    if mode.0 {
        let left = keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft);
        let right = keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight);
        let mut input = CarInput {
            throttle: keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp),
            brake: keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown),
            handbrake: keys.pressed(KeyCode::Space),
            steer: (left as i8 - right as i8) as f32,
            // manual gearbox: X up, Z down
            gear_up: keys.pressed(KeyCode::KeyX),
            gear_down: keys.pressed(KeyCode::KeyZ),
        };
        // on the grid until GO: the brake is held on and the automatic waits in neutral, but the
        // engine can be revved (retail)
        state.car.sim.held = hold.0;
        if hold.0 {
            input.steer = 0.0;
        }
        state.steer = input.steer;
        state.braking = (input.brake && state.car.speed > 0.5) || input.handbrake;
        state.car.step(input, time.delta_secs(), &ground.0, &walls.0);
    }
    for mut t in &mut bodies {
        t.translation = state.car.pos;
        t.rotation = Quat::from_euler(EulerRot::YXZ, state.car.yaw, state.car.pitch, state.car.roll);
    }
}

/// Camera while driving: 0 = chase (behind the car), 1 = cockpit (the driver's eyes), 2 = orbit
/// (circles the car; right mouse button or a captured mouse rotates, the wheel zooms). C cycles.
#[derive(Resource)]
struct CamMode(u8);

const CAMERA_NAMES: [&str; 3] = ["chase", "cockpit", "orbit"];

/// Angles (relative to the car's heading) and distance of the orbit camera.
#[derive(Resource)]
struct Orbit {
    yaw: f32,
    pitch: f32,
    radius: f32,
}

/// Marks the driver's meshes so they can be hidden in the cockpit view.
#[derive(Component)]
struct DriverPart;

/// Marks a cabin mesh: the detailed cockpit cabin (`cockpit: true`) or the simpler outside one.
#[derive(Component)]
struct CabinPart {
    cockpit: bool,
}

fn camera_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    captured: Res<MouseCaptured>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    drive: Res<DriveMode>,
    mut mode: ResMut<CamMode>,
    mut orbit: ResMut<Orbit>,
    console: Res<console::Console>,
) {
    if console.open {
        return;
    }
    if keys.just_pressed(KeyCode::KeyC) {
        mode.0 = (mode.0 + 1) % CAMERA_NAMES.len() as u8;
        log_line(&format!("camera: {}", CAMERA_NAMES[mode.0 as usize]));
    }
    if drive.0 && mode.0 == 2 {
        if captured.0 || mouse.pressed(MouseButton::Right) {
            orbit.yaw -= motion.delta.x * 0.005;
            orbit.pitch = (orbit.pitch + motion.delta.y * 0.005).clamp(-0.2, 1.4);
        }
        orbit.radius = (orbit.radius * (1.0 - scroll.delta.y * 0.1)).clamp(1.5, 20.0);
    }
}

fn driver_visibility(
    mode: Res<CamMode>,
    drive: Res<DriveMode>,
    mut parts: Query<(Entity, &mut Visibility), (With<DriverPart>, Without<CabinPart>)>,
    mut cabins: Query<(Entity, &mut Visibility, &CabinPart), Without<DriverPart>>,
    parents: Query<&ChildOf>,
    ai: Query<&race_mode::AiBody>,
) {
    // the cockpit view belongs to the player's car; opponents always show the outside cabin
    let in_cockpit = drive.0 && mode.0 == 1;
    for (e, mut v) in &mut parts {
        let hide = in_cockpit && opponent_of(e, &parents, &ai).is_none();
        v.set_if_neq(if hide { Visibility::Hidden } else { Visibility::Inherited });
    }
    for (e, mut v, cabin) in &mut cabins {
        let cockpit = in_cockpit && opponent_of(e, &parents, &ai).is_none();
        v.set_if_neq(if cabin.cockpit == cockpit { Visibility::Inherited } else { Visibility::Hidden });
    }
}

/// The chase camera's running state: the camera itself, frame time not yet run as a 64 Hz tick,
/// the camera mode last frame and the road path used for the slope lift.
#[derive(Default)]
struct ChaseState {
    cam: ChaseCam,
    pending: f32,
    last_mode: Option<u8>,
    path_track: Option<String>,
    path: Option<RoadPath>,
    slice: usize,
}

/// The view used for camera mode 0 (see `docs/provenance/car.md`, "Chase camera").
const CHASE_VIEW: ChaseView = chase_cam::MEDIUM;

#[allow(clippy::too_many_arguments)]
fn chase_camera(
    time: Res<Time>,
    mode: Res<DriveMode>,
    cam_mode: Res<CamMode>,
    orbit: Res<Orbit>,
    car: Option<Res<CarRes>>,
    track: Res<TrackName>,
    ground: Option<Res<GroundRes>>,
    cockpit: Query<&CockpitView>,
    bounds: Query<&CarBounds>,
    mut state: Local<ChaseState>,
    mut cams: Query<&mut Transform, With<FlyCamera>>,
) {
    let (true, Some(car_state)) = (mode.0, car) else {
        state.last_mode = None;
        return;
    };
    let c = &car_state.car;
    let rot = Quat::from_euler(EulerRot::YXZ, c.yaw, c.pitch, c.roll);
    let chase = cam_mode.0 == 0;
    if chase {
        let st = &mut *state;
        // the road path, for the slope ahead; loaded once per track
        // (a mirrored race of the same track has a mirrored path)
        let key = format!("{} {}", track.0, nfspu_game::world_mirror_x());
        if st.path_track.as_deref() != Some(key.as_str()) {
            st.path_track = Some(key);
            st.path = match race_path::load(&track.0, Direction::Forward) {
                Ok(t) => Some(t.path),
                Err(e) => {
                    log_line(&format!("chase camera: no road path for the slope ({e})"));
                    None
                }
            };
            st.slice = st.path.as_ref().map_or(0, |p| p.nearest(c.pos));
        }
        let b = bounds.iter().next().copied().unwrap_or(CarBounds { centre_height: 0.65, half_length: 2.2 });
        let heading = c.heading();
        let right = Vec3::new(-heading.z, 0.0, heading.x);
        let vel = Vec3::new(c.sim.vel[0], 0.0, c.sim.vel[1]);
        let slope = match &st.path {
            Some(p) if !p.slices.is_empty() => {
                st.slice = p.track(st.slice.min(p.slices.len() - 1), c.pos);
                // ahead in the way the car points, whichever way that is along the road
                let steps = if heading.dot(p.slices[st.slice].forward) >= 0.0 { chase_cam::SLOPE_SLICES } else { -chase_cam::SLOPE_SLICES };
                let ahead = p.walk(st.slice, steps, Direction::Forward);
                (p.slices[ahead].pos.y - p.slices[st.slice].pos.y) / chase_cam::SLOPE_SLICES as f32
            }
            _ => 0.0,
        };
        let frame = CarFrame {
            centre: c.pos + rot * Vec3::Y * (b.centre_height - RIDE_HEIGHT),
            rot,
            half_length: b.half_length,
            forward_speed: vel.dot(heading),
            side_speed: vel.dot(right),
            slope,
        };
        // a switch into the chase view (or a jump, e.g. a reset) starts it again from behind the car
        let jumped = cams.iter().next().is_some_and(|t| t.translation.distance(frame.centre) > 40.0);
        if st.last_mode != Some(0) || jumped {
            st.cam = ChaseCam::start(&frame);
            st.pending = 0.0;
        }
        st.pending += time.delta_secs();
        let tick = 1.0 / chase_cam::TICK_HZ;
        let mut ticks = 0;
        while st.pending >= tick && ticks < 16 {
            st.pending -= tick;
            st.cam.tick(&CHASE_VIEW, &frame);
            ticks += 1;
        }
        if ticks == 16 {
            st.pending = 0.0;
        }
        let free = st.cam.eye(&frame, None);
        let under = ground.as_ref().and_then(|g| g.0.height_at(free.x, free.z, free.y, 1.0));
        let eye = st.cam.eye(&frame, under);
        for mut t in &mut cams {
            t.translation = eye;
            t.look_at(ChaseCam::target(&frame), Vec3::Y);
        }
    }
    state.last_mode = Some(cam_mode.0);
    if chase {
        return;
    }
    for mut t in &mut cams {
        match cam_mode.0 {
            1 => {
                // the driver's eyes, from the car's .tpg (see `car_model::driver_eye`); the model's
                // origin is at ground level, RIDE_HEIGHT below the car's position. The camera is
                // turned by the .tpg's `drivercamrot` (pitch down, yaw).
                let view = cockpit.iter().next().copied().unwrap_or(CockpitView { eye: Vec3::new(-0.38, 1.05, 0.15), pitch: 0.0, yaw: 0.0 });
                t.translation = c.pos + rot * (view.eye - Vec3::Y * RIDE_HEIGHT);
                t.rotation = rot * Quat::from_euler(EulerRot::YXZ, -view.yaw, -view.pitch, 0.0);
            }
            _ => {
                let target = c.pos + Vec3::Y * 0.6;
                let r = Quat::from_euler(EulerRot::YXZ, c.yaw + orbit.yaw, -orbit.pitch, 0.0);
                t.translation = target + r * Vec3::new(0.0, 0.0, orbit.radius);
                t.look_at(target, Vec3::Y);
            }
        }
    }
}

/// Whether the reference build's field of view should be used for the driving cameras.
///
/// Off unless `NFSPU_RETAIL_FOV` is set to something other than 0/false/no, so a normal run is
/// unchanged. See `docs/provenance/refbuild.md` for where the numbers come from.
#[derive(Resource)]
struct RetailFov(bool);

impl RetailFov {
    fn from_env() -> Self {
        let on = env::var("NFSPU_RETAIL_FOV")
            .map(|v| !matches!(v.trim().to_ascii_lowercase().as_str(), "" | "0" | "false" | "no"))
            .unwrap_or(false);
        if on {
            log_line("retail fov: enabled by NFSPU_RETAIL_FOV");
        }
        RetailFov(on)
    }
}

/// Gives the chase and cockpit cameras the engine's field of view (`chase_cam::engine_vertical_fov`),
/// or with `NFSPU_RETAIL_FOV` set the patched build's, recomputed when the window size or camera
/// mode changes. The orbit and free-fly views keep Bevy's default.
fn retail_fov(
    enabled: Res<RetailFov>,
    drive: Res<DriveMode>,
    cam_mode: Res<CamMode>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut cams: Query<&mut Projection, With<FlyCamera>>,
    mut noted: Local<Option<(u32, u32)>>,
) {
    let Ok(window) = windows.single() else { return };
    let (w, h) = (window.physical_width(), window.physical_height());
    let view = refbuild::VirtualScreen::for_client(w, h);
    // only the driving cameras (chase 0, cockpit 1) take the engine's (or the reference build's) view
    let driving = drive.0 && cam_mode.0 < 2;
    let apply = enabled.0 && driving;
    let want = if apply {
        view.fov_degrees.to_radians()
    } else if driving {
        chase_cam::engine_vertical_fov(w.max(1) as f32 / h.max(1) as f32)
    } else {
        PerspectiveProjection::default().fov
    };
    for mut projection in &mut cams {
        if let Projection::Perspective(p) = &mut *projection {
            if (p.fov - want).abs() > 1e-4 {
                p.fov = want;
            }
        }
    }
    if apply && *noted != Some((w, h)) {
        *noted = Some((w, h));
        log_line(&format!(
            "retail fov: client {}x{} -> virtual {:.0}x{:.0}, {:.3} degrees (widescreen {})",
            w, h, view.width, view.height, view.fov_degrees, view.widescreen
        ));
    }
}

fn window_title(
    time: Res<Time>,
    track: Res<TrackName>,
    mode: Res<DriveMode>,
    cam_mode: Res<CamMode>,
    car: Option<Res<CarRes>>,
    name: Res<CarName>,
    style: Res<CarStyle>,
    holder: Option<Res<CarModelEntity>>,
    mut since: Local<f32>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    *since += time.delta_secs();
    if *since < 0.25 {
        return;
    }
    *since = 0.0;
    let Ok(mut w) = windows.single_mut() else { return };
    let styles = holder.map_or(0, |h| h.style_count);
    w.title = match (&car, mode.0) {
        (Some(c), true) => format!(
            "NFS PU (Rust) - {} - car {} style {}/{} (N/B car, V style) - {:.0} km/h - camera: {} (C)",
            track.0,
            name.0,
            style.0,
            styles,
            c.car.speed.abs() * 3.6,
            CAMERA_NAMES[cam_mode.0 as usize]
        ),
        (_, false) => format!("NFS Porsche Unleashed (Rust rewrite) - M3 - {} - FLY camera (F = back to the car)", track.0),
        _ => format!("NFS Porsche Unleashed (Rust rewrite) - M3 - {}", track.0),
    };
}
fn fly_camera(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    captured: Res<MouseCaptured>,
    mode: Res<DriveMode>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    mut speed: ResMut<FlySpeed>,
    mut cameras: Query<(&mut Transform, &mut FlyCamera)>,
) {
    if mode.0 {
        return;
    }
    speed.0 = (speed.0 * (1.0 + scroll.delta.y * 0.1)).clamp(5.0, 5000.0);
    for (mut transform, mut cam) in &mut cameras {
        if captured.0 || mouse.pressed(MouseButton::Right) {
            cam.yaw -= motion.delta.x * 0.003;
            cam.pitch = (cam.pitch - motion.delta.y * 0.003).clamp(-1.55, 1.55);
            transform.rotation = Quat::from_euler(EulerRot::YXZ, cam.yaw, cam.pitch, 0.0);
        }
        let mut dir = Vec3::ZERO;
        if keys.pressed(KeyCode::KeyW) { dir += *transform.forward(); }
        if keys.pressed(KeyCode::KeyS) { dir -= *transform.forward(); }
        if keys.pressed(KeyCode::KeyD) { dir += *transform.right(); }
        if keys.pressed(KeyCode::KeyA) { dir -= *transform.right(); }
        if keys.pressed(KeyCode::KeyE) { dir += Vec3::Y; }
        if keys.pressed(KeyCode::KeyQ) { dir -= Vec3::Y; }
        let boost = if keys.pressed(KeyCode::ShiftLeft) { 5.0 } else { 1.0 };
        transform.translation += dir.normalize_or_zero() * speed.0 * boost * time.delta_secs();
    }
}

fn quit_on_f10(keys: Res<ButtonInput<KeyCode>>, mut exit: MessageWriter<AppExit>) {
    if keys.just_pressed(KeyCode::F10) {
        log_line("F10 pressed, quitting");
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod horizon_geometry_tests {
    use super::*;
    #[test]
    fn atlas_mesh_fits_inside_dome_and_keeps_separate_uv_seams() {
        let layout = sky::HorizonLayout {
            zero_turn: 0.25, colour_start: 5, columns: 4, rows: 2,
            bottom: -100.0, height: 450.0, cells: [0; 16], flipped: [false; 16],
        };
        let mesh = horizon_band_mesh(&layout);
        assert_eq!(mesh.count_vertices(), 64);
        assert_eq!(mesh.indices().unwrap().len(), 96);
        let bevy::mesh::VertexAttributeValues::Float32x3(points) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() else { panic!("expected positions") };
        for p in points {
            let distance = Vec3::from_array(*p).length();
            assert!(distance < SKY_RADIUS * 0.96);
        }
        assert_eq!(points[1], points[4]);
        assert_eq!(points[61], points[0]);
    }
}

#[cfg(test)]
mod track_batch_tests {
    use super::*;
    #[test]
    fn trailing_corners_never_join_the_next_batch() {
        let positions: Vec<_> = (0..6).map(|i| [i as f32, 1.0, 2.0]).collect();
        let m = crp::ArticleMesh { positions, uvs: vec![[0.25, 0.75];6], batches: Vec::new(), skipped_batches: 0 };
        let batch = |indices: Vec<u16>| crp::Batch {
            material: 0, colors: vec![[255;4];indices.len()], uv_idx: indices.clone(), vertex_idx: indices,
        };
        let mut group = MaterialMesh { positions: vec![], uvs: vec![], colors: vec![], texture: None, one_sided: false };
        // Deliberately invalid leftover indices prove they are never read or joined.
        assert_eq!(append_track_batch(&mut group, &m, &batch(vec![0,1,2,999]), None), 1);
        assert_eq!(append_track_batch(&mut group, &m, &batch(vec![3,4,5,999,999]), None), 2);
        assert_eq!(group.positions.len(), 6);
        assert_eq!(group.uvs.len(), 6);
        assert_eq!(group.colors.len(), 6);
        for i in 0..6 { assert_eq!(group.positions[i], [i as f32 * MIRROR_X,1.0,2.0]); }
    }

    #[test]
    #[ignore = "reads the player's install; run explicitly with NFSPU_GAME_DIR"]
    fn real_tracks_have_complete_independent_triangles() {
        let dir = PathBuf::from(env::var("NFSPU_GAME_DIR").expect("set NFSPU_GAME_DIR"));
        let catalog = console::catalog(&dir);
        assert_eq!(catalog.tracks.len(),15);
        for name in catalog.tracks {
            let track = load_track(&name).unwrap();
            for group in &track.groups {
                assert_eq!(group.positions.len() % 3,0,"{name}");
                assert_eq!(group.positions.len(),group.uvs.len(),"{name}");
                assert_eq!(group.positions.len(),group.colors.len(),"{name}");
                assert!(group.positions.iter().flatten().all(|v| v.is_finite()),"{name}");
            }
            println!("{name}: PASS {} groups, {} complete triangles",track.groups.len(),track.triangles);
        }
    }

    /// Every point of the drivable road must give the car support; a point with none is a
    /// fall-through spot. Walks each road triangle's corners and centre.
    #[test]
    #[ignore = "reads the player's install; run explicitly with NFSPU_GAME_DIR"]
    fn real_tracks_have_no_fall_through_on_the_road() {
        let dir = PathBuf::from(env::var("NFSPU_GAME_DIR").expect("set NFSPU_GAME_DIR"));
        let catalog = console::catalog(&dir);
        assert_eq!(catalog.tracks.len(), 15);
        for name in catalog.tracks {
            let g = nfspu_game::track::load_ground(&name).unwrap();
            let ground = Ground::new(g.tris.clone(), 16.0);
            let raw = std::fs::read(crate::track::track_dir().unwrap().join(format!("{name}.crp"))).unwrap();
            let data = if refpack::is_refpack(&raw) { refpack::decompress(&raw).unwrap() } else { raw };
            let model = crp::parse(&data).unwrap();
            let (mut tested, mut missing) = (0usize, 0usize);
            let mut point_misses = 0usize;
            let (mut edge_points, mut edge_missing) = (0usize, 0usize);
            let mut worst: Option<(f32, f32, f32)> = None;
            for a in 0..model.articles.len() {
                let Some(an) = model.article_name(a) else { continue };
                if !an.starts_with("RD") {
                    continue;
                }
                let Some(m) = model.article_mesh(a, 0) else { continue };
                for b in &m.batches {
                    for t in 0..b.vertex_idx.len() / 3 {
                        let tri: [[f32; 3]; 3] = [0, 1, 2].map(|q| {
                            let v = m.positions[b.vertex_idx[3 * t + q] as usize];
                            [v[0] * nfspu_game::world_mirror_x(), v[1], v[2]]
                        });
                        if crate::track::classify(&[tri[0], tri[2], tri[1]]) != crate::track::Face::Ground {
                            continue; // not a drivable face (the file's winding is mirrored back)
                        }
                        let centre = [
                            (tri[0][0] + tri[1][0] + tri[2][0]) / 3.0,
                            (tri[0][1] + tri[1][1] + tri[2][1]) / 3.0,
                            (tri[0][2] + tri[1][2] + tri[2][2]) / 3.0,
                        ];
                        for p in tri.iter().chain(std::iter::once(&centre)) {
                            tested += 1;
                            if ground.surface_near(p[0], p[2], 1.5, p[1] - 1.5, p[1] + 1.0).is_none() {
                                missing += 1;
                                if worst.is_none() {
                                    worst = Some((p[0], p[1], p[2]));
                                }
                            }
                            // the old single-point query, kept as a measure of the gaps it fell into
                            if ground.height_at(p[0], p[2], p[1], 1.5).is_none() {
                                point_misses += 1;
                            }
                        }
                        // just off the tarmac, where the road meets a wall: the car's centre can be
                        // pushed here, so it must still find support within the probe tiers
                        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                            let (a, b) = (tri[i], tri[j]);
                            let (ex, ez) = (b[0] - a[0], b[2] - a[2]);
                            let len = (ex * ex + ez * ez).sqrt();
                            if len < 1e-3 {
                                continue;
                            }
                            // outward is away from the triangle's centre
                            let (mx, mz) = ((a[0] + b[0]) / 2.0, (a[2] + b[2]) / 2.0);
                            let (mut nx, mut nz) = (ez / len, -ex / len);
                            if nx * (mx - centre[0]) + nz * (mz - centre[2]) < 0.0 {
                                nx = -nx;
                                nz = -nz;
                            }
                            for dist in [0.5f32, 1.5, 2.5, 4.0, 5.5] {
                                let (x, z) = (mx + nx * dist, mz + nz * dist);
                                let y = (a[1] + b[1]) / 2.0;
                                edge_points += 1;
                                let tiers = [(1.5f32, 1.5f32), (3.0, 2.0), (6.0, 5.0)];
                                if !tiers.iter().any(|(r, d)| ground.surface_near(x, z, *r, y - d, y + 1.0).is_some()) {
                                    edge_missing += 1;
                                }
                            }
                        }
                    }
                }
            }
            println!("{name}: {tested} road points, {missing} without support, {edge_missing}/{edge_points} just-off-road points without support ({point_misses} a single-point query would have missed)");
            assert_eq!(missing, 0, "{name} has road points the car would fall through: {worst:?}");
            assert_eq!(edge_missing, 0, "{name} has road-edge points the car would fall through");
        }
    }
}
