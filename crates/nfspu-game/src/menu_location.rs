//! The location screen's 3D track room (`room=ID_TRACK`), drawn by `menu_garage`'s render-to-texture
//! camera. Retail's `FETrackSelect3DRoom` (read-only reference, in our own words; notes in
//! `docs/provenance/fe.md`):
//! - the room model `track.crp` (floor, vent, door) with four copies of `trackwall.crp` around it,
//!   a quarter turn apart;
//! - the wall holds 12 horizontal panels (animation slots 20-31). Each panel is a three-sided
//!   prism whose faces use three 256x192 picture pages, so the wall turns like a hoarding to show
//!   the next picture;
//! - choosing another track draws the new track's picture onto the face that is about to come up
//!   and turns the prism a third of a turn in half a second, and the whole room turns a quarter
//!   (clockwise when stepping on, counter-clockwise when stepping back). Left alone, the panels
//!   step to the next picture every four seconds;
//! - the camera flies in over 300 ticks of the 128 Hz clock from outside the near wall to the
//!   middle of the room, levelling out from a small tilt.

use super::menu_garage::{spawn_room, Garage, GarageCamera};
use super::FrontEnd;
use bevy::prelude::*;
use nfspu_game::garage;

/// Seconds for the room to turn a quarter (retail: 0.0028571 of a turn per tick at 128 Hz).
const QUARTER_SECONDS: f32 = 0.25 / (0.002_857_142_8 * 128.0);
/// Seconds for the panels to step to the next face (64 ticks).
const STEP_SECONDS: f32 = 64.0 / 128.0;
/// Seconds a picture stays before the wall steps on by itself (512 ticks).
const IDLE_SECONDS: f32 = 512.0 / 128.0;
/// First animation slot of the panels; there are 12.
const FIRST_SLOT: u8 = 20;
/// Size of a picture page on the wall and the picture area inside it.
const PAGE: usize = 256;
const PICTURE_HEIGHT: usize = 192;

/// One panel of a wall (all panels turn together).
#[derive(Component)]
pub(super) struct Panel;

/// A picture that can be drawn onto a face page: RGBA8, width, height.
type Picture = (usize, usize, Vec<u8>);

/// A small overlay (the track's flag) drawn on the first picture: RGBA8 size and its position.
struct Flag {
    w: usize,
    h: usize,
    x: usize,
    y: usize,
    rgba: Vec<u8>,
}

#[derive(Default)]
enum Build {
    #[default]
    Fresh,
    /// The first wall is spawned; its parts are copied into the other three next frame.
    Cloning(Entity),
    Done,
}

#[derive(Default)]
pub(super) struct TrackRoom {
    root: Option<Entity>,
    build: Build,
    /// Seconds since the room was built (the fly-in clock).
    clock: f32,
    /// Face images in turning order and the panel angle that shows the first one.
    faces: Vec<Handle<Image>>,
    base_angle: f32,
    /// Faces stepped so far, and the progress of a step in progress (0..1).
    step: usize,
    stepping: Option<f32>,
    idle: f32,
    /// The track shown, its pictures and the next one to draw.
    track: Option<usize>,
    pictures: Vec<Picture>,
    /// The two flags of the art file (forward, backward) and the direction shown.
    flags: Vec<Flag>,
    direction: usize,
    next_picture: usize,
    /// The room's turn in whole turns (clockwise positive) and where it is heading.
    turns: f32,
    target_turns: f32,
}

/// Reads every picture of a track's art file (`FEData/TrackArt/<name>.fsh`).
fn load_pictures(game_dir: &std::path::Path, file: &str) -> (Vec<Picture>, Vec<Flag>) {
    let Ok(data) = nfspu_game::menu::load_art(game_dir, file) else { return (Vec::new(), Vec::new()) };
    let Ok(archive) = nfspu_formats::shpi::parse(&data) else { return (Vec::new(), Vec::new()) };
    let pictures = archive
        .entries
        .iter()
        .filter(|e| e.width as usize == PAGE && e.height as usize == PICTURE_HEIGHT)
        .filter_map(|e| nfspu_formats::shpi::decode_rgba8(&data, e).map(|rgba| (e.width as usize, e.height as usize, rgba)))
        .collect();
    // the entries after the pictures are the flags; header offsets 8 and 10 hold where each sits
    let flags = archive
        .entries
        .iter()
        .filter(|e| (e.width as usize) < PAGE / 2)
        .filter_map(|e| {
            let rgba = nfspu_formats::shpi::decode_rgba8(&data, e)?;
            let at = |o: usize| data.get(e.offset + o..e.offset + o + 2).map_or(0, |b| i16::from_le_bytes([b[0], b[1]]).max(0) as usize);
            Some(Flag { w: e.width as usize, h: e.height as usize, x: at(8), y: at(10), rgba })
        })
        .collect();
    (pictures, flags)
}

