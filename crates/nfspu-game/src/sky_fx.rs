//! The retail sky and its weather effects, drawn the way `Porsche.exe` draws them (evidence and
//! addresses in `docs/provenance/weather.md`):
//! * **Sky layers** (exe 0x45B5A0, 0x45E8E0): a 16x16-point dome from the `.bin` header; per layer
//!   a tiling texture (`flat`, `cc1a`, `cm1a`) that scrolls with the wind and the camera, times
//!   per-point colours from a 16x16 sky map picture (`st1a`, `sc1a`) - one pixel per point -
//!   blended between weather region A and B.
//! * **Sun and lens flare** (0x460290, 0x460460): the `suna` glow from `Render/flare.dat` at the
//!   sun's point on the dome, drawn between the sky levels that do and do not "block the sun"; then
//!   over the finished picture, the `sunb`..`suni` ghosts along the line from the sun through the
//!   screen centre at `Render/flarepos.dat` fractions, as bright as the sun is unhidden.
//! * **Lightning bolts** (0x45CD90, 0x45E3D0): a strike brightens the sky-map colours around a
//!   random point of the dome, capped by the track's 7x7 `ltmp` picture.
//! * **Track glows** (0x450D10, 0x451940): lamps and signs from the track's `.crp`, drawn with
//!   their `flare.dat` records, blinking on retail's clock.
//! * **Drops on the rear window** (0x458C90, 0x4577A0): `SPLT` drops that grow and fade on a
//!   sloping quad behind the car's roof while it rains.

use crate::sky::{SkyDome, SkyLevel, TrackSky, SKY_GRID};
use crate::weather_fx::{SkyPart, WeatherInput, WeatherState, WorldCamera};
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::camera::ScalingMode;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::picking::mesh_picking::ray_cast::{MeshRayCast, MeshRayCastSettings, RayCastBackfaces, RayCastVisibility};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::path::PathBuf;

/// Render layer of the overlay camera that draws the lens flare over the finished picture.
pub const FLARE_LAYER: usize = 2;
/// Points along each side of the sky grid.
const N: usize = SKY_GRID + 1;
/// Sort spacing for the sky pieces. Nothing in the sky pass writes depth, so the draw order is the
/// sort order, and `depth_bias` (added to the sort distance) sets it: rank x this, far beyond any
/// real distance. Ranks: non-blocking layers 0-2, sun 3, blocking layers 4-6, horizon 8.
const RANK: f32 = 100_000.0;
/// `depth_bias` for the horizon band: drawn last in the sky pass, as retail draws it last.
pub const HORIZON_DEPTH_BIAS: f32 = 8.0 * RANK;
/// Retail's simulation tick rate.
const TICK_HZ: f32 = 64.0;

/// Marks the flare overlay camera.
#[derive(Component)]
pub struct FlareCamera;
/// A lens-flare sprite (overlay camera).
#[derive(Component)]
pub struct FlareSprite {
    /// Index into `SkyFx::flares`, and which half (0 = first texture, 1 = second) of that record.
    record: usize,
    half: usize,
}
/// One of the sun's two sprites, in the sky pass.
#[derive(Component)]
pub struct SunSprite(usize);
/// A sky layer mesh: centred on the camera every frame.
#[derive(Component)]
pub struct SkyLayerMesh;
/// The rear-window drop mesh.
#[derive(Component)]
pub struct DropSheet;

/// A 16x16 sky map: RGBA bytes, one per sky point.
type Map = Vec<[u8; 4]>;

/// One `flare.dat` record (140 bytes): two sprites, each a texture, a colour and a size.
#[derive(Clone, Debug)]
pub struct FlareRecord {
    /// The record's name, e.g. `suna`.
    pub name: String,
    /// The two sprite textures (`common.fsh` names such as `GLAR`, `SUNA`), `null` for none.
    pub textures: [String; 2],
    /// Colours, 0xAARRGGBB.
    pub colours: [u32; 2],
    /// Sprite sizes: the half-size is size / 10 in a 640x480 screen.
    pub sizes: [f32; 2],
    /// Bit 4: turn the sprite with its screen position.
    pub flags: u32,
    /// Blinking: phase, ticks on, ticks off (0, 0 = always on).
    pub blink: [i32; 3],
}

/// Reads `Render/flare.dat`. Names are stored as little-endian dwords that read as the name
/// backwards (`lloa` is `aoll`).
pub fn parse_flare_dat(bytes: &[u8]) -> Vec<FlareRecord> {
    let name = |b: &[u8]| -> String { b.iter().rev().map(|&c| c as char).collect::<String>().trim_matches('\0').to_string() };
    let u = |r: &[u8], o: usize| u32::from_le_bytes([r[o], r[o + 1], r[o + 2], r[o + 3]]);
    bytes
        .chunks_exact(140)
        .map(|r| FlareRecord {
            name: name(&r[0..4]),
            textures: [name(&r[4..8]), name(&r[8..12])],
            colours: [u(r, 0x1C), u(r, 0x20)],
            sizes: [f32::from_bits(u(r, 0x34)), f32::from_bits(u(r, 0x38))],
            flags: u(r, 0x4C),
            blink: [u(r, 0x50) as i32, u(r, 0x54) as i32, u(r, 0x58) as i32],
        })
        .collect()
}

