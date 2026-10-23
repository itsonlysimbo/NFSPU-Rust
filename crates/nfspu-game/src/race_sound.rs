//! In-race sound: which retail sounds play and how loud and high they play
//! (`docs/provenance/sound.md`). This module holds the rules and the data loading; the Bevy side
//! that starts the voices and feeds them every frame is `race_audio.rs` in the game binary.
//!
//! What is modelled, all from the executable and the shipped files:
//! - the player's engine: the car's engine archive (`GameData/Sounds/<sound>.viv`, the
//!   catalogue's sound name) holds a bank and four crossfade tables. Eight layers (the `.ect`
//!   coast table and the `.elt` load table, four sounds each for the exhaust side and four for
//!   the engine side) fade by engine speed (the tables' curves), by throttle (coast against load)
//!   and by view (exhaust against engine). Every layer plays at one shared pitch that rises in a
//!   straight line with engine speed; each sound's recorded key moves it to a common note, so the
//!   layers stay in tune with each other while they cross. See [`EngineMix`];
//! - opponents' engines: one loop each from `<sound>lden.bnk` and `<sound>ldex.bnk` in
//!   `zzzwzzz.viv`, same pitch rule, louder when near, the first while the car comes closer and
//!   the second while it moves away ([`opponent_engine`]);
//! - tyres, road, wind and hits from `SND_coll.bnk` ([`coll`]); the countdown voice from the
//!   speech archive; the track's ambient sounds placed by `<track>_audio.scn` ([`Ambient`]).

use nfspu_formats::engine_snd::{self, Tuning};
use nfspu_formats::{bnk, viv};
use std::path::{Path, PathBuf};

/// The sound clock (the race's 64 Hz tick).
pub const TICK_HZ: f32 = 64.0;
/// The note every sound is pitched to before the engine-speed pitch applies (the sound
/// library's default note).
pub const BASE_KEY: f32 = 60.0;

/// The mixer's crossfade curve: 0..128 in, 0..127 out, rising fast then flattening (close to an
/// equal-power fade). Retail uses a 129-entry table; this logarithmic curve stays within two
/// steps of it everywhere.
pub fn fade(x: f32) -> f32 {
    let t = (x / 128.0).clamp(0.0, 1.0);
    127.0 * (1.0 + 8.0 * t).ln() / 9.0f32.ln()
}

/// Playback rate factor for a sound recorded at `key` (+`fine` cents) to sound at the base note.
pub fn key_ratio(key: u8, fine: i8) -> f32 {
    ((BASE_KEY - key as f32) / 12.0 + fine as f32 / 1200.0).exp2()
}

/// The engine's shared pitch (1 = as recorded at the base note) at a fraction of the car's
/// redline: `scale` x (fraction + 0.2) / 4, with the per-car scale of 80 by default. So 0.25 at
/// standstill and 1.5 at the redline for most cars.
pub fn engine_pitch(rpm_frac: f32, pitch_scale: u32) -> f32 {
    let x = rpm_frac.max(0.0) * 65536.0;
    pitch_scale as f32 * (x + 13107.0) / 1024.0 / 4096.0
}

/// Volume-curve step (0..511) for a fraction of the redline: `scale` x fraction x 4 (416 at the
/// redline with the usual scale of 104).
pub fn engine_step(rpm_frac: f32, step_scale: u32) -> usize {
    let x = rpm_frac.max(0.0) * 65536.0;
    ((step_scale as f32 * x / 16384.0) as usize).min(engine_snd::STEPS - 1)
}

/// Retail's distance volume: `2048 / (metres + 2)`, used above 127 for a nearby engine (it caps
/// later).
pub fn distance_volume(metres: f32) -> f32 {
    2048.0 / (metres.max(0.0) + 2.0)
}

/// A decoded sound: interleaved 16-bit samples. A looped sound keeps only its loop (the part
/// before the loop start is the attack, which the loops never return to).
#[derive(Debug, Clone, PartialEq)]
pub struct Pcm {
    pub samples: Vec<i16>,
    pub channels: u16,
    pub rate: u32,
    pub looped: bool,
    pub key: u8,
    pub fine: i8,
}

impl Pcm {
    pub fn seconds(&self) -> f32 {
        self.samples.len() as f32 / self.channels.max(1) as f32 / self.rate.max(1) as f32
    }

    /// A WAV file of the samples (what Bevy's audio decoder takes).
    pub fn wav(&self) -> Vec<u8> {
        bnk::wav(&self.samples, self.channels.max(1), self.rate)
    }
}

