//! The retail loading screen shown between pressing RACE and the race starting.
//!
//! What it shows (see `docs/provenance/loading.md` for the evidence):
//! - a 640x480 background built from 45 tiles of `FEData/Art/era<N>.fsh`;
//! - from stage 5 on, the "track" version of that background (`era<N>tr.fsh`, which carries the
//!   word LOADING and the bar's groove) with the track's map, its name and a finish flag from
//!   `FEData/TrackArt/<track>.fsh` on top;
//! - an orange bar at (302, 467), 6 pixels high, 12.4 pixels longer for each of the ten stages;
//! - in a non-English game, a translated LOADING label from `FEData/Art/LoadText.fsh`.
//!
//! No car artwork is shown. The screen ends by itself when loading is done (no key press).
//! Retail draws the 640x480 picture unscaled in the middle of the screen with black around it.
//!
//! The first half of this file is pure logic (which art, where, how long the bar is) and is unit
//! tested; the second half shows it in Bevy (`LoadingScreenPlugin`).

use crate::menu_canvas::Canvas;
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::PrimaryWindow;
use nfspu_formats::{refpack, shpi};
use std::path::{Path, PathBuf};

pub const SCREEN_WIDTH: u32 = 640;
pub const SCREEN_HEIGHT: u32 = 480;
/// The last stage; reaching it ends the loading screen.
pub const LAST_STAGE: u8 = 10;
/// The stage at which the background switches to the track version and the track art appears.
pub const TRACK_ART_STAGE: u8 = 5;
/// The bar: left edge, top, height, length per stage, colour (opaque orange, `0xFFBB00`).
pub const BAR_X: f32 = 302.0;
pub const BAR_Y: f32 = 467.0;
pub const BAR_HEIGHT: f32 = 6.0;
pub const BAR_PER_STAGE: f32 = 12.4;
pub const BAR_COLOUR: [u8; 3] = [0xFF, 0xBB, 0x00];

/// What the player picked for the race, as far as the loading screen and the loader need it.
#[derive(Debug, Clone, PartialEq, Eq, Resource)]
pub struct RaceSetup {
    /// Track file name without extension, as in `GameData/Track` (`alps`, `monaco1`, ...).
    pub track: String,
    /// Car file name without extension, as in `GameData/CarModel` (`993`, `356a`, ...).
    pub car: String,
    /// Driving the track backwards: the finish flag sits at the other end of the map.
    pub reversed: bool,
    /// Mirrored track: the map is drawn flipped left to right.
    pub mirrored: bool,
    /// Which era's background: 0 for quick race. Retail picks 1-3 only in one game mode
    /// (believed to be Evolution) from the current era.
    pub era: u8,
    /// Game language, in the game's order (0 English, 1 French, 2 German, 3 Italian, 4 Spanish,
    /// 5 Swedish, 6 Portuguese). English needs no extra label.
    pub language: u8,
}

impl Default for RaceSetup {
    fn default() -> Self {
        RaceSetup { track: "alps".into(), car: "993".into(), reversed: false, mirrored: false, era: 0, language: 0 }
    }
}

impl RaceSetup {
    /// Reads `<track>[,<car>][,reverse][,mirror]` (the `NFSPU_LOADING` value). The car defaults to
    /// the 993. Returns `None` for an empty track name.
    pub fn parse(spec: &str) -> Option<RaceSetup> {
        let mut parts = spec.split(',').map(str::trim);
        let track = parts.next().filter(|t| !t.is_empty())?.to_string();
        let mut setup = RaceSetup { track, ..Default::default() };
        let mut car_set = false;
        for part in parts.filter(|p| !p.is_empty()) {
            match part.to_ascii_lowercase().as_str() {
                "r" | "rev" | "reverse" | "reversed" => setup.reversed = true,
                "m" | "mirror" | "mirrored" => setup.mirrored = true,
                _ if !car_set => {
                    setup.car = part.to_string();
                    car_set = true;
                }
                _ => {}
            }
        }
        Some(setup)
    }
}

