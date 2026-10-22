//! Sound levels from the Options > Audio page (`docs/provenance/options-audio.md`).
//!
//! Retail keeps five levels per player in the `.sav` (`Audio.MusicLevel` and friends), each a
//! whole number 0..=127 that starts at 127. We keep the same names and range in our own file,
//! `settings/audio.txt` in the project folder (gitignored), never in the game install. The first
//! run, with no file yet, takes its levels from the selected player's save.
//!
//! Other systems read [`AudioSettings`] (a Bevy resource) as 0..=1 levels: the front end's music,
//! and the race sound.

use bevy::prelude::*;
use std::path::PathBuf;

/// Retail's top level (the slider's right end).
pub const MAX_LEVEL: u32 = 127;

/// The level names, in the order the Audio page lists its sliders top to bottom.
pub const ACCESSORS: [&str; 5] = ["Audio.MusicLevel", "Audio.SfxLevel", "Audio.EngineLevel", "Audio.SpeechLevel", "Audio.AmbientLevel"];

/// The five levels, each 0..=1 (retail level / 127).
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct AudioSettings {
    pub music: f32,
    pub sfx: f32,
    pub engine: f32,
    pub speech: f32,
    pub ambient: f32,
}

impl Default for AudioSettings {
    /// Retail's starting levels: all at the top.
    fn default() -> Self {
        AudioSettings { music: 1.0, sfx: 1.0, engine: 1.0, speech: 1.0, ambient: 1.0 }
    }
}

impl AudioSettings {
    fn slot(&mut self, accessor: &str) -> Option<&mut f32> {
        match accessor {
            "Audio.MusicLevel" => Some(&mut self.music),
            "Audio.SfxLevel" => Some(&mut self.sfx),
            "Audio.EngineLevel" => Some(&mut self.engine),
            "Audio.SpeechLevel" => Some(&mut self.speech),
            "Audio.AmbientLevel" => Some(&mut self.ambient),
            _ => None,
        }
    }

    /// The level as retail stores it, 0..=127.
    pub fn level(&self, accessor: &str) -> Option<u32> {
        let v = *self.clone().slot(accessor)?;
        Some((v.clamp(0.0, 1.0) * MAX_LEVEL as f32).round() as u32)
    }

    /// Sets a level from retail's 0..=127 (clamped). False for an unknown name.
    pub fn set_level(&mut self, accessor: &str, level: u32) -> bool {
        match self.slot(accessor) {
            Some(v) => {
                *v = level.min(MAX_LEVEL) as f32 / MAX_LEVEL as f32;
                true
            }
            None => false,
        }
    }

    /// Takes the levels a retail save holds (others keep their value).
    pub fn from_save(save: &[u8]) -> AudioSettings {
        let mut out = AudioSettings::default();
        for a in ACCESSORS {
            if let Some(v) = crate::menu::save_value(save, a) {
                out.set_level(a, v);
            }
        }
        out
    }

    /// Reads `Name=level` lines; unknown or broken lines are skipped.
    pub fn parse(text: &str) -> AudioSettings {
        let mut out = AudioSettings::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            if let Ok(v) = v.trim().parse::<u32>() {
                out.set_level(k.trim(), v);
            }
        }
        out
    }

    pub fn to_text(&self) -> String {
        let mut s = String::from("# NFSPU-Rust sound levels, 0-127 like retail's Options > Audio.\n");
        for a in ACCESSORS {
            s.push_str(&format!("{a}={}\n", self.level(a).unwrap_or(MAX_LEVEL)));
        }
        s
    }

    /// Our settings file: `NFSPU_SETTINGS_DIR/audio.txt`, else `settings/audio.txt` in the project.
    pub fn path() -> PathBuf {
        let dir = std::env::var_os("NFSPU_SETTINGS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("settings"));
        dir.join("audio.txt")
    }

    /// The saved levels, if the file exists.
    pub fn load() -> Option<AudioSettings> {
        std::fs::read_to_string(Self::path()).ok().map(|t| Self::parse(&t))
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, self.to_text())
    }
}

/// Puts [`AudioSettings`] in the app from our file (defaults when there is none yet).
pub struct AudioSettingsPlugin;

impl Plugin for AudioSettingsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(AudioSettings::load().unwrap_or_default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_round_trip_through_text() {
        let mut s = AudioSettings::default();
        assert!(s.set_level("Audio.MusicLevel", 55));
        assert!(s.set_level("Audio.SfxLevel", 300));
        assert!(!s.set_level("Audio.Nothing", 1));
        let back = AudioSettings::parse(&s.to_text());
        assert_eq!(back.level("Audio.MusicLevel"), Some(55));
        assert_eq!(back.level("Audio.SfxLevel"), Some(127));
        assert_eq!(back.level("Audio.EngineLevel"), Some(127));
        assert!((back.music - 55.0 / 127.0).abs() < 1e-6);
    }

    #[test]
    fn reads_a_save() {
        let mut save = b"junkAudio.MusicLevel\0".to_vec();
        save.extend(0u32.to_le_bytes());
        save.extend(b"Audio.SpeechLevel\0");
        save.extend(64u32.to_le_bytes());
        let s = AudioSettings::from_save(&save);
        assert_eq!(s.level("Audio.MusicLevel"), Some(0));
        assert_eq!(s.level("Audio.SpeechLevel"), Some(64));
        assert_eq!(s.level("Audio.SfxLevel"), Some(127));
    }
}
