//! Offline scripted race (no window): one car starts on the retail grid, waits for the countdown,
//! then an autopilot drives the road slices in race direction while `race::Race` counts laps.
//! Checks that the countdown holds the car, laps are counted and the race finishes.
//! With opponents, AI drivers (`nfspu_game::ai`) race on the grid behind it and every car's laps
//! and finish are logged.
//! Run: `cargo run -p nfspu-game --example race_drive --release -- monaco1 forward 2 7 2`
//! (track, forward|backward, laps, opponents 0-7, difficulty 0-2; with no track it runs every track
//! once, alone). `NFSPU_AI_SEED` fixes the random draws. Log: logs/race_drive.txt
//! `RACE_DRIVE_TRAFFIC=<0-3>` adds road traffic (none, light, medium, heavy; default 0).
//! `RACE_DRIVE_CAR=<catalogue id>` drives that retail car (default: the made-up `Spec::generic`);
//! opponents drive the 993's retail car.
//! `RACE_DRIVE_SOUND=1` also runs the race-sound rules (`nfspu_game::race_sound`) with the
//! listener on the player's car and writes every channel's triggers and volumes to
//! `logs/race_sound.log` (it also wets the road where the track's weather says so, as the game
//! does; `NFSPU_WEATHER=rain` forces rain).

use nfspu_game::ai::{self, Opponent, Other, Rng, View};
use nfspu_game::car::{Car, Input};
use nfspu_game::collision::WallSet;
use nfspu_game::ground::Ground;
use nfspu_game::race::{self, CarSample, Kind, Race, RaceEvent, RaceSetup};
use nfspu_game::race_path::{self, crosses, grid_spot, Direction};
use nfspu_game::track;
use nfspu_game::race_sound as rs;
use std::fmt::Write as _;

/// The race-sound rules run alongside the scripted race (`RACE_DRIVE_SOUND=1`).
struct SoundRun {
    weather: nfspu_game::weather::TrackWeather,
    engine: Option<rs::EngineMix>,
    wind: rs::Wind,
    rain_ratio: f32,
    triggers: rs::TriggerLog,
    last_distance: Vec<Option<f32>>,
    hits: u32,
    log: String,
}

impl SoundRun {
    fn new(game: &std::path::Path, track: &str, car: &Car, opponents: usize) -> SoundRun {
        let weather = nfspu_game::weather::TrackWeather::load(game, track, nfspu_game::weather::WeatherOverride::from_env(), nfspu_game::weather::TimeOverride::Track);
        let mut log = String::new();
        // the player's engine set by catalogue sound name, as the game picks it
        let entry = nfspu_game::car_load::catalogue(game).into_iter().find(|c| nfspu_game::car_load::read_sim(game, &c.sim).is_ok_and(|(s, _)| s.name == car.sim.spec.name));
        let engine = entry.as_ref().and_then(|c| rs::load_engine(game, &c.sound, &c.sim).ok());
        let _ = writeln!(log, "race_drive sound run: track {track}, wet road {}, peak rain {:.2}, player engine {}", weather.wet, weather.sky.as_ref().map_or(0.0, |s| s.peaks().0), engine.as_ref().map_or("none (made-up car)".to_string(), |e| format!("{} ({} layers)", e.name, e.layers.len())));
        let rain_ratio = rs::find_file(&rs::sounds_dir(game), "zzzwzzz.viv").and_then(|p| rs::Archive::open(&p)).and_then(|a| rs::decode(a.get("rain.bnk")?, 0)).map(|p| {
            let _ = writeln!(log, "rain.bnk: {} samples at {} Hz, key {}", p.samples.len(), p.rate, p.key);
            rs::key_ratio(p.key, p.fine)
        });
        SoundRun { weather, engine: engine.as_ref().map(rs::EngineMix::new), wind: rs::Wind::default(), rain_ratio: rain_ratio.unwrap_or(1.0), triggers: rs::TriggerLog::default(), last_distance: vec![None; opponents], hits: 0, log }
    }

