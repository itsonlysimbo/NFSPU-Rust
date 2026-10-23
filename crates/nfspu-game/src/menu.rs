//! The front end, assembled: a `.lay` screen plus the control templates it uses, resolved against
//! the locale table into something that can be drawn.
//!
//! A screen is a fixed `640x480` space (the layout's own `size`), so everything here is in those
//! units; the viewer scales it to the window the way the retail build does. Only the control types
//! verified so far are turned into draw commands - the rest are reported in `skipped` so the
//! screen's completeness is visible rather than silently wrong.
//!
//! Evidence: `docs/provenance/fe.md` and `docs/DESIGN.md` (milestone M6).

use nfspu_formats::fe::{Block, ControlTemplate, Lay, Locale};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A rectangle in screen units: `x, y, width, height`, with (0,0) at the top left.
pub type Rect = [f32; 4];

/// One thing to draw.
#[derive(Debug, Clone, PartialEq)]
pub enum Draw {
    /// A filled rectangle. `hover` is the fill while the pointer is inside it, and `link` is the
    /// screen its control's `destination=` names.
    Quad { rect: Rect, colour: [f32; 4], hover: Option<[f32; 4]>, link: Option<String>, back: bool },
    /// A line of text with its top-left at `pos`, in the `.ffn` font `font` (`FEData/Art`);
    /// `size` is that font's point size, for drawing without the font file.
    Text { text: String, pos: [f32; 2], size: f32, colour: [f32; 4], font: String },
    /// A four-colour quad (`NFS5.GOURAUDBOX`). Corners are upper-left, upper-right, lower-left,
    /// lower-right; each may be pushed off the box by its own offset, which is how the layouts
    /// draw mitred frame edges. Colours blend smoothly between the corners.
    Gradient { corners: [[f32; 2]; 4], colours: [[f32; 4]; 4], when: Show },
    /// One image from an `FEData/Art` archive at its natural size. `flip` bit 1 mirrors it
    /// left-right, bit 2 top-bottom (`NFS5.PICTURE`, and the four quarters of a popup circle).
    /// `alpha` scales the image's own opacity (disabled bottom-bar icons are drawn at half).
    Picture { file: String, index: usize, pos: [f32; 2], flip: u8, alpha: f32 },
    /// An invisible click target (a list row, a quit button); what it looks like is drawn by other
    /// commands.
    Target { rect: Rect, hit: Hit },
    /// Any draw shown only when idle or only when hovered (for text that changes colour on hover).
    Shown { when: Show, draw: Box<Draw> },
    /// Draws painted only inside `rect` (a dropdown list while it slides open or closed).
    Clipped { rect: Rect, draws: Vec<Draw> },
    /// A playing movie (`NFS5.MOVIECIRCLE`): the current frame of `source` scaled into `rect`,
    /// shown only where the `stencil` pieces (rectangles and pictures) cover; an empty stencil
    /// shows the whole rectangle.
    Movie { rect: Rect, source: String, stencil: Vec<Draw> },
    /// An animated icon (`NFS5.BOTTOMBARBUTTON`): frames of `file`, from `idle`, from `hover` while
    /// the pointer is over `rect`, or from `down` while it is also pressed; otherwise as `Picture`.
    /// The bright light that runs once round a hovered button's outline (measured from retail footage).
    Glint { rect: Rect },
    Anim { file: String, idle: Frames, hover: Frames, down: Frames, rect: Rect, pos: [f32; 2], flip: u8, alpha: f32 },
}

/// A looping run of archive frames (`startframe`, `endframe`, `totalticks` in the layouts, and
/// the `hover...` and `hovermousedown...` sets). Retail's animating button shows
/// `start + elapsed * (end - start) / ticks` and restarts once `ticks` have passed (the bottom
/// bar's buttons have no delay between loops); a set with 0 ticks holds its first frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Frames {
    pub start: usize,
    pub end: usize,
    /// Length of one run in 1/128 s ticks of the front-end clock.
    pub ticks: u32,
}

impl Frames {
    /// The frame shown `clock` ticks into the front end.
    pub fn at(&self, clock: u32) -> usize {
        if self.ticks == 0 || self.end <= self.start {
            return self.start;
        }
        let elapsed = (clock % (self.ticks + 1)) as usize;
        self.start + elapsed * (self.end - self.start) / self.ticks as usize
    }
}

/// How a control comes on and goes off when its screen changes (`transon=` / `transoff=`):
/// bit set of `TRANS_LEFT` .. `TRANS_FADE`.
pub const TRANS_SAME: u8 = 1;
pub const TRANS_LEFT: u8 = 2;
pub const TRANS_RIGHT: u8 = 4;
pub const TRANS_TOP: u8 = 8;
pub const TRANS_BOTTOM: u8 = 16;
pub const TRANS_FADE: u8 = 32;

/// One `transon` / `transoff` word as retail reads it (case ignored): `same`, `left`, `right`,
/// `top`, `bottom`, `fade`; anything else is no movement.
pub fn trans_flag(word: &str) -> u8 {
    match word.trim().to_ascii_lowercase().as_str() {
        "same" => TRANS_SAME,
        "left" => TRANS_LEFT,
        "right" => TRANS_RIGHT,
        "top" => TRANS_TOP,
        "bottom" => TRANS_BOTTOM,
        "fade" => TRANS_FADE,
        _ => 0,
    }
}

/// A control's (on, off) transitions: `SAME` on one side copies the other side; both `SAME`
/// means none (retail's control set-up does exactly this).
pub fn transitions(on: &str, off: &str) -> (u8, u8) {
    let (on, off) = (trans_flag(on), trans_flag(off));
    match (on & TRANS_SAME != 0, off & TRANS_SAME != 0) {
        (true, true) => (0, 0),
        (true, false) => (off, off),
        (false, true) => (on, on),
        (false, false) => (on, off),
    }
}

/// Where a transitioning control is drawn: shifted by a full screen width (left/right) or height
/// (top/bottom) times `away` (0 in place, 1 fully off), and faded to `1 - away` (fade). Retail:
/// offset `640 * p / 512`, alpha `255 - 255 * p / 512` with `p` the menu's 0..512 progress.
pub fn trans_offset(flags: u8, away: f32) -> ([f32; 2], f32) {
    let mut shift = [0.0, 0.0];
    if flags & TRANS_LEFT != 0 {
        shift[0] -= 640.0 * away;
    }
    if flags & TRANS_RIGHT != 0 {
        shift[0] += 640.0 * away;
    }
    if flags & TRANS_TOP != 0 {
        shift[1] -= 480.0 * away;
    }
    if flags & TRANS_BOTTOM != 0 {
        shift[1] += 480.0 * away;
    }
    let alpha = if flags & TRANS_FADE != 0 { 1.0 - away } else { 1.0 };
    (shift, alpha)
}

/// When a draw command is visible, for pieces that change with the pointer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Show {
    Always,
    /// Only while the pointer is not over this rectangle (a button's gloss and drop shadow).
    Idle(Rect),
    /// Only while the pointer is over this rectangle (a button's pressed-in bevel).
    Hover(Rect),
}

/// What clicking a control does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hit {
    /// Go to the named screen (`destination=`).
    Goto(String),
    /// Go back one screen (a control whose `callback=` is `GOBACK`).
    Back,
    /// Select a row of the user list.
    Select(usize),
    /// Leave the game (a control whose `callback=` is `QUIT`).
    Quit,
    /// Open or close the dropdown for this accessor.
    ComboToggle(String),
    /// Choose item `index` of the dropdown for this accessor (headers are not counted).
    ComboPick(String, usize),
    /// Scroll the open dropdown for this accessor by whole rows (its arrow buttons).
    ComboScroll(String, i32),
    /// A named script hook from a control's `callback=` (`PLAYGAME`, `TRACK1INC`, ...).
    Callback(String),
    /// Inside an open dropdown list but on nothing choosable (a section title, the scroll bar's
    /// track, the edge): the click is used up, so nothing under the list reacts.
    Inert,
    /// A level bar (`NFS5.GouraudIndicatorBar` with `changeable=1`): its accessor and the bar's
    /// left edge and width, so the pointer's x sets the level.
    Slider(String, i32, i32),
    /// An Options tab (`NFS5.GRAPHICTABBUTTON`): show the panel it names.
    Tab(String),
}

/// One row of a dropdown list: a centred section title, or a choosable item with an optional
/// lead column (the car list's two-digit year), colour swatch and divider after the lead.
#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    Header(String),
    Item { lead: Option<String>, text: String, swatch: Option<[u8; 3]>, divider: bool },
}

impl Row {
    /// A plain item with only text.
    pub fn item(text: String) -> Row {
        Row::Item { lead: None, text, swatch: None, divider: false }
    }
}

/// What a dropdown offers: its rows, the chosen item (counting items only), and what the closed
/// control shows (text, or a colour swatch for a colour box).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Choice {
    pub rows: Vec<Row>,
    pub selected: usize,
    pub closed: String,
    pub closed_swatch: Option<[u8; 3]>,
}

/// One loaded screen.
#[derive(Debug, Clone, Default)]
pub struct Screen {
    pub name: String,
    pub size: [f32; 2],
    pub background: [f32; 4],
    /// Draw commands in layout section order. The first section is the **topmost**: layouts list
    /// small details and buttons first and their large backing panels last (`selectname.lay` ends
    /// with its full-panel gradients and the popup circle), so retail paints from the end.
    pub draw: Vec<Draw>,
    /// Control ids present in the layout that this renderer does not draw yet.
    pub skipped: Vec<String>,
    /// The header's `screen=` frame id (`ID_MOVIEOVAL`), when it names a real frame layout.
    pub frame: Option<String>,
    /// The header's `room=` id (`ID_GARAGE`, `ID_NONE`, or empty).
    pub room: String,
    /// The header's `canplaygame`: 0 or empty greys the bottom bar's RACE button, 1 enables it;
    /// other values defer to the game's own readiness check in retail (treated as enabled here).
    pub can_play: bool,
    /// The header's `musicfile=` song pattern (`menu*`, `NONE`, ...); empty when absent.
    pub music: String,
    /// Controls with a help tip (`helptext=`), topmost first; none on a disabled page.
    pub help: Vec<crate::menu_help::Help>,
    /// Draws that go above every section (an open dropdown's list), topmost first; moved to the
    /// front of `draw` once the screen is built.
    pub top: Vec<Draw>,
    /// Each control's draws as `(first, end)` indices into `draw` with its (on, off)
    /// transitions; the open list (`top`) does not move.
    pub trans: Vec<(usize, usize, u8, u8)>,
    /// A screen change in progress for this layer: (going off, how far away 0..=1). Set by the
    /// viewer on the copy it draws.
    pub moving: Option<(bool, f32)>,
}

impl Screen {
    /// The (on, off) transitions of the control that owns draw `index`.
    pub fn trans_of(&self, index: usize) -> (u8, u8) {
        self.trans.iter().find(|(a, b, _, _)| index >= *a && index < *b).map(|(_, _, on, off)| (*on, *off)).unwrap_or((0, 0))
    }
}

/// The layout file a frame id names: `ID_MOVIEOVAL` is `screenmovieoval.lay`, and `ID_SCREEN` is
/// `screenempty.lay` (the ring of `CnstLyr` pictures around the 3D room: retail's frame lookup
/// maps it so in its normal hardware-3D mode, and the recordings show the ring on the quick-race
/// page). `ID_NONE` and empty mean no frame. (The retail build also swaps `screenoptionoval` for a
/// `lite` variant in some states, and uses other frames in its low-memory mode; not modelled.)
pub fn frame_layout(id: &str) -> Option<String> {
    let id = id.trim();
    let rest = id.strip_prefix("ID_")?;
    if rest.eq_ignore_ascii_case("SCREEN") {
        return Some("screenempty".to_string());
    }
    if rest.is_empty() || rest.eq_ignore_ascii_case("NONE") {
        return None;
    }
    Some(format!("screen{}", rest.to_ascii_lowercase()))
}

impl Screen {
    /// The link under a point in screen units; earlier sections win, as they are drawn on top.
    pub fn link_at(&self, x: f32, y: f32) -> Option<&str> {
        self.draw
            .iter()
            .filter_map(|d| match d {
                Draw::Quad { rect, link: Some(l), .. } => {
                    let [rx, ry, rw, rh] = *rect;
                    (x >= rx && x < rx + rw && y >= ry && y < ry + rh).then_some(l.as_str())
                }
                _ => None,
            })
            .next()
    }

    /// What a click at a point in screen units does: the topmost (earliest) control with an
    /// action wins.
    pub fn hit_at(&self, x: f32, y: f32) -> Option<Hit> {
        self.draw
            .iter()
            .find_map(|d| match d {
                Draw::Quad { rect, link, back, .. } => {
                    let [rx, ry, rw, rh] = *rect;
                    if !(x >= rx && x < rx + rw && y >= ry && y < ry + rh) {
                        return None;
                    }
                    if *back {
                        return Some(Hit::Back);
                    }
                    link.as_ref().map(|l| Hit::Goto(l.clone()))
                }
                Draw::Target { rect, hit } => {
                    let [rx, ry, rw, rh] = *rect;
                    (x >= rx && x < rx + rw && y >= ry && y < ry + rh).then(|| hit.clone())
                }
                _ => None,
            })
    }

    /// The rectangle of the quad at `index`, for hover tests in the viewer.
    pub fn quad_rects(&self) -> Vec<Rect> {
        self.draw
            .iter()
            .filter_map(|d| match d {
                Draw::Quad { rect, .. } => Some(*rect),
                _ => None,
            })
            .collect()
    }
}

/// The saved players, as the user list shows them.
///
/// Verified from the retail executable: the list is every `<save dir>*.sav` file name without its
/// extension, except the default template `XXDefXX`, sorted; the row whose name equals the text of
/// `<save dir>player.lst` is selected.
///
/// The first character of each save's file name is the player's face, and the rest is the name
/// shown (`0Greg.sav` is "Greg" with face 0). Verified from the executable: the list draws each
/// row from its second character, and the portrait takes the first character as a digit. Faces
/// come from `FEData/Art/ALLdrivr.fsh` (9 faces, 54x64; index 8 is the helmet `driv`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Profiles {
    /// Save file names without `.sav`, face digit included.
    pub names: Vec<String>,
    pub selected: Option<usize>,
}

/// The default save template that never appears in the list.
const DEFAULT_SAVE: &str = "XXDefXX";
/// The highest face index in `ALLdrivr.fsh`.
const LAST_FACE: i32 = 8;

/// The name a save shows in the list: its file name without the leading face digit.
pub fn display_name(stem: &str) -> &str {
    stem.char_indices().nth(1).map(|(i, _)| &stem[i..]).unwrap_or("")
}

/// The face a save uses: its first character read as a digit, clamped to 0..=8 the way the
/// retail portrait does (anything below '1', letters above '8', give 0 or 8).
pub fn face_index(stem: &str) -> usize {
    let first = stem.bytes().next().unwrap_or(b'0') as i32 - b'0' as i32;
    first.clamp(0, LAST_FACE) as usize
}

impl Profiles {
    /// Reads the save folder. A missing folder or file gives an empty list.
    pub fn load(save_dir: &Path) -> Profiles {
        let mut names: Vec<String> = std::fs::read_dir(save_dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                let is_sav = path.extension().and_then(|x| x.to_str()).map(|x| x.eq_ignore_ascii_case("sav")).unwrap_or(false);
                let stem = path.file_stem()?.to_str()?.to_string();
                (is_sav && stem != DEFAULT_SAVE && stem.len() < 100).then_some(stem)
            })
            .collect();
        // Sorted by the shown name, without the face digit: a retail screenshot lists "Driver"
        // above "Greg" although Greg's face digit is lower. Case-insensitive is a guess.
        names.sort_by_key(|n| display_name(n).to_ascii_lowercase());
        let last = std::fs::read(save_dir.join("player.lst")).ok().filter(|b| b.len() <= 100).map(|b| nfspu_formats::fe::decode_latin1(&b).trim_end_matches(['\0', '\r', '\n']).to_string());
        // With no `player.lst` match the first row is selected (**guess**).
        let selected = last.and_then(|l| names.iter().position(|n| *n == l)).or(if names.is_empty() { None } else { Some(0) });
        Profiles { names, selected }
    }
}

/// One decoded front-end image (an entry of an `FEData/Art` `.fsh`), RGBA8, top row first.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Why a shape could not be loaded, for the audit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShapeError {
    NoFile,
    BadArchive(String),
    NoEntry { count: usize },
    Format(u8),
}