/// Reads `Render/flarepos.dat`: where each flare sprite sits on the line from the screen centre
/// (0) to the sun (1). Retail's built-in fallback is used when the file is missing (0x460780).
pub fn parse_flare_pos(bytes: Option<&[u8]>) -> Vec<f32> {
    match bytes {
        Some(b) if b.len() >= 36 => b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect(),
        _ => vec![1.0, 0.75, -0.75, 0.5, -0.5, 0.25, -0.25, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    }
}

/// One sky layer.
struct Layer {
    mesh: Handle<Mesh>,
    /// Sky maps of region A and B (None: plain white).
    maps: [Option<Map>; 2],
    /// Per region, 1 / scale across and down (texture repeats per grid cell).
    inv_scale: [[f32; 2]; 2],
    /// Per region, wind drift in texture repeats per tick.
    velocity: [[f32; 2]; 2],
    /// Whether the wind moves this layer (some region's wind speed > 0.25).
    wind: bool,
    /// Wind drift so far, kept in 0..1.
    offset: [f32; 2],
    flash: Option<Flash>,
}

/// One lightning strike on one layer: up to `sparks.len()` bright points round the strike.
struct Flash {
    /// Ticks left in which new sparks may start.
    budget: i32,
    /// New sparks per tick at most.
    per_tick: usize,
    sparks: Vec<Spark>,
}

#[derive(Clone, Copy)]
struct Spark {
    /// Peak brightness, current brightness (negative: not lit) and change per tick.
    max: i32,
    cur: i32,
    step: i32,
    /// Sky point it brightens, and the colour it may not exceed.
    point: usize,
    cap: [u8; 3],
}

/// Lightning patterns (exe 0x5CF018): budget ticks, sparks, rise factor, peak, sparks per tick.
const PATTERNS: [(i32, usize, f32, i32, usize); 5] =
    [(90, 35, 0.82, 255, 6), (90, 8, 0.68, 255, 5), (90, 80, 0.30, 255, 5), (90, 8, 0.26, 255, 5), (90, 33, 0.11, 255, 18)];
/// Which patterns each strength may use (0x5CF07C).
const PATTERN_CHOICE: [[usize; 3]; 3] = [[0, 0, 1], [1, 1, 2], [2, 3, 4]];
/// Spark offsets from the strike point, per strength (0x5CF144); picked separately for x and y.
const SPARK_OFFSETS: [[i8; 32]; 3] = [
    [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 2, 2, 0, 0, 0, 0, 0, 0, 0, 0, -1, -1, -1, -1, -1, -1, -2, -2],
    [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -1, -1, -1, -1, -1, 0, 0, 0],
    [0; 32],
];
/// Strike distance from the dome centre, as a share of half the dome: base + random x range
/// (0x5CF0C8, 0x5CF0D4).
const STRIKE_BASE: [f32; 3] = [0.4, 0.3, 0.2];
const STRIKE_RANGE: [f32; 3] = [0.2, 0.1, 0.1];

/// Rear-window drops (0x458C90): the fallback quad in the engine's car frame (+z forward),
/// corners at x +-1, the top edge 1.4 up and 0.55 back, the bottom edge 0.8 up and 0.95 back.
const DROP_HALF_WIDTH: f32 = 1.0;
const DROP_TOP: [f32; 2] = [1.4, -0.55];
const DROP_BOTTOM: [f32; 2] = [0.8, -0.95];
/// Drops at full rain (0x5B248C), times min(1, glass area x 0.769).
const DROP_MAX: f32 = 150.0;
/// Age stages (0x5CEE40): from this age (ticks), grow towards (length, width) x 0.5 at `rate`,
/// and fade towards `alpha` by `step` per tick. The fourth stage fades the drop out.
const DROP_STAGES: [(i32, f32, f32, f32, i32, i32); 4] =
    [(0, 0.05, 0.05, 0.2, 255, 1), (20, 0.025, 0.025, 0.2, 255, 1), (40, 0.025, 0.025, 0.03, 255, 1), (60, 0.025, 0.025, 0.03, 0, 1)];

#[derive(Clone, Copy, Default)]
struct WindowDrop {
    /// Position on the quad: across (-1..1 x half width) and down the slope (0..length).
    across: f32,
    down: f32,
    /// Direction the drop points in, in quad coordinates (across, down), eased to `target`.
    dir: Vec2,
    target: Vec2,
    length: f32,
    width: f32,
    alpha: i32,
    age: i32,
    alive: bool,
}

/// Everything this module draws for the loaded track.
#[derive(Resource)]
pub struct SkyFx {
    dome: SkyDome,
    level: SkyLevel,
    /// World units per retail sky unit, and the track's x mirror.
    scale: f32,
    mirror: f32,
    layers: Vec<Layer>,
    /// 1 / move ratio per region (0 = layers do not slide with the camera).
    inv_move: [f32; 2],
    /// The strike point on the grid while a bolt is lit (column, row), and the bolt's serial.
    strike: Option<(usize, usize)>,
    strike_serial: u32,
    ltmp: Option<Vec<[u8; 4]>>,
    /// The sun's point on the dome (retail axes, unscaled), if the track has one.
    sun: Option<Vec3>,
    flares: Vec<FlareRecord>,
    flare_pos: Vec<f32>,
    /// How much of the sun is unhidden (0..1), from the last 200 ms test.
    sun_visible: f32,
    logged_visible: f32,
    next_sun_test: f32,
    drops: Vec<WindowDrop>,
    drop_mesh: Option<Handle<Mesh>>,
    drop_debt: f32,
    tick_debt: f32,
    rng: u32,
    next_log: f32,
}

impl SkyFx {
    fn random(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / (1u32 << 24) as f32
    }
}

pub struct SkyFxPlugin;

impl Plugin for SkyFxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (add_flare_camera, tick_sky, update_drops, update_glows).chain())
            .add_systems(PostUpdate, (follow_camera, update_sky_meshes, sun_blockers, place_sun_and_flare).chain().before(bevy::transform::TransformSystems::Propagate));
    }
}

fn log(msg: &str) {
    use std::io::Write as _;
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("logs").join("m0.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "[sky] {msg}");
    }
}

