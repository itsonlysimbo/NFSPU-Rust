//! Plays the race's sounds (`nfspu_game::race_sound` has the rules, `docs/provenance/sound.md`
//! the evidence): the player's engine layers, the opponents' engines, tyres, road, wind, hits,
//! the countdown voice and the track's ambient sounds. Everything is decoded from the install when
//! the race starts; numbers go to `logs/race_sound.log`.
//!
//! Every sound is a looping (or one-shot) Bevy voice whose volume and speed are set each frame.
//! The player's own sounds play centred; opponents and ambient sounds are spatial voices kept
//! one metre from a listener that follows the camera, so they pan left/right while their volume
//! follows retail's distance rules instead of Bevy's.

use crate::flow::AppState;
use bevy::audio::{AudioSinkPlayback, PlaybackMode, SpatialAudioSink, SpatialListener, Volume};
use bevy::prelude::*;
use nfspu_game::race::RaceStatus;
use nfspu_game::race_sound::{self as rs, Ambient, Archive, EngineInput, EngineMix, Pcm, Wind};
use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// Sound levels, 0..1.
// TODO(options-audio): read `AudioSettings` (`audio_settings.rs`, the `claude/options-audio`
// branch: sfx / music / engine / speech) once it is merged; until then everything is at full.
#[derive(Clone, Copy)]
struct Levels {
    sfx: f32,
    engine: f32,
    speech: f32,
}

impl Default for Levels {
    fn default() -> Self {
        Levels { sfx: 1.0, engine: 1.0, speech: 1.0 }
    }
}

/// Overall scale on everything this module plays (a guess to leave headroom when several
/// full-volume layers overlap).
const MIX: f32 = 0.7;

pub struct RaceAudioPlugin;

impl Plugin for RaceAudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (start_sound, run_sound).chain().run_if(crate::flow::in_race_scene))
            .add_systems(OnExit(AppState::Race), |mut commands: Commands| commands.remove_resource::<RaceSound>());
    }
}

/// A voice this module drives.
#[derive(Component)]
struct Voice;

/// The listener that follows the camera.
#[derive(Component)]
struct Listener;

struct OpponentVoices {
    approach: Entity,
    away: Entity,
    ratio: (f32, f32),
}

struct AmbientVoice {
    sound: Ambient,
    /// The looping voice while within range.
    voice: Option<Entity>,
    /// Seconds until the next play (one-shot sounds).
    wait: f32,
}

#[derive(Resource)]
struct RaceSound {
    levels: Levels,
    listener: Entity,
    engine: Option<(EngineMix, Vec<Entity>)>,
    shift: Option<Handle<AudioSource>>,
    was_shifting: bool,
    opponents: Vec<Option<OpponentVoices>>,
    coll: Option<Vec<u8>>,
    coll_cache: HashMap<usize, Option<(Handle<AudioSource>, Pcm)>>,
    road: Option<Entity>,
    squeal: Option<Entity>,
    scrub: Option<Entity>,
    scrape: Option<Entity>,
    wind: Option<Entity>,
    wind_walk: Wind,
    speech: Option<Archive>,
    last_count: Option<u8>,
    was_held: bool,
    ambient: Vec<AmbientVoice>,
    track_sounds: Option<Archive>,
    track_cache: HashMap<usize, Option<(Handle<AudioSource>, Pcm)>>,
    /// Unspent frame time for the 64 Hz sound clock.
    carry: f32,
    /// Level of the last hit, decaying (a weaker hit right after a strong one is not played).
    recent_hit: f32,
    last_wall_hit: usize,
    rng: u32,
    log_timer: f32,
}

fn log_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("logs").join("race_sound.log")
}

fn sound_log(msg: &str) {
    crate::log_line(&format!("sound: {msg}"));
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(log_path()) {
        let _ = writeln!(f, "{msg}");
    }
}

fn game_dir() -> PathBuf {
    PathBuf::from(std::env::var("NFSPU_GAME_DIR").unwrap_or_default())
}