/// Sound `n` of a bank, decoded.
pub fn decode(bank: &[u8], n: usize) -> Option<Pcm> {
    let patch = bnk::sound(bank, n)?;
    let mut samples = bnk::decode(bank, &patch, bnk::Interleave::Planar);
    let ch = patch.channels.max(1) as usize;
    let looped = patch.looped();
    if looped {
        let end = ((patch.loop_end as usize + 1) * ch).min(samples.len());
        let start = (patch.loop_start as usize * ch).min(end);
        samples = samples[start..end].to_vec();
    }
    (!samples.is_empty()).then_some(Pcm { samples, channels: patch.channels.max(1), rate: patch.sample_rate, looped, key: patch.key, fine: patch.fine })
}

/// A `.viv` archive read whole (the sound archives are small) and its index.
pub struct Archive {
    pub data: Vec<u8>,
    pub entries: Vec<viv::Entry>,
}

impl Archive {
    pub fn open(path: &Path) -> Option<Archive> {
        let data = std::fs::read(path).ok()?;
        let size = viv::header_size(data.get(0..16)?).ok()?;
        let entries = viv::index(data.get(..size)?).ok()?;
        Some(Archive { data, entries })
    }

    /// An entry's bytes by name (any letter case).
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        let e = self.entries.iter().find(|e| e.name.eq_ignore_ascii_case(name))?;
        self.data.get(e.offset as usize..(e.offset + e.size) as usize)
    }
}

/// A file in `dir` by name, any letter case.
pub fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let want = name.to_ascii_lowercase();
    std::fs::read_dir(dir).ok()?.filter_map(|e| e.ok()).find(|e| e.file_name().to_string_lossy().to_ascii_lowercase() == want).map(|e| e.path())
}

pub fn sounds_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("GameData").join("Sounds")
}

/// One engine layer: which sound, its volume over engine speed, and how it fades.
#[derive(Debug, Clone, PartialEq)]
pub struct EngineLayer {
    pub sound: usize,
    pub volume: Vec<u8>,
    /// From the load table (louder with throttle); else the coast table (louder without).
    pub on_load: bool,
    pub pcm: Pcm,
}

impl EngineLayer {
    /// Sounds 64 and up are the engine side, below 64 the exhaust side (the view balance fades
    /// between them).
    pub fn engine_side(&self) -> bool {
        self.sound >= 64
    }
}

/// A car's engine sound set.
#[derive(Debug, Clone)]
pub struct EngineSound {
    /// Which archive was used (`sd1a`; `sd7a` when the car's own is missing, as retail does).
    pub name: String,
    pub layers: Vec<EngineLayer>,
    /// Engine-bank sound 2: the gear-change clunk.
    pub shift: Option<Pcm>,
    pub tuning: Tuning,
    /// Whether the tuning came from the car's `.spl` (else the built-in defaults).
    pub tuned: bool,
}

/// Loads a car's engine sound: `<sound>.viv` (falling back to `sd7a.viv`) and `<sim>.spl`.
pub fn load_engine(game_dir: &Path, sound: &str, sim: &str) -> Result<EngineSound, String> {
    let dir = sounds_dir(game_dir);
    let (name, archive) = [sound, "sd7a"]
        .iter()
        .filter(|n| !n.is_empty())
        .find_map(|n| Some((n.to_string(), Archive::open(&find_file(&dir, &format!("{n}.viv"))?)?)))
        .ok_or_else(|| format!("no engine archive for '{sound}' (nor sd7a.viv) in {}", dir.display()))?;
    let bank = archive.entries.iter().find(|e| e.name.to_ascii_lowercase().ends_with(".bnk")).and_then(|e| archive.get(&e.name)).ok_or("engine archive has no .bnk")?;
    let table = |ext: &str| archive.entries.iter().find(|e| e.name.to_ascii_lowercase().ends_with(ext)).and_then(|e| archive.get(&e.name)).and_then(|d| engine_snd::parse_table(d).ok());
    let mut layers = Vec::new();
    for (ext, on_load) in [(".ect", false), (".elt", true)] {
        let t = table(ext).ok_or_else(|| format!("{name}.viv: no {ext} table"))?;
        for l in t.layers.into_iter().flatten() {
            if let Some(pcm) = decode(bank, l.sound) {
                layers.push(EngineLayer { sound: l.sound, volume: l.volume, on_load, pcm });
            }
        }
    }
    let spl = find_file(&dir, &format!("{sim}.spl")).and_then(|p| std::fs::read(p).ok()).and_then(|d| engine_snd::parse_tuning(&d).ok());
    Ok(EngineSound { name, layers, shift: decode(bank, 2), tuning: spl.unwrap_or_default(), tuned: spl.is_some() })
}

