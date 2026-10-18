//! The 3D room behind front-end pages with `room=ID_GARAGE`: the garage model turning in front of
//! a fixed camera, with the quick-race car on its pit when the page has a car box (`NFS5.3DCAR`).
//! A child of `menu_mode`, so it reads the front end's state directly. Logic and evidence are in
//! `nfspu_game::garage` and `docs/provenance/garage.md`.
//!
//! The 3D camera draws the room into a texture the size of the window's 640x480 area. One UI node
//! then shows the composed menu image over it, blended in screen space as retail does (the menu
//! image is see-through where a room page draws nothing); Bevy's own UI blending works in linear
//! light, which made half-transparent art over the room too light.

use super::{pointer, project, FrontEnd, MenuVisual};
use bevy::asset::{uuid_handle, RenderAssetUsages};
use bevy::camera::{ClearColorConfig, ImageRenderTarget, RenderTarget};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::{Shader, ShaderRef};
use bevy::ui_render::prelude::{MaterialNode, UiMaterial, UiMaterialPlugin};
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::PrimaryWindow;
use nfspu_game::car_model::{self, CarOptions, Piece};
use nfspu_game::garage::{self, Fade, RoomLook, Turntable, Zone};

/// Brightness of the room light rule (ambient + light from `garage::GARAGE`), applied as a
/// linear-light vertex colour (no tonemapping). Chosen so sampled patches of wall, floor and pit
/// come close to the retail recording (`logs/garage_compare_quickrace.png`, numbers in
/// `docs/provenance/garage.md`; **fitted**, not from the code).
const ROOM_GAIN: f32 = 0.5;

/// Fan turn rates in turns per second, by animation slot: retail feeds slot 1 the clock / 128 and
/// slot 2 the clock / 192 (128 Hz clock). Reading the value as whole turns is a **guess**.
const FAN_RATE: [f32; 3] = [0.0, 1.0, 128.0 / 192.0];

/// Lights fade on in about half a second (retail times them with the system clock; the length is
/// read from data not traced yet, **guess**).
const LIGHT_SECONDS: f32 = 0.4;
/// Level of detail of the show car. Retail's showroom uses the garage model (level 1), the only
/// levels (with 2) where the doors are separate parts; but the car loader draws levels 1-2 with
/// wrong artwork on the rear quarters, so level 3 is used and the doors cannot open
/// (`NFSPU_GARAGE_LOD=2` shows them opening).
const SHOW_CAR_LEVEL: u8 = 3;

/// Doors: 255 steps; the step rate is not traced (**guess**: one second end to end).
const DOOR_SECONDS: f32 = 1.0;

pub struct MenuGarage;

impl Plugin for MenuGarage {
    fn build(&self, app: &mut App) {
        app.insert_resource(Garage::from_env())
            .add_plugins(UiMaterialPlugin::<ScreenBlend>::default())
            .add_systems(Startup, spawn_camera)
            .add_systems(Update, (stage, inspect, animate, blend).chain());
    }
}

#[derive(Resource)]
struct Garage {
    /// The room drawn now, and its spinning root (the room's origin, where the car stands).
    look: Option<RoomLook>,
    root: Option<Entity>,
    /// The car on show: (model, paint section, body style) and its entity.
    car: Option<(String, usize, usize)>,
    car_root: Option<Entity>,
    /// The car's box on the page, in layout units.
    car_box: Option<[f32; 4]>,
    layout_for: String,
    turntable: Turntable,
    clock: f32,
    doors: [Fade; 2],
    front_lights: Fade,
    rear_lights: Fade,
    cabin: Fade,
    hovering: bool,
    /// Scripted test clicks already made (`NFSPU_GARAGE_CLICKS`).
    clicks_done: usize,
    /// The texture the room is drawn into.
    target: Handle<Image>,
}

impl Garage {
    fn from_env() -> Garage {
        // Retail starts from wherever its clock is; `NFSPU_GARAGE_TURN=0..1` fixes the start for
        // captures. The default (0.85, front to the right, toward the camera) is the angle of the
        // first retail frame (**guess**).
        let start = std::env::var("NFSPU_GARAGE_TURN").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(0.85);
        Garage {
            look: None,
            root: None,
            car: None,
            car_root: None,
            car_box: None,
            layout_for: String::new(),
            turntable: Turntable { turns: start },
            clock: 0.0,
            doors: Default::default(),
            front_lights: Default::default(),
            rear_lights: Default::default(),
            cabin: Default::default(),
            hovering: false,
            clicks_done: 0,
            target: Handle::default(),
        }
    }
}