/// An image with a repeating, smooth sampler.
fn picture(px: &[u8], w: usize, h: usize, repeat: bool) -> Image {
    let mut image = Image::new(
        Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        px.to_vec(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    let mode = if repeat { ImageAddressMode::Repeat } else { ImageAddressMode::ClampToEdge };
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: mode,
        address_mode_v: mode,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}

/// A picture from `Render/common.fsh` by name.
fn common_picture(name: &str) -> Option<(Vec<u8>, usize, usize)> {
    let game = PathBuf::from(std::env::var("NFSPU_GAME_DIR").ok()?);
    let raw = std::fs::read(game.join("GameData").join("Render").join("common.fsh")).ok()?;
    let data = if nfspu_formats::refpack::is_refpack(&raw) { nfspu_formats::refpack::decompress(&raw).ok()? } else { raw };
    let archive = nfspu_formats::shpi::parse(&data).ok()?;
    let e = archive.entries.iter().find(|e| e.name.trim() == name)?;
    Some((nfspu_formats::shpi::decode_rgba8(&data, e)?, e.width as usize, e.height as usize))
}

fn read_render(file: &str) -> Option<Vec<u8>> {
    let game = PathBuf::from(std::env::var("NFSPU_GAME_DIR").ok()?);
    std::fs::read(game.join("GameData").join("Render").join(file)).ok()
}

/// A 16x16 sky map from the track's pictures.
fn sky_map(sky: &TrackSky, name: &str) -> Option<Map> {
    if name == "none" || name.is_empty() {
        return None;
    }
    let (px, w, h) = sky.pictures.find(name)?;
    if w != N || h != N {
        log(&format!("sky map '{name}' is {w}x{h}, not 16x16; not used"));
        return None;
    }
    Some(px.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect())
}

/// The dome grid as a mesh: positions fixed, UVs and colours rewritten each frame.
fn layer_mesh(points: &[[f32; 3]]) -> Mesh {
    let mut indices = Vec::with_capacity(SKY_GRID * SKY_GRID * 6);
    for r in 0..SKY_GRID {
        for c in 0..SKY_GRID {
            let i = (r * N + c) as u32;
            let n = N as u32;
            indices.extend([i, i + 1, i + n + 1, i, i + n + 1, i + n]);
        }
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, points.to_vec());
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; points.len()]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0f32; 4]; points.len()]);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Spawns the track's sky layers and sun, and inserts `SkyFx`. `scale` is world units per retail
/// sky unit (main.rs uses its horizon band's radius over retail's 400). Returns the entities, or
/// an empty list when the track has no sky level or dome (main.rs then keeps its plain dome).
pub fn spawn(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, images: &mut Assets<Image>, sky: &TrackSky, scale: f32) -> Vec<Entity> {
    let mut out = Vec::new();
    let (Some(level), Some(dome)) = (sky.sky_level.clone(), sky.dome) else {
        log("no sky level or dome header; retail layers not drawn");
        return out;
    };
    let mirror = crate::world_mirror_x();
    let points: Vec<[f32; 3]> = dome.grid().into_iter().map(|p| [p[0] * mirror * scale, p[1] * scale, p[2] * scale]).collect();
    let white = images.add(picture(&[255, 255, 255, 255], 1, 1, true));
    let mut layers = Vec::new();
    for i in 0..3 {
        let tex = level.textures[i].as_str();
        if tex == "none" || tex.is_empty() {
            continue;
        }
        let (texture, tex_opaque) = if tex == "flat" {
            (white.clone(), true)
        } else if let Some((px, w, h)) = sky.pictures.find(tex) {
            (images.add(picture(px, w, h, true)), px.chunks_exact(4).all(|c| c[3] == 255))
        } else {
            log(&format!("layer {i}: texture '{tex}' not in the track's sky pictures; layer skipped"));
            continue;
        };
        let map_a = sky_map(sky, &level.regions[0].maps[i]);
        let map_b = sky_map(sky, &level.regions[1].maps[i]).or_else(|| map_a.clone());
        let opaque = tex_opaque && [&map_a, &map_b].iter().all(|m| m.as_ref().is_none_or(|m| m.iter().all(|p| p[3] == 255)));
        let region = |r: usize| -> ([f32; 2], [f32; 2]) {
            let g = &level.regions[r];
            let inv = |s: f32| if s.abs() > f32::EPSILON { 1.0 / s } else { 0.0 };
            // Direction: degrees clockwise from north, turned to texture drift (0x45F1B0).
            let turn = -g.wind_dir[i] / 360.0 * std::f32::consts::TAU;
            ([inv(g.scale_x[i]), inv(g.scale_y[i])], [turn.sin() * g.wind_speed[i] * 0.00025, turn.cos() * g.wind_speed[i] * 0.00025])
        };
        let (sa, va) = region(0);
        let (sb, vb) = region(1);
        let wind = level.regions.iter().any(|g| g.wind_speed[i] > 0.25);
        let rank = if level.blocks_sun { 4.0 } else { 0.0 } + layers.len() as f32;
        let mesh = meshes.add(layer_mesh(&points));
        let material = materials.add(StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: Some(texture),
            unlit: true,
            cull_mode: None,
            // Every layer blends, so none writes depth and the ranks alone order the sky pass.
            alpha_mode: AlphaMode::Blend,
            depth_bias: rank * RANK,
            ..default()
        });
        out.push(commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material), Transform::default(), NoFrustumCulling, SkyPart, SkyLayerMesh)).id());
        log(&format!(
            "layer {}: texture '{tex}', maps '{}'/'{}', scale {:?}/{:?}, wind {:?}/{:?} {}, {}",
            layers.len(),
            level.regions[0].maps[i],
            level.regions[1].maps[i],
            [level.regions[0].scale_x[i], level.regions[0].scale_y[i]],
            [level.regions[1].scale_x[i], level.regions[1].scale_y[i]],
            [level.regions[0].wind_speed[i], level.regions[0].wind_dir[i]],
            [level.regions[1].wind_speed[i], level.regions[1].wind_dir[i]],
            if wind { "drifts" } else { "still" },
            if opaque { "opaque" } else { "see-through" }
        ));
        layers.push(Layer { mesh, maps: [map_a, map_b], inv_scale: [sa, sb], velocity: [va, vb], wind, offset: [0.0; 2], flash: None });
    }
    let inv_move = [0, 1].map(|r| {
        let m = level.regions[r].move_ratio;
        if m != 0.0 { 1.0 / m } else { 0.0 }
    });
    if level.regions.iter().any(|g| g.warp_speed > 0.01) {
        log("this level asks for texture warping, which is not drawn");
    }

    // The sun: `suna` from flare.dat, both sprites, in the sky pass between the levels.
    let flares = read_render("flare.dat").map(|b| parse_flare_dat(&b)).unwrap_or_default();
    let flare_pos = parse_flare_pos(read_render("flarepos.dat").as_deref());
    let sun = dome.sun().map(Vec3::from_array);
    if let (Some(_), Some(rec)) = (sun, flares.iter().find(|r| r.name == "suna")) {
        for half in 0..2 {
            let Some(material) = sprite_material(materials, images, rec, half, 3.0 * RANK) else { continue };
            out.push(commands.spawn((Mesh3d(meshes.add(Rectangle::new(1.0, 1.0))), MeshMaterial3d(material), Transform::default(), Visibility::Hidden, NoFrustumCulling, SkyPart, SunSprite(half))).id());
        }
    }
    // The flare ghosts: sunb.. along the flare line, on the overlay camera.
    if sun.is_some() && dome.flare {
        for k in 0..13usize {
            let name = format!("sun{}", (b'b' + k as u8) as char);
            let Some(idx) = flares.iter().position(|r| r.name == name) else { continue };
            for half in 0..2 {
                let Some(material) = sprite_material(materials, images, &flares[idx], half, 0.0) else { continue };
                out.push(
                    commands
                        .spawn((Mesh3d(meshes.add(Rectangle::new(1.0, 1.0))), MeshMaterial3d(material), Transform::default(), Visibility::Hidden, NoFrustumCulling, RenderLayers::layer(FLARE_LAYER), FlareSprite { record: idx, half }))
                        .id(),
                );
            }
        }
    }
    log(&format!(
        "level '{}' ({}), {} layers, blocks sun {}, move ratio {:?}; dome size {} curve {} height {} scale {} turn {:.3}; sun {:?}, flare {}; {} flare records, flare line {:?}",
        level.level,
        level.capability,
        layers.len(),
        level.blocks_sun,
        [level.regions[0].move_ratio, level.regions[1].move_ratio],
        dome.size,
        dome.curve,
        dome.height,
        dome.scale,
        dome.angle,
        sun.map(|s| [s.x, s.y, s.z]),
        dome.flare,
        flares.len(),
        flare_pos
    ));
    let ltmp = sky.pictures.find("ltmp").filter(|(_, w, h)| *w == 7 && *h == 7).map(|(px, _, _)| px.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect());
    commands.insert_resource(SkyFx {
        dome,
        level,
        scale,
        mirror,
        layers,
        inv_move,
        strike: None,
        strike_serial: 0,
        ltmp,
        sun,
        flares,
        flare_pos,
        sun_visible: 0.0,
        logged_visible: -1.0,
        next_sun_test: 0.0,
        drops: Vec::new(),
        drop_mesh: None,
        drop_debt: 0.0,
        tick_debt: 0.0,
        rng: 0x9E37_79B9,
        next_log: 0.0,
    });
    out
}