/// What the engine mixer needs each tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EngineInput {
    /// Engine speed over the redline (1 = redline).
    pub rpm_frac: f32,
    /// Throttle the engine used, 0..1.
    pub throttle: f32,
    /// A gear change is in progress.
    pub shifting: bool,
    /// First-person view (cockpit/bumper); else an outside view.
    pub near: bool,
    /// Camera to car (m).
    pub distance: f32,
    /// Car speed (m/s).
    pub speed: f32,
}

/// The player's engine mixer: per-layer volumes (0..127) and the shared pitch, updated on the
/// 64 Hz clock.
#[derive(Debug, Clone)]
pub struct EngineMix {
    curves: Vec<(Vec<u8>, bool, bool)>,
    ratios: Vec<f32>,
    tuning: Tuning,
    /// Smoothed throttle, 0..255.
    held: f32,
    target: Vec<f32>,
    level: Vec<f32>,
    pitch: f32,
    step: usize,
    load: f32,
    ticks: u32,
}

/// Retail moves each layer's volume at most this far per mixer update.
const SLEW: f32 = 8.0;
/// The mixer updates every third tick.
const MIX_EVERY: u32 = 3;

impl EngineMix {
    pub fn new(sound: &EngineSound) -> EngineMix {
        let n = sound.layers.len();
        EngineMix {
            curves: sound.layers.iter().map(|l| (l.volume.clone(), l.on_load, l.engine_side())).collect(),
            ratios: sound.layers.iter().map(|l| key_ratio(l.pcm.key, l.pcm.fine)).collect(),
            tuning: sound.tuning,
            held: 0.0,
            target: vec![0.0; n],
            level: vec![0.0; n],
            pitch: engine_pitch(0.0, sound.tuning.pitch_scale),
            step: 0,
            load: 0.0,
            ticks: 0,
        }
    }

    /// One 64 Hz tick.
    pub fn tick(&mut self, input: EngineInput) {
        // throttle smoothing: up by half the gap per tick, down by an eighth (at least one step)
        let want = if input.shifting { 0.0 } else { input.throttle.clamp(0.0, 1.0) * 255.0 };
        let gap = want - self.held;
        if gap > 0.0 {
            self.held += (gap / 2.0).max(1.0).min(gap);
        } else if gap < 0.0 {
            self.held += (gap / 8.0).min(-1.0).max(gap);
        }
        let l = (self.held / 2.0).floor();
        self.load = (l + (l / 64.0).floor() + (l / 32.0).floor()).clamp(0.0, 128.0);
        self.step = engine_step(input.rpm_frac, self.tuning.step_scale);
        self.pitch = engine_pitch(input.rpm_frac, self.tuning.pitch_scale);

        // overall level: distance, a little louder with speed, the view's level
        let near = distance_volume(input.distance);
        let mph = (input.speed.abs() * 2.237).min(150.0);
        let mut v = near.max((mph + 40.0) * near / 128.0) * 127.0 / 128.0;
        if input.shifting {
            v *= 0.6;
        }
        let p2 = (1686.0 * v / 16384.0).floor();
        let (level, balance) = if input.near { (self.tuning.level_near, self.tuning.balance_near) } else { (self.tuning.level_far, self.tuning.balance_far) };
        let master = (level as f32 * p2 / 64.0).min(2047.0);
        let (bal_exhaust, bal_engine) = (fade(balance as f32), fade(128.0 - balance as f32));
        for (i, (curve, on_load, engine_side)) in self.curves.iter().enumerate() {
            let a = curve.get(self.step).copied().unwrap_or(0) as f32;
            let f = if *on_load { fade(self.load) } else { fade(128.0 - self.load) };
            let b = if *engine_side { bal_engine } else { bal_exhaust };
            self.target[i] = ((a * f * b / 8.0).floor() * master / 262144.0).min(127.0);
        }
        self.ticks += 1;
        if self.ticks % MIX_EVERY == 0 {
            for (lv, t) in self.level.iter_mut().zip(&self.target) {
                *lv += (t - *lv).clamp(-SLEW, SLEW);
            }
        }
    }

    pub fn layers(&self) -> usize {
        self.level.len()
    }

    /// Layer `i`'s volume, 0..1.
    pub fn gain(&self, i: usize) -> f32 {
        self.level.get(i).copied().unwrap_or(0.0) / 127.0
    }

    /// Layer `i`'s playback rate factor.
    pub fn speed(&self, i: usize) -> f32 {
        self.pitch * self.ratios.get(i).copied().unwrap_or(1.0)
    }

    pub fn pitch(&self) -> f32 {
        self.pitch
    }

    pub fn step(&self) -> usize {
        self.step
    }

    /// Mixer load, 0..128 (0 coasting, 128 full throttle).
    pub fn load(&self) -> f32 {
        self.load
    }
}

