//! The car body's two retail shine layers, drawn on the paint (`CarExtEnv`) pieces:
//!
//! * **Sun highlight.** The light direction mirrored about the surface normal, against the view
//!   direction, to the 8th power, times the sun colour and a detail level (1.0 at full detail),
//!   added to the colour.
//! * **Light glow.** Where the track's `.map` lists a reflection section for the current slice, a
//!   picture from the track's `.env` (blue or yellow lamp glows, tunnel lights) is laid flat over
//!   the ground around the slice's centre, `texturesize` metres across and `aspect` as deep, and
//!   looked up at each point of the body pushed out along its normal. The glow slides over the car
//!   as it passes the lamp.
//!
//! See `docs/provenance/car.md` ("Body reflections") for what is found and what is guessed.

use crate::weather_fx::{WeatherState, WorldCamera};
use bevy::asset::{uuid_handle, RenderAssetUsages};
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, Face, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;
use std::collections::HashMap;
use std::path::PathBuf;

const SHADER: Handle<Shader> = uuid_handle!("6c1f0b8e-3d1a-4f57-9b8e-2a6f0e5d9c41");

/// Power of the sun highlight (retail multiplies the cosine by itself eight times).
pub const SPECULAR_POWER: f32 = 8.0;
/// The highlight's strength at the three reflection detail levels (retail's table; we draw the top).
pub const SPECULAR_LEVELS: [f32; 3] = [0.2, 0.8, 1.0];
/// How far (metres) a body point is pushed along its normal before the glow lookup. **Guess**: the
/// engine reads it from a field we have not seen set.
pub const GLOW_NORMAL_PUSH: f32 = 0.5;
/// Sun highlight strength at night (**guess**: retail's night `.map` tints were not compared).
const NIGHT_SPECULAR: f32 = 0.35;

const SHADER_SOURCE: &str = r#"
#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::view

struct Shine {
    sun_dir: vec4<f32>,
    sun_colour: vec4<f32>,
    glow_centre: vec4<f32>,
    glow_scale: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> shine: Shine;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var base_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var base_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var glow_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var glow_sampler: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var c = textureSample(base_texture, base_sampler, in.uv);
#ifdef VERTEX_COLORS
    c = c * in.color;
#endif
    if (shine.glow_scale.w > 0.0 && c.a < 0.5) {
        discard;
    }
    let n = normalize(in.world_normal);
    let v = normalize(view.world_position - in.world_position.xyz);
    // the light mirrored about the normal, against the view
    let l = shine.sun_dir.xyz;
    let r = 2.0 * dot(n, l) * n - l;
    let s = max(dot(r, v), 0.0);
    var rgb = c.rgb + shine.sun_colour.rgb * pow(s, shine.sun_colour.w);
    if (shine.glow_scale.z > 0.0) {
        let p = in.world_position.xyz + n * shine.glow_centre.w - shine.glow_centre.xyz;
        let uv = vec2<f32>(p.x * shine.glow_scale.x, p.z * shine.glow_scale.y) + vec2<f32>(0.5, 0.5);
        let g = textureSample(glow_texture, glow_sampler, uv);
        rgb = rgb + g.rgb * g.a * shine.glow_scale.z;
    }
    return vec4<f32>(rgb, c.a);
}
"#;

#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct Shine {
    /// Towards the sun (world), unused w.
    pub sun_dir: Vec4,
    /// Highlight colour (linear) and power in w.
    pub sun_colour: Vec4,
    /// Glow centre (world) and the normal push in w.
    pub glow_centre: Vec4,
    /// 1 / size across, aspect / size along, glow on (1) or off (0), cut-outs on (1) or off (0).
    pub glow_scale: Vec4,
}

/// The paint pieces' material: the page texture times the baked vertex light, plus the shine.
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
pub struct CarPaint {
    #[uniform(0)]
    pub shine: Shine,
    #[texture(1)]
    #[sampler(2)]
    pub base: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    pub glow: Handle<Image>,
    /// Cut-outs where the page is transparent.
    pub cut: bool,
}

