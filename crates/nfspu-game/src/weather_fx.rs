//! Drawing a track's conditions (see `weather.rs` for how retail decides them):
//! * distance fog for the world and a separate fog for the sky, from the sky `.bin` keyframes at
//!   the camera's road slice. The sky is drawn by its own camera first (render layer 1) so it can
//!   have its own fog, as retail switches fog between the sky pass and the world pass;
//! * rain and snow streaks around the camera, as many as the keyframe asks for (up to 300);
//! * the lightning clock: when the keyframes say the weather is stormy, bolts start at retail's
//!   rate; `sky_fx` draws each bolt in the sky;
//! * at night, the headlight pool on the road ahead, shaped by `Render/night.psh`.
//!
//! `main.rs` marks the world camera (`WorldCamera`) and the sky meshes (`SkyPart`), loads
//! `WeatherState` with the track, and fills `WeatherInput` with the car each frame.

use crate::race_path::RoadPath;
use crate::weather::{self, TimeOverride, TrackWeather, WeatherOverride};
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use nfspu_formats::weather::Conditions;
use std::path::PathBuf;

/// Render layer of the sky meshes and the sky camera.
pub const SKY_LAYER: usize = 1;
/// How far away retail draws the horizon (the reference radius of the horizon ring). Our sky dome
/// sits further out, so the sky fog density is scaled by this over the dome's distance. Using the
/// ring radius for the whole sky is a guess.
pub const RETAIL_SKY_DISTANCE: f32 = 400.0;
/// Retail's graphics detail for precipitation: the highest setting.
const DETAIL: usize = 3;

/// The camera that draws the track (main.rs's fly/chase camera).
#[derive(Component)]
pub struct WorldCamera;
/// The camera that draws only the sky, before the world camera.
#[derive(Component)]
pub struct SkyCamera;
/// A sky mesh: drawn by the sky camera only.
#[derive(Component)]
pub struct SkyPart;

/// The car, as `main.rs` sees it this frame.
#[derive(Resource, Default)]
pub struct WeatherInput {
    /// Position, yaw and forward speed (m/s) of the player's car.
    pub car: Option<(Vec3, f32, f32)>,
    /// Headlights switched on.
    pub headlights: bool,
    /// Ground heights at `pool_points()` around the car, filled by main.rs (None = no ground).
    pub pool_heights: Vec<Option<f32>>,
    /// Road height under the car, half the car's length, and whether it is braking (for the
    /// tail-light reflection on a wet road).
    pub ground_y: f32,
    pub half_length: f32,
    pub braking: bool,
}

/// The loaded track's weather and what it is doing now.
#[derive(Resource)]
pub struct WeatherState {
    pub track: TrackWeather,
    path: Option<RoadPath>,
    /// Per slice: a roof is over the road there (retail's "covered" flag; see `track::covered_points`).
    covered: Vec<bool>,
    slice: Option<usize>,
    /// Conditions at the camera's slice this frame.
    pub now: Conditions,
    /// Weather region blend at the camera's slice (0 region A .. 1 region B).
    pub region_blend: f32,
    /// How far away main.rs draws the sky, for the sky fog scale.
    pub sky_distance: f32,
    /// Lightning: a serial that goes up when a bolt starts, whether one is lit now, and its
    /// strength (0..2, from the keyframe's lightning amount). `sky_fx` draws it.
    pub bolt: u32,
    pub bolt_active: bool,
    pub bolt_strength: usize,
    flash: f32,
    flash_ticks: f32,
    tick_debt: f32,
    rng: u32,
    next_log: f32,
}