/// Finds `FEData/Art/<file>` ignoring case (layouts write `MovieOval.fsh`, files vary).
fn art_path(game_dir: &Path, file: &str) -> Option<PathBuf> {
    // `TrackArt/<name>.fsh` names a track picture in `FEData/TrackArt` (our own convention, for
    // the location screen's stand-in picture).
    let (dir, file) = match file.split_once('/') {
        Some((folder, rest)) if folder.eq_ignore_ascii_case("TrackArt") => (game_dir.join("FEData").join("TrackArt"), rest),
        _ => (game_dir.join("FEData").join("Art"), file),
    };
    let exact = dir.join(file);
    if exact.is_file() {
        return Some(exact);
    }
    std::fs::read_dir(&dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.file_name().and_then(|n| n.to_str()).map(|n| n.eq_ignore_ascii_case(file)).unwrap_or(false))
}

/// Reads the archive bytes of a front-end `.fsh`, decompressing RefPack if it is packed.
pub fn load_art(game_dir: &Path, file: &str) -> Result<Vec<u8>, ShapeError> {
    let path = art_path(game_dir, file.trim()).ok_or(ShapeError::NoFile)?;
    let raw = std::fs::read(path).map_err(|_| ShapeError::NoFile)?;
    if nfspu_formats::refpack::is_refpack(&raw) {
        nfspu_formats::refpack::decompress(&raw).map_err(|e| ShapeError::BadArchive(e.to_string()))
    } else {
        Ok(raw)
    }
}

/// Decodes entry `index` (directory order, as `shapeIndex` counts) of an archive.
pub fn decode_shape(data: &[u8], index: usize) -> Result<Shape, ShapeError> {
    let archive = nfspu_formats::shpi::parse(data).map_err(|e| ShapeError::BadArchive(e.to_string()))?;
    let entry = archive.entries.get(index).ok_or(ShapeError::NoEntry { count: archive.entries.len() })?;
    let rgba = nfspu_formats::shpi::decode_rgba8(data, entry).ok_or(ShapeError::Format(entry.format))?;
    Ok(Shape { width: entry.width as u32, height: entry.height as u32, rgba })
}

/// Reads all control templates from `FEData/Controls` (`.ctr` and `.scr`).
pub fn load_templates(game_dir: &Path) -> BTreeMap<String, ControlTemplate> {
    let mut out = BTreeMap::new();
    let dir = game_dir.join("FEData").join("Controls");
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
        if !matches!(ext.as_deref(), Some("ctr") | Some("scr")) {
            continue;
        }
        let Some(text) = std::fs::read(&path).ok().map(|b| nfspu_formats::fe::decode_latin1(&b)) else { continue };
        if let Some(t) = ControlTemplate::parse(&text) {
            out.insert(t.id.clone(), t);
        }
    }
    out
}

/// The Options screen's panels (`Options.lay`), in tab order of the layout's sections.
pub const OPTION_PANELS: [&str; 5] = ["ID_controllers_panel", "ID_graphics_panel", "ID_display_panel", "ID_audio_panel", "ID_cameras_panel"];

/// Builds one screen. `name` is the layout's file stem (`singleplayer`, `quickrace`, ...).
pub fn load_screen(
    game_dir: &Path,
    name: &str,
    locale: &Locale,
    language: u8,
    templates: &BTreeMap<String, ControlTemplate>,
) -> Option<Screen> {
    load_screen_with(game_dir, name, locale, language, templates, &Profiles::default(), false, &Session::default())
}

/// [`load_screen`] with the saved players, which fill the user list and the portrait. With
/// `disabled`, controls that have a disabled state are drawn in it. `session` carries the state
/// that is not in the layout (bottom-bar availability, settings, an open dropdown).
pub fn load_screen_with(
    game_dir: &Path,
    name: &str,
    locale: &Locale,
    language: u8,
    templates: &BTreeMap<String, ControlTemplate>,
    profiles: &Profiles,
    disabled: bool,
    session: &Session,
) -> Option<Screen> {
    let path: PathBuf = game_dir.join("FEData").join("Layouts").join(format!("{name}.lay"));
    let text = nfspu_formats::fe::decode_latin1(&std::fs::read(path).ok()?);
    let lay = Lay::parse(&text);

    let header = lay.screen();
    let size = header.and_then(|b| b.pair("size")).map(|(w, h)| [w as f32, h as f32]).unwrap_or([640.0, 480.0]);
    let mut screen = Screen {
        name: name.to_string(),
        size,
        // The screen control's colour is the clear colour behind everything.
        background: header.and_then(|b| b.get("color")).and_then(|c| eval_colour(c, &BTreeMap::new())).unwrap_or([0.0, 0.0, 0.0, 1.0]),
        frame: header.and_then(|b| b.get("screen")).and_then(frame_layout),
        room: header.and_then(|b| b.get("room")).unwrap_or("").trim().to_string(),
        can_play: header.and_then(|b| b.get("canplaygame")).map(|v| !matches!(v.trim(), "" | "0")).unwrap_or(false),
        music: header.and_then(|b| b.get("musicfile")).unwrap_or("").trim().to_string(),
        ..Default::default()
    };

    for block in lay.controls() {
        let Some(id) = block.control() else { continue };
        // A panel or tab group draws another layout in place (`Panel.Ctr`, `Radio Group.Ctr`);
        // all of them sit at 0,0. A hidden panel draws nothing.
        if matches!(id, "NFS5.PANEL" | "NFS5.RADIOGROUP") {
            let name_field = block.get("name").map(str::trim).unwrap_or("");
            if session.hidden.iter().any(|h| h == name_field) {
                continue;
            }
            let file = block.get("filename").map(str::trim).unwrap_or("");
            match (!file.is_empty() && !file.eq_ignore_ascii_case(name)).then(|| load_screen_with(game_dir, file, locale, language, templates, profiles, disabled, session)).flatten() {
                // Its own `top` pieces are already at the front of its draw list.
                Some(sub) => {
                    let base = screen.draw.len();
                    screen.draw.extend(sub.draw);
                    screen.trans.extend(sub.trans.into_iter().map(|(a, b, on, off)| (a + base, b + base, on, off)));
                    screen.help.extend(sub.help);
                    screen.skipped.extend(sub.skipped);
                }
                None => screen.skipped.push(id.to_string()),
            }
            continue;
        }
        let Some(template) = templates.get(id) else {
            screen.skipped.push(id.to_string());
            continue;
        };
        let context = Context { profiles, game_dir: Some(game_dir), disabled, can_go_back: session.can_go_back, can_play: session.can_play, difficulty: session.difficulty, open_combo: session.open_combo.clone(), combo_reveal: session.combo_reveal, session: Some(session) };
        let drawn = screen.draw.len();
        if !append_control_with(&mut screen, block, template, locale, language, &context) {
            screen.skipped.push(id.to_string());
        }
        let (on, off) = transitions(block.get("transon").unwrap_or(""), block.get("transoff").unwrap_or(""));
        if on | off != 0 && screen.draw.len() > drawn {
            screen.trans.push((drawn, screen.draw.len(), on, off));
        }
        // Retail shows help only for enabled controls.
        if let Some(help) = crate::menu_help::target(block, &screen.draw[drawn..], locale, language).filter(|_| !disabled) {
            screen.help.push(help);
        }
    }
    let top = std::mem::take(&mut screen.top);
    let lifted = top.len();
    screen.draw.splice(0..0, top);
    for t in &mut screen.trans {
        t.0 += lifted;
        t.1 += lifted;
    }
    Some(screen)
}

/// The control's fields: its own keys win over the template's `[part.data]` defaults, which win
/// over the template's `[control]` values.
fn fields(block: &Block, template: &ControlTemplate) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    if let Some(info) = template.blocks.iter().find(|b| b.get("description").is_some()) {
        for (k, v) in &info.entries {
            map.insert(k.clone(), v.clone());
        }
    }
    for part in template.blocks.iter().filter(|b| b.get("id") == Some("DATA")) {
        for (k, v) in &part.entries {
            if let Some(key) = k.strip_suffix(".value") {
                map.insert(key.to_string(), v.clone());
            }
        }
    }
    for (k, v) in &block.entries {
        map.insert(k.clone(), v.clone());
    }
    map
}

/// Adds whatever this control draws. Returns false when the type is not drawn yet.
#[cfg(test)]
fn append_control(screen: &mut Screen, block: &Block, template: &ControlTemplate, locale: &Locale, language: u8) -> bool {
    append_control_with(screen, block, template, locale, language, &Context::new(&Profiles::default()))
}

/// The bottom bar's career-only pieces: the "ACCOUNT" label and the cash backdrop.
const ACCOUNT_LABEL: &str = "ID_ACCOUNTTHINGY";
const CASH_BACKDROP: &str = "ID_CASHBKGD";

/// What a control needs beyond its own fields.
pub struct Context<'a> {
    pub profiles: &'a Profiles,
    /// For reading art sizes and fonts; `None` in tests.
    pub game_dir: Option<&'a Path>,
    /// Draw controls in their `disabled` state (the page under a popup).
    pub disabled: bool,
    /// Bottom bar: whether there is a screen to go back to (retail greys "back" at the root
    /// screen) and whether the page can start a game (`canplaygame`; RACE is grey when not).
    pub can_go_back: bool,
    pub can_play: bool,
    /// Selected difficulty (0 beginner, 1 advanced, 2 expert), the accessor of an open dropdown
    /// and how far its list is revealed (0..=1).
    pub difficulty: usize,
    pub open_combo: Option<String>,
    pub combo_reveal: f32,
    /// The whole session, for controls that read dropdown choices, hidden names or watched text.
    pub session: Option<&'a Session>,
}

/// Front-end state the layouts refer to but do not contain.
#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub can_go_back: bool,
    pub can_play: bool,
    /// `Game.Difficutly`: 0 beginner, 1 advanced, 2 expert (strings 450-452).
    pub difficulty: usize,
    /// The accessor of the dropdown that is open, if any.
    pub open_combo: Option<String>,
    /// How far the open dropdown's list is revealed, 0..=1 (it slides over 16 ticks).
    pub combo_reveal: f32,
    /// Dropdown contents by accessor (without a `(SHORT)` suffix). An accessor not listed here
    /// falls back to the built-in lists (difficulty).
    pub choices: BTreeMap<String, Choice>,
    /// The first row shown of the open dropdown (it scrolls when taller than its maximum).
    pub scroll: usize,
    /// Control `name=`s not to draw (one of two controls sharing a place).
    pub hidden: Vec<String>,
    /// Text for `NFS5.AccessorWatch` labels, by accessor as the layout writes it.
    pub watch: BTreeMap<String, String>,
    /// `NFS5.CarLogo`: the chosen car's logo archive and logo number (entries `2n`, `2n + 1`).
    pub car_logo: Option<(String, usize)>,
    /// Picture drawn for a `room=ID_TRACK` screen (`TrackArt/<name>.fsh`).
    pub track_art: Option<String>,
    /// Values of the level bars by accessor, 0..=127 (`Audio.MusicLevel`, ...).
    pub levels: BTreeMap<String, u32>,
    /// The Options tab's selected panel (`ID_audio_panel`, ...).
    pub panel: String,
    /// The Evolution career is in use (game type 4): the bottom bar shows the account.
    pub career: bool,
}

impl Default for Session {
    fn default() -> Self {
        Session {
            can_go_back: true,
            can_play: true,
            difficulty: 1,
            open_combo: None,
            combo_reveal: 1.0,
            choices: BTreeMap::new(),
            scroll: 0,
            hidden: Vec::new(),
            watch: BTreeMap::new(),
            car_logo: None,
            track_art: None,
            levels: BTreeMap::new(),
            panel: OPTION_PANELS[3].to_string(),
            career: false,
        }
    }
}

/// An accessor without its display suffix: `Track1.Track(SHORT)` is `Track1.Track`.
pub fn accessor_key(accessor: &str) -> String {
    let a = accessor.trim();
    a.split_once('(').map(|(k, _)| k).unwrap_or(a).trim().to_string()
}

/// A named value in a `.sav`: the accessor name, a zero byte, then a 32-bit little-endian value
/// (`Game.Difficutly`, `Game.GameType`). Found by comparing the author's three saves; read-only.
pub fn save_value(save: &[u8], name: &str) -> Option<u32> {
    let mut tag = name.as_bytes().to_vec();
    tag.push(0);
    let at = save.windows(tag.len()).position(|w| w == tag.as_slice())? + tag.len();
    Some(u32::from_le_bytes(save.get(at..at + 4)?.try_into().ok()?))
}

