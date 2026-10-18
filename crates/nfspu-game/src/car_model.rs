//! Builds the exterior shape of a car from `GameData/CarModel/<name>.crp` in the player's install.
//!
//! Each triangle is sorted into a piece by its material kind (paint, trim, glass, cabin, ...) and
//! carries the texture page its material names; the pages are assembled from the archives embedded
//! in the car file and the shared files next to it, as the car's `.tpg` lays them out (see
//! `docs/provenance/car.md`). The body style picks which variant of each piece is used; the wheel
//! shadow quads are a piece of their own. Units are metres, the car faces -Z and its origin is at ground level.

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
    /// Texture page of each corner (index into `CarModel::pages`), from the triangle's material.
    pub pages: Vec<u8>,
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
    /// The wheel shadow quads (see `is_shadow_name`): upright, on the wheels' outer faces.
    pub shadow: Piece,
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
    /// Every texture page of the car (`[tpageN.details]` is index N - 1), built as the engine does;
    /// `None` for pages without a picture. Pieces point into this by `Piece::pages`.
    pub pages: Vec<Option<Page>>,
    /// For each page, the page whose picture it shows (`sourcetpage`), if not its own.
    pub page_source: Vec<Option<usize>>,
    /// The driver's eye in car space (see `driver_eye`) and the in-car camera's turn in degrees
    /// (x: pitch, positive looks down; y: yaw), from the `.tpg` for the chosen style.
    pub driver_eye: [f32; 3],
    pub driver_rot: [f32; 3],
    /// Height of the engine's car centre above the ground: half the model's height, wheels included.
    pub centre_height: f32,
}

impl CarModel {
    pub fn triangles(&self) -> usize {
        let body = (self.paint.positions.len() + self.trim.positions.len() + self.glass.positions.len() + self.interior.positions.len() + self.cabrio_glass.positions.len() + self.trim_solid.positions.len() + self.paint_inner.positions.len() + self.headlights.positions.len() + self.taillights.positions.len() + self.driver_suit.positions.len() + self.driver_head.positions.len() + self.shadow.positions.len()) / 3;
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

/// Names of articles that are never drawn: the passenger. Decals (badges, gauge faces), needles,
/// the fuel cap and the aerial are drawn: the body style picks their type and the tile rules leave a
/// decal's artwork out (transparent) when the style does not use it.
fn is_skipped_name(name: &str) -> bool {
    name.starts_with("Passenger")
}

/// The wheel shadow quads (`WheelShadowFront`, `WheelShadowRear`): one upright, wheel-sized square
/// just outside each wheel, whose shared shadow page darkens the top of the tyre under the arch.
fn is_shadow_name(name: &str) -> bool {
    name.starts_with("WheelShadow")
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
    /// `tint=`: whether the tile is painted by colour band, overriding its file's setting.
    pub tint: Option<bool>,
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
            let mut rule = TileRule { frontend: flag("frontend"), testdrive: flag("testdrive"), race_decal: flag("racedecal") == Some(1), tint: flag("tint").map(|t| t != 0), ..Default::default() };
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

/// One texture page of a car (`[tpageN.details]` in its `.tpg`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TpageInfo {
    pub width: u32,
    pub height: u32,
    /// `sourcetpage=M`: this page shows page M's picture (0-based) instead of its own. The body
    /// page's twin (`tpage2`, `sourcetpage=1`) is used by the car's other side.
    pub source: Option<usize>,
    /// Pixel format (`bpp=`, written as a decimal code): 1555 (the default), 4444 or 32. It decides
    /// how much of the artwork's alpha survives, see `PageFormat::reduce_alpha`.
    pub format: PageFormat,
}

/// The pixel format of a texture page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PageFormat {
    /// 1-bit alpha (`bpp=1555`, also the default): a texel is either see-through (alpha 0) or solid.
    #[default]
    A1,
    /// 4-bit alpha (`bpp=4444`).
    A4,
    /// Full 8-bit alpha (`bpp=32`).
    A8,
}

impl PageFormat {
    fn from_code(code: u32) -> Self {
        match code {
            4444 => PageFormat::A4,
            32 => PageFormat::A8,
            _ => PageFormat::A1,
        }
    }

    /// The alpha each texel keeps once stored in this format: on 1555 pages any alpha above 0 is
    /// solid, which is why the body and cabin artwork, whose alpha holds other data (the paint
    /// mask), shows as solid in the game.
    pub fn reduce_alpha(self, rgba: &mut [u8]) {
        for px in rgba.chunks_exact_mut(4) {
            px[3] = match self {
                PageFormat::A1 => if px[3] > 0 { 255 } else { 0 },
                PageFormat::A4 => (px[3] >> 4) * 17,
                PageFormat::A8 => px[3],
            };
        }
    }
}

/// One texture file of a car (`fileN=` in `[header]`, `[fileN.details]`).
#[derive(Clone, Debug, PartialEq)]
pub struct FileInfo {
    /// Lower-case file name, e.g. `exterior.fsh`.
    pub name: String,
    /// The page (0-based) its tiles are pasted on, at `offset` pixels.
    pub tpage: Option<usize>,
    pub offset: [u32; 2],
    /// `global=K`: the file is not embedded in the car but shared by all cars; 1 shadow,
    /// 2 intglass, 3 suit, 4 head, 5 cabrio (the order of the engine's list of shared files).
    pub global: u32,
    /// `tint=0` turns off the colour bands for the file's tiles (see `tint_bands`); on by default.
    pub tint: bool,
}

impl Default for FileInfo {
    fn default() -> Self {
        FileInfo { name: String::new(), tpage: None, offset: [0, 0], global: 0, tint: true }
    }
}

/// The texture layout of a car's `.tpg`: its pages and which files fill them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TpgLayout {
    pub tpages: Vec<TpageInfo>,
    /// `files[N - 1]` is `fileN`; embedded archive N - 1 holds its tiles unless it is global.
    pub files: Vec<FileInfo>,
}

