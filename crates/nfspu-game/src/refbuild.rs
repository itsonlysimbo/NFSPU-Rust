//! The retail reference build this rewrite compares against.
//!
//! The author's reference copy is the community "Enhanced 3.5" build with the 2019 Combined
//! Patch (v1.0.6) on top. That patch adds a thin runtime layer over the 2000 engine: it reads its
//! own settings files, picks a renderer module out of `drivers/`, widens the field of view on
//! widescreen modes, gates a couple of renderer states on two graphics options, and lifts the
//! CPU-detail class. A rewrite that wants retail numbers has to reproduce exactly that layer, so
//! this module is the model of it.
//!
//! Evidence is in `docs/provenance/refbuild.md`: the patch's own settings files, the constants it
//! uses, the values it computes, and a read-only sample of a live patched session that confirmed
//! them. Everything here is window-free; the engine asks this module for numbers.
//!
//! Names and structure are ours; the numbers are the reference build's.

/// Aspect ratio above which the reference build uses its widescreen view model.
///
/// The build compares against exactly 4:3, and equal is *not* widescreen: a 4:3 client stays on
/// the narrow model. Verified live (a 640x480 race kept the narrow values).
pub const WIDESCREEN_ASPECT: f32 = 4.0 / 3.0;

/// Field of view, in degrees, of the narrow (4:3 and taller) view model.
///
/// This is the same value the original engine already used for the camera, so on a 4:3 client the
/// patch changes nothing. Confirmed live at 640x480.
pub const NARROW_FOV_DEGREES: f32 = 37.5;

/// Virtual width of the narrow view model.
pub const NARROW_VIRTUAL_WIDTH: f32 = 640.0;

/// Virtual height of the widescreen view model.
pub const WIDESCREEN_VIRTUAL_HEIGHT: f32 = 480.0;

/// Slope of the widescreen field of view: `fov = atan(SLOPE * aspect)`, in degrees.
///
/// The build derives this as `480 * 3 / 4`, i.e. the widescreen virtual height times 3/4. For a
/// 16:9 client that gives about 89.9105 degrees.
pub const WIDESCREEN_FOV_SLOPE: f32 = 480.0 * 3.0 / 4.0;

/// The interface language the reference build selects by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    French,
    German,
    Italian,
    Spanish,
    Swedish,
    BrazilianPortuguese,
}

impl Language {
    /// Every language the reference build knows, in its own order.
    pub const ALL: [Language; 7] = [
        Language::English,
        Language::French,
        Language::German,
        Language::Italian,
        Language::Spanish,
        Language::Swedish,
        Language::BrazilianPortuguese,
    ];

    /// The numeric id the engine uses (0-based, in [`Language::ALL`] order).
    pub fn id(self) -> u8 {
        match self {
            Language::English => 0,
            Language::French => 1,
            Language::German => 2,
            Language::Italian => 3,
            Language::Spanish => 4,
            Language::Swedish => 5,
            Language::BrazilianPortuguese => 6,
        }
    }

    /// The token the settings file uses for this language.
    pub fn config_name(self) -> &'static str {
        match self {
            Language::English => "ENGLISH",
            Language::French => "FRENCH",
            Language::German => "GERMAN",
            Language::Italian => "ITALIAN",
            Language::Spanish => "SPANISH",
            Language::Swedish => "SWEDISH",
            Language::BrazilianPortuguese => "PORTBRZL",
        }
    }

    /// Reads a language token, ignoring case. Unknown tokens yield `None`.
    pub fn from_config_name(name: &str) -> Option<Language> {
        Language::ALL.into_iter().find(|l| l.config_name().eq_ignore_ascii_case(name.trim()))
    }
}

/// The renderer back ends the reference build can select.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RendererBackend {
    Direct3d,
    Glide,
    OpenGl,
    Software,
}