impl<'a> Context<'a> {
    pub fn new(profiles: &'a Profiles) -> Context<'a> {
        Context { profiles, game_dir: None, disabled: false, can_go_back: true, can_play: true, difficulty: 1, open_combo: None, combo_reveal: 1.0, session: None }
    }
}

/// The natural size of an art entry, from its archive header (`None` without a game folder or
/// when the entry is missing).
fn shape_size(game_dir: Option<&Path>, file: &str, index: usize) -> Option<(f32, f32)> {
    let data = load_art(game_dir?, file).ok()?;
    let entry = nfspu_formats::shpi::parse(&data).ok()?.entries.get(index)?.clone();
    Some((entry.width as f32, entry.height as f32))
}

/// Width of a label in layout units: the sum of the font's advances. Without the font file (tests,
/// a missing font) it falls back to 6.5 units per character at 11 points, scaled by point size.
fn label_width(game_dir: Option<&Path>, font: &str, text: &str) -> f32 {
    match game_dir.and_then(|d| load_font(d, font)) {
        Some(f) => f.measure(text) as f32,
        None => text.chars().count() as f32 * 6.5 * point_size(font) / 11.0,
    }
}

fn append_control_with(
    screen: &mut Screen,
    block: &Block,
    template: &ControlTemplate,
    locale: &Locale,
    language: u8,
    context: &Context,
) -> bool {
    let (profiles, game_dir, disabled) = (context.profiles, context.game_dir, context.disabled);
    // The bottom bar shows its cash backdrop and "ACCOUNT" label only in career mode (game type
    // 4) and hides them otherwise (verified in the executable).
    if matches!(block.get("name").map(str::trim), Some(ACCOUNT_LABEL) | Some(CASH_BACKDROP)) && !context.session.is_some_and(|s| s.career) {
        return true;
    }
    // The cash backdrop (`NFS5.ACCOUNTTHING`, the account display control) draws the player's
    // money to the right of the "ACCOUNT" label. Its backdrop art is not drawn yet and the text
    // position is a **guess**.
    if block.get("id").map(str::trim) == Some("NFS5.ACCOUNTTHING") {
        if let Some(text) = context.session.and_then(|s| s.watch.get("Tournament.Data(ACCOUNT)")).cloned() {
            screen.draw.push(Draw::Text { text, pos: [198.0, 464.0], size: point_size("univers7b.ffn"), colour: [1.0; 4], font: "univers7b.ffn".to_string() });
        }
        return true;
    }
    if let (Some(session), Some(name)) = (context.session, block.get("name").map(str::trim)) {
        if session.hidden.iter().any(|h| h == name) {
            return true;
        }
    }
    let data = fields(block, template);
    let number = |k: &str| data.get(k).and_then(|v| eval_number(v, &data));
    let pair = |k: &str| data.get(k).and_then(|v| eval_pair(v, &data));

    // Geometry: the control's own `pos`, and a size from `size` or `width`/`height` (buttons carry
    // width/height rather than a size pair). Text-only controls have no size at all.
    let (x, y) = pair("pos").unwrap_or((0.0, 0.0));
    let size = pair("size").or_else(|| Some((number("width")?, number("height")?)));

    let link = data.get("destination").map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let back = link.is_none()
        && data.get("callback").map(|c| c.trim().eq_ignore_ascii_case("GOBACK")).unwrap_or(false);
    let colour = data.get("maincolor").and_then(|c| eval_colour(c, &data));
    let hover = data.get("hovercolor").and_then(|c| eval_colour(c, &data));

    match template.id.as_str() {
        // Bottom-bar button (`BottomBarButton.ctr`): an icon frame (`startframe` of
        // `shapePathName`) at `posx`, `posy + picoffy`, then the label `space` units to its right
        // at `posy + 8`, white. `align=1` right-aligns the whole button at `posx`. Hover
        // animation and the yellow hover label are not drawn yet.
        "NFS5.BOTTOMBARBUTTON" => {
            if hidden(&data) {
                return true;
            }
            let posx = number("posx").unwrap_or(0.0);
            let posy = number("posy").unwrap_or(0.0);
            let space = number("space").unwrap_or(0.0);
            let file = data.get("shapePathName").map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
            let frame = number("startframe").unwrap_or(0.0).max(0.0) as usize;
            let (pic_w, pic_h) = file.as_deref().and_then(|f| shape_size(game_dir, f, frame)).unwrap_or((20.0, 20.0));
            let font = font_name(template, &data);
            let size = point_size(&font);
            let text = label_text(block, locale, language);
            let text_w = text.as_deref().map(|t| label_width(game_dir, &font, t)).unwrap_or(0.0);
            let total = pic_w + space + text_w;
            let left = if number("align").unwrap_or(0.0) as i32 & 1 == 1 { posx - total } else { posx };
            let height = number("height").unwrap_or(24.0).max(pic_h);
            let callback = data.get("callback").map(|c| c.trim().to_ascii_uppercase()).unwrap_or_default();
            // Disabled state (verified rules, see `Context`): back at the root screen, RACE when
            // the page cannot start a game. Label `disabledlabel` grey 0xff808080; icon at half
            // opacity (measured from a retail recording); no clicks.
            let off = (callback == "GOBACK" && !context.can_go_back) || (callback == "PLAYGAME" && !context.can_play);
            let (label_colour, icon_alpha) = if off { ([128.0 / 255.0, 128.0 / 255.0, 128.0 / 255.0, 1.0], 0.5) } else { ([1.0; 4], 1.0) };
            // The clickable area; transparent like the template's `bg` part.
            screen.draw.push(Draw::Quad {
                rect: [left, posy, total, height],
                colour: [0.0, 0.0, 0.0, 0.0],
                hover: None,
                link: if off { None } else { link.clone() },
                back: !off && callback == "GOBACK",
            });
            if callback == "QUIT" {
                screen.draw.push(Draw::Target { rect: [left, posy, total, height], hit: Hit::Quit });
            } else if !off && !callback.is_empty() && callback != "GOBACK" {
                // RACE (`PLAYGAME`), the location arrows (`TRACK1INC`, ...), options.
                screen.draw.push(Draw::Target { rect: [left, posy, total, height], hit: Hit::Callback(callback.clone()) });
            }
            let area = [left, posy, total, height];
            if let Some(text) = text {
                let label = |colour| Draw::Text { text: text.clone(), pos: [left + pic_w + space, posy + 8.0], size, colour, font: font.clone() };
                if off {
                    screen.draw.push(label(label_colour));
                } else {
                    // The label turns `hoverlabel` yellow (0xffffbb00, `BottomBarButton.ctr`) under
                    // the pointer.
                    let yellow = data.get("hoverlabel").and_then(|c| eval_colour(c, &data)).unwrap_or([1.0, 0.733, 0.0, 1.0]);
                    screen.draw.push(Draw::Shown { when: Show::Hover(area), draw: Box::new(label(yellow)) });
                    screen.draw.push(Draw::Shown { when: Show::Idle(area), draw: Box::new(label(label_colour)) });
                }
            }
            if let Some(file) = file {
                let flip = number("flip").unwrap_or(0.0).max(0.0) as u8;
                let pos = [left, posy + number("picoffy").unwrap_or(0.0)];
                // The icon animates like retail's animating button: the idle frames loop, the
                // hover frames while the pointer is over it, the mouse-down frames while it is
                // pressed (with `seperatehovermousedown=1`; otherwise the hover set). A grey button
                // holds its first frame.
                let set = |prefix: &str| Frames {
                    start: number(&format!("{prefix}startframe")).unwrap_or(frame as f32).max(0.0) as usize,
                    end: number(&format!("{prefix}endframe")).unwrap_or(frame as f32).max(0.0) as usize,
                    ticks: number(&format!("{prefix}totalticks")).unwrap_or(0.0).max(0.0) as u32,
                };
                let (idle, hover) = (set(""), set("hover"));
                let down = if number("seperatehovermousedown").unwrap_or(0.0) as i32 == 1 { set("hovermousedown") } else { hover };
                if off {
                    screen.draw.push(Draw::Picture { file, index: frame, pos, flip, alpha: icon_alpha });
                } else {
                    screen.draw.push(Draw::Anim { file, idle, hover, down, rect: area, pos, flip, alpha: icon_alpha });
                }
            }
            true
        }
        // The half ring over the 3D garage (`extracircle.ctr`, "Extra Circle Thing"): its
        // `pic0..pic7` parts are `extracircle.fsh` frames 0-7 at fixed positions (the control's
        // own position is read-only 0,0). Seen in retail recordings as the dark lens ring around
        // the car on the quick-race page.
        // The 3D car draws nothing in 2D: the viewer's garage scene (`menu_garage.rs`) shows the
        // car behind the page and reads this control's box from the layout for its clicks.
        "NFS5.3DCAR" => true,
        "NFS5.ExtraCircle" => {
            if hidden(&data) {
                return true;
            }
            for part in template.blocks.iter().filter(|b| b.name.starts_with("part.pic")) {
                let Some(file) = part.get("shapePathName.value").map(|s| s.trim().to_string()) else { continue };
                let (px, py) = part.get("pos.value").and_then(|v| eval_pair(v, &data)).unwrap_or((0.0, 0.0));
                let index = part.get("shapeIndex.value").and_then(|v| eval_number(v, &data)).unwrap_or(0.0).max(0.0) as usize;
                screen.draw.push(Draw::Picture { file, index, pos: [x + px, y + py], flip: 0, alpha: 1.0 });
            }
            true
        }
        // Movie in the oval (`MovieCircle.ctr`): the frame fills the control's rectangle. In the
        // layout's `test1` state the template also draws `stencilshape1..9` - a black rectangle and
        // eight `movieoval.fsh` quarter-edge pieces - which together make the oval the movie is
        // cut to. Part positions are relative to the control.
        "NFS5.MOVIECIRCLE" => {
            if hidden(&data) {
                return true;
            }
            let Some((w, h)) = size else { return false };
            let source = data.get("othersource").map(|s| s.trim().to_string()).unwrap_or_default();
            let state = data.get("state").map(|s| s.trim().to_ascii_lowercase()).unwrap_or_default();
            let mut stencil = Vec::new();
            if state == "test1" {
                for part in template.blocks.iter().filter(|b| b.name.starts_with("part.stencilshape")) {
                    let (px, py) = part.get("pos.value").and_then(|v| eval_pair(v, &data)).unwrap_or((0.0, 0.0));
                    match part.get("id") {
                        Some("RECT") => {
                            let Some((sw, sh)) = part.get("size.value").and_then(|v| eval_pair(v, &data)) else { continue };
                            stencil.push(Draw::Quad { rect: [x + px, y + py, sw, sh], colour: [0.0, 0.0, 0.0, 1.0], hover: None, link: None, back: false });
                        }
                        Some("SHAPE") => {
                            let Some(file) = part.get("shapePathName.value") else { continue };
                            let index = part.get("shapeIndex.value").and_then(|v| v.trim().parse().ok()).unwrap_or(0);
                            let flip = part.get("flip.value").and_then(|v| v.trim().parse().ok()).unwrap_or(0);
                            stencil.push(Draw::Picture { file: file.trim().to_string(), index, pos: [x + px, y + py], flip, alpha: 1.0 });
                        }
                        _ => {}
                    }
                }
            }
            screen.draw.push(Draw::Movie { rect: [x, y, w, h], source, stencil });
            true
        }
        // Dropdown (`ComboBox.ctr`, also the colour box): red back rectangle (full size), the
        // control's label at 5,2 (universC11b, white), the current value right-aligned 13 units
        // before the right edge (universC11m, dark), and the down arrow (`weeicons.fsh` #14) 4 from
        // the right edge. Gloss and drop shadow as on transition buttons: the combo class does not
        // call those routines itself, but the retail recording shows the same styling (**guess**
        // on source).
        "NFS5.ComboBoxSlider" | "NFS5.ColorBox" => {
            if hidden(&data) {
                return true;
            }
            let (w, h) = (number("width").unwrap_or(100.0), number("height").unwrap_or(18.0));
            let rect = [x, y, w, h];
            let accessor = accessor_key(data.get("accessor").map(String::as_str).unwrap_or(""));
            let choice = context.session.and_then(|s| s.choices.get(&accessor)).cloned().unwrap_or_else(|| {
                let options = combo_options(&accessor, locale, language);
                let selected = context.difficulty.min(options.len().saturating_sub(1));
                Choice { closed: options.get(selected).cloned().unwrap_or_default(), rows: options.into_iter().map(Row::item).collect(), selected, closed_swatch: None }
            });
            let items = choice.rows.iter().filter(|r| matches!(r, Row::Item { .. })).count();
            // Open: the list slides out downwards from under the control (retail recording, both
            // for the difficulty list and the car list, whose `selectionboxslidedirection` is -1).
            let open = context.open_combo.as_deref() == Some(accessor.as_str()) && items > 0 && !disabled;
            if open {
                let list_w = number("selectionboxwidth").filter(|v| *v > 0.0).unwrap_or(w);
                let max_h = number("selectionboxmaxheight").filter(|v| *v > 0.0).unwrap_or(160.0);
                let scroll = context.session.map(|s| s.scroll).unwrap_or(0);
                let font = data.get("fontpath").map(|s| s.trim().to_string()).unwrap_or_else(|| "Univers8b.ffn".to_string());
                let reveal = context.combo_reveal.clamp(0.0, 1.0);
                let pieces = combo_list(&choice, [x, y + h, list_w, max_h], scroll, &accessor, &font, reveal >= 1.0, game_dir, &mut screen.top);
                let full = list_height(&choice.rows).min(max_h);
                let shown = [x, y + h, list_w + 4.0, (full + 5.0) * reveal];
                // Only the revealed body catches input while sliding; rows/arrows added
                // above take precedence once the list is fully open.
                screen.top.push(Draw::Target {
                    rect: [x, y + h, list_w, full.min(shown[3])],
                    hit: Hit::Inert,
                });
                screen.top.push(Draw::Clipped { rect: shown, draws: pieces });
            }
            if !disabled && items > 0 {
                screen.draw.push(Draw::Target { rect, hit: Hit::ComboToggle(accessor.clone()) });
            }
            if number("glintybox").map(|g| g as i32 != -1).unwrap_or(true) {
                screen.draw.extend(outline(rect, [0.0, 0.0, 0.0, 1.0]));
            }
            screen.draw.extend(button_gloss(rect));
            if number("dropshadow").unwrap_or(1.0) as i32 == 1 {
                screen.draw.extend(button_shadow(rect, 4.0, 4.0));
            }
            let (arrow_w, _) = shape_size(game_dir, "weeicons.fsh", COMBO_ARROW).unwrap_or((10.0, 7.0));
            screen.draw.push(Draw::Picture { file: "weeicons.fsh".into(), index: COMBO_ARROW, pos: [x + w - arrow_w - 4.0, y + 4.0], flip: 0, alpha: 1.0 });
            if let Some(rgb) = choice.closed_swatch {
                // The colour box's closed value: a swatch before the arrow (retail recording,
                // about 18x8 with a black edge; measured, not traced).
                let swatch = [x + w - arrow_w - 4.0 - 4.0 - 20.0, y + 5.0, 20.0, 8.0];
                screen.draw.extend(outline(swatch, [0.0, 0.0, 0.0, 1.0]));
                screen.draw.push(Draw::Quad { rect: swatch, colour: rgb_colour(rgb), hover: None, link: None, back: false });
            }
            let faint = if disabled { 48.0 / 255.0 } else { 1.0 };
            if !choice.closed.is_empty() {
                let value = choice.closed.clone();
                let font = "universC11m.ffn".to_string();
                let left = x + w - 10.0 - label_width(game_dir, &font, &value) - 3.0;
                screen.draw.push(Draw::Text { text: value, pos: [left, y + 2.0], size: point_size(&font), colour: [0.0, 0.0, 0.0, faint], font });
            }
            if let Some(text) = label_text(block, locale, language) {
                let font = "universC11b.ffn".to_string();
                // A black divider after the label (seen in both retail recordings, closed and
                // open; position measured, its source in the code is not traced).
                let divider = x + 5.0 + label_width(game_dir, &font, &text) + 4.0;
                screen.draw.push(Draw::Quad { rect: [divider, y, 1.0, h], colour: [0.0, 0.0, 0.0, 1.0], hover: None, link: None, back: false });
                screen.draw.push(Draw::Text { text, pos: [x + 5.0, y + 2.0], size: point_size(&font), colour: [1.0, 1.0, 1.0, faint], font });
            }
            // Red, yellow under the pointer and while open (retail recording).
            let red = data.get("backrect.color").and_then(|c| eval_colour(c, &data)).unwrap_or([188.0 / 255.0, 0.0, 0.0, 1.0]);
            let yellow = [1.0, 0.733, 0.0, 1.0];
            // Under a popup the page's dropdowns fade like its buttons (retail recording of the settings
            // popup over quick race): the buttons' disabled fill 0x30FF0000 and faint text.
            let faded = [1.0, 0.0, 0.0, 48.0 / 255.0];
            let (fill, hover) = if disabled { (faded, None) } else if open { (yellow, None) } else { (red, Some(yellow)) };
            screen.draw.push(Draw::Quad { rect, colour: fill, hover, link: None, back: false });
            true
        }
        // The chosen car's logo (`CarLogo.ctr`), from the car's catalogue record, at the control's
        // position and natural size. Logo `n` is two half pictures, archive entries `2n` (left)
        // and `2n + 1` (right), both named `n` ("PORS" + "CHE" for logo 4); matched against the
        // retail recording.
        "NFS5.CarLogo" => {
            if hidden(&data) {
                return true;
            }
            if let Some((file, logo)) = context.session.and_then(|s| s.car_logo.clone()) {
                let (left_w, _) = shape_size(game_dir, &file, logo * 2).unwrap_or((0.0, 0.0));
                let (right_w, _) = shape_size(game_dir, &file, logo * 2 + 1).unwrap_or((0.0, 0.0));
                // `align=1` centres the logo on its position (the settings popup; matches the retail
                // recording); `align=0` starts it there (quick race page).
                let x = if number("align").unwrap_or(0.0) as i32 == 1 { (x - (left_w + right_w) / 2.0).round() } else { x };
                screen.draw.push(Draw::Picture { file: file.clone(), index: logo * 2 + 1, pos: [x + left_w, y], flip: 0, alpha: 1.0 });
                screen.draw.push(Draw::Picture { file, index: logo * 2, pos: [x, y], flip: 0, alpha: 1.0 });
            }
            true
        }
        // A level bar (`GouraudIndicatorBar.ctr`): filled from the left to its accessor's level
        // (0..=127). The fill blends `c0` -> `c50` -> `c100` along the bar (**guess**: how retail
        // uses the three colours is not traced). `changeable=1` makes it a slider.
        "NFS5.GouraudIndicatorBar" => {
            if hidden(&data) {
                return true;
            }
            let Some((w, h)) = size else { return false };
            let accessor = accessor_key(data.get("accessor").map(String::as_str).unwrap_or(""));
            let level = context.session.and_then(|s| s.levels.get(&accessor)).copied().unwrap_or(127).min(127);
            if number("changeable").unwrap_or(0.0) != 0.0 {
                screen.draw.push(Draw::Target { rect: [x, y - 3.0, w, h + 6.0], hit: Hit::Slider(accessor, x as i32, w as i32) });
            }
            let colour = |k: &str| data.get(k).and_then(|c| eval_colour(c, &data)).unwrap_or([1.0; 4]);
            let stops = [colour("c0"), colour("c50"), colour("c100")];
            let at = |t: f32| {
                let (a, b, f) = if t <= 0.5 { (stops[0], stops[1], t * 2.0) } else { (stops[1], stops[2], t * 2.0 - 1.0) };
                std::array::from_fn::<f32, 4, _>(|i| a[i] + (b[i] - a[i]) * f)
            };
            let fill = level as f32 / 127.0;
            for (t0, t1) in [(0.0f32, 0.5f32), (0.5, 1.0)] {
                let t1 = t1.min(fill);
                if t1 <= t0 {
                    continue;
                }
                let (x0, x1) = (x + w * t0, x + w * t1);
                let (a, b) = (at(t0), at(t1));
                screen.draw.push(Draw::Gradient { corners: [[x0, y], [x1, y], [x0, y + h], [x1, y + h]], colours: [a, b, a, b], when: Show::Always });
            }
            true
        }
        // An Options tab (`Graphic Tab Button.ctr`): its label at `pos` + `textoffset`, over the
        // `tabs.fsh` frame `selectedshape` when its panel shows or `hovershape` under the pointer
        // (`flipit` mirrors it). **Stand-in**: the unselected look and the corner offsets
        // (`TLoffsetx`, ...) are not drawn.
        "NFS5.GRAPHICTABBUTTON" => {
            let panel = data.get("panel").map(|s| s.trim().to_string()).unwrap_or_default();
            let Some(text) = label_text(block, locale, language) else { return false };
            let font = font_name(template, &data);
            let file = data.get("bgSmallPathName").map(|s| s.trim().to_string()).unwrap_or_else(|| "tabs.fsh".to_string());
            let selected = context.session.map(|s| s.panel.eq_ignore_ascii_case(&panel)).unwrap_or(false);
            let flip = if number("flipit").unwrap_or(0.0) != 0.0 { 1 } else { 0 };
            let shape = |k: &str| number(k).unwrap_or(0.0).max(0.0) as usize;
            let (sw, sh) = shape_size(game_dir, &file, shape("selectedshape")).unwrap_or((label_width(game_dir, &font, &text) + 40.0, 24.0));
            let rect = [x, y, sw, sh];
            let (tx, ty) = (x + number("textoffsetx").unwrap_or(0.0), y + number("textoffsety").unwrap_or(0.0));
            let white = [1.0, 1.0, 1.0, 1.0];
            screen.draw.push(Draw::Text { text, pos: [tx, ty], size: point_size(&font), colour: white, font });
            screen.draw.push(Draw::Target { rect, hit: Hit::Tab(panel) });
            if selected {
                screen.draw.push(Draw::Picture { file, index: shape("selectedshape"), pos: [x, y], flip, alpha: 1.0 });
            } else {
                let pic = Draw::Picture { file, index: shape("hovershape"), pos: [x, y], flip, alpha: 1.0 };
                screen.draw.push(Draw::Shown { when: Show::Hover(rect), draw: Box::new(pic) });
            }
            true
        }
        // A line of text from a data accessor (`AccessorWatch.ctr`): the track name, difficulty and
        // record time on the location screens. Font and colour from the layout.
        "NFS5.AccessorWatch" => {
            if hidden(&data) {
                return true;
            }
            let accessor = data.get("accessor").map(|a| a.trim().to_string()).unwrap_or_default();
            let Some(text) = context.session.and_then(|s| s.watch.get(&accessor)).cloned() else { return false };
            let font = data.get("fontPathName").map(|s| s.trim().to_string()).unwrap_or_else(|| "univers8b.ffn".to_string());
            let colour = data.get("foreColor").and_then(|c| eval_colour(c, &data)).unwrap_or([1.0; 4]);
            let shift = -label_width(game_dir, &font, &text) / 2.0 * number("align").unwrap_or(0.0);
            screen.draw.push(Draw::Text { text, pos: [x + shift, y], size: point_size(&font), colour, font });
            true
        }
        // A transition button is a coloured rectangle plus a label; its template draws the
        // background inset by one unit on every side.
        "NFS5.GOTOBUTTON" | "NFS5.MEDIUMBUTTON" => {
            let Some((w, h)) = size else { return false };
            if w <= 0.0 || h <= 0.0 {
                return false;
            }
            // Disabled state (`disabledbg`): fill `disabledcolor` (template 0x30FF0000), labels
            // white at alpha 0x30, no hover and no click.
            let (fill, hover, link, back, label_alpha) = if disabled {
                let fill = data.get("disabledcolor").and_then(|c| eval_colour(c, &data)).unwrap_or([1.0, 0.0, 0.0, 48.0 / 255.0]);
                (fill, None, None, false, 48.0 / 255.0)
            } else {
                (colour.unwrap_or([0.74, 0.0, 0.0, 1.0]), hover, link.clone(), back, 1.0)
            };
            // Retail paints fill, label, drop shadow, then gloss (see `button_gloss`); this list is
            // topmost first, so the order is reversed here.
            // Hovered (live buttons only): the pressed-in bevel replaces gloss and shadow.
            if template.id == "NFS5.GOTOBUTTON" && !disabled {
                screen.draw.push(Draw::Glint { rect: [x, y, w, h] });
            }
            if template.id == "NFS5.GOTOBUTTON" {
                // `glintybox` other than -1 (template default 1; empty reads as 0): a black
                // 1-unit outline the button's size, drawn last (on top) by the retail button.
                // With it come two black 1-unit "stems": at the left edge, the button's height plus
                // `stemleftheight`; just right of the right edge (x + w), plus `stemrightheight`.
                // Both are drawn even at 0 (measured black in the retail recording).
                if number("glintybox").map(|g| g as i32 != -1).unwrap_or(true) {
                    let black = [0.0, 0.0, 0.0, 1.0];
                    let left = number("stemleftheight").unwrap_or(0.0).max(0.0);
                    let right = number("stemrightheight").unwrap_or(0.0).max(0.0);
                    screen.draw.push(Draw::Quad { rect: [x, y, 1.0, h + left], colour: black, hover: None, link: None, back: false });
                    screen.draw.push(Draw::Quad { rect: [x + w, y, 1.0, h + right], colour: black, hover: None, link: None, back: false });
                    screen.draw.extend(outline([x, y, w, h], black));
                }
                let pointer = [x + 1.0, y + 1.0, w - 2.0, h - 2.0];
                let (idle, hovered) = if disabled { (Show::Always, None) } else { (Show::Idle(pointer), Some(Show::Hover(pointer))) };
                if let Some(when) = hovered {
                    screen.draw.extend(shown(button_bevel([x, y, w, h], 3.0), when));
                }
                screen.draw.extend(shown(button_gloss([x, y, w, h]), idle));
                if number("dropshadow").unwrap_or(1.0) as i32 == 1 {
                    screen.draw.extend(shown(button_shadow([x, y, w, h], 4.0, 4.0), idle));
                }
            }
            if let Some(text) = label_text(block, locale, language) {
                // `GotoButton.ctr` has three label parts and `medium` picks one: 0 = Universc11b
                // at 5,2; bit 1 = Universc11m centred; bit 2 = univers8b centred; bit 4 =
                // univers8b at 5,2. All white.
                let medium = if template.id == "NFS5.GOTOBUTTON" { number("medium").unwrap_or(0.0) as i32 } else { 0 };
                let (font, centred) = match medium {
                    0 => (font_name(template, &data), false),
                    m if m & 1 != 0 => ("Universc11m.ffn".to_string(), true),
                    m if m & 4 != 0 => ("univers8b.ffn".to_string(), false),
                    _ => ("univers8b.ffn".to_string(), true),
                };
                let left = if centred { x + (w - label_width(game_dir, &font, &text)) / 2.0 } else { x + 5.0 };
                screen.draw.push(Draw::Text { text, pos: [left, y + 2.0], size: point_size(&font), colour: [1.0, 1.0, 1.0, label_alpha], font });
            }
            // A button with no destination that runs a game command (the player popup's
            // `SETNAMETHENBACK`, `CREATEPLAYERENTER`, ...): the front end decides what it does.
            let callback = data.get("callback").map(|c| c.trim().to_ascii_uppercase()).unwrap_or_default();
            if !disabled && link.is_none() && !callback.is_empty() && callback != "GOBACK" {
                screen.draw.push(Draw::Target { rect: [x, y, w, h], hit: Hit::Callback(callback) });
            }
            screen.draw.push(Draw::Quad { rect: [x + 1.0, y + 1.0, w - 2.0, h - 2.0], colour: fill, hover, link, back });
            true
        }
        // A bare label control: text at its own position, no box.
        "REAL.LABEL" | "NFS5.LABEL" | "NFS5.HEADER" => {
            let Some(text) = label_text(block, locale, language) else { return false };
            let colour = data
                .get("foreColor")
                .and_then(|c| eval_colour(c, &data))
                .unwrap_or([1.0, 1.0, 1.0, 1.0]);
            // `Label.ctr` shifts the text by (0 - width) / 2 * align: 0 left, 1 centred, 2 right.
            let font = font_name(template, &data);
            let align = number("align").unwrap_or(0.0);
            let shift = -label_width(game_dir, &font, &text) / 2.0 * align;
            screen.draw.push(Draw::Text { text, pos: [x + shift, y], size: point_size(&font), colour, font });
            true
        }
        // Decorative pieces the layout places as plain rectangles or lines.
        "NFS5.GLINTYLINE" | "NFS5.Rectangle" | "NFS5.OutlineRectangle" => {
            let Some((w, h)) = size else { return false };
            if w <= 0.0 || h <= 0.0 {
                return false;
            }
            let fill = data.get("color").and_then(|c| eval_colour(c, &data)).unwrap_or([0.0, 0.0, 0.0, 0.6]);
            screen.draw.push(Draw::Quad { rect: [x, y, w, h], colour: fill, hover: None, link, back });
            true
        }
        // Gradient box: each corner sits at the box corner plus its own offset (offsets are added,
        // never mirrored) and carries its own ARGB colour. A hidden box draws nothing.
        "NFS5.GOURAUDBOX" => {
            if hidden(&data) {
                return true;
            }
            let Some((w, h)) = size else { return false };
            let off = |kx: &str, ky: &str| [number(kx).unwrap_or(0.0), number(ky).unwrap_or(0.0)];
            // The template spells the lower-left Y key `BLoffy`; accept either case.
            let bl_y = if data.contains_key("BLoffy") { "BLoffy" } else { "BLoffY" };
            let offsets = [off("ULoffX", "ULoffY"), off("URoffX", "URoffY"), off("BLoffX", bl_y), off("BRoffX", "BRoffY")];
            let base = [[x, y], [x + w, y], [x, y + h], [x + w, y + h]];
            let corners = std::array::from_fn(|i| [base[i][0] + offsets[i][0], base[i][1] + offsets[i][1]]);
            let default = [0.0, 0.0, 1.0, 1.0];
            let colours = ["cUL", "cUR", "cBL", "cBR"].map(|k| data.get(k).and_then(|c| eval_colour(c, &data)).unwrap_or(default));
            screen.draw.push(Draw::Gradient { corners, colours, when: Show::Always });
            true
        }
        // One image at its natural size; the layout's `size` is the template default and unused
        // (the template sets `autosize`).
        "NFS5.PICTURE" => {
            if hidden(&data) {
                return true;
            }
            let Some(file) = data.get("shapePathName").map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) else { return false };
            let index = number("shapeIndex").unwrap_or(0.0).max(0.0) as usize;
            let flip = number("flip").unwrap_or(0.0) as u8;
            screen.draw.push(Draw::Picture { file, index, pos: [x, y], flip, alpha: 1.0 });
            true
        }
        // A 1-unit outline in `col` (`GlintyRectangle.ctr`: top, bottom, left, right parts). The
        // template's `animate` glint is not drawn.
        "NFS5.GlintyRectangle" => {
            if hidden(&data) {
                return true;
            }
            let (Some(w), Some(h)) = (number("width"), number("height")) else { return false };
            let colour = data.get("col").and_then(|c| eval_colour(c, &data)).unwrap_or([0.0, 0.0, 0.0, 1.0]);
            for rect in [[x, y, w, 1.0], [x, y + h - 1.0, w, 1.0], [x, y, 1.0, h], [x + w - 1.0, y, 1.0, h]] {
                screen.draw.push(Draw::Quad { rect, colour, hover: None, link: None, back: false });
            }
            true
        }
        // The user list (`accessor=UserList.UserList`): one row per saved player, the selected row
        // filled with the template's selected colour. The template colours carry alpha 0, yet the
        // retail screen shows them solid, so alpha is ignored here. Row height and the white
        // selected text are measured from a retail screenshot (**guess** until the `.ffn` fonts
        // are read).
        "NFS5.ListBox" => {
            if hidden(&data) {
                return true;
            }
            if data.get("accessor").map(|a| a.trim()) != Some("UserList.UserList") {
                return false;
            }
            let width = number("width").unwrap_or(100.0);
            let max_height = number("selectionboxmaxheight").unwrap_or(130.0);
            let solid = |k: &str, fallback: [f32; 4]| {
                let mut c = data.get(k).and_then(|c| eval_colour(c, &data)).unwrap_or(fallback);
                c[3] = 1.0;
                c
            };
            // `highlightcolorhover` is not drawn yet.
            let selected_fill = solid("highlightcolorselected", [0.74, 0.0, 0.0, 1.0]);
            let text_colour = solid("textcolor", [0.0, 0.0, 0.0, 1.0]);
            let font = data.get("fontpath").map(|s| s.trim().to_string()).unwrap_or_else(|| "Univers8b.ffn".to_string());
            const ROW: f32 = 14.0;
            for (row, stem) in profiles.names.iter().enumerate() {
                let name = display_name(stem);
                let top = y + 1.0 + row as f32 * ROW;
                if top + ROW > y + 1.0 + max_height {
                    break;
                }
                let rect = [x, top, width, ROW];
                let is_selected = profiles.selected == Some(row);
                // Topmost first: the name above its highlight.
                screen.draw.push(Draw::Target { rect, hit: Hit::Select(row) });
                screen.draw.push(Draw::Text {
                    text: name.to_string(),
                    pos: [x + 2.0, top + 1.0],
                    size: point_size(&font),
                    colour: if is_selected { [1.0, 1.0, 1.0, 1.0] } else { text_colour },
                    font: font.clone(),
                });
                if is_selected {
                    screen.draw.push(Draw::Quad { rect, colour: selected_fill, hover: None, link: None, back: false });
                }
            }
            // Backing box, pushed last so it sits under the rows: the template's `selectionbox`
            // part (at 0,1, width x max height, alpha 0x40). Its colour value is an editor green;
            // retail shows a white tint (measured ~26-31% white over the panel in a retail
            // screenshot), so white at the template alpha is used (**guess**).
            const BACKING: [f32; 4] = [1.0, 1.0, 1.0, 64.0 / 255.0];
            screen.draw.push(Draw::Quad { rect: [x, y + 1.0, width, max_height], colour: BACKING, hover: None, link: None, back: false });
            true
        }
        // The selected player's face from `ALLdrivr.fsh`, at the control's position (inside the
        // 56x66 outline the layout draws around it). No players: nothing drawn (retail shows a
        // `leader` face in that case; not modelled).
        "NFS5.PLAYERPICTURE" => {
            if hidden(&data) {
                return true;
            }
            if let Some(stem) = profiles.selected.and_then(|i| profiles.names.get(i)) {
                screen.draw.push(Draw::Picture { file: "ALLdrivr.fsh".into(), index: face_index(stem), pos: [x, y], flip: 0, alpha: 1.0 });
            }
            true
        }
        // The popup oval: one quarter image drawn four times from the template's own parts, each
        // at its part offset from the control position (template default 113,100).
        "NFS5.POPUPCIRCLE" => {
            if hidden(&data) {
                return true;
            }
            let (x, y) = data.get("pos").or_else(|| data.get("pos.value")).and_then(|v| eval_pair(v, &data)).unwrap_or((0.0, 0.0));
            let mut any = false;
            for part in template.blocks.iter().filter(|b| b.get("id") == Some("SHAPE")) {
                let Some(file) = part.get("shapePathName.value") else { continue };
                let (px, py) = part.get("pos.value").and_then(|v| eval_pair(v, &data)).unwrap_or((0.0, 0.0));
                let index = part.get("shapeIndex.value").and_then(|v| v.trim().parse().ok()).unwrap_or(0);
                let flip = part.get("flip.value").and_then(|v| v.trim().parse().ok()).unwrap_or(0);
                screen.draw.push(Draw::Picture { file: file.trim().to_string(), index, pos: [x + px, y + py], flip, alpha: 1.0 });
                any = true;
            }
            any
        }
        _ => false,
    }
}

