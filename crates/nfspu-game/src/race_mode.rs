//! Race mode for the driving viewer: `NFSPU_RACE=<track>[,<forward|backward>[,<laps>]]` puts the
//! player's car on the retail start grid, runs the countdown (car held until GO), counts laps,
//! finishes the race and then shows a placeholder results screen.
//!
//! Hook API (for the coordinator, HUD and loading screen):
//! - `RacePlugin(Option<RaceRequest>)`: always inserts `nfspu_game::race::RaceStatus` (default =
//!   inactive) and `RaceHold`; with a request it also starts the race after `setup`.
//! - `RaceRequest::from_env()` parses `NFSPU_RACE`; build one by hand to start a race from a menu.
//! - `RaceHold(true)` while the cars must stay still; `drive_car` ignores input then.
//! - Events go to `logs/race.log`.

use bevy::prelude::*;
use nfspu_game::car::Car;
use nfspu_game::race::{self, CarSample, Kind, Race, RaceEvent, RaceSetup, RaceStatus};
use nfspu_game::race_path::{self, crosses, grid_spot, Direction, TrackRace};
use std::io::Write as _;
use std::path::PathBuf;

/// Which race to run.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct RaceRequest {
    pub track: String,
    pub direction: Direction,
    /// Laps for circuits (point-to-point roads always run once).
    pub laps: u32,
    /// Cars on the grid (only the player drives for now; AI comes later).
    pub cars: usize,
}

impl RaceRequest {
    /// Parses `<track>[,<forward|backward>[,<laps>]]`.
    pub fn parse(s: &str) -> Option<Self> {
        let mut parts = s.split(',').map(str::trim);
        let track = parts.next().filter(|t| !t.is_empty())?.to_ascii_lowercase();
        let direction = match parts.next().map(|d| d.to_ascii_lowercase()) {
            Some(d) if d.starts_with('b') || d == "reverse" => Direction::Backward,
            _ => Direction::Forward,
        };
        let laps = parts.next().and_then(|l| l.parse().ok()).filter(|&l| l > 0).unwrap_or(2);
        Some(RaceRequest { track, direction, laps, cars: 1 })
    }

    pub fn from_env() -> Option<Self> {
        std::env::var("NFSPU_RACE").ok().and_then(|s| Self::parse(&s))
    }
}

/// True while the cars must stay on the grid.
#[derive(Resource, Default)]
pub struct RaceHold(pub bool);

#[derive(Resource)]
struct RaceRun {
    race: Race,
    track: TrackRace,
    player: usize,
    slice: usize,
    last_pos: Vec3,
    /// Unspent frame time, run off in 64 Hz ticks.
    carry: f32,
    wrong_way: bool,
    finished_for: f32,
    results_shown: bool,
}

pub struct RacePlugin(pub Option<RaceRequest>);

impl Plugin for RacePlugin {
    fn build(&self, app: &mut App) {
        // With a race requested the car is held from the start (also behind the loading screen).
        app.insert_resource(RaceStatus::default()).insert_resource(RaceHold(self.0.is_some()));
        if let Some(req) = &self.0 {
            // The race starts on the first frame the car exists: right after start-up, or once the
            // loading screen has run `setup`.
            app.insert_resource(req.clone())
                .add_systems(Update, (start_race, race_tick, results_screen, leave_results).chain().after(crate::drive_car));
        }
    }
}

fn race_log_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("logs").join("race.log")
}

fn race_log(msg: &str) {
    crate::log_line(&format!("race: {msg}"));
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(race_log_path()) {
        let _ = writeln!(f, "{msg}");
    }
}

fn start_race(mut commands: Commands, mut started: Local<bool>, req: Res<RaceRequest>, car: Option<ResMut<crate::CarRes>>, mut status: ResMut<RaceStatus>, mut hold: ResMut<RaceHold>) {
    if *started || car.is_none() {
        return;
    }
    *started = true;
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
    let cars = req.cars.max(1);
    // The player starts at the back of the grid (not checked against retail yet).
    let player = cars - 1;
    let spots: Vec<_> = (0..cars).map(|g| grid_spot(p, track.start_slice, g, cars, req.direction)).collect();
    let spot = spots[player];
    let start = spot.pos + Vec3::Y * 1.5;
    car.car = Car::new(start, spot.yaw);
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
    for (g, s) in spots.iter().enumerate() {
        race_log(&format!("grid {g}: slice {} at ({:.1}, {:.1}, {:.1}) yaw {:.3}{}", s.slice, s.pos.x, s.pos.y, s.pos.z, s.yaw, if g == player { " (player)" } else { "" }));
    }
    *status = RaceStatus { track: req.track.clone(), backwards: req.direction == Direction::Backward, ..Default::default() };
    race.fill_status(player, &mut status);
    hold.0 = true;
    commands.insert_resource(RaceRun { race, player, slice: spot.slice, last_pos: start, carry: 0.0, wrong_way: false, finished_for: 0.0, results_shown: false, track });
}

