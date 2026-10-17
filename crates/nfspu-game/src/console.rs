//! A text console for the viewer: press `~` (backquote) to open it, type a command, Enter to run,
//! Escape to close. Commands that change the car or the track take effect immediately.
//!
//! Commands:
//! - `help` - list the commands
//! - `cars` / `tracks` - list what is available
//! - `car [name]` - show the current car, or switch to one by name
//! - `style [n]` - show or set the body style (1..number of styles)
//! - `paint [n]` - show or set the paint scheme (section number in the car's .clr file)
//! - `driver [n]` - show or set the driver (1 = racing suit, 2-10 casual, 0 empty)
//! - `track [name]` - show the current track, or reload it
//! - `cam [0-2]` - camera: 0 chase, 1 cockpit, 2 orbit
//! - `lights [on|off]` - headlights
//! - `reset` - put the car back at the start
//! - `info` - what is loaded right now
//! - `close` - close the console

use bevy::ecs::system::SystemParam;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use std::path::PathBuf;

/// Is the console open, and what has been typed.
#[derive(Resource, Default)]
pub struct Console {
    pub open: bool,
    pub input: String,
    /// Lines shown above the input, newest last (cleared by `clear`).
    pub lines: Vec<String>,
}

/// Which cars and tracks exist, and which is loaded.
#[derive(Resource)]
pub struct Catalog {
    pub cars: Vec<String>,
    pub tracks: Vec<String>,
}

/// A request made by a console command; the systems that own the world act on it.
#[derive(Resource, Default)]
pub struct Pending {
    pub car: Option<String>,
    pub style: Option<usize>,
    pub paint: Option<usize>,
    pub driver: Option<u8>,
    pub track: Option<String>,
    pub camera: Option<u8>,
    pub lights: Option<bool>,
    pub reset: bool,
}

/// Marks the console's UI so it can be shown and hidden.
#[derive(Component)]
pub struct ConsoleRoot;

/// Tag on the text node that shows the transcript and the prompt.
#[derive(Component)]
pub struct ConsoleText;

/// Car and track file lists from the install (both folders must exist).
pub fn catalog(game_dir: &PathBuf) -> Catalog {
    let list = |dir: PathBuf, suffix: &str| -> Vec<String> {
        let mut out: Vec<String> = fs_names(&dir, suffix);
        out.sort_by_key(|n| n.to_ascii_lowercase());
        out
    };
    Catalog { cars: list(game_dir.join("GameData").join("CarModel"), "crp"), tracks: list(game_dir.join("GameData").join("Track"), "crp") }
}

fn fs_names(dir: &PathBuf, suffix: &str) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case(suffix)))
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
        .collect()
}

/// Spawns the console's UI node once, at startup.
pub fn spawn(commands: &mut Commands) {
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            left: Val::Px(8.0),
            width: Val::Px(760.0),
            padding: UiRect::all(Val::Px(8.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.72)),
        ZIndex(100),
        ConsoleRoot,
        ConsoleText,
        Text::new(""),
        TextFont { font_size: FontSize::Px(15.0), ..default() },
        TextColor(Color::srgb(0.92, 0.95, 1.0)),
        Visibility::Hidden,
    ));
}