/// An opponent's engine sounds: `<sound>lden.bnk` and `<sound>ldex.bnk` from `zzzwzzz.viv`
/// (`sd6b` when the car's own are missing, as retail does).
#[derive(Debug, Clone)]
pub struct OpponentSound {
    pub name: String,
    pub approach: Pcm,
    pub away: Pcm,
}

pub fn load_opponent(archive: &Archive, sound: &str) -> Option<OpponentSound> {
    [sound, "sd6b"].iter().filter(|n| !n.is_empty()).find_map(|n| {
        let one = |suffix: &str| decode(archive.get(&format!("{n}{suffix}.bnk"))?, 0);
        Some(OpponentSound { name: n.to_string(), approach: one("lden")?, away: one("ldex")? })
    })
}

/// An opponent engine's two volumes (approach sound, away sound; 0..1 before the effects
/// level) and the shared pitch. `receding` is the car's speed away from the listener over its
/// speed (-1 straight at the listener .. 1 straight away).
pub fn opponent_engine(rpm_frac: f32, distance: f32, receding: f32) -> (f32, f32, f32) {
    let vol = distance_volume(distance).min(127.0);
    let pan = (receding * 1.5 * 64.0).clamp(-63.0, 63.0);
    let scale = vol / 127.0 * OPPONENT_LEVEL / 127.0; // fade() is 0..127
    (fade(64.0 - pan) * scale, fade(64.0 + pan) * scale, engine_pitch(rpm_frac, 0x50))
}

/// Opponent engines' channel level: retail scales them by 70/127 and 85/128 (~0.37).
pub const OPPONENT_LEVEL: f32 = 70.0 * 85.0 / 128.0 / 127.0;
/// Most effect channels (tyres, road, wind, hits): 127 x 85 / 128 / 127 ~ 0.66 of full.
pub const EFFECT_LEVEL: f32 = 85.0 / 128.0;

/// Sound numbers in `SND_coll.bnk` (the game's numbering: entry n + 1 of the slot table).
pub mod coll {
    /// Tyres rolling on tarmac (the road-noise channel: base 0x11 plus the surface; 0 = tarmac).
    pub const ROAD: usize = 0x11;
    /// The skid layers picked for a tarmac surface: the tonal squeal and the rougher scrub that
    /// joins it on hard slides.
    pub const SQUEAL: usize = 0x2a;
    pub const SCRUB: usize = 0x28;
    /// Scraping along a wall (the same table's default entry).
    pub const SCRAPE: usize = 0x2c;
    /// Wind (a looping gust whose level and pitch wander at random).
    pub const WIND: usize = 0x1f;
    /// Hits against a wall: three variants, never the same one twice in a row.
    pub const WALL_HIT: usize = 0x40;
    /// Car against car.
    pub const CAR_HIT: usize = 0x20;
}

/// Tyre skid volumes (squeal, scrub) for the strongest wheel slide (0..1).
pub fn skid_gains(slide: f32) -> (f32, f32) {
    let s = slide.clamp(0.0, 1.0);
    (s * EFFECT_LEVEL, ((s - 0.5) / 0.5).clamp(0.0, 1.0) * EFFECT_LEVEL)
}

/// Road noise volume at a speed (m/s): retail fades it in above about 15 units of speed and caps
/// it at 122 of 127.
pub fn road_gain(speed: f32) -> f32 {
    let kmh = speed.abs() * 3.6;
    ((kmh / 2.0 - 15.0) / 127.0).clamp(0.0, 122.0 / 127.0) * EFFECT_LEVEL
}

/// A hit's volume (0..1) from its impact speed into the wall (m/s): 64..127 of 127, full at
/// 10 m/s.
pub fn hit_gain(impact: f32) -> f32 {
    (64.0 + (impact * 63.0 / 10.0).clamp(0.0, 63.0)) / 127.0
}

/// Speed of sound (m/s) for the Doppler shift.
pub const SOUND_SPEED: f32 = 343.0;

/// Doppler pitch factor for a source closing on the listener at `closing` m/s (negative while it
/// moves away): `c / (c - v)`, with the closing speed held to +-60 m/s so a glitching car can't
/// send the pitch wild (0.85..1.21). The rule is textbook physics; whether retail shifts the pitch
/// at all, or by how much, is **not traced** (the Ghidra bridge was down for this step).
pub fn doppler(closing: f32) -> f32 {
    let v = closing.clamp(-60.0, 60.0);
    SOUND_SPEED / (SOUND_SPEED - v)
}