/// Background file for a stage: the plain era picture, then the "track" one from stage 5.
pub fn background_file(era: u8, stage: u8) -> String {
    if stage >= TRACK_ART_STAGE {
        format!("era{era}tr.fsh")
    } else {
        format!("era{era}.fsh")
    }
}

/// Track art file in `FEData/TrackArt`.
pub fn track_art_file(track: &str) -> String {
    format!("{track}.fsh")
}

/// The bar's rectangle `[x, y, width, height]` at a stage (stages past 10 count as 10).
pub fn bar_rect(stage: u8) -> [f32; 4] {
    [BAR_X, BAR_Y, stage.min(LAST_STAGE) as f32 * BAR_PER_STAGE, BAR_HEIGHT]
}

/// Where each of the 45 background tiles goes, as `(entry index, x, y, size)`: a row of twenty
/// 32-pixel squares, a row of ten 64-pixel squares, then three rows of five 128-pixel squares.
pub fn background_tiles() -> Vec<(usize, i32, i32, i32)> {
    let rows: [(i32, i32); 5] = [(0, 32), (32, 64), (96, 128), (224, 128), (352, 128)];
    let mut tiles = Vec::new();
    for (y, size) in rows {
        for x in (0..SCREEN_WIDTH as i32).step_by(size as usize) {
            tiles.push((tiles.len(), x, y, size));
        }
    }
    tiles
}

/// The part of an image's header the loading screen uses: its size and the screen position
/// stored in it (header bytes 8 and 10; retail takes their absolute values).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    pub width: i32,
    pub height: i32,
    pub x: i32,
    pub y: i32,
}

/// One thing to draw, bottom first.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// Background tile `index` of the plain (`track == false`) or track background.
    Tile { track: bool, index: usize, x: i32, y: i32 },
    /// Track art image `index`, flipped left to right when `flip`.
    TrackArt { index: usize, x: i32, y: i32, flip: bool },
    /// The translated LOADING label, entry `index` of `LoadText.fsh`.
    LoadText { index: usize, x: i32, y: i32 },
    /// The bar.
    Bar { rect: [f32; 4] },
}

/// The track art images to draw and where: the map (0), the name (1) and the finish flag
/// (2 forwards, 3 backwards). When mirrored, map and name are flipped in place and the flag's
/// left edge is reflected about the name's centre (retail's formula, flag width not included).
pub fn track_art_ops(art: &[Placed], setup: &RaceSetup) -> Vec<Op> {
    let mut ops = Vec::new();
    for index in 0..2 {
        if let Some(p) = art.get(index) {
            ops.push(Op::TrackArt { index, x: p.x, y: p.y, flip: setup.mirrored });
        }
    }
    let flag = if setup.reversed { 3 } else { 2 };
    if let Some(f) = art.get(flag) {
        let x = match (setup.mirrored, art.get(1)) {
            (true, Some(name)) => 2 * name.x + name.width - f.x,
            _ => f.x,
        };
        ops.push(Op::TrackArt { index: flag, x, y: f.y, flip: false });
    }
    ops
}

/// Everything drawn at `stage`, bottom first. `track_art` is the header data of the track's art
/// (empty when the file is missing); `load_text` that of the translated label, if any.
pub fn plan(setup: &RaceSetup, stage: u8, track_art: &[Placed], load_text: Option<Placed>) -> Vec<Op> {
    let track = stage >= TRACK_ART_STAGE;
    let mut ops: Vec<Op> = background_tiles().into_iter().map(|(index, x, y, _)| Op::Tile { track, index, x, y }).collect();
    if track {
        ops.extend(track_art_ops(track_art, setup));
    }
    if let (Some(p), Some(index)) = (load_text, load_text_index(setup.language)) {
        ops.push(Op::LoadText { index, x: p.x, y: p.y });
    }
    ops.push(Op::Bar { rect: bar_rect(stage) });
    ops
}