/// A four-colour quad from corner colours in the order upper-left, upper-right, lower-left,
/// lower-right, each `[r, g, b, a]`.
fn quad(corners: [[f32; 2]; 4], colours: [[f32; 4]; 4]) -> Draw {
    Draw::Gradient { corners, colours, when: Show::Always }
}

/// Sets when a group of gradient pieces is visible.
fn shown(mut draws: Vec<Draw>, show: Show) -> Vec<Draw> {
    for d in &mut draws {
        if let Draw::Gradient { when, .. } = d {
            *when = show;
        }
    }
    draws
}

/// The hovered transition button's pressed-in bevel (from the retail hover routine, inset `i` = 3):
/// top and left bands from 80% black at the edge to 10% inside, right and bottom bands from 80%
/// white at the edge to 20% inside. The retail bottom band has two irregular corner positions;
/// this draws it symmetric with the others.
fn button_bevel([x, y, w, h]: Rect, i: f32) -> Vec<Draw> {
    let dark = [0.0, 0.0, 0.0, 204.0 / 255.0];
    let dim = [0.0, 0.0, 0.0, 25.0 / 255.0];
    let light = [1.0, 1.0, 1.0, 204.0 / 255.0];
    let soft = [1.0, 1.0, 1.0, 51.0 / 255.0];
    // Outer edge (inset 1) and inner edge (inset 1 + i).
    let (l, t, r, b) = (x + 1.0, y + 1.0, x + w - 1.0, y + h - 1.0);
    let (li, ti, ri, bi) = (l + i, t + i, r - i, b - i);
    vec![
        quad([[l, t], [r, t], [li, ti], [ri, ti]], [dark, dark, dim, dim]),
        quad([[l, t], [li, ti], [l, b], [li, bi]], [dark, dim, dark, dim]),
        quad([[ri, ti], [r, t], [ri, bi], [r, b]], [soft, light, soft, light]),
        quad([[li, bi], [ri, bi], [l, b], [r, b]], [soft, soft, light, light]),
    ]
}