/// The material for one half of a flare record, or None if that half has no picture.
fn sprite_material(materials: &mut Assets<StandardMaterial>, images: &mut Assets<Image>, rec: &FlareRecord, half: usize, bias: f32) -> Option<Handle<StandardMaterial>> {
    let tex = &rec.textures[half];
    if tex == "null" || tex.is_empty() || rec.sizes[half] <= 0.0 {
        return None;
    }
    let Some((px, w, h)) = common_picture(tex) else {
        log(&format!("flare '{}': picture '{tex}' is not in common.fsh; not drawn (retail skips it too)", rec.name));
        return None;
    };
    let c = rec.colours[half];
    Some(materials.add(StandardMaterial {
        base_color: Color::srgba_u8((c >> 16) as u8, (c >> 8) as u8, c as u8, (c >> 24) as u8),
        base_color_texture: Some(images.add(picture(&px, w, h, false))),
        unlit: true,
        cull_mode: None,
        fog_enabled: false,
        // Additive, as glows are; retail's blend state for these sprites was not traced.
        alpha_mode: AlphaMode::Add,
        depth_bias: bias,
        ..default()
    }))
}

/// The overlay camera for the lens flare: a 640x480 screen over the finished picture.
fn add_flare_camera(mut commands: Commands, cams: Query<(), Added<WorldCamera>>, old: Query<Entity, With<FlareCamera>>) {
    if cams.is_empty() {
        return;
    }
    for o in &old {
        commands.entity(o).despawn();
    }
    commands.spawn((
        Camera3d::default(),
        Camera { order: 1, clear_color: ClearColorConfig::None, ..default() },
        Projection::Orthographic(OrthographicProjection { scaling_mode: ScalingMode::Fixed { width: 640.0, height: 480.0 }, near: -100.0, far: 100.0, ..OrthographicProjection::default_3d() }),
        // The picture underneath is already tone-mapped by the world camera.
        Tonemapping::None,
        Transform::IDENTITY,
        RenderLayers::layer(FLARE_LAYER),
        FlareCamera,
    ));
}

/// The sky is drawn round the camera (retail draws it with the view's translation removed).
fn follow_camera(cams: Query<&Transform, (With<WorldCamera>, Without<SkyLayerMesh>)>, mut layers: Query<&mut Transform, With<SkyLayerMesh>>) {
    let Ok(cam) = cams.single() else { return };
    for mut t in &mut layers {
        t.translation = cam.translation;
    }
}

/// Retail's 64 Hz work: wind drift, lightning sparks.
fn tick_sky(time: Res<Time>, fx: Option<ResMut<SkyFx>>, weather: Option<Res<WeatherState>>) {
    let Some(mut fx) = fx else { return };
    let blend = weather.as_ref().map_or(0.0, |w| w.region_blend.clamp(0.0, 1.0));
    // A new bolt: place the strike (0x45CD90) and light every layer that has a sky map.
    if let Some(w) = weather.as_ref() {
        if w.bolt != fx.strike_serial {
            fx.strike_serial = w.bolt;
            let s = w.bolt_strength.min(2);
            let half = fx.dome.size * 0.5;
            let r = (fx.random() * STRIKE_RANGE[s] + STRIKE_BASE[s]) * half;
            let a = fx.random() * std::f32::consts::TAU;
            let col = ((half - a.sin() * r) / fx.dome.size * SKY_GRID as f32).clamp(0.0, SKY_GRID as f32) as usize;
            let row = ((a.cos() * r + half) / fx.dome.size * SKY_GRID as f32).clamp(0.0, SKY_GRID as f32) as usize;
            fx.strike = Some((col, row));
            log(&format!("lightning bolt {} strength {s} at sky point column {col} row {row}", w.bolt));
        }
        if !w.bolt_active {
            fx.strike = None;
        }
    }
    fx.tick_debt += time.delta_secs().min(0.25) * TICK_HZ;
    while fx.tick_debt >= 1.0 {
        fx.tick_debt -= 1.0;
        let strength = weather.as_ref().map_or(0, |w| w.bolt_strength.min(2));
        let strike = fx.strike;
        let mut layers = std::mem::take(&mut fx.layers);
        for layer in &mut layers {
            if layer.wind {
                for k in 0..2 {
                    let v = layer.velocity[0][k] + (layer.velocity[1][k] - layer.velocity[0][k]) * blend;
                    layer.offset[k] = (layer.offset[k] + v).rem_euclid(1.0);
                }
            }
            // Start a flash on a layer with a sky map while the bolt is lit (0x45DF50).
            if layer.flash.is_none() && layer.maps[0].is_some() && strike.is_some() {
                let pick = PATTERN_CHOICE[strength][((fx.random() * 3.0) as usize).min(2)];
                let (budget, count, rise, peak, per_tick) = PATTERNS[pick];
                let sparks = (0..count)
                    .map(|_| {
                        let max = (fx.random() * peak as f32).round() as i32;
                        let d = (fx.random() * budget as f32 * rise).max(2.0);
                        Spark { max, cur: -1, step: ((2.0 * max as f32 / d).round() as i32).max(1), point: 0, cap: [255; 3] }
                    })
                    .collect();
                layer.flash = Some(Flash { budget, per_tick, sparks });
            }
            let Some(flash) = layer.flash.as_mut() else { continue };
            // One tick of the flash (0x45E3D0 / 0x45E550).
            flash.budget -= 1;
            let (mut started, mut idle) = (0, 0);
            let total = flash.sparks.len();
            for i in 0..total {
                let mut s = flash.sparks[i];
                if s.cur < 0 {
                    if let (true, Some((col, row))) = (started < flash.per_tick && flash.budget >= 1, strike) {
                        started += 1;
                        let ox = SPARK_OFFSETS[strength][(fx.random() * 32.0) as usize & 31];
                        let oy = SPARK_OFFSETS[strength][(fx.random() * 32.0) as usize & 31];
                        let x = (col as i32 + ox as i32).clamp(0, SKY_GRID as i32) as usize;
                        let y = (row as i32 + oy as i32).clamp(0, SKY_GRID as i32) as usize;
                        s.point = y * N + x;
                        // The cap is the template pixel at the offset (0x45CC20 indexes it x * 7 + y).
                        s.cap = fx.ltmp.as_ref().map_or([255; 3], |t| {
                            let p = t[((ox as i32 + 3) * 7 + (oy as i32 + 3)) as usize];
                            [p[0], p[1], p[2]]
                        });
                        s.cur = 0;
                        s.step = s.step.abs();
                    } else {
                        idle += 1;
                    }
                } else {
                    if s.cur >= s.max && s.step > 0 {
                        s.step = -s.step;
                    }
                    s.cur += s.step;
                }
                flash.sparks[i] = s;
            }
            if idle >= total {
                layer.flash = None;
            }
        }
        fx.layers = layers;
    }
}

