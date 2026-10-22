//! Race mode for the driving viewer:
//! `NFSPU_RACE=<track>[,<forward|backward>[,<laps>[,<opponents>[,<difficulty>]]]]` puts the
//! player's car on pole of the retail start grid with the opponents behind, runs the countdown
//! (cars held until GO), drives the opponents (`nfspu_game::ai`), counts laps for every car,
//! finishes the race and then shows a placeholder results screen.
//! `NFSPU_OPPONENTS=<car>,...` names each opponent's car by catalogue id (`nfs5.car`) or model
//! stem (default: the player's car); `NFSPU_AI_SEED` fixes the random draws. Opponents drive the
//! retail car physics of their car's `.sim` and hit each other and the player (`collide_cars`).
//!
//! Hook API (for the coordinator, HUD and loading screen):
//! - `RacePlugin`: always inserts `nfspu_game::race::RaceStatus` (default = inactive) and
//!   `RaceHold`; while a `RaceRequest` resource exists it starts the race once `setup` has built
//!   the scene (`flow::RaceConfig` puts the request in place).
//! - `RaceRequest::from_env()` parses `NFSPU_RACE`; the menu builds one from its choices.
//! - `RaceHold(true)` while the cars must stay still; `drive_car` ignores input then.
//! - Events go to `logs/race.log`.

use bevy::prelude::*;
use nfspu_game::ai::{self, Opponent, Other, Rng, View};
use nfspu_game::car::{self as car_mod, Car};
use nfspu_game::car_load::{self, Pick};
use nfspu_game::car_model::CarOptions;
use nfspu_game::race::{self, CarSample, Kind, Race, RaceEvent, RaceSetup, RaceStatus};
use nfspu_game::race_path::{self, crosses, grid_spot, Direction, TrackRace};
use crate::flow::AppState;
use std::io::Write as _;
use std::path::PathBuf;

/// Which race to run.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct RaceRequest {
    pub track: String,
    pub direction: Direction,
    /// Mirrored track layout (`Track1.Direction` "…/mirrored"): the world's x factor flips.
    pub mirrored: bool,
    /// Laps for circuits (point-to-point roads always run once).
    pub laps: u32,
    /// Cars on the grid, the player included (1 + opponents, at most 8).
    pub cars: usize,
    /// 0 beginner, 1 advanced, 2 expert: which drivers race and how the lead slowdown works.
    pub difficulty: u8,
    /// Each opponent's car: a catalogue id or a model stem; empty entries (or a shorter list)
    /// use the player's car.
    pub opponent_cars: Vec<String>,
}

impl RaceRequest {
    /// Parses `<track>[,<forward|backward>[_mirrored][,<laps>[,<opponents>[,<difficulty>]]]]`.
    /// `mirror` anywhere in the direction token (`forward_mirrored`, `bkw/mir`) mirrors the track.
    pub fn parse(s: &str) -> Option<Self> {
        let mut parts = s.split(',').map(str::trim);
        let track = parts.next().filter(|t| !t.is_empty())?.to_ascii_lowercase();
        let dir = parts.next().map(str::to_ascii_lowercase).unwrap_or_default();
        let backward = dir.starts_with('b') || dir == "reverse";
        let mirrored = dir.contains("mirror") || dir.ends_with("mir");
        let direction = if backward { Direction::Backward } else { Direction::Forward };
        let laps = parts.next().and_then(|l| l.parse().ok()).filter(|&l| l > 0).unwrap_or(2);
        let opponents: usize = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0).min(7);
        let difficulty = parts.next().and_then(|d| d.parse().ok()).unwrap_or(0u8).min(2);
        Some(RaceRequest { track, direction, mirrored, laps, cars: 1 + opponents, difficulty, opponent_cars: Vec::new() })
    }

    pub fn from_env() -> Option<Self> {
        let mut r = std::env::var("NFSPU_RACE").ok().and_then(|s| Self::parse(&s))?;
        if let Ok(list) = std::env::var("NFSPU_OPPONENTS") {
            r.opponent_cars = list.split(',').map(|m| m.trim().to_string()).collect();
        }
        Some(r)
    }
}