/// The transition button's gloss, drawn over its fill and label when it is not hovered (from the
/// retail button draw routine): over the inner rectangle (inset 1), the top half fades from 50%
/// white to clear and the bottom half from clear to 57% black (alpha 0x91).
fn button_gloss([x, y, w, h]: Rect) -> Vec<Draw> {
    let half = (h - 2.0) * 0.5;
    let (l, r, top, mid, bottom) = (x + 1.0, x + w - 1.0, y + 1.0, y + 1.0 + half, y + 1.0 + 2.0 * half);
    let white = |a: f32| [1.0, 1.0, 1.0, a];
    let black = |a: f32| [0.0, 0.0, 0.0, a];
    vec![
        quad([[l, top], [r, top], [l, mid], [r, mid]], [white(128.0 / 255.0), white(128.0 / 255.0), white(0.0), white(0.0)]),
        quad([[l, mid], [r, mid], [l, bottom], [r, bottom]], [black(0.0), black(0.0), black(145.0 / 255.0), black(145.0 / 255.0)]),
    ]
}

/// The transition button's drop shadow for `dropshadow=1` (offset 4,4 in retail): a right strip
/// and a bottom strip from 80% black (0xCC) at the button edge to 10% (0x19) at the offset, with
/// 5% (0x0C) tapers where the strips start. Same four quads as the retail routine.
fn button_shadow([x, y, w, h]: Rect, ox: f32, oy: f32) -> Vec<Draw> {
    let dark = [0.0, 0.0, 0.0, 204.0 / 255.0];
    let fade = [0.0, 0.0, 0.0, 25.0 / 255.0];
    let faint = [0.0, 0.0, 0.0, 12.0 / 255.0];
    let (r, b) = (x + w, y + h);
    let (xo, yo, ro, bo) = (x + ox, y + oy, x + w + ox, y + h + oy);
    let (x2, y2) = (x + 2.0 * ox, y + 2.0 * oy);
    vec![
        quad([[r, yo], [ro, yo], [r, b], [ro, bo]], [dark, fade, dark, fade]),
        quad([[x2, b], [r, b], [xo, bo], [ro, bo]], [dark, dark, fade, fade]),
        quad([[xo, b], [x2, b], [xo, bo], [xo, bo]], [faint, dark, faint, faint]),
        quad([[r, yo], [ro, yo], [r, y2], [r, y2]], [faint, faint, dark, dark]),
    ]
}

/// A 1-unit outline: top, bottom, left and right strips.
fn outline([x, y, w, h]: Rect, colour: [f32; 4]) -> Vec<Draw> {
    [[x, y, w, 1.0], [x, y + h - 1.0, w, 1.0], [x, y, 1.0, h], [x + w - 1.0, y, 1.0, h]]
        .into_iter()
        .map(|rect| Draw::Quad { rect, colour, hover: None, link: None, back: false })
        .collect()
}

/// The open dropdown list's background, measured from a retail recording (#E9B819).
const COMBO_LIST_YELLOW: [f32; 4] = [0xE9 as f32 / 255.0, 0xB8 as f32 / 255.0, 0x19 as f32 / 255.0, 1.0];

/// Row heights of an open dropdown list: items 14 units, section titles 13 (both from the retail
/// list drawing; the car list's titles advance the pen by 13).
const LIST_ROW: f32 = 14.0;
const LIST_HEADER: f32 = 13.0;
/// Width of the list's scroll bar column (measured from the retail car list, about 12 units).
const SCROLLBAR: f32 = 12.0;

fn row_height(row: &Row) -> f32 {
    match row {
        Row::Header(_) => LIST_HEADER,
        Row::Item { .. } => LIST_ROW,
    }
}

/// The full height of a list: its rows plus the 1-unit bottom edge.
fn list_height(rows: &[Row]) -> f32 {
    rows.iter().map(row_height).sum::<f32>() + 1.0
}

/// The last useful first row of a list `max_h` tall (scrolling further would leave a gap).
pub fn max_scroll(rows: &[Row], max_h: f32) -> usize {
    let mut h = 1.0;
    for i in (0..rows.len()).rev() {
        h += row_height(&rows[i]);
        if h > max_h {
            return i + 1;
        }
    }
    0
}

/// The row that shows item `item` (headers skipped), for scrolling the choice into view.
pub fn item_row(rows: &[Row], item: usize) -> usize {
    rows.iter().enumerate().filter(|(_, r)| matches!(r, Row::Item { .. })).nth(item).map(|(i, _)| i).unwrap_or(0)
}

fn rgb_colour([r, g, b]: [u8; 3]) -> [f32; 4] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}

/// The open list of a dropdown, topmost first, inside `area` (`x`, top, width, maximum height).
/// Click targets go to `targets` (they must sit outside the sliding clip to be hit-tested); rows
/// take clicks only when `ready` (fully slid open).
///
/// Look, measured from retail recordings: flat yellow rows (~#E9B819) with a black outline and a
/// drop shadow; the row under the pointer turns red (0xBC0000) with white text. Section titles
/// are centred black capitals (Univers7b, the font the car list's accessor loads) between a light
/// top line and a dark bottom line (the list code draws a line above and below each). The car
/// list's year column is white, its name starts 22 units in, and the owned row has a paint
/// swatch and a divider. Lists taller than `selectionboxmaxheight` scroll, with a bar of up and
/// down arrows on the right. Styling beyond these points is approximate.
#[allow(clippy::too_many_arguments)]
fn combo_list(choice: &Choice, area: Rect, scroll: usize, accessor: &str, font: &str, ready: bool, game_dir: Option<&Path>, targets: &mut Vec<Draw>) -> Vec<Draw> {
    let [x, top, w, max_h] = area;
    let black = [0.0, 0.0, 0.0, 1.0];
    let total = list_height(&choice.rows);
    let scrolling = total > max_h;
    let height = total.min(max_h);
    let first = if scrolling { scroll.min(max_scroll(&choice.rows, max_h)) } else { 0 };
    let rows_w = if scrolling { w - 1.0 - SCROLLBAR } else { w - 1.0 };
    let bottom = top + height - 1.0;
    let mut pieces = outline([x, top, w, height], black);
    let mut item = choice.rows[..first].iter().filter(|r| matches!(r, Row::Item { .. })).count();
    let mut cy = top;
    let quad = |rect: Rect, colour: [f32; 4]| Draw::Quad { rect, colour, hover: None, link: None, back: false };
    for row in &choice.rows[first..] {
        if cy >= bottom {
            break;
        }
        let rh = row_height(row);
        let r = [x + 1.0, cy, rows_w - 1.0, rh];
        match row {
            Row::Header(title) => {
                let title_font = "Univers7b.ffn".to_string();
                let tw = label_width(game_dir, &title_font, title);
                pieces.push(Draw::Text { text: title.clone(), pos: [r[0] + (r[2] - tw) / 2.0, cy + 2.0], size: point_size(&title_font), colour: black, font: title_font });
                pieces.push(quad([r[0], cy, r[2], 1.0], [1.0, 1.0, 1.0, 0.6]));
                pieces.push(quad([r[0], cy + rh - 1.0, r[2], 1.0], [0.0, 0.0, 0.0, 0.4]));
                pieces.push(quad(r, COMBO_LIST_YELLOW));
            }
            Row::Item { lead, text, swatch, divider } => {
                let hit = [r[0], cy, r[2], rh.min(bottom - cy)];
                if ready {
                    targets.push(Draw::Target { rect: hit, hit: Hit::ComboPick(accessor.to_string(), item) });
                }
                let offset = if swatch.is_some() && !text.is_empty() { 12.0 } else { 0.0 };
                let text_x = if lead.is_some() { r[0] + 22.0 + offset } else { r[0] + 2.0 + offset };
                let line = |colour| Draw::Text { text: text.clone(), pos: [text_x, cy + 1.0], size: point_size(font), colour, font: font.to_string() };
                if ready {
                    pieces.push(Draw::Shown { when: Show::Hover(hit), draw: Box::new(line([1.0; 4])) });
                    pieces.push(Draw::Shown { when: Show::Idle(hit), draw: Box::new(line(black)) });
                } else {
                    pieces.push(line(black));
                }
                if let Some(l) = lead {
                    pieces.push(Draw::Text { text: l.clone(), pos: [r[0] + 2.0 + offset, cy + 1.0], size: point_size(font), colour: [1.0; 4], font: font.to_string() });
                }
                if *divider {
                    pieces.push(quad([r[0] + 20.0 + offset, cy, 1.0, rh], black));
                }
                if let Some(rgb) = swatch {
                    // A paint swatch: small before a name, the whole row in a colour list.
                    let s = if text.is_empty() { [r[0] + 3.0, cy + 3.0, r[2] - 6.0, rh - 6.0] } else { [r[0] + 2.0, cy + 3.0, 8.0, 8.0] };
                    pieces.extend(outline([s[0] - 1.0, s[1] - 1.0, s[2] + 2.0, s[3] + 2.0], black));
                    pieces.push(quad(s, rgb_colour(*rgb)));
                }
                pieces.push(Draw::Quad { rect: r, colour: COMBO_LIST_YELLOW, hover: ready.then_some([188.0 / 255.0, 0.0, 0.0, 1.0]), link: None, back: false });
                item += 1;
            }
        }
        cy += rh;
    }
    if scrolling {
        // Scroll bar: up and down buttons with the arrow icon, and a thumb sized to the view.
        let sx = x + w - 1.0 - SCROLLBAR;
        let (up, down) = ([sx, top + 1.0, SCROLLBAR, SCROLLBAR], [sx, bottom - SCROLLBAR, SCROLLBAR, SCROLLBAR]);
        if ready {
            targets.push(Draw::Target { rect: up, hit: Hit::ComboScroll(accessor.to_string(), -1) });
            targets.push(Draw::Target { rect: down, hit: Hit::ComboScroll(accessor.to_string(), 1) });
        }
        let (arrow_w, arrow_h) = shape_size(game_dir, "weeicons.fsh", COMBO_ARROW).unwrap_or((10.0, 7.0));
        for (button, flip) in [(up, 2u8), (down, 0u8)] {
            pieces.push(Draw::Picture { file: "weeicons.fsh".into(), index: COMBO_ARROW, pos: [(button[0] + (SCROLLBAR - arrow_w) / 2.0).round(), (button[1] + (SCROLLBAR - arrow_h) / 2.0).round()], flip, alpha: 1.0 });
            pieces.extend(outline(button, black));
            pieces.push(quad(button, COMBO_LIST_YELLOW));
        }
        let track = [sx, up[1] + SCROLLBAR, SCROLLBAR, down[1] - up[1] - SCROLLBAR];
        let last = max_scroll(&choice.rows, max_h).max(1) as f32;
        let thumb_h = (track[3] * (height / total)).max(6.0).min(track[3]);
        let thumb_y = track[1] + (track[3] - thumb_h) * (first as f32 / last).min(1.0);
        pieces.push(quad([sx + 1.0, thumb_y, SCROLLBAR - 2.0, thumb_h], [1.0, 1.0, 1.0, 0.85]));
        pieces.push(quad(track, [1.0, 1.0, 1.0, 0.45]));
        pieces.push(quad([sx - 1.0, top, 1.0, height], black));
    }
    pieces.extend(button_shadow([x, top, w, height], 4.0, 4.0));
    pieces
}

/// The dropdown arrow in `weeicons.fsh` (`BarD`, 10x7).
const COMBO_ARROW: usize = 14;

/// The options a dropdown offers for its accessor, in index order. Only `Game.Difficutly`
/// (spelled so in the game data) is known: strings 450-452, "beginner", "advanced", "expert",
/// consecutive in the locale table, which fits the executable's localized-text list for it (the
/// list's construction is not traced). The selected index is read from the player's save
/// (`Game.Difficutly` value; 0 beginner .. 2 expert assumed in that order).
fn combo_options(accessor: &str, locale: &Locale, language: u8) -> Vec<String> {
    match accessor {
        "Game.Difficutly" => (DIFFICULTY_FIRST..DIFFICULTY_FIRST + 3).filter_map(|id| locale.text(id, language)).map(|s| s.trim().to_string()).collect(),
        _ => Vec::new(),
    }
}
const DIFFICULTY_FIRST: u32 = 450;

/// A control placed in its `hidden` state draws nothing.
fn hidden(data: &BTreeMap<String, String>) -> bool {
    data.get("state").map(|s| s.trim().eq_ignore_ascii_case("hidden")).unwrap_or(false)
}

/// Rasterises a gradient box to an RGBA8 image covering its bounding box at `k` pixels per layout
/// unit. Returns the image's top-left in layout units, its pixel size and the pixels. Colours are
/// interpolated across two triangles (UL, UR, BL) and (UR, BR, BL), as a Gouraud-shaded quad is;
/// the retail triangle split is **not verified**.
pub fn rasterise_gradient(corners: &[[f32; 2]; 4], colours: &[[f32; 4]; 4], k: f32) -> ([f32; 2], [u32; 2], Vec<u8>) {
    let min_x = corners.iter().map(|c| c[0]).fold(f32::MAX, f32::min);
    let min_y = corners.iter().map(|c| c[1]).fold(f32::MAX, f32::min);
    let max_x = corners.iter().map(|c| c[0]).fold(f32::MIN, f32::max);
    let max_y = corners.iter().map(|c| c[1]).fold(f32::MIN, f32::max);
    let w = (((max_x - min_x) * k).ceil() as u32).max(1);
    let h = (((max_y - min_y) * k).ceil() as u32).max(1);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let triangles = [[0usize, 1, 2], [1, 3, 2]];
    for py in 0..h {
        for px in 0..w {
            // Pixel centre in layout units.
            let p = [min_x + (px as f32 + 0.5) / k, min_y + (py as f32 + 0.5) / k];
            for t in triangles {
                let [a, b, c] = t.map(|i| corners[i]);
                let area = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
                if area.abs() < 1e-6 {
                    continue;
                }
                let wa = ((b[0] - p[0]) * (c[1] - p[1]) - (c[0] - p[0]) * (b[1] - p[1])) / area;
                let wb = ((c[0] - p[0]) * (a[1] - p[1]) - (a[0] - p[0]) * (c[1] - p[1])) / area;
                let wc = 1.0 - wa - wb;
                if wa < -1e-4 || wb < -1e-4 || wc < -1e-4 {
                    continue;
                }
                let at = ((py * w + px) * 4) as usize;
                for ch in 0..4 {
                    let v = wa * colours[t[0]][ch] + wb * colours[t[1]][ch] + wc * colours[t[2]][ch];
                    rgba[at + ch] = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                }
                break;
            }
        }
    }
    ([min_x, min_y], [w, h], rgba)
}

/// The point size the control's font name carries: `Universc11b.ffn` is 11, `Univers17b.ffn` is
/// 17. The layout may name the font itself; otherwise the template's label part does.
/// The font a control's label uses: the layout's `fontPathName`, else the template label part's,
/// else `Univers17b.ffn`.
fn font_name(template: &ControlTemplate, data: &BTreeMap<String, String>) -> String {
    data.get("fontPathName")
        .map(String::as_str)
        .or_else(|| {
            template
                .blocks
                .iter()
                .find(|b| b.get("id") == Some("LOCALIZEDTEXT"))
                .and_then(|b| b.get("fontPathName.value"))
        })
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Univers17b.ffn".to_string())
}

/// The point size in a font's name: `Universc11b.ffn` is 11.
fn point_size(name: &str) -> f32 {
    let digits: String = name.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<f32>().ok().filter(|size| *size > 0.0).unwrap_or(17.0)
}

/// Reads a front-end font from `FEData/Art` (names are matched ignoring case).
pub fn load_font(game_dir: &Path, name: &str) -> Option<nfspu_formats::ffn::Font> {
    nfspu_formats::ffn::Font::parse(&std::fs::read(art_path(game_dir, name.trim())?).ok()?)
}