/// The 3D view: drawn before the menu image, into the window's 640x480 area.
#[derive(Component)]
struct GarageCamera;

/// A turning part of the room (a fan), by animation slot.
#[derive(Component)]
struct RoomFan(u8);

/// A door of the show car: car animation slot 6 (left) or 7 (right), hinged at the entity's origin.
#[derive(Component)]
struct ShowDoor(u8);

/// A head- or tail-light mesh of the show car with its own material (so it can glow).
#[derive(Component)]
struct ShowLamp {
    front: bool,
    material: Handle<StandardMaterial>,
}

/// One of the show car's two cabins: the detailed one (seen from inside) or the outside one.
#[derive(Component)]
struct ShowCabin {
    cockpit: bool,
}

/// A soft round glare in front of a headlight, shown while the headlights are on (the retail
/// recording shows a bright round glow on each lamp; its art and size are not traced, **guess**).
#[derive(Component)]
struct ShowGlare;

/// Glare size in metres across.
const GLARE_SIZE: f32 = 0.5;

/// A white radial glow with alpha falling off to the edge.
fn glare_image() -> Image {
    const N: u32 = 64;
    let mut rgba = Vec::with_capacity((N * N * 4) as usize);
    for y in 0..N {
        for x in 0..N {
            let (dx, dy) = ((x as f32 + 0.5) / N as f32 * 2.0 - 1.0, (y as f32 + 0.5) / N as f32 * 2.0 - 1.0);
            let a = (1.0 - (dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0).powf(2.0);
            rgba.extend_from_slice(&[255, 255, 245, (a * 255.0) as u8]);
        }
    }
    Image::new(Extent3d { width: N, height: N, depth_or_array_layers: 1 }, TextureDimension::D2, rgba, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default())
}

/// The node that shows the menu image blended over the room.
#[derive(Component)]
struct GarageBlend;

const BLEND_SHADER: Handle<Shader> = uuid_handle!("6b1f2a7e-3c55-4b0e-9a1d-5e2f8c4d7a10");

/// Screen-space "over": both textures are sRGB, so sampling gives linear values; they are turned
/// back into their stored (gamma) values, mixed by the menu's alpha, and returned as linear light
/// for the window. The result equals retail's blend on the stored values.
const BLEND_WGSL: &str = r#"
#import bevy_ui::ui_vertex_output::UiVertexOutput

@group(1) @binding(0) var menu_texture: texture_2d<f32>;
@group(1) @binding(1) var menu_sampler: sampler;
@group(1) @binding(2) var room_texture: texture_2d<f32>;
@group(1) @binding(3) var room_sampler: sampler;

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((max(c, vec3<f32>(0.0)) + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let menu = textureSample(menu_texture, menu_sampler, in.uv);
    let room = textureSample(room_texture, room_sampler, in.uv);
    let out = mix(to_srgb(room.rgb), to_srgb(menu.rgb), menu.a);
    return vec4<f32>(to_linear(out), 1.0);
}
"#;

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct ScreenBlend {
    #[texture(0)]
    #[sampler(1)]
    menu: Handle<Image>,
    #[texture(2)]
    #[sampler(3)]
    room: Handle<Image>,
}

impl UiMaterial for ScreenBlend {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(BLEND_SHADER)
    }
}

fn spawn_camera(mut commands: Commands, mut images: ResMut<Assets<Image>>, mut shaders: ResMut<Assets<Shader>>, mut blends: ResMut<Assets<ScreenBlend>>, mut garage: ResMut<Garage>) {
    let _ = shaders.insert(&BLEND_SHADER, Shader::from_wgsl(BLEND_WGSL, "nfspu/garage_blend.wgsl"));
    // resized to the window's 640x480 area by `stage`
    let target = images.add(Image::new_target_texture(640, 480, TextureFormat::Rgba8UnormSrgb, None));
    garage.target = target.clone();
    commands.spawn((
        Camera3d::default(),
        // `NFSPU_GARAGE_DEBUG=1` clears the 3D view to magenta, to see where it is drawn.
        Camera {
            order: -1,
            is_active: false,
            clear_color: ClearColorConfig::Custom(if std::env::var("NFSPU_GARAGE_DEBUG").as_deref() == Ok("1") { Color::srgb(1.0, 0.0, 1.0) } else { Color::BLACK }),
            ..default()
        },
        RenderTarget::Image(ImageRenderTarget { handle: target.clone(), scale_factor: 1.0 }),
        // unlit textures and baked light must come out as they are, as in retail
        Tonemapping::None,
        Projection::Perspective(PerspectiveProjection { fov: garage::vertical_fov(&garage::GARAGE, 4.0 / 3.0), near: 0.05, far: 100.0, ..default() }),
        Transform::from_xyz(0.0, 0.0, 0.0).looking_to(Vec3::Z, Vec3::Y),
        GarageCamera,
    ));
    let material = blends.add(ScreenBlend { menu: Handle::default(), room: target });
    commands.spawn((Node { position_type: PositionType::Absolute, ..default() }, MaterialNode(material), Visibility::Hidden, GarageBlend));
}

/// Shows the menu through the blend node while a room is drawn, and the plain menu image
/// otherwise; the 3D camera only runs while a room is drawn.
#[allow(clippy::type_complexity)]
fn blend(
    garage: Res<Garage>,
    mut menu: Query<(&Node, &ImageNode, &mut Visibility), (With<MenuVisual>, Without<GarageBlend>)>,
    mut node: Query<(&mut Node, &MaterialNode<ScreenBlend>, &mut Visibility), With<GarageBlend>>,
    mut blends: ResMut<Assets<ScreenBlend>>,
    mut cameras: Query<&mut Camera, With<GarageCamera>>,
) {
    let active = garage.root.is_some();
    if let Ok(mut camera) = cameras.single_mut() {
        if camera.is_active != active {
            camera.is_active = active;
        }
    }
    let (Ok((menu_node, image, mut menu_vis)), Ok((mut blend_node, material, mut blend_vis))) = (menu.single_mut(), node.single_mut()) else { return };
    let (want_menu, want_blend) = if active { (Visibility::Hidden, Visibility::Inherited) } else { (Visibility::Inherited, Visibility::Hidden) };
    if *menu_vis != want_menu {
        *menu_vis = want_menu;
    }
    if *blend_vis != want_blend {
        *blend_vis = want_blend;
    }
    if !active {
        return;
    }
    if (blend_node.left, blend_node.top, blend_node.width, blend_node.height) != (menu_node.left, menu_node.top, menu_node.width, menu_node.height) {
        blend_node.left = menu_node.left;
        blend_node.top = menu_node.top;
        blend_node.width = menu_node.width;
        blend_node.height = menu_node.height;
    }
    // Touched every frame: the material's GPU binding is only rebuilt when the material changes,
    // and it must follow the room texture too (recreated when the window is resized), not just a
    // new menu image. With a still pointer nothing else changes it, and the room froze on screen.
    if let Some(mut m) = blends.get_mut(&material.0) {
        if m.menu != image.image {
            m.menu = image.image.clone();
        }
    }
}

/// The car on the page, from the quick-race choice: (model file, paint section, body style).
/// The paint is the colour list's choice; an owned car's own colour (kept in the save) is not
/// read yet.
fn wanted_car(state: &FrontEnd) -> Option<(String, usize, usize)> {
    let (index, _owned) = state.quick.chosen_car()?;
    let car = state.quick.cars.get(index)?;
    let dir = state.game_dir.join("GameData").join("CarModel");
    let clr = std::fs::read(dir.join(format!("{}.clr", car.model))).ok();
    let sections = clr.map(|b| garage::clr_sections(&String::from_utf8_lossy(&b))).unwrap_or_default();
    let paint = sections.get(state.quick.colour).copied().unwrap_or(CarOptions::default().paint);
    let tpg = std::fs::read(dir.join(format!("{}.tpg", car.model))).map(|b| String::from_utf8_lossy(&b).to_string()).unwrap_or_default();
    Some((car.model.clone(), paint, garage::style_for(&car.name, &tpg)))
}

fn layout_path(game_dir: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
    let dir = game_dir.join("FEData").join("Layouts");
    let wanted = format!("{name}.lay").to_ascii_lowercase();
    std::fs::read_dir(&dir).ok()?.flatten().find(|e| e.file_name().to_string_lossy().to_ascii_lowercase() == wanted).map(|e| e.path())
}

/// Builds or removes the room and the car to match the page, and keeps the camera on the
/// 640x480 area.
#[allow(clippy::too_many_arguments)]
fn stage(
    mut garage: ResMut<Garage>,
    state: Res<FrontEnd>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    // The room texture follows the size of the window's 640x480 area, in physical pixels.
    if let Ok(window) = windows.single() {
        let (scale, _) = project(window);
        let f = window.scale_factor();
        let size = Extent3d { width: ((super::SCREEN_WIDTH * scale * f) as u32).max(1), height: ((super::SCREEN_HEIGHT * scale * f) as u32).max(1), depth_or_array_layers: 1 };
        if images.get(&garage.target).is_some_and(|i| i.texture_descriptor.size != size) {
            if let Some(mut image) = images.get_mut(&garage.target) {
                image.resize(size);
            }
            crate::log_line(&format!("garage: room view {}x{}", size.width, size.height));
        }
    }

    // The room follows the page (a popup keeps the page's room behind it).
    let look = garage::room_for(&state.page.room);
    if look != garage.look {
        if let Some(root) = garage.root.take() {
            commands.entity(root).despawn();
        }
        garage.car = None;
        garage.car_root = None;
        garage.look = look;
        if let Some(look) = look {
            garage.root = Some(spawn_room(&look, &state.game_dir, &mut commands, &mut meshes, &mut materials, &mut images));
        }
    }

    if state.page.name != garage.layout_for {
        garage.layout_for = state.page.name.clone();
        let lay = layout_path(&state.game_dir, &state.page.name).and_then(|p| std::fs::read(p).ok());
        garage.car_box = lay.and_then(|b| garage::car_box(&nfspu_formats::fe::Lay::parse(&nfspu_formats::fe::decode_latin1(&b))));
        garage.doors = Default::default();
        garage.cabin = Default::default();
        crate::log_line(&format!("garage: page '{}' room {:?}, car box {:?}", state.page.name, look.map(|l| l.model), garage.car_box));
    }

    let want = if garage.root.is_some() && garage.car_box.is_some() { wanted_car(&state) } else { None };
    if want != garage.car {
        if let Some(e) = garage.car_root.take() {
            commands.entity(e).despawn();
        }
        garage.front_lights = Default::default();
        garage.rear_lights = Default::default();
        garage.doors = Default::default();
        if let (Some((model, paint, style)), Some(root)) = (&want, garage.root) {
            // The car's origin is at ground level, on the pit (the room's floor is at 0). No driver
            // in the showroom (retail frames).
            let holder = commands.spawn((Transform::default(), Visibility::default())).id();
            commands.entity(root).add_child(holder);
            // Levels 1 (the garage model) and 2 have the doors as separate parts; level 3 bakes them
            // into the body. `NFSPU_GARAGE_LOD` overrides for tests.
            let level = std::env::var("NFSPU_GARAGE_LOD").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(SHOW_CAR_LEVEL);
            let options = CarOptions { level, driver: 0, paint: *paint, style: *style };
            let started = std::time::Instant::now();
            spawn_show_car(&mut commands, &mut meshes, &mut materials, &mut images, holder, model, &options);
            crate::log_line(&format!("garage: car '{model}' paint section {paint} style {style} built in {:.0} ms", started.elapsed().as_secs_f32() * 1000.0));
            garage.car_root = Some(holder);
        }
        garage.car = want;
    }
}

fn spawn_room(look: &RoomLook, game_dir: &std::path::Path, commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, images: &mut Assets<Image>) -> Entity {
    let o = look.offset;
    let root = commands.spawn((Transform::from_xyz(o[0] * nfspu_game::MIRROR_X, o[1], o[2]), Visibility::default())).id();
    let started = std::time::Instant::now();
    let model = match garage::load_room(game_dir, look.model) {
        Ok(m) => m,
        Err(e) => {
            crate::log_line(&format!("garage: room '{}' not loaded: {e}", look.model));
            return root;
        }
    };
    let pages: Vec<Option<Handle<Image>>> = model
        .pages
        .iter()
        .map(|p| {
            p.as_ref().map(|p| {
                let mut image = Image::new(Extent3d { width: p.size, height: p.size, depth_or_array_layers: 1 }, TextureDimension::D2, p.rgba.clone(), TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
                image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                    address_mode_u: ImageAddressMode::Repeat,
                    address_mode_v: ImageAddressMode::Repeat,
                    mag_filter: ImageFilterMode::Linear,
                    min_filter: ImageFilterMode::Linear,
                    ..default()
                });
                images.add(image)
            })
        })
        .collect();
    let mut cache: std::collections::HashMap<(u32, bool), Handle<StandardMaterial>> = Default::default();
    for piece in &model.pieces {
        let page = model.pages.get(piece.page as usize).and_then(|p| p.as_ref());
        let material = cache
            .entry((piece.page, piece.translucent))
            .or_insert_with(|| {
                materials.add(StandardMaterial {
                    base_color: Color::WHITE,
                    base_color_texture: pages.get(piece.page as usize).cloned().flatten(),
                    unlit: true,
                    double_sided: true,
                    cull_mode: None,
                    alpha_mode: if piece.translucent {
                        AlphaMode::Blend
                    } else if page.is_some_and(|p| p.has_alpha) {
                        AlphaMode::Mask(0.5)
                    } else {
                        AlphaMode::Opaque
                    },
                    ..default()
                })
            })
            .clone();
        let colors: Vec<[f32; 4]> = piece.normals.iter().map(|n| {
            let l = if piece.self_lit { [1.0; 3] } else { garage::light_vertex(look, *n, ROOM_GAIN) };
            [l[0], l[1], l[2], 1.0]
        }).collect();
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, piece.positions.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, piece.normals.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, piece.uvs.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
        mesh.insert_indices(Indices::U32((0..piece.positions.len() as u32).collect()));
        let mut e = commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(material), Transform::from_translation(Vec3::from(piece.pivot))));
        if piece.slot != 0 {
            e.insert(RoomFan(piece.slot));
        }
        let e = e.id();
        commands.entity(root).add_child(e);
    }
    crate::log_line(&format!(
        "garage: room '{}' {} piece(s), {} triangles, {} page(s) [{}], built in {:.0} ms",
        look.model,
        model.pieces.len(),
        model.triangles,
        model.pages.iter().flatten().count(),
        model.pages.iter().map(|p| p.as_ref().map_or("-".to_string(), |p| format!("{}{}", p.size, if p.has_alpha { "a" } else { "" }))).collect::<Vec<_>>().join(" "),
        started.elapsed().as_secs_f32() * 1000.0
    ));
    root
}

