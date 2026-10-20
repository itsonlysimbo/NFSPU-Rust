//! A track's conditions: day or night, rain, fog and lightning, as retail decides them, plus the
//! `NFSPU_WEATHER` / `NFSPU_TIME` overrides for testing. No Bevy here; `weather_fx.rs` draws it.
//!
//! How retail decides (evidence in `docs/provenance/weather.md`):
//! * **Night** is not a menu option. A track is lit for night when its `.crp` uses headlight
//!   vertex-light layers: industrial and monaco1-5. Everything else is day (or dusk, by its fog).
//! * **Wet road**: the race start sets the wet flag only when the track is `autobahn`. While it is
//!   set, a car that is not under cover uses the wet grip table and wet-asphalt tyre sounds.
//! * **Rain, snow, lightning and fog** come from the keyframes in `Sky/<track>.bin`, blended by the
//!   camera's road slice: autobahn rains (not under its bridges), alps snows near the village,
//!   industrial has a lightning storm.

use nfspu_formats::weather::{Conditions, LightMap, SkyWeather, TrackLighting};
use std::path::Path;

/// What the testing overrides can force.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeatherOverride {
    /// Use the track's own keyframes.
    Track,
    /// No rain, snow or lightning, dry road.
    Dry,
    /// Rain at full strength and a wet road everywhere.
    Rain,
    /// Snow at full strength (dry grip; retail has no snow grip change).
    Snow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeOverride {
    Track,
    Day,
    Night,
}

impl WeatherOverride {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "dry" | "clear" | "off" => Self::Dry,
            "rain" | "wet" => Self::Rain,
            "snow" => Self::Snow,
            _ => Self::Track,
        }
    }
    pub fn from_env() -> Self {
        std::env::var("NFSPU_WEATHER").map(|s| Self::parse(&s)).unwrap_or(Self::Track)
    }
}

impl TimeOverride {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "day" => Self::Day,
            "night" => Self::Night,
            _ => Self::Track,
        }
    }
    pub fn from_env() -> Self {
        std::env::var("NFSPU_TIME").map(|s| Self::parse(&s)).unwrap_or(Self::Track)
    }
}

/// Everything one track's weather and lighting needs, loaded once per race.
#[derive(Debug, Clone, Default)]
pub struct TrackWeather {
    pub track: String,
    /// Night lighting (headlights light the road, night gauges).
    pub night: bool,
    /// Night as the track itself asks for it, before any override.
    pub track_night: bool,
    /// Wet road (retail: autobahn only).
    pub wet: bool,
    /// The sky `.bin` keyframes; `None` when the track has no version-9 file (retail then has no
    /// fog and no precipitation).
    pub sky: Option<SkyWeather>,
    pub lighting: TrackLighting,
    /// Headlight light map from `Render/night.psh`.
    pub light_map: Option<LightMap>,
    pub weather: Option<WeatherOverride>,
}

impl TrackWeather {
    /// Reads `<game_dir>/GameData/Track/Sky/<track>.bin`, `<track>.map`, `<track>.crp` (for the night
    /// layers) and `Render/night.psh`. Missing files leave retail's defaults.
    pub fn load(game_dir: &Path, track: &str, weather: WeatherOverride, time: TimeOverride) -> Self {
        let tdir = game_dir.join("GameData").join("Track");
        let sky = find_case_insensitive(&tdir.join("Sky"), &format!("{track}.bin")).and_then(|p| std::fs::read(p).ok()).and_then(|b| SkyWeather::parse(&b));
        let lighting = std::fs::read_to_string(tdir.join(format!("{track}.map"))).map(|t| TrackLighting::parse(&t)).unwrap_or_default();
        let track_night = std::fs::read(tdir.join(format!("{track}.crp")))
            .ok()
            .and_then(|raw| if nfspu_formats::refpack::is_refpack(&raw) { nfspu_formats::refpack::decompress(&raw).ok() } else { Some(raw) })
            .map(|data| nfspu_formats::crp::parse(&data).map(|c| nfspu_formats::weather::crp_wants_night(&c)).unwrap_or(false))
            .unwrap_or(false);
        let light_map = std::fs::read(game_dir.join("GameData").join("Render").join("night.psh")).ok().and_then(|b| LightMap::parse_psh(&b));
        Self::from_parts(track, sky, lighting, track_night, light_map, weather, time)
    }