/// Draws a picture at the top left of a face page, on black.
/// With `flag`, its overlay goes on top; `mirrored` flips the whole picture (retail mirrors both).
fn draw_picture(images: &mut Assets<Image>, face: &Handle<Image>, picture: &Picture, flag: Option<&Flag>, mirrored: bool) {
    let Some(mut image) = images.get_mut(face) else { return };
    let size = image.texture_descriptor.size.width as usize;
    let Some(data) = image.data.as_mut() else { return };
    for px in data.chunks_exact_mut(4) {
        px.copy_from_slice(&[0, 0, 0, 255]);
    }
    let (w, h, rgba) = picture;
    for y in 0..(*h).min(size) {
        for x in 0..(*w).min(size) {
            let s = (y * w + x) * 4;
            let d = (y * size + x) * 4;
            data[d..d + 3].copy_from_slice(&rgba[s..s + 3]);
        }
    }
    if mirrored {
        for y in 0..(*h).min(size) {
            let row = &mut data[y * size * 4..(y * size + *w) * 4];
            for x in 0..*w / 2 {
                for k in 0..4 {
                    row.swap(x * 4 + k, (*w - 1 - x) * 4 + k);
                }
            }
        }
    }
    if let Some(f) = flag {
        for y in 0..f.h {
            for x in 0..f.w {
                let s = (y * f.w + x) * 4;
                let a = f.rgba[s + 3] as u32;
                // a flag lands where the file says; mirrored, it sits the same distance from the other edge
                let px = if mirrored { w.saturating_sub(f.x + f.w) + x } else { f.x + x };
                let py = f.y + y;
                if px >= *w || py >= *h || px >= size || py >= size {
                    continue;
                }
                let d = (py * size + px) * 4;
                for k in 0..3 {
                    data[d + k] = ((f.rgba[s + k] as u32 * a + data[d + k] as u32 * (255 - a)) / 255) as u8;
                }
            }
        }
    }
}