/// Rewrites each layer's colours (region blend plus sparks) and texture coordinates (wind plus
/// camera slide), the work retail's scrollers and sky-map layers do each frame.
fn update_sky_meshes(fx: Option<Res<SkyFx>>, weather: Option<Res<WeatherState>>, cams: Query<&Transform, With<WorldCamera>>, mut meshes: ResMut<Assets<Mesh>>) {
    let (Some(fx), Ok(cam)) = (fx, cams.single()) else { return };
    let t = weather.as_ref().map_or(0.0, |w| w.region_blend.clamp(0.0, 1.0));
    let inv_move = fx.inv_move[0] + (fx.inv_move[1] - fx.inv_move[0]) * t;
    // Retail axes for the camera slide: our x is mirrored.
    let (cx, cz) = (cam.translation.x * fx.mirror, cam.translation.z);
    let slide = fx.level.regions.iter().any(|g| g.move_ratio > 1.0);
    let ta = (t * 65536.0).round() as u32;
    let tb = 65536 - ta;
    for layer in &fx.layers {
        let inv = [0, 1].map(|k| layer.inv_scale[0][k] + (layer.inv_scale[1][k] - layer.inv_scale[0][k]) * t);
        let (mut su, mut sv) = (0.0, 0.0);
        if slide {
            // 0x45ED40: the slide is the fractional part of camera distance x 1/scale x 1/ratio.
            let fu = inv[0] * inv_move * cx;
            let fv = inv[1] * inv_move * cz;
            su = -(fu - fu.floor());
            sv = fv - fv.floor();
        }
        let mut colours: Vec<[u8; 4]> = match (&layer.maps[0], &layer.maps[1]) {
            (Some(a), Some(b)) => a
                .iter()
                .zip(b)
                .map(|(pa, pb)| [0, 1, 2, 3].map(|k| ((pa[k] as u32 * tb + pb[k] as u32 * ta) >> 16) as u8))
                .collect(),
            (Some(a), None) => a.clone(),
            _ => vec![[255; 4]; N * N],
        };
        if let Some(flash) = &layer.flash {
            for s in flash.sparks.iter().filter(|s| s.cur >= 0) {
                let c = &mut colours[s.point];
                for k in 0..3 {
                    c[k] = ((c[k] as i32 + s.cur).min(s.cap[k] as i32)).clamp(0, 255) as u8;
                }
            }
        }
        let linear: Vec<[f32; 4]> = colours.iter().map(|c| Color::srgba_u8(c[0], c[1], c[2], c[3]).to_linear().to_f32_array()).collect();
        let mut uvs = Vec::with_capacity(N * N);
        for r in 0..N {
            for c in 0..N {
                uvs.push([c as f32 * inv[0] + layer.offset[0] + su, r as f32 * inv[1] + layer.offset[1] + sv]);
            }
        }
        if let Some(mut mesh) = meshes.get_mut(&layer.mesh) {
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, linear);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        }
    }
}

/// Retail tests the sun against the depth buffer, which sees every surface whichever way it faces.
/// Bevy's ray cast skips back faces unless told otherwise, and the X mirror of the track import
/// turns every track triangle round, so without this the sun test saw straight through rocks,
/// tunnels and buildings. Every world mesh counts both faces.
fn sun_blockers(mut commands: Commands, new: Query<Entity, (Added<Mesh3d>, Without<SkyPart>, Without<RenderLayers>, Without<RayCastBackfaces>)>) {
    for e in &new {
        commands.entity(e).try_insert(RayCastBackfaces);
    }
}

/// Where a world point lands on a 640x480 screen for this camera, if in front.
fn screen_point(cam: &Transform, proj: &Projection, p: Vec3) -> Option<Vec2> {
    let clip = proj.get_clip_from_view() * cam.to_matrix().inverse() * p.extend(1.0);
    if clip.w <= 0.0 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    Some(Vec2::new((ndc.x + 1.0) * 320.0, (1.0 - ndc.y) * 240.0))
}