/// Entry of `LoadText.fsh` for a language: none for English, French is entry 0 and so on.
pub fn load_text_index(language: u8) -> Option<usize> {
    (language > 0).then(|| language as usize - 1)
}

// ---------------------------------------------------------------------------------------------
// Reading the art

/// A decoded image with its header placement.
#[derive(Debug, Clone)]
pub struct Sprite {
    pub placed: Placed,
    pub rgba: Vec<u8>,
}

/// Finds `dir/name` ignoring case (the install mixes `forestSpSm.FSH` and `alps.fsh`).
fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let exact = dir.join(name);
    if exact.is_file() {
        return Some(exact);
    }
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| p.file_name().is_some_and(|f| f.to_string_lossy().eq_ignore_ascii_case(name)))
}

/// Reads every entry of an `.fsh`. Entries whose format has bit 0x80 set are compressed on their
/// own (RefPack after the 16-byte header); those are unpacked first. Undecodable entries are `None`.
pub fn read_sprites(path: &Path) -> Result<Vec<Option<Sprite>>, String> {
    let raw = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let data = if refpack::is_refpack(&raw) { refpack::decompress(&raw).map_err(|e| format!("{}: {e}", path.display()))? } else { raw };
    let archive = shpi::parse(&data).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(archive.entries.iter().map(|entry| decode_entry(&data, entry)).collect())
}

fn decode_entry(data: &[u8], entry: &shpi::ShpiEntry) -> Option<Sprite> {
    let at = entry.offset;
    let word = |o: usize| data.get(at + o..at + o + 2).map(|b| i16::from_le_bytes([b[0], b[1]]) as i32);
    let placed = Placed { width: entry.width as i32, height: entry.height as i32, x: word(8)?.abs(), y: word(10)?.abs() };
    let rgba = if entry.format & 0x80 != 0 {
        // Rebuild an uncompressed entry (header with the plain format, then the unpacked pixels).
        let packed = data.get(at + 16..)?;
        let pixels = refpack::decompress(packed).ok()?;
        let mut single = data.get(at..at + 16)?.to_vec();
        single[0] = entry.format & 0x7F;
        single.extend_from_slice(&pixels);
        let plain = shpi::ShpiEntry { offset: 0, format: entry.format & 0x7F, ..entry.clone() };
        shpi::decode_rgba8(&single, &plain)?
    } else {
        shpi::decode_rgba8(data, entry)?
    };
    Some(Sprite { placed, rgba })
}

/// All art one loading screen needs, read once.
#[derive(Debug, Clone, Default)]
pub struct LoadingArt {
    pub plain: Vec<Option<Sprite>>,
    pub track_bg: Vec<Option<Sprite>>,
    pub track_art: Vec<Option<Sprite>>,
    pub load_text: Option<Sprite>,
    /// Files that could not be read, for the log.
    pub misses: Vec<String>,
}

impl LoadingArt {
    pub fn load(game_dir: &Path, setup: &RaceSetup) -> LoadingArt {
        let art = game_dir.join("FEData").join("Art");
        let mut misses = Vec::new();
        let mut read = |dir: &Path, name: &str| -> Vec<Option<Sprite>> {
            match find_file(dir, name).ok_or_else(|| format!("{} not found in {}", name, dir.display())).and_then(|p| read_sprites(&p)) {
                Ok(s) => s,
                Err(e) => {
                    misses.push(e);
                    Vec::new()
                }
            }
        };
        let plain = read(&art, &background_file(setup.era, 0));
        let track_bg = read(&art, &background_file(setup.era, TRACK_ART_STAGE));
        let track_art = read(&game_dir.join("FEData").join("TrackArt"), &track_art_file(&setup.track));
        let load_text = match load_text_index(setup.language) {
            Some(i) => read(&art, "LoadText.fsh").into_iter().nth(i).flatten(),
            None => None,
        };
        LoadingArt { plain, track_bg, track_art, load_text, misses }
    }