/// File names of the shared car textures, by their `global=` number.
pub const GLOBAL_FILES: [&str; 5] = ["shadow.fsh", "intglass.fsh", "suit.fsh", "head.fsh", "cabrio.fsh"];

pub fn parse_tpg_layout(text: &str) -> TpgLayout {
    let mut layout = TpgLayout::default();
    let mut section = String::new();
    let grow_t = |v: &mut Vec<TpageInfo>, n: usize| {
        if v.len() < n {
            v.resize(n, TpageInfo::default());
        }
    };
    let grow_f = |v: &mut Vec<FileInfo>, n: usize| {
        if v.len() < n {
            v.resize(n, FileInfo::default());
        }
    };
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.to_ascii_lowercase();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let (key, value) = (key.trim().to_ascii_lowercase(), value.trim());
        let num = value.parse::<u32>().ok();
        let index = |prefix: &str| section.strip_prefix(prefix).and_then(|r| r.strip_suffix(".details")).and_then(|n| n.parse::<usize>().ok()).filter(|n| *n >= 1);
        if section == "header" {
            if let Some(n) = key.strip_prefix("file").and_then(|n| n.parse::<usize>().ok()).filter(|n| *n >= 1) {
                grow_f(&mut layout.files, n);
                layout.files[n - 1].name = value.to_ascii_lowercase();
            }
        } else if let (Some(t), Some(v)) = (index("tpage"), num) {
            grow_t(&mut layout.tpages, t);
            let page = &mut layout.tpages[t - 1];
            match key.as_str() {
                "width" => page.width = v,
                "height" => page.height = v,
                "sourcetpage" if v >= 1 && v as usize != t => page.source = Some(v as usize - 1),
                "bpp" => page.format = PageFormat::from_code(v),
                _ => {}
            }
        } else if let (Some(f), Some(v)) = (index("file"), num) {
            grow_f(&mut layout.files, f);
            let file = &mut layout.files[f - 1];
            if key == "tint" {
                file.tint = v != 0;
            }
            match key.as_str() {
                "tpage" if v >= 1 => file.tpage = Some(v as usize - 1),
                "offsetx" => file.offset[0] = v,
                "offsety" => file.offset[1] = v,
                "global" => file.global = v,
                _ => {}
            }
        }
    }
    layout
}

/// The in-car camera from a car's `.tpg` header: `drivercampos` (metres) and `drivercamrot`
/// (degrees), replaced by `drivercamposstyleK` / `drivercamrotstyleK` when the body style (K counted
/// from 0) has its own. Without any, the engine uses position (-0.36, 0.4, -0.25) and no rotation.
/// The position is in the engine's car frame: origin at the car's centre (half its height above the
/// model's origin), +Z forward, -X the driver's (left) side; see `driver_eye`.
pub fn driver_camera(tpg: &str, style_number: usize) -> ([f32; 3], [f32; 3]) {
    let mut values: std::collections::HashMap<String, [f32; 3]> = std::collections::HashMap::new();
    let mut in_header = false;
    for line in tpg.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_header = line.eq_ignore_ascii_case("[header]");
            continue;
        }
        let Some((key, value)) = line.split_once('=').filter(|_| in_header) else { continue };
        let v: Vec<f32> = value.split(',').filter_map(|s| s.trim().parse().ok()).collect();
        if let [x, y, z] = v[..] {
            values.insert(key.trim().to_ascii_lowercase(), [x, y, z]);
        }
    }
    let k = style_number.saturating_sub(1);
    let pos = values.get(&format!("drivercamposstyle{k}")).or_else(|| values.get("drivercampos")).copied().unwrap_or([-0.36, 0.4, -0.25]);
    let rot = values.get(&format!("drivercamrotstyle{k}")).or_else(|| values.get("drivercamrot")).copied().unwrap_or([0.0; 3]);
    (pos, rot)
}

/// Where the driver's eye sits in the viewer's car space (origin on the ground, car facing -Z,
/// x already mirrored, see `MIRROR_X`), from the `.tpg` camera position and the car's half height.
/// The engine's car frame is the model turned half a turn (its +Z is forward) and raised by half the
/// model's height, so z changes sign and x keeps it (the half turn and the mirror cancel).
pub fn driver_eye(campos: [f32; 3], half_height: f32) -> [f32; 3] {
    [campos[0] * -MIRROR_X, half_height + campos[1], -campos[2]]
}

/// What decides which tiles of a file are drawn: the body style, or (suit and head files) the driver.
pub struct PageChoice<'a> {
    pub rules: &'a Tpg,
    pub style: &'a Style,
    /// Driver type (1 racing suit, 2-10 casual, 0 none).
    pub driver: u8,
    /// The colour of each band (see `tint_bands`), from the paint scheme; `None` leaves the
    /// artwork's own colours (cars without a `.clr`, such as traffic).
    pub bands: Option<[[u8; 3]; BANDS]>,
}