/// The sun sprites in the sky pass, the occlusion test and the flare ghosts.
#[allow(clippy::too_many_arguments)]
fn place_sun_and_flare(
    time: Res<Time>,
    fx: Option<ResMut<SkyFx>>,
    cams: Query<(&Transform, &Projection), (With<WorldCamera>, Without<SunSprite>, Without<FlareSprite>)>,
    mut suns: Query<(&SunSprite, &mut Transform, &mut Visibility), Without<FlareSprite>>,
    mut ghosts: Query<(&FlareSprite, &mut Transform, &mut Visibility, &MeshMaterial3d<StandardMaterial>), Without<SunSprite>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut ray_cast: MeshRayCast,
    skip: Query<(), Or<(With<SkyPart>, With<NoFrustumCulling>, With<RenderLayers>)>>,
) {
    let (Some(mut fx), Ok((cam, proj))) = (fx, cams.single()) else { return };
    let Some(sun) = fx.sun else { return };
    let Projection::Perspective(p) = proj else { return };
    let sun_offset = Vec3::new(sun.x * fx.mirror, sun.y, sun.z) * fx.scale;
    let sun_world = cam.translation + sun_offset;
    let on_screen = screen_point(cam, proj, sun_world).filter(|s| s.x >= 0.0 && s.x < 640.0 && s.y >= 0.0 && s.y < 480.0);
    let suna = fx.flares.iter().find(|r| r.name == "suna").cloned();
    // The sun's sprites: screen-sized squares at the sun's point (size / 10 is the half-size in a
    // 640x480 screen; width and height scale with the screen separately, as retail's do).
    let dist = sun_offset.length();
    let tan = (p.fov * 0.5).tan();
    for (s, mut t, mut vis) in &mut suns {
        let Some(rec) = suna.as_ref() else { continue };
        *vis = if on_screen.is_some() { Visibility::Visible } else { Visibility::Hidden };
        let half = rec.sizes[s.0] * 0.1;
        t.translation = sun_world;
        t.rotation = cam.rotation;
        t.scale = Vec3::new(2.0 * dist * tan * p.aspect_ratio * half / 320.0, 2.0 * dist * tan * half / 240.0, 1.0);
    }
    // Occlusion, every 200 ms like retail: 16 rays round the sun's pixel (retail reads a 4x4
    // block of the depth buffer and counts the pixels still at sky depth).
    if time.elapsed_secs() >= fx.next_sun_test {
        fx.next_sun_test = time.elapsed_secs() + 0.2;
        fx.sun_visible = 0.0;
        if on_screen.is_some() {
            let dir = sun_offset.normalize();
            let right = cam.rotation * Vec3::X;
            let up = cam.rotation * Vec3::Y;
            // One pixel of a 480-line screen, as an angle at unit distance.
            let px = 2.0 * tan / 480.0;
            let filter = |e: Entity| !skip.contains(e);
            let settings = MeshRayCastSettings::default().with_visibility(RayCastVisibility::VisibleInView).with_filter(&filter).always_early_exit();
            let mut clear = 0;
            for i in 0..4 {
                for j in 0..4 {
                    let d = (dir + right * (i as f32 - 1.5) * px + up * (j as f32 - 1.5) * px).normalize();
                    if ray_cast.cast_ray(Ray3d::new(cam.translation, Dir3::new_unchecked(d)), &settings).is_empty() {
                        clear += 1;
                    }
                }
            }
            fx.sun_visible = clear as f32 * 0.0625;
        }
        if fx.sun_visible != fx.logged_visible {
            fx.logged_visible = fx.sun_visible;
            log(&format!("sun unhidden {:.2} (screen {:?})", fx.sun_visible, on_screen.map(|s| [s.x.round(), s.y.round()])));
        }
    }
    // The ghosts on the overlay: centre + (sun - centre) x the record's flare-line position.
    let v = fx.sun_visible;
    for (g, mut t, mut vis, mat) in &mut ghosts {
        let rec = &fx.flares[g.record];
        let slot = rec.name.as_bytes()[3].saturating_sub(b'a') as usize;
        let (Some(s), Some(&f)) = (on_screen, fx.flare_pos.get(slot)) else {
            *vis = Visibility::Hidden;
            continue;
        };
        if v < 0.05 {
            *vis = Visibility::Hidden;
            continue;
        }
        *vis = Visibility::Visible;
        let c = Vec2::new(320.0, 240.0);
        let at = c + (s - c) * f;
        let half = rec.sizes[g.half] * 0.1;
        // Turned by (x + y) / 1440 of a turn when the record says so (0x451710).
        let turn = if rec.flags & 4 != 0 { (at.x + at.y) / 1440.0 * std::f32::consts::TAU } else { 0.0 };
        t.translation = Vec3::new(at.x - 320.0, 240.0 - at.y, 0.0);
        t.rotation = Quat::from_rotation_z(-turn);
        t.scale = Vec3::new(2.0 * half, 2.0 * half, 1.0);
        // Below full visibility the alpha is the visibility itself (0x451710).
        let colour = rec.colours[g.half];
        let alpha = if v < 1.0 { v } else { (colour >> 24) as f32 / 255.0 };
        if let Some(mut m) = materials.get_mut(&mat.0) {
            let want = Color::srgba_u8((colour >> 16) as u8, (colour >> 8) as u8, colour as u8, (alpha * 255.0).round() as u8);
            if m.base_color != want {
                m.base_color = want;
            }
        }
    }
    if time.elapsed_secs() >= fx.next_log {
        fx.next_log = time.elapsed_secs() + 2.0;
        log(&format!("sun at screen {:?}, unhidden {:.2}", on_screen.map(|s| [s.x.round(), s.y.round()]), fx.sun_visible));
    }
}