impl CarPaint {
    pub fn new(base: Handle<Image>, cut: bool) -> Self {
        let mut shine = default_shine();
        shine.glow_scale.w = if cut { 1.0 } else { 0.0 };
        CarPaint { shine, base, glow: Handle::default(), cut }
    }
}

impl Material for CarPaint {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(SHADER)
    }

    fn alpha_mode(&self) -> AlphaMode {
        if self.cut { AlphaMode::Mask(0.5) } else { AlphaMode::Opaque }
    }

    fn specialize(_: &MaterialPipeline, descriptor: &mut RenderPipelineDescriptor, _: &MeshVertexBufferLayoutRef, _: MaterialPipelineKey<Self>) -> Result<(), SpecializedMeshPipelineError> {
        // the file's front faces are clockwise; mirroring x (MIRROR_X) turns them anticlockwise
        descriptor.primitive.cull_mode = Some(if crate::MIRROR_X < 0.0 { Face::Back } else { Face::Front });
        Ok(())
    }
}

/// The sun as the car's baked shading has it (model space), turned into the world: up and to one
/// side. **Guess**: retail takes it from the track's light, which we do not read yet.
fn sun_dir() -> Vec3 {
    Vec3::new(0.35 * crate::world_mirror_x() / crate::MIRROR_X, 0.85, -0.4).normalize()
}

fn default_shine() -> Shine {
    let top = SPECULAR_LEVELS[2];
    Shine {
        sun_dir: sun_dir().extend(0.0),
        sun_colour: Vec4::new(top, top, top, SPECULAR_POWER),
        glow_centre: Vec4::new(0.0, 0.0, 0.0, GLOW_NORMAL_PUSH),
        glow_scale: Vec4::ZERO,
    }
}

/// The glow pictures of the loaded track's `.env`, by name.
#[derive(Resource, Default)]
struct GlowPictures {
    track: String,
    images: HashMap<String, Handle<Image>>,
    logged: Option<usize>,
}

pub struct CarShinePlugin;

impl Plugin for CarShinePlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().resource_mut::<Assets<Shader>>().insert(SHADER.id(), Shader::from_wgsl(SHADER_SOURCE, "nfspu://car_shine.wgsl")).ok();
        app.add_plugins(MaterialPlugin::<CarPaint>::default()).init_resource::<GlowPictures>().add_systems(PostUpdate, update_shine);
    }
}

/// Loads every picture of `<game>/GameData/Track/<track>.env` as a clamped image.
fn load_env(track: &str, images: &mut Assets<Image>) -> HashMap<String, Handle<Image>> {
    let game = PathBuf::from(std::env::var("NFSPU_GAME_DIR").unwrap_or_default());
    let Ok(blob) = std::fs::read(game.join("GameData").join("Track").join(format!("{track}.env"))) else { return HashMap::new() };
    let Ok(archive) = nfspu_formats::shpi::parse(&blob) else { return HashMap::new() };
    let mut out = HashMap::new();
    for e in &archive.entries {
        let Some(rgba) = nfspu_formats::shpi::decode_rgba8(&blob, e) else { continue };
        let mut image = Image::new(Extent3d { width: e.width as u32, height: e.height as u32, depth_or_array_layers: 1 }, TextureDimension::D2, rgba, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
        image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::ClampToEdge,
            address_mode_v: ImageAddressMode::ClampToEdge,
            mag_filter: ImageFilterMode::Linear,
            min_filter: ImageFilterMode::Linear,
            ..default()
        });
        out.insert(e.name.to_ascii_uppercase(), images.add(image));
    }
    out
}

/// The reflection section of a slice: the last one whose slice range holds it.
pub fn section_for(sections: &[nfspu_formats::weather::ReflectionSection], slice: i32) -> Option<&nfspu_formats::weather::ReflectionSection> {
    sections.iter().rev().find(|s| s.start_slice <= slice && slice <= s.end_slice)
}