impl WeatherState {
    /// Loads a track's weather with the `NFSPU_WEATHER` / `NFSPU_TIME` overrides.
    pub fn load(track: &str, sky_distance: f32) -> Self {
        let game = PathBuf::from(std::env::var("NFSPU_GAME_DIR").unwrap_or_default());
        let weather = TrackWeather::load(&game, track, WeatherOverride::from_env(), TimeOverride::from_env());
        let path = crate::race_path::load(track, crate::race_path::Direction::Forward).ok().map(|r| r.path);
        let covered = match (&path, crate::track::load_ground(track)) {
            (Some(p), Ok(g)) => {
                let points: Vec<Vec3> = p.slices.iter().map(|s| s.pos).collect();
                let c = crate::track::covered_points(&g.ceilings, &points);
                crate_log(&format!("covered slices on {track}: {} of {}", c.iter().filter(|&&x| x).count(), c.len()));
                c
            }
            _ => Vec::new(),
        };
        Self {
            track: weather,
            path,
            covered,
            slice: None,
            now: Conditions::default(),
            region_blend: 0.0,
            sky_distance,
            bolt: 0,
            bolt_active: false,
            bolt_strength: 0,
            flash: 0.0,
            flash_ticks: 0.0,
            tick_debt: 0.0,
            rng: 0x1234_5678,
            next_log: 0.0,
        }
    }

    fn random(&mut self) -> f32 {
        // xorshift: deterministic, no dependency.
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / (1u32 << 24) as f32
    }

    /// The current road slice and the lap's slice count.
    pub fn slice(&self) -> Option<(usize, usize)> {
        Some((self.slice?, self.path.as_ref()?.slices.len()))
    }

    /// The road slice nearest a point, and its centre (world).
    pub fn slice_near(&self, p: Vec3) -> Option<(usize, Vec3)> {
        let path = self.path.as_ref()?;
        let i = path.nearest(p);
        Some((i, path.slices.get(i)?.pos))
    }

    /// A roof is over the given slice.
    pub fn is_covered(&self, slice: usize) -> bool {
        self.covered.get(slice).copied().unwrap_or(false)
    }

    /// Night lighting is on for this track.
    pub fn night(&self) -> bool {
        self.track.night
    }
}

pub struct WeatherFxPlugin;

impl Plugin for WeatherFxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WeatherInput>()
            .insert_resource(WeatherCapture::from_env())
            .add_systems(Update, (add_sky_camera, put_sky_on_its_layer, update_conditions, update_precipitation, update_headlight_pool, update_wet_reflection).chain())
            .add_systems(PostUpdate, (capture, follow_world_camera).chain().before(bevy::transform::TransformSystems::Propagate));
    }
}

/// Gives a new world camera its sky camera, and stops the world camera clearing the picture.
fn add_sky_camera(mut commands: Commands, cams: Query<(Entity, &Transform, &Projection), Added<WorldCamera>>, old: Query<Entity, With<SkyCamera>>, clear: Res<ClearColor>) {
    for (e, t, p) in &cams {
        for o in &old {
            commands.entity(o).despawn();
        }
        commands.spawn((
            Camera3d::default(),
            Camera { order: -1, clear_color: ClearColorConfig::Custom(clear.0), ..default() },
            *t,
            p.clone(),
            RenderLayers::layer(SKY_LAYER),
            DistanceFog { color: Color::NONE, falloff: FogFalloff::Exponential { density: 0.0 }, ..default() },
            SkyCamera,
        ));
        commands.entity(e).insert((
            Camera { order: 0, clear_color: ClearColorConfig::None, ..default() },
            DistanceFog { color: Color::NONE, falloff: FogFalloff::Exponential { density: 0.0 }, ..default() },
        ));
    }
}

/// Removes the weather effects and the track's weather (run when a race scene is taken down).
#[allow(clippy::type_complexity)]
pub fn clear(mut commands: Commands, fx: Query<Entity, Or<(With<SkyCamera>, With<Precipitation>, With<HeadlightPool>, With<WetReflection>)>>, mut input: ResMut<WeatherInput>) {
    for e in &fx {
        commands.entity(e).try_despawn();
    }
    commands.remove_resource::<WeatherState>();
    *input = WeatherInput::default();
}

fn put_sky_on_its_layer(mut commands: Commands, parts: Query<Entity, Added<SkyPart>>) {
    for e in &parts {
        commands.entity(e).insert(RenderLayers::layer(SKY_LAYER));
    }
}