/// Builds the room's extras, follows the chosen track and flies the camera in. Runs after the
/// garage's own `animate`, which turns and places the room root; the track room takes over that
/// transform.
#[allow(clippy::too_many_arguments)]
pub(super) fn track_room(
    mut room: Local<TrackRoom>,
    garage: Res<Garage>,
    state: Res<FrontEnd>,
    time: Res<Time>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    children: Query<&Children>,
    parts: Query<(&Mesh3d, &MeshMaterial3d<StandardMaterial>, Option<&super::menu_garage::RoomFan>)>,
    mut transforms: Query<&mut Transform>,
    panels: Query<Entity, With<Panel>>,
    mut cameras: Query<&mut Projection, With<GarageCamera>>,
) {
    // the camera's view angle follows the room on show
    if let (Some(look), Ok(mut projection)) = (garage.look, cameras.single_mut()) {
        if let Projection::Perspective(p) = &mut *projection {
            let fov = garage::vertical_fov(&look, 4.0 / 3.0);
            if (p.fov - fov).abs() > 1e-6 {
                p.fov = fov;
            }
        }
    }
    let on = garage.look.is_some_and(|l| l.model == garage::TRACK.model);
    let Some(root) = garage.root.filter(|_| on) else {
        *room = TrackRoom::default();
        return;
    };
    let dt = time.delta_secs().min(0.1);
    if room.root != Some(root) {
        *room = TrackRoom { root: Some(root), ..default() };
    }

    match room.build {
        Build::Fresh => {
            let wall = spawn_room(&garage::TRACK_WALL, &state.game_dir, &mut commands, &mut meshes, &mut materials, &mut images);
            commands.entity(root).add_child(wall);
            commands.entity(wall).insert(Transform::default());
            room.build = Build::Cloning(wall);
            return;
        }
        Build::Cloning(wall) => {
            let Ok(kids) = children.get(wall) else { return };
            let mut copies = Vec::new();
            let mut faces: Vec<(Handle<Image>, f32)> = Vec::new();
            let mut walls_materials = Vec::new();
            for kid in kids.iter() {
                let (Ok((mesh, material, fan)), Ok(transform)) = (parts.get(kid), transforms.get(kid)) else { continue };
                let is_panel = fan.is_some_and(|f| f.0 >= FIRST_SLOT);
                copies.push((mesh.0.clone(), material.0.clone(), *transform, is_panel));
                if is_panel {
                    commands.entity(kid).insert(Panel);
                    // the face is the page the part samples; its angle is where its middle sits
                    // around the panel's long axis, seen from the side (y up, z toward the far wall)
                    if fan.is_some_and(|f| f.0 == FIRST_SLOT) {
                        let image = materials.get(&material.0).and_then(|m| m.base_color_texture.clone());
                        let positions = meshes.get(&mesh.0).and_then(|m| m.attribute(Mesh::ATTRIBUTE_POSITION)).and_then(|a| a.as_float3().map(|p| p.to_vec()));
                        if let (Some(image), Some(p)) = (image, positions) {
                            let n = p.len().max(1) as f32;
                            let (cy, cz) = (p.iter().map(|v| v[1]).sum::<f32>() / n, p.iter().map(|v| v[2]).sum::<f32>() / n);
                            faces.push((image, cy.atan2(cz)));
                        }
                    }
                } else {
                    walls_materials.push(material.0.clone());
                }
            }
            // The wall's back faces are not drawn: the near wall seen from outside at the start of
            // the fly-in must not cover the view (**guess**: retail culls them).
            for m in walls_materials {
                if let Some(mut m) = materials.get_mut(&m) {
                    m.cull_mode = Some(bevy::render::render_resource::Face::Back);
                    m.double_sided = false;
                }
            }
            // the other three walls, a quarter turn apart
            for k in 1..4 {
                let side = commands.spawn((Transform::from_rotation(Quat::from_rotation_y(-(k as f32) * 0.25 * std::f32::consts::TAU)), Visibility::default())).id();
                commands.entity(root).add_child(side);
                for (mesh, material, transform, is_panel) in &copies {
                    let mut part = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), *transform));
                    if *is_panel {
                        part.insert(Panel);
                    }
                    let part = part.id();
                    commands.entity(side).add_child(part);
                }
            }
            // faces in turning order: the one that fronts at angle phi, then those 120 and 240
            // degrees on
            let tau = std::f32::consts::TAU;
            if let Some(first) = faces.first().map(|f| f.1) {
                let phi0 = first - std::f32::consts::PI;
                faces.sort_by(|a, b| {
                    let (da, db) = ((a.1 - first).rem_euclid(tau), (b.1 - first).rem_euclid(tau));
                    da.total_cmp(&db)
                });
                room.base_angle = phi0;
            }
            // the faces list started with whichever part came first; turning order starts there
            room.faces = faces.into_iter().map(|f| f.0).collect();
            nfspu_game_log(&format!("location: track room built, {} face(s), base angle {:.1} deg", room.faces.len(), room.base_angle.to_degrees()));
            room.build = Build::Done;
        }
        Build::Done => {}
    }
    room.clock += dt;

    // the chosen track: new pictures, a quarter turn of the room and a step of the panels
    let chosen = state.quick.chosen_track().map(|t| (state.quick.track, if t.art[1].is_empty() { t.art[0].clone() } else { t.art[1].clone() }));
    if let Some((index, art)) = chosen {
        if room.track != Some(index) {
            let first = room.track.is_none();
            let forward = room.track.is_none_or(|old| index >= old);
            (room.pictures, room.flags) = load_pictures(&state.game_dir, &format!("TrackArt/{art}.fsh"));
            room.next_picture = 0;
            room.direction = state.quick.direction;
            nfspu_game_log(&format!("location: track {index} '{art}' {} picture(s), {} flag(s) {:?}", room.pictures.len(), room.flags.len(), room.pictures.iter().map(|p| (p.0, p.1)).collect::<Vec<_>>()));
            if first {
                if let (Some(face), Some(pic)) = (room.faces.first(), room.pictures.first()) {
                    draw_picture(&mut images, face, pic, room.flags.get(usize::from(room.direction % 2 == 1)), room.direction >= 2);
                    room.next_picture = 1;
                }
            } else {
                room.target_turns += if forward { 0.25 } else { -0.25 };
                finish_step(&mut room);
                begin_step(&mut room, &mut images);
            }
            room.track = Some(index);
        }
    }

    // stepping the panels
    if let Some(progress) = room.stepping {
        let progress = progress + dt / STEP_SECONDS;
        if progress >= 1.0 {
            finish_step(&mut room);
        } else {
            room.stepping = Some(progress);
        }
    } else {
        room.idle += dt;
        if room.idle >= IDLE_SECONDS && room.pictures.len() > 1 {
            begin_step(&mut room, &mut images);
        }
    }
    let faces = room.faces.len().max(1);
    let s = (room.step % faces) as f32 + room.stepping.unwrap_or(0.0);
    let angle = room.base_angle + s * std::f32::consts::TAU / 3.0;
    for e in &panels {
        if let Ok(mut t) = transforms.get_mut(e) {
            t.rotation = Quat::from_rotation_x(angle);
        }
    }

    // the room's turn
    let step = dt / QUARTER_SECONDS * 0.25;
    let delta = room.target_turns - room.turns;
    room.turns += delta.clamp(-step, step);
    // the fly-in
    let f = (room.clock / garage::TRACK_FLY_SECONDS).clamp(0.0, 1.0);
    let (a, b) = (garage::TRACK.offset, garage::TRACK_END);
    let at = |i: usize| a[i] + (b[i] - a[i]) * f;
    let tilt = garage::TRACK_TILT[0] + (garage::TRACK_TILT[1] - garage::TRACK_TILT[0]) * f;
    if let Ok(mut t) = transforms.get_mut(root) {
        t.translation = Vec3::new(at(0) * nfspu_game::MIRROR_X, at(1), at(2));
        t.rotation = Quat::from_rotation_x(tilt * std::f32::consts::TAU) * Quat::from_rotation_y(-room.turns * std::f32::consts::TAU);
    }
}