/// Closing speed (m/s, positive = coming closer) of a source at `rel` (source minus listener)
/// moving at `src_vel`, heard by a listener moving at `ear_vel`. Only the ground plane counts.
pub fn closing_speed(rel: [f32; 2], src_vel: [f32; 2], ear_vel: [f32; 2]) -> f32 {
    let len = (rel[0] * rel[0] + rel[1] * rel[1]).sqrt();
    if len < 1e-3 {
        return 0.0;
    }
    let dir = [rel[0] / len, rel[1] / len];
    // the gap shrinks when the listener moves along `dir` or the source moves against it
    (ear_vel[0] - src_vel[0]) * dir[0] + (ear_vel[1] - src_vel[1]) * dir[1]
}

/// Rain on the car (`rain.bnk` in `zzzwzzz.viv`, one loop): the weather's rain amount (0..1)
/// times the effects channel, at full level in the cockpit and 0.6 outside (the drumming is on
/// the roof you sit under). The scaling by view is our guess; the retail rain channel's inputs
/// are not traced.
pub fn rain_gain(amount: f32, near: bool) -> f32 {
    amount.clamp(0.0, 1.0) * if near { 1.0 } else { 0.6 } * EFFECT_LEVEL
}

/// An opponent's tyre squeal: the player's skid rule for its slide, times retail's distance
/// volume (`2048 / (m + 2)` capped at 127) like its engine.
pub fn opponent_skid_gain(slide: f32, distance: f32) -> f32 {
    skid_gains(slide).0 * distance_volume(distance).min(127.0) / 127.0
}

/// One car as the sound trace sees it: ground position (x, z), ground velocity (m/s), strongest
/// wheel slide (0..1) and engine speed over its redline.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TraceCar {
    pub pos: [f32; 2],
    pub vel: [f32; 2],
    pub slide: f32,
    pub rpm_frac: f32,
}

/// Logs when the event-like channels change: rain starting and stopping, each car's skid
/// starting and stopping (car 0 is the player), and opponents passing the player (closing turns
/// to opening within 40 m) with the Doppler factor and engine volumes either side. Used by the
/// game (`race_audio.rs`) and the headless `race_drive` so both write the same lines to
/// `logs/race_sound.log`. Volumes are before the Options > Audio levels.
#[derive(Debug, Clone, Default)]
pub struct SoundTrace {
    raining: bool,
    skidding: Vec<bool>,
    closing: Vec<bool>,
}

impl SoundTrace {
    /// Volume below which a channel counts as silent.
    pub const AUDIBLE: f32 = 0.02;

    pub fn observe(&mut self, tick: u32, rain_amount: f32, near: bool, cars: &[TraceCar]) -> Vec<String> {
        let mut out = Vec::new();
        let rain = rain_gain(rain_amount, near);
        if (rain > Self::AUDIBLE) != self.raining {
            self.raining = !self.raining;
            out.push(format!("tick {tick}: rain {} (amount {rain_amount:.2}, volume {rain:.2})", if self.raining { "starts" } else { "stops" }));
        }
        self.skidding.resize(cars.len(), false);
        self.closing.resize(cars.len(), false);
        let Some(me) = cars.first() else { return out };
        for (i, c) in cars.iter().enumerate() {
            let speed = (c.vel[0] * c.vel[0] + c.vel[1] * c.vel[1]).sqrt();
            let rel = [c.pos[0] - me.pos[0], c.pos[1] - me.pos[1]];
            let dist = (rel[0] * rel[0] + rel[1] * rel[1]).sqrt();
            let slide = if speed > 2.0 { c.slide } else { 0.0 };
            let skid = if i == 0 { skid_gains(slide).0 } else { opponent_skid_gain(slide, dist) };
            if (skid > Self::AUDIBLE) != self.skidding[i] {
                self.skidding[i] = !self.skidding[i];
                out.push(format!("tick {tick}: car {i} skid {} (slide {:.2}, {dist:.1} m, volume {skid:.2})", if self.skidding[i] { "starts" } else { "stops" }, c.slide));
            }
            if i == 0 {
                continue;
            }
            let closing = closing_speed(rel, c.vel, me.vel);
            if dist < 40.0 && closing.abs() > 3.0 && (closing > 0.0) != self.closing[i] {
                self.closing[i] = closing > 0.0;
                let receding = if speed > 0.5 && dist > 1e-3 { (rel[0] * c.vel[0] + rel[1] * c.vel[1]) / dist / speed } else { 0.0 };
                let (front, back, pitch) = opponent_engine(c.rpm_frac, dist, receding);
                out.push(format!(
                    "tick {tick}: car {i} {} at {dist:.1} m, closing {closing:+.1} m/s, Doppler x{:.3}, engine pitch {:.3}, volumes approach {front:.2} away {back:.2}",
                    if self.closing[i] { "approaching" } else { "passes, moving away" },
                    doppler(closing),
                    pitch * doppler(closing)
                ));
            }
        }
        out
    }
}