/// How far the sky camera sees: the retail sky dome's corners reach about 4.4 km at our scale.
pub const SKY_CAMERA_FAR: f32 = 20_000.0;

/// The sky camera looks exactly where the world camera looks (but further).
fn follow_world_camera(world: Query<(&Transform, &Projection), (With<WorldCamera>, Without<SkyCamera>)>, mut sky: Query<(&mut Transform, &mut Projection), With<SkyCamera>>) {
    let Ok((wt, wp)) = world.single() else { return };
    for (mut t, mut p) in &mut sky {
        *t = *wt;
        if let (Projection::Perspective(a), Projection::Perspective(b)) = (&*p, wp) {
            if a.fov == b.fov && a.aspect_ratio == b.aspect_ratio && a.far == SKY_CAMERA_FAR.max(b.far) {
                continue;
            }
        }
        let mut wanted = wp.clone();
        if let Projection::Perspective(q) = &mut wanted {
            q.far = SKY_CAMERA_FAR.max(q.far);
        }
        *p = wanted;
    }
}

/// Conditions at the camera's slice, fog on both cameras, and lightning.
fn update_conditions(
    time: Res<Time>,
    state: Option<ResMut<WeatherState>>,
    mut world: Query<(&GlobalTransform, &mut DistanceFog), (With<WorldCamera>, Without<SkyCamera>)>,
    mut sky: Query<&mut DistanceFog, (With<SkyCamera>, Without<WorldCamera>)>,
) {
    let Some(mut state) = state else { return };
    let Ok((cam, mut world_fog)) = world.single_mut() else { return };
    // The rendered position (last frame's), not the Transform the chase camera is rewriting now.
    let pos = cam.translation();
    if let Some(path) = &state.path {
        let s = match state.slice {
            Some(prev) if prev < path.slices.len() => path.track(prev, pos),
            _ => path.nearest(pos),
        };
        state.slice = Some(s);
    }
    let (slice, count) = state.slice().unwrap_or((0, 1));
    let c = state.track.at(slice, count);
    state.now = c;
    state.region_blend = state.track.sky.as_ref().map_or(0.0, |s| s.region_blend_at(slice as i32, count as i32));

    // Lightning, on retail's 64 Hz tick: a chance per tick by storm strength, then a flash for
    // 60-120 ticks.
    state.tick_debt += time.delta_secs() * 64.0;
    while state.tick_debt >= 1.0 {
        state.tick_debt -= 1.0;
        if state.flash_ticks > 0.0 {
            state.flash_ticks -= 1.0;
        } else {
            let chance = weather::lightning_chance_per_tick(c.lightning);
            if chance > 0.0 && state.random() < chance {
                state.flash_ticks = (state.random() + 1.0) * 60.0;
                state.bolt = state.bolt.wrapping_add(1);
                state.bolt_strength = weather::lightning_strength(c.lightning).unwrap_or(0);
            }
        }
    }
    // Retail lights the sky where the bolt strikes (sky_fx); the fog is not brightened.
    state.bolt_active = state.flash_ticks > 0.0;
    state.flash = if state.bolt_active { 1.0 } else { 0.0 };

    world_fog.color = if c.world_fog_density > nfspu_formats::weather::FOG_EPSILON { Color::srgb(c.world_fog_colour[0], c.world_fog_colour[1], c.world_fog_colour[2]) } else { Color::NONE };
    world_fog.falloff = FogFalloff::Exponential { density: c.world_fog_density };
    let sky_scale = RETAIL_SKY_DISTANCE / state.sky_distance.max(1.0);
    for mut f in &mut sky {
        f.color = if c.sky_fog_density > nfspu_formats::weather::FOG_EPSILON { Color::srgb(c.sky_fog_colour[0], c.sky_fog_colour[1], c.sky_fog_colour[2]) } else { Color::NONE };
        f.falloff = FogFalloff::Exponential { density: c.sky_fog_density * sky_scale };
    }

    if state.next_log <= time.elapsed_secs() {
        state.next_log = time.elapsed_secs() + 2.0;
        crate_log(&format!(
            "weather at slice {slice}/{count}: world fog {:.5} rgb {:.0},{:.0},{:.0}; sky fog {:.5} rgb {:.0},{:.0},{:.0}; rain {:.2} snow {:.2} lightning {:.2} flash {:.2}; region blend {:.2}",
            c.world_fog_density,
            c.world_fog_colour[0] * 255.0,
            c.world_fog_colour[1] * 255.0,
            c.world_fog_colour[2] * 255.0,
            c.sky_fog_density,
            c.sky_fog_colour[0] * 255.0,
            c.sky_fog_colour[1] * 255.0,
            c.sky_fog_colour[2] * 255.0,
            c.rain,
            c.snow,
            c.lightning,
            state.flash,
            state.region_blend
        ));
    }
}