impl RendererBackend {
    /// The `Type=` token in a driver profile.
    pub fn config_token(self) -> &'static str {
        match self {
            RendererBackend::Direct3d => "d3d",
            RendererBackend::Glide => "voodoo",
            RendererBackend::OpenGl => "opengl",
            RendererBackend::Software => "software",
        }
    }

    /// Reads a `Type=` token, ignoring case.
    pub fn from_config_token(token: &str) -> Option<RendererBackend> {
        [RendererBackend::Direct3d, RendererBackend::Glide, RendererBackend::OpenGl, RendererBackend::Software]
            .into_iter()
            .find(|b| b.config_token().eq_ignore_ascii_case(token.trim()))
    }

    /// A short human name, for logs.
    pub fn label(self) -> &'static str {
        match self {
            RendererBackend::Direct3d => "Direct3D",
            RendererBackend::Glide => "Glide",
            RendererBackend::OpenGl => "OpenGL",
            RendererBackend::Software => "software",
        }
    }
}

/// The settings block the reference build reads at start-up.
///
/// Field names are ours; the keys and defaults are the reference build's, so a file written by
/// its own configuration tool loads here unchanged.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub language: Language,
    pub movies: bool,
    pub driver: String,
    pub windowed: bool,
    pub single_processor_affinity: bool,
    /// A fake CPU clock in MHz the build reports to the engine; 0 means "measure it".
    pub cpu_mhz_limit: u32,
    pub error_reporting: bool,
    pub fog: bool,
    pub filtering: bool,
    pub car_control_after_finish: bool,
}

impl Default for Settings {
    /// The reference build's built-in defaults, used when its settings file is absent.
    fn default() -> Self {
        Self {
            language: Language::English,
            movies: true,
            driver: "dx7z".to_string(),
            windowed: false,
            single_processor_affinity: false,
            cpu_mhz_limit: 0,
            error_reporting: true,
            fog: true,
            filtering: true,
            car_control_after_finish: false,
        }
    }
}

impl Settings {
    /// Reads the settings block. Missing keys keep their default, so a partial file works.
    pub fn parse(text: &str) -> Self {
        let mut s = Settings::default();
        let block = Sections::parse(text);
        if let Some(name) = block.get("NFS5", "Language").and_then(|v| Language::from_config_name(v)) {
            s.language = name;
        }
        s.movies = block.flag("NFS5", "NoMovies").map_or(s.movies, |set| !set);
        if let Some(d) = block.get("NFS5", "ThrashDriver") {
            if !d.trim().is_empty() {
                s.driver = d.trim().to_string();
            }
        }
        s.windowed = block.flag("NFS5", "WindowedMode").unwrap_or(s.windowed);
        s.single_processor_affinity = block.flag("NFS5", "SingleProcAffinity").unwrap_or(s.single_processor_affinity);
        if let Some(n) = block.get("NFS5", "CpuLimit").and_then(|v| v.trim().parse().ok()) {
            s.cpu_mhz_limit = n;
        }
        s.error_reporting = block.flag("NFS5", "NoErrorReporting").map_or(s.error_reporting, |set| !set);
        s.fog = block.flag("NFS5", "Fog").unwrap_or(s.fog);
        s.filtering = block.flag("NFS5", "Filtering").unwrap_or(s.filtering);
        s.car_control_after_finish = block.flag("NFS5", "CarControl").unwrap_or(s.car_control_after_finish);
        s
    }
}

/// One renderer module as described by its own profile file (`drivers/<name>/thrash.ini`).
#[derive(Debug, Clone, PartialEq)]
pub struct DriverProfile {
    /// The module file to load (for example `opengl3z.dll`).
    pub module: String,
    pub backend: RendererBackend,
    /// Pass-through option pairs the build hands to the module, in file order.
    pub options: Vec<(String, String)>,
}

impl DriverProfile {
    /// Reads a profile. Returns `None` when the `[THRASH]` block is missing.
    pub fn parse(text: &str) -> Option<DriverProfile> {
        let s = Sections::parse(text);
        let module = s.get("THRASH", "File")?.trim().to_string();
        let backend = RendererBackend::from_config_token(s.get("THRASH", "Type")?)?;
        let options = s.pairs("ENV");
        Some(DriverProfile { module, backend, options })
    }

    /// Looks up one pass-through option.
    pub fn option(&self, key: &str) -> Option<&str> {
        self.options.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str())
    }
}