/// The wind's wandering level and pitch (retail's random walk: level 40..127 in steps of 1..8,
/// pitch 0.977..1.023 of normal in steps of 10..90 / 65536).
#[derive(Debug, Clone)]
pub struct Wind {
    pub level: i32,
    pub pitch: i32,
    level_step: i32,
    pitch_step: i32,
    rng: u32,
}

impl Default for Wind {
    fn default() -> Self {
        Wind { level: 0x50, pitch: 0x10000, level_step: 3, pitch_step: 20, rng: 0x1234_5678 }
    }
}

impl Wind {
    fn next(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng
    }

    pub fn tick(&mut self) {
        self.level += self.level_step;
        self.pitch += self.pitch_step;
        if self.level > 0x7f {
            self.level = 0x7f;
            self.level_step = -1 - (self.next() & 7) as i32;
        }
        if self.level < 0x28 {
            self.level = 0x28;
            self.level_step = (self.next() & 7) as i32 + 1;
        }
        if self.pitch > 0x105dc {
            self.pitch = 0x105dc;
            self.pitch_step = -0x14 - (self.next() % 0x50) as i32;
        }
        if self.pitch < 0xfa24 {
            self.pitch = 0xfa24;
            self.pitch_step = (self.next() % 0x32) as i32 + 10;
        }
    }

    /// Volume (0..1) at a speed (m/s): the wandering level times the speed (full at 200 km/h).
    /// The speed scaling is our guess; the retail input to this channel is not traced.
    pub fn gain(&self, speed: f32) -> f32 {
        (speed.abs() * 3.6 / 200.0).clamp(0.0, 1.0) * self.level as f32 / 127.0 * EFFECT_LEVEL
    }

    pub fn speed(&self) -> f32 {
        self.pitch as f32 / 65536.0
    }
}

/// The countdown voice: `sor-a-3`, `-2`, `-1` and `-go` in `GameData/Speech/zzzx<lang>.viv`.
pub fn countdown_entry(count: u8) -> String {
    match count {
        0 => "sor-a-go.bnk".to_string(),
        n => format!("sor-a-{n}.bnk"),
    }
}

/// The speech archive (`zzzxeng.viv` for English; any `zzzx*.viv` otherwise).
pub fn speech_archive(game_dir: &Path) -> Option<Archive> {
    let dir = game_dir.join("GameData").join("Speech");
    let path = find_file(&dir, "zzzxeng.viv").or_else(|| {
        std::fs::read_dir(&dir).ok()?.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| {
            let n = p.file_name().unwrap_or_default().to_string_lossy().to_ascii_lowercase();
            n.starts_with("zzzx") && n.ends_with(".viv")
        })
    })?;
    Archive::open(&path)
}

/// One ambient sound of a track (`GameData/Track/<track>_audio.scn`, `AUDIO_ELEMENT 4`): a
/// position line, a line of twelve numbers and a name. Read so far: the first number is the
/// play mode, the second the sound (`track.viv` entry `*_<n>.bnk`), then the outer and inner
/// radius (m), the shortest and longest wait between plays (s; both 0 = a steady loop) and, ninth
/// to last, the volume 0..100. The rest are not read.
#[derive(Debug, Clone, PartialEq)]
pub struct Ambient {
    pub pos: [f32; 3],
    pub mode: i32,
    pub sound: usize,
    pub outer: f32,
    pub inner: f32,
    pub wait: (f32, f32),
    pub volume: f32,
    pub name: String,
}

impl Ambient {
    /// A steady loop (no wait between plays).
    pub fn steady(&self) -> bool {
        self.wait.1 <= 0.0
    }

    /// Volume (0..1) at a distance: full inside the inner radius, fading to nothing at the outer
    /// one. The fade shape is our guess (the retail placement code is not traced).
    pub fn gain(&self, distance: f32) -> f32 {
        if distance >= self.outer {
            return 0.0;
        }
        let span = (self.outer - self.inner).max(1.0);
        let near = 1.0 - ((distance - self.inner) / span).clamp(0.0, 1.0);
        near * self.volume / 100.0 * EFFECT_LEVEL
    }
}