/// Draws a line of text with a font into an RGBA8 image, tinted `colour` with the glyph coverage
/// as alpha. Returns the image's top-left relative to the text position (glyph offsets can reach
/// left of or above it), its size and its pixels; `None` for text with no drawable glyphs.
pub fn rasterise_text(font: &nfspu_formats::ffn::Font, text: &str, colour: [f32; 4]) -> Option<([f32; 2], [u32; 2], Vec<u8>)> {
    // Place each glyph, then size the image to fit them all.
    let mut placed = Vec::new();
    let mut pen = 0i32;
    for c in text.chars() {
        let Some(g) = font.glyph(c) else { continue };
        if g.width > 0 && g.height > 0 && c != ' ' {
            placed.push((pen + g.x_offset as i32, g.y_offset as i32, *g));
        }
        pen += g.advance as i32;
    }
    let min_x = placed.iter().map(|p| p.0).min()?;
    let min_y = placed.iter().map(|p| p.1).min()?;
    let max_x = placed.iter().map(|p| p.0 + p.2.width as i32).max()?;
    let max_y = placed.iter().map(|p| p.1 + p.2.height as i32).max()?;
    let (w, h) = ((max_x - min_x) as u32, (max_y - min_y) as u32);
    let rgb = colour.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for (x, y, g) in placed {
        for gy in 0..g.height as u32 {
            for gx in 0..g.width as u32 {
                let src = ((g.y as u32 + gy) * font.atlas_width + g.x as u32 + gx) as usize;
                let Some(&cover) = font.coverage.get(src) else { continue };
                let at = ((((y - min_y) as u32 + gy) * w + (x - min_x) as u32 + gx) * 4) as usize;
                let alpha = (cover as f32 * colour[3]).round() as u8;
                if alpha > rgba[at + 3] {
                    rgba[at..at + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], alpha]);
                }
            }
        }
    }
    Some(([min_x as f32, min_y as f32], [w, h], rgba))
}