/// Hover and clicks on the car: retail picks the part from the box and the turn
/// (`garage::zone_at`). Doors and lights toggle; a click on an open door climbs in, any click in
/// the cabin climbs out.
fn inspect(
    mut garage: ResMut<Garage>,
    state: Res<FrontEnd>,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let Ok(window) = windows.single() else { return };
    let mut p = pointer(window);
    // For tests without a mouse: `NFSPU_GARAGE_CLICKS=x,y@secs;...` clicks at layout points
    // once that many seconds of garage time have passed.
    let mut scripted = false;
    if let Ok(script) = std::env::var("NFSPU_GARAGE_CLICKS") {
        // the first click not made yet, once its time has come
        if let Some(step) = script.split(';').nth(garage.clicks_done) {
            let parsed = step.split_once('@').and_then(|(at, secs)| {
                let (x, y) = at.split_once(',')?;
                Some((x.trim().parse::<f32>().ok()?, y.trim().parse::<f32>().ok()?, secs.trim().parse::<f32>().ok()?))
            });
            if let Some((x, y, secs)) = parsed.filter(|s| garage.clock >= s.2) {
                p = Vec2::new(x, y);
                scripted = true;
                garage.clicks_done += 1;
                crate::log_line(&format!("garage: scripted click at {x},{y} after {secs} s (turn {:.3})", garage.turntable.turns.fract()));
            }
        }
    }
    let Some(rect) = garage.car_box.filter(|_| garage.car.is_some() && state.popup.is_none() && state.session.open_combo.is_none()) else {
        garage.hovering = false;
        return;
    };
    let inside = p.x >= rect[0] && p.x < rect[0] + rect[2] && p.y >= rect[1] && p.y < rect[1] + rect[3];
    // A menu control over the box takes the click (the car box sits under the bottom bar's logo
    // area on some pages).
    let free = inside && state.hit_at(p.x, p.y).is_none();
    if !scripted {
        garage.hovering = free;
    }
    if !free || !(scripted || buttons.just_pressed(MouseButton::Left)) {
        return;
    }
    if garage.cabin.on {
        garage.cabin.on = false;
        crate::log_line("garage: climb out");
        return;
    }
    // The roof band needs the roof animation, which is not built (cars are drawn as one style).
    let zone = garage::zone_at(p.x, p.y, rect, garage.turntable.turns as f32, false);
    match zone {
        Zone::Door(n) => {
            // Zone 2 is the side facing the camera a quarter turn on, the car's left (slot 6);
            // see the notes for why this differs from retail's own pairing.
            let slot = if n == 2 { 6 } else { 7 };
            let door = &mut garage.doors[slot - 6];
            if door.on && door.level >= 1.0 {
                garage.cabin.on = true;
                crate::log_line(&format!("garage: climb in through door zone {n} (slot {slot})"));
            } else {
                door.toggle();
                crate::log_line(&format!("garage: door zone {n} (slot {slot}) {}", if door.on { "opens" } else { "closes" }));
            }
        }
        Zone::End(4) => {
            garage.front_lights.toggle();
            crate::log_line(&format!("garage: headlights {}", if garage.front_lights.on { "on" } else { "off" }));
        }
        Zone::End(_) => {
            garage.rear_lights.toggle();
            crate::log_line(&format!("garage: rear lights {}", if garage.rear_lights.on { "on" } else { "off" }));
        }
        Zone::Roof => {}
    }
}