/// Catalogue sound names and `.sim` stems by simulation name (`Spec::name`, the `.sim` file's
/// own title), read from the catalogue and each car's `.sim`.
fn sounds_by_spec(dir: &Path, specs: &[String]) -> HashMap<String, (String, String)> {
    let mut out = HashMap::new();
    for c in nfspu_game::car_load::catalogue(dir) {
        if out.len() == specs.len() {
            break;
        }
        let Ok((sim, _)) = nfspu_game::car_load::read_sim(dir, &c.sim) else { continue };
        if specs.contains(&sim.name) && !out.contains_key(&sim.name) {
            out.insert(sim.name, (c.sound.clone(), c.sim.clone()));
        }
    }
    out
}

fn add_pcm(pcm: &Pcm, assets: &mut Assets<AudioSource>) -> Handle<AudioSource> {
    assets.add(AudioSource { bytes: pcm.wav().into() })
}

/// Spawns a silent looping voice.
fn spawn_loop(commands: &mut Commands, handle: Handle<AudioSource>, spatial: bool) -> Entity {
    let settings = PlaybackSettings { mode: PlaybackMode::Loop, volume: Volume::Linear(0.0), spatial, ..PlaybackSettings::LOOP };
    commands.spawn((AudioPlayer(handle), settings, Transform::default(), Voice, DespawnOnExit(AppState::Race))).id()
}

fn spawn_once(commands: &mut Commands, handle: Handle<AudioSource>, volume: f32, speed: f32, at: Option<Vec3>) {
    let settings = PlaybackSettings { volume: Volume::Linear(volume.max(0.0)), speed, spatial: at.is_some(), ..PlaybackSettings::DESPAWN };
    commands.spawn((AudioPlayer(handle), settings, Transform::from_translation(at.unwrap_or_default()), DespawnOnExit(AppState::Race)));
}

/// Decodes (once) sound `n` of a bank into a cache.
fn cached<'a>(cache: &'a mut HashMap<usize, Option<(Handle<AudioSource>, Pcm)>>, bank: Option<&[u8]>, n: usize, assets: &mut Assets<AudioSource>) -> Option<&'a (Handle<AudioSource>, Pcm)> {
    cache
        .entry(n)
        .or_insert_with(|| {
            let pcm = rs::decode(bank?, n)?;
            Some((add_pcm(&pcm, assets), pcm))
        })
        .as_ref()
}