/// Sets every paint material's sun and glow for this frame. The glow follows the slice each car
/// is on (its paint meshes share one material per car). Without a track (garage) only the sun
/// highlight is drawn.
fn update_shine(
    state: Option<Res<WeatherState>>,
    mut pictures: ResMut<GlowPictures>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<CarPaint>>,
    paints: Query<(&MeshMaterial3d<CarPaint>, &GlobalTransform)>,
    cams: Query<(), With<WorldCamera>>,
) {
    if materials.is_empty() {
        return;
    }
    // `NFSPU_SHINE=<gain>` scales the shine for tests; 0 draws the paint without it (before/after)
    let gain = std::env::var("NFSPU_SHINE").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.0);
    let off = gain <= 0.0;
    let mut shine = default_shine();
    let track = state.as_ref().filter(|_| !cams.is_empty());
    if let Some(state) = track {
        let t = &state.track.lighting.tint;
        let night = if state.night() { NIGHT_SPECULAR } else { 1.0 };
        let top = SPECULAR_LEVELS[2] * night;
        shine.sun_colour = Vec4::new(t[1] * top, t[2] * top, t[3] * top, SPECULAR_POWER);
        if pictures.track != state.track.track {
            pictures.track = state.track.track.clone();
            pictures.images = load_env(&state.track.track, &mut images);
            pictures.logged = None;
            crate::weather_fx::crate_log(&format!("car shine: {} glow pictures, {} reflection sections", pictures.images.len(), state.track.lighting.sections.len()));
        }
    }
    shine.sun_colour = (shine.sun_colour.truncate() * gain.max(0.0)).extend(SPECULAR_POWER);
    let mut seen = std::collections::HashSet::new();
    for (handle, at) in &paints {
        if !seen.insert(handle.0.id()) {
            continue;
        }
        let mut s = shine;
        let mut glow = None;
        if let (Some(state), false) = (track, off) {
            if let Some((slice, centre)) = state.slice_near(at.translation()) {
                // the open-air picture of the top detail level ("covered" is not known yet)
                let section = section_for(&state.track.lighting.sections, slice as i32);
                if let Some((sec, h)) = section.and_then(|sec| pictures.images.get(&sec.textures[2].0.to_ascii_uppercase()).map(|h| (sec, h.clone()))) {
                    let size = sec.texture_size.max(0.1);
                    s.glow_centre = centre.extend(GLOW_NORMAL_PUSH);
                    s.glow_scale = Vec4::new(1.0 / size, sec.aspect / size, gain, 0.0);
                    glow = Some(h);
                    if pictures.logged != Some(slice) {
                        pictures.logged = Some(slice);
                        crate::weather_fx::crate_log(&format!("car glow: slice {slice}, picture {} ({:.2} m, aspect {:.2})", sec.textures[2].0, sec.texture_size, sec.aspect));
                    }
                }
            }
        }
        let Some(m) = materials.get(&handle.0) else { continue };
        s.glow_scale.w = m.shine.glow_scale.w;
        let changed = m.shine.sun_colour != s.sun_colour || m.shine.glow_centre != s.glow_centre || m.shine.glow_scale != s.glow_scale || m.shine.sun_dir != s.sun_dir;
        let new_glow = glow.filter(|g| *g != m.glow);
        if changed || new_glow.is_some() {
            let Some(mut m) = materials.get_mut(&handle.0) else { continue };
            m.shine = s;
            if let Some(g) = new_glow {
                m.glow = g;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nfspu_formats::weather::ReflectionSection;

    fn sec(a: i32, b: i32, name: &str) -> ReflectionSection {
        let t = (name.to_string(), name.to_string());
        ReflectionSection { start_slice: a, end_slice: b, textures: [t.clone(), t.clone(), t], texture_size: 5.0, aspect: 0.2 }
    }

    #[test]
    fn a_slice_finds_its_reflection_section() {
        let s = [sec(0, 0, "1"), sec(414, 414, "BT1"), sec(417, 420, "BT2")];
        assert_eq!(section_for(&s, 414).map(|s| s.textures[2].0.as_str()), Some("BT1"));
        assert_eq!(section_for(&s, 419).map(|s| s.textures[2].0.as_str()), Some("BT2"));
        assert!(section_for(&s, 415).is_none());
    }
}