/// Builds every texture page of a car as the engine does: each page starts empty and each file's
/// kept tiles are pasted on its page at the file's offset; a page with `sourcetpage` shows its source
/// page's picture. Embedded archive N - 1 holds `fileN`; shared (`global`) files are read from
/// `dir`. Tiles are painted by colour band (`tint_bands`) unless their file or tile says `tint=0`
/// or the page is a 4444 page (the engine copies those unchanged), and every page keeps only the
/// alpha its pixel format can hold (`PageFormat::reduce_alpha`). Returns one entry per page (`None`
/// if it has no size).
pub fn build_tpages(layout: &TpgLayout, embedded: &[(u16, &[u8])], dir: &std::path::Path, choice: &PageChoice) -> Vec<Option<Page>> {
    let mut pages: Vec<Option<Page>> = layout
        .tpages
        .iter()
        .map(|t| (t.width > 0 && t.width == t.height).then(|| Page { size: t.width, rgba: vec![0; (t.width * t.width * 4) as usize], has_alpha: true }))
        .collect();
    let driver_style = Style { geometry: [(9u32, choice.driver as u32)].into_iter().collect(), ..Default::default() };
    for (f, file) in layout.files.iter().enumerate() {
        let Some(t) = file.tpage.filter(|t| layout.tpages.get(*t).is_some_and(|p| p.source.is_none())) else { continue };
        let Some(page) = pages[t].as_mut() else { continue };
        let shared;
        let blob: &[u8] = if file.global > 0 {
            let Some(name) = GLOBAL_FILES.get(file.global as usize - 1) else { continue };
            let Some(path) = find_case_insensitive(dir, name) else { continue };
            let Ok(bytes) = fs::read(path) else { continue };
            shared = bytes;
            &shared
        } else {
            match embedded.iter().find(|(i, _)| *i as usize == f) {
                Some((_, b)) => b,
                None => continue,
            }
        };
        let Ok(archive) = shpi::parse(blob) else { continue };
        let is_driver = matches!(file.name.as_str(), "suit.fsh" | "head.fsh");
        if is_driver && choice.driver == 0 {
            continue;
        }
        let style = if is_driver { &driver_style } else { choice.style };
        let keep = tile_flags(&archive, choice.rules, f as u16, style);
        // which kept tiles get painted: the tile's own `tint=` or else the file's
        let format = layout.tpages[t].format;
        let tinted: Vec<bool> = archive
            .entries
            .iter()
            .map(|e| {
                let own = choice.rules.tiles.get(&(f as u32 + 1, e.name.trim().to_ascii_lowercase())).and_then(|r| r.tint);
                choice.bands.is_some() && format != PageFormat::A4 && own.unwrap_or(file.tint)
            })
            .collect();
        for paint in [false, true] {
            let these: Vec<bool> = keep.iter().zip(&tinted).map(|(k, t)| *k && *t == paint).collect();
            if !these.contains(&true) {
                continue;
            }
            let mut rgba = shpi::compose_page_filtered(blob, &archive, page.size, &these);
            if let (true, Some(bands)) = (paint, &choice.bands) {
                tint_bands(&mut rgba, bands);
            }
            format.reduce_alpha(&mut rgba);
            // paste at the file's offset; transparent texels leave what is already there
            let s = page.size as usize;
            let [ox, oy] = [file.offset[0] as usize, file.offset[1] as usize];
            for y in 0..s.saturating_sub(oy) {
                for x in 0..s.saturating_sub(ox) {
                    let src = (y * s + x) * 4;
                    if rgba[src + 3] == 0 {
                        continue;
                    }
                    let dst = ((y + oy) * s + x + ox) * 4;
                    page.rgba[dst..dst + 4].copy_from_slice(&rgba[src..src + 4]);
                }
            }
        }
    }
    for page in pages.iter_mut().flatten() {
        page.has_alpha = page.rgba.chunks_exact(4).any(|p| p[3] < 255);
    }
    for (t, info) in layout.tpages.iter().enumerate() {
        if let Some(s) = info.source.filter(|s| *s != t) {
            pages[t] = pages.get(s).cloned().flatten();
        }
    }
    pages
}

/// A file in `dir` whose name matches `name` ignoring case (the install mixes `INTGLASS.FSH` and `Cabrio.fsh`).
fn find_case_insensitive(dir: &std::path::Path, name: &str) -> Option<PathBuf> {
    let direct = dir.join(name);
    if direct.exists() {
        return Some(direct);
    }
    fs::read_dir(dir).ok()?.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name)))
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

/// One paint scheme `[sectionN]` of a car's `.clr` file: four colours (`color1` is the body) and
/// the stripe package (`stripepackage`, counted from 0) that decides where colours 2-4 go.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PaintScheme {
    pub colours: [[u8; 3]; 4],
    pub stripe_package: usize,
}

