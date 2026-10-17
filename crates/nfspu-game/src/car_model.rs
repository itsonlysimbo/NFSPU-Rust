//! Builds the exterior shape of a car from `GameData/CarModel/<name>.crp` in the player's install.
//!
//! Each triangle is tagged with a finish taken from its material kind, and each finish uses one
//! texture page assembled from the archives embedded in the car file. Only the default configuration
//! is used (Base type index 0); decals, shadow quads and driver models are left out. Units are
//! metres, the car faces -Z and its origin is at ground level.

use crate::MIRROR_X;
use nfspu_formats::{crp, refpack, shpi};
use std::{env, fs, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finish {
    /// Body paint (material kind `CarExtEnv`).
    Paint,
    /// Trim, underbody, wheel wells (`CarExt`).
    Trim,
    /// Glass (`CarWindow`).
    Glass,
    /// Tyres and rims (`CarWheel`).
    Tyre,
    /// Cabin: seats, dashboard, headliner (`CarInt`).
    Interior,
}

/// Triangles of one finish. `normals` are outward face normals, repeated per corner; `uvs` and
/// `materials` (the material index of the triangle part) are also per corner.
#[derive(Clone, Debug, Default)]
pub struct Piece {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub materials: Vec<u16>,
    /// Index of the article each corner came from (see `CarModel::article_names`).
    pub articles: Vec<u16>,
}

#[derive(Clone, Debug)]
pub struct Wheel {
    /// Where the wheel centre sits on the car.
    pub offset: [f32; 3],
    pub front: bool,
    pub piece: Piece,
}

/// A texture page (square RGBA8 image) assembled from the tiles of one embedded archive.
#[derive(Clone, Debug)]
pub struct Page {
    pub size: u32,
    pub rgba: Vec<u8>,
    /// True if some pixels are transparent or translucent.
    pub has_alpha: bool,
}

#[derive(Clone, Debug)]
pub struct CarModel {
    pub name: String,
    pub paint: Piece,
    pub trim: Piece,
    pub glass: Piece,
    pub interior: Piece,
    /// The soft top's rear window (a batch of a `Cabrio*` article with UVs spanning most of the page).
    pub cabrio_glass: Piece,
    /// Trim-page parts drawn without cut-outs: the wheel-arch liners (`WheelWell*`) sample a single
    /// grey texel whose alpha is 0 (on the 993 and GT3), which would make them invisible.
    pub trim_solid: Piece,
    /// Body triangles facing down into the shell (roof lining, underbody), drawn with the cabin
    /// material instead of the body artwork.
    pub paint_inner: Piece,
    /// The cabin at the exterior's level of detail, drawn in outside views (`interior` is the
    /// detailed level-1 cabin for the cockpit view).
    pub interior_outside: Piece,
    /// Headlights (articles `Light1` and `Light3`, at the front) and tail lights (`Light2`, at the rear),
    /// kept apart from the paint so the viewer can make them glow.
    pub headlights: Piece,
    pub taillights: Piece,
    /// The driver's body (suit) and head, from the `LODDriver` article of the chosen driver type.
    pub driver_suit: Piece,
    pub driver_head: Piece,
    pub wheels: Vec<Wheel>,
    /// Page for body paint pieces (embedded archive 0, "exterior"); 256 x 256 on the 993.
    pub exterior: Option<Page>,
    /// Page for the cabin (embedded archive 1, "interior").
    pub interior_page: Option<Page>,
    /// Page for trim and wheels (embedded archive 2, "extint"); 128 x 128 on the 993.
    pub extint: Option<Page>,
    /// Page for glass (embedded archive 3); 64 x 64.
    pub glass_page: Option<Page>,
    /// The soft top's window texture, from the shared `Cabrio.fsh` (32 x 32).
    pub cabrio_page: Option<Page>,
    /// Driver textures from the shared `Suit.fsh` and `Head.fsh` (128 x 128), for the chosen driver type.
    pub suit_page: Option<Page>,
    pub head_page: Option<Page>,
    /// Width, height, length of the body parts (without wheels).
    pub size: [f32; 3],
    pub articles_used: usize,
    /// Name of every article in the file, by article index.
    pub article_names: Vec<String>,
    /// Name of the body style that was used, e.g. "Carreracabrio".
    pub style_name: String,
    /// How many body styles the car's .tpg defines (0 if none).
    pub style_count: usize,
}

impl CarModel {
    pub fn triangles(&self) -> usize {
        let body = (self.paint.positions.len() + self.trim.positions.len() + self.glass.positions.len() + self.interior.positions.len() + self.cabrio_glass.positions.len() + self.trim_solid.positions.len() + self.paint_inner.positions.len() + self.headlights.positions.len() + self.taillights.positions.len() + self.driver_suit.positions.len() + self.driver_head.positions.len()) / 3;
        body + self.wheels.iter().map(|w| w.piece.positions.len() / 3).sum::<usize>()
    }
}

pub fn finish_for_material(kind: &str) -> Option<Finish> {
    match kind {
        "CarExtEnv" => Some(Finish::Paint),
        "CarExt" => Some(Finish::Trim),
        "CarWindow" => Some(Finish::Glass),
        "CarWheel" => Some(Finish::Tyre),
        "CarInt" => Some(Finish::Interior),
        _ => Some(Finish::Trim),
    }
}

/// Whether any triangle of a batch has its UVs inside one of the body page's `hand` tiles (with
/// two texels of slack on a 256 page), meaning the handle's artwork is on the body page, not the
/// trim page. Per triangle, because a handle batch can hold the grip and a second small part with
/// UVs elsewhere (928).
pub fn handle_on_body_page(uv_idx: &[u16], uvs: &[[f32; 2]], tiles: &[[f32; 4]]) -> bool {
    let slack = 2.0 / 256.0;
    uv_idx.chunks_exact(3).any(|tri| {
        let (mut u0, mut v0, mut u1, mut v1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for &i in tri {
            let [u, v] = uvs[i as usize];
            u0 = u0.min(u);
            v0 = v0.min(v);
            u1 = u1.max(u);
            v1 = v1.max(v);
        }
        tiles.iter().any(|t| u0 >= t[0] - slack && v0 >= t[1] - slack && u1 <= t[2] + slack && v1 <= t[3] + slack)
    })
}

/// How far (metres) lamp triangles are moved out along their normal so they draw over the body.
const LAMP_LIFT: f32 = 0.005;

/// Whether a body triangle belongs to a lamp, and which end: `Some(true)` for the front (headlights),
/// `Some(false)` for the rear (tail lights). Lamps are the painted-page articles named `Light*` or
/// `HeadLight*` (names differ per car: the 993 has Light1-3, the 928 HeadLight and Light4); the end
/// comes from the triangle's position (the car faces -Z). Side markers near the middle are left out.
fn lamp_end(article: &str, finish: Finish, interior_pass: bool, tri: &[[f32; 3]]) -> Option<bool> {
    if interior_pass || finish != Finish::Paint || !(article.starts_with("Light") || article.starts_with("HeadLight")) {
        return None;
    }
    let z = tri.iter().map(|p| p[2]).sum::<f32>() / tri.len().max(1) as f32;
    if z.abs() < 1.0 {
        return None;
    }
    Some(z < 0.0)
}

/// Names of articles that are never drawn: overlay decals, shadow quads, drivers, passengers, gauges.
fn is_skipped_name(name: &str) -> bool {
    ["Decal", "WheelShadow", "Passenger", "Needle", "Gauge", "Cap", "Ant"].iter().any(|p| name.starts_with(p))
}

/// Cabin pieces that are never drawn: interior glass overlays (the windows are drawn from the
/// exterior), the engine and trunk space (hidden behind panels), and the driver and passenger,
/// which need their own textures.
fn is_hidden_in_cabin(name: &str) -> bool {
    ["Engine", "TrunkSpace", "InteriorGlass", "InteriorRoofGlass", "InteriorRoof1Glass", "InteriorSunRoofGlass", "InteriorDoorGlass", "InteriorDoorInGlass", "DriverBody", "LODDriver", "Legs", "LeftArm", "RightArm", "LeftHand", "RightHand", "Passenger"]
        .iter()
        .any(|p| name.starts_with(p))
}

/// One body style from a car's `.tpg` file (`[styleN]`): which type of each piece (geometry id) and
/// which type of each artwork family (texture id) it uses. Ids that are not listed use type 0.
#[derive(Clone, Debug, Default)]
pub struct Style {
    pub name: String,
    pub geometry: std::collections::HashMap<u32, u32>,
    pub texture: std::collections::HashMap<u32, u32>,
}

/// When a texture tile is drawn (`[fileN.tile]` in the `.tpg`): if any listed (geometry, type) or
/// (texture, type) pair matches the style. Tiles without pairs are always drawn, except the
/// front-end and test-drive wheel faces and race decals, which are hidden in a normal race.
#[derive(Clone, Debug, Default)]
pub struct TileRule {
    pub geometry: Vec<(u32, u32)>,
    pub texture: Vec<(u32, u32)>,
    pub frontend: Option<u32>,
    pub testdrive: Option<u32>,
    pub race_decal: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Tpg {
    pub styles: Vec<Style>,
    /// Keyed by (file number starting at 1, lower-case tile name).
    pub tiles: std::collections::HashMap<(u32, String), TileRule>,
}

impl Style {
    /// The type of piece `geometry` this style uses.
    pub fn geometry_type(&self, geometry: u32) -> u32 {
        self.geometry.get(&geometry).copied().unwrap_or(0)
    }
    pub fn texture_type(&self, texture: u32) -> u32 {
        self.texture.get(&texture).copied().unwrap_or(0)
    }
}

impl TileRule {
    /// Whether the tile is drawn for `style` in a normal race (not the front end or test drive).
    pub fn applies(&self, style: &Style) -> bool {
        if self.frontend == Some(1) || self.testdrive == Some(1) || (self.race_decal && !style.name.to_ascii_lowercase().contains("race")) {
            return false;
        }
        if self.geometry.is_empty() && self.texture.is_empty() {
            return true;
        }
        self.geometry.iter().any(|&(g, t)| style.geometry_type(g) == t) || self.texture.iter().any(|&(x, t)| style.texture_type(x) == t)
    }
}

/// Parses the `[styleN]` and `[fileN.tile]` sections of a `.tpg` file.
pub fn parse_tpg(text: &str) -> Tpg {
    let mut sections: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            sections.push((name.to_ascii_lowercase(), Vec::new()));
        } else if let (Some((key, value)), Some(last)) = (line.split_once('='), sections.last_mut()) {
            last.1.push((key.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    // `geometryK=id` / `textureK=id` pair with `typeK=value` of the same K
    let pairs = |kv: &[(String, String)]| -> Vec<(bool, u32, u32)> {
        let num = |s: &str| s.parse::<u32>().ok();
        let mut out = Vec::new();
        for (key, value) in kv {
            let (is_geometry, k) = if let Some(k) = key.strip_prefix("geometry") { (true, k) } else if let Some(k) = key.strip_prefix("texture") { (false, k) } else { continue };
            let (Some(id), Some(ty)) = (num(value), kv.iter().find(|(t, _)| *t == format!("type{k}")).and_then(|(_, v)| num(v))) else { continue };
            out.push((is_geometry, id, ty));
        }
        out
    };
    let mut tpg = Tpg::default();
    for (name, kv) in &sections {
        if let Some(n) = name.strip_prefix("style").and_then(|n| n.parse::<usize>().ok()) {
            let mut style = Style { name: kv.iter().find(|(k, _)| k == "name").map(|(_, v)| v.clone()).unwrap_or_default(), ..Default::default() };
            for (is_geometry, id, ty) in pairs(kv) {
                if is_geometry { style.geometry.insert(id, ty) } else { style.texture.insert(id, ty) };
            }
            if tpg.styles.len() < n {
                tpg.styles.resize(n, Style::default());
            }
            tpg.styles[n - 1] = style;
        } else if let Some((file, tile)) = name.strip_prefix("file").and_then(|r| r.split_once('.')) {
            let Ok(file) = file.parse::<u32>() else { continue };
            if tile == "details" {
                continue;
            }
            let flag = |key: &str| kv.iter().find(|(k, _)| k == key).and_then(|(_, v)| v.parse::<u32>().ok());
            let mut rule = TileRule { frontend: flag("frontend"), testdrive: flag("testdrive"), race_decal: flag("racedecal") == Some(1), ..Default::default() };
            for (is_geometry, id, ty) in pairs(kv) {
                if is_geometry { rule.geometry.push((id, ty)) } else { rule.texture.push((id, ty)) }
            }
            tpg.tiles.insert((file, tile.to_string()), rule);
        }
    }
    tpg
}

/// Which tiles of embedded archive `index` (file `index + 1` in the `.tpg`) are drawn for `style`.
pub fn tile_flags(archive: &shpi::ShpiArchive, tpg: &Tpg, index: u16, style: &Style) -> Vec<bool> {
    archive
        .entries
        .iter()
        .map(|e| tpg.tiles.get(&(index as u32 + 1, e.name.trim().to_ascii_lowercase())).map_or(true, |r| r.applies(style)))
        .collect()
}

/// On the body page every texel is partly "transparent" (alpha about 140-220 of 255): the channel is
/// a paint mask, not transparency, and cutting out texels below 50% punches holes in the side panels.
/// Only texels with alpha 0 are real holes (wheel arches, light corners), so every other texel becomes
/// opaque. (The mask could later drive the paint colour.)
pub fn alpha_to_mask(rgba: &mut [u8]) {
    for px in rgba.chunks_exact_mut(4) {
        px[3] = if px[3] > 0 { 255 } else { 0 };
    }
}

/// Reads the page sizes from a car's `.tpg` text file. Embedded archive `i` is listed there as
/// `[file{i+1}.details]` with `tpage=M`, and `[tpageM.details]` gives `width=W`. Returns a map from
/// the embedded archive index to the page width in pixels.
pub fn tpg_page_sizes(text: &str) -> std::collections::HashMap<u16, u32> {
    use std::collections::HashMap;
    let (mut file_page, mut page_width): (HashMap<u32, u32>, HashMap<u32, u32>) = (HashMap::new(), HashMap::new());
    let mut section = String::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.to_ascii_lowercase();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let Ok(value) = value.trim().parse::<u32>() else { continue };
        let number = |prefix: &str| section.strip_prefix(prefix).and_then(|r| r.strip_suffix(".details")).and_then(|n| n.parse::<u32>().ok());
        match (key.trim().to_ascii_lowercase().as_str(), number("file"), number("tpage")) {
            ("tpage", Some(f), _) => {
                file_page.insert(f, value);
            }
            ("width", _, Some(p)) => {
                page_width.insert(p, value);
            }
            _ => {}
        }
    }
    file_page.into_iter().filter_map(|(f, p)| Some(((f - 1) as u16, *page_width.get(&p)?))).collect()
}

/// The texture files a car's `.tpg` lists (`fileN=exterior.fsh` in `[header]`), as embedded archive
/// index (N - 1) to lower-case file name.
pub fn tpg_file_names(text: &str) -> std::collections::HashMap<u16, String> {
    let mut out = std::collections::HashMap::new();
    for line in text.lines() {
        let Some((key, value)) = line.trim().split_once('=') else { continue };
        let key = key.trim().to_ascii_lowercase();
        let Some(n) = key.strip_prefix("file").and_then(|n| n.parse::<u16>().ok()) else { continue };
        if n >= 1 {
            out.insert(n - 1, value.trim().to_ascii_lowercase());
        }
    }
    out
}

/// What an embedded texture archive is for, from its file name in the `.tpg`: 0 body (exterior),
/// 1 cabin (interior), 2 trim and wheels (extint), 3 glass. Shadow, driver and other files are
/// not pages of the car body (`None`). Without a name the archive's position decides (Porsche order).
pub fn page_role(index: u16, file_name: Option<&str>) -> Option<u16> {
    match file_name {
        Some("exterior.fsh") => Some(0),
        Some("interior.fsh") => Some(1),
        Some("extint.fsh") => Some(2),
        Some("glass.fsh") => Some(3),
        Some(_) => None,
        None => (index <= 3).then_some(index),
    }
}

/// A tile of a page: its UV rectangle (u0, v0, u1, v1) and its name.
pub type NamedTile = ([f32; 4], String);

/// A traffic car's two pages side by side in one square texture: the exterior page at the top left,
/// the interior page to its right (the bottom half stays empty).
#[derive(Clone, Debug)]
pub struct TrafficAtlas {
    pub page: Page,
    /// Sizes of the exterior and interior pages, and of the whole atlas, in pixels.
    sizes: [u32; 2],
    side: u32,
    /// Kept tiles of each page.
    tiles: [Vec<NamedTile>; 2],
}

/// Tile-name prefixes that belong to an article, for when its UVs fit a tile on both pages.
fn tile_affinity(article: &str) -> &'static [&'static str] {
    let a = article.to_ascii_lowercase();
    if a.starts_with("wheelwell") {
        &["tre", "ww"]
    } else if a.starts_with("wheel") {
        &["whe", "wh", "tre"]
    } else if a.starts_with("light") || a.starts_with("headlight") {
        &["lit", "li"]
    } else if a.starts_with("license") {
        &["lic"]
    } else if a.starts_with("bottom") {
        &["bot", "cbot"]
    } else if a.contains("mirror") {
        &["mi"]
    } else {
        &[]
    }
}

impl TrafficAtlas {
    pub fn build(exterior: &Page, interior: &Page, tiles: [Vec<NamedTile>; 2]) -> TrafficAtlas {
        let half = exterior.size.max(interior.size);
        let side = 2 * half;
        let mut rgba = vec![0u8; (side * side * 4) as usize];
        for (page, x0) in [(exterior, 0u32), (interior, half)] {
            for y in 0..page.size {
                let src = (y * page.size * 4) as usize;
                let dst = ((y * side + x0) * 4) as usize;
                let n = (page.size * 4) as usize;
                rgba[dst..dst + n].copy_from_slice(&page.rgba[src..src + n]);
            }
        }
        TrafficAtlas { page: Page { size: side, rgba, has_alpha: true }, sizes: [exterior.size, interior.size], side, tiles }
    }

    /// Alpha of page `half` at a UV of that page.
    fn alpha_at(&self, uv: [f32; 2], half: usize) -> u8 {
        let p = self.place([uv[0].clamp(0.0, 0.999), uv[1].clamp(0.0, 0.999)], half);
        let (x, y) = ((p[0] * self.side as f32) as u32, (p[1] * self.side as f32) as u32);
        self.page.rgba[((y.min(self.side - 1) * self.side + x.min(self.side - 1)) * 4 + 3) as usize]
    }

    /// Which page some UVs' artwork is on (0 exterior, 1 interior): the page with a tile containing
    /// them all; if both have one, the page whose tile name suits the article (lamps `lit`, plate
    /// `lice`, wheels `whe`/`tre`, ...); otherwise the page that is opaque at their centre; otherwise
    /// `fallback`.
    pub fn half_for(&self, uvs: &[[f32; 2]], article: &str, fallback: usize) -> usize {
        let (mut u0, mut v0, mut u1, mut v1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for uv in uvs {
            u0 = u0.min(uv[0]);
            v0 = v0.min(uv[1]);
            u1 = u1.max(uv[0]);
            v1 = v1.max(uv[1]);
        }
        let slack = 1.5 / 128.0;
        let fitting = |h: usize| -> Vec<&str> {
            self.tiles[h]
                .iter()
                .filter(|(t, _)| u0 >= t[0] - slack && v0 >= t[1] - slack && u1 <= t[2] + slack && v1 <= t[3] + slack)
                .map(|(_, n)| n.as_str())
                .collect()
        };
        let (f0, f1) = (fitting(0), fitting(1));
        match (f0.is_empty(), f1.is_empty()) {
            (false, true) => return 0,
            (true, false) => return 1,
            (false, false) => {
                let prefixes = tile_affinity(article);
                let suits = |names: &[&str]| names.iter().any(|n| prefixes.iter().any(|p| n.to_ascii_lowercase().starts_with(p)));
                match (suits(&f0), suits(&f1)) {
                    (true, false) => return 0,
                    (false, true) => return 1,
                    _ => {}
                }
            }
            (true, true) => {}
        }
        let centre = [(u0 + u1) / 2.0, (v0 + v1) / 2.0];
        match (self.alpha_at(centre, 0) >= 128, self.alpha_at(centre, 1) >= 128) {
            (true, false) => 0,
            (false, true) => 1,
            _ => fallback,
        }
    }

    /// Moves a UV of page `half` to where that page sits in the atlas.
    pub fn place(&self, uv: [f32; 2], half: usize) -> [f32; 2] {
        let s = self.side as f32;
        let x0 = if half == 0 { 0.0 } else { (self.side / 2) as f32 };
        let size = self.sizes[half] as f32;
        [(x0 + uv[0] * size) / s, uv[1] * size / s]
    }
}
/// Moves a point by a row-vector 4x4 matrix (translation in the last row).
pub fn apply_matrix(m: &[f32; 16], p: [f32; 3]) -> [f32; 3] {
    [
        p[0] * m[0] + p[1] * m[4] + p[2] * m[8] + m[12],
        p[0] * m[1] + p[1] * m[5] + p[2] * m[9] + m[13],
        p[0] * m[2] + p[1] * m[6] + p[2] * m[10] + m[14],
    ]
}

/// Outward normal of a triangle in the file's winding: opposite to `(b - a) x (c - a)`.
pub fn outward_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len < 1e-9 {
        [0.0, 1.0, 0.0]
    } else {
        [-n[0] / len, -n[1] / len, -n[2] / len]
    }
}

pub fn car_dir() -> Result<PathBuf, String> {
    let dir = env::var("NFSPU_GAME_DIR").map_err(|_| "NFSPU_GAME_DIR is not set".to_string())?;
    Ok(PathBuf::from(dir).join("GameData").join("CarModel"))
}

/// What to build when loading a car.
#[derive(Clone, Copy, Debug)]
pub struct CarOptions {
    /// Level of detail: 1 garage, 2 in-car, 3 and up exterior.
    pub level: u8,
    /// Body style (1-based) from the car's `.tpg` file, for example 1 = Carrera coupe, 2 = targa,
    /// 6 = cabrio on the 993. The style decides which type of each piece is used (roof, bumper,
    /// wheels, ...) and which version of each artwork tile is drawn. An unknown number falls back to
    /// the first style; a car without a `.tpg` uses type 0 of everything.
    pub style: usize,
    /// Driver type to seat in the car: 1 = racing suit and helmet, 2-10 = casual outfits, 0 = empty.
    pub driver: u8,
    /// Paint scheme: the section number in the car's `.clr` file (its `color1` is the body colour,
    /// for example 4 = red on the 993), or 0 / an unknown number for unpainted (white and grey).
    pub paint: usize,
}

impl Default for CarOptions {
    fn default() -> Self {
        CarOptions { level: 3, style: 1, driver: 1, paint: 4 }
    }
}

/// Body colour (`color1`) of each paint scheme `[sectionN]` in a car's `.clr` file, by section number.
pub fn parse_clr(text: &str) -> std::collections::HashMap<usize, [u8; 3]> {
    let mut out = std::collections::HashMap::new();
    let mut current: Option<usize> = None;
    let mut rgb = [None::<u8>; 3];
    let mut flush = |section: Option<usize>, rgb: &[Option<u8>; 3]| {
        if let (Some(n), [Some(r), Some(g), Some(b)]) = (section, rgb) {
            out.insert(n, [*r, *g, *b]);
        }
    };
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            flush(current, &rgb);
            current = name.to_ascii_lowercase().strip_prefix("section").and_then(|n| n.parse().ok());
            rgb = [None; 3];
        } else if let Some((key, value)) = line.split_once('=') {
            let Ok(value) = value.trim().parse::<u8>() else { continue };
            match key.trim().to_ascii_lowercase().as_str() {
                "color1.r" => rgb[0] = Some(value),
                "color1.g" => rgb[1] = Some(value),
                "color1.b" => rgb[2] = Some(value),
                _ => {}
            }
        }
    }
    flush(current, &rgb);
    out
}

/// Applies the paint colour to the body page using its alpha channel as the paint mask: alpha 255
/// means "not painted" (black soft top, exhaust, lamp lenses), alpha 0 is a hole, and the partial
/// values (about 140-220) are painted body, strongest near 224. Colours are multiplied toward the
/// paint colour by the mask strength. Evidence: the soft-top tile is 31% alpha 255, the exhaust 100%.
pub fn tint_paint_mask(rgba: &mut [u8], colour: [u8; 3]) {
    for px in rgba.chunks_exact_mut(4) {
        let a = px[3];
        if a == 0 || a == 255 {
            continue;
        }
        let m = (a as f32 / 224.0).min(1.0);
        for k in 0..3 {
            let mult = 1.0 - m * (1.0 - colour[k] as f32 / 255.0);
            px[k] = (px[k] as f32 * mult).round() as u8;
        }
    }
}

/// Loads a car (e.g. "993") built as described by `options`.
pub fn load_car(name: &str, options: &CarOptions) -> Result<CarModel, String> {
    let CarOptions { level, style: style_number, driver, paint } = *options;
    let path = car_dir()?.join(format!("{name}.crp"));
    let raw = fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    // most cars are RefPack-compressed; a few (e.g. the 928) are stored plain
    let data = if refpack::is_refpack(&raw) { refpack::decompress(&raw).map_err(|e| format!("{}: {e}", path.display()))? } else { raw };
    let crp = crp::parse(&data).map_err(|e| format!("{e}"))?;
    let kinds: std::collections::HashMap<u16, String> = crp.materials().into_iter().map(|m| (m.index, m.kind)).collect();

    let mut model = CarModel {
        name: name.to_string(),
        paint: Piece::default(),
        trim: Piece::default(),
        glass: Piece::default(),
        interior: Piece::default(),
        cabrio_glass: Piece::default(),
        interior_outside: Piece::default(),
        trim_solid: Piece::default(),
        paint_inner: Piece::default(),
        headlights: Piece::default(),
        taillights: Piece::default(),
        driver_suit: Piece::default(),
        driver_head: Piece::default(),
        wheels: Vec::new(),
        exterior: None,
        interior_page: None,
        extint: None,
        glass_page: None,
        cabrio_page: None,
        suit_page: None,
        head_page: None,
        size: [0.0; 3],
        articles_used: 0,
        article_names: (0..crp.articles.len()).map(|a| crp.article_name(a).unwrap_or_default()).collect(),
        style_name: String::new(),
        style_count: 0,
    };
    // Textures live inside the car file as embedded archives; index 0 = exterior, 1 = interior,
    // 2 = trim and wheels, 3 = glass (checked by eye on the 993). Page sizes come from the car's
    // .tpg text file (the wheel and trim page is 128 x 128, not 256); defaults if it is missing.
    let tpg = fs::read_to_string(car_dir()?.join(format!("{name}.tpg"))).unwrap_or_default();
    let sizes = tpg_page_sizes(&tpg);
    let rules = parse_tpg(&tpg);
    let style = rules.styles.get(style_number.saturating_sub(1)).or_else(|| rules.styles.first()).cloned().unwrap_or_default();
    model.style_name = style.name.clone();
    model.style_count = rules.styles.len();
    // door-handle tiles on the body page, as UV rectangles (u0, v0, u1, v1); see `handle_on_body_page`
    let mut body_handle_tiles: Vec<[f32; 4]> = Vec::new();
    // the plate tile(s) on the trim page (`lice`), as UV rectangles; see `plate_batch`
    let mut plate_tiles: Vec<[f32; 4]> = Vec::new();
    let file_names = tpg_file_names(&tpg);
    // Traffic cars (no trim page) only go down to level 3, which is very crude (32 body triangles
    // and no lamps on eas1), and are low-poly anyway: use their level 1.
    let traffic = !file_names.is_empty() && !file_names.values().any(|n| n == "extint.fsh");
    let level = if traffic { 1 } else { level };
    // traffic cars spread one part's artwork over two 128-px pages ("exterior": body and tyres;
    // "interior": lamps, glass, plate, wheel faces, underside); kept tiles of each, see `TrafficAtlas`
    let mut atlas_tiles: [Vec<NamedTile>; 2] = [Vec::new(), Vec::new()];
    for (index, blob) in crp.embedded_archives() {
        // the archive's job comes from its file name: traffic cars list exterior, interior, shadow
        // and driver files, so their third archive is a shadow, not a trim page
        let file_name = file_names.get(&index).map(String::as_str);
        if traffic && driver != 0 && file_name == Some("driver.fsh") {
            // a traffic car's own driver texture (one page for body and head)
            if let Ok(archive) = shpi::parse(blob) {
                let size = sizes.get(&index).copied().unwrap_or(64);
                let rgba = shpi::compose_page(blob, &archive, size);
                let has_alpha = rgba.chunks_exact(4).any(|p| p[3] < 255);
                model.suit_page = Some(Page { size, rgba, has_alpha });
                model.head_page = model.suit_page.clone();
            }
            continue;
        }
        let Some(role) = page_role(index, file_name) else { continue };
        let default_size = match role {
            0 | 1 => 256,
            2 => 128,
            _ => 64,
        };
        let size = sizes.get(&index).copied().unwrap_or(default_size);
        let Ok(archive) = shpi::parse(blob) else { continue };
        // draw only the tiles the style uses (alternative roofs, sides, badges, wheel faces, ...)
        let keep = tile_flags(&archive, &rules, index, &style);
        let mut rgba = shpi::compose_page_filtered(blob, &archive, size, &keep);
        if traffic && role <= 1 {
            let s = size as f32;
            let rects: Vec<NamedTile> = archive
                .entries
                .iter()
                .zip(&keep)
                .filter(|(_, k)| **k)
                .map(|(e, _)| ([e.x as f32 / s, e.y as f32 / s, (e.x as f32 + e.width as f32) / s, (e.y as f32 + e.height as f32) / s], e.name.clone()))
                .collect();
            atlas_tiles[role as usize] = rects;
        }
        if role == 2 {
            let s = size as f32;
            plate_tiles = archive
                .entries
                .iter()
                .filter(|e| e.name.to_ascii_lowercase().starts_with("lice"))
                .map(|e| [e.x as f32 / s, e.y as f32 / s, (e.x as f32 + e.width as f32) / s, (e.y as f32 + e.height as f32) / s])
                .collect();
        }
        if role == 0 {
            let s = size as f32;
            body_handle_tiles = archive
                .entries
                .iter()
                .filter(|e| e.name.to_ascii_lowercase().starts_with("hand"))
                .map(|e| [e.x as f32 / s, e.y as f32 / s, (e.x as f32 + e.width as f32) / s, (e.y as f32 + e.height as f32) / s])
                .collect();
            // the body page's alpha is a paint mask, not transparency: paint first, then keep only the real holes
            let clr = fs::read_to_string(car_dir()?.join(format!("{name}.clr"))).unwrap_or_default();
            if let Some(colour) = parse_clr(&clr).get(&paint) {
                tint_paint_mask(&mut rgba, *colour);
            }
            alpha_to_mask(&mut rgba);
        }
        let has_alpha = rgba.chunks_exact(4).any(|p| p[3] < 255);
        let page = Page { size, rgba, has_alpha };
        match role {
            0 => model.exterior = Some(page),
            1 => model.interior_page = Some(page),
            2 => model.extint = Some(page),
            _ => model.glass_page = Some(page),
        }
    }
    // traffic cars: one texture with the exterior page on the left and the interior page on the
    // right; every exterior part points into the half its UVs fit (see `TrafficAtlas::place`)
    let atlas = if traffic {
        match (&model.exterior, &model.interior_page) {
            (Some(e), Some(i)) => {
                let atlas = TrafficAtlas::build(e, i, atlas_tiles.clone());
                let page = atlas.page.clone();
                model.exterior = Some(page.clone());
                model.extint = Some(page.clone());
                model.glass_page = Some(page);
                Some(atlas)
            }
            _ => None,
        }
    } else {
        None
    };
    // cars with a single body page draw trim, wheels and glass from it too
    if model.extint.is_none() {
        model.extint = model.exterior.clone();
    }
    if model.glass_page.is_none() {
        model.glass_page = model.exterior.clone();
    }
    // the soft top's rear window has its own small texture in a shared file next to the cars
    if let Ok(blob) = fs::read(car_dir()?.join("Cabrio.fsh")) {
        if let Ok(archive) = shpi::parse(&blob) {
            let size = sizes.get(&6).copied().unwrap_or(32); // file7 in the .tpg
            let rgba = shpi::compose_page(&blob, &archive, size);
            let has_alpha = rgba.chunks_exact(4).any(|p| p[3] < 255);
            model.cabrio_page = Some(Page { size, rgba, has_alpha });
        }
    }
    // Driver textures: Suit.fsh (file 9) and Head.fsh (file 10) hold one skin per driver type; draw
    // only the tiles the .tpg assigns to this type (type 1 = `suit` and `helm`).
    if driver != 0 && !traffic {
        let driver_style = Style { geometry: [(9u32, driver as u32)].into_iter().collect(), ..Default::default() };
        for (file_name, index) in [("Suit.fsh", 8u16), ("Head.fsh", 9u16)] {
            let Ok(blob) = fs::read(car_dir()?.join(file_name)) else { continue };
            let Ok(archive) = shpi::parse(&blob) else { continue };
            let size = sizes.get(&index).copied().unwrap_or(128);
            let keep = tile_flags(&archive, &rules, index, &driver_style);
            let rgba = shpi::compose_page_filtered(&blob, &archive, size, &keep);
            let has_alpha = rgba.chunks_exact(4).any(|p| p[3] < 255);
            let page = Page { size, rgba, has_alpha };
            if index == 8 {
                model.suit_page = Some(page);
            } else {
                model.head_page = Some(page);
            }
        }
    }
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    // the driver's batches by material; split into head and suit after the loop
    let mut driver_parts: Vec<(u16, Piece)> = Vec::new();

    // Three passes: the exterior from the requested level; the cabin from level 1 (the garage
    // model), which has the complete dashboard, seats and door panels that level 3 leaves out, for
    // the cockpit view; and the cabin from the exterior's own level for outside views, because the
    // detailed cabin can poke through the simpler body (the 928's door edges).
    for (interior_pass, lvl, outside_cabin) in [(false, level, false), (true, 1u8, false), (true, level, true)] {
    for a in 0..crp.articles.len() {
        let Some(article_name) = crp.article_name(a) else { continue };
        let Some(base) = crp.article_base(a) else { continue };
        // the driver is chosen by its own type (1 = racing suit and helmet), not by the body style
        // traffic cars have a single driver of their own (`DriverBody0`, at level 1)
        let is_driver = !interior_pass && (article_name.starts_with("LODDriver") || (traffic && article_name.starts_with("DriverBody")));
        if is_driver {
            if driver == 0 || base.geometry != 9 || (!traffic && base.type_index != driver) {
                continue;
            }
        } else if base.type_index as u32 != style.geometry_type(base.geometry as u32) || is_skipped_name(&article_name) || (interior_pass && is_hidden_in_cabin(&article_name)) {
            continue;
        }
        let Some(mesh) = crp.article_mesh(a, lvl) else { continue };
        let matrix = crp.article_transform(a, lvl);
        // WheelFront / WheelRear are the wheels; WheelWell (the arch liner) is ordinary body trim
        let is_wheel = !interior_pass && (article_name.starts_with("WheelFront") || article_name.starts_with("WheelRear"));
        let mut wheel_piece = Piece::default();
        let mut used = false;

        for b in &mesh.batches {
            let kind = kinds.get(&b.material).map(String::as_str).unwrap_or("");
            let Some(finish) = finish_for_material(kind) else { continue };
            // the shiny dash top (356) has a body-paint kind but its artwork is on the cabin page
            let finish = if article_name.starts_with("ShinyDash") { Finish::Interior } else { finish };
            // the interior pass keeps only cabin triangles, the exterior pass leaves them out
            if !is_driver && (finish == Finish::Interior) != interior_pass {
                continue;
            }
            // door handles: the same material kind points at the trim page on some cars (928) and
            // at a `hand` tile on the body page on others (959); go by where the UVs fall
            let finish = if !interior_pass && article_name.starts_with("Handle") && handle_on_body_page(&b.uv_idx, &mesh.uvs, &body_handle_tiles) {
                Finish::Paint
            } else {
                finish
            };
            // traffic cars: which of the two pages this batch's artwork is on (body parts default to
            // the exterior page, everything else to the interior page)
            let atlas_half = match (&atlas, interior_pass || is_driver) {
                (Some(at), false) => {
                    let uvs: Vec<[f32; 2]> = b.uv_idx.iter().map(|&i| mesh.uvs[i as usize]).collect();
                    Some(at.half_for(&uvs, &article_name, if finish == Finish::Paint { 0 } else { 1 }))
                }
                _ => None,
            };
            // a batch whose UVs span less than 2 texels of the 128-px trim page samples one solid
            // colour (944 underside, arch liners); its texel's alpha is not a cut-out
            let solid_sample = !is_wheel && {
                let (mut u0, mut u1, mut v0, mut v1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
                for &i in &b.uv_idx {
                    let uv = mesh.uvs[i as usize];
                    u0 = u0.min(uv[0]);
                    u1 = u1.max(uv[0]);
                    v0 = v0.min(uv[1]);
                    v1 = v1.max(uv[1]);
                }
                !b.uv_idx.is_empty() && (u1 - u0) * 128.0 < 2.0 && (v1 - v0) * 128.0 < 2.0
            };
            // a License batch whose UVs fit the trim page's `lice` tile is the plate itself
            let plate_batch = article_name.starts_with("License") && (plate_tiles.is_empty() || handle_on_body_page(&b.uv_idx, &mesh.uvs, &plate_tiles));
            // traffic lamps are trim-kind batches on the interior page; treat them like body lamps
            let lamp_finish = if atlas.is_some() && finish == Finish::Trim { Finish::Paint } else { finish };
            // a batch of a soft-top article whose UVs cover most of a page is the rear window
            let cabrio_window = !interior_pass && article_name.starts_with("Cabrio") && {
                let (mut u0, mut u1, mut v0, mut v1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
                for &i in &b.uv_idx {
                    let uv = mesh.uvs[i as usize];
                    u0 = u0.min(uv[0]);
                    u1 = u1.max(uv[0]);
                    v0 = v0.min(uv[1]);
                    v1 = v1.max(uv[1]);
                }
                u1 - u0 > 0.5 && v1 - v0 > 0.5
            };
            for t in 0..b.vertex_idx.len() / 3 {
                let raw_uvs: Vec<[f32; 2]> = (0..3).map(|q| mesh.uvs[b.uv_idx[3 * t + q] as usize]).collect();
                // traffic cars: per triangle, since one batch can mix tiles of both pages (eas10 tyres)
                let corner_uvs: Vec<[f32; 2]> = match (&atlas, atlas_half) {
                    (Some(at), Some(h)) => {
                        let h = at.half_for(&raw_uvs, &article_name, h);
                        raw_uvs.iter().map(|uv| at.place(*uv, h)).collect()
                    }
                    _ => raw_uvs,
                };
                let tri: Vec<[f32; 3]> = (0..3)
                    .map(|q| {
                        let p = mesh.positions[b.vertex_idx[3 * t + q] as usize];
                        // wheels keep their local shape and are placed by their offset
                        match (&matrix, is_wheel) {
                            (Some(m), false) => apply_matrix(m, p),
                            _ => p,
                        }
                    })
                    .collect();
                let n = outward_normal(tri[0], tri[1], tri[2]);
                // mirror x (see MIRROR_X): positions and the normal, after the winding-based normal is known
                let tri: Vec<[f32; 3]> = tri.iter().map(|p| [p[0] * MIRROR_X, p[1], p[2]]).collect();
                let n = [n[0] * MIRROR_X, n[1], n[2]];
                // Body triangles whose front face points downwards into the shell (roof lining on cars with no
                // separate `InteriorRoof` article, underbody): invisible from outside, but their body
                // artwork would show from the cockpit and through the glass, so they get the cabin
                // material instead.
                let target = if is_wheel {
                    &mut wheel_piece
                } else if !interior_pass && !is_driver && finish == Finish::Paint && n[1] < -0.25 {
                    &mut model.paint_inner
                } else if is_driver {
                    let at = driver_parts.iter().position(|(m, _)| *m == b.material).unwrap_or_else(|| {
                        driver_parts.push((b.material, Piece::default()));
                        driver_parts.len() - 1
                    });
                    &mut driver_parts[at].1
                } else if cabrio_window {
                    &mut model.cabrio_glass
                } else if !interior_pass && matches!(finish, Finish::Trim | Finish::Tyre) && (article_name.starts_with("WheelWell") || solid_sample) {
                    &mut model.trim_solid
                } else if !interior_pass && plate_batch {
                    // the plate is the `lice` tile on the trim page (`[file3.lice] license=1` in the .tpg),
                    // whatever its material says
                    &mut model.trim
                } else if let Some(front) = lamp_end(&article_name, lamp_finish, interior_pass, &tri) {
                    if front { &mut model.headlights } else { &mut model.taillights }
                } else {
                    match finish {
                        Finish::Paint => &mut model.paint,
                        Finish::Glass => &mut model.glass,
                        Finish::Interior if outside_cabin => &mut model.interior_outside,
                        Finish::Interior => &mut model.interior,
                        Finish::Trim | Finish::Tyre => &mut model.trim,
                    }
                };
                // lamp lenses, door handles and plates lie on the body surface and flicker against it
                // (or vanish behind it); lift them slightly
                let on_surface = !interior_pass && (article_name.starts_with("Handle") || article_name.starts_with("License"));
                let lift = if on_surface || lamp_end(&article_name, lamp_finish, interior_pass, &tri).is_some() { LAMP_LIFT } else { 0.0 };
                for (q, p) in tri.iter().enumerate() {
                    let p = &[p[0] + n[0] * lift, p[1] + n[1] * lift, p[2] + n[2] * lift];
                    target.positions.push(*p);
                    target.normals.push(n);
                    target.uvs.push(corner_uvs[q]);
                    target.materials.push(b.material);
                    target.articles.push(a as u16);
                    if !is_wheel && !interior_pass && !is_driver {
                        for k in 0..3 {
                            lo[k] = lo[k].min(p[k]);
                            hi[k] = hi[k].max(p[k]);
                        }
                    }
                }
                used = true;
            }
        }
        if used && !outside_cabin {
            model.articles_used += 1;
            if is_wheel {
                let offset = matrix.map(|m| [m[12] * MIRROR_X, m[13], m[14]]).unwrap_or([0.0; 3]);
                model.wheels.push(Wheel { offset, front: article_name.contains("Front"), piece: wheel_piece });
            }
        }
    }
    }
    // the driver has two materials: the batch that sits higher up is the head, the other the suit
    if !driver_parts.is_empty() {
        let mean_y = |p: &Piece| p.positions.iter().map(|v| v[1]).sum::<f32>() / p.positions.len().max(1) as f32;
        driver_parts.sort_by(|a, b| mean_y(&b.1).total_cmp(&mean_y(&a.1)));
        let mut parts = driver_parts.into_iter();
        let first = parts.next().map(|(_, p)| p);
        match (first, parts.next().map(|(_, p)| p)) {
            (Some(head), Some(suit)) => {
                model.driver_head = head;
                model.driver_suit = suit;
            }
            (Some(only), None) => model.driver_suit = only,
            _ => {}
        }
    }
    if model.paint.positions.is_empty() {
        return Err(format!("no body triangles found for car '{name}' at level {level}"));
    }
    model.size = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    Ok(model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_kinds_map_to_finishes() {
        assert_eq!(finish_for_material("CarExtEnv"), Some(Finish::Paint));
        assert_eq!(finish_for_material("CarWindow"), Some(Finish::Glass));
        assert_eq!(finish_for_material("CarWheel"), Some(Finish::Tyre));
        assert_eq!(finish_for_material("CarInt"), Some(Finish::Interior));
        assert_eq!(finish_for_material("Something"), Some(Finish::Trim));
    }

    #[test]
    fn page_sizes_come_from_the_tpg_file() {
        let tpg = "[header]\r\nnumfiles=3\r\n[tpage1.details]\r\nwidth=256\r\nheight=256\r\n[tpage4.details]\r\nwidth=128\r\n\
                   [tpage5.details]\r\nwidth=64\r\n[file1.details]\r\nmirror=1\r\ntpage=1\r\n[file3.details]\r\ntpage=4\r\n\
                   [file4.details]\r\nwidth=64\r\ntpage=5\r\n[file9.details]\r\ntpage=99\r\n";
        let sizes = tpg_page_sizes(tpg);
        assert_eq!(sizes.get(&0), Some(&256)); // file1 -> embedded archive 0 -> tpage1
        assert_eq!(sizes.get(&2), Some(&128)); // file3 -> archive 2 -> tpage4
        assert_eq!(sizes.get(&3), Some(&64)); // file4 -> archive 3 -> tpage5 (its own width=64 line is ignored)
        assert_eq!(sizes.get(&8), None); // tpage99 has no width
        assert!(tpg_page_sizes("").is_empty());
    }

    const SAMPLE_TPG: &str = "[style1]\nname=Carreracoupe\ncount=2\ngeometry1=9\ntype1=1\ntexture2=22\ntype2=3\n\
        [style6]\nname=Carreracabrio\ncount=3\ngeometry1=7\ntype1=3\ngeometry2=37\ntype2=1\ntexture3=13\ntype3=2\n\
        [file1.top]\ncount=2\ngeometry1=7\ntype1=0\ngeometry2=7\ntype2=1\n\
        [file1.top2]\ncount=1\ngeometry1=7\ntype1=3\n\
        [file1.det3]\ncount=1\ntexture1=22\ntype1=3\n\
        [file1.decr]\nracedecal=1\n\
        [file1.details]\nmirror=1\n\
        [file3.whg2]\nfrontend=0\ntestdrive=0\ncount=1\ntexture1=13\ntype1=2\n\
        [file3.whf2]\nfrontend=1\ntestdrive=0\ncount=1\ntexture1=13\ntype1=2\n";

    #[test]
    fn parses_styles_and_tile_rules() {
        let tpg = parse_tpg(SAMPLE_TPG);
        assert_eq!(tpg.styles.len(), 6); // style6 exists, the gap is filled with empty styles
        assert_eq!(tpg.styles[0].name, "Carreracoupe");
        assert_eq!(tpg.styles[0].geometry_type(9), 1);
        assert_eq!(tpg.styles[0].texture_type(22), 3);
        assert_eq!(tpg.styles[0].geometry_type(7), 0); // not listed = type 0
        assert_eq!(tpg.styles[5].name, "Carreracabrio");
        assert_eq!(tpg.styles[5].geometry_type(7), 3);
        assert!(tpg.tiles.contains_key(&(1, "top2".to_string())));
        assert!(!tpg.tiles.contains_key(&(1, "details".to_string())));
    }

    #[test]
    fn tiles_follow_the_style() {
        let tpg = parse_tpg(SAMPLE_TPG);
        let (coupe, cabrio) = (&tpg.styles[0], &tpg.styles[5]);
        let rule = |file: u32, tile: &str| tpg.tiles.get(&(file, tile.to_string())).unwrap();
        // roof: the coupe uses piece 7 type 0 -> `top`; the cabrio uses type 3 -> `top2`
        assert!(rule(1, "top").applies(coupe) && !rule(1, "top").applies(cabrio));
        assert!(!rule(1, "top2").applies(coupe) && rule(1, "top2").applies(cabrio));
        // badge lettering follows the texture family
        assert!(rule(1, "det3").applies(coupe) && !rule(1, "det3").applies(cabrio));
        // wheel faces: the in-game ones (frontend=0) are drawn, the front-end ones are not
        assert!(rule(3, "whg2").applies(cabrio) && !rule(3, "whf2").applies(cabrio));
        // race decals are hidden unless the style is a race car
        assert!(!rule(1, "decr").applies(coupe));
        assert!(rule(1, "decr").applies(&Style { name: "GT2 Racecar".into(), ..Default::default() }));
    }

    #[test]
    fn body_alpha_is_a_mask_only_zero_is_a_hole() {
        let mut px = vec![10, 20, 30, 0, 10, 20, 30, 1, 10, 20, 30, 140, 10, 20, 30, 255];
        alpha_to_mask(&mut px);
        assert_eq!(px.iter().skip(3).step_by(4).copied().collect::<Vec<_>>(), vec![0, 255, 255, 255]);
        assert_eq!(&px[0..3], &[10, 20, 30]); // colours are untouched
    }

    #[test]
    fn paint_colours_come_from_the_clr_sections() {
        let clr = "[section1]\r\nstripepackage=2\r\ncolor1.h=51\r\ncolor1.r=93\r\ncolor1.g=94\r\ncolor1.b=89\r\ncolor2.r=255\r\n\
                   [section4]\r\ncolor1.r=255\r\ncolor1.g=0\r\ncolor1.b=0\r\n[section9]\r\ncolor1.r=1\r\n";
        let paint = parse_clr(clr);
        assert_eq!(paint.get(&1), Some(&[93, 94, 89])); // color2 is ignored
        assert_eq!(paint.get(&4), Some(&[255, 0, 0]));
        assert_eq!(paint.get(&9), None); // incomplete section
        assert!(parse_clr("").is_empty());
    }

    #[test]
    fn paint_tint_uses_the_mask() {
        // strongest paint (alpha 224): white becomes the paint colour
        let mut px = vec![200, 200, 200, 224, 200, 200, 200, 255, 200, 200, 200, 0, 200, 200, 200, 112];
        tint_paint_mask(&mut px, [255, 0, 0]);
        assert_eq!(&px[0..4], &[200, 0, 0, 224]); // fully painted red
        assert_eq!(&px[4..8], &[200, 200, 200, 255]); // alpha 255 = not painted
        assert_eq!(&px[8..12], &[200, 200, 200, 0]); // holes untouched
        assert_eq!(&px[12..16], &[200, 100, 100, 112]); // half mask = half way to red
    }

    #[test]
    fn lamps_are_found_by_name_and_end() {
        let front = [[0.5, 0.6, -2.0], [0.6, 0.6, -2.0], [0.5, 0.7, -2.1]];
        let rear = front.map(|p| [p[0], p[1], -p[2]]);
        let side = front.map(|p| [p[0], p[1], -0.7]);
        assert_eq!(lamp_end("HeadLight", Finish::Paint, false, &front), Some(true));
        assert_eq!(lamp_end("Light4", Finish::Paint, false, &front), Some(true));
        assert_eq!(lamp_end("Light2", Finish::Paint, false, &rear), Some(false));
        assert_eq!(lamp_end("Light6", Finish::Paint, false, &side), None); // side marker
        assert_eq!(lamp_end("Body", Finish::Paint, false, &front), None);
        assert_eq!(lamp_end("Light1", Finish::Trim, false, &front), None);
        assert_eq!(lamp_end("Light1", Finish::Paint, true, &front), None);
    }

    #[test]
    fn pages_are_chosen_by_file_name() {
        let porsche = tpg_file_names("[header]\r\nnumfiles=4\r\nfile1=exterior.fsh\r\nfile2=interior.fsh\r\nfile3=extint.fsh\r\nfile4=glass.fsh\r\n");
        let traffic = tpg_file_names("[header]\r\nfile1=Exterior.fsh\r\nfile2=interior.fsh\r\nfile3=shadow.fsh\r\nfile4=driver.fsh\r\n");
        assert_eq!(page_role(2, porsche.get(&2).map(String::as_str)), Some(2));
        assert_eq!(page_role(3, porsche.get(&3).map(String::as_str)), Some(3));
        assert_eq!(page_role(0, traffic.get(&0).map(String::as_str)), Some(0)); // case does not matter
        assert_eq!(page_role(2, traffic.get(&2).map(String::as_str)), None); // shadow
        assert_eq!(page_role(3, traffic.get(&3).map(String::as_str)), None); // driver
        assert_eq!(page_role(2, None), Some(2)); // no .tpg: Porsche order
        assert_eq!(page_role(5, None), None);
    }

    #[test]
    fn traffic_atlas_picks_the_right_page() {
        let opaque = |size: u32| Page { size, rgba: vec![255; (size * size * 4) as usize], has_alpha: false };
        let mut interior = opaque(4);
        for px in interior.rgba.chunks_exact_mut(4).skip(8) {
            px[3] = 0; // bottom half of the interior page is empty
        }
        let tiles = [
            vec![([0.5, 0.0, 1.0, 0.5], "tref".to_string())],
            vec![([0.5, 0.0, 1.0, 0.5], "lit2".to_string()), ([0.0, 0.5, 0.5, 1.0], "glas".to_string())],
        ];
        let at = TrafficAtlas::build(&opaque(4), &interior, tiles);
        let both = [[0.6, 0.1], [0.9, 0.4]];
        assert_eq!(at.half_for(&both, "WheelWell", 1), 0); // tread tile wins over the tail light
        assert_eq!(at.half_for(&both, "Light2", 0), 1);
        assert_eq!(at.half_for(&[[0.1, 0.6], [0.4, 0.9]], "Body", 0), 1); // only the interior has a tile
        assert_eq!(at.half_for(&[[0.6, 0.6], [0.9, 0.9]], "Bottom", 1), 0); // no tile: the opaque page
        assert_eq!(at.place([0.5, 0.5], 1), [0.75, 0.25]);
    }

    #[test]
    fn handles_follow_their_uvs() {
        let uvs = [[0.13, 0.25], [0.25, 0.25], [0.25, 0.31], [0.6, 0.6]];
        let tile = [[32.0 / 256.0, 64.0 / 256.0, 64.0 / 256.0, 80.0 / 256.0]];
        assert!(handle_on_body_page(&[0, 1, 2], &uvs, &tile));
        assert!(!handle_on_body_page(&[0, 1, 3], &uvs, &tile)); // reaches outside the tile
        assert!(!handle_on_body_page(&[0, 1, 2], &uvs, &[])); // no hand tile on this car
        assert!(!handle_on_body_page(&[], &uvs, &tile));
    }

    #[test]
    fn no_style_means_type_zero() {
        let none = Style::default();
        assert_eq!(none.geometry_type(7), 0);
        assert!(parse_tpg("").styles.is_empty());
    }

    #[test]
    fn matrix_translation_is_the_last_row() {
        let mut m = [0.0f32; 16];
        for k in 0..4 {
            m[k * 5] = 1.0;
        }
        m[12] = 1.0;
        m[13] = 2.0;
        m[14] = 3.0;
        assert_eq!(apply_matrix(&m, [10.0, 20.0, 30.0]), [11.0, 22.0, 33.0]);
    }

    #[test]
    fn outward_normal_matches_the_files_winding() {
        // measured on the real data: faces that point up have (b - a) x (c - a) pointing down
        let n = outward_normal([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
        assert!((n[1] - 1.0).abs() < 1e-6 && n[0].abs() < 1e-6 && n[2].abs() < 1e-6);
    }

    #[test]
    fn skipped_names() {
        assert!(is_skipped_name("DecalHood"));
        assert!(is_skipped_name("WheelShadowFront"));
        assert!(!is_skipped_name("LODDriver3")); // drivers are picked by their own type, not skipped
        assert!(!is_skipped_name("Body"));
        assert!(!is_skipped_name("WheelFront"));
        assert!(!is_skipped_name("SideMirror"));
    }

    #[test]
    fn wheel_wells_are_not_wheels() {
        let is_wheel = |n: &str| n.starts_with("WheelFront") || n.starts_with("WheelRear");
        assert!(is_wheel("WheelFront") && is_wheel("WheelRear") && !is_wheel("WheelWell") && !is_wheel("WheelShadowFront"));
    }
}