    pub fn from_parts(track: &str, sky: Option<SkyWeather>, lighting: TrackLighting, track_night: bool, light_map: Option<LightMap>, weather: WeatherOverride, time: TimeOverride) -> Self {
        let night = match time {
            TimeOverride::Track => track_night,
            TimeOverride::Day => false,
            TimeOverride::Night => true,
        };
        let wet = match weather {
            WeatherOverride::Track => track.eq_ignore_ascii_case("autobahn"),
            WeatherOverride::Rain => true,
            WeatherOverride::Dry | WeatherOverride::Snow => false,
        };
        Self {
            track: track.to_string(),
            night,
            track_night,
            wet,
            sky,
            lighting,
            light_map,
            weather: (weather != WeatherOverride::Track).then_some(weather),
        }
    }

    /// Conditions at a road slice, with the override applied.
    pub fn at(&self, slice: usize, slice_count: usize) -> Conditions {
        let mut c = self.sky.as_ref().map(|s| s.sample(slice as i32, slice_count as i32)).unwrap_or_default();
        match self.weather {
            Some(WeatherOverride::Dry) => {
                c.rain = 0.0;
                c.snow = 0.0;
                c.lightning = 0.0;
            }
            Some(WeatherOverride::Rain) => {
                c.rain = 1.0;
                c.snow = 0.0;
            }
            Some(WeatherOverride::Snow) => {
                c.rain = 0.0;
                c.snow = 1.0;
            }
            _ => {}
        }
        c
    }

    /// One line for the log.
    pub fn describe(&self) -> String {
        let (rain, lightning, snow) = self.sky.as_ref().map(|s| s.peaks()).unwrap_or_default();
        format!(
            "weather '{}': {} (track says {}), road {}, keyframes {}, peak rain {:.2} lightning {:.2} snow {:.2}, override {:?}, car ambient {:.3}, light map {}",
            self.track,
            if self.night { "night" } else { "day" },
            if self.track_night { "night" } else { "day" },
            if self.wet { "wet" } else { "dry" },
            self.sky.as_ref().map_or(0, |s| s.keys.len()),
            rain,
            lightning,
            snow,
            self.weather,
            self.lighting.ambient[1],
            if self.light_map.is_some() { "loaded" } else { "missing" },
        )
    }
}

fn find_case_insensitive(dir: &Path, name: &str) -> Option<std::path::PathBuf> {
    let direct = dir.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    std::fs::read_dir(dir).ok()?.filter_map(|e| e.ok()).find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name)).map(|e| e.path())
}