/// Completes the step in progress (the face that was coming up is now in front).
fn finish_step(room: &mut TrackRoom) {
    if room.stepping.take().is_some() {
        room.step += 1;
    }
    room.idle = 0.0;
}

/// Draws the next picture on the face that comes up next and starts turning to it.
fn begin_step(room: &mut TrackRoom, images: &mut Assets<Image>) {
    if room.faces.is_empty() || room.pictures.is_empty() {
        return;
    }
    let face = (room.step + 1) % room.faces.len();
    let pic = room.next_picture % room.pictures.len();
    let flag = (pic == 0).then(|| room.flags.get(usize::from(room.direction % 2 == 1))).flatten();
    draw_picture(images, &room.faces[face], &room.pictures[pic], flag, room.direction >= 2);
    room.next_picture = pic + 1;
    room.stepping = Some(0.0);
    room.idle = 0.0;
}

fn nfspu_game_log(line: &str) {
    crate::log_line(line);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(images: &mut Assets<Image>) -> Handle<Image> {
        images.add(Image::new_fill(bevy::render::render_resource::Extent3d { width: 256, height: 256, depth_or_array_layers: 1 }, bevy::render::render_resource::TextureDimension::D2, &[0, 0, 0, 255], bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb, bevy::asset::RenderAssetUsages::default()))
    }

    fn pixel(images: &Assets<Image>, h: &Handle<Image>, x: usize, y: usize) -> [u8; 3] {
        let d = images.get(h).unwrap().data.as_ref().unwrap();
        let i = (y * 256 + x) * 4;
        [d[i], d[i + 1], d[i + 2]]
    }

    #[test]
    fn the_flag_lands_where_the_file_says_and_mirrors_to_the_other_side() {
        let mut images = Assets::<Image>::default();
        let h = face(&mut images);
        let pic: Picture = (256, 192, vec![10; 256 * 192 * 4]);
        let flag = Flag { w: 28, h: 16, x: 134, y: 65, rgba: [200, 100, 50, 255].repeat(28 * 16) };
        draw_picture(&mut images, &h, &pic, Some(&flag), false);
        assert_eq!(pixel(&images, &h, 140, 70), [200, 100, 50]);
        assert_eq!(pixel(&images, &h, 100, 70), [10, 10, 10]);
        draw_picture(&mut images, &h, &pic, Some(&flag), true);
        assert_eq!(pixel(&images, &h, 140, 70), [10, 10, 10]);
        assert_eq!(pixel(&images, &h, 100, 70), [200, 100, 50]);
    }
}