    fn line(&mut self, l: String) {
        let _ = writeln!(self.log, "{l}");
    }

    /// One 64 Hz tick: the listener rides with the player's car (an outside view 5 m behind).
    fn tick(&mut self, tick: u32, car: &Car, others: &[&Car], slice: usize, slices: usize, dt: f32) {
        let now = self.weather.at(slice, slices);
        if let Some(m) = &mut self.engine {
            m.tick(rs::EngineInput { rpm_frac: car.rpm() / car.sim.spec.redline.max(1.0), throttle: car.sim.throttle(), shifting: car.sim.drive.shift_left > 0, near: false, distance: 5.0, speed: car.speed, side: 0.0 });
        }
        self.wind.tick();
        let input = rs::EffectInput { speed: car.speed, skid: car.skid(), on_ground: car.on_ground, road_wet: car.road_wet, hit_wall: car.hit_wall, rain: now.rain, covered: self.weather.wet && car.on_ground && !car.road_wet, near: false };
        for (name, g, _) in rs::effect_channels(&input, &self.wind, self.rain_ratio) {
            if let Some(l) = self.triggers.level(tick, name, g) {
                self.line(l);
            }
        }
        if car.impact > 1.0 {
            self.hits += 1;
            self.line(format!("tick {tick}: player wall hit, impact {:.1}, volume {:.2}", car.impact, rs::hit_gain(car.impact)));
        }
        for (i, o) in others.iter().enumerate() {
            let rel = o.pos - car.pos;
            let dist = rel.length();
            let dir = rel.normalize_or(bevy::math::Vec3::NEG_Z);
            let vel = bevy::math::Vec3::new(-o.yaw.sin(), 0.0, -o.yaw.cos()) * o.speed;
            let receding = if o.speed.abs() > 0.5 { dir.dot(vel) / o.speed.abs() } else { 0.0 };
            let separating = self.last_distance[i].map_or(0.0, |d| (dist - d) / dt);
            self.last_distance[i] = Some(dist);
            let m = rs::opponent_mix(o.rpm() / o.sim.spec.redline.max(1.0), dist, receding, separating, o.skid(), o.speed);
            for (name, g) in [("engine", m.approach.max(m.away)), ("skid squeal", m.squeal)] {
                if let Some(l) = self.triggers.level(tick, &format!("opponent {} {name}", i + 1), g) {
                    self.line(format!("{l} at {dist:.0} m, closing {:.1} m/s, doppler x{:.3}", -separating, m.doppler));
                }
            }
            if o.impact > 1.0 {
                self.hits += 1;
                self.line(format!("tick {tick}: opponent {} wall hit, impact {:.1}, {dist:.0} m, volume {:.2}", i + 1, o.impact, rs::opponent_hit(o.impact, dist)));
            }
            // a pass: the Doppler factor while the car is within 15 m and moving relative to us
            if dist < 15.0 && separating.abs() > 3.0 && tick % 16 == 0 {
                self.line(format!("tick {tick}: opponent {} passing at {dist:.1} m, separating {separating:.1} m/s, doppler x{:.3}, approach {:.2} away {:.2}", i + 1, m.doppler, m.approach, m.away));
            }
        }
        if tick % (64 * 5) == 0 {
            let layers = self.engine.as_ref().map_or(String::new(), |m| (0..m.layers()).map(|i| format!("{:.2}", m.gain(i))).collect::<Vec<_>>().join(" "));
            let fx: Vec<String> = rs::effect_channels(&input, &self.wind, self.rain_ratio).iter().map(|(n, g, _)| format!("{n} {g:.2}")).collect();
            self.line(format!("tick {tick}: speed {:.1} m/s, rain {:.2}{}, engine layers [{layers}], {}", car.speed, now.rain, if input.covered { " (covered)" } else { "" }, fx.join(", ")));
        }
    }