/// A mesh with one empty triangle, replaced every frame once there is something to draw.
pub(crate) fn placeholder_mesh() -> Mesh {
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3]);
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; 3]);
    m.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32; 4]; 3]);
    m.insert_indices(Indices::U32(vec![0, 1, 2]));
    m
}

/// Writes to the shared game log (`logs/m0.log`), the same file main.rs uses.
pub(crate) fn crate_log(msg: &str) {
    use std::io::Write as _;
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("logs").join("m0.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "[weather] {msg}");
    }
}

/// One falling drop or flake, relative to the box round the camera.
#[derive(Clone, Copy)]
struct Drop {
    pos: Vec3,
    speed: f32,
}

/// Rain and snow streaks round the camera.
#[derive(Component)]
pub struct Precipitation {
    snow: bool,
    drops: Vec<Drop>,
    mesh: Handle<Mesh>,
}

/// The box round the camera that drops live in: 15 m to each side, 3 m below to 10 m above
/// (retail's emitter numbers; their exact use is a guess).
const BOX_HALF: f32 = 15.0;
const BOX_LOW: f32 = -3.0;
const BOX_HIGH: f32 = 10.0;

fn sprite(file: &str, name: &str, images: &mut Assets<Image>) -> Option<Handle<Image>> {
    let game = PathBuf::from(std::env::var("NFSPU_GAME_DIR").ok()?);
    let raw = std::fs::read(game.join("GameData").join("Render").join(file)).ok()?;
    let data = if nfspu_formats::refpack::is_refpack(&raw) { nfspu_formats::refpack::decompress(&raw).ok()? } else { raw };
    let archive = nfspu_formats::shpi::parse(&data).ok()?;
    let e = archive.entries.iter().find(|e| e.name.trim() == name)?;
    let rgba = nfspu_formats::shpi::decode_rgba8(&data, e)?;
    let mut image = Image::new(
        Extent3d { width: e.width as u32, height: e.height as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    Some(images.add(image))
}

#[allow(clippy::too_many_arguments)]
fn update_precipitation(
    mut commands: Commands,
    time: Res<Time>,
    state: Option<ResMut<WeatherState>>,
    cams: Query<&GlobalTransform, With<WorldCamera>>,
    mut fx: Query<(&mut Precipitation, &mut Visibility)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let (Some(mut state), Ok(cam)) = (state, cams.single()) else { return };
    if fx.is_empty() {
        for snow in [false, true] {
            // Snow uses its sprite. The rain sprite is a one-pixel line down each edge, fading in
            // towards the bottom; rain is drawn as thin streaks with that fade in vertex colours.
            let texture = if snow { sprite("particle.fsh", "snow", &mut images) } else { None };
            let mesh = meshes.add(placeholder_mesh());
            let material = materials.add(StandardMaterial {
                base_color: if snow { Color::srgba(1.0, 1.0, 1.0, 0.9) } else { Color::srgba(0.82, 0.85, 0.92, 1.0) },
                base_color_texture: texture,
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                cull_mode: None,
                fog_enabled: false,
                ..default()
            });
            commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material), Transform::IDENTITY, NoFrustumCulling, Visibility::Hidden, Precipitation { snow, drops: Vec::new(), mesh }));
        }
        return;
    }
    let dt = time.delta_secs().min(0.1);
    let eye = cam.translation();
    let rotation = cam.rotation();
    let right = rotation * Vec3::X;
    let now = state.now;
    for (mut p, mut vis) in &mut fx {
        let amount = if p.snow { now.snow } else { now.rain };
        // Retail: round(amount x particle count - 0.5), none below 3.
        let mut want = (amount * weather::PRECIPITATION_PARTICLES[DETAIL] as f32 - 0.5).round().max(0.0) as usize;
        if want < 3 {
            want = 0;
        }
        while p.drops.len() < want {
            let pos = Vec3::new((state.random() * 2.0 - 1.0) * BOX_HALF, BOX_LOW + state.random() * (BOX_HIGH - BOX_LOW), (state.random() * 2.0 - 1.0) * BOX_HALF);
            let speed = if p.snow { 1.3 + state.random() * 0.6 } else { 25.6 + state.random() * 10.0 };
            p.drops.push(Drop { pos: eye + pos, speed });
        }
        p.drops.truncate(want);
        *vis = if want == 0 { Visibility::Hidden } else { Visibility::Visible };
        if want == 0 {
            continue;
        }
        // Rain falls straight down, snow drifts; both stay inside the box round the camera.
        let t = time.elapsed_secs();
        let span = Vec3::new(2.0 * BOX_HALF, BOX_HIGH - BOX_LOW, 2.0 * BOX_HALF);
        let low = eye + Vec3::new(-BOX_HALF, BOX_LOW, -BOX_HALF);
        let (mut positions, mut uvs, mut indices) = (Vec::with_capacity(want * 4), Vec::with_capacity(want * 4), Vec::with_capacity(want * 6));
        let mut colours: Vec<[f32; 4]> = Vec::with_capacity(want * 4);
        let snow = p.snow;
        for (i, d) in p.drops.iter_mut().enumerate() {
            let fall = if snow { Vec3::new(0.3 * (t * 0.7 + i as f32).sin(), -d.speed, 0.3 * (t * 0.5 + i as f32 * 1.7).cos()) } else { Vec3::new(0.0, -d.speed, 0.0) };
            d.pos += fall * dt;
            let rel = (d.pos - low).rem_euclid(span);
            d.pos = low + rel;
            let (half_w, len) = if snow { (0.06, 0.12) } else { (0.01, 0.5) };
            let up = if snow { rotation * Vec3::Y * len } else { Vec3::Y * len };
            let base = positions.len() as u32;
            let side = right * half_w;
            positions.extend([d.pos - side + up, d.pos + side + up, d.pos + side, d.pos - side].map(|v| v.to_array()));
            uvs.extend([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
            let (top, bottom) = if snow { (1.0, 1.0) } else { (0.05, 0.55) };
            colours.extend([[1.0, 1.0, 1.0, top], [1.0, 1.0, 1.0, top], [1.0, 1.0, 1.0, bottom], [1.0, 1.0, 1.0, bottom]]);
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        if let Some(mut mesh) = meshes.get_mut(&p.mesh) {
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
            mesh.insert_indices(Indices::U32(indices));
            keep_non_empty(&mut mesh);
        }
    }
}

/// Columns across and rows ahead of the headlight pool grid.
pub const POOL_COLS: usize = 15;
pub const POOL_ROWS: usize = 26;
/// How far the pool grid reaches. Retail's light map covers 0..99 m of the lamp frame, but its
/// height falloff ends the pool on the road about 45 m ahead.
const POOL_AHEAD: f32 = 60.0;
/// The lamp frame: 4.8 m above the car and tilted 17 degrees down (exe 0x5CEC54, 0x5CEC5C). That
/// the 4.8 m is a height is our reading; it puts the brightest band about 16 m ahead.
const LAMP_HEIGHT: f32 = 4.8;
const LAMP_TILT_DEG: f32 = 17.0;
const POOL_SIDE: f32 = 17.4;
/// How far the pool is lifted above the road so it draws over it.
const POOL_LIFT: f32 = 0.06;

/// World (x, z) of every pool grid point for a car at `pos` facing `yaw`, row by row; main.rs
/// fills `WeatherInput::pool_heights` with the ground height at each.
pub fn pool_points(pos: Vec3, yaw: f32) -> Vec<(f32, f32)> {
    let fwd = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    let mut out = Vec::with_capacity(POOL_COLS * POOL_ROWS);
    for r in 0..POOL_ROWS {
        let ahead = POOL_AHEAD * (r as f32 / (POOL_ROWS - 1) as f32).powf(1.5);
        for c in 0..POOL_COLS {
            let side = POOL_SIDE * (c as f32 / (POOL_COLS - 1) as f32 * 2.0 - 1.0);
            let p = pos + fwd * ahead + right * side;
            out.push((p.x, p.z));
        }
    }
    out
}

/// The headlight pool on the road at night.
#[derive(Component)]
pub struct HeadlightPool {
    mesh: Handle<Mesh>,
}

fn update_headlight_pool(
    mut commands: Commands,
    state: Option<Res<WeatherState>>,
    input: Res<WeatherInput>,
    mut pool: Query<(&HeadlightPool, &mut Visibility)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(state) = state else { return };
    let Some(map) = state.track.light_map.as_ref() else { return };
    if pool.is_empty() {
        if !state.night() {
            return;
        }
        let mesh = meshes.add(placeholder_mesh());
        // Retail adds the lamp colour (0x808080) times the map to the road's vertex colours; an
        // additive overlay is our approximation of that.
        let material = materials.add(StandardMaterial { base_color: Color::WHITE, alpha_mode: AlphaMode::Add, unlit: true, cull_mode: None, fog_enabled: true, ..default() });
        commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material), Transform::IDENTITY, NoFrustumCulling, Visibility::Hidden, HeadlightPool { mesh }));
        return;
    }
    let Ok((p, mut vis)) = pool.single_mut() else { return };
    let (Some((pos, yaw, _)), true, true) = (input.car, input.headlights, state.night()) else {
        *vis = Visibility::Hidden;
        return;
    };
    if input.pool_heights.len() != POOL_COLS * POOL_ROWS {
        *vis = Visibility::Hidden;
        return;
    }
    *vis = Visibility::Visible;
    let points = pool_points(pos, yaw);
    let fwd = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    let decal = state.track.lighting.light_decal[1];
    let (sin_t, cos_t) = LAMP_TILT_DEG.to_radians().sin_cos();
    let lamp = pos + Vec3::Y * LAMP_HEIGHT;
    let lamp_fwd = fwd * cos_t - Vec3::Y * sin_t;
    let lamp_up = Vec3::Y * cos_t + fwd * sin_t;
    let mut positions = Vec::with_capacity(points.len());
    let mut colours = Vec::with_capacity(points.len());
    for (i, (x, z)) in points.iter().enumerate() {
        let y = input.pool_heights[i].unwrap_or(pos.y - 0.5);
        let rel = Vec3::new(*x, y, *z) - lamp;
        let light = map.intensity(rel.dot(right), rel.dot(lamp_fwd), rel.dot(lamp_up)) * 0.5 * decal;
        // Linear-space value of an sRGB step of `light` on a mid-grey road: an approximation of
        // retail's vertex-colour addition.
        let lin = Color::srgb(light, light, light * 0.95).to_linear();
        positions.push([*x, y + POOL_LIFT, *z]);
        colours.push([lin.red * 0.5, lin.green * 0.5, lin.blue * 0.5, 1.0]);
    }
    let mut indices = Vec::with_capacity((POOL_ROWS - 1) * (POOL_COLS - 1) * 6);
    for r in 0..POOL_ROWS - 1 {
        for c in 0..POOL_COLS - 1 {
            let a = (r * POOL_COLS + c) as u32;
            let b = a + POOL_COLS as u32;
            if input.pool_heights[a as usize].is_none() || input.pool_heights[b as usize].is_none() {
                continue;
            }
            indices.extend([a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    if let Some(mut mesh) = meshes.get_mut(&p.mesh) {
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
        mesh.remove_attribute(Mesh::ATTRIBUTE_UV_0);
        mesh.insert_indices(Indices::U32(indices));
        keep_non_empty(&mut mesh);
    }
}

/// Opt-in captures for comparing with retail: `NFSPU_WEATHER_CAPTURE=start,0.3,0.6` waits at the
/// start (normal chase view), then puts the camera behind the road at those fractions of the
/// lap, saves `logs/weather/ours_<track>_<shot>.png` for each and exits.
#[derive(Resource, Default)]
struct WeatherCapture {
    shots: Vec<Option<f32>>,
    index: usize,
    frames: u32,
    waiting: bool,
    done: bool,
}

impl WeatherCapture {
    fn from_env() -> Self {
        let shots = std::env::var("NFSPU_WEATHER_CAPTURE")
            .map(|v| v.split(',').filter(|s| !s.trim().is_empty()).map(|s| s.trim().parse::<f32>().ok()).collect())
            .unwrap_or_default();
        Self { shots, ..default() }
    }
}

fn capture(
    mut cap: ResMut<WeatherCapture>,
    state: Option<Res<WeatherState>>,
    mut cams: Query<&mut Transform, With<WorldCamera>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    if cap.shots.is_empty() || cap.done {
        return;
    }
    let Some(state) = state else { return };
    if cap.index >= cap.shots.len() {
        crate_log("weather capture: all shots saved, intentional exit");
        cap.done = true;
        exit.write(AppExit::Success);
        return;
    }
    let shot = cap.shots[cap.index];
    if let (Some(frac), Some(path)) = (shot, state.path.as_ref()) {
        let n = path.slices.len().max(1);
        let i = ((frac.rem_euclid(1.0)) * n as f32) as usize % n;
        let ahead = (i + 12) % n;
        let s = &path.slices[i];
        let eye = s.pos - s.forward * 7.0 + s.up * 2.6;
        // `NFSPU_WEATHER_CAPTURE_UP=<m>` raises the point looked at, to frame the sky.
        let lift = std::env::var("NFSPU_WEATHER_CAPTURE_UP").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
        let look = path.slices[ahead].pos + s.up * (1.0 + lift);
        for mut t in &mut cams {
            *t = Transform::from_translation(eye).looking_at(look, Vec3::Y);
        }
    }
    if cap.waiting {
        return;
    }
    cap.frames += 1;
    // The start shot waits for the countdown to settle the chase camera; the others for the fog
    // and drops to catch up with the new place.
    if cap.frames < if shot.is_none() { 240 } else { 120 } {
        return;
    }
    let tag = shot.map_or("start".to_string(), |f| format!("{:03}", (f * 100.0).round() as i32));
    let safe: String = state.track.track.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("logs").join("weather");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join(format!("ours_{safe}_{tag}.png"));
    crate_log(&format!("weather capture: shot {tag} at slice {:?}, conditions {:?}", state.slice(), state.now));
    cap.waiting = true;
    commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(
        move |captured: On<bevy::render::view::screenshot::ScreenshotCaptured>, mut cap: ResMut<WeatherCapture>| {
            let result = captured.image.clone().try_into_dynamic().map_err(|e| e.to_string()).and_then(|i| i.to_rgb8().save(&file).map_err(|e| e.to_string()));
            crate_log(&format!("weather capture: {} -> {:?}", file.display(), result));
            cap.index += 1;
            cap.frames = 0;
            cap.waiting = false;
        },
    );
}

/// The red smear of the tail lights on a wet road.
#[derive(Component)]
pub struct WetReflection {
    mesh: Handle<Mesh>,
}

/// Retail shows the tail lights reflected on autobahn's wet road (let's play); how it draws them was
/// not traced. Ours: two additive red streaks on the road behind the tail lights, brighter when
/// braking. **Guess** in size and strength.
fn update_wet_reflection(
    mut commands: Commands,
    state: Option<Res<WeatherState>>,
    input: Res<WeatherInput>,
    mut fx: Query<(&WetReflection, &mut Visibility)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(state) = state else { return };
    if !state.track.wet {
        for (_, mut v) in &mut fx {
            *v = Visibility::Hidden;
        }
        return;
    }
    if fx.is_empty() {
        let mesh = meshes.add(placeholder_mesh());
        let material = materials.add(StandardMaterial { base_color: Color::WHITE, alpha_mode: AlphaMode::Add, unlit: true, cull_mode: None, ..default() });
        commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material), Transform::IDENTITY, NoFrustumCulling, Visibility::Hidden, WetReflection { mesh }));
        return;
    }
    let Ok((r, mut vis)) = fx.single_mut() else { return };
    let Some((pos, yaw, _)) = input.car else {
        *vis = Visibility::Hidden;
        return;
    };
    let strength = if input.braking { 1.0 } else if input.headlights { 0.45 } else { 0.0 };
    if strength == 0.0 {
        *vis = Visibility::Hidden;
        return;
    }
    *vis = Visibility::Visible;
    let fwd = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    let y = input.ground_y + 0.03;
    let rear = pos - fwd * input.half_length.max(1.5);
    let red = Color::srgb(0.3 * strength, 0.03 * strength, 0.02 * strength).to_linear();
    let (mut positions, mut colours, mut indices) = (Vec::new(), Vec::new(), Vec::new());
    for side in [-0.55f32, 0.55] {
        let c = rear + right * side;
        let base = positions.len() as u32;
        // Near end just under the bumper, far end 1.6 m back, fading out.
        for (along, half, k) in [(-0.2f32, 0.12f32, 1.0f32), (1.6, 0.07, 0.0)] {
            for s in [-half, half] {
                let p = c - fwd * along + right * s;
                positions.push([p.x, y, p.z]);
                colours.push([red.red * k, red.green * k, red.blue * k, 1.0]);
            }
        }
        indices.extend([base, base + 1, base + 3, base, base + 3, base + 2]);
    }
    if let Some(mut mesh) = meshes.get_mut(&r.mesh) {
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
        mesh.remove_attribute(Mesh::ATTRIBUTE_UV_0);
        mesh.insert_indices(Indices::U32(indices));
        keep_non_empty(&mut mesh);
    }
}