/// True while the cars must stay on the grid.
#[derive(Resource, Default)]
pub struct RaceHold(pub bool);

/// The body entity of an opponent's car; its wheels, lamps and driver read their state from here.
#[derive(Component, Clone, Copy, Default)]
pub struct AiBody {
    pub speed: f32,
    pub steer: f32,
    pub braking: bool,
}

/// One opponent: its car, its driver and the entity that shows it.
struct Racer {
    car: Car,
    brain: Opponent,
    body: Entity,
    name: String,
    steer: f32,
    braking: bool,
    resets: u32,
}

#[derive(Resource)]
struct RaceRun {
    race: Race,
    track: TrackRace,
    player: usize,
    /// Opponents; racer `i` is race car `i + 1` (the player is car 0, on pole).
    racers: Vec<Racer>,
    line: Vec<Vec3>,
    rng: Rng,
    difficulty: u8,
    /// The player's progress when the race started (for the lead slowdown's lap rate).
    player_start: f32,
    slice: usize,
    last_pos: Vec3,
    /// Unspent frame time, run off in 64 Hz ticks.
    carry: f32,
    wrong_way: bool,
    finished_for: f32,
    results_shown: bool,
}

pub struct RacePlugin;

impl Plugin for RacePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(RaceStatus::default()).insert_resource(RaceHold(false));
        // The race starts on the first frame the car exists, once the loading screen (or the
        // direct start) has run `setup`.
        app.add_systems(
            Update,
            (start_race, race_tick, place_opponents, results_screen, leave_results)
                .chain()
                .after(crate::drive_car)
                .run_if(crate::flow::in_race_scene)
                .run_if(resource_exists::<RaceRequest>),
        )
        .add_systems(OnExit(AppState::Race), end_race);
    }
}

/// Forgets the race that was run (its opponents' bodies and the results panel go with the race
/// scene, `DespawnOnExit`).
fn end_race(mut commands: Commands) {
    commands.remove_resource::<RaceRun>();
    commands.remove_resource::<RaceTried>();
}

/// The race was set up (or failed to) for this scene; it is not tried again every frame.
#[derive(Resource)]
struct RaceTried;

fn race_log_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("logs").join("race.log")
}

fn race_log(msg: &str) {
    crate::log_line(&format!("race: {msg}"));
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(race_log_path()) {
        let _ = writeln!(f, "{msg}");
    }
}

fn game_dir() -> PathBuf {
    PathBuf::from(std::env::var("NFSPU_GAME_DIR").unwrap_or_default())
}