/// Retail's tyre grip factors by surface row (0..15) and tyre class (0 road, 1, 2 race), dry
/// (exe 0x5D0D50) and wet (0x5D0E4C). Row 1 is asphalt. The wet table is the dry one with the first
/// two tyre columns swapped.
pub const DRY_GRIP: [[f32; 3]; 16] = [
    [0.5, 0.5, 0.5],
    [1.0, 0.9, 1.1],
    [0.85, 0.9, 0.85],
    [0.7, 0.775, 0.625],
    [0.95, 0.9, 1.05],
    [0.85, 0.875, 0.8],
    [0.7, 0.8, 0.6],
    [0.9, 0.9, 1.0],
    [0.6, 0.75, 0.5],
    [0.675, 0.75, 0.6],
    [0.85, 0.8, 0.95],
    [0.9, 0.85, 0.9],
    [1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
];
pub const WET_GRIP: [[f32; 3]; 16] = [
    [0.5, 0.5, 0.5],
    [0.9, 1.0, 1.1],
    [0.9, 0.85, 0.85],
    [0.775, 0.7, 0.625],
    [0.9, 0.95, 1.05],
    [0.875, 0.85, 0.8],
    [0.8, 0.7, 0.6],
    [0.9, 0.9, 1.0],
    [0.75, 0.6, 0.5],
    [0.75, 0.675, 0.6],
    [0.8, 0.85, 0.95],
    [0.85, 0.9, 0.9],
    [1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
];

/// Grip factor for a surface row and tyre class, wet or dry.
pub fn surface_grip(surface: usize, tyre: usize, wet: bool) -> f32 {
    let t = if wet { &WET_GRIP } else { &DRY_GRIP };
    t[surface.min(15)][tyre.min(2)]
}

/// The asphalt grip factor for the physics session: what `ASPHALT[tyre]` becomes on a wet road.
/// Retail only applies it when the car is not under cover (a surface more than 3.5 m above it).
pub fn asphalt_grip(tyre: usize, wet: bool) -> f32 {
    surface_grip(1, tyre, wet)
}

/// Lightning: how stormy (`Conditions::lightning`) picks one of three strengths, each with a
/// chance per 64 Hz tick of starting a flash.
pub fn lightning_chance_per_tick(storm: f32) -> f32 {
    if storm < 0.04 {
        0.0
    } else if storm < 0.33 {
        1.0 / 30.0
    } else if storm < 0.66 {
        1.0 / 15.0
    } else {
        1.0 / 10.0
    }
}

/// Retail's precipitation particle count by graphics detail (0 lowest .. 3 highest).
pub const PRECIPITATION_PARTICLES: [u32; 4] = [60, 140, 220, 300];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_parse() {
        assert_eq!(WeatherOverride::parse("RAIN"), WeatherOverride::Rain);
        assert_eq!(WeatherOverride::parse("dry"), WeatherOverride::Dry);
        assert_eq!(WeatherOverride::parse("anything"), WeatherOverride::Track);
        assert_eq!(TimeOverride::parse("night"), TimeOverride::Night);
        assert_eq!(TimeOverride::parse(""), TimeOverride::Track);
    }

    #[test]
    fn retail_rules_for_night_and_wet() {
        let w = |track: &str, night, weather, time| TrackWeather::from_parts(track, None, TrackLighting::default(), night, None, weather, time);
        assert!(w("autobahn", false, WeatherOverride::Track, TimeOverride::Track).wet);
        assert!(w("AutoBahn", false, WeatherOverride::Track, TimeOverride::Track).wet);
        assert!(!w("alps", false, WeatherOverride::Track, TimeOverride::Track).wet);
        assert!(w("alps", false, WeatherOverride::Rain, TimeOverride::Track).wet);
        assert!(!w("autobahn", false, WeatherOverride::Dry, TimeOverride::Track).wet);
        assert!(w("monaco1", true, WeatherOverride::Track, TimeOverride::Track).night);
        assert!(!w("monaco1", true, WeatherOverride::Track, TimeOverride::Day).night);
        assert!(w("alps", false, WeatherOverride::Track, TimeOverride::Night).night);
        let rain = w("alps", false, WeatherOverride::Rain, TimeOverride::Track).at(0, 100);
        assert_eq!(rain.rain, 1.0);
    }

    #[test]
    fn wet_asphalt_swaps_road_and_second_tyres() {
        assert_eq!(asphalt_grip(0, false), 1.0);
        assert_eq!(asphalt_grip(0, true), 0.9);
        assert_eq!(asphalt_grip(1, true), 1.0);
        assert_eq!(asphalt_grip(2, true), 1.1);
        for r in 0..16 {
            assert_eq!(WET_GRIP[r][2], DRY_GRIP[r][2]);
            assert_eq!(WET_GRIP[r][0], DRY_GRIP[r][1]);
            assert_eq!(WET_GRIP[r][1], DRY_GRIP[r][0]);
        }
    }

    #[test]
    fn lightning_strengths() {
        assert_eq!(lightning_chance_per_tick(0.0), 0.0);
        assert_eq!(lightning_chance_per_tick(0.135), 1.0 / 30.0);
        assert_eq!(lightning_chance_per_tick(0.5), 1.0 / 15.0);
        assert_eq!(lightning_chance_per_tick(0.9), 1.0 / 10.0);
    }
}