/// Reads keys while the console is open and runs the commands it builds. Typed characters come
/// from the keyboard events (so caps and punctuation work), everything else from `KeyCode`.
pub fn handle_input(
    mut events: MessageReader<KeyboardInput>,
    mut console: ResMut<Console>,
    catalog: Res<Catalog>,
    mut pending: ResMut<Pending>,
    mut text: Query<&mut Text, With<ConsoleText>>,
    mut root: Query<&mut Visibility, With<ConsoleRoot>>,
    car: Res<super::CarName>,
    track: Res<super::TrackName>,
    style: Res<super::CarStyle>,
    paint: Res<super::CarPaint>,
    driver: Res<super::CarDriver>,
    cam: Res<super::CamMode>,
    lights: Res<super::LightsOn>,
    styles: Option<Res<super::CarModelEntity>>,
) {
    // `~` toggles; when the console is open the other keys type into it instead
    if let Ok(mut v) = root.single_mut() {
        *v = if console.open { Visibility::Inherited } else { Visibility::Hidden };
    }

    let mut typed = String::new();
    for e in events.read() {
        if e.state != bevy::input::ButtonState::Pressed {
            continue;
        }
        // the toggle key must not also type its own ` character
        if e.key_code == KeyCode::Backquote {
            console.open = !console.open;
            console.input.clear();
            continue;
        }
        if !console.open {
            continue;
        }
        match &e.logical_key {
            Key::Character(c) => {
                let c = c.as_str();
                if c.chars().count() == 1 && console.input.len() < 60 {
                    typed.push_str(c);
                }
            }
            // the space bar arrives as its own key, not as a character
            Key::Space => {
                typed.push(' ');
            }
            Key::Backspace => {
                console.input.pop();
            }
            Key::Enter => {
                typed.push('\n');
            }
            Key::Escape => {
                console.open = false;
                console.input.clear();
            }
            _ => {}
        }
    }
    for ch in typed.chars() {
        if ch == '\n' {
            let line = console.input.trim().to_string();
            console.input.clear();
            if !line.is_empty() {
                console.lines.push(format!("> {line}"));
                run(&line, &mut console, &catalog, &mut pending, &car, &track, &style, &paint, &driver, &cam, &lights, styles.as_deref());
                if console.lines.len() > 12 {
                    let drop = console.lines.len() - 12;
                    console.lines.drain(0..drop);
                }
            }
        } else {
            console.input.push(ch);
        }
    }

    let mut shown = console.lines.join("\n");
    shown.push_str(&format!("\n> {}_", console.input));
    for mut t in &mut text {
        **t = shown.clone();
    }
}

/// Finds a name the way a person would type it: exact (any case) wins, otherwise a unique
/// beginning of the name. Returns the names that start with `word`, for the error message.
fn matches(candidates: &[String], word: &str) -> Vec<String> {
    let w = word.to_ascii_lowercase();
    candidates.iter().filter(|c| c.to_ascii_lowercase().starts_with(&w)).cloned().collect()
}

/// The one entry of `candidates` that `word` names, if it is unambiguous.
fn resolve(candidates: &[String], word: &str) -> Option<String> {
    if let Some(hit) = candidates.iter().find(|c| c.eq_ignore_ascii_case(word)) {
        return Some(hit.clone());
    }
    let hits = matches(candidates, word);
    (hits.len() == 1).then(|| hits[0].clone())
}