/// The virtual framebuffer and camera field of view the reference build uses for a client size.
///
/// On a 4:3-or-taller client it keeps a fixed 640-wide virtual screen and the engine's own
/// 37.5-degree field of view. Wider clients switch to a fixed 480-high virtual screen whose width
/// grows with the aspect ratio, and a field of view that widens with it. Both a 640x480 and a
/// 1920x1080 client were confirmed against a live session.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VirtualScreen {
    pub width: f32,
    pub height: f32,
    pub fov_degrees: f32,
    pub widescreen: bool,
}

impl VirtualScreen {
    /// Derives the virtual screen for a client (window or output) size in pixels.
    pub fn for_client(client_width: u32, client_height: u32) -> VirtualScreen {
        let aspect = client_width.max(1) as f32 / client_height.max(1) as f32;
        if aspect > WIDESCREEN_ASPECT {
            VirtualScreen {
                width: WIDESCREEN_VIRTUAL_HEIGHT * aspect,
                height: WIDESCREEN_VIRTUAL_HEIGHT,
                fov_degrees: (WIDESCREEN_FOV_SLOPE * aspect).atan().to_degrees(),
                widescreen: true,
            }
        } else {
            VirtualScreen {
                width: NARROW_VIRTUAL_WIDTH,
                height: NARROW_VIRTUAL_WIDTH / aspect,
                fov_degrees: NARROW_FOV_DEGREES,
                widescreen: false,
            }
        }
    }

    /// Aspect ratio of the virtual screen itself.
    pub fn aspect(&self) -> f32 {
        self.width / self.height
    }
}

/// Gates a few renderer state calls on the Fog and Filtering settings.
///
/// The reference build wraps the renderer's state function: a small set of state codes is either
/// skipped or has its value forced to zero when the matching option is off, and everything else
/// passes straight through. The code sets were read from the build's own dispatch table and
/// confirmed to be installed in a live session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RendererStateGate {
    pub fog: bool,
    pub filtering: bool,
}

impl RendererStateGate {
    /// State codes controlled by the Filtering option.
    pub const FILTER_STATES: [u32; 2] = [0x07, 0x49];
    /// State codes skipped entirely when Fog is off.
    pub const FOG_SKIPPED_STATES: [u32; 4] = [0x0E, 0x0F, 0x16, 0x17];
    /// State codes whose value is forced to zero when Fog is off.
    pub const FOG_ZEROED_STATES: [u32; 3] = [0x14, 0x15, 0x69];

    /// Applies the gate. `Some(value)` means "call the renderer with this value"; `None` means
    /// "do not call it at all".
    pub fn gate(self, state: u32, value: i32) -> Option<i32> {
        if Self::FILTER_STATES.contains(&state) && !self.filtering {
            return Some(0);
        }
        if Self::FOG_SKIPPED_STATES.contains(&state) && !self.fog {
            return None;
        }
        if Self::FOG_ZEROED_STATES.contains(&state) && !self.fog {
            return Some(0);
        }
        Some(value)
    }
}

/// The CPU-detail class the reference build leaves the engine with.
///
/// The build replaces the engine's clock measurement and then clamps the resulting class: a class
/// above 2 is pulled down to 2 while a secondary detection value stays below 32. On the sampled
/// machine the engine reported class 2 with that value at 16, so the clamp was a no-op and detail
/// stayed at the high setting. Classes are 0 (slowest) to 3.
pub fn cpu_detail_class(measured: u8, secondary: u32) -> u8 {
    if measured > 2 && secondary < 32 {
        2
    } else {
        measured.min(3)
    }
}

/// The data folders the reference build resolves relative to its own folder, in its own order.
///
/// This is what makes a patched install portable: the engine is handed these relative paths
/// instead of reading an install location from the registry. Kept here so a portable install can
/// be located the same way.
pub const PORTABLE_DATA_DIRS: [&str; 25] = [
    ".\\gamedata\\simulation\\ai\\",
    ".\\gamedata\\simulation\\cardata\\",
    ".\\gamedata\\simulation\\aicardata\\",
    ".\\gamedata\\track\\",
    ".\\gamedata\\track\\racer\\work\\",
    ".\\gamedata\\track\\cops\\",
    ".\\gamedata\\carmodel\\",
    ".\\drivers\\",
    ".\\gamedata\\render\\",
    ".\\gamedata\\dashhud\\",
    ".\\gamedata\\music\\",
    ".\\gamedata\\sounds\\",
    ".\\gamedata\\speech\\",
    ".\\fedata\\movies\\",
    ".\\fedata\\controls\\",
    ".\\fedata\\art\\",
    ".\\fedata\\showcase\\",
    ".\\fedata\\chron\\",
    ".\\fedata\\locale\\",
    ".\\fedata\\layouts\\",
    ".\\fedata\\data\\",
    ".\\fedata\\models\\",
    ".\\fedata\\trackart\\",
    ".\\fedata\\factory\\",
    ".\\savedata\\",
];