/// Reads a track's ambient sounds.
pub fn parse_ambient(text: &str) -> Vec<Ambient> {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let mut out = Vec::new();
    let nums = |l: &str| l.split_whitespace().map(|t| t.parse::<f32>().ok()).collect::<Option<Vec<f32>>>();
    for (i, l) in lines.iter().enumerate() {
        if !l.starts_with("AUDIO_ELEMENT") {
            continue;
        }
        let (Some(p), Some(v), Some(name)) = (lines.get(i + 1).and_then(|l| nums(l)), lines.get(i + 2).and_then(|l| nums(l)), lines.get(i + 3)) else { continue };
        if p.len() != 3 || v.len() < 11 || v[1] < 0.0 {
            continue;
        }
        out.push(Ambient {
            pos: [p[0], p[1], p[2]],
            mode: v[0] as i32,
            sound: v[1] as usize,
            outer: v[2],
            inner: v[3],
            wait: (v[4], v[5]),
            volume: v[10],
            name: name.to_string(),
        });
    }
    out
}

/// Finds `<track>_audio.scn` (any letter case) in `GameData/Track`.
pub fn ambient_file(game_dir: &Path, track: &str) -> Option<PathBuf> {
    find_file(&game_dir.join("GameData").join("Track"), &format!("{track}_audio.scn"))
}