#[allow(clippy::too_many_arguments)]
fn run(
    line: &str,
    console: &mut Console,
    catalog: &Catalog,
    pending: &mut Pending,
    car: &super::CarName,
    track: &super::TrackName,
    style: &super::CarStyle,
    paint: &super::CarPaint,
    driver: &super::CarDriver,
    cam: &super::CamMode,
    lights: &super::LightsOn,
    styles: Option<&super::CarModelEntity>,
) {
    let say = |console: &mut Console, msg: String| console.lines.push(msg);
    let (word, rest) = match line.split_once(char::is_whitespace) {
        Some((w, r)) => (w.to_ascii_lowercase(), r.trim()),
        None => (line.to_ascii_lowercase(), ""),
    };
    let style_count = styles.map_or(0, |s| s.style_count);
    match word.as_str() {
        "help" | "?" => {
            say(console, "commands: help, cars, car [name], style [n], paint [n], driver [n], tracks, track [name], cam [0-2], lights [on|off], reset, info, clear, close".into());
        }
        "cars" => {
            let all = catalog.cars.join(" ");
            say(console, format!("cars ({}): {}", catalog.cars.len(), all));
        }
        "tracks" => {
            let all = catalog.tracks.join(" ");
            say(console, format!("tracks ({}): {}", catalog.tracks.len(), all));
        }
        "car" => {
            if rest.is_empty() {
                say(console, format!("car: {}", car.0));
            } else if let Some(name) = resolve(&catalog.cars, rest) {
                pending.car = Some(name.clone());
                say(console, format!("switching to car '{name}'"));
            } else {
                let near = matches(&catalog.cars, rest).join(" ");
                say(console, if near.is_empty() { format!("no car named '{rest}' (try 'cars')") } else { format!("ambiguous: {near}") });
            }
        }
        "style" => {
            if rest.is_empty() {
                say(console, format!("style: {} of {}", style.0, style_count));
            } else if let Ok(n) = rest.parse::<usize>() {
                pending.style = Some(n.max(1));
                say(console, format!("style {n}"));
            } else {
                say(console, format!("style must be a number (1..{style_count})"));
            }
        }
        "paint" => {
            if rest.is_empty() {
                say(console, format!("paint: {}", paint.0));
            } else if let Ok(n) = rest.parse::<usize>() {
                pending.paint = Some(n);
                say(console, format!("paint {n}"));
            } else {
                say(console, "paint must be a section number from the car's .clr file".into());
            }
        }
        "driver" => {
            if rest.is_empty() {
                say(console, format!("driver: {}", driver.0));
            } else if let Ok(n) = rest.parse::<u8>() {
                pending.driver = Some(n);
                say(console, format!("driver {n}"));
            } else {
                say(console, "driver must be 0 (none) or 1-10".into());
            }
        }
        "track" => {
            if rest.is_empty() {
                say(console, format!("track: {}", track.0));
            } else if let Some(name) = resolve(&catalog.tracks, rest) {
                pending.track = Some(name.clone());
                say(console, format!("loading track '{name}'"));
            } else {
                say(console, format!("no track named '{rest}' (try 'tracks')"));
            }
        }
        "cam" | "camera" => {
            if rest.is_empty() {
                say(console, format!("camera: {}", super::CAMERA_NAMES[cam.0 as usize]));
            } else if let Ok(n) = rest.parse::<u8>() {
                if (n as usize) < super::CAMERA_NAMES.len() {
                    pending.camera = Some(n);
                    say(console, format!("camera: {}", super::CAMERA_NAMES[n as usize]));
                } else {
                    say(console, format!("camera must be 0..{}", super::CAMERA_NAMES.len() - 1));
                }
            }
        }
        "lights" => {
            if rest.is_empty() {
                say(console, format!("headlights: {}", if lights.0 { "on" } else { "off" }));
            } else {
                let on = matches!(rest.to_ascii_lowercase().as_str(), "on" | "1" | "true" | "yes");
                pending.lights = Some(on);
                say(console, format!("headlights: {}", if on { "on" } else { "off" }));
            }
        }
        "reset" => {
            pending.reset = true;
            say(console, "car reset to the start".into());
        }
        "info" => {
            say(console, format!("track {} | car {} style {}/{} paint {} driver {} camera {} headlights {}", track.0, car.0, style.0, style_count, paint.0, driver.0, super::CAMERA_NAMES[cam.0 as usize], if lights.0 { "on" } else { "off" }));
        }
        "clear" => console.lines.clear(),
        "close" => console.open = false,
        other => say(console, format!("unknown command '{other}' (try 'help')")),
    }
}

/// What the console can change about the car, in one place (Bevy allows 16 parameters per system).
#[derive(SystemParam)]
pub struct CarState<'w> {
    pub name: ResMut<'w, super::CarName>,
    pub style: ResMut<'w, super::CarStyle>,
    pub paint: ResMut<'w, super::CarPaint>,
    pub driver: ResMut<'w, super::CarDriver>,
}

/// The rest of the world the console touches: the track, the view, the car on the road and the
/// camera.
#[derive(SystemParam)]
pub struct World<'w, 's> {
    pub track: ResMut<'w, super::TrackName>,
    pub cam: ResMut<'w, super::CamMode>,
    pub lights: ResMut<'w, super::LightsOn>,
    pub car: Option<ResMut<'w, super::CarRes>>,
    pub holder: Option<ResMut<'w, super::CarModelEntity>>,
    pub old: Option<Res<'w, super::TrackEntities>>,
    pub ground: Option<ResMut<'w, super::GroundRes>>,
    pub walls: Option<ResMut<'w, super::WallRes>>,
    pub cams: Query<'w, 's, &'static mut Transform, With<super::FlyCamera>>,
}