/// Turns the room and car, runs the fans, the lights and the cabin zoom.
#[allow(clippy::too_many_arguments)]
fn animate(
    mut garage: ResMut<Garage>,
    time: Res<Time>,
    mut transforms: Query<&mut Transform>,
    fans: Query<(Entity, &RoomFan)>,
    doors: Query<(Entity, &ShowDoor)>,
    lamps: Query<&ShowLamp>,
    new_lamps: Query<(), Added<ShowLamp>>,
    mut cabins: Query<(&ShowCabin, &mut Visibility)>,
    glares: Query<Entity, With<ShowGlare>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(look) = garage.look else { return };
    let dt = time.delta_secs().min(0.1);
    garage.clock += dt;
    // Pausing while the pointer is on the car matches a retail recording (the car held still for
    // four seconds under the glove); the code for it was not found (**guess**).
    let paused = garage.hovering || garage.cabin.level > 0.0;
    garage.turntable.advance(&look, dt, paused);
    let g = &mut *garage;
    g.cabin.advance(dt, garage::CABIN_BLEND_SECONDS);
    for d in &mut g.doors {
        d.advance(dt, DOOR_SECONDS);
    }
    let lights_moved = g.front_lights.advance(dt, LIGHT_SECONDS) | g.rear_lights.advance(dt, LIGHT_SECONDS);

    if let Some(root) = g.root {
        if let Ok(mut t) = transforms.get_mut(root) {
            let o = garage::staged_offset(&look, g.cabin.level);
            t.translation = Vec3::new(o[0] * nfspu_game::MIRROR_X, o[1], o[2]);
            t.rotation = Quat::from_rotation_y(g.turntable.yaw());
        }
    }
    for (e, fan) in &fans {
        if let Ok(mut t) = transforms.get_mut(e) {
            let rate = FAN_RATE.get(fan.0 as usize).copied().unwrap_or(0.0);
            t.rotation = Quat::from_rotation_z((g.clock * rate).fract() * std::f32::consts::TAU);
        }
    }
    for (e, door) in &doors {
        if let Ok(mut t) = transforms.get_mut(e) {
            // the left door (slot 6, at -x) swings out to -x, the right one to +x
            let level = g.doors[(door.0 - 6).min(1) as usize].level;
            let sign = if door.0 == 6 { -1.0 } else { 1.0 };
            t.rotation = Quat::from_rotation_y(sign * level * garage::DOOR_OPEN_TURNS * std::f32::consts::TAU);
        }
    }
    // Glares face the camera (undo the room's turn) and grow with the headlights.
    for e in &glares {
        if let Ok(mut t) = transforms.get_mut(e) {
            t.rotation = Quat::from_rotation_y(-g.turntable.yaw());
            t.scale = Vec3::splat(g.front_lights.level);
        }
    }
    // The detailed cabin only from inside.
    let inside = g.cabin.level > 0.5;
    for (part, mut vis) in &mut cabins {
        let want = if part.cockpit == inside { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
    }
    if lights_moved || !new_lamps.is_empty() {
        for lamp in &lamps {
            let Some(mut m) = materials.get_mut(&lamp.material) else { continue };
            // as in the race viewer: values above 1 brighten the unlit texture into a glow
            m.base_color = if lamp.front {
                let k = g.front_lights.level;
                Color::linear_rgb(1.0 + 1.6 * k, 1.0 + 1.6 * k, 1.0 + 1.0 * k)
            } else {
                let k = g.rear_lights.level;
                Color::linear_rgb(1.0 + 2.0 * k, 1.0 - 0.6 * k, 1.0 - 0.6 * k)
            };
        }
    }
}

/// Animation slot and hinge (Bevy frame) of each door article of a car file: slots 6 and 7, with
/// the hinge at the article's transform position.
type DoorSlots = std::collections::HashMap<u16, (u8, [f32; 3])>;

fn door_slots(name: &str, level: u8) -> DoorSlots {
    let mut out = DoorSlots::new();
    let Ok(path) = car_model::car_dir().map(|d| d.join(format!("{name}.crp"))) else { return out };
    let Ok(raw) = std::fs::read(path) else { return out };
    let data = if nfspu_formats::refpack::is_refpack(&raw) { nfspu_formats::refpack::decompress(&raw).unwrap_or_default() } else { raw };
    let Ok(c) = nfspu_formats::crp::parse(&data) else { return out };
    // Every part of a door shares the door's slot; the hinge is the transform farthest out to the
    // side (the outer panel's, at the door's front edge: x ±0.73, z -0.73 on the 356). Some door
    // parts carry a transform near the centre line, which is not the hinge.
    let mut hinge: [Option<[f32; 3]>; 2] = [None, None];
    let mut members = Vec::new();
    for a in 0..c.articles.len() {
        let slot = nfspu_formats::fe_room::animation_slot(&c, a);
        if slot != 6 && slot != 7 {
            continue;
        }
        members.push((a as u16, slot));
        if let Some(m) = c.article_transform(a, level).or_else(|| (1..=5).find_map(|l| c.article_transform(a, l))) {
            let h = &mut hinge[(slot - 6) as usize];
            if h.is_none_or(|p| m[12].abs() > p[0].abs()) {
                *h = Some([m[12] * nfspu_game::MIRROR_X, m[13], m[14]]);
            }
        }
    }
    for (a, slot) in members {
        if let Some(p) = hinge[(slot - 6) as usize] {
            out.insert(a, (slot, p));
        }
    }
    out
}

/// Splits a piece into the rest and each door's triangles (by the article of each corner); door
/// corners are made relative to their hinge.
fn split_doors(piece: &Piece, slots: &DoorSlots) -> (Piece, Vec<(u8, [f32; 3], Piece)>) {
    let mut rest = Piece::default();
    let mut doors: Vec<(u8, [f32; 3], Piece)> = Vec::new();
    for corner in 0..piece.positions.len() {
        let article = piece.articles.get(corner).copied().unwrap_or(u16::MAX);
        let mut p = piece.positions[corner];
        let target = match slots.get(&article) {
            Some(&(slot, pivot)) => {
                p = [p[0] - pivot[0], p[1] - pivot[1], p[2] - pivot[2]];
                let i = doors.iter().position(|d| d.0 == slot).unwrap_or_else(|| {
                    doors.push((slot, pivot, Piece::default()));
                    doors.len() - 1
                });
                &mut doors[i].2
            }
            None => &mut rest,
        };
        target.positions.push(p);
        target.normals.push(piece.normals[corner]);
        target.uvs.push(piece.uvs[corner]);
        target.materials.push(piece.materials.get(corner).copied().unwrap_or(0));
        target.articles.push(article);
    }
    (rest, doors)
}

/// What a show-car mesh is, for the components it gets.
#[derive(Clone)]
enum Role {
    Body,
    Cabin(bool),
    Lamp(bool, Handle<StandardMaterial>),
}

/// Builds the show car under `root` with the race viewer's pages and looks
/// (`crate::spawn_car_model`), but with the doors as separate hinged meshes and lamps that glow.
fn spawn_show_car(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, images: &mut Assets<Image>, root: Entity, name: &str, options: &CarOptions) {
    let m = match car_model::load_car(name, options) {
        Ok(m) => m,
        Err(e) => {
            crate::log_line(&format!("garage: car '{name}' not loaded: {e}"));
            return;
        }
    };
    let slots = door_slots(name, options.level);
    let white = materials.add(StandardMaterial { base_color: Color::WHITE, unlit: true, double_sided: true, cull_mode: None, ..default() });
    let paint_mat = crate::page_material(&m.exterior, false, images, materials);
    let trim_mat = crate::page_material(&m.extint, false, images, materials);
    let trim_solid_mat = trim_mat.as_ref().and_then(|h| materials.get(h).cloned()).map(|mut copy| {
        copy.alpha_mode = AlphaMode::Opaque;
        materials.add(copy)
    });
    let glass_mat = crate::page_material(&m.glass_page, true, images, materials);
    let cabrio_mat = crate::page_material(&m.cabrio_page, true, images, materials);
    // the cabin as in the race viewer: flat dark grey, only the faces that point into the cabin
    let cabin_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.16, 0.16, 0.17),
        unlit: true,
        double_sided: false,
        cull_mode: Some(if nfspu_game::MIRROR_X < 0.0 { bevy::render::render_resource::Face::Back } else { bevy::render::render_resource::Face::Front }),
        ..default()
    });

    let mut parts: Vec<(&Piece, [f32; 3], f32, Handle<StandardMaterial>, Role)> = Vec::new();
    for (piece, flat, mat, alpha) in [
        (&m.paint, [0.62, 0.02, 0.03], paint_mat.clone(), 1.0f32),
        (&m.trim, [0.08, 0.08, 0.09], trim_mat.clone(), 1.0),
        (&m.trim_solid, [0.08, 0.08, 0.09], trim_solid_mat.clone(), 1.0),
        (&m.glass, [0.03, 0.05, 0.09], glass_mat.clone(), 0.55),
        (&m.cabrio_glass, [0.03, 0.05, 0.09], cabrio_mat.clone(), 0.6),
    ] {
        let (tint, handle) = match mat {
            Some(h) => ([1.0, 1.0, 1.0], h),
            None => (flat, white.clone()),
        };
        parts.push((piece, tint, alpha, handle, Role::Body));
    }
    for (piece, cockpit) in [(&m.interior, true), (&m.interior_outside, false)] {
        parts.push((piece, [1.0, 1.0, 1.0], 1.0, cabin_mat.clone(), Role::Cabin(cockpit)));
    }
    for (piece, front) in [(&m.headlights, true), (&m.taillights, false)] {
        let (tint, handle) = match paint_mat.as_ref().and_then(|h| materials.get(h).cloned()) {
            Some(copy) => ([1.0, 1.0, 1.0], materials.add(copy)),
            None => ([0.9, 0.9, 0.9], white.clone()),
        };
        parts.push((piece, tint, 1.0, handle.clone(), Role::Lamp(front, handle)));
    }

    let mut hinges: std::collections::HashMap<u8, Entity> = Default::default();
    let mut door_triangles = 0;
    for (piece, tint, alpha, material, role) in parts {
        let (rest, doors) = split_doors(piece, &slots);
        let mut pieces = vec![(None, rest)];
        pieces.extend(doors.into_iter().map(|(slot, pivot, p)| (Some((slot, pivot)), p)));
        for (door, p) in pieces {
            if p.positions.is_empty() {
                continue;
            }
            let parent = match door {
                None => root,
                Some((slot, pivot)) => {
                    door_triangles += p.positions.len() / 3;
                    *hinges.entry(slot).or_insert_with(|| {
                        let h = commands.spawn((Transform::from_translation(Vec3::from(pivot)), Visibility::default(), ShowDoor(slot))).id();
                        commands.entity(root).add_child(h);
                        h
                    })
                }
            };
            let mut e = commands.spawn((Mesh3d(meshes.add(crate::piece_mesh(&p, tint, alpha))), MeshMaterial3d(material.clone()), Transform::default()));
            match &role {
                Role::Body => {}
                Role::Cabin(cockpit) => {
                    e.insert(ShowCabin { cockpit: *cockpit });
                }
                Role::Lamp(front, handle) => {
                    e.insert(ShowLamp { front: *front, material: handle.clone() });
                }
            }
            let e = e.id();
            commands.entity(parent).add_child(e);
        }
    }
    // one glare per headlight: the centre of the headlight triangles on each side, a little ahead
    let glare_mat = materials.add(StandardMaterial { base_color_texture: Some(images.add(glare_image())), unlit: true, alpha_mode: AlphaMode::Add, cull_mode: None, double_sided: true, ..default() });
    let glare_mesh = meshes.add(Rectangle::new(GLARE_SIZE, GLARE_SIZE));
    for side in [-1.0f32, 1.0] {
        let points: Vec<&[f32; 3]> = m.headlights.positions.iter().filter(|p| p[0] * side > 0.0).collect();
        if points.is_empty() {
            continue;
        }
        let n = points.len() as f32;
        let c = points.iter().fold([0.0f32; 3], |a, p| [a[0] + p[0] / n, a[1] + p[1] / n, a[2] + p[2] / n]);
        let e = commands
            .spawn((Mesh3d(glare_mesh.clone()), MeshMaterial3d(glare_mat.clone()), Transform::from_xyz(c[0], c[1], c[2] - 0.08).with_scale(Vec3::ZERO), ShowGlare))
            .id();
        commands.entity(root).add_child(e);
    }
    for w in &m.wheels {
        let (tint, handle) = match &trim_mat {
            Some(h) => ([1.0, 1.0, 1.0], h.clone()),
            None => ([0.10, 0.10, 0.11], white.clone()),
        };
        let e = commands.spawn((Mesh3d(meshes.add(crate::piece_mesh(&w.piece, tint, 1.0))), MeshMaterial3d(handle), Transform::from_translation(Vec3::from(w.offset)))).id();
        commands.entity(root).add_child(e);
    }
    let mut hinge_list: Vec<(u8, [f32; 3])> = slots.values().copied().collect();
    hinge_list.sort_by(|a, b| a.0.cmp(&b.0));
    hinge_list.dedup_by_key(|h| h.0);
    crate::log_line(&format!(
        "garage: car '{}' style '{}': {} triangles, size {:.2} x {:.2} x {:.2} m, doors {hinge_list:?} with {door_triangles} triangles",
        m.name,
        m.style_name,
        m.triangles(),
        m.size[0],
        m.size[1],
        m.size[2],
    ));
}