    fn event(&mut self, e: &RaceEvent, tick: u32, total_laps: u32, cars: usize) {
        let say = match *e {
            RaceEvent::Countdown(n) => Some(rs::countdown_entry(n)),
            RaceEvent::Lap { car: 0, lap, .. } => rs::lap_line(lap + 1, total_laps),
            RaceEvent::Finished { car: 0, position, .. } => rs::finish_line(position, cars),
            _ => None,
        };
        if let Some(entry) = say {
            self.line(format!("tick {tick}: voice {entry}"));
        }
    }
}

fn wrap(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

fn drive(name: &str, dir: Direction, laps: u32, opponents: usize, difficulty: u8, out: &mut String) -> bool {
    let t = match race_path::load(name, dir) {
        Ok(t) => t,
        Err(e) => { let _ = writeln!(out, "{name}: {e}"); return false; }
    };
    let g = track::load_ground(name).expect("ground");
    let ground = Ground::new(g.tris, 16.0);
    let walls = WallSet::new(g.walls, 4.0);
    let p = &t.path;
    let cars = 1 + opponents;
    let spots: Vec<_> = (0..cars).map(|g| grid_spot(p, t.start_slice, g, cars, dir)).collect();
    let spot = spots[0];
    // RACE_DRIVE_CAR=<catalogue id> drives a retail car's .sim physics instead of the made-up one
    let mut car = match std::env::var("RACE_DRIVE_CAR").ok().and_then(|v| v.parse::<u16>().ok()) {
        Some(id) => {
            let game = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").unwrap_or_default());
            let l = nfspu_game::car_load::load(&game, nfspu_game::car_load::Pick::Id(id), false).expect("car spec");
            let _ = writeln!(out, "   car: {}", l.note);
            Car::with_spec(spot.pos + bevy::math::Vec3::Y, spot.yaw, l.spec, Default::default())
        }
        None => Car::new(spot.pos + bevy::math::Vec3::Y, spot.yaw),
    };
    let kind = if t.point_to_point() { Kind::PointToPoint } else { Kind::Circuit };
    let mut slice = spot.slice;
    let fractions: Vec<f32> = spots.iter().map(|s| p.fraction(s.slice, dir)).collect();
    let mut r = Race::new(RaceSetup { kind, laps, cars }, &fractions);
    // opponents: retail drivers of the difficulty, all with the 993's tables
    let game = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").unwrap_or_default());
    let seed = std::env::var("NFSPU_AI_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(12345u32);
    let mut rng = Rng::new(seed);
    let lsp = race_path::load_racing_line(name, dir);
    let line = ai::driving_line(p, lsp.as_deref());
    let (drivers, names) = ai::load_drivers(&game);
    let (spec, _) = ai::load_ais(&game, "993").expect("993.ais");
    let opp_spec = nfspu_game::car_load::load(&game, nfspu_game::car_load::Pick::Model("993"), false).expect("993 car spec").spec;
    let mut racers: Vec<(Car, Opponent, String)> = ai::pick_drivers(&drivers, difficulty, opponents, &mut rng)
        .into_iter()
        .enumerate()
        .map(|(i, d)| {
            let s = spots[i + 1];
            let n = names.get(&(d.name_id as u16)).cloned().unwrap_or_default();
            (Car::with_spec(s.pos + bevy::math::Vec3::Y, s.yaw, opp_spec.clone(), Default::default()), Opponent::new(spec.clone(), d, s.slice), n)
        })
        .collect();
    // road traffic: the density from RACE_DRIVE_TRAFFIC, cars with the 993's physics
    let density = std::env::var("RACE_DRIVE_TRAFFIC").ok().and_then(|v| v.parse::<u8>().ok()).unwrap_or(0).min(3);
    let mut fleet = nfspu_game::traffic::Fleet::new(nfspu_game::traffic::cars_for_density(density), &Car::with_spec(bevy::math::Vec3::ZERO, 0.0, opp_spec.clone(), Default::default()), seed);
    let (mut traffic_spawns, mut traffic_removed, mut traffic_hits, mut traffic_honks, mut traffic_max) = (0u32, 0u32, 0u32, 0u32, 0usize);
    if density > 0 {
        let _ = writeln!(out, "   traffic density {density}: {} cars", fleet.cars.len());
    }
    let mut resets = vec![0u32; opponents];
    let mut taken = std::collections::BTreeMap::<(usize, usize), u32>::new();
    let mut choice_lines = 0u32;
    let mut hits = 0u32;
    let mut sound = std::env::var("RACE_DRIVE_SOUND").is_ok().then(|| SoundRun::new(&game, name, &car, opponents));
    if let Some(snd) = &sound {
        car.wet = snd.weather.wet;
        for r in &mut racers {
            r.0.wet = snd.weather.wet;
        }
    }
    let who = |c: usize, racers: &[(Car, Opponent, String)]| if c == 0 { "player".to_string() } else { racers[c - 1].2.clone() };
    if opponents > 0 {
        let _ = writeln!(out, "   {opponents} opponents, difficulty {difficulty}, seed {seed}, racing line {}", if lsp.as_ref().is_some_and(|l| l.len() == p.slices.len()) { "from .lsp" } else { "slice centres" });
    }
    let _ = writeln!(out, "{name} {dir:?}: {kind:?}, {} laps, start slice {}, grid slice {}, start fraction {:.3}", r.setup.laps, t.start_slice, slice, p.fraction(slice, dir));
    let dt = 1.0 / race::TICK_HZ as f32;
    let start_pos = car.pos;
    let mut route = slice;
    let mut route_target = p.slices[route].pos;
    let debug = std::env::var("RACE_DRIVE_DEBUG").is_ok();
    let mut moved_before_go = 0.0f32;
    // spin-outs: the slip angle (velocity against heading) passing 35 degrees above 10 m/s
    let (mut spins, mut spinning) = (0u32, false);
    let (mut wrong_ticks, mut stuck, mut backing, mut best_progress, mut last_gain) = (0u32, false, 0.0f32, 0.0f32, 0u32);
    while !r.all_finished() {
        let before = car.pos;
        let input = if r.held() {
            Input::default()
        } else {
            // the autopilot follows its own route along the main road (waypoints), separate from
            // the race's slice tracking
            while route_target.distance(car.pos) < 14.0 {
                let next = p.walk(route, 2, dir);
                if next == route { break; }
                route = next;
                route_target = p.slices[route].pos;
            }
            let to = route_target - car.pos;
            let diff = wrap((-to.x).atan2(-to.z) - car.yaw);
            let want = if diff.abs() > 0.5 { 10.0 } else { 22.0 };
            // with the retail model the driver damps its steering with the yaw rate and lifts
            // while sliding, or full lock plus full throttle spins a rear-drive car
            let sliding = car.sim.lateral_speed(car.yaw).abs() > 3.0;
            let steer = (diff * 2.5 - 0.25 * car.sim.yaw_rate).clamp(-1.0, 1.0);
            if backing > 0.0 {
                backing -= dt;
                Input { brake: true, steer: -(diff * 2.5).clamp(-1.0, 1.0), ..Default::default() }
            } else {
                Input { throttle: car.speed < want && !sliding, brake: car.speed > want + 5.0, steer, ..Default::default() }
            }
        };
        car.sim.held = r.held(); // on the grid the brake is held on until GO (retail)
        car.step(input, dt, &ground, &walls);
        {
            let (v_f, v_r) = (car.sim.forward_speed(car.yaw), car.sim.lateral_speed(car.yaw));
            let slip = v_r.abs().atan2(v_f.abs()).to_degrees();
            let now = car.sim.speed() > 10.0 && (slip > 35.0 || v_f < -2.0);
            if now && !spinning {
                spins += 1;
                if spins <= 5 {
                    let _ = writeln!(out, "   tick {:>5}: SPIN at ({:.0},{:.0},{:.0}) v_f {v_f:.1} v_r {v_r:.1} yaw rate {:.2} wall {} ground {} pitch {:.2} roll {:.2} steer {:.2} thr {} brk {}",
                        r.tick, car.pos.x, car.pos.y, car.pos.z, car.sim.yaw_rate, car.hit_wall as u8, car.on_ground as u8, car.pitch, car.roll, input.steer, input.throttle as u8, input.brake as u8);
                }
            }
            if car.sim.speed() < 3.0 || slip < 15.0 {
                spinning = false;
            }
            spinning |= now;
        }
        let player_progress = r.cars[0].progress;
        let rate = ai::player_rate(player_progress - fractions[0], r.race_ticks());
        let others: Vec<Other> = std::iter::once(Other { pos: car.pos, speed: car.speed }).chain(racers.iter().map(|x| Other { pos: x.0.pos, speed: x.0.speed })).collect();
        let mut samples = vec![CarSample::default(); cars];
        for i in 0..racers.len() {
            let view = View { path: p, routes: &t.routes, line: &line, dir, held: r.held(), finished: r.cars[i + 1].state == race::CarState::Finished, difficulty, lead: r.cars[i + 1].progress - player_progress, player_rate: rate };
            let near: Vec<Other> = others.iter().enumerate().filter(|(k, _)| *k != i + 1).map(|(_, o)| *o).collect();
            let (c, brain, rname) = &mut racers[i];
            let before_rec = brain.recoveries;
            let input = brain.think(c, &view, &near, &mut rng);
            if let Some(ch) = brain.new_choice.take() {
                if ch.options > 1 {
                    *taken.entry((ch.junction, ch.route)).or_insert(0u32) += 1;
                    if choice_lines < 40 {
                        choice_lines += 1;
                        let rt = t.routes.junctions[ch.junction].routes[ch.route];
                        let _ = writeln!(out, "      tick {:>5}: {rname} at junction {} takes route {} (run {} weight {:.2}, r {:.2}, {} ways)", r.tick, ch.junction, ch.route, rt.run, rt.weight, ch.r, ch.options);
                    }
                }
            }
            c.sim.held = r.held();
            if brain.recoveries != before_rec && brain.recoveries <= 5 {
                let s = &p.slices[brain.slice];
                let _ = writeln!(out, "      tick {:>5}: {rname} recovers at slice {} (segment {}), speed {:.1}, {:.1} m off centre, heading . road {:.2}",
                    r.tick, brain.slice, p.segment_of(brain.slice), c.speed, (c.pos - s.pos).dot(s.right), c.heading().dot(s.forward * dir.sign()));
            }
            let before = c.pos;
            c.step(input, dt, &ground, &walls);
            brain.slice = p.track(brain.slice, c.pos);
            let fallen = c.pos.y < p.slices[brain.slice].pos.y - 30.0;
            if !r.held() && r.cars[i + 1].state == race::CarState::Racing && (fallen || brain.wants_reset()) {
                let (pos, yaw) = ai::reset_pose(p, &line, brain.slice, dir);
                *c = c.reset(pos, yaw);
                brain.was_reset(brain.slice);
                resets[i] += 1;
            }
            let crossed = t.finish.map_or(false, |(a, b)| crosses(before, c.pos, a, b));
            samples[i + 1] = CarSample { fraction: p.fraction(brain.slice, dir), crossed_finish: crossed };
        }
        // car against car (retail impulse): player with each opponent, opponents with each other
        for i in 0..racers.len() {
            if nfspu_game::car::collide_cars(&mut car, &mut racers[i].0).is_some_and(|h| h >= 20.0) {
                hits += 1;
            }
            for j in i + 1..racers.len() {
                let (a, b) = racers.split_at_mut(j);
                if nfspu_game::car::collide_cars(&mut a[i].0, &mut b[0].0).is_some_and(|h| h >= 20.0) {
                    hits += 1;
                }
            }
        }
        if !fleet.cars.is_empty() {
            let mut refs: Vec<&mut Car> = std::iter::once(&mut car).chain(racers.iter_mut().map(|x| &mut x.0)).collect();
            for e in fleet.tick(p, &ground, &walls, &mut refs, slice, dt, false) {
                use nfspu_game::traffic::Event;
                match e {
                    Event::Spawned { .. } => traffic_spawns += 1,
                    Event::Removed { .. } => traffic_removed += 1,
                    Event::Hit { .. } => traffic_hits += 1,
                    Event::Honk { .. } => traffic_honks += 1,
                }
                if traffic_spawns + traffic_removed <= 12 || matches!(e, Event::Hit { .. }) && traffic_hits <= 12 {
                    let _ = writeln!(out, "      tick {:>5}: traffic {e:?}", r.tick);
                }
            }
            traffic_max = traffic_max.max(fleet.driving());
        }
        if r.held() {
            let d = car.pos - start_pos;
            moved_before_go = moved_before_go.max((d.x * d.x + d.z * d.z).sqrt());
        }
        if let Some((a, b)) = std::env::var("RACE_DRIVE_WINDOW").ok().and_then(|w| { let (a, b) = w.split_once('-')?; Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?)) }) {
            if r.tick >= a && r.tick <= b && r.tick % 4 == 0 {
                let _ = writeln!(out, "      w {:>5} pos ({:.1},{:.1},{:.1}) yaw {:.2} rate {:.2} v_f {:.1} v_r {:.1} gear {} rpm {:.0} in t{} b{} s{:.2} wall {} imp {:.1} ground {} pitch {:.2} roll {:.2}",
                    r.tick, car.pos.x, car.pos.y, car.pos.z, car.yaw, car.sim.yaw_rate, car.speed, car.sim.lateral_speed(car.yaw), car.gear(), car.rpm(),
                    input.throttle as u8, input.brake as u8, input.steer, car.hit_wall as u8, car.impact, car.on_ground as u8, car.pitch, car.roll);
            }
        }
        if debug && ((r.tick % 64 == 0 && r.tick <= 64 * 120) || r.tick % 640 == 0) {
            let _ = writeln!(out, "      t {:>5} slice {slice:>5} seg {:>3} frac {:.3} pos ({:.0},{:.0},{:.0}) yaw {:.2} speed {:.1}",
                r.tick, p.segment_of(slice), p.fraction(slice, dir), car.pos.x, car.pos.y, car.pos.z, car.yaw, car.speed);
        }
        slice = p.track(slice, car.pos);
        if !r.held() && (car.pos.y < p.slices[slice].pos.y - 30.0 || car.wants_recover) {
            // as in the game: a car that fell off the world or crashed is put back on the road
            let (pos, yaw) = ai::reset_pose(p, &line, slice, dir);
            car = car.recover(pos, yaw);
            route = slice;
            route_target = p.slices[route].pos;
        }
        let crossed = t.finish.map_or(false, |(a, b)| crosses(before, car.pos, a, b));
        let vel = car.pos - before;
        if p.wrong_way(slice, vel / dt, dir) && !r.held() {
            wrong_ticks += 1;
        }
        samples[0] = CarSample { fraction: p.fraction(slice, dir), crossed_finish: crossed };
        for e in r.step(&samples) {
            if let Some(snd) = &mut sound {
                snd.event(&e, r.tick, r.setup.laps, cars);
            }
            let line = match e {
                RaceEvent::Countdown(n) => format!("   tick {:>5}: countdown {}", r.tick, if n == 0 { "GO".to_string() } else { n.to_string() }),
                RaceEvent::Lap { car: c, lap, ticks } => format!("   tick {:>5}: {} lap {lap} in {}", r.tick, who(c, &racers), race::format_time(ticks)),
                RaceEvent::LapOwed { car: c } => format!("   tick {:>5}: {} crossed the line backwards", r.tick, who(c, &racers)),
                RaceEvent::Finished { car: c, position, ticks } => format!("   tick {:>5}: {} FINISHED P{position} in {}, best lap {}", r.tick, who(c, &racers), race::format_time(ticks), r.best_lap(c).map_or("-".into(), race::format_time)),
            };
            let _ = writeln!(out, "{line}");
        }
        if let Some(snd) = &mut sound {
            let others: Vec<&Car> = racers.iter().map(|x| &x.0).collect();
            snd.tick(r.tick, &car, &others, slice, p.slices.len(), dt);
        }
        let prog = r.cars[0].progress;
        if prog > best_progress + 0.002 || r.held() { best_progress = best_progress.max(prog); last_gain = r.tick; }
        if !r.held() && r.tick - last_gain > 64 * 10 && backing <= 0.0 { backing = 3.0; last_gain = r.tick; }
        if r.race_ticks() > 64 * 60 * 25 { stuck = r.cars[0].state != race::CarState::Finished; break; }
    }
    for (i, (_, brain, n)) in racers.iter().enumerate() {
        let _ = writeln!(out, "   {n}: progress {:.3}, {} recoveries, {} resets{}", r.cars[i + 1].progress, brain.recoveries, resets[i], if r.cars[i + 1].finish_time.is_none() { " (DID NOT FINISH)" } else { "" });
    }
    if opponents > 0 {
        let rt = &t.routes;
        let _ = writeln!(out, "   junctions: {} ({} weights), runs {}", rt.junctions.len(), if rt.from_file { ".jnc" } else { "default" }, rt.runs.len());
        for (j, jn) in rt.junctions.iter().enumerate() {
            let out_ways = rt.leaving(j, dir == Direction::Forward);
            if out_ways.len() < 2 {
                continue;
            }
            let ways: Vec<String> = out_ways.iter().map(|&i| format!("route {i} run {} w {:.2} taken {}", jn.routes[i].run, jn.routes[i].weight, taken.get(&(j, i)).copied().unwrap_or(0))).collect();
            let _ = writeln!(out, "   junction {j}: {}", ways.join("; "));
        }
        let _ = writeln!(out, "   car-to-car hits of impact 20+: {hits}");
    }
    if !fleet.cars.is_empty() {
        let _ = writeln!(out, "   traffic: {traffic_spawns} spawns, {traffic_removed} removals, up to {traffic_max} driving at once, {traffic_hits} hits of impact 20+, {traffic_honks} honks");
    }
    if let Some(snd) = sound.take() {
        let path = log_file().with_file_name("race_sound.log");
        let _ = writeln!(out, "   sound: {} hits logged, log {}", snd.hits, path.display());
        let _ = std::fs::write(path, snd.log);
    }
    let _ = writeln!(out, "   held before GO: moved {moved_before_go:.2} m sideways; wrong-way ticks {wrong_ticks}; spins {spins}; final progress {:.3}; {}",
        r.cars[0].progress, if stuck { "GAVE UP (stuck or too slow)" } else { "ok" });
    !stuck && moved_before_go < 0.5 && r.all_finished()
}