/// Carries out the commands the console asked for: rebuild the car, reload the track, and so on.
pub fn apply(
    mut pending: ResMut<Pending>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut car: CarState,
    mut world: World,
) {
    if let Some(n) = pending.camera.take() {
        world.cam.0 = n;
    }
    if let Some(on) = pending.lights.take() {
        world.lights.0 = on;
    }
    if pending.reset {
        pending.reset = false;
        if let Some(c) = world.car.as_mut() {
            c.car = super::Car::new(c.start_pos, c.start_yaw);
            super::log_line("car reset to the start (console)");
        }
    }
    if let Some(t) = pending.track.take() {
        if t != world.track.0 {
            world.track.0 = t;
            super::reload_track(
                &mut commands,
                &mut meshes,
                &mut materials,
                &mut images,
                &world.track.0,
                world.old.as_deref(),
                world.car.as_deref_mut(),
                world.ground.as_deref_mut(),
                world.walls.as_deref_mut(),
                world.cams,
            );
        }
    }
    let rebuild = pending.car.is_some() || pending.style.is_some() || pending.paint.is_some() || pending.driver.is_some();
    if !rebuild {
        return;
    }
    if let Some(c) = pending.car.take() {
        car.name.0 = c;
        car.style.0 = 1;
    }
    if let Some(s) = pending.style.take() {
        car.style.0 = s;
    }
    if let Some(p) = pending.paint.take() {
        car.paint.0 = p;
    }
    if let Some(d) = pending.driver.take() {
        car.driver.0 = d;
    }
    let Some(mut holder) = world.holder.take() else { return };
    commands.entity(holder.model).despawn();
    let options = super::car_model::CarOptions { style: car.style.0, driver: car.driver.0, paint: car.paint.0, ..Default::default() };
    let (model, count) = super::spawn_car_model(&mut commands, &mut meshes, &mut materials, &mut images, holder.root, &car.name.0, &options);
    holder.model = model;
    holder.style_count = count;
    super::log_line(&format!(
        "console: car '{}' style {} of {} paint {} driver {}",
        car.name.0, car.style.0, count, car.paint.0, car.driver.0
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Catalog {
        Catalog { cars: vec!["993".into(), "boxster".into(), "cop993".into()], tracks: vec!["alps".into(), "monaco1".into()] }
    }

    fn run_line(line: &str) -> (Console, Pending) {
        let mut console = Console::default();
        let mut pending = Pending::default();
        let catalog = names();
        let car = super::super::CarName("993".into());
        let track = super::super::TrackName("alps".into());
        let style = super::super::CarStyle(1);
        let paint = super::super::CarPaint(4);
        let driver = super::super::CarDriver(1);
        let cam = super::super::CamMode(0);
        let lights = super::super::LightsOn(false);
        let holder = super::super::CarModelEntity { root: Entity::PLACEHOLDER, model: Entity::PLACEHOLDER, style_count: 9 };
        run(line, &mut console, &catalog, &mut pending, &car, &track, &style, &paint, &driver, &cam, &lights, Some(&holder));
        (console, pending)
    }

    #[test]
    fn car_names_may_be_shortened() {
        assert_eq!(run_line("car 993").1.car.as_deref(), Some("993"));
        assert_eq!(run_line("car BOXSTER").1.car.as_deref(), Some("boxster")); // any case
        assert_eq!(run_line("car cop993").1.car.as_deref(), Some("cop993"));
        let (console, pending) = run_line("car nosuchcar");
        assert!(pending.car.is_none());
        assert!(console.lines.last().unwrap().contains("no car named"));
        assert_eq!(run_line("car").1.car, None); // asking, not changing
    }

    #[test]
    fn numbers_are_checked() {
        assert_eq!(run_line("style 6").1.style, Some(6));
        assert_eq!(run_line("style 0").1.style, Some(1)); // styles start at 1
        assert!(run_line("style x").1.style.is_none());
        assert_eq!(run_line("paint 21").1.paint, Some(21));
        assert_eq!(run_line("driver 0").1.driver, Some(0));
        assert_eq!(run_line("cam 2").1.camera, Some(2));
        assert_eq!(run_line("lights on").1.lights, Some(true));
        assert_eq!(run_line("lights off").1.lights, Some(false));
        assert!(run_line("reset").1.reset);
        let (console, pending) = run_line("cam 9");
        assert!(pending.camera.is_none());
        assert!(console.lines.last().unwrap().contains("camera must be"));
    }

    #[test]
    fn tracks_and_help_and_unknown() {
        assert_eq!(run_line("track monaco1").1.track.as_deref(), Some("monaco1"));
        assert_eq!(run_line("track").1.track, None);
        let (console, _) = run_line("track nowhere");
        assert!(console.lines.last().unwrap().contains("no track named"));
        assert!(run_line("help").0.lines.last().unwrap().contains("commands"));
        assert!(run_line("wibble").0.lines.last().unwrap().contains("unknown command"));
    }
}