/// The number at the end of a `track.viv` entry name (`waves_980.bnk` -> 980).
pub fn track_sound_number(entry: &str) -> Option<usize> {
    let stem = entry.rsplit_once('.').map_or(entry, |(s, _)| s);
    stem.rsplit_once('_')?.1.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_curve_tracks_the_retail_table() {
        // a few entries of retail's table (index -> value)
        for (x, want) in [(0.0, 0.0), (8.0, 24.0), (32.0, 65.0), (64.0, 94.0), (96.0, 112.0), (128.0, 127.0)] {
            assert!((fade(x) - want).abs() <= 2.0, "fade({x}) = {} want {want}", fade(x));
        }
    }

    #[test]
    fn engine_pitch_and_step() {
        assert!((engine_pitch(0.0, 80) - 0.25).abs() < 0.01);
        assert!((engine_pitch(1.0, 80) - 1.5).abs() < 0.01);
        assert_eq!(engine_step(1.0, 104), 416);
        assert_eq!(engine_step(2.0, 104), 511);
        // a layer recorded an octave below the base note plays twice as fast
        assert!((key_ratio(48, 0) - 2.0).abs() < 1e-4);
        assert!((key_ratio(60, -100) - 0.9439).abs() < 1e-3);
    }

    fn layer(sound: usize, on_load: bool, volume: u8) -> EngineLayer {
        EngineLayer { sound, volume: vec![volume; engine_snd::STEPS], on_load, pcm: Pcm { samples: vec![0; 4], channels: 1, rate: 22050, looped: true, key: 60, fine: 0 } }
    }

    #[test]
    fn mixer_fades_coast_to_load_with_throttle() {
        let sound = EngineSound { name: "t".into(), layers: vec![layer(1, false, 127), layer(65, true, 127)], shift: None, tuning: Tuning::default(), tuned: false };
        let mut m = EngineMix::new(&sound);
        let input = |throttle: f32| EngineInput { rpm_frac: 0.5, throttle, shifting: false, near: false, distance: 5.0, speed: 0.0 };
        for _ in 0..200 {
            m.tick(input(0.0));
        }
        assert_eq!(m.load(), 0.0);
        let coast = (m.gain(0), m.gain(1));
        for _ in 0..200 {
            m.tick(input(1.0));
        }
        assert_eq!(m.load(), 128.0);
        let load = (m.gain(0), m.gain(1));
        assert!(coast.0 > 0.2 && coast.1 == 0.0, "coasting: {coast:?}");
        assert!(load.0 == 0.0 && load.1 > 0.2, "full throttle: {load:?}");
        assert!((m.pitch() - engine_pitch(0.5, 0x50)).abs() < 1e-6);
    }

    #[test]
    fn opponent_engine_crossfades_by_direction() {
        let (front, back, _) = opponent_engine(0.5, 10.0, -1.0);
        assert!(front > back);
        let (front, back, _) = opponent_engine(0.5, 10.0, 1.0);
        assert!(front < back);
        let (near, _, _) = opponent_engine(0.5, 5.0, 0.0);
        let (far, _, _) = opponent_engine(0.5, 100.0, 0.0);
        assert!(near > far * 4.0);
    }

    #[test]
    fn wind_wanders_within_bounds() {
        let mut w = Wind::default();
        for _ in 0..5000 {
            w.tick();
            assert!((0x28..=0x7f).contains(&w.level));
            assert!((0.97..=1.03).contains(&w.speed()));
        }
    }

    #[test]
    fn reads_ambient_elements() {
        let text = "AUDIO_ELEMENT 4\n2562.9 8.1 -968.7\n0 338 200.0 0.0 40 108 0 2 1489 -1 88 0\nShips Bell 3 733\nAUDIO_ELEMENT 4\n1 2 3\n4 94 300.0 80.0 0 0 0 0 0 0 100 0\nWater Lapping 1 729\n";
        let a = parse_ambient(text);
        assert_eq!(a.len(), 2);
        assert_eq!((a[0].mode, a[0].sound, a[0].wait, a[0].volume), (0, 338, (40.0, 108.0), 88.0));
        assert!(a[1].steady() && !a[0].steady());
        assert_eq!(a[1].gain(50.0), EFFECT_LEVEL);
        assert_eq!(a[1].gain(300.0), 0.0);
        assert_eq!(track_sound_number("95riverdistant1a_299.bnk"), Some(299));
        assert_eq!(countdown_entry(0), "sor-a-go.bnk");
        assert_eq!(countdown_entry(3), "sor-a-3.bnk");
    }

    #[test]
    fn doppler_raises_pitch_coming_and_lowers_going() {
        assert!((doppler(0.0) - 1.0).abs() < 1e-6);
        assert!(doppler(30.0) > 1.09 && doppler(30.0) < 1.1);
        assert!(doppler(-30.0) < 0.92 && doppler(-30.0) > 0.91);
        assert_eq!(doppler(500.0), doppler(60.0));
        assert!(doppler(60.0) < 1.22 && doppler(-60.0) > 0.85);
        // a car ahead driving at us, listener still: closing
        assert!((closing_speed([0.0, 10.0], [0.0, -20.0], [0.0, 0.0]) - 20.0).abs() < 1e-4);
        // the listener drives past a parked car: closing before, opening after
        assert!(closing_speed([0.0, 10.0], [0.0, 0.0], [0.0, 25.0]) > 0.0);
        assert!(closing_speed([0.0, -10.0], [0.0, 0.0], [0.0, 25.0]) < 0.0);
        // side by side at the same speed: no shift
        assert_eq!(closing_speed([3.0, 0.0], [0.0, 30.0], [0.0, 30.0]), 0.0);
        assert_eq!(closing_speed([0.0, 0.0], [5.0, 0.0], [0.0, 0.0]), 0.0);
    }

    #[test]
    fn rain_and_opponent_skid_levels() {
        assert_eq!(rain_gain(0.0, true), 0.0);
        assert!((rain_gain(1.0, true) - EFFECT_LEVEL).abs() < 1e-6);
        assert!((rain_gain(1.0, false) - 0.6 * EFFECT_LEVEL).abs() < 1e-6);
        assert_eq!(rain_gain(2.0, true), rain_gain(1.0, true));
        assert_eq!(opponent_skid_gain(0.0, 5.0), 0.0);
        // close by it is as loud as the player's own squeal; far off it fades as 1/distance
        assert_eq!(opponent_skid_gain(1.0, 1.0), skid_gains(1.0).0);
        let (a, b) = (opponent_skid_gain(1.0, 30.0), opponent_skid_gain(1.0, 62.0));
        assert!((a / b - 2.0).abs() < 0.01, "{a} {b}");
    }

    #[test]
    fn trace_logs_rain_skids_and_passes() {
        let mut t = SoundTrace::default();
        let me = |z: f32| TraceCar { pos: [0.0, z], vel: [0.0, 30.0], slide: 0.0, rpm_frac: 0.7 };
        let other = |z: f32, slide: f32| TraceCar { pos: [3.0, z], vel: [0.0, 20.0], slide, rpm_frac: 0.6 };
        // the player catches a slower car 30 m ahead, overtakes it while it skids, in rain
        let mut log = Vec::new();
        for tick in 0..200u32 {
            let z = tick as f32 * 30.0 / 64.0;
            let oz = 30.0 + tick as f32 * 20.0 / 64.0;
            let rain = if (50..150).contains(&tick) { 0.8 } else { 0.0 };
            let slide = if (60..90).contains(&tick) { 0.9 } else { 0.0 };
            log.extend(t.observe(tick, rain, true, &[me(z), other(oz, slide)]));
        }
        let has = |s: &str| log.iter().any(|l| l.contains(s));
        assert!(has("tick 50: rain starts"), "{log:#?}");
        assert!(has("tick 150: rain stops"), "{log:#?}");
        assert!(has("tick 60: car 1 skid starts"), "{log:#?}");
        assert!(has("tick 90: car 1 skid stops"), "{log:#?}");
        assert!(has("car 1 approaching") && has("car 1 passes, moving away"), "{log:#?}");
        let pass = log.iter().position(|l| l.contains("passes")).unwrap();
        assert!(log[pass].contains("Doppler x0.9"), "{}", log[pass]);
        assert!(log.iter().position(|l| l.contains("approaching")).unwrap() < pass);
        assert!(!has("car 0 skid"));
    }
}