/// The control's label text, from its `stringID`. String 0 is the table's placeholder ("Hello,
/// World!"); a control naming it (the bottom bar's RACE button) shows no label in retail.
fn label_text(block: &Block, locale: &Locale, language: u8) -> Option<String> {
    let id = block.string_id().filter(|id| *id != 0)?;
    let text = locale.text(id, language)?.trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// A value the template arithmetic can name: a number, `data.<key>`, `control.<key>`, or
/// `control.size.width` / `.height` (halves of the control's `size` pair). Font metrics such as
/// `label.size.width` are not known here and give `None`.
fn token(name: &str, data: &BTreeMap<String, String>) -> Option<f32> {
    let name = name.trim();
    if let Some(side) = name.strip_prefix("control.size.") {
        let (w, h) = data.get("size")?.split_once(',')?;
        return match side {
            "width" => w.trim().parse().ok(),
            "height" => h.trim().parse().ok(),
            _ => None,
        };
    }
    if let Some(key) = name.strip_prefix("data.").or_else(|| name.strip_prefix("control.")) {
        return data.get(key).and_then(|v| v.trim().parse::<f32>().ok());
    }
    name.parse::<f32>().ok()
}

/// Evaluates the arithmetic the templates use: numbers and names (see `token`) combined with
/// `+ - * /` and parentheses, with the usual precedence (`640-(160+112)-160`). Anything it cannot
/// evaluate (font metrics, `?` ternaries) returns `None`.
pub fn eval_number(expr: &str, data: &BTreeMap<String, String>) -> Option<f32> {
    let chars: Vec<char> = expr.chars().filter(|c| !c.is_whitespace()).collect();
    let mut at = 0;
    let value = sum(&chars, &mut at, data)?;
    (at == chars.len()).then_some(value)
}

fn sum(c: &[char], at: &mut usize, data: &BTreeMap<String, String>) -> Option<f32> {
    let mut v = product(c, at, data)?;
    while let Some(&op) = c.get(*at).filter(|&&op| op == '+' || op == '-') {
        *at += 1;
        let r = product(c, at, data)?;
        v = if op == '+' { v + r } else { v - r };
    }
    Some(v)
}

fn product(c: &[char], at: &mut usize, data: &BTreeMap<String, String>) -> Option<f32> {
    let mut v = atom(c, at, data)?;
    while let Some(&op) = c.get(*at).filter(|&&op| op == '*' || op == '/') {
        *at += 1;
        let r = atom(c, at, data)?;
        v = if op == '*' { v * r } else { v / r };
    }
    Some(v)
}

fn atom(c: &[char], at: &mut usize, data: &BTreeMap<String, String>) -> Option<f32> {
    match c.get(*at)? {
        '(' => {
            *at += 1;
            let v = sum(c, at, data)?;
            (c.get(*at) == Some(&')')).then(|| *at += 1)?;
            Some(v)
        }
        '-' => {
            *at += 1;
            Some(-atom(c, at, data)?)
        }
        _ => {
            let start = *at;
            while c.get(*at).map(|ch| ch.is_alphanumeric() || *ch == '.' || *ch == '_').unwrap_or(false) {
                *at += 1;
            }
            let name: String = c[start..*at].iter().collect();
            token(&name, data)
        }
    }
}

/// `x,y` where either side may be an expression.
pub fn eval_pair(expr: &str, data: &BTreeMap<String, String>) -> Option<(f32, f32)> {
    let (a, b) = expr.split_once(',')?;
    Some((eval_number(a, data)?, eval_number(b, data)?))
}

/// `0xAARRGGBB`, optionally written as `cond?a:b` (used for the label colour of medium buttons).
pub fn eval_colour(expr: &str, data: &BTreeMap<String, String>) -> Option<[f32; 4]> {
    let expr = expr.trim();
    if let Some((cond, rest)) = expr.split_once('?') {
        let (yes, no) = rest.split_once(':')?;
        let flag = eval_number(cond, data).unwrap_or(0.0) != 0.0;
        return eval_colour(if flag { yes } else { no }, data);
    }
    let hex = expr.strip_prefix("0x").or_else(|| expr.strip_prefix("0X"))?;
    let v = u32::from_str_radix(hex, 16).ok()?;
    let a = ((v >> 24) & 0xff) as f32 / 255.0;
    let r = ((v >> 16) & 0xff) as f32 / 255.0;
    let g = ((v >> 8) & 0xff) as f32 / 255.0;
    let b = (v & 0xff) as f32 / 255.0;
    Some([r, g, b, a])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn colour_is_argb() {
        assert_eq!(eval_colour("0xffbc0000", &data(&[])).unwrap(), [0.7372549, 0.0, 0.0, 1.0]);
        assert_eq!(eval_colour("0x30ff0000", &data(&[])).unwrap()[3], 0.1882353);
    }

    #[test]
    fn colour_ternary_picks_a_branch() {
        let d = data(&[("medium", "1")]);
        assert_eq!(eval_colour("data.medium?0x00000000:0xffffffff", &d).unwrap(), [0.0, 0.0, 0.0, 0.0]);
        let d = data(&[("medium", "0")]);
        assert_eq!(eval_colour("data.medium?0x00000000:0xffffffff", &d).unwrap(), [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn numbers_with_data_and_offset() {
        let d = data(&[("width", "162"), ("height", "18")]);
        assert_eq!(eval_number("162", &d), Some(162.0));
        assert_eq!(eval_number("data.width-2", &d), Some(160.0));
        assert_eq!(eval_number("control.height", &d), Some(18.0));
        assert_eq!(eval_pair("data.width-2,data.height-2", &d), Some((160.0, 16.0)));
        assert_eq!(eval_number("(label.size.width+data.space)", &d), None, "font metrics are not evaluated");
        // `MovieCircle.ctr` stencil positions.
        assert_eq!(eval_number("640-(160+112)-160", &d), Some(208.0));
        assert_eq!(eval_number("548-(158+100)-116", &d), Some(174.0));
        assert_eq!(eval_number("146-100+12", &d), Some(58.0));
        assert_eq!(eval_number("(data.width-2)/2*3", &d), Some(240.0));
        let sized = data(&[("size", "416,348")]);
        assert_eq!(eval_number("control.size.height", &sized), Some(348.0));
        assert_eq!(eval_number("2+", &d), None);
        assert_eq!(eval_pair("1,1", &d), Some((1.0, 1.0)));
    }

    #[test]
    fn a_button_with_a_command_and_no_destination_is_clickable() {
        // The player popup's "continue" (`selectname.lay`): no destination, `SETNAMETHENBACK`.
        let mut block = button_block();
        block.entries.retain(|(k, _)| k != "destination");
        block.entries.push(("callback".into(), "SETNAMETHENBACK".into()));
        let template = ControlTemplate { id: "NFS5.GOTOBUTTON".into(), ..Default::default() };
        let mut screen = Screen::default();
        assert!(append_control(&mut screen, &block, &template, &Locale::default(), 0));
        assert_eq!(screen.hit_at(300.0, 262.0), Some(Hit::Callback("SETNAMETHENBACK".into())));
        // A destination still wins over the command (the main menu's `SETTYPE...` buttons).
        let mut screen = Screen::default();
        let mut block = button_block();
        block.entries.push(("callback".into(), "SETTYPEQUICKRACE".into()));
        assert!(append_control(&mut screen, &block, &template, &Locale::default(), 0));
        assert_eq!(screen.hit_at(300.0, 262.0), Some(Hit::Goto("quickrace".into())));
    }

    /// A real transition button as it appears in `singleplayer.lay`.
    fn button_block() -> Block {
        Block {
            name: String::new(),
            index: Some(8),
            entries: [
                ("id", "NFS5.GOTOBUTTON"),
                ("name", "ID_GOTOBUTTON"),
                ("pos", "243,254"),
                ("stringID", "15704"),
                ("destination", "quickrace"),
                ("width", "152"),
                ("height", "18"),
                ("maincolor", "0xFFBC0000"),
                ("hovercolor", "0xffffBB00"),
                ("medium", "0"),
            ]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        }
    }

    #[test]
    fn a_transition_button_becomes_a_quad_and_a_label() {
        let template = ControlTemplate {
            id: "NFS5.GOTOBUTTON".into(),
            description: "Transition Button".into(),
            parts: vec!["label".into(), "bg".into()],
            states: vec!["normal".into()],
            blocks: vec![Block {
                name: String::new(),
                index: None,
                entries: [("id", "DATA"), ("maincolor.value", "0xFFBC0000"), ("hovercolor.value", "0xffffBB00"), ("width.value", "157"), ("height.value", "18")]
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            }],
        };
        let locale = Locale::parse_csv(
            "String ID,ENGLISH_IDS,FRENCH_IDS,GERMAN_IDS,ITALIAN_IDS,SPANISH_IDS,SWEDISH_IDS,PORTUGUESE_ID,DescID,Conditionals,Comments,OrigFile\n\
15704,quick race,,,,,,,IDS_QUICKRACE,,,\n",
        );
        let mut screen = Screen::default();
        assert!(append_control(&mut screen, &button_block(), &template, &locale, 0));
        // Topmost first: stems (2), black outline (4), hover bevel (4), gloss (2 bands), drop shadow
        // (4 quads), label, fill.
        assert_eq!(screen.draw.len(), 18);
        assert!(matches!(&screen.draw[1], Draw::Quad { rect, .. } if *rect == [243.0 + 152.0, 254.0, 1.0, 18.0]), "right stem just outside the button");
        let inner = [244.0, 255.0, 150.0, 16.0];
        assert!(matches!(&screen.draw[2], Draw::Quad { colour, .. } if *colour == [0.0, 0.0, 0.0, 1.0]), "black outline on top");
        assert!(matches!(&screen.draw[6], Draw::Gradient { when: Show::Hover(r), .. } if *r == inner), "bevel shows on hover");
        match &screen.draw[10] {
            Draw::Gradient { corners, colours, when } => {
                assert_eq!(*when, Show::Idle(inner), "gloss hides on hover");
                assert_eq!(corners[0], [244.0, 255.0], "gloss covers the inner rectangle");
                assert_eq!(corners[2], [244.0, 263.0], "top band is half the inner height");
                assert_eq!(colours[0][3], 128.0 / 255.0);
            }
            other => panic!("expected the gloss, got {other:?}"),
        }
        match &screen.draw[12] {
            Draw::Gradient { corners, .. } => assert_eq!(corners[0], [395.0, 258.0], "right shadow strip starts at the edge, 4 down"),
            other => panic!("expected the shadow, got {other:?}"),
        }
        match &screen.draw[17] {
            Draw::Quad { rect, colour, hover, link, back } => {
                assert_eq!(*rect, [244.0, 255.0, 150.0, 16.0], "the template insets the background by one unit");
                assert_eq!(*colour, [0.7372549, 0.0, 0.0, 1.0]);
                assert_eq!(*hover, Some([1.0, 0.73333335, 0.0, 1.0]));
                assert_eq!(link.as_deref(), Some("quickrace"));
                assert!(!back);
            }
            other => panic!("expected a quad, got {other:?}"),
        }
        match &screen.draw[16] {
            Draw::Text { text, pos, .. } => {
                assert_eq!(text, "quick race");
                assert_eq!(*pos, [248.0, 256.0]);
            }
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[test]
    fn link_at_finds_the_topmost_control() {
        let screen = Screen {
            draw: vec![
                // The first section is on top.
                Draw::Quad { rect: [10.0, 10.0, 20.0, 20.0], colour: [0.0; 4], hover: None, link: Some("over".into()), back: false },
                Draw::Quad { rect: [0.0, 0.0, 100.0, 100.0], colour: [0.0; 4], hover: None, link: Some("under".into()), back: false },
            ],
            ..Default::default()
        };
        assert_eq!(screen.link_at(15.0, 15.0), Some("over"));
        assert_eq!(screen.link_at(90.0, 90.0), Some("under"));
        assert_eq!(screen.link_at(200.0, 200.0), None);
    }

    #[test]
    fn a_back_button_reports_back() {
        // `singleplayer.lay`'s back button has an empty `destination` and `callback=GOBACK`.
        let mut block = button_block();
        block.entries.retain(|(k, _)| k != "destination");
        block.entries.push(("destination".to_string(), String::new()));
        block.entries.push(("callback".to_string(), "GOBACK".to_string()));
        let template = ControlTemplate {
            id: "NFS5.GOTOBUTTON".into(),
            description: "Transition Button".into(),
            parts: vec![],
            states: vec![],
            blocks: vec![],
        };
        let mut screen = Screen::default();
        assert!(append_control(&mut screen, &block, &template, &Locale::default(), 0));
        assert_eq!(screen.hit_at(250.0, 260.0), Some(Hit::Back), "draw = {:?}", screen.draw);
    }

    fn gouraud_template() -> ControlTemplate {
        ControlTemplate {
            id: "NFS5.GOURAUDBOX".into(),
            description: "Gouraud Box".into(),
            parts: vec![],
            states: vec![],
            blocks: vec![Block {
                name: String::new(),
                index: None,
                entries: [("id", "DATA"), ("cUL.value", "0xFF0000FF"), ("ULoffX.value", "0"), ("BLoffy.value", "0")]
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            }],
        }
    }

    /// The top bevel of the `accountlogin.lay` frame: 287x3, white fading down, upper-left
    /// corner pushed 3 units right to make a mitre.
    #[test]
    fn a_gouraud_box_becomes_a_skewed_four_colour_quad() {
        let block = Block {
            name: String::new(),
            index: Some(3),
            entries: [
                ("id", "NFS5.GOURAUDBOX"),
                ("state", "normal"),
                ("pos", "177,349"),
                ("size", "287,3"),
                ("cUL", "0xe5ffffff"),
                ("cUR", "0xe5ffffff"),
                ("cBL", "0x19ffffff"),
                ("cBR", "0x19ffffff"),
                ("ULoffX", "3"),
                ("BRoffX", "-3"),
            ]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        };
        let mut screen = Screen::default();
        assert!(append_control(&mut screen, &block, &gouraud_template(), &Locale::default(), 0));
        match &screen.draw[..] {
            [Draw::Gradient { corners, colours, .. }] => {
                assert_eq!(*corners, [[180.0, 349.0], [464.0, 349.0], [177.0, 352.0], [461.0, 352.0]]);
                assert_eq!(colours[0], [1.0, 1.0, 1.0, 0.8980392]);
                assert_eq!(colours[3], [1.0, 1.0, 1.0, 0.09803922]);
            }
            other => panic!("expected one gradient, got {other:?}"),
        }
    }

    #[test]
    fn a_hidden_gouraud_box_draws_nothing_but_counts_as_handled() {
        let block = Block {
            name: String::new(),
            index: Some(1),
            entries: [("id", "NFS5.GOURAUDBOX"), ("state", "hidden"), ("pos", "0,0"), ("size", "10,10")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };
        let mut screen = Screen::default();
        assert!(append_control(&mut screen, &block, &gouraud_template(), &Locale::default(), 0));
        assert!(screen.draw.is_empty());
    }

    fn block(pairs: &[(&str, &str)]) -> Block {
        Block { name: String::new(), index: Some(1), entries: pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect() }
    }

    #[test]
    fn frame_ids_name_screen_layouts() {
        assert_eq!(frame_layout("ID_MOVIEOVAL").as_deref(), Some("screenmovieoval"));
        assert_eq!(frame_layout("ID_OPTIONOVAL").as_deref(), Some("screenoptionoval"));
        assert_eq!(frame_layout("ID_SCREEN").as_deref(), Some("screenempty"));
        assert_eq!(frame_layout("ID_NONE"), None);
        assert_eq!(frame_layout(""), None);
    }

    /// A `GameSetup.lay` picture: natural size, so the layout's 50x150 is ignored.
    #[test]
    fn a_picture_names_its_archive_entry_and_flip() {
        let template = ControlTemplate { id: "NFS5.PICTURE".into(), description: "Picture".into(), parts: vec![], states: vec![], blocks: vec![] };
        let b = block(&[("id", "NFS5.PICTURE"), ("pos", "448,274"), ("size", "50,150"), ("shapePathName", "MovieOval.fsh"), ("shapeIndex", "7"), ("flip", "3")]);
        let mut screen = Screen::default();
        assert!(append_control(&mut screen, &b, &template, &Locale::default(), 0));
        assert_eq!(screen.draw, vec![Draw::Picture { file: "MovieOval.fsh".into(), index: 7, pos: [448.0, 274.0], flip: 3, alpha: 1.0 }]);
    }

    /// `PopUp.ctr`: four SHAPE parts of one 207x175 quarter, mirrored into a full oval.
    #[test]
    fn a_popup_circle_is_four_mirrored_quarters() {
        let part = |pos: &str, flip: Option<&str>| {
            let mut e = vec![("id", "SHAPE"), ("pos.value", pos), ("shapePathName.value", "PopUp.fsh"), ("shapeIndex.value", "0")];
            if let Some(f) = flip {
                e.push(("flip.value", f));
            }
            block(&e)
        };
        let template = ControlTemplate {
            id: "NFS5.POPUPCIRCLE".into(),
            description: "Popup Circle".into(),
            parts: vec![],
            states: vec![],
            blocks: vec![
                block(&[("description", "Popup Circle"), ("pos.value", "113,100")]),
                part("0,0", None),
                part("207,0", Some("1")),
                part("0,175", Some("2")),
                part("207,175", Some("3")),
            ],
        };
        let b = block(&[("id", "NFS5.POPUPCIRCLE"), ("state", "normal"), ("size", "50,150")]);
        let mut screen = Screen::default();
        assert!(append_control(&mut screen, &b, &template, &Locale::default(), 0));
        let placed: Vec<([f32; 2], u8)> = screen
            .draw
            .iter()
            .map(|d| match d {
                Draw::Picture { pos, flip, .. } => (*pos, *flip),
                other => panic!("expected pictures, got {other:?}"),
            })
            .collect();
        assert_eq!(placed, vec![([113.0, 100.0], 0), ([320.0, 100.0], 1), ([113.0, 275.0], 2), ([320.0, 275.0], 3)]);
    }

    #[test]
    fn a_gradient_rasterises_with_its_corner_colours_and_mitre() {
        // 4x2 box, top white opaque, bottom transparent, upper-left pushed 2 units right.
        let corners = [[2.0, 0.0], [4.0, 0.0], [0.0, 2.0], [4.0, 2.0]];
        let white = [1.0, 1.0, 1.0, 1.0];
        let clear = [1.0, 1.0, 1.0, 0.0];
        let (origin, size, rgba) = rasterise_gradient(&corners, &[white, white, clear, clear], 1.0);
        assert_eq!(origin, [0.0, 0.0]);
        assert_eq!(size, [4, 2]);
        let alpha = |x: u32, y: u32| rgba[((y * 4 + x) * 4 + 3) as usize];
        assert_eq!(alpha(0, 0), 0, "outside the mitre stays empty");
        assert!(alpha(3, 0) > 150, "top right is near opaque");
        assert!(alpha(3, 1) < alpha(3, 0), "fades toward the bottom");
    }

    #[test]
    fn profiles_list_saves_except_the_default_and_select_the_last_player() {
        let dir = std::env::temp_dir().join(format!("nfspu_profiles_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for f in ["XXDefXX.sav", "1zed.sav", "0Greg.sav", "8Driver.sav", "notes.txt"] {
            std::fs::write(dir.join(f), b"").unwrap();
        }
        assert_eq!(
            Profiles::load(&dir),
            Profiles { names: vec!["8Driver".into(), "0Greg".into(), "1zed".into()], selected: Some(0) },
            "sorted by shown name, not by face digit"
        );
        std::fs::write(dir.join("player.lst"), b"0Greg").unwrap();
        assert_eq!(Profiles::load(&dir).selected, Some(1));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(Profiles::load(&dir), Profiles::default(), "a missing folder is an empty list");
    }

    #[test]
    fn text_rasterises_from_glyph_coverage() {
        use nfspu_formats::ffn::{Font, Glyph};
        // One 2x1 glyph 'A' (coverage 255, 136) at atlas (0,0), advance 3, top 1 below the line.
        let glyph = Glyph { width: 2, height: 1, x: 0, y: 0, advance: 3, x_offset: 0, y_offset: 1 };
        let font = Font { line_height: 4, glyphs: [(b'A' as u16, glyph), (b' ' as u16, Glyph { width: 1, height: 1, advance: 2, ..glyph })].into(), atlas_width: 2, atlas_height: 1, coverage: vec![255, 136] };
        let (origin, size, rgba) = rasterise_text(&font, "A A", [1.0, 0.0, 0.0, 1.0]).unwrap();
        assert_eq!(origin, [0.0, 1.0]);
        assert_eq!(size, [7, 1], "two glyphs 5 apart (3 + space 2), 2 wide each");
        assert_eq!(&rgba[0..8], &[255, 0, 0, 255, 255, 0, 0, 136]);
        assert_eq!(rgba[2 * 4 + 3], 0, "the gap is empty");
        assert!(rasterise_text(&font, "   ", [1.0; 4]).is_none(), "spaces alone draw nothing");
    }

    #[test]
    fn a_disabled_transition_button_is_faint_and_inert() {
        let template = ControlTemplate { id: "NFS5.GOTOBUTTON".into(), description: String::new(), parts: vec![], states: vec![], blocks: vec![] };
        let mut b = button_block();
        b.entries.push(("disabledcolor".into(), "0x30FF0000".into()));
        let locale = Locale::parse_csv("String ID,ENGLISH_IDS\n15704,quick race\n");
        let profiles = Profiles::default();
        let context = Context { profiles: &profiles, game_dir: None, disabled: true, can_go_back: true, can_play: true, difficulty: 1, open_combo: None, combo_reveal: 1.0, session: None };
        let mut screen = Screen::default();
        assert!(append_control_with(&mut screen, &b, &template, &locale, 0, &context));
        assert_eq!(screen.hit_at(250.0, 260.0), None, "no click while disabled");
        // Disabled buttons keep their gloss and shadow (retail draws both unless hovered).
        match &screen.draw[12..] {
            [Draw::Text { colour: text, .. }, Draw::Quad { colour, hover: None, .. }] => {
                assert_eq!(*colour, [1.0, 0.0, 0.0, 48.0 / 255.0]);
                assert_eq!(text[3], 48.0 / 255.0);
            }
            other => panic!("expected a faint quad and label, got {other:?}"),
        }
    }

    #[test]
    fn the_account_pieces_stay_hidden_outside_career_mode() {
        let template = ControlTemplate { id: "REAL.LABEL".into(), description: String::new(), parts: vec![], states: vec![], blocks: vec![] };
        let locale = Locale::parse_csv("String ID,ENGLISH_IDS\n10504,ACCOUNT\n");
        let b = block(&[("id", "REAL.LABEL"), ("name", "ID_ACCOUNTTHINGY"), ("pos", "191,464"), ("stringID", "10504")]);
        let mut screen = Screen::default();
        assert!(append_control(&mut screen, &b, &template, &locale, 0), "handled, not skipped");
        assert!(screen.draw.is_empty());
    }

    /// `GameSetup.lay`'s movie: the oval stencil comes from the template's named parts.
    #[test]
    fn the_movie_circle_builds_its_oval_stencil() {
        let part = |name: &str, entries: &[(&str, &str)]| {
            let mut b = block(entries);
            b.name = name.to_string();
            b
        };
        let template = ControlTemplate {
            id: "NFS5.MOVIECIRCLE".into(),
            description: "Movie In A Circle".into(),
            parts: vec![],
            states: vec![],
            blocks: vec![
                part("part.area", &[("id", "RECT"), ("pos.value", "0,0"), ("size.value", "control.size.width,control.size.height")]),
                part("part.stencilshape1", &[("id", "RECT"), ("pos.value", "192-112,146-100+12"), ("size.value", "256,256-24")]),
                part("part.stencilshape4", &[("id", "SHAPE"), ("pos.value", "640-(160+112)-160,94-100"), ("shapePathName.value", "movieoval.fsh"), ("shapeIndex.value", "0"), ("flip.value", "1")]),
            ],
        };
        let b = block(&[("id", "NFS5.MOVIECIRCLE"), ("state", "test1"), ("pos", "112,100"), ("size", "416,348"), ("othersource", "demoloop")]);
        let mut screen = Screen::default();
        assert!(append_control(&mut screen, &b, &template, &Locale::default(), 0));
        match &screen.draw[..] {
            [Draw::Movie { rect, source, stencil }] => {
                assert_eq!(*rect, [112.0, 100.0, 416.0, 348.0]);
                assert_eq!(source, "demoloop");
                assert_eq!(stencil.len(), 2, "the area part is not part of the stencil");
                assert!(matches!(&stencil[0], Draw::Quad { rect, .. } if *rect == [192.0, 158.0, 256.0, 232.0]));
                assert!(matches!(&stencil[1], Draw::Picture { pos, flip: 1, .. } if *pos == [320.0, 94.0]));
            }
            other => panic!("expected a movie, got {other:?}"),
        }
    }

    /// `singleplayer.lay`'s difficulty dropdown, closed.
    #[test]
    fn the_difficulty_dropdown_shows_label_value_and_arrow() {
        let template = ControlTemplate { id: "NFS5.ComboBoxSlider".into(), description: String::new(), parts: vec![], states: vec![], blocks: vec![] };
        let locale = Locale::parse_csv("String ID,ENGLISH_IDS\n451,advanced\n15707,difficulty\n");
        let b = block(&[("id", "NFS5.ComboBoxSlider"), ("pos", "243,206"), ("accessor", "Game.Difficutly"), ("width", "152"), ("height", "18"), ("dropshadow", "1"), ("stringID", "15707")]);
        let mut screen = Screen::default();
        assert!(append_control(&mut screen, &b, &template, &locale, 0));
        let texts: Vec<&str> = screen.draw.iter().filter_map(|d| if let Draw::Text { text, .. } = d { Some(text.as_str()) } else { None }).collect();
        assert_eq!(texts, vec!["advanced", "difficulty"]);
        assert!(screen.draw.contains(&Draw::Picture { file: "weeicons.fsh".into(), index: 14, pos: [243.0 + 152.0 - 10.0 - 4.0, 210.0], flip: 0, alpha: 1.0 }));
        assert!(matches!(screen.draw.last(), Some(Draw::Quad { rect, .. }) if *rect == [243.0, 206.0, 152.0, 18.0]), "the red back is lowest");
    }

    /// The open dropdown lists all three difficulties with click targets; the chosen one is red.
    #[test]
    fn an_open_dropdown_lists_its_options() {
        let template = ControlTemplate { id: "NFS5.ComboBoxSlider".into(), description: String::new(), parts: vec![], states: vec![], blocks: vec![] };
        let locale = Locale::parse_csv("String ID,ENGLISH_IDS\n450,beginner\n451,advanced\n452,expert\n15707,difficulty\n");
        let b = block(&[("id", "NFS5.ComboBoxSlider"), ("pos", "243,206"), ("accessor", "Game.Difficutly"), ("width", "152"), ("height", "18"), ("stringID", "15707")]);
        let profiles = Profiles::default();
        let open = Context { profiles: &profiles, game_dir: None, disabled: false, can_go_back: true, can_play: true, difficulty: 2, open_combo: Some("Game.Difficutly".into()), combo_reveal: 1.0, session: None };
        let mut screen = Screen::default();
        assert!(append_control_with(&mut screen, &b, &template, &locale, 0, &open));
        lift_top(&mut screen);
        assert_eq!(screen.hit_at(250.0, 210.0), Some(Hit::ComboToggle("Game.Difficutly".into())));
        assert_eq!(screen.hit_at(250.0, 226.0), Some(Hit::ComboPick("Game.Difficutly".into(), 0)));
        assert_eq!(screen.hit_at(250.0, 254.0), Some(Hit::ComboPick("Game.Difficutly".into(), 2)));
        let texts: Vec<&str> = screen.draw.iter().filter_map(|d| if let Draw::Text { text, .. } = d { Some(text.as_str()) } else { None }).collect();
        assert_eq!(texts, vec!["expert", "difficulty"], "row texts are wrapped by hover state; the closed value and label follow");
        let clipped: Vec<&Draw> = screen.draw.iter().filter_map(|d| if let Draw::Clipped { draws, .. } = d { Some(draws.iter()) } else { None }).flatten().collect();
        let rows = clipped.iter().filter(|d| matches!(d, Draw::Shown { .. })).count();
        assert_eq!(rows, 6, "each of three rows has a hover and an idle text, inside the sliding clip");
        // Half-way through the slide the rows are painted but not choosable yet (the list still catches the click).
        let sliding = Context { combo_reveal: 0.5, ..open };
        let mut half = Screen::default();
        assert!(append_control_with(&mut half, &b, &template, &locale, 0, &sliding));
        half.draw.push(Draw::Target { rect: [243.0, 224.0, 152.0, 60.0], hit: Hit::Callback("BEHIND".into()) });
        lift_top(&mut half);
        assert_eq!(half.hit_at(250.0, 226.0), Some(Hit::Inert), "visible rows consume clicks while sliding");
        assert_eq!(half.hit_at(250.0, 254.0), Some(Hit::Callback("BEHIND".into())), "the unrevealed area does not intercept input");
    }

    /// What `load_screen_with` does once every control is placed.
    fn lift_top(screen: &mut Screen) {
        let top = std::mem::take(&mut screen.top);
        screen.draw.splice(0..0, top);
    }

    /// The quick-race car dropdown: headers are not choosable, items count without them, the list
    /// scrolls past its maximum height and the arrows scroll it.
    #[test]
    fn a_long_list_with_headers_scrolls() {
        let template = ControlTemplate { id: "NFS5.ComboBoxSlider".into(), description: String::new(), parts: vec![], states: vec![], blocks: vec![] };
        let b = block(&[("id", "NFS5.ComboBoxSlider"), ("pos", "152,47"), ("accessor", "zMergedList.Car(SHORT)"), ("width", "162"), ("height", "18"), ("selectionboxwidth", "262"), ("selectionboxmaxheight", "160")]);
        let mut rows = vec![Row::Header("FACTORY STOCK".into())];
        rows.extend((0..20).map(|i| Row::Item { lead: Some("'50".into()), text: format!("car {i}"), swatch: None, divider: false }));
        let mut session = Session { open_combo: Some("zMergedList.Car".into()), ..Session::default() };
        session.choices.insert("zMergedList.Car".into(), Choice { rows: rows.clone(), selected: 3, closed: "356".into(), closed_swatch: None });
        let profiles = Profiles::default();
        let context = Context { open_combo: session.open_combo.clone(), session: Some(&session), ..Context::new(&profiles) };
        let mut screen = Screen::default();
        assert!(append_control_with(&mut screen, &b, &template, &Locale::default(), 0, &context));
        lift_top(&mut screen);
        // List top 65; the header is 13 tall, so item 0 starts at 78.
        assert_eq!(screen.hit_at(160.0, 70.0), Some(Hit::Inert), "a header is not choosable, but the click stops there");
        assert_eq!(screen.hit_at(152.0 + 262.0 - 6.0, 140.0), Some(Hit::Inert), "so does the scroll bar's track");
        assert_eq!(screen.hit_at(160.0, 80.0), Some(Hit::ComboPick("zMergedList.Car".into(), 0)));
        assert_eq!(screen.hit_at(160.0, 94.0), Some(Hit::ComboPick("zMergedList.Car".into(), 1)));
        assert_eq!(screen.hit_at(152.0 + 262.0 - 6.0, 70.0), Some(Hit::ComboScroll("zMergedList.Car".into(), -1)));
        assert_eq!(screen.hit_at(152.0 + 262.0 - 6.0, 65.0 + 155.0), Some(Hit::ComboScroll("zMergedList.Car".into(), 1)));
        assert_eq!(screen.hit_at(160.0, 65.0 + 170.0), None, "nothing below the 160-unit list");
        assert!(screen.draw.iter().any(|d| matches!(d, Draw::Text { text, .. } if text == "356")), "closed value");
        // Scrolled by 3 rows: the header and two items are off the top.
        session.scroll = 3;
        let context = Context { open_combo: session.open_combo.clone(), session: Some(&session), ..Context::new(&profiles) };
        let mut scrolled = Screen::default();
        assert!(append_control_with(&mut scrolled, &b, &template, &Locale::default(), 0, &context));
        lift_top(&mut scrolled);
        assert_eq!(scrolled.hit_at(160.0, 66.0), Some(Hit::ComboPick("zMergedList.Car".into(), 2)));
        assert_eq!(max_scroll(&rows, 160.0), 21 - 11, "eleven 14-unit rows fit in 159");
        assert_eq!(item_row(&rows, 3), 4);
    }

    /// RACE and the location arrows report their callbacks; the arrow keeps its mirror flag.
    #[test]
    fn bottom_bar_callbacks_are_reported() {
        let template = ControlTemplate { id: "NFS5.BOTTOMBARBUTTON".into(), description: String::new(), parts: vec![], states: vec![], blocks: vec![] };
        let race = block(&[("id", "NFS5.BOTTOMBARBUTTON"), ("stringID", "0"), ("callback", "PLAYGAME"), ("posx", "330"), ("posy", "456"), ("shapePathName", "racebutton.fsh")]);
        let mut screen = Screen::default();
        assert!(append_control(&mut screen, &race, &template, &Locale::default(), 0));
        assert_eq!(screen.hit_at(335.0, 460.0), Some(Hit::Callback("PLAYGAME".into())));
        let profiles = Profiles::default();
        let grey = Context { can_play: false, ..Context::new(&profiles) };
        let mut off = Screen::default();
        assert!(append_control_with(&mut off, &race, &template, &Locale::default(), 0, &grey));
        assert_eq!(off.hit_at(335.0, 460.0), None, "RACE is inert while the page cannot start a game");
        let arrow = block(&[("id", "NFS5.BOTTOMBARBUTTON"), ("callback", "TRACK1INC"), ("posx", "500"), ("posy", "265"), ("shapePathName", "SPINArrw.fsh"), ("flip", "1")]);
        let mut screen = Screen::default();
        assert!(append_control(&mut screen, &arrow, &template, &Locale::default(), 0));
        assert_eq!(screen.hit_at(505.0, 270.0), Some(Hit::Callback("TRACK1INC".into())));
        assert!(screen.draw.iter().any(|d| matches!(d, Draw::Anim { flip: 1, .. })));
    }

    /// One of two controls at the same place is hidden by name; watches and the logo read the
    /// session.
    #[test]
    fn session_driven_controls() {
        let empty = |id: &str| ControlTemplate { id: id.into(), description: String::new(), parts: vec![], states: vec![], blocks: vec![] };
        let mut session = Session { hidden: vec!["ID_MERGEDLISTCARSETUP".into()], car_logo: Some(("finallogos2.fsh".into(), 4)), ..Session::default() };
        session.watch.insert("Track1.Track(DIFFICULTY)".into(), "advanced".into());
        let profiles = Profiles::default();
        let context = Context { session: Some(&session), ..Context::new(&profiles) };
        let garage = block(&[("id", "NFS5.GOTOBUTTON"), ("name", "ID_MERGEDLISTCARSETUP"), ("pos", "101,71"), ("width", "162"), ("height", "18"), ("destination", "garage")]);
        let mut screen = Screen::default();
        assert!(append_control_with(&mut screen, &garage, &empty("NFS5.GOTOBUTTON"), &Locale::default(), 0, &context));
        assert!(screen.draw.is_empty(), "hidden by name");
        let watch = block(&[("id", "NFS5.AccessorWatch"), ("pos", "436,429"), ("accessor", "Track1.Track(DIFFICULTY)"), ("fontPathName", "univers8b.ffn"), ("foreColor", "0xffffffff")]);
        assert!(append_control_with(&mut screen, &watch, &empty("NFS5.AccessorWatch"), &Locale::default(), 0, &context));
        assert!(matches!(&screen.draw[0], Draw::Text { text, pos, .. } if text == "advanced" && *pos == [436.0, 429.0]));
        let logo = block(&[("id", "NFS5.CarLogo"), ("pos", "330,426")]);
        assert!(append_control_with(&mut screen, &logo, &empty("NFS5.CarLogo"), &Locale::default(), 0, &context));
        assert_eq!(screen.draw[2], Draw::Picture { file: "finallogos2.fsh".into(), index: 8, pos: [330.0, 426.0], flip: 0, alpha: 1.0 }, "logo 4's left half");
        assert!(matches!(&screen.draw[1], Draw::Picture { index: 9, .. }), "and its right half");
        assert_eq!(accessor_key("Track1.Direction(SHORT)"), "Track1.Direction");
    }

    #[test]
    fn save_values_follow_their_names() {
        let mut save = b"xxGame.Damage\0".to_vec();
        save.extend_from_slice(&1u32.to_le_bytes());
        save.extend_from_slice(b"Game.Difficutly\0");
        save.extend_from_slice(&2u32.to_le_bytes());
        assert_eq!(save_value(&save, "Game.Difficutly"), Some(2));
        assert_eq!(save_value(&save, "Game.Nothing"), None);
    }

    /// At the root screen retail greys "back": grey label, half-opacity icon, no click.
    #[test]
    fn back_is_grey_at_the_root_screen() {
        let template = ControlTemplate { id: "NFS5.BOTTOMBARBUTTON".into(), description: String::new(), parts: vec![], states: vec![], blocks: vec![] };
        let locale = Locale::parse_csv("String ID,ENGLISH_IDS\n10501,back\n");
        let b = block(&[("id", "NFS5.BOTTOMBARBUTTON"), ("stringID", "10501"), ("callback", "GOBACK"), ("space", "4"), ("posx", "5"), ("posy", "452"), ("shapePathName", "backbutton.fsh")]);
        let profiles = Profiles::default();
        let context = Context { profiles: &profiles, game_dir: None, disabled: false, can_go_back: false, can_play: true, difficulty: 1, open_combo: None, combo_reveal: 1.0, session: None };
        let mut screen = Screen::default();
        assert!(append_control_with(&mut screen, &b, &template, &locale, 0, &context));
        assert_eq!(screen.hit_at(10.0, 460.0), None);
        assert!(screen.draw.iter().any(|d| matches!(d, Draw::Text { colour, .. } if colour[0] == 128.0 / 255.0)));
        assert!(screen.draw.iter().any(|d| matches!(d, Draw::Picture { alpha, .. } if *alpha == 0.5)));
    }

    /// `bottombar.lay`'s exit button: right-aligned at 635, quits.
    #[test]
    fn a_right_aligned_bottom_bar_button_ends_at_its_position() {
        let template = ControlTemplate { id: "NFS5.BOTTOMBARBUTTON".into(), description: String::new(), parts: vec![], states: vec![], blocks: vec![] };
        let locale = Locale::parse_csv("String ID,ENGLISH_IDS\n10500,exit\n");
        let b = block(&[("id", "NFS5.BOTTOMBARBUTTON"), ("stringID", "10500"), ("callback", "QUIT"), ("space", "4"), ("picoffy", "6"), ("startframe", "32"), ("align", "1"), ("posx", "635"), ("posy", "452"), ("shapePathName", "backbutton.fsh")]);
        let mut screen = Screen::default();
        assert!(append_control(&mut screen, &b, &template, &locale, 0));
        // Without a game folder the icon is assumed 20 wide; "exit" at 17 points is 4 x 6.5 x 17/11.
        let text_w = 4.0 * 6.5 * 17.0 / 11.0;
        let left = 635.0 - (20.0 + 4.0 + text_w);
        assert_eq!(screen.hit_at(634.0, 460.0), Some(Hit::Quit));
        assert!(screen.draw.iter().any(|d| matches!(d, Draw::Anim { file, idle, pos, .. } if file == "backbutton.fsh" && idle.start == 32 && *pos == [left, 458.0])));
    }

    /// `bottombar.lay`'s exit button: idle frame 32, hover 32..51 over 128 ticks, mouse-down 52;
    /// the label turns yellow under the pointer.
    #[test]
    fn bottom_bar_icons_animate_and_labels_light_up() {
        let template = ControlTemplate { id: "NFS5.BOTTOMBARBUTTON".into(), description: String::new(), parts: vec![], states: vec![], blocks: vec![] };
        let locale = Locale::parse_csv("String ID,ENGLISH_IDS\n10500,exit\n");
        let b = block(&[
            ("id", "NFS5.BOTTOMBARBUTTON"), ("stringID", "10500"), ("callback", "QUIT"), ("posx", "635"), ("posy", "452"), ("align", "1"), ("shapePathName", "backbutton.fsh"),
            ("seperatehovermousedown", "1"), ("startframe", "32"), ("endframe", "32"), ("totalticks", "0"), ("hoverstartframe", "32"), ("hoverendframe", "51"), ("hovertotalticks", "128"),
            ("hovermousedownstartframe", "52"), ("hovermousedownendframe", "52"), ("hovermousedowntotalticks", "0"),
        ]);
        let mut screen = Screen::default();
        assert!(append_control(&mut screen, &b, &template, &locale, 0));
        let anim = screen.draw.iter().find_map(|d| if let Draw::Anim { idle, hover, down, .. } = d { Some((*idle, *hover, *down)) } else { None }).expect("an animated icon");
        assert_eq!(anim, (Frames { start: 32, end: 32, ticks: 0 }, Frames { start: 32, end: 51, ticks: 128 }, Frames { start: 52, end: 52, ticks: 0 }));
        let labels: Vec<Show> = screen.draw.iter().filter_map(|d| if let Draw::Shown { when, draw } = d { matches!(**draw, Draw::Text { .. }).then_some(*when) } else { None }).collect();
        assert!(matches!(labels[..], [Show::Hover(_), Show::Idle(_)]), "yellow label under the pointer, white otherwise");
    }

    /// `transon=` / `transoff=` words, and `SAME` taking the other side's.
    #[test]
    fn transition_words_follow_retail() {
        assert_eq!(transitions("LEFT", "SAME"), (TRANS_LEFT, TRANS_LEFT));
        assert_eq!(transitions("SAME", "FADE"), (TRANS_FADE, TRANS_FADE));
        assert_eq!(transitions("SAME", "SAME"), (0, 0));
        assert_eq!(transitions("RIGHT", "top"), (TRANS_RIGHT, TRANS_TOP));
        assert_eq!(transitions("", "nonsense"), (0, 0));
        assert_eq!(trans_offset(TRANS_LEFT, 0.5), ([-320.0, 0.0], 1.0));
        assert_eq!(trans_offset(TRANS_BOTTOM | TRANS_FADE, 0.25), ([0.0, 120.0], 0.75));
    }

    /// `selectname.lay`'s list outline: 152x102, black, 1 unit.
    #[test]
    fn a_glinty_rectangle_is_a_one_unit_outline() {
        let template = ControlTemplate { id: "NFS5.GlintyRectangle".into(), description: String::new(), parts: vec![], states: vec![], blocks: vec![] };
        let b = block(&[("id", "NFS5.GlintyRectangle"), ("pos", "283,218"), ("col", "0xff000000"), ("width", "152"), ("height", "102")]);
        let mut screen = Screen::default();
        assert!(append_control(&mut screen, &b, &template, &Locale::default(), 0));
        let rects: Vec<Rect> = screen.quad_rects();
        assert_eq!(rects, vec![[283.0, 218.0, 152.0, 1.0], [283.0, 319.0, 152.0, 1.0], [283.0, 218.0, 1.0, 102.0], [434.0, 218.0, 1.0, 102.0]]);
    }

    #[test]
    fn a_save_name_carries_the_face_digit() {
        assert_eq!(display_name("0Greg"), "Greg");
        assert_eq!(face_index("0Greg"), 0);
        assert_eq!(face_index("5Ana"), 5);
        assert_eq!(face_index("8Driver"), 8);
        assert_eq!(face_index("9x"), 8, "above 8 clamps to the helmet");
        assert_eq!(face_index("Greg"), 8, "a letter is above '8'");
        assert_eq!(face_index("!x"), 0, "below '0' clamps to 0");
        assert_eq!(display_name(""), "");
    }

    /// `selectname.lay`'s list box and portrait with two saved players.
    #[test]
    fn the_user_list_draws_rows_and_the_selected_face() {
        let empty = |id: &str| ControlTemplate { id: id.into(), description: String::new(), parts: vec![], states: vec![], blocks: vec![] };
        let profiles = Profiles { names: vec!["8Driver".into(), "4Greg".into()], selected: Some(1) };
        let list = block(&[("id", "NFS5.ListBox"), ("pos", "283,218"), ("accessor", "UserList.UserList"), ("width", "152"), ("selectionboxmaxheight", "100")]);
        let mut screen = Screen::default();
        assert!(append_control_with(&mut screen, &list, &empty("NFS5.ListBox"), &Locale::default(), 0, &Context::new(&profiles)));
        assert_eq!(screen.hit_at(300.0, 222.0), Some(Hit::Select(0)));
        assert_eq!(screen.hit_at(300.0, 236.0), Some(Hit::Select(1)));
        assert!(screen.draw.iter().any(|d| matches!(d, Draw::Quad { rect, .. } if rect[1] == 233.0)), "selected row is filled");
        assert!(screen.draw.iter().any(|d| matches!(d, Draw::Text { text, .. } if text == "Greg")), "the face digit is not shown");

        let face = block(&[("id", "NFS5.PLAYERPICTURE"), ("pos", "216,236")]);
        assert!(append_control_with(&mut screen, &face, &empty("NFS5.PLAYERPICTURE"), &Locale::default(), 0, &Context::new(&profiles)));
        assert!(screen.draw.contains(&Draw::Picture { file: "ALLdrivr.fsh".into(), index: 4, pos: [216.0, 236.0], flip: 0, alpha: 1.0 }));

        let mut none = Screen::default();
        assert!(append_control_with(&mut none, &face, &empty("NFS5.PLAYERPICTURE"), &Locale::default(), 0, &Context::new(&Profiles::default())));
        assert!(none.draw.is_empty(), "no players, no face");
    }

    #[test]
    fn an_unknown_control_type_is_reported_not_drawn() {
        let template = ControlTemplate {
            id: "NFS5.NoSuchControl".into(),
            description: "not a real control".into(),
            parts: vec![],
            states: vec![],
            blocks: vec![],
        };
        let mut screen = Screen::default();
        assert!(!append_control(&mut screen, &button_block(), &template, &Locale::default(), 0));
        assert!(screen.draw.is_empty());
    }

    #[test]
    fn a_level_bar_fills_to_its_level_and_is_a_slider() {
        let template = ControlTemplate { id: "NFS5.GouraudIndicatorBar".into(), description: String::new(), parts: vec![], states: vec![], blocks: vec![] };
        let b = block(&[("id", "NFS5.GouraudIndicatorBar"), ("pos", "307,150"), ("size", "198,9"), ("accessor", "Audio.MusicLevel"), ("changeable", "1"), ("c0", "0xffe60000"), ("c50", "0xffffd76c"), ("c100", "0xfffdb306")]);
        let profiles = Profiles::default();
        let mut session = Session::default();
        session.levels.insert("Audio.MusicLevel".into(), 32);
        let mut context = Context::new(&profiles);
        context.session = Some(&session);
        let mut screen = Screen::default();
        assert!(append_control_with(&mut screen, &b, &template, &Locale::default(), 0, &context));
        assert_eq!(screen.hit_at(400.0, 154.0), Some(Hit::Slider("Audio.MusicLevel".into(), 307, 198)));
        // A quarter full: one gradient piece, ending a quarter along.
        let ends: Vec<f32> = screen.draw.iter().filter_map(|d| if let Draw::Gradient { corners, .. } = d { Some(corners[1][0]) } else { None }).collect();
        assert_eq!(ends.len(), 1);
        assert!((ends[0] - (307.0 + 198.0 * 32.0 / 127.0)).abs() < 1e-3);
    }
}