/// Every paint scheme of a `.clr` file, by section number. Sections without a complete `color1`
/// are left out; missing other colours are black.
pub fn parse_clr(text: &str) -> std::collections::HashMap<usize, PaintScheme> {
    let mut out = std::collections::HashMap::new();
    let mut current: Option<usize> = None;
    let mut scheme = PaintScheme::default();
    let mut has_body = [false; 3];
    let mut flush = |section: Option<usize>, scheme: &PaintScheme, has_body: &[bool; 3]| {
        if let (Some(n), [true, true, true]) = (section, has_body) {
            out.insert(n, *scheme);
        }
    };
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            flush(current, &scheme, &has_body);
            current = name.to_ascii_lowercase().strip_prefix("section").and_then(|n| n.parse().ok());
            scheme = PaintScheme::default();
            has_body = [false; 3];
        } else if let Some((key, value)) = line.split_once('=') {
            let key = key.trim().to_ascii_lowercase();
            if key == "stripepackage" {
                scheme.stripe_package = value.trim().parse().unwrap_or(0);
                continue;
            }
            let Some((colour, channel)) = key.strip_prefix("color").and_then(|r| r.split_once('.')) else { continue };
            let (Ok(colour), Ok(value)) = (colour.parse::<usize>(), value.trim().parse::<u8>()) else { continue };
            let Some(k) = ["r", "g", "b"].iter().position(|c| *c == channel) else { continue };
            if (1..=4).contains(&colour) {
                scheme.colours[colour - 1][k] = value;
                if colour == 1 {
                    has_body[k] = true;
                }
            }
        }
    }
    flush(current, &scheme, &has_body);
    out
}

/// How many colour bands a car's artwork can use.
pub const BANDS: usize = 18;

/// The stripe packages of `CarModel/packages.ini`: for each package (`[packageN]`, N from 1), which
/// of the scheme's four colours (0 = `color1`) each band takes. Unlisted bands use colour 0, except
/// bands 14 and 15, which use colour 3 (`color4`); that is the engine's default.
pub fn parse_packages(text: &str) -> Vec<[u8; BANDS]> {
    let mut default = [0u8; BANDS];
    default[14] = 3;
    default[15] = 3;
    let mut out: Vec<[u8; BANDS]> = Vec::new();
    let mut current: Option<usize> = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            current = name.to_ascii_lowercase().strip_prefix("package").and_then(|n| n.parse::<usize>().ok()).filter(|n| *n >= 1);
            if let Some(n) = current {
                if out.len() < n {
                    out.resize(n, default);
                }
            }
        } else if let (Some(n), Some((key, value))) = (current, line.split_once('=')) {
            let band = key.trim().to_ascii_lowercase().strip_prefix("band").and_then(|b| b.parse::<usize>().ok());
            if let (Some(band), Ok(colour)) = (band, value.trim().parse::<u8>()) {
                if band < BANDS && colour < 4 {
                    out[n - 1][band] = colour;
                }
            }
        }
    }
    out
}

/// The colour of every band for a paint scheme: each band takes the scheme colour its stripe
/// package names (the default bands when the package does not exist).
pub fn band_colours(scheme: &PaintScheme, packages: &[[u8; BANDS]]) -> [[u8; 3]; BANDS] {
    let mut table = [0u8; BANDS];
    table[14] = 3;
    table[15] = 3;
    let table = packages.get(scheme.stripe_package).copied().unwrap_or(table);
    table.map(|c| scheme.colours[c as usize])
}

/// Which colour band a texel of car artwork belongs to, from its alpha: the alpha is split into 21
/// steps (`(alpha + 1) * 20 / 256`); steps 0, 10 and 20 keep the artwork's own colour (`None`),
/// steps 11-19 are bands 8 down to 0 and steps 1-9 bands 17 down to 9. Read from the engine's tile
/// copy; e.g. the body panels (alpha about 200) are band 4 and the cabin trim (alpha about 50)
/// band 14, which the default package paints in `color4`.
pub fn alpha_band(alpha: u8) -> Option<usize> {
    let step = (alpha as usize + 1) * 20 / 256;
    match step {
        0 | 10 | 20 => None,
        11..=19 => Some(19 - step),
        _ => Some(18 - step),
    }
}

