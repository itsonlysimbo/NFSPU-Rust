//! The 3D rooms behind the front end: the garage (`room=ID_GARAGE`) with the chosen car turning in
//! it, which the player can inspect by clicking. Window-free logic only; the Bevy scene is in the
//! viewer (`menu_garage.rs`). Evidence for every number is in `docs/provenance/garage.md`.
//!
//! How retail stages it: the camera never moves. The room model and the car are placed together
//! in front of it (`offset`) and turned about the vertical axis, one turn per `spin_seconds`, so
//! the room seems to orbit with the car. Clicking the car's box (`NFS5.3DCAR` on the layout) works
//! on the part under the pointer: the roof, a door, the front or the rear, chosen from where the
//! pointer is in the box and how far the car has turned.

use crate::MIRROR_X;
use nfspu_formats::{crp, fe_room, refpack, shpi};
use std::path::Path;

/// The garage's staging values as retail sets them for its garage room.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoomLook {
    /// Model file stem in `FEData/Models`.
    pub model: &'static str,
    /// Where the room (and the car at its centre) sits relative to the camera, in game units
    /// (x right, y up, z forward).
    pub offset: [f32; 3],
    /// Seconds per turn; 0 = still.
    pub spin_seconds: f32,
    /// The view angle setting, read as half the horizontal field of view in degrees (fitted to
    /// a retail frame, see the notes).
    pub view_angle: f32,
    /// Ambient light (0-255 per channel).
    pub ambient: [f32; 3],
    /// The room light's colour (0-255) and direction (pointing from the light).
    pub light: [f32; 3],
    pub light_dir: [f32; 3],
}

/// The garage (`FEGarage3DRoom` in the executable).
pub const GARAGE: RoomLook = RoomLook {
    model: "garage",
    offset: [0.0, -1.1, 4.8],
    spin_seconds: 30.0,
    view_angle: 37.5,
    ambient: [55.0, 65.0, 55.0],
    light: [120.0, 120.0, 120.0],
    light_dir: [-0.14, -0.98, 0.114],
};

/// Where the room moves to when the player climbs into the car (blend 1): the camera ends up in
/// the cabin. Retail blends there and back over 64 ticks of its 128 Hz clock.
pub const CABIN_OFFSET: [f32; 3] = [0.0, -0.8, 0.3];
pub const CABIN_BLEND_SECONDS: f32 = 64.0 / 128.0;

/// Door opening: a door turns up to 0.15 of a turn (54 degrees) over 255 steps.
pub const DOOR_OPEN_TURNS: f32 = 0.15;

/// The room a layout's `room=` id asks for (`None` for no room or one not drawn yet).
pub fn room_for(id: &str) -> Option<RoomLook> {
    match id.trim().to_ascii_uppercase().as_str() {
        "ID_GARAGE" => Some(GARAGE),
        _ => None,
    }
}

/// The room offset with the cabin blend applied (0 = normal, 1 = inside the car).
pub fn staged_offset(look: &RoomLook, blend: f32) -> [f32; 3] {
    let b = blend.clamp(0.0, 1.0);
    let o = look.offset;
    [(1.0 - b) * o[0] + b * CABIN_OFFSET[0], (1.0 - b) * o[1] + b * CABIN_OFFSET[1], (1.0 - b) * o[2] + b * CABIN_OFFSET[2]]
}

/// Vertical field of view in radians for a viewport of `aspect` (width / height), from the room's
/// view angle read as half the horizontal angle.
pub fn vertical_fov(look: &RoomLook, aspect: f32) -> f32 {
    let half_h = look.view_angle.to_radians();
    2.0 * (half_h.tan() / aspect.max(0.01)).atan()
}

/// The part of the car a click works on, numbered as retail numbers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    /// 0: the roof (only on cars whose roof can move).
    Roof,
    /// 1: the right-hand door, 2: the left-hand door.
    Door(u8),
    /// 3: the rear, 4: the front (the lights there).
    End(u8),
}

/// Which part of the car is under `(x, y)` (layout units) in the car's box `[x, y, w, h]`, with
/// the car turned `turns` (whole turns count from facing the camera). Retail's rule: the top 5/16
/// of the box is the roof when the roof can move; otherwise the turn decides. Near 0 (front to the
/// camera) and 0.5 (rear) the whole box is that end; in the quarters between, the middle is a door
/// and the outer sixths the ends.
pub fn zone_at(x: f32, y: f32, rect: [f32; 4], turns: f32, movable_roof: bool) -> Zone {
    let [left, top, w, h] = rect;
    if movable_roof && y <= top + h * 5.0 / 16.0 {
        return Zone::Roof;
    }
    let f = turns - turns.floor();
    let sixth = w / 6.0;
    let (in_left, in_right) = (x < left + sixth, x > left + w - sixth);
    if f > 0.9 || f <= 0.1 {
        Zone::End(4)
    } else if f <= 0.4 {
        if in_left {
            Zone::End(4)
        } else if in_right {
            Zone::End(3)
        } else {
            Zone::Door(2)
        }
    } else if f <= 0.6 {
        Zone::End(3)
    } else if in_left {
        Zone::End(3)
    } else if in_right {
        Zone::End(4)
    } else {
        Zone::Door(1)
    }
}