    /// Header placements of the track art (missing entries count as zero-sized at the origin).
    fn track_placements(&self) -> Vec<Placed> {
        self.track_art.iter().map(|s| s.as_ref().map(|s| s.placed).unwrap_or(Placed { width: 0, height: 0, x: 0, y: 0 })).collect()
    }

    /// Draws the screen at `stage` into a 640x480 picture.
    pub fn compose(&self, setup: &RaceSetup, stage: u8) -> Canvas {
        let mut canvas = Canvas::new(SCREEN_WIDTH, SCREEN_HEIGHT, [0.0, 0.0, 0.0, 1.0]);
        let draw = |canvas: &mut Canvas, s: &Sprite, x: i32, y: i32, flip: bool| {
            canvas.draw_image(x, y, s.placed.width as u32, s.placed.height as u32, &s.rgba, flip as u8, 1.0);
        };
        for op in plan(setup, stage, &self.track_placements(), self.load_text.as_ref().map(|s| s.placed)) {
            match op {
                Op::Tile { track, index, x, y } => {
                    let set = if track { &self.track_bg } else { &self.plain };
                    if let Some(Some(s)) = set.get(index) {
                        draw(&mut canvas, s, x, y, false);
                    }
                }
                Op::TrackArt { index, x, y, flip } => {
                    if let Some(Some(s)) = self.track_art.get(index) {
                        draw(&mut canvas, s, x, y, flip);
                    }
                }
                Op::LoadText { x, y, .. } => {
                    if let Some(s) = &self.load_text {
                        draw(&mut canvas, s, x, y, false);
                    }
                }
                Op::Bar { rect } => {
                    let c = BAR_COLOUR;
                    canvas.fill_rect(rect, [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0]);
                }
            }
        }
        canvas
    }
}

// ---------------------------------------------------------------------------------------------
// Bevy: showing it

/// The current loading stage, 0..=10. Whoever loads the race raises it; 10 ends the screen.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LoadingStage(pub u8);

/// Sent once when the loading screen has gone and the race is on screen.
#[derive(Message, Debug, Clone)]
pub struct LoadingFinished {
    pub setup: RaceSetup,
    /// Seconds from the first frame of the loading screen to its removal.
    pub seconds: f32,
}

/// The screen's state while it is up.
#[derive(Resource)]
pub struct LoadingScreen {
    pub setup: RaceSetup,
    art: LoadingArt,
    shown: Option<u8>,
    started: std::time::Instant,
    /// Write each stage's picture to `logs/loading_<track>_stage<N>.png` (`NFSPU_LOADING_CAPTURE=1`).
    capture: bool,
}

#[derive(Component)]
struct LoadingVisual;

#[derive(Component)]
struct LoadingCamera;

/// Puts the loading screen up for `setup` at stage 0. Reads its art from `game_dir` (a few
/// hundred kilobytes, quick). The screen draws above every other camera until stage 10.
pub fn begin(commands: &mut Commands, game_dir: &Path, setup: RaceSetup) -> Vec<String> {
    let art = LoadingArt::load(game_dir, &setup);
    let misses = art.misses.clone();
    let capture = std::env::var("NFSPU_LOADING_CAPTURE").as_deref() == Ok("1");
    commands.insert_resource(LoadingScreen { setup, art, shown: None, started: std::time::Instant::now(), capture });
    commands.insert_resource(LoadingStage(0));
    // An own UI camera on top, clearing to black, so the 3D scene spawning underneath stays hidden.
    let camera = commands
        .spawn((Camera2d, Camera { order: 100, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() }, IsDefaultUiCamera, LoadingCamera))
        .id();
    commands.spawn((LoadingVisual, Node { position_type: PositionType::Absolute, ..default() }, ImageNode::default(), UiTargetCamera(camera), GlobalZIndex(1000)));
    misses
}