/// Paints artwork the way the engine does when it copies a tile onto a car page: every texel in a
/// colour band becomes that band's colour scaled by the texel's brightness (its red channel; the
/// artwork is grey), and is solid unless its alpha is 0. Texels outside the bands are untouched.
pub fn tint_bands(rgba: &mut [u8], bands: &[[u8; 3]; BANDS]) {
    for px in rgba.chunks_exact_mut(4) {
        let Some(band) = alpha_band(px[3]) else { continue };
        let shade = px[0] as u32;
        for k in 0..3 {
            px[k] = (bands[band][k] as u32 * shade / 256) as u8;
        }
        px[3] = 255;
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
    // the texture page each material is drawn with (see `crp::Material::page`)
    let material_pages: std::collections::HashMap<u16, usize> = crp.materials().into_iter().map(|m| (m.index, m.page as usize)).collect();

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
        shadow: Piece::default(),
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
        pages: Vec::new(),
        page_source: Vec::new(),
        driver_eye: [0.0; 3],
        driver_rot: [0.0; 3],
        centre_height: 0.0,
    };
    // The car's .tpg text file lists its texture pages, the files that fill them (most embedded
    // in the car file), the body styles and which tiles each style draws.
    let tpg = fs::read_to_string(car_dir()?.join(format!("{name}.tpg"))).unwrap_or_default();
    let rules = parse_tpg(&tpg);
    let style = rules.styles.get(style_number.saturating_sub(1)).or_else(|| rules.styles.first()).cloned().unwrap_or_default();
    model.style_name = style.name.clone();
    model.style_count = rules.styles.len();
    // every texture page, as the engine builds them (see `build_tpages`)
    let layout = parse_tpg_layout(&tpg);
    // the paint scheme's colours, spread over the colour bands by its stripe package
    let packages = parse_packages(&fs::read_to_string(car_dir()?.join("packages.ini")).unwrap_or_default());
    let bands = parse_clr(&fs::read_to_string(car_dir()?.join(format!("{name}.clr"))).unwrap_or_default()).get(&paint).map(|scheme| band_colours(scheme, &packages));
    let embedded = crp.embedded_archives();
    model.pages = build_tpages(&layout, &embedded, &car_dir()?, &PageChoice { rules: &rules, style: &style, driver, bands });
    model.page_source = layout.tpages.iter().map(|t| t.source).collect();
    let has_page: Vec<bool> = model.pages.iter().map(Option::is_some).collect();
    // a material whose page is missing falls back to the page of its kind's usual file
    let file_page = |file: &str| layout.files.iter().find(|f| f.name == file).and_then(|f| f.tpage).filter(|t| has_page.get(*t) == Some(&true));
    let fallback_page = |finish: Finish| -> usize {
        let file = match finish {
            Finish::Paint => "exterior.fsh",
            Finish::Interior => "interior.fsh",
            Finish::Glass => "glass.fsh",
            Finish::Trim | Finish::Tyre => "extint.fsh",
        };
        file_page(file).or_else(|| file_page("exterior.fsh")).unwrap_or(0)
    };
    let (campos, camrot) = driver_camera(&tpg, style_number);
    model.driver_rot = camrot;
    let file_names = tpg_file_names(&tpg);
    // Traffic cars (no trim page) only go down to level 3, which is very crude (32 body triangles
    // and no lamps on eas1), and are low-poly anyway: use their level 1.
    let traffic = !file_names.is_empty() && !file_names.values().any(|n| n == "extint.fsh");
    let level = if traffic { 1 } else { level };
    // the pages by their usual job, for the examples that look at them
    let pages = model.pages.clone();
    let page_of = |file: &str| file_page(file).and_then(|t| pages[t].clone());
    model.exterior = page_of("exterior.fsh");
    model.interior_page = page_of("interior.fsh");
    model.extint = page_of("extint.fsh").or_else(|| model.exterior.clone());
    model.glass_page = page_of("glass.fsh").or_else(|| model.exterior.clone());
    model.cabrio_page = page_of("cabrio.fsh");
    model.suit_page = page_of("suit.fsh").or_else(|| page_of("driver.fsh"));
    model.head_page = page_of("head.fsh").or_else(|| page_of("driver.fsh"));
    // the soft top's rear window is drawn from the shared cabrio file's page
    let cabrio_page = file_page("cabrio.fsh");
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
        } else if base.type_index as u32 != style.geometry_type(base.geometry as u32) || is_skipped_name(&article_name) || (interior_pass && (is_hidden_in_cabin(&article_name) || is_shadow_name(&article_name))) {
            continue;
        }
        let is_shadow = is_shadow_name(&article_name);
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
            if !is_driver && !is_shadow && (finish == Finish::Interior) != interior_pass {
                continue;
            }
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
            let plate_batch = article_name.starts_with("License");
            // traffic lamps are trim-kind batches; treat them like body lamps
            let lamp_finish = if traffic && finish == Finish::Trim { Finish::Paint } else { finish };
            // the texture page comes from the material; a page the car does not have falls back to
            // the usual page of the material's kind
            let page = material_pages.get(&b.material).copied().filter(|p| has_page.get(*p) == Some(&true)).unwrap_or_else(|| fallback_page(finish));
            // the soft top's rear window is drawn from the shared cabrio page, see-through
            let cabrio_window = !interior_pass && cabrio_page == Some(page);
            for t in 0..b.vertex_idx.len() / 3 {
                let corner_uvs: Vec<[f32; 2]> = (0..3).map(|q| mesh.uvs[b.uv_idx[3 * t + q] as usize]).collect();
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
                } else if is_shadow {
                    &mut model.shadow
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
                // lamp lenses, door handles, plates and decals (badges, gauge faces) lie on the surface
                // under them and flicker against it (or vanish behind it); lift them slightly
                let on_surface = (!interior_pass && (article_name.starts_with("Handle") || article_name.starts_with("License"))) || article_name.starts_with("Decal");
                let lift = if on_surface || lamp_end(&article_name, lamp_finish, interior_pass, &tri).is_some() { LAMP_LIFT } else { 0.0 };
                for (q, p) in tri.iter().enumerate() {
                    let p = &[p[0] + n[0] * lift, p[1] + n[1] * lift, p[2] + n[2] * lift];
                    target.positions.push(*p);
                    target.normals.push(n);
                    target.uvs.push(corner_uvs[q]);
                    target.materials.push(b.material);
                    target.articles.push(a as u16);
                    target.pages.push(page as u8);
                    if !is_wheel && !interior_pass && !is_driver && !is_shadow {
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
    // the engine's car centre is half the model's height (wheels included, so from the ground) up
    model.centre_height = (hi[1] - lo[1].min(0.0)) / 2.0;
    model.driver_eye = driver_eye(campos, model.centre_height);
    Ok(model)
}

/// A soft ground shadow for a car: an alpha mask over a square of the ground plane (model space,
/// x and z), drawn as one blended quad under the car. Our own stand-in for the engine's projected
/// shadow texture (see `docs/provenance/car.md`, "Ground shadow").
#[derive(Clone, Debug)]
pub struct GroundShadow {
    /// Corner of the square with the smallest x and z, and the one with the largest.
    pub min: [f32; 2],
    pub max: [f32; 2],
    /// Mask resolution (square) and one alpha value per texel, row by row from `min` in z.
    pub size: u32,
    pub alpha: Vec<u8>,
}

/// Texels across the shadow mask.
const SHADOW_SIZE: usize = 64;
/// Ground left around the car's outline inside the mask (metres), room for the soft edge.
const SHADOW_MARGIN: f32 = 0.45;
/// Darkest alpha of the shadow (0-255), as a blend on screen values.
const SHADOW_ALPHA: f32 = 150.0;
/// The outline is grown by this many texels before blurring, so the soft edge shows round the car.
const SHADOW_GROW: usize = 2;
/// Box blur radius in texels and number of passes (three passes look close to a Gaussian).
const SHADOW_BLUR: usize = 2;
const SHADOW_PASSES: usize = 3;

impl CarModel {
    /// The car's outline seen from straight above, softened: every body, glass and wheel triangle
    /// is filled into the mask, which is then grown a little and blurred.
    pub fn ground_shadow(&self) -> Option<GroundShadow> {
        let mut tris: Vec<[[f32; 2]; 3]> = Vec::new();
        for piece in [&self.paint, &self.paint_inner, &self.trim, &self.trim_solid, &self.glass, &self.cabrio_glass, &self.headlights, &self.taillights] {
            for t in piece.positions.chunks_exact(3) {
                tris.push([[t[0][0], t[0][2]], [t[1][0], t[1][2]], [t[2][0], t[2][2]]]);
            }
        }
        for w in &self.wheels {
            for t in w.piece.positions.chunks_exact(3) {
                let at = |p: [f32; 3]| [p[0] + w.offset[0], p[2] + w.offset[2]];
                tris.push([at(t[0]), at(t[1]), at(t[2])]);
            }
        }
        if tris.is_empty() {
            return None;
        }
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for p in tris.iter().flatten() {
            for k in 0..2 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        // a square mask (texels the same size both ways) centred on the outline
        let half = (hi[0] - lo[0]).max(hi[1] - lo[1]) / 2.0 + SHADOW_MARGIN;
        let centre = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
        let min = [centre[0] - half, centre[1] - half];
        let n = SHADOW_SIZE;
        let texel = 2.0 * half / n as f32;
        let edge = |a: [f32; 2], b: [f32; 2], p: [f32; 2]| (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
        let mut cover = vec![0.0f32; n * n];
        for t in &tris {
            let px = t.map(|p| [(p[0] - min[0]) / texel, (p[1] - min[1]) / texel]);
            let area = edge(px[0], px[1], px[2]);
            if area.abs() < 1e-6 {
                continue;
            }
            let (mut x0, mut x1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
            for p in &px {
                x0 = x0.min(p[0]);
                x1 = x1.max(p[0]);
                y0 = y0.min(p[1]);
                y1 = y1.max(p[1]);
            }
            let (x0, y0) = (x0.floor().max(0.0) as usize, y0.floor().max(0.0) as usize);
            let (x1, y1) = ((x1.ceil().max(0.0) as usize).min(n - 1), (y1.ceil().max(0.0) as usize).min(n - 1));
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let c = [x as f32 + 0.5, y as f32 + 0.5];
                    // either winding: the centre is inside when all three edges agree with the area's sign
                    if edge(px[1], px[2], c) * area >= 0.0 && edge(px[2], px[0], c) * area >= 0.0 && edge(px[0], px[1], c) * area >= 0.0 {
                        cover[y * n + x] = 1.0;
                    }
                }
            }
        }
        cover = grow(&cover, n, SHADOW_GROW);
        for _ in 0..SHADOW_PASSES {
            cover = box_blur(&cover, n, SHADOW_BLUR);
        }
        let alpha = cover.iter().map(|c| (c * SHADOW_ALPHA).round() as u8).collect();
        Some(GroundShadow { min, max: [centre[0] + half, centre[1] + half], size: n as u32, alpha })
    }
}

/// Each texel takes the largest value within `r` texels (a square neighbourhood).
fn grow(src: &[f32], n: usize, r: usize) -> Vec<f32> {
    let mut out = vec![0.0; n * n];
    for y in 0..n {
        for x in 0..n {
            let mut m = 0.0f32;
            for j in y.saturating_sub(r)..=(y + r).min(n - 1) {
                for i in x.saturating_sub(r)..=(x + r).min(n - 1) {
                    m = m.max(src[j * n + i]);
                }
            }
            out[y * n + x] = m;
        }
    }
    out
}

/// One horizontal and one vertical box blur of an `n` x `n` grid (outside the grid counts as 0).
fn box_blur(src: &[f32], n: usize, r: usize) -> Vec<f32> {
    let w = (2 * r + 1) as f32;
    let mut tmp = vec![0.0; n * n];
    for y in 0..n {
        for x in 0..n {
            tmp[y * n + x] = (x.saturating_sub(r)..=(x + r).min(n - 1)).map(|i| src[y * n + i]).sum::<f32>() / w;
        }
    }
    let mut out = vec![0.0; n * n];
    for y in 0..n {
        for x in 0..n {
            out[y * n + x] = (y.saturating_sub(r)..=(y + r).min(n - 1)).map(|j| tmp[j * n + x]).sum::<f32>() / w;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ground_shadow_covers_the_outline_softly() {
        let mut paint = Piece::default();
        // a 2 m x 4 m rectangle, as two triangles of opposite windings
        for p in [[-1.0, 0.5, -2.0], [1.0, 0.5, -2.0], [1.0, 0.5, 2.0], [-1.0, 0.5, -2.0], [-1.0, 0.5, 2.0], [1.0, 0.5, 2.0]] {
            paint.positions.push(p);
        }
        let model = CarModel { paint, ..empty_model() };
        let s = model.ground_shadow().unwrap();
        let n = s.size as usize;
        let at = |x: f32, z: f32| {
            let i = ((x - s.min[0]) / (s.max[0] - s.min[0]) * n as f32) as usize;
            let j = ((z - s.min[1]) / (s.max[1] - s.min[1]) * n as f32) as usize;
            s.alpha[j * n + i]
        };
        assert_eq!(at(0.0, 0.0), SHADOW_ALPHA as u8, "full under the middle");
        assert_eq!(at(2.4, 0.0), 0, "nothing well outside");
        // grown by two texels, so still dark at the outline and soft further out
        assert!(at(1.0, 0.0) > 100, "dark at the outline: {}", at(1.0, 0.0));
        assert!(at(1.3, 0.0) > 10 && at(1.3, 0.0) < 100, "soft outside it: {}", at(1.3, 0.0));
        assert!((s.max[1] - s.min[1] - (4.0 + 2.0 * SHADOW_MARGIN)).abs() < 1e-4);
    }

    fn empty_model() -> CarModel {
        CarModel {
            name: String::new(),
            paint: Piece::default(),
            trim: Piece::default(),
            glass: Piece::default(),
            interior: Piece::default(),
            cabrio_glass: Piece::default(),
            trim_solid: Piece::default(),
            paint_inner: Piece::default(),
            interior_outside: Piece::default(),
            headlights: Piece::default(),
            taillights: Piece::default(),
            driver_suit: Piece::default(),
            driver_head: Piece::default(),
            shadow: Piece::default(),
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
            article_names: Vec::new(),
            style_name: String::new(),
            style_count: 0,
            pages: Vec::new(),
            page_source: Vec::new(),
            driver_eye: [0.0; 3],
            driver_rot: [0.0; 3],
            centre_height: 0.0,
        }
    }

    #[test]
    fn material_kinds_map_to_finishes() {
        assert_eq!(finish_for_material("CarExtEnv"), Some(Finish::Paint));
        assert_eq!(finish_for_material("CarWindow"), Some(Finish::Glass));
        assert_eq!(finish_for_material("CarWheel"), Some(Finish::Tyre));
        assert_eq!(finish_for_material("CarInt"), Some(Finish::Interior));
        assert_eq!(finish_for_material("Something"), Some(Finish::Trim));
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
    fn page_formats_keep_their_alpha() {
        // 1555 pages (the default): only alpha 0 is see-through, so the body's paint mask is solid
        let mut px = vec![10, 20, 30, 0, 10, 20, 30, 1, 10, 20, 30, 140, 10, 20, 30, 255];
        PageFormat::A1.reduce_alpha(&mut px);
        assert_eq!(px.iter().skip(3).step_by(4).copied().collect::<Vec<_>>(), vec![0, 255, 255, 255]);
        assert_eq!(&px[0..3], &[10, 20, 30]); // colours are untouched
        let mut px = vec![0, 0, 0, 140, 0, 0, 0, 15];
        PageFormat::A4.reduce_alpha(&mut px);
        assert_eq!((px[3], px[7]), (136, 0));
        let mut px = vec![0, 0, 0, 140];
        PageFormat::A8.reduce_alpha(&mut px);
        assert_eq!(px[3], 140);
        assert_eq!(parse_tpg_layout("[tpage1.details]
bpp=4444
[tpage2.details]
width=8
").tpages.iter().map(|t| t.format).collect::<Vec<_>>(), vec![PageFormat::A4, PageFormat::A1]);
    }

    #[test]
    fn paint_schemes_come_from_the_clr_sections() {
        let clr = "[section1]\r\nstripepackage=2\r\ncolor1.h=51\r\ncolor1.r=93\r\ncolor1.g=94\r\ncolor1.b=89\r\ncolor4.r=197\r\ncolor4.g=161\r\ncolor4.b=98\r\n\
                   [section4]\r\ncolor1.r=255\r\ncolor1.g=0\r\ncolor1.b=0\r\n[section9]\r\ncolor1.r=1\r\n";
        let paint = parse_clr(clr);
        assert_eq!(paint[&1], PaintScheme { colours: [[93, 94, 89], [0; 3], [0; 3], [197, 161, 98]], stripe_package: 2 });
        assert_eq!(paint[&4].colours[0], [255, 0, 0]);
        assert_eq!(paint.get(&9), None); // incomplete body colour
        assert!(parse_clr("").is_empty());
    }

    #[test]
    fn alpha_picks_the_colour_band() {
        assert_eq!(alpha_band(0), None); // see-through
        assert_eq!(alpha_band(255), None); // own colour (lamp lenses, soft top)
        assert_eq!(alpha_band(130), None);
        assert_eq!(alpha_band(200), Some(4)); // body panels
        assert_eq!(alpha_band(140), Some(8)); // side panel
        assert_eq!(alpha_band(243), Some(0));
        assert_eq!(alpha_band(50), Some(15)); // cabin trim: color4 by default
        assert_eq!(alpha_band(12), Some(17));
    }

    #[test]
    fn packages_spread_the_scheme_colours() {
        let packages = parse_packages("[header]\r\ncount=3\r\n[package1]\r\n[package3]\r\nband1=1\r\nband7=2\r\n");
        assert_eq!(packages.len(), 3);
        assert_eq!(packages[0][14], 3); // default: bands 14 and 15 take color4
        assert_eq!(packages[2][1], 1);
        let scheme = PaintScheme { colours: [[255, 0, 0], [0, 72, 106], [255, 255, 255], [197, 161, 98]], stripe_package: 2 };
        let bands = band_colours(&scheme, &packages);
        assert_eq!((bands[0], bands[1], bands[7], bands[14]), ([255, 0, 0], [0, 72, 106], [255, 255, 255], [197, 161, 98]));
        // a white body texel (band 4) becomes the body colour, a band-15 texel the cabin colour
        let mut px = vec![255, 255, 255, 200, 128, 128, 128, 50, 9, 9, 9, 255];
        tint_bands(&mut px, &bands);
        assert_eq!(px, vec![254, 0, 0, 255, 98, 80, 49, 255, 9, 9, 9, 255]);
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
    fn tpg_layout_lists_pages_and_files() {
        let tpg = "[header]\r\nnumfiles=3\r\nfile1=exterior.fsh\r\nfile2=interior.fsh\r\nfile3=Cabrio.fsh\r\n\
                   [tpage1.details]\r\nwidth=256\r\nheight=256\r\nmirrortpage=2\r\n[tpage2.details]\r\nwidth=256\r\nheight=256\r\nsourcetpage=1\r\n\
                   [tpage3.details]\r\nwidth=32\r\nheight=32\r\nbpp=4444\r\n[file1.details]\r\nmirror=1\r\ntpage=1\r\noffsetx=0\r\n\
                   [file2.details]\r\ntpage=2\r\noffsetx=8\r\noffsety=16\r\n[file3.details]\r\ntpage=3\r\nglobal=5\r\ntint=0\r\n";
        let l = parse_tpg_layout(tpg);
        assert_eq!(l.tpages.len(), 3);
        assert_eq!(l.tpages[0], TpageInfo { width: 256, height: 256, source: None, format: PageFormat::A1 });
        assert_eq!(l.tpages[2].format, PageFormat::A4);
        assert_eq!(l.tpages[1].source, Some(0)); // the body page's twin
        assert_eq!(l.files[0], FileInfo { name: "exterior.fsh".into(), tpage: Some(0), ..Default::default() });
        assert_eq!(l.files[1].offset, [8, 16]);
        assert_eq!(l.files[2], FileInfo { name: "cabrio.fsh".into(), tpage: Some(2), global: 5, tint: false, ..Default::default() });
        assert_eq!(GLOBAL_FILES[l.files[2].global as usize - 1], "cabrio.fsh");
        assert_eq!(parse_tpg_layout(""), TpgLayout::default());
    }

    #[test]
    fn driver_camera_prefers_the_style_entry() {
        let tpg = "[header]\r\ndrivercampos=-0.43,0.30,-0.53\r\ndrivercamrot=1.0,0.0,0.0\r\n\
                   drivercamposstyle0=-0.43,0.34,-0.48\r\ndrivercamrotstyle0=0.3,0.0,0.0\r\n[style1]\r\ndrivercampos=9,9,9\r\n";
        // style 1 is `style0` in the camera keys
        assert_eq!(driver_camera(tpg, 1), ([-0.43, 0.34, -0.48], [0.3, 0.0, 0.0]));
        assert_eq!(driver_camera(tpg, 2), ([-0.43, 0.30, -0.53], [1.0, 0.0, 0.0]));
        assert_eq!(driver_camera("", 1), ([-0.36, 0.4, -0.25], [0.0; 3]));
    }

    #[test]
    fn driver_eye_turns_the_engine_frame() {
        // 993: 0.38 to the left (the steering-wheel side), 0.40 above the centre, 0.155 behind it
        let eye = driver_eye([-0.382813, 0.397512, -0.155002], 0.65);
        assert!((eye[0] - 0.382813 * MIRROR_X).abs() < 1e-6);
        assert!((eye[1] - 1.047512).abs() < 1e-6);
        assert!((eye[2] - 0.155002).abs() < 1e-6); // behind = +z for a car facing -z
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
        assert!(!is_skipped_name("DecalHood"));
        assert!(!is_skipped_name("DecalGauge"));
        assert!(!is_skipped_name("WheelShadowFront"));
        assert!(is_shadow_name("WheelShadowFront") && is_shadow_name("WheelShadowRear") && !is_shadow_name("WheelFront"));
        assert!(is_skipped_name("Passenger"));
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
