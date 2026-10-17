//! Front-end music: which song plays and when it changes (`docs/provenance/music.md`).
//!
//! The songs are the streams in `GameData/Music/zzzymus.viv`. Every front-end layout names the
//! music it wants in its header (`musicfile=menu*` on 93 screens, `NONE` on 5, `dogface` on one).
//! Verified from the executable's flow, described here in our own words:
//! - a screen whose pattern differs from the current one fades the music out (1.5 s) and, unless
//!   the pattern is `NONE`, starts a random song whose name matches it (`menu*` -> `menu01`..`05`);
//! - a song that ends is followed by the next matching song in archive order (the shuffle option,
//!   `RANDOMPLAYLIST`, is off by default);
//! - arriving at the main menu at least 60 s (7680 ticks) after it last picked starts another
//!   random menu song (fading the old one over 0.5 s).
//!
//! Checked against a retail recording: the main menu played `menu05` throughout, Options (`NONE`)
//! changed the music, and back on the main menu a different menu song (`menu02`) played.

/// Fade when a screen asks for different music (ms).
pub const SCREEN_FADE_MS: f32 = 1500.0;
/// Fade when the main menu replaces the song (ms).
pub const SWITCH_FADE_MS: f32 = 500.0;
/// The main menu picks a new song when it has not done so for this long (s).
pub const MAIN_MENU_REPICK_S: f64 = 7680.0 / crate::menu_movie::TICKS_PER_SECOND as f64;

/// Case-insensitive wildcard match: `*` any run, `?` one character.
pub fn matches(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.to_ascii_lowercase().chars().collect();
    let n: Vec<char> = name.to_ascii_lowercase().chars().collect();
    let (mut i, mut j) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while j < n.len() {
        if i < p.len() && (p[i] == '?' || p[i] == n[j]) {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == '*' {
            star = Some((i, j));
            i += 1;
        } else if let Some((si, sj)) = star {
            i = si + 1;
            j = sj + 1;
            star = Some((si, sj + 1));
        } else {
            return false;
        }
    }
    p[i..].iter().all(|c| *c == '*')
}

/// The pattern a screen asks for: its `musicfile`, or `menu*` when it names none.
pub fn screen_pattern(musicfile: &str) -> String {
    let m = musicfile.trim();
    if m.is_empty() { "menu*".to_string() } else { m.to_string() }
}

/// What the player should do.
#[derive(Debug, Clone, PartialEq)]
pub enum Cue {
    /// Fade the current song out over this many ms, then play this song (archive index), or
    /// nothing.
    Change { fade_ms: f32, next: Option<usize> },
}

/// Song choice state. `names` are the archive's entry names in archive order.
#[derive(Debug, Clone)]
pub struct Jukebox {
    names: Vec<String>,
    pattern: Option<String>,
    /// The matching songs (archive indices, archive order) and the position playing in it.
    list: Vec<usize>,
    position: Option<usize>,
    /// When the main menu last picked a song (s since start).
    main_menu_pick: f64,
}

impl Jukebox {
    pub fn new(names: Vec<String>) -> Jukebox {
        Jukebox { names, pattern: None, list: Vec::new(), position: None, main_menu_pick: 0.0 }
    }

    pub fn name(&self, song: usize) -> &str {
        self.names.get(song).map(String::as_str).unwrap_or("")
    }

    fn pick(&mut self, pattern: &str, random: u32) -> Option<usize> {
        self.list = (0..self.names.len()).filter(|i| matches(pattern, &self.names[*i])).collect();
        if self.list.is_empty() {
            self.position = None;
            return None;
        }
        let k = random as usize % self.list.len();
        self.position = Some(k);
        Some(self.list[k])
    }

    /// A screen became the active one. `random` is any fresh random number.
    pub fn enter_screen(&mut self, musicfile: &str, main_menu: bool, now_s: f64, random: u32) -> Option<Cue> {
        let pattern = screen_pattern(musicfile);
        if self.pattern.as_deref().is_none_or(|p| !p.eq_ignore_ascii_case(&pattern)) {
            self.pattern = Some(pattern.clone());
            let next = if pattern.eq_ignore_ascii_case("NONE") { None } else { self.pick(&pattern, random) };
            if next.is_none() {
                self.list.clear();
                self.position = None;
            }
            if main_menu {
                self.main_menu_pick = now_s;
            }
            return Some(Cue::Change { fade_ms: SCREEN_FADE_MS, next });
        }
        if main_menu && now_s - self.main_menu_pick >= MAIN_MENU_REPICK_S {
            self.main_menu_pick = now_s;
            let next = self.pick("menu*", random);
            return Some(Cue::Change { fade_ms: SWITCH_FADE_MS, next });
        }
        None
    }

    /// The song playing finished: the next matching song, in archive order.
    pub fn song_ended(&mut self) -> Option<usize> {
        let k = self.position?;
        let k = (k + 1) % self.list.len();
        self.position = Some(k);
        Some(self.list[k])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Vec<String> {
        ["game13-Activator", "menu01-Psychonaught", "menu02-Aircon", "win01-Warm", "menu03-Dr. Know"].iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn wildcards() {
        assert!(matches("menu*", "menu01-Psychonaught"));
        assert!(matches("Menu*", "menu01-Psychonaught"));
        assert!(!matches("menu*", "game13-Activator"));
        assert!(matches("m?nu0*-*", "menu02-Aircon"));
        assert!(!matches("NONE", "menu01"));
        assert!(matches("*", ""));
    }

    #[test]
    fn screens_pick_change_and_silence() {
        let mut j = Jukebox::new(names());
        // First screen: a random menu song (random 1 -> second match).
        assert_eq!(j.enter_screen("menu*", true, 0.0, 1), Some(Cue::Change { fade_ms: SCREEN_FADE_MS, next: Some(2) }));
        // Same pattern: nothing changes.
        assert_eq!(j.enter_screen("menu*", false, 10.0, 0), None);
        // NONE fades out to silence; back to menu* picks again.
        assert_eq!(j.enter_screen("NONE", false, 20.0, 0), Some(Cue::Change { fade_ms: SCREEN_FADE_MS, next: None }));
        assert_eq!(j.song_ended(), None);
        assert_eq!(j.enter_screen("", true, 30.0, 2), Some(Cue::Change { fade_ms: SCREEN_FADE_MS, next: Some(4) }));
    }

    #[test]
    fn songs_follow_in_order_and_main_menu_repicks_after_a_minute() {
        let mut j = Jukebox::new(names());
        j.enter_screen("menu*", true, 0.0, 0);
        assert_eq!(j.song_ended(), Some(2));
        assert_eq!(j.song_ended(), Some(4));
        assert_eq!(j.song_ended(), Some(1));
        assert_eq!(j.enter_screen("menu*", true, 59.0, 0), None);
        assert_eq!(j.enter_screen("menu*", true, 61.0, 2), Some(Cue::Change { fade_ms: SWITCH_FADE_MS, next: Some(4) }));
    }
}