fn log_file() -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("logs");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("race_drive.txt")
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if std::env::var("NFSPU_MIRROR").as_deref() == Ok("1") {
        nfspu_game::set_world_mirrored(true);
    }
    let mut out = String::new();
    let mut ok = true;
    if let Some(name) = a.first() {
        let dir = if a.get(1).map_or(false, |d| d.starts_with('b')) { Direction::Backward } else { Direction::Forward };
        let laps = a.get(2).and_then(|l| l.parse().ok()).unwrap_or(2);
        let opponents = a.get(3).and_then(|n| n.parse().ok()).unwrap_or(0usize).min(7);
        let difficulty = a.get(4).and_then(|d| d.parse().ok()).unwrap_or(0u8).min(2);
        ok &= drive(name, dir, laps, opponents, difficulty, &mut out);
    } else {
        for n in ["alps", "autobahn", "canyon", "castle", "coastal", "farmland", "foothills", "forest", "industrial", "monaco1", "monaco2", "monaco3", "monaco4", "monaco5", "skidpad"] {
            ok &= drive(n, Direction::Forward, 2, 0, 0, &mut out);
            let _ = std::fs::write(log_file(), &out);
        }
    }
    print!("{out}");
    let _ = std::fs::write(log_file(), &out);
    println!("{}", if ok { "ALL OK" } else { "SOME FAILED" });
}