#[allow(clippy::too_many_arguments)]
fn start_race(
    mut commands: Commands,
    tried: Option<Res<RaceTried>>,
    req: Res<RaceRequest>,
    car: Option<ResMut<crate::CarRes>>,
    car_name: Res<crate::CarName>,
    player_paint: Res<crate::CarPaint>,
    mut status: ResMut<RaceStatus>,
    mut hold: ResMut<RaceHold>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    if tried.is_some() || car.is_none() {
        return;
    }
    commands.insert_resource(RaceTried);
    let _ = std::fs::create_dir_all(race_log_path().parent().unwrap());
    let _ = std::fs::remove_file(race_log_path());
    race_log(&format!("request: {:?}", *req));
    let Some(mut car) = car else {
        race_log("no car (the track did not load), race not started");
        return;
    };
    let track = match race_path::load(&req.track, req.direction) {
        Ok(t) => t,
        Err(e) => {
            race_log(&format!("race data failed to load: {e}"));
            return;
        }
    };
    let p = &track.path;
    let kind = if track.point_to_point() { Kind::PointToPoint } else { Kind::Circuit };
    let cars = req.cars.clamp(1, 8);
    // The player starts on pole: the reference let's play (10:32, an Evolution race) shows the
    // player 1st of 8 with all seven opponents 18-114 m behind just after the start.
    let player = 0;
    let spots: Vec<_> = (0..cars).map(|g| grid_spot(p, track.start_slice, g, cars, req.direction)).collect();
    let spot = spots[player];
    let start = spot.pos + Vec3::Y * 1.5;
    car.car = car.car.reset(start, spot.yaw);
    car.start_pos = start;
    car.start_yaw = spot.yaw;
    let fractions: Vec<f32> = spots.iter().map(|s| p.fraction(s.slice, req.direction)).collect();
    let race = Race::new(RaceSetup { kind, laps: req.laps, cars }, &fractions);
    race_log(&format!(
        "{} {:?}: {:?}, {} lap(s), {} slices in {} runs, lap {:.0} m, start slice {}, finish line {}",
        req.track,
        req.direction,
        kind,
        race.setup.laps,
        p.slices.len(),
        p.segments.len(),
        track.lap_metres(),
        track.start_slice,
        if track.finish.is_some() { "yes" } else { "no" }
    ));

    // opponents: drivers of the chosen difficulty, each in its car, on grid spots 1..
    let seed = std::env::var("NFSPU_AI_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.subsec_nanos()));
    let mut rng = Rng::new(seed);
    let game = game_dir();
    let lsp = race_path::load_racing_line(&req.track, req.direction);
    let line = ai::driving_line(p, lsp.as_deref());
    race_log(&format!(
        "racing line: {}",
        match &lsp {
            Some(l) if l.len() == p.slices.len() => format!("{} points from the .lsp", l.len()),
            Some(l) => format!(".lsp has {} points for {} slices, using slice centres", l.len(), p.slices.len()),
            None => "no .lsp, using slice centres".to_string(),
        }
    ));
    let (drivers, names) = ai::load_drivers(&game);
    let catalogue = car_load::catalogue(&game);
    let picked = ai::pick_drivers(&drivers, req.difficulty, cars - 1, &mut rng);
    race_log(&format!("opponents: {}, difficulty {}, seed {seed}, {} drivers in nfs5.per", cars - 1, req.difficulty, drivers.len()));
    let mut racers = Vec::new();
    // body colours by race index, for the HUD map and opponent list; the player's from its paint
    let mut colours = vec![nfspu_game::catalog::load_paints(&game, &car_name.0).get(player_paint.0).map_or([128; 3], |p| p.body)];
    for (i, driver) in picked.into_iter().enumerate() {
        // the car: catalogue record (model, AI name) and its .sim physics
        let entry = req.opponent_cars.get(i).filter(|m| !m.is_empty()).cloned().unwrap_or_else(|| car_name.0.clone());
        let id = entry.parse::<u16>().ok();
        let record = catalogue.iter().find(|c| Some(c.id) == id || (id.is_none() && c.kind == 0 && c.model.eq_ignore_ascii_case(&entry))).cloned();
        let Some(record) = record else {
            race_log(&format!("opponent {}: no catalogue car '{entry}', left out", i + 1));
            continue;
        };
        let model = record.model.clone();
        let loaded = match car_load::load(&game, Pick::Id(record.id), false) {
            Ok(l) => l,
            Err(e) => {
                race_log(&format!("opponent {}: '{model}' has no car spec ({e}), left out", i + 1));
                continue;
            }
        };
        let Some((spec, ais)) = ai::load_ais(&game, &record.ai_name) else {
            race_log(&format!("opponent {}: no AICarData for '{}', left out", i + 1, record.ai_name));
            continue;
        };
        let spot = spots[1 + racers.len()];
        let start = spot.pos + Vec3::Y * 1.5;
        // Each opponent in its catalogue body style and a random paint (`.clr` section number).
        let paints = nfspu_game::catalog::load_paints(&game, &model);
        let paint = if paints.is_empty() { CarOptions::default().paint } else { paints[rng.below(paints.len())].section };
        let options = CarOptions { paint, style: record.style, ..Default::default() };
        let body = commands.spawn((Transform::from_translation(start).with_rotation(Quat::from_rotation_y(spot.yaw)), Visibility::default(), AiBody::default(), DespawnOnExit(AppState::Race))).id();
        crate::spawn_car_model(&mut commands, &mut meshes, &mut materials, &mut images, body, &model, &options);
        let name = names.get(&(driver.name_id as u16)).cloned().unwrap_or_else(|| format!("Driver {}", driver.id));
        race_log(&format!(
            "car {}: {name} (driver {}, difficulty {}, skills {:?}) in {} '{model}' style {} paint {}, {ais}.ais (top table speed {:.1} m/s); {}",
            racers.len() + 1,
            driver.id,
            driver.difficulty,
            driver.skills,
            record.name,
            options.style,
            options.paint,
            spec.speeds[0],
            loaded.note
        ));
        colours.push(nfspu_game::catalog::load_paints(&game, &model).get(options.paint).map_or([128; 3], |p| p.body));
        racers.push(Racer { car: Car::with_spec(start, spot.yaw, loaded.spec, Default::default()), brain: Opponent::new(spec, driver, spot.slice), body, name, steer: 0.0, braking: false, resets: 0 });
    }
    // opponents that could not be set up drop out of the race
    let cars = 1 + racers.len();
    let race = if cars == race.cars.len() { race } else { Race::new(RaceSetup { kind, laps: req.laps, cars }, &fractions[..cars]) };
    for (g, s) in spots.iter().take(cars).enumerate() {
        race_log(&format!("grid {g}: slice {} at ({:.1}, {:.1}, {:.1}) yaw {:.3}{}", s.slice, s.pos.x, s.pos.y, s.pos.z, s.yaw, if g == player { " (player)" } else { "" }));
    }
    *status = RaceStatus { track: req.track.clone(), backwards: req.direction == Direction::Backward, ..Default::default() };
    status.names = std::iter::once("You".to_string()).chain(racers.iter().map(|r| r.name.clone())).collect();
    status.car_colours = colours;
    status.lap_metres = track.lap_metres();
    status.road = std::sync::Arc::new(road_lines(&track.path));
    race.fill_status(player, &mut status);
    hold.0 = true;
    let player_start = race.cars[player].progress;
    commands.insert_resource(RaceRun {
        race,
        player,
        racers,
        line,
        rng,
        difficulty: req.difficulty,
        player_start,
        slice: spot.slice,
        last_pos: start,
        carry: 0.0,
        wrong_way: false,
        finished_for: 0.0,
        results_shown: false,
        track,
    });
}

/// One 64 Hz tick for every opponent: think, drive the car physics, follow it along the slices
/// and fill its race sample. Cars stuck for long or fallen off the world go back on the road.
fn drive_opponents(run: &mut RaceRun, player: &mut Car, dt: f32, ground: &nfspu_game::ground::Ground, walls: &nfspu_game::collision::WallSet, samples: &mut [CarSample]) -> f32 {
    let dir = run.track.dir;
    let held = run.race.held();
    let player_progress = run.race.cars[run.player].progress;
    let rate = ai::player_rate(player_progress - run.player_start, run.race.race_ticks());
    let others: Vec<Other> = std::iter::once(Other { pos: player.pos, speed: player.speed }).chain(run.racers.iter().map(|r| Other { pos: r.car.pos, speed: r.car.speed })).collect();
    for i in 0..run.racers.len() {
        let race_car = i + 1;
        let finished = run.race.cars[race_car].state == race::CarState::Finished;
        let lead = run.race.cars[race_car].progress - player_progress;
        let view = View { path: &run.track.path, routes: &run.track.routes, line: &run.line, dir, held, finished, difficulty: run.difficulty, lead, player_rate: rate };
        let near: Vec<Other> = others.iter().enumerate().filter(|(k, _)| *k != race_car).map(|(_, o)| *o).collect();
        let r = &mut run.racers[i];
        let input = r.brain.think(&r.car, &view, &near, &mut run.rng);
        r.car.sim.held = held; // on the grid the brake is held on until GO (retail)
        r.car.wet = player.wet;
        let before = r.car.pos;
        r.car.step(input, dt, ground, walls);
        r.steer = input.steer;
        r.braking = input.brake && r.car.speed > 0.5;
        let slice = run.track.path.track(r.brain.slice, r.car.pos);
        r.brain.slice = slice;
        let fallen = r.car.pos.y < run.track.path.slices[slice].pos.y - 30.0;
        if !held && !finished && (fallen || r.brain.wants_reset()) {
            let (pos, yaw) = ai::reset_pose(&run.track.path, &run.line, slice, dir);
            r.car = r.car.reset(pos, yaw);
            r.brain.was_reset(slice);
            r.resets += 1;
            race_log(&format!("tick {}: car {race_car} ({}) put back on the road at slice {slice} ({})", run.race.tick, r.name, if fallen { "fell" } else { "stuck" }));
        }
        let crossed = run.track.finish.is_some_and(|(a, b)| crosses(before, r.car.pos, a, b));
        samples[race_car] = CarSample { fraction: run.track.path.fraction(slice, dir), crossed_finish: crossed };
    }
    // car against car: every opponent with the player and with each other (retail's impulse)
    let tick = run.race.tick;
    let mut player_hit = 0.0f32;
    for i in 0..run.racers.len() {
        if let Some(hit) = car_mod::collide_cars(player, &mut run.racers[i].car) {
            player_hit = player_hit.max(hit);
            if hit >= 20.0 {
                race_log(&format!("tick {tick}: player hit {} (impact {hit:.0})", run.racers[i].name));
            }
        }
        for j in i + 1..run.racers.len() {
            let (a, b) = run.racers.split_at_mut(j);
            if let Some(hit) = car_mod::collide_cars(&mut a[i].car, &mut b[0].car) {
                if hit >= 20.0 {
                    race_log(&format!("tick {tick}: {} hit {} (impact {hit:.0})", a[i].name, b[0].name));
                }
            }
        }
    }
    player_hit
}

/// The road's centre line as one (x, z) polyline per road run, for the HUD map.
fn road_lines(path: &race_path::RoadPath) -> Vec<Vec<[f32; 2]>> {
    path.segments.iter().map(|seg| (seg.first..=seg.last).filter_map(|i| path.slices.get(i)).map(|s| [s.pos.x, s.pos.z]).collect()).collect()
}

/// Puts the opponents' bodies where their cars are and shares their speed with the wheels.
fn place_opponents(run: Option<Res<RaceRun>>, mut bodies: Query<(&mut Transform, &mut AiBody)>) {
    let Some(run) = run else { return };
    for r in &run.racers {
        if let Ok((mut t, mut b)) = bodies.get_mut(r.body) {
            t.translation = r.car.pos;
            let (pitch, roll) = r.car.body_tilt();
            t.rotation = Quat::from_euler(EulerRot::YXZ, r.car.yaw, pitch, roll);
            *b = AiBody { speed: r.car.speed, steer: r.steer, braking: r.braking };
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn race_tick(
    time: Res<Time>,
    run: Option<ResMut<RaceRun>>,
    car: Option<ResMut<crate::CarRes>>,
    ground: Option<Res<crate::GroundRes>>,
    walls: Option<Res<crate::WallRes>>,
    mut status: ResMut<RaceStatus>,
    mut hold: ResMut<RaceHold>,
) {
    let (Some(mut run), Some(mut car), Some(ground), Some(walls)) = (run, car, ground, walls) else { return };
    let run = &mut *run;
    let dt = time.delta_secs();
    let pos = car.car.pos;
    let dir = run.track.dir;
    run.slice = run.track.path.track(run.slice, pos);
    let mut crossed = run.track.finish.map_or(false, |(a, b)| crosses(run.last_pos, pos, a, b));
    let velocity = if dt > 0.0 { (pos - run.last_pos) / dt } else { Vec3::ZERO };
    run.last_pos = pos;
    let player_done = run.race.cars[run.player].state == race::CarState::Finished;
    let wrong = !run.race.held() && !player_done && run.track.path.wrong_way(run.slice, velocity, dir);
    if wrong != run.wrong_way {
        run.wrong_way = wrong;
        race_log(&format!("tick {}: wrong way {}", run.race.tick, if wrong { "ON" } else { "off" }));
    }
    // frame time runs off in whole 64 Hz ticks; long hitches are capped so a stall does not
    // fast-forward the race
    run.carry += dt.min(0.25);
    let step = 1.0 / race::TICK_HZ as f32;
    let fraction = run.track.path.fraction(run.slice, dir);
    let name = |names: &[String], car: usize| names.get(car).cloned().unwrap_or_else(|| format!("car {car}"));
    let mut bump = 0.0f32;
    while run.carry >= step {
        run.carry -= step;
        let mut samples = vec![CarSample::default(); run.race.cars.len()];
        samples[run.player] = CarSample { fraction, crossed_finish: crossed };
        crossed = false;
        bump = bump.max(drive_opponents(run, &mut car.car, step, &ground.0, &walls.0, &mut samples));
        for e in run.race.step(&samples) {
            let t = run.race.tick;
            match e {
                RaceEvent::Countdown(0) => race_log(&format!("tick {t}: GO")),
                RaceEvent::Countdown(n) => race_log(&format!("tick {t}: countdown {n}")),
                RaceEvent::Lap { car, lap, ticks } => race_log(&format!("tick {t}: car {car} ({}) lap {lap} in {}", name(&status.names, car), race::format_time(ticks))),
                RaceEvent::LapOwed { car } => race_log(&format!("tick {t}: car {car} ({}) crossed the line backwards, lap owed", name(&status.names, car))),
                RaceEvent::Finished { car, position, ticks } => race_log(&format!("tick {t}: car {car} ({}) FINISHED P{position} in {}", name(&status.names, car), race::format_time(ticks))),
            }
        }
        // every 10 s: the running order with each car's progress and speed (target for opponents)
        let t = run.race.tick;
        if t > race::GO_TICK && (t - race::GO_TICK) % (race::TICK_HZ * 10) == 0 {
            let mut line = format!("tick {t}: order");
            for (pos, c) in run.race.order().into_iter().enumerate() {
                let speed = match c.checked_sub(1).and_then(|i| run.racers.get(i)) {
                    Some(r) => format!("{:.0}/{:.0} m/s", r.car.speed, r.brain.target_speed),
                    None => format!("{:.0} m/s", car.car.speed),
                };
                line += &format!(" | P{} {} {:.3} {speed}", pos + 1, name(&status.names, c), run.race.cars[c].progress);
            }
            race_log(&line);
        }
    }
    if crossed {
        // the finish was crossed in a frame too short to hold a tick: keep it for the next one
        run.last_pos = pos - velocity * dt;
    }
    hold.0 = run.race.held();
    run.race.fill_status(run.player, &mut status);
    status.wrong_way = run.wrong_way;
    status.slice = run.slice;
    status.speed_kmh = car.car.speed.abs() * 3.6;
    status.rpm = Some(car.car.rpm());
    status.car_places = std::iter::once(&car.car).chain(run.racers.iter().map(|r| &r.car)).map(|c| [c.pos.x, c.pos.z, c.yaw, c.speed]).collect();
    let cars = || std::iter::once(&car.car).chain(run.racers.iter().map(|r| &r.car));
    status.car_rpm = cars().map(|c| c.rpm() / c.sim.spec.redline.max(1.0)).collect();
    if status.car_specs.len() != status.car_rpm.len() {
        status.car_specs = cars().map(|c| c.sim.spec.name.clone()).collect();
    }
    status.player_bump = bump;
    if status.finished {
        run.finished_for += dt;
    }
}

/// Marks the placeholder results panel.
#[derive(Component)]
struct ResultsPanel;

/// The standings text: every car's time, DNF while it is still racing.
fn results_text(status: &RaceStatus) -> String {
    let mut text = format!("RACE RESULTS\n{} ({})\n\n", status.track, if status.backwards { "backward" } else { "forward" });
    for r in &status.results {
        let who = status.names.get(r.car).cloned().unwrap_or_else(|| format!("Car {}", r.car + 1));
        let time = r.time.map_or("racing".to_string(), race::format_secs);
        let best = r.best_lap.map_or("-".to_string(), race::format_secs);
        text += &format!("{}.  {:<10} {}   best lap {}\n", r.position, who, time, best);
    }
    if !status.point_to_point {
        for (i, t) in status.lap_times.iter().enumerate() {
            text += &format!("\n   lap {}: {}", i + 1, race::format_secs(*t));
        }
    }
    text
}

fn log_results(text: &str) {
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        race_log(&format!("results | {}", line.trim()));
    }
}

/// After the player finishes (and 2 s pass, a guess at retail's pause), lists the standings. The
/// opponents keep racing behind it and their times fill in as they finish (ours; what retail
/// shows for cars still racing was not checked). Logged once now and again when all are in.
fn results_screen(mut commands: Commands, run: Option<ResMut<RaceRun>>, status: Res<RaceStatus>, mut panel: Query<&mut Text, With<ResultsPanel>>, mut logged_all: Local<bool>) {
    let Some(mut run) = run else { return };
    if !status.finished || run.finished_for < 2.0 {
        *logged_all = false;
        return;
    }
    let text = results_text(&status);
    let full = format!("{text}\n\nPress Enter to continue");
    if run.results_shown {
        if let Ok(mut t) = panel.single_mut() {
            if t.0 != full {
                t.0 = full;
            }
        }
        if !*logged_all && run.race.all_finished() {
            *logged_all = true;
            race_log("all cars finished");
            log_results(&text);
        }
        return;
    }
    run.results_shown = true;
    log_results(&text);
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: Val::Percent(20.0),
            left: Val::Percent(30.0),
            width: Val::Percent(40.0),
            padding: UiRect::all(Val::Px(16.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.75)),
        Text::new(full),
        TextFont { font_size: FontSize::Px(20.0), ..default() },
        TextColor(Color::srgb(1.0, 0.95, 0.8)),
        ResultsPanel,
        DespawnOnExit(AppState::Race),
    ));
}

/// Enter, Space or Escape on the results closes the race: back to the menu's quick-race screen,
/// or out of the program when the race was started without the menu.
fn leave_results(
    run: Option<Res<RaceRun>>,
    keys: Res<ButtonInput<KeyCode>>,
    back: Res<crate::flow::ReturnToMenu>,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
) {
    if run.is_some_and(|r| r.results_shown) && keys.any_just_pressed([KeyCode::Enter, KeyCode::Space, KeyCode::Escape]) {
        race_log("results closed, leaving the race");
        crate::flow::leave_race(&back, &mut next, &mut exit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_race_requests() {
        let r = RaceRequest::parse("Monaco1,backward,3").unwrap();
        assert_eq!((r.track.as_str(), r.direction, r.laps), ("monaco1", Direction::Backward, 3));
        assert!(!r.mirrored);
        let r = RaceRequest::parse("alps").unwrap();
        assert_eq!((r.direction, r.laps, r.cars, r.difficulty), (Direction::Forward, 2, 1, 0));
        let r = RaceRequest::parse("coastal,forward,1,7,2").unwrap();
        assert_eq!((r.cars, r.difficulty), (8, 2));
        assert_eq!(RaceRequest::parse("coastal,forward,1,12,9").unwrap().cars, 8, "at most 7 opponents");
        assert_eq!(RaceRequest::parse("skidpad, f, 0").unwrap().laps, 2);
        assert!(RaceRequest::parse("").is_none());
        let r = RaceRequest::parse("coastal,forward_mirrored,2").unwrap();
        assert_eq!((r.direction, r.mirrored, r.laps), (Direction::Forward, true, 2));
        let r = RaceRequest::parse("coastal,bkw/mir,2").unwrap();
        assert_eq!((r.direction, r.mirrored, r.laps), (Direction::Backward, true, 2));
    }
}
