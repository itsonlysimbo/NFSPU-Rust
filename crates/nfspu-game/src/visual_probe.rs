//! Opt-in rendered regression captures. Normal play is unaffected when disabled.
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};

#[derive(Resource, Default)]
pub struct Probe {
    enabled: bool,
    heading: usize,
    settled_frames: u32,
    waiting: bool,
}
impl Probe {
    pub fn from_env() -> Self {
        Self { enabled: std::env::var("NFSPU_SKY_CAPTURE").as_deref() == Ok("1"), ..default() }
    }
}

pub fn update(
    mut probe: ResMut<Probe>,
    mut mode: ResMut<super::DriveMode>,
    car: Option<Res<super::CarRes>>,
    track: Res<super::TrackName>,
    mut camera: Query<(&mut Transform, &mut super::FlyCamera)>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    if !probe.enabled { return; }
    if probe.heading >= 4 {
        super::log_line("visual probe: four captures complete, intentional exit");
        probe.enabled = false;
        exit.write(AppExit::Success);
        return;
    }
    let Some(car) = car else { return };
    mode.0 = false;
    let yaw = probe.heading as f32 * std::f32::consts::FRAC_PI_2;
    for (mut transform, mut fly) in &mut camera {
        transform.translation = car.start_pos + Vec3::Y * 250.0;
        transform.rotation = Quat::from_rotation_y(yaw);
        fly.yaw = yaw;
        fly.pitch = 0.0;
    }
    if probe.waiting { return; }
    probe.settled_frames += 1;
    if probe.settled_frames < 90 { return; }
    // Keep all capture output inside this project's ignored logs directory.
    let safe_track: String = track.0.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    let path = super::log_path().parent().unwrap().join(format!("horizon_{safe_track}_{}.png", probe.heading * 90));
    super::log_line(&format!("visual probe: heading {} deg, position {:?}, capture {}",
        probe.heading * 90, car.start_pos + Vec3::Y * 250.0, path.display()));
    probe.waiting = true;
    commands.spawn(Screenshot::primary_window()).observe(
        move |captured: On<ScreenshotCaptured>, mut probe: ResMut<Probe>, mut exit: MessageWriter<AppExit>| {
            // Save synchronously so an intentional exit never races the file write.
            let result = captured.image.clone().try_into_dynamic()
                .map_err(|e| format!("image conversion: {e}"))
                .and_then(|image| image.to_rgb8().save(&path).map_err(|e| e.to_string()));
            match result {
                Ok(()) => {
                    super::log_line(&format!("visual probe: saved {}", path.display()));
                    probe.heading += 1;
                    probe.settled_frames = 0;
                    probe.waiting = false;
                }
                Err(error) => {
                    super::log_line(&format!("visual probe FAILED: {error}"));
                    probe.enabled = false;
                    exit.write(AppExit::error());
                }
            }
        },
    );
}