#[allow(clippy::too_many_arguments)]
fn start_sound(
    mut commands: Commands,
    existing: Option<Res<RaceSound>>,
    status: Res<RaceStatus>,
    car: Option<Res<crate::CarRes>>,
    mut assets: ResMut<Assets<AudioSource>>,
) {
    if existing.is_some() || !status.active || status.car_specs.is_empty() {
        return;
    }
    let Some(car) = car else { return };
    let dir = game_dir();
    let _ = std::fs::create_dir_all(log_path().parent().unwrap());
    let _ = std::fs::remove_file(log_path());
    let t0 = std::time::Instant::now();
    let names = sounds_by_spec(&dir, &status.car_specs);
    let listener = commands.spawn((SpatialListener::new(0.6), Transform::default(), Listener, DespawnOnExit(AppState::Race))).id();

    // the player's engine
    let player_spec = &status.car_specs[0];
    let (sound, sim) = names.get(player_spec).cloned().unwrap_or_default();
    let mut engine = None;
    let mut shift = None;
    match rs::load_engine(&dir, &sound, &sim) {
        Ok(e) => {
            sound_log(&format!(
                "player engine: car '{player_spec}' -> sound '{sound}', archive {}.viv, {} layers, tuning {} ({:?})",
                e.name,
                e.layers.len(),
                if e.tuned { format!("{sim}.spl") } else { "defaults (no .spl)".into() },
                e.tuning
            ));
            for l in &e.layers {
                let span = l.volume.iter().position(|v| *v > 0).zip(l.volume.iter().rposition(|v| *v > 0));
                sound_log(&format!(
                    "  layer sound {:3} {} {}: {} samples at {} Hz ({:.2} s loop), key {} fine {} -> rate x{:.3}, audible steps {:?}",
                    l.sound,
                    if l.on_load { "load " } else { "coast" },
                    if l.engine_side() { "engine " } else { "exhaust" },
                    l.pcm.samples.len(),
                    l.pcm.rate,
                    l.pcm.seconds(),
                    l.pcm.key,
                    l.pcm.fine,
                    rs::key_ratio(l.pcm.key, l.pcm.fine),
                    span
                ));
            }
            sound_log("  rpm/redline -> curve step, pitch (x recorded at the base note):");
            for i in 0..=12 {
                let f = i as f32 / 10.0;
                sound_log(&format!("    {f:.1} -> step {:3}, pitch {:.3}", rs::engine_step(f, e.tuning.step_scale), rs::engine_pitch(f, e.tuning.pitch_scale)));
            }
            let voices = e.layers.iter().map(|l| spawn_loop(&mut commands, add_pcm(&l.pcm, &mut assets), false)).collect();
            shift = e.shift.as_ref().map(|p| add_pcm(p, &mut assets));
            engine = Some((EngineMix::new(&e), voices));
        }
        Err(err) => sound_log(&format!("player engine: car '{player_spec}': {err}; no engine sound")),
    }

    // opponents
    let opp_archive = rs::find_file(&rs::sounds_dir(&dir), "zzzwzzz.viv").and_then(|p| Archive::open(&p));
    let mut opponents = vec![None];
    for spec in status.car_specs.iter().skip(1) {
        let sound = names.get(spec).map(|n| n.0.clone()).unwrap_or_default();
        let o = opp_archive.as_ref().and_then(|a| rs::load_opponent(a, &sound));
        match &o {
            Some(o) => sound_log(&format!("opponent '{spec}' -> {}lden/{}ldex ({} + {} samples, keys {}/{})", o.name, o.name, o.approach.samples.len(), o.away.samples.len(), o.approach.key, o.away.key)),
            None => sound_log(&format!("opponent '{spec}': no engine sound (zzzwzzz.viv missing?)")),
        }
        opponents.push(o.map(|o| OpponentVoices {
            approach: spawn_loop(&mut commands, add_pcm(&o.approach, &mut assets), true),
            away: spawn_loop(&mut commands, add_pcm(&o.away, &mut assets), true),
            ratio: (rs::key_ratio(o.approach.key, o.approach.fine), rs::key_ratio(o.away.key, o.away.fine)),
        }));
    }

    // tyres, road, wind
    let coll = rs::find_file(&rs::sounds_dir(&dir), "SND_coll.bnk").and_then(|p| std::fs::read(p).ok());
    let mut coll_cache = HashMap::new();
    let mut coll_loop = |n: usize, label: &str, commands: &mut Commands| {
        let c = cached(&mut coll_cache, coll.as_deref(), n, &mut assets);
        sound_log(&format!("SND_coll sound {n:#x} ({label}): {}", c.map_or("missing".to_string(), |(_, p)| format!("{} samples at {} Hz, loop {}", p.samples.len(), p.rate, p.looped))));
        c.map(|(h, _)| spawn_loop(commands, h.clone(), false))
    };
    let road = coll_loop(rs::coll::ROAD, "road", &mut commands);
    let squeal = coll_loop(rs::coll::SQUEAL, "skid squeal", &mut commands);
    let scrub = coll_loop(rs::coll::SCRUB, "skid scrub", &mut commands);
    let scrape = coll_loop(rs::coll::SCRAPE, "wall scrape", &mut commands);
    let wind = coll_loop(rs::coll::WIND, "wind", &mut commands);

    // countdown voice, track ambience
    let speech = rs::speech_archive(&dir);
    sound_log(&format!("speech archive: {}", speech.as_ref().map_or("missing".to_string(), |a| format!("{} entries", a.entries.len()))));
    let ambient: Vec<Ambient> = rs::ambient_file(&dir, &status.track).and_then(|p| std::fs::read_to_string(p).ok()).map(|t| rs::parse_ambient(&t)).unwrap_or_default();
    let track_sounds = rs::find_file(&rs::sounds_dir(&dir), "track.viv").and_then(|p| Archive::open(&p));
    let steady = ambient.iter().filter(|a| a.steady()).count();
    sound_log(&format!(
        "track '{}': {} ambient sounds ({steady} steady loops, {} repeating), track.viv {} entries",
        status.track,
        ambient.len(),
        ambient.len() - steady,
        track_sounds.as_ref().map_or(0, |a| a.entries.len())
    ));
    let mut rng = 0x9e37_79b9u32 ^ status.tick;
    let ambient = ambient
        .into_iter()
        .map(|a| {
            rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let wait = a.wait.0 * (rng >> 8) as f32 / (1 << 24) as f32;
            AmbientVoice { sound: a, voice: None, wait }
        })
        .collect();
    sound_log(&format!("sound set up in {:.0} ms (player car at {:?})", t0.elapsed().as_secs_f64() * 1000.0, car.car.pos));

    commands.insert_resource(RaceSound {
        levels: Levels::default(),
        listener,
        engine,
        shift,
        was_shifting: false,
        opponents,
        coll,
        coll_cache,
        road,
        squeal,
        scrub,
        scrape,
        wind,
        wind_walk: Wind::default(),
        speech,
        last_count: None,
        was_held: true,
        ambient,
        track_sounds,
        track_cache: HashMap::new(),
        carry: 0.0,
        recent_hit: 0.0,
        last_wall_hit: 0,
        rng,
        log_timer: 0.0,
    });
}