fn race_tick(time: Res<Time>, run: Option<ResMut<RaceRun>>, car: Option<Res<crate::CarRes>>, mut status: ResMut<RaceStatus>, mut hold: ResMut<RaceHold>) {
    let (Some(mut run), Some(car)) = (run, car) else { return };
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
    while run.carry >= step {
        run.carry -= step;
        let mut samples = vec![CarSample::default(); run.race.cars.len()];
        samples[run.player] = CarSample { fraction, crossed_finish: crossed };
        crossed = false;
        for e in run.race.step(&samples) {
            let t = run.race.tick;
            match e {
                RaceEvent::Countdown(0) => race_log(&format!("tick {t}: GO")),
                RaceEvent::Countdown(n) => race_log(&format!("tick {t}: countdown {n}")),
                RaceEvent::Lap { car, lap, ticks } => race_log(&format!("tick {t}: car {car} lap {lap} in {} (slice {})", race::format_time(ticks), run.slice)),
                RaceEvent::LapOwed { car } => race_log(&format!("tick {t}: car {car} crossed the line backwards, lap owed")),
                RaceEvent::Finished { car, position, ticks } => race_log(&format!("tick {t}: car {car} FINISHED P{position} in {}", race::format_time(ticks))),
            }
        }
    }
    if crossed {
        // the finish was crossed in a frame too short to hold a tick: keep it for the next one
        run.last_pos = pos - velocity * dt;
    }
    hold.0 = run.race.held();
    run.race.fill_status(run.player, &mut status);
    status.wrong_way = run.wrong_way;
    status.speed_kmh = car.car.speed.abs() * 3.6;
    status.rpm = None;
    if status.finished {
        run.finished_for += dt;
    }
}

/// Marks the placeholder results panel.
#[derive(Component)]
struct ResultsPanel;

/// After the player finishes (and 2 s pass, a guess at retail's pause), lists the standings.
fn results_screen(mut commands: Commands, run: Option<ResMut<RaceRun>>, status: Res<RaceStatus>) {
    let Some(mut run) = run else { return };
    if run.results_shown || !status.finished || run.finished_for < 2.0 {
        return;
    }
    run.results_shown = true;
    let mut text = format!("RACE RESULTS\n{} ({})\n\n", status.track, if status.backwards { "backward" } else { "forward" });
    for r in &status.results {
        let who = if r.is_player { "You".to_string() } else { format!("Car {}", r.car + 1) };
        let time = r.time.map_or("DNF".to_string(), race::format_secs);
        let best = r.best_lap.map_or("-".to_string(), race::format_secs);
        text += &format!("{}.  {:<8} {}   best lap {}\n", r.position, who, time, best);
    }
    if !status.point_to_point {
        for (i, t) in status.lap_times.iter().enumerate() {
            text += &format!("\n   lap {}: {}", i + 1, race::format_secs(*t));
        }
    }
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        race_log(&format!("results | {}", line.trim()));
    }
    text += "

Press Enter to continue";
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
        Text::new(text),
        TextFont { font_size: FontSize::Px(20.0), ..default() },
        TextColor(Color::srgb(1.0, 0.95, 0.8)),
        ResultsPanel,
    ));
}

/// Enter, Space or Escape on the results closes the race (the launcher then reopens the menu).
fn leave_results(run: Option<Res<RaceRun>>, keys: Res<ButtonInput<KeyCode>>, mut exit: MessageWriter<AppExit>) {
    if run.is_some_and(|r| r.results_shown) && keys.any_just_pressed([KeyCode::Enter, KeyCode::Space, KeyCode::Escape]) {
        race_log("results closed, leaving the race");
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_race_requests() {
        let r = RaceRequest::parse("Monaco1,backward,3").unwrap();
        assert_eq!((r.track.as_str(), r.direction, r.laps), ("monaco1", Direction::Backward, 3));
        let r = RaceRequest::parse("alps").unwrap();
        assert_eq!((r.direction, r.laps), (Direction::Forward, 2));
        assert_eq!(RaceRequest::parse("skidpad, f, 0").unwrap().laps, 2);
        assert!(RaceRequest::parse("").is_none());
    }
}