/// Shows the loading screen whenever a `LoadingScreen` resource exists.
pub struct LoadingScreenPlugin;

impl Plugin for LoadingScreenPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<LoadingFinished>().add_systems(PostUpdate, present);
    }
}

/// Retail draws the picture at its own size in the middle of the screen: one layout pixel per
/// physical pixel, so in a window it is `640 / scale factor` logical pixels wide.
fn place(node: &mut Node, window: &Window) {
    let scale = window.scale_factor().max(0.01);
    let (w, h) = (SCREEN_WIDTH as f32 / scale, SCREEN_HEIGHT as f32 / scale);
    node.left = Val::Px(((window.width() - w) / 2.0).round());
    node.top = Val::Px(((window.height() - h) / 2.0).round());
    node.width = Val::Px(w);
    node.height = Val::Px(h);
}

#[allow(clippy::too_many_arguments)]
fn present(
    mut commands: Commands,
    screen: Option<ResMut<LoadingScreen>>,
    stage: Option<Res<LoadingStage>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut shown: Query<(Entity, &mut Node, &mut ImageNode), With<LoadingVisual>>,
    cameras: Query<Entity, With<LoadingCamera>>,
    mut images: ResMut<Assets<Image>>,
    mut finished: MessageWriter<LoadingFinished>,
) {
    let (Some(mut screen), Some(stage)) = (screen, stage) else { return };
    let Ok((entity, mut node, mut image_node)) = shown.single_mut() else { return };
    if let Ok(window) = windows.single() {
        place(&mut node, window);
    }
    let stage = stage.0.min(LAST_STAGE);
    if stage >= LAST_STAGE {
        let seconds = screen.started.elapsed().as_secs_f32();
        commands.entity(entity).despawn();
        for camera in &cameras {
            commands.entity(camera).despawn();
        }
        images.remove(&image_node.image);
        finished.write(LoadingFinished { setup: screen.setup.clone(), seconds });
        commands.remove_resource::<LoadingScreen>();
        commands.remove_resource::<LoadingStage>();
        return;
    }
    if screen.shown == Some(stage) {
        return;
    }
    let canvas = screen.art.compose(&screen.setup, stage);
    if screen.capture {
        save_png(&canvas, &capture_path(&screen.setup.track, stage));
    }
    let image = Image::new(
        Extent3d { width: canvas.width, height: canvas.height, depth_or_array_layers: 1 },
        TextureDimension::D2,
        canvas.rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    let old = std::mem::replace(&mut image_node.image, images.add(image));
    images.remove(&old);
    screen.shown = Some(stage);
}

/// `logs/loading_<track>_stage<N>.png` in the project folder.
pub fn capture_path(track: &str, stage: u8) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("logs").join(format!("loading_{track}_stage{stage:02}.png"))
}