/// The drops on the rear window while it rains (retail 0x4582C0 and its helpers).
#[allow(clippy::too_many_arguments)]
fn update_drops(
    mut commands: Commands,
    time: Res<Time>,
    fx: Option<ResMut<SkyFx>>,
    weather: Option<Res<WeatherState>>,
    input: Res<WeatherInput>,
    mut sheets: Query<&mut Visibility, With<DropSheet>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let (Some(mut fx), Some(weather)) = (fx, weather) else { return };
    if fx.drop_mesh.is_none() {
        let Some((px, w, h)) = common_picture("SPLT") else { return };
        let mesh = meshes.add(Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default()));
        let material = materials.add(StandardMaterial {
            base_color_texture: Some(images.add(picture(&px, w, h, false))),
            unlit: true,
            cull_mode: None,
            alpha_mode: AlphaMode::Blend,
            ..default()
        });
        commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material), Transform::IDENTITY, NoFrustumCulling, Visibility::Hidden, DropSheet));
        fx.drop_mesh = Some(mesh);
        return;
    }
    let Some((pos, yaw, speed)) = input.car else { return };
    // Glass size: 2 across, slope length from the two edges.
    let slope = Vec2::new(DROP_BOTTOM[0] - DROP_TOP[0], DROP_BOTTOM[1] - DROP_TOP[1]);
    let length = slope.length();
    let width = 2.0 * DROP_HALF_WIDTH;
    let area = (length * width * 0.769_230_8).min(1.0);
    let amount = (weather.now.rain + weather.now.snow).min(1.0);
    let want = ((amount * DROP_MAX).trunc() * area).trunc() as usize;
    // Faster than 22.2 m/s the drops point away from a spot a quarter of the way down the glass;
    // reversing they run down it, otherwise up it (0x4584F0). Snow-only weather keeps them still.
    let snowy = weather.now.rain <= 0.5 && weather.now.snow >= 0.5;
    let fast = !snowy && speed.abs() > 22.222;
    let reverse = speed <= 0.0;
    // Whole 64 Hz ticks since the last frame; the rest carries over.
    fx.drop_debt += time.delta_secs().min(0.25) * TICK_HZ;
    let ticks = fx.drop_debt.floor() as i32;
    fx.drop_debt -= ticks as f32;
    if fx.drops.len() < want {
        fx.drops.resize(want, WindowDrop::default());
    }
    let mut drops = std::mem::take(&mut fx.drops);
    let mut started = 0;
    for (i, d) in drops.iter_mut().enumerate() {
        let spawn = |fx: &mut SkyFx, d: &mut WindowDrop| {
            d.across = (fx.random() * 2.0 - 1.0) * DROP_HALF_WIDTH;
            d.down = fx.random() * length;
            d.alpha = (fx.random() * 255.0) as i32;
            d.age = 0;
            d.length = 0.0;
            d.width = 0.0;
            d.alive = true;
        };
        if !d.alive {
            if i < want && started < 5 {
                started += 1;
                spawn(&mut fx, d);
            } else {
                continue;
            }
        }
        // Direction target, eased 1/60 per update (0x4585D0).
        d.target = if fast {
            Vec2::new(d.across, d.down - length * 0.25).normalize_or(Vec2::Y)
        } else if reverse {
            Vec2::Y
        } else {
            -Vec2::Y
        };
        if d.dir == Vec2::ZERO {
            d.dir = d.target;
        } else {
            d.dir = (d.dir + (d.target - d.dir) / 60.0).normalize_or(d.target);
        }
        for _ in 0..ticks.max(0) {
            d.age += 1;
            let st = DROP_STAGES.iter().rev().find(|s| d.age >= s.0).copied().unwrap_or(DROP_STAGES[0]);
            let grow = (st.1 * 0.5 - d.length) * st.3 * 0.5;
            d.across -= grow * d.dir.x * 0.5;
            d.down -= grow * d.dir.y * 0.5;
            d.length += grow;
            d.width += (st.2 * 0.5 - d.width) * st.3 * 0.5;
            if d.alpha > st.4 {
                d.alpha = (d.alpha - st.5).max(st.4);
            } else if d.alpha < st.4 {
                d.alpha = (d.alpha + st.5).min(st.4);
            }
        }
        // Off the glass or faded out: start again if still wanted, else stop (0x4588D0).
        let off = d.across.abs() > DROP_HALF_WIDTH || d.down < 0.0 || d.down > length;
        if off || d.alpha < 15 {
            if i < want {
                spawn(&mut fx, d);
            } else {
                d.alive = false;
            }
        }
    }
    let live = drops.iter().filter(|d| d.alive).count();
    // Build the triangles in the world: head at the drop, tail corners length back along its
    // direction and width to each side (0x457FD0). Quad axes from the car's yaw.
    let fwd = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    let base = Vec3::new(pos.x, input.ground_y, pos.z);
    let top = base + Vec3::Y * DROP_TOP[0] + fwd * DROP_TOP[1];
    let down3 = (Vec3::Y * slope.x + fwd * slope.y).normalize();
    let across3 = right;
    let (mut positions, mut uvs, mut colours) = (Vec::with_capacity(live * 3), Vec::with_capacity(live * 3), Vec::with_capacity(live * 3));
    for (i, d) in drops.iter().filter(|d| d.alive).enumerate() {
        let head = top + across3 * d.across + down3 * d.down;
        let dir3 = across3 * d.dir.x + down3 * d.dir.y;
        let side = across3 * d.dir.y - down3 * d.dir.x;
        let tail = head + dir3 * d.length;
        positions.extend([head, tail - side * d.width, tail + side * d.width].map(|v| v.to_array()));
        uvs.extend(if i % 2 == 1 { [[0.0, 1.0], [0.0, 0.0], [1.0, 1.0]] } else { [[1.0, 0.0], [1.0, 1.0], [0.0, 0.0]] });
        let a = d.alpha.clamp(0, 255) as f32 / 255.0;
        colours.extend([[1.0, 1.0, 1.0, a]; 3]);
    }
    fx.drops = drops;
    for mut vis in &mut sheets {
        *vis = if live > 0 { Visibility::Visible } else { Visibility::Hidden };
    }
    if live == 0 {
        return;
    }
    let indices: Vec<u32> = (0..positions.len() as u32).collect();
    if let Some(mut mesh) = fx.drop_mesh.as_ref().and_then(|h| meshes.get_mut(h)) {
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
        mesh.insert_indices(Indices::U32(indices));
    }
}

/// Track glows: lamps, signs and lights from the track's `.crp` (type-2 entries in its `fe`
/// parts, exe 0x450D10), each drawn with its `flare.dat` record (0x451940).
#[derive(Resource, Default)]
pub struct Glows {
    /// One group per record half: the mesh and the glows it draws.
    groups: Vec<GlowGroup>,
    clock: f32,
}

struct GlowGroup {
    mesh: Handle<Mesh>,
    /// World position and blink phase of each glow.
    at: Vec<(Vec3, u32)>,
    half_size: f32,
    blink: [i32; 3],
}