/// The turntable clock: whole turns since the screen opened. Retail derives the angle from its
/// clock directly; this keeps the angle where it was when the turning pauses (see `paused`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Turntable {
    pub turns: f64,
}

impl Turntable {
    /// Moves on by `seconds` unless paused.
    pub fn advance(&mut self, look: &RoomLook, seconds: f32, paused: bool) {
        if !paused && look.spin_seconds > 0.0 {
            self.turns += seconds as f64 / look.spin_seconds as f64;
        }
    }

    /// The yaw for Bevy (radians about +Y): retail turns clockwise seen from above.
    pub fn yaw(&self) -> f32 {
        -(self.turns.fract() * std::f64::consts::TAU) as f32
    }
}

/// A 0..1 animation level that runs toward on or off at a fixed rate (doors, lights, the cabin
/// zoom).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Fade {
    pub level: f32,
    pub on: bool,
}

impl Fade {
    pub fn toggle(&mut self) {
        self.on = !self.on;
    }

    /// Moves toward the target taking `seconds_full` for the whole way; true while moving.
    pub fn advance(&mut self, seconds: f32, seconds_full: f32) -> bool {
        let target = if self.on { 1.0 } else { 0.0 };
        let step = if seconds_full > 0.0 { seconds / seconds_full } else { 1.0 };
        let before = self.level;
        self.level = if self.on { (self.level + step).min(target) } else { (self.level - step).max(target) };
        self.level != before
    }
}

/// Retail's light rule for the room (fixed-function style): ambient plus the room light on faces
/// that face it. Returns a linear multiplier per channel; `gain` scales the result (fitted).
pub fn light_vertex(look: &RoomLook, normal: [f32; 3], gain: f32) -> [f32; 3] {
    let d = look.light_dir;
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-6);
    let lambert = (-(normal[0] * d[0] + normal[1] * d[1] + normal[2] * d[2]) / len).max(0.0);
    let mut out = [0.0; 3];
    for c in 0..3 {
        out[c] = (look.ambient[c] + look.light[c] * lambert) / 255.0 * gain;
    }
    out
}

/// One drawable piece of a room: triangles (three corners each) that share a texture page.
#[derive(Debug, Clone, Default)]
pub struct RoomPiece {
    /// Article name, e.g. `Floor`, `Fan01`.
    pub name: String,
    /// Positions are in the room's frame (game units, x already mirrored for Bevy); for an
    /// animated piece they are relative to `pivot`.
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// Embedded archive the piece samples.
    pub page: u32,
    pub translucent: bool,
    /// Drawn at full brightness instead of lit (the ceiling light rods).
    pub self_lit: bool,
    /// Animation slot (0 = static; the garage's fans are 1 and 2).
    pub slot: u8,
    pub pivot: [f32; 3],
}