/// Bevy's GPU mesh allocator logs "Use-after-free: attempted to copy element data for an
/// unallocated key" when a mesh it already holds is rewritten with no vertices or no indices.
/// Effect meshes rebuilt every frame (rain, glows, light pools) can have nothing to draw, so such
/// a frame gets one zero-area triangle instead, which draws nothing.
pub(crate) fn keep_non_empty(mesh: &mut Mesh) {
    let empty = mesh.count_vertices() == 0 || mesh.indices().is_none_or(|i| i.is_empty());
    if !empty {
        return;
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3]);
    if mesh.contains_attribute(Mesh::ATTRIBUTE_UV_0) {
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; 3]);
    }
    if mesh.contains_attribute(Mesh::ATTRIBUTE_COLOR) {
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32; 4]; 3]);
    }
    mesh.insert_indices(Indices::U32(vec![0, 1, 2]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_grid_starts_at_the_car_and_reaches_ahead() {
        let pts = pool_points(Vec3::ZERO, 0.0);
        assert_eq!(pts.len(), POOL_COLS * POOL_ROWS);
        // yaw 0 faces -z; the first row is at the car, the last 99 m ahead.
        assert!((pts[POOL_COLS / 2].1).abs() < 1e-4);
        assert!((pts[(POOL_ROWS - 1) * POOL_COLS + POOL_COLS / 2].1 + POOL_AHEAD).abs() < 1e-3);
        assert!((pts[0].0 + POOL_SIDE).abs() < 1e-4, "first column on the left");
    }
}