/// Sets a voice's volume and speed (spatial or not); the sink appears a frame after the spawn.
fn set_voice(e: Option<Entity>, volume: f32, speed: f32, sinks: &mut Query<&mut AudioSink>, spatial: &mut Query<&mut SpatialAudioSink>) {
    let Some(e) = e else { return };
    let volume = Volume::Linear(volume.clamp(0.0, 2.0));
    let speed = speed.clamp(0.05, 8.0);
    if let Ok(mut s) = sinks.get_mut(e) {
        s.set_volume(volume);
        s.set_speed(speed);
    } else if let Ok(mut s) = spatial.get_mut(e) {
        s.set_volume(volume);
        s.set_speed(speed);
    }
}

#[allow(clippy::too_many_arguments)]
fn run_sound(
    mut commands: Commands,
    time: Res<Time>,
    sound: Option<ResMut<RaceSound>>,
    status: Res<RaceStatus>,
    car: Option<Res<crate::CarRes>>,
    cam_mode: Option<Res<crate::CamMode>>,
    cams: Query<&GlobalTransform, With<nfspu_game::weather_fx::WorldCamera>>,
    mut transforms: Query<&mut Transform, With<Voice>>,
    mut listener: Query<&mut Transform, (With<Listener>, Without<Voice>)>,
    mut sinks: Query<&mut AudioSink>,
    mut spatial: Query<&mut SpatialAudioSink>,
    mut assets: ResMut<Assets<AudioSource>>,
) {
    let (Some(mut sound), Some(car)) = (sound, car) else { return };
    let s = &mut *sound;
    let dt = time.delta_secs();
    let cam = cams.iter().next().map(|g| g.compute_transform()).unwrap_or_default();
    if let Ok(mut t) = listener.get_mut(s.listener) {
        *t = cam;
    }
    let near = cam_mode.is_some_and(|m| m.0 == 1);
    let c = &car.car;
    let shifting = c.sim.drive.shift_left > 0;
    let input = EngineInput {
        rpm_frac: c.rpm() / c.sim.spec.redline.max(1.0),
        throttle: c.sim.throttle(),
        shifting,
        near,
        distance: if near { 0.0 } else { cam.translation.distance(c.pos) },
        speed: c.speed,
    };

    // the 64 Hz sound clock
    s.carry += dt.min(0.25);
    let step = 1.0 / rs::TICK_HZ;
    while s.carry >= step {
        s.carry -= step;
        if let Some((mix, _)) = &mut s.engine {
            mix.tick(input);
        }
        s.wind_walk.tick();
        s.recent_hit = (s.recent_hit - 1.0 / 16.0).max(0.0);
    }
    let lv = s.levels;
    if let Some((mix, voices)) = &s.engine {
        for (i, v) in voices.iter().enumerate() {
            set_voice(Some(*v), mix.gain(i) * lv.engine * MIX, mix.speed(i), &mut sinks, &mut spatial);
        }
    }
    // the gear-change clunk at the end of a change
    if s.was_shifting && !shifting {
        if let Some(h) = &s.shift {
            spawn_once(&mut commands, h.clone(), 0.5 * lv.engine * MIX, 1.0, None);
        }
    }
    s.was_shifting = shifting;

    // tyres, road, wind, scrape
    let slide = if c.on_ground { c.sim.wheels.iter().map(|w| if w.locked { 1.0 } else { w.slide }).fold(0.0f32, f32::max) } else { 0.0 };
    let (squeal, scrub) = rs::skid_gains(if c.speed.abs() > 2.0 { slide } else { 0.0 });
    let fx = lv.sfx * MIX;
    set_voice(s.squeal, squeal * fx, 1.0, &mut sinks, &mut spatial);
    set_voice(s.scrub, scrub * fx, 1.0, &mut sinks, &mut spatial);
    set_voice(s.road, if c.on_ground { rs::road_gain(c.speed) * fx } else { 0.0 }, 1.0, &mut sinks, &mut spatial);
    set_voice(s.scrape, if c.hit_wall && c.speed.abs() > 2.0 { rs::EFFECT_LEVEL * (c.speed.abs() / 20.0).min(1.0) * fx } else { 0.0 }, 1.0, &mut sinks, &mut spatial);
    let wind = s.wind_walk.gain(c.speed) * if near { 1.0 } else { 0.5 };
    set_voice(s.wind, wind * fx, s.wind_walk.speed(), &mut sinks, &mut spatial);

    // hits: walls (m/s into the wall) and cars (collide_cars units, 80 = a big hit)
    let mut hit = |n: usize, impact: f32, s: &mut RaceSound, commands: &mut Commands| {
        let g = rs::hit_gain(impact);
        if g * 127.0 / 16.0 <= s.recent_hit {
            return;
        }
        s.recent_hit = g * 127.0 / 16.0;
        if let Some((h, _)) = cached(&mut s.coll_cache, s.coll.as_deref(), n, &mut assets) {
            spawn_once(commands, h.clone(), g * fx, 1.0, None);
            sound_log(&format!("tick {}: hit sound {n:#x}, impact {impact:.1}, volume {g:.2}", status.tick));
        }
    };
    if c.impact > 1.0 {
        s.rng = s.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let mut k = (s.rng >> 16) as usize % 3;
        if k == s.last_wall_hit {
            k = (k + 1) % 3;
        }
        s.last_wall_hit = k;
        hit(rs::coll::WALL_HIT + k, c.impact, s, &mut commands);
    }
    if status.player_bump >= 20.0 {
        hit(rs::coll::CAR_HIT, status.player_bump / 8.0, s, &mut commands);
    }

    // opponents: kept one metre from the listener in their direction, volume by distance
    let listen = cam.translation;
    for (i, o) in s.opponents.iter().enumerate() {
        let (Some(o), Some(p)) = (o, status.car_places.get(i)) else { continue };
        let pos = Vec3::new(p[0], c.pos.y, p[1]);
        let rel = pos - listen;
        let dist = rel.length();
        let dir = rel.normalize_or(Vec3::NEG_Z);
        let vel = Vec3::new(-p[2].sin(), 0.0, -p[2].cos()) * p[3];
        let receding = if p[3].abs() > 0.5 { dir.dot(vel) / p[3].abs() } else { 0.0 };
        let rpm = status.car_rpm.get(i).copied().unwrap_or(0.2);
        let (front, back, pitch) = rs::opponent_engine(rpm, dist, receding);
        for (e, g, r) in [(o.approach, front, o.ratio.0), (o.away, back, o.ratio.1)] {
            if let Ok(mut t) = transforms.get_mut(e) {
                t.translation = listen + dir;
            }
            set_voice(Some(e), g * lv.engine * MIX, pitch * r, &mut sinks, &mut spatial);
        }
    }

    // countdown voice: 3, 2, 1 as the numbers show, "go" when the cars are let go
    let count = status.countdown.filter(|n| *n > 0);
    let go = s.was_held && !status.held && status.active;
    s.was_held = status.held;
    let say = if go { Some(0) } else if count.is_some() && count != s.last_count { count } else { None };
    s.last_count = count;
    if let (Some(n), Some(speech)) = (say, &s.speech) {
        let entry = rs::countdown_entry(n);
        match speech.get(&entry).and_then(|b| rs::decode(b, 0)) {
            Some(pcm) => {
                spawn_once(&mut commands, add_pcm(&pcm, &mut assets), rs::EFFECT_LEVEL * lv.speech * MIX, 1.0, None);
                sound_log(&format!("tick {}: countdown voice {entry} ({} samples at {} Hz)", status.tick, pcm.samples.len(), pcm.rate));
            }
            None => sound_log(&format!("tick {}: countdown voice {entry} not found", status.tick)),
        }
    }

    // track ambience: loops within their outer radius, repeating sounds after their wait
    let mirror = nfspu_game::world_mirror_x();
    for a in &mut s.ambient {
        let pos = Vec3::new(a.sound.pos[0] * mirror, a.sound.pos[1], a.sound.pos[2]);
        let rel = pos - listen;
        let dist = rel.length();
        let g = a.sound.gain(dist) * fx;
        if a.sound.steady() {
            if g > 0.0 && a.voice.is_none() {
                if let Some((h, _)) = cached(&mut s.track_cache, track_entry(&s.track_sounds, a.sound.sound), a.sound.sound, &mut assets) {
                    a.voice = Some(spawn_loop(&mut commands, h.clone(), true));
                    sound_log(&format!("tick {}: ambient '{}' (sound {}) starts at {dist:.0} m", status.tick, a.sound.name, a.sound.sound));
                }
            } else if g <= 0.0 {
                if let Some(e) = a.voice.take() {
                    commands.entity(e).despawn();
                }
            }
            if let Some(e) = a.voice {
                if let Ok(mut t) = transforms.get_mut(e) {
                    t.translation = listen + rel.normalize_or(Vec3::NEG_Z);
                }
                set_voice(Some(e), g, 1.0, &mut sinks, &mut spatial);
            }
        } else {
            a.wait -= dt;
            if a.wait <= 0.0 {
                s.rng = s.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let (lo, hi) = a.sound.wait;
                a.wait = lo + (hi - lo).max(0.0) * (s.rng >> 8) as f32 / (1 << 24) as f32;
                if g > 0.0 {
                    if let Some((h, _)) = cached(&mut s.track_cache, track_entry(&s.track_sounds, a.sound.sound), a.sound.sound, &mut assets) {
                        spawn_once(&mut commands, h.clone(), g, 1.0, Some(listen + rel.normalize_or(Vec3::NEG_Z)));
                        sound_log(&format!("tick {}: ambient '{}' (sound {}) plays at {dist:.0} m, volume {g:.2}", status.tick, a.sound.name, a.sound.sound));
                    }
                }
            }
        }
    }

    // a line every 5 s for checking the mix
    s.log_timer += dt;
    if s.log_timer >= 5.0 {
        s.log_timer = 0.0;
        if let Some((mix, _)) = &s.engine {
            let gains: Vec<String> = (0..mix.layers()).map(|i| format!("{:.2}", mix.gain(i))).collect();
            sound_log(&format!(
                "tick {}: rpm {:.0} ({:.2} of redline), throttle {:.2}, load {:.0}, step {}, pitch {:.3}, {}, layer volumes [{}], skid {slide:.2}, speed {:.1} m/s",
                status.tick,
                c.rpm(),
                input.rpm_frac,
                input.throttle,
                mix.load(),
                mix.step(),
                mix.pitch(),
                if near { "cockpit" } else { "outside" },
                gains.join(" "),
                c.speed
            ));
        }
    }
}

/// The `track.viv` bank holding ambient sound `n` (its entry name ends `_<n>.bnk`).
fn track_entry(archive: &Option<Archive>, n: usize) -> Option<&[u8]> {
    let a = archive.as_ref()?;
    let e = a.entries.iter().find(|e| rs::track_sound_number(&e.name) == Some(n))?;
    a.get(&e.name)
}