/// Saves a composed picture as PNG (for comparing with retail captures).
pub fn save_png(canvas: &Canvas, path: &Path) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let image = Image::new(
        Extent3d { width: canvas.width, height: canvas.height, depth_or_array_layers: 1 },
        TextureDimension::D2,
        canvas.rgba.clone(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    if let Ok(dynamic) = image.try_into_dynamic() {
        let _ = dynamic.to_rgba8().save(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_spec() {
        assert_eq!(RaceSetup::parse(""), None);
        let s = RaceSetup::parse("alps").unwrap();
        assert_eq!((s.track.as_str(), s.car.as_str(), s.reversed, s.mirrored), ("alps", "993", false, false));
        let s = RaceSetup::parse("monaco1, 356a, reverse").unwrap();
        assert_eq!((s.track.as_str(), s.car.as_str(), s.reversed, s.mirrored), ("monaco1", "356a", true, false));
        let s = RaceSetup::parse("alps,r,m").unwrap();
        assert_eq!((s.car.as_str(), s.reversed, s.mirrored), ("993", true, true));
    }

    #[test]
    fn files_follow_the_stage() {
        assert_eq!(background_file(0, 0), "era0.fsh");
        assert_eq!(background_file(0, 4), "era0.fsh");
        assert_eq!(background_file(0, 5), "era0tr.fsh");
        assert_eq!(background_file(2, 9), "era2tr.fsh");
        assert_eq!(track_art_file("Monaco1"), "Monaco1.fsh");
    }

    #[test]
    fn bar_grows_by_stage() {
        assert_eq!(bar_rect(0), [302.0, 467.0, 0.0, 6.0]);
        assert_eq!(bar_rect(5)[2], 62.0);
        assert!((bar_rect(10)[2] - 124.0).abs() < 1e-4);
        assert_eq!(bar_rect(200), bar_rect(10));
    }

    #[test]
    fn tiles_cover_the_screen_once() {
        let tiles = background_tiles();
        assert_eq!(tiles.len(), 45);
        let mut covered = vec![0u8; (SCREEN_WIDTH * SCREEN_HEIGHT) as usize];
        for (i, (index, x, y, size)) in tiles.iter().enumerate() {
            assert_eq!(i, *index);
            for py in *y..y + size {
                for px in *x..x + size {
                    covered[(py * SCREEN_WIDTH as i32 + px) as usize] += 1;
                }
            }
        }
        assert!(covered.iter().all(|&c| c == 1));
        assert_eq!(tiles[20], (20, 0, 32, 64));
        assert_eq!(tiles[44], (44, 512, 352, 128));
    }

    // alps.fsh header values from the author's install.
    fn alps() -> Vec<Placed> {
        vec![
            Placed { width: 414, height: 132, x: 115, y: 177 },
            Placed { width: 40, height: 14, x: 301, y: 306 },
            Placed { width: 16, height: 16, x: 525, y: 169 },
            Placed { width: 16, height: 16, x: 113, y: 216 },
        ]
    }

    #[test]
    fn track_art_appears_at_stage_five() {
        let setup = RaceSetup::default();
        let early = plan(&setup, 4, &alps(), None);
        assert_eq!(early.len(), 46);
        assert!(early.iter().all(|op| !matches!(op, Op::TrackArt { .. } | Op::Tile { track: true, .. })));
        let late = plan(&setup, 5, &alps(), None);
        assert_eq!(late.len(), 45 + 3 + 1);
        assert_eq!(late[45], Op::TrackArt { index: 0, x: 115, y: 177, flip: false });
        assert_eq!(late[47], Op::TrackArt { index: 2, x: 525, y: 169, flip: false });
        assert_eq!(late.last(), Some(&Op::Bar { rect: bar_rect(5) }));
    }

    #[test]
    fn flag_follows_direction_and_mirror() {
        let reversed = RaceSetup { reversed: true, ..Default::default() };
        assert_eq!(track_art_ops(&alps(), &reversed)[2], Op::TrackArt { index: 3, x: 113, y: 216, flip: false });
        let mirrored = RaceSetup { mirrored: true, ..Default::default() };
        let ops = track_art_ops(&alps(), &mirrored);
        assert_eq!(ops[0], Op::TrackArt { index: 0, x: 115, y: 177, flip: true });
        // 2 * 301 + 40 - 525
        assert_eq!(ops[2], Op::TrackArt { index: 2, x: 117, y: 169, flip: false });
        // missing art draws nothing
        assert!(track_art_ops(&[], &mirrored).is_empty());
    }

    #[test]
    fn load_text_only_when_translated() {
        assert_eq!(load_text_index(0), None);
        assert_eq!(load_text_index(1), Some(0));
        assert_eq!(load_text_index(6), Some(5));
        let french = RaceSetup { language: 1, ..Default::default() };
        let ops = plan(&french, 0, &[], Some(Placed { width: 185, height: 24, x: 111, y: 456 }));
        assert_eq!(ops[45], Op::LoadText { index: 0, x: 111, y: 456 });
    }
}