/// A texture page assembled from one embedded archive.
#[derive(Debug, Clone)]
pub struct RoomPage {
    pub size: u32,
    pub rgba: Vec<u8>,
    pub has_alpha: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RoomModel {
    pub pieces: Vec<RoomPiece>,
    /// By embedded archive index.
    pub pages: Vec<Option<RoomPage>>,
    pub triangles: usize,
}

/// Loads `FEData/Models/<name>.crp` with its `.tpg`. Level of detail 1 (the only one rooms have).
pub fn load_room(game_dir: &Path, name: &str) -> Result<RoomModel, String> {
    let dir = game_dir.join("FEData").join("Models");
    let path = dir.join(format!("{name}.crp"));
    let raw = std::fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let data = if refpack::is_refpack(&raw) { refpack::decompress(&raw).map_err(|e| format!("{}: {e}", path.display()))? } else { raw };
    let c = crp::parse(&data).map_err(|e| format!("{}: {e}", path.display()))?;
    let tpg = fe_room::parse_tpg(&std::fs::read_to_string(dir.join(format!("{name}.tpg"))).unwrap_or_default());
    let materials: std::collections::HashMap<u16, fe_room::RoomMaterial> = fe_room::materials(&c).into_iter().map(|m| (m.index, m)).collect();

    let mut model = RoomModel::default();
    for (index, archive) in c.embedded_archives() {
        let index = index as usize;
        if model.pages.len() <= index {
            model.pages.resize(index + 1, None);
        }
        let Ok(parsed) = shpi::parse(archive) else { continue };
        let info = tpg.get(index);
        let size = info.map(|p| p.width.max(p.height)).filter(|s| *s > 0).unwrap_or(256);
        let rgba = shpi::compose_page(archive, &parsed, size);
        let has_alpha = info.is_some_and(|p| p.has_alpha()) || rgba.chunks_exact(4).any(|p| p[3] < 255);
        model.pages[index] = Some(RoomPage { size, rgba, has_alpha });
    }

    const LOD: u8 = 1;
    for a in 0..c.articles.len() {
        let Some(mesh) = c.article_mesh(a, LOD) else { continue };
        let normals = fe_room::vertex_normals(&c, a, LOD).unwrap_or_default();
        let slot = fe_room::animation_slot(&c, a);
        let pivot = c.article_transform(a, LOD).map(|m| [m[12] * MIRROR_X, m[13], m[14]]).unwrap_or_default();
        let name = c.article_name(a).unwrap_or_default();
        for batch in &mesh.batches {
            let mat = materials.get(&batch.material);
            let page = mat.map_or(0, |m| m.page);
            let translucent = mat.is_some_and(|m| m.translucent);
            let self_lit = mat.is_some_and(|m| m.brightness > 1.0);
            let piece = match model.pieces.iter_mut().position(|p| p.name == name && p.page == page && p.translucent == translucent && p.self_lit == self_lit) {
                Some(i) => &mut model.pieces[i],
                None => {
                    model.pieces.push(RoomPiece { name: name.clone(), page, translucent, self_lit, slot, pivot, ..Default::default() });
                    model.pieces.last_mut().unwrap()
                }
            };
            for (k, &v) in batch.vertex_idx.iter().enumerate() {
                let p = mesh.positions[v as usize];
                piece.positions.push([p[0] * MIRROR_X, p[1], p[2]]);
                let n = normals.get(v as usize).copied().unwrap_or([0.0, 1.0, 0.0]);
                piece.normals.push([n[0] * MIRROR_X, n[1], n[2]]);
                piece.uvs.push(mesh.uvs[batch.uv_idx[k] as usize]);
            }
            model.triangles += batch.vertex_idx.len() / 3;
        }
    }
    Ok(model)
}

/// The car's box on a layout (`id=NFS5.3DCAR`: `pos` and `size`) as `[x, y, w, h]`.
pub fn car_box(lay: &nfspu_formats::fe::Lay) -> Option<[f32; 4]> {
    let block = lay.controls().find(|b| b.control().is_some_and(|c| c.eq_ignore_ascii_case("NFS5.3DCAR")))?;
    let (x, y) = block.pair("pos").unwrap_or((0, 0));
    let (w, h) = block.pair("size")?;
    Some([x as f32, y as f32, w as f32, h as f32])
}

/// The section numbers of a car's `.clr` in file order: the quick-race colour list is in file
/// order, and `car_model::CarOptions::paint` wants the section number (they differ: the 356's
/// file lists section 5 before 4).
pub fn clr_sections(text: &str) -> Vec<usize> {
    text.lines()
        .filter_map(|l| l.trim().strip_prefix('[')?.strip_suffix(']').map(|s| s.to_ascii_lowercase()))
        .filter_map(|s| s.strip_prefix("section")?.parse().ok())
        .collect()
}

/// The body style (1-based, as `car_model::CarOptions::style`) whose `.tpg` name appears in the
/// catalogue name: `'50 356 1100 Cabriolet` picks `[style2] name=Cabrio`. The first style when none
/// matches. (**Guess** at retail's rule: the catalogue record's own style field is not traced.)
pub fn style_for(car_name: &str, tpg: &str) -> usize {
    let name = car_name.to_ascii_lowercase();
    let mut style = 0usize;
    let mut best = (0usize, 1usize);
    for line in tpg.lines() {
        let line = line.trim();
        if let Some(n) = line.strip_prefix("[style").and_then(|l| l.strip_suffix(']')) {
            style = n.parse().unwrap_or(0);
        } else if let Some(value) = line.strip_prefix("name=") {
            let word = value.trim().to_ascii_lowercase();
            if style > 0 && !word.is_empty() && name.contains(&word) && word.len() > best.0 {
                best = (word.len(), style);
            }
        } else if line.starts_with('[') {
            style = 0;
        }
    }
    best.1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_from_catalogue_name() {
        let tpg = "[header]\nnumstyles=2\n[style1]\nname=Coupe\ncount=1\n[style2]\nname=Cabrio\n[file1.top]\nname=x\n";
        assert_eq!(style_for("'50 356 1100 Cabriolet", tpg), 2);
        assert_eq!(style_for("'50 356 1100 Coupe Ferdinand", tpg), 1);
        assert_eq!(style_for("'51 356 1300", tpg), 1);
        assert_eq!(style_for("x", ""), 1);
    }

    const BOX: [f32; 4] = [110.0, 230.0, 420.0, 180.0];

    #[test]
    fn zones_follow_the_turn() {
        // front to the camera: the whole box is the front, even the roof band of a fixed roof
        assert_eq!(zone_at(320.0, 300.0, BOX, 0.0, false), Zone::End(4));
        assert_eq!(zone_at(320.0, 235.0, BOX, 0.95, false), Zone::End(4));
        // a movable roof takes the top 5/16 (230 + 56.25)
        assert_eq!(zone_at(320.0, 286.0, BOX, 0.0, true), Zone::Roof);
        assert_eq!(zone_at(320.0, 287.0, BOX, 0.0, true), Zone::End(4));
        // rear
        assert_eq!(zone_at(120.0, 300.0, BOX, 0.5, false), Zone::End(3));
        // quarter turn: front at the left sixth (110..180), door in the middle, rear at the right
        assert_eq!(zone_at(150.0, 300.0, BOX, 0.25, false), Zone::End(4));
        assert_eq!(zone_at(320.0, 300.0, BOX, 0.25, false), Zone::Door(2));
        assert_eq!(zone_at(500.0, 300.0, BOX, 0.25, false), Zone::End(3));
        // three-quarter turn: mirrored, the other door
        assert_eq!(zone_at(150.0, 300.0, BOX, 0.75, false), Zone::End(3));
        assert_eq!(zone_at(320.0, 300.0, BOX, 1.75, false), Zone::Door(1));
        assert_eq!(zone_at(500.0, 300.0, BOX, 0.75, false), Zone::End(4));
    }

    #[test]
    fn turntable_turns_once_per_spin_and_pauses() {
        let mut t = Turntable::default();
        t.advance(&GARAGE, 7.5, false);
        assert!((t.turns - 0.25).abs() < 1e-9);
        assert!((t.yaw() + std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        t.advance(&GARAGE, 10.0, true);
        assert!((t.turns - 0.25).abs() < 1e-9);
    }

    #[test]
    fn cabin_blend_moves_the_room() {
        assert_eq!(staged_offset(&GARAGE, 0.0), [0.0, -1.1, 4.8]);
        assert_eq!(staged_offset(&GARAGE, 1.0), CABIN_OFFSET);
        let half = staged_offset(&GARAGE, 0.5);
        assert!((half[1] + 0.95).abs() < 1e-6 && (half[2] - 2.55).abs() < 1e-6);
    }

    #[test]
    fn fade_runs_both_ways() {
        let mut f = Fade::default();
        f.toggle();
        assert!(f.advance(0.25, 0.5));
        assert!((f.level - 0.5).abs() < 1e-6);
        assert!(f.advance(1.0, 0.5));
        assert_eq!(f.level, 1.0);
        assert!(!f.advance(0.1, 0.5));
        f.toggle();
        f.advance(1.0, 0.5);
        assert_eq!(f.level, 0.0);
    }

    #[test]
    fn field_of_view_from_half_horizontal_angle() {
        // 4:3: tan(37.5) * 0.75 = 0.5755 -> 29.9 degrees half, 59.8 full
        let v = vertical_fov(&GARAGE, 4.0 / 3.0).to_degrees();
        assert!((v - 59.8).abs() < 0.1, "{v}");
    }

    #[test]
    fn light_is_ambient_plus_facing() {
        let up = light_vertex(&GARAGE, [0.0, 1.0, 0.0], 1.0);
        let side = light_vertex(&GARAGE, [1.0, 0.0, 0.0], 1.0);
        assert!((side[1] - 65.0 / 255.0).abs() < 1e-6 || side[1] > 65.0 / 255.0);
        assert!(up[0] > 0.6 && up[0] < 0.7);
        let down = light_vertex(&GARAGE, [0.0, -1.0, 0.0], 1.0);
        assert_eq!(down, [55.0 / 255.0, 65.0 / 255.0, 55.0 / 255.0]);
    }

    #[test]
    fn car_box_and_paint_sections() {
        let lay = nfspu_formats::fe::Lay::parse("[section.0]\nid=REAL.BLACK_640x480\nname=ID_SCREEN\nroom=ID_GARAGE\n\n[section.1]\nid=NFS5.3DCAR\nname=ID_3DCar\npos=110,230\nsize=420,180\n");
        assert_eq!(car_box(&lay), Some(BOX));
        assert_eq!(clr_sections("[section1]\na=1\n[section2]\n[Section5]\n[section4]\n"), vec![1, 2, 5, 4]);
        assert_eq!(room_for("id_garage"), Some(GARAGE));
        assert_eq!(room_for("ID_NONE"), None);
    }
}
