//! Opt-in car captures for checking the car models by eye: `NFSPU_CAR_CAPTURE=993,959/2,boxster`
//! loads each car (an optional `/style` picks the body style), takes a screenshot from the chase,
//! cockpit and four orbit views, saves them as `logs/car_<name>_s<style>_<view>.png` and exits.
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

#[derive(Resource, Default)]
pub struct CarProbe {
    cars: Vec<(String, usize)>,
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
                    .map(|s| match s.trim().split_once('/') {
                        Some((n, st)) => (n.to_string(), st.parse().unwrap_or(1)),
                        None => (s.trim().to_string(), 1),
                    })
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
    let (car, car_style) = probe.cars[probe.car].clone();
    if !probe.loaded {
        commands.entity(holder.model).despawn();
        name.0 = car.clone();
        style.0 = car_style;
        let options = car_model_options(car_style, driver.0, paint.0);
        let (model, count) = super::spawn_car_model(&mut commands, &mut meshes, &mut materials, &mut images, holder.root, &car, &options);
        holder.model = model;
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
    let path = super::log_path().parent().unwrap().join(format!("car_{safe}_s{car_style}_{view_name}.png"));
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

fn car_model_options(style: usize, driver: u8, paint: usize) -> nfspu_game::car_model::CarOptions {
    nfspu_game::car_model::CarOptions { style, driver, paint, ..Default::default() }
}