/// Reads the track's glows and spawns one mesh per record half. Returns the entities.
pub fn spawn_glows(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, images: &mut Assets<Image>, track: &str) -> Vec<Entity> {
    let mut out = Vec::new();
    let flares = read_render("flare.dat").map(|b| parse_flare_dat(&b)).unwrap_or_default();
    let Some(game) = std::env::var("NFSPU_GAME_DIR").ok().map(PathBuf::from) else { return out };
    let Ok(raw) = std::fs::read(game.join("GameData").join("Track").join(format!("{track}.crp"))) else { return out };
    let data = if nfspu_formats::refpack::is_refpack(&raw) { nfspu_formats::refpack::decompress(&raw).unwrap_or_default() } else { raw };
    let Ok(c) = nfspu_formats::crp::parse(&data) else { return out };
    let mirror = crate::world_mirror_x();
    // Entry: type 2, a dword, position x y z w, up x y z, record name, blink phase (44 bytes).
    let mut found: Vec<(usize, Vec3, u32)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let parts = c.articles.iter().flat_map(|a| a.parts.iter()).chain(c.misc.iter());
    for p in parts {
        if p.tag[2..] != *b"fe" {
            continue;
        }
        let d = c.part_data(p);
        let w = |k: usize| u32::from_le_bytes([d[4 * k], d[4 * k + 1], d[4 * k + 2], d[4 * k + 3]]);
        if d.len() < 44 || w(0) != 2 {
            continue;
        }
        let name: String = w(9).to_be_bytes().iter().map(|&b| b as char).collect();
        let Some(rec) = flares.iter().position(|r| r.name == name) else { continue };
        let pos = Vec3::new(f32::from_bits(w(2)), f32::from_bits(w(3)), f32::from_bits(w(4)));
        // Glows on templates at the origin are not placed (the viewer skips those objects too).
        if pos.x.hypot(pos.z) < 50.0 || !seen.insert((rec, pos.x.to_bits(), pos.y.to_bits(), pos.z.to_bits())) {
            continue;
        }
        found.push((rec, Vec3::new(pos.x * mirror, pos.y, pos.z), w(10)));
    }
    let mut groups = Vec::new();
    let mut counts = std::collections::BTreeMap::new();
    for rec in 0..flares.len() {
        let at: Vec<(Vec3, u32)> = found.iter().filter(|g| g.0 == rec).map(|g| (g.1, g.2)).collect();
        if at.is_empty() {
            continue;
        }
        counts.insert(flares[rec].name.clone(), at.len());
        for half in 0..2 {
            let Some(material) = sprite_material(materials, images, &flares[rec], half, 0.0) else { continue };
            let mesh = meshes.add(Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default()));
            out.push(commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material), Transform::IDENTITY, NoFrustumCulling, Visibility::Hidden, GlowMesh)).id());
            groups.push(GlowGroup { mesh, at: at.clone(), half_size: flares[rec].sizes[half] * 0.1, blink: flares[rec].blink });
        }
    }
    log(&format!("track '{track}': {} glows {:?}", found.len(), counts));
    commands.insert_resource(Glows { groups, clock: 0.0 });
    out
}

/// Marks a glow mesh.
#[derive(Component)]
pub struct GlowMesh;

/// Camera-facing glow quads, blinking on retail's clock and faded by distance under fog.
fn update_glows(
    time: Res<Time>,
    glows: Option<ResMut<Glows>>,
    weather: Option<Res<WeatherState>>,
    cams: Query<&GlobalTransform, With<WorldCamera>>,
    mut vis: Query<&mut Visibility, With<GlowMesh>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let (Some(mut glows), Ok(cam)) = (glows, cams.single()) else { return };
    glows.clock += time.delta_secs() * TICK_HZ;
    let clock = glows.clock as u32;
    let eye = cam.translation();
    let right = cam.rotation() * Vec3::X;
    let up = cam.rotation() * Vec3::Y;
    let fog = weather.as_ref().is_some_and(|w| w.now.world_fog_density > nfspu_formats::weather::FOG_EPSILON);
    for g in &glows.groups {
        let (mut positions, mut colours, mut uvs) = (Vec::new(), Vec::new(), Vec::new());
        for &(p, phase) in &g.at {
            let [ph, on, off] = g.blink;
            if on != 0 || off != 0 {
                let period = (on + off).max(1) as u32;
                if (ph as u32).wrapping_add(phase).wrapping_add(clock) % period > on as u32 {
                    continue;
                }
            }
            // With fog on, retail fades glows out by 300 m (0x4515F0).
            let a = if fog { 1.0 - p.distance(eye) / 300.0 } else { 1.0 };
            if a <= 0.0 {
                continue;
            }
            let (r, u) = (right * g.half_size, up * g.half_size);
            positions.extend([p - r + u, p + r + u, p + r - u, p - r + u, p + r - u, p - r - u].map(|v| v.to_array()));
            uvs.extend([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
            colours.extend([[1.0, 1.0, 1.0, a]; 6]);
        }
        let n = positions.len() as u32;
        if let Some(mut mesh) = meshes.get_mut(&g.mesh) {
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
            mesh.insert_indices(Indices::U32((0..n).collect()));
        }
    }
    for mut v in &mut vis {
        if *v != Visibility::Visible {
            *v = Visibility::Visible;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flare_records_read_back() {
        let mut r = vec![0u8; 140];
        r[0..4].copy_from_slice(b"anus");
        r[4..8].copy_from_slice(b"ANUS");
        r[8..12].copy_from_slice(b"RULB");
        r[0x1C..0x20].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        r[0x34..0x38].copy_from_slice(&205.16f32.to_le_bytes());
        r[0x4C] = 4;
        let recs = parse_flare_dat(&r);
        assert_eq!(recs[0].name, "suna");
        assert_eq!(recs[0].textures, ["SUNA".to_string(), "BLUR".to_string()]);
        assert_eq!(recs[0].flags, 4);
        assert!((recs[0].sizes[0] - 205.16).abs() < 1e-3);
    }

    #[test]
    fn flare_line_falls_back_to_retail_table() {
        let p = parse_flare_pos(None);
        assert_eq!(&p[..9], &[1.0, 0.75, -0.75, 0.5, -0.5, 0.25, -0.25, 0.0, -1.0]);
    }
}

/// Removes everything the sky effects spawned (flare camera and sprites, sky layers, sun, drops,
/// glows) and their state, so nothing is left drawing over the menu after a race.
#[allow(clippy::type_complexity)]
pub fn clear(
    mut commands: Commands,
    parts: Query<Entity, Or<(With<FlareCamera>, With<FlareSprite>, With<SunSprite>, With<SkyLayerMesh>, With<DropSheet>, With<GlowMesh>)>>,
) {
    for e in &parts {
        commands.entity(e).despawn();
    }
    commands.remove_resource::<SkyFx>();
    commands.remove_resource::<Glows>();
}