/// A very small `[section] key = value` reader, enough for the reference build's files.
struct Sections {
    entries: Vec<(String, String, String)>,
}

impl Sections {
    fn parse(text: &str) -> Sections {
        let mut entries = Vec::new();
        let mut section = String::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = name.trim().to_string();
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                entries.push((section.clone(), key.trim().to_string(), value.trim().to_string()));
            }
        }
        Sections { entries }
    }

    fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(s, k, _)| s.eq_ignore_ascii_case(section) && k.eq_ignore_ascii_case(key))
            .map(|(_, _, v)| v.as_str())
    }

    /// Reads a 0/1 key as a flag; anything else is treated as unset.
    fn flag(&self, section: &str, key: &str) -> Option<bool> {
        match self.get(section, key) {
            Some("0") => Some(false),
            Some("1") => Some(true),
            _ => None,
        }
    }

    /// Every key/value pair of a section, in file order.
    fn pairs(&self, section: &str) -> Vec<(String, String)> {
        self.entries
            .iter()
            .filter(|(s, _, _)| s.eq_ignore_ascii_case(section))
            .map(|(_, k, v)| (k.clone(), v.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn narrow_client_keeps_the_engine_view() {
        for (w, h) in [(640, 480), (800, 600), (1280, 960), (1600, 1200)] {
            let v = VirtualScreen::for_client(w, h);
            assert!(!v.widescreen, "{w}x{h} should stay narrow");
            assert!(close(v.width, 640.0) && close(v.height, 480.0), "{w}x{h} -> {v:?}");
            assert!(close(v.fov_degrees, 37.5));
        }
    }

    #[test]
    fn exactly_four_thirds_is_not_widescreen() {
        // 1280x960 is exactly 4:3; the build tests `aspect > 4/3`.
        assert!(!VirtualScreen::for_client(1280, 960).widescreen);
        // 1280x959 is a hair wider.
        assert!(VirtualScreen::for_client(1280, 959).widescreen);
    }

    #[test]
    fn widescreen_grows_width_and_fov() {
        let v = VirtualScreen::for_client(1920, 1080);
        assert!(v.widescreen);
        assert!(close(v.height, 480.0));
        assert!(close(v.width, 853.3333), "{v:?}");
        assert!(close(v.fov_degrees, 89.9105), "{}", v.fov_degrees);

        let ten16 = VirtualScreen::for_client(1920, 1200);
        assert!(close(ten16.width, 768.0) && close(ten16.fov_degrees, 89.9005), "{ten16:?}");

        let ultrawide = VirtualScreen::for_client(2560, 1080);
        assert!(close(ultrawide.width, 1137.7778) && close(ultrawide.fov_degrees, 89.9329), "{ultrawide:?}");
    }

    #[test]
    fn virtual_aspect_matches_the_client() {
        for (w, h) in [(640, 480), (1920, 1080), (2560, 1080)] {
            let v = VirtualScreen::for_client(w, h);
            assert!(close(v.aspect(), w as f32 / h as f32), "{w}x{h} -> {v:?}");
        }
    }

    #[test]
    fn settings_defaults_match_the_reference_build() {
        let s = Settings::default();
        assert_eq!(s.language, Language::English);
        assert!(s.movies && s.fog && s.filtering && s.error_reporting);
        assert!(!s.windowed && !s.single_processor_affinity && !s.car_control_after_finish);
        assert_eq!(s.cpu_mhz_limit, 0);
        assert_eq!(s.driver, "dx7z");
    }

    #[test]
    fn settings_parse_a_real_looking_file() {
        let text = "\
[NFS5]
Language=english
ThrashDriver=opengl3
WindowedMode=1
SingleProcAffinity=0
CpuLimit=0
NoErrorReporting=0
NoMovies=1
Fog=1
Filtering=1
CarControl=0
";
        let s = Settings::parse(text);
        assert_eq!(s.language, Language::English);
        assert_eq!(s.driver, "opengl3");
        assert!(s.windowed);
        assert!(!s.movies, "NoMovies=1 means movies are off");
        assert!(s.fog && s.filtering, "Fog=1 and Filtering=1 are both on");
        assert!(s.error_reporting, "NoErrorReporting=0 leaves error reporting on");
    }

    #[test]
    fn settings_tolerate_a_partial_file() {
        let s = Settings::parse("[NFS5]\nFog=0\n");
        assert!(!s.fog);
        assert!(s.filtering, "unset keys keep their default");
        assert_eq!(s.language, Language::English);
    }

    #[test]
    fn driver_profile_parses_type_and_options() {
        let text = "\
[THRASH]
File=opengl3z.dll
Type=opengl

[ENV]
OGL_RESOLUTION=1
OGL_COLORDEPTH=32
OGL_GAMMA=5
";
        let p = DriverProfile::parse(text).unwrap();
        assert_eq!(p.module, "opengl3z.dll");
        assert_eq!(p.backend, RendererBackend::OpenGl);
        assert_eq!(p.option("OGL_GAMMA"), Some("5"));
        assert_eq!(p.option("ogl_colordepth"), Some("32"), "keys are case-insensitive");
        assert_eq!(p.option("NGLIDE_GAMMA"), None);
    }

    #[test]
    fn driver_profile_rejects_an_unknown_backend() {
        assert!(DriverProfile::parse("[THRASH]\nFile=x.dll\nType=metallic\n").is_none());
        assert!(DriverProfile::parse("[THRASH]\nFile=x.dll\n").is_none());
    }

    #[test]
    fn gate_passes_uncontrolled_states_through() {
        let g = RendererStateGate { fog: false, filtering: false };
        assert_eq!(g.gate(0x20, 1234), Some(1234));
    }

    #[test]
    fn gate_applies_filtering_and_fog_rules() {
        let off = RendererStateGate { fog: false, filtering: false };
        for state in RendererStateGate::FILTER_STATES {
            assert_eq!(off.gate(state, 9), Some(0), "filtering off zeroes {state:#x}");
        }
        for state in RendererStateGate::FOG_SKIPPED_STATES {
            assert_eq!(off.gate(state, 9), None, "fog off skips {state:#x}");
        }
        for state in RendererStateGate::FOG_ZEROED_STATES {
            assert_eq!(off.gate(state, 9), Some(0), "fog off zeroes {state:#x}");
        }

        let on = RendererStateGate { fog: true, filtering: true };
        for state in RendererStateGate::FILTER_STATES
            .iter()
            .chain(RendererStateGate::FOG_SKIPPED_STATES.iter())
            .chain(RendererStateGate::FOG_ZEROED_STATES.iter())
        {
            assert_eq!(on.gate(*state, 9), Some(9), "with options on, {state:#x} passes through");
        }
    }

    #[test]
    fn cpu_class_clamps_the_top_class() {
        assert_eq!(cpu_detail_class(3, 16), 2, "class 3 with a low secondary value is pulled to 2");
        assert_eq!(cpu_detail_class(2, 16), 2);
        assert_eq!(cpu_detail_class(3, 32), 3, "at the threshold the clamp no longer applies");
        assert_eq!(cpu_detail_class(1, 0), 1);
        assert_eq!(cpu_detail_class(9, 0), 2, "an out-of-range class is still clamped");
        assert_eq!(cpu_detail_class(9, 32), 3, "classes stop at 3");
    }

    #[test]
    fn language_ids_and_names_round_trip() {
        assert_eq!(Language::English.id(), 0);
        assert_eq!(Language::BrazilianPortuguese.id(), 6);
        for l in Language::ALL {
            assert_eq!(Language::from_config_name(l.config_name()), Some(l));
        }
        assert_eq!(Language::from_config_name(" french "), Some(Language::French));
        assert_eq!(Language::from_config_name("klingon"), None);
    }
}
