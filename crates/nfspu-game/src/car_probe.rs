//! Opt-in car captures for checking the car models by eye: `NFSPU_CAR_CAPTURE=993,959/2,914/1/7`
//! loads each car (an optional `/style` picks the body style, a further `/N` the `.clr` paint
//! section or `/#rrggbb` a custom body colour), takes a screenshot from the chase, cockpit and four
//! orbit views, saves them as `logs/car_<name>_s<style>[_p<paint>]_<view>.png` and exits.
//! Normal play is unaffected when the variable is not set.
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};

/// Camera mode and, for the orbit camera, (yaw, pitch, radius) of each capture.
const VIEWS: [(&str, u8, [f32; 3]); 6] = [
    ("chase", 0, [0.0; 3]),
    ("cockpit", 1, [0.0; 3]),
    ("front", 2, [2.6, 0.18, 5.2]),
    ("side", 2, [1.5708, 0.10, 5.2]),
    ("rear", 2, [-0.6, 0.30, 5.2]),
    ("top", 2, [0.9, 1.0, 5.5]),
];

/// One car to capture: model, body style, and paint (a section, or a custom colour), if given.
#[derive(Clone, Debug, PartialEq)]
struct Entry {
    car: String,
    style: usize,
    paint: Option<usize>,
    body: Option<[u8; 3]>,
    /// File-name tag for the paint (`_p7`, `_p0080ff`), empty without one.
    tag: String,
}

fn parse_entry(text: &str) -> Entry {
    let mut parts = text.trim().split('/');
    let car = parts.next().unwrap_or_default().to_string();
    let style = parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let (mut paint, mut body, mut tag) = (None, None, String::new());
    if let Some(p) = parts.next() {
        if let Some(hex) = p.strip_prefix('#').filter(|h| h.len() == 6) {
            let v = u32::from_str_radix(hex, 16).unwrap_or(0);
            body = Some([(v >> 16) as u8, (v >> 8) as u8, v as u8]);
            tag = format!("_p{}", hex.to_ascii_lowercase());
        } else if let Ok(n) = p.parse() {
            paint = Some(n);
            tag = format!("_p{n}");
        }
    }
    Entry { car, style, paint, body, tag }
}

#[derive(Resource, Default)]
pub struct CarProbe {
    cars: Vec<Entry>,
    car: usize,
    view: usize,
    frames: u32,
    loaded: bool,
    waiting: bool,
    /// Seconds spent on the current view.
    elapsed: f32,
}

impl CarProbe {
    pub fn from_env() -> Self {
        let cars = std::env::var("NFSPU_CAR_CAPTURE")
            .map(|v| {
                v.split(',')
                    .filter(|s| !s.trim().is_empty())
                    .map(parse_entry)
                    .collect()
            })
            .unwrap_or_default();
        CarProbe { cars, ..default() }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn update(
    time: Res<Time>,
    mut probe: ResMut<CarProbe>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    holder: Option<ResMut<super::CarModelEntity>>,
    mut name: ResMut<super::CarName>,
    mut style: ResMut<super::CarStyle>,
    driver: Res<super::CarDriver>,
    paint: Res<super::CarPaint>,
    mut cam_mode: ResMut<super::CamMode>,
    mut orbit: ResMut<super::Orbit>,
    mut exit: MessageWriter<AppExit>,
) {
    if probe.cars.is_empty() || probe.waiting {
        return;
    }
    let Some(mut holder) = holder else { return };
    if probe.car >= probe.cars.len() {
        super::log_line("car probe: all captures done, intentional exit");
        probe.cars.clear();
        exit.write(AppExit::Success);
        return;
    }
    let entry = probe.cars[probe.car].clone();
    let (car, car_style) = (entry.car.clone(), entry.style);
    if !probe.loaded {
        commands.entity(holder.model).despawn();
        name.0 = car.clone();
        style.0 = car_style;
        let options = nfspu_game::car_model::CarOptions { style: car_style, driver: driver.0, paint: entry.paint.unwrap_or(paint.0), body: entry.body, ..Default::default() };
        let (model, lean, count) = super::spawn_car_model(&mut commands, &mut meshes, &mut materials, &mut images, holder.root, &car, &options);
        holder.model = model;
        holder.lean = lean;
        holder.style_count = count;
        probe.loaded = true;
        probe.frames = 0;
        probe.elapsed = 0.0;
        return;
    }
    let (view_name, mode, [yaw, pitch, radius]) = VIEWS[probe.view];
    cam_mode.0 = mode;
    if mode == 2 {
        orbit.yaw = yaw;
        orbit.pitch = pitch;
        orbit.radius = radius;
    }
    probe.frames += 1;
    // the very first capture waits longer: the track and window are still settling; the chase view
    // also needs its camera to swing in from straight behind (about a second, `chase_cam`)
    probe.elapsed += time.delta_secs();
    let wait = if probe.car == 0 && probe.view == 0 { 150 } else { 40 };
    if probe.frames < wait || (mode == 0 && probe.elapsed < 1.5) {
        return;
    }
    let safe: String = car.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    let path = super::log_path().parent().unwrap().join(format!("car_{safe}_s{car_style}{}_{view_name}.png", entry.tag));
    probe.waiting = true;
    commands.spawn(Screenshot::primary_window()).observe(move |captured: On<ScreenshotCaptured>, mut probe: ResMut<CarProbe>| {
        let result = captured.image.clone().try_into_dynamic().map_err(|e| e.to_string()).and_then(|image| image.to_rgb8().save(&path).map_err(|e| e.to_string()));
        super::log_line(&format!("car probe: {} -> {:?}", path.display(), result));
        probe.waiting = false;
        probe.frames = 0;
        probe.elapsed = 0.0;
        probe.view += 1;
        if probe.view >= VIEWS.len() {
            probe.view = 0;
            probe.car += 1;
            probe.loaded = false;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_name_style_and_paint() {
        assert_eq!(parse_entry("993"), Entry { car: "993".into(), style: 1, paint: None, body: None, tag: String::new() });
        assert_eq!(parse_entry("914/1/7").paint, Some(7));
        let custom = parse_entry("914/2/#0080FF");
        assert_eq!((custom.style, custom.body, custom.tag.as_str()), (2, Some([0, 128, 255]), "_p0080ff"));
    }
}
