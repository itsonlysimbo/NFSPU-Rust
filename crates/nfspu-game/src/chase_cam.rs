//! The retail chase camera: where it sits behind the car, how it trails the car's turns and what
//! it looks at. Behaviour read from the decomp (`docs/provenance/car.md`, "Chase camera"); the
//! code is our own. Positions are in Bevy space; offsets are given in the engine's car frame
//! (+z forward, origin at the car's centre, half the model's height above the ground).

use bevy::math::{Quat, Vec3};

/// The camera runs on the game's 64 Hz tick, so every share below is per tick.
pub const TICK_HZ: f32 = 64.0;
/// Share of the gap the side/forward offset closes per tick in the views without speed follow,
/// and in every view while it settles after a switch.
const FIXED_FOLLOW: f32 = 0.1;
/// Share of the gap the height closes per tick.
const HEIGHT_FOLLOW: f32 = 0.05;
/// Ticks after a switch during which both shares are [`FIXED_FOLLOW`] and the camera may come
/// closer than [`MIN_DISTANCE`].
const SETTLE_TICKS: u8 = 50;
/// The camera is pushed out to at least this distance from the car's centre.
const MIN_DISTANCE: f32 = 4.94;
/// Speed (m/s) at which the speed-following share reaches 1 (it is clamped well below that).
const FOLLOW_SPEED: f32 = 900.0;
/// Smallest speed-following share (a standing car).
const MIN_SPEED_FOLLOW: f32 = 0.02;
/// The view aims this far above the car's centre.
const LOOK_LIFT: f32 = 1.3;
/// The camera stays at least this far above the ground under it.
const GROUND_CLEARANCE: f32 = 1.0;
/// On a switch into a chase view the camera starts this far straight behind the car's centre.
const START_BEHIND: f32 = 4.0;
/// The track slope is measured over this many road slices ahead.
pub const SLOPE_SLICES: i32 = 3;

/// One chase view of the engine's camera table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChaseView {
    /// Offset in the engine's car frame; the car's half length is taken off z on top.
    pub offset: Vec3,
    /// Range the slope lift is held to (metres added to the height), or `None` for no lift.
    pub lift: Option<(f32, f32)>,
    /// Largest per-tick share of the speed-following side/forward offset, or `None` for the
    /// fixed share.
    pub speed_follow: Option<f32>,
}

/// Camera table entry 2: close behind the car, no slope lift, fixed follow.
pub const CLOSE: ChaseView = ChaseView { offset: Vec3::new(0.0, 1.15, -3.3), lift: None, speed_follow: None };
/// Camera table entry 3: the chase view seen in the retail recording (inferred, see the notes).
pub const MEDIUM: ChaseView = ChaseView { offset: Vec3::new(0.0, 1.35, -4.5), lift: Some((0.25, 1.0)), speed_follow: Some(0.1) };
/// Camera table entry 4: further back and lower, slow follow.
pub const FAR: ChaseView = ChaseView { offset: Vec3::new(0.0, 1.15, -4.8), lift: Some((-0.25, 2.0)), speed_follow: Some(0.05) };

/// What the camera needs to know about the car for one tick.
#[derive(Clone, Copy, Debug)]
pub struct CarFrame {
    /// The car's centre (half the model's height above its ground point).
    pub centre: Vec3,
    /// The car's orientation; its local -Z is forward.
    pub rot: Quat,
    /// Half the model's length.
    pub half_length: f32,
    /// Speed along the car's heading and sideways, m/s.
    pub forward_speed: f32,
    pub side_speed: f32,
    /// Rise of the road per slice over the next [`SLOPE_SLICES`] slices ahead (metres).
    pub slope: f32,
}

impl CarFrame {
    /// The car's forward direction.
    pub fn forward(&self) -> Vec3 {
        self.rot * Vec3::NEG_Z
    }
}

/// The chase camera's state: its offset from the car's centre in world space, which trails the
/// car's turns, and the settle counter.
#[derive(Clone, Copy, Debug, Default)]
pub struct ChaseCam {
    pub offset: Vec3,
    settle: u8,
}

impl ChaseCam {
    /// The camera as it is right after a switch into a chase view: straight behind, settling.
    pub fn start(car: &CarFrame) -> ChaseCam {
        ChaseCam { offset: -car.forward() * START_BEHIND, settle: SETTLE_TICKS }
    }

    /// Where the view wants to be this tick, relative to the car's centre in world space.
    pub fn wanted(view: &ChaseView, car: &CarFrame) -> Vec3 {
        let lift = view.lift.map_or(0.0, |(lo, hi)| car.slope.clamp(lo, hi));
        let o = view.offset;
        // engine car frame -> the car's local Bevy frame: forward is -Z there
        car.rot * Vec3::new(o.x, o.y + lift, -(o.z - car.half_length))
    }

    /// Advances one 64 Hz tick.
    pub fn tick(&mut self, view: &ChaseView, car: &CarFrame) {
        let wanted = Self::wanted(view, car);
        let (mut side, mut up) = (view.speed_follow.map_or(FIXED_FOLLOW, |cap| speed_follow(car, cap)), HEIGHT_FOLLOW);
        if self.settle > 0 {
            self.settle -= 1;
            (side, up) = (FIXED_FOLLOW, FIXED_FOLLOW);
        }
        let o = &mut self.offset;
        o.x += (wanted.x - o.x) * side;
        o.y += (wanted.y - o.y) * up;
        o.z += (wanted.z - o.z) * side;
        if self.settle == 0 {
            let d = o.length();
            if d > 1e-4 && d < MIN_DISTANCE {
                *o *= MIN_DISTANCE / d;
            }
        }
    }

    /// The camera position, kept [`GROUND_CLEARANCE`] above `ground` (the height of the surface
    /// under it, if any).
    pub fn eye(&self, car: &CarFrame, ground: Option<f32>) -> Vec3 {
        let mut p = car.centre + self.offset;
        if let Some(g) = ground {
            p.y = p.y.max(g + GROUND_CLEARANCE);
        }
        p
    }

    /// The point the view aims at.
    pub fn target(car: &CarFrame) -> Vec3 {
        car.centre + Vec3::Y * LOOK_LIFT
    }
}

/// The speed-following share: faster cars pull the camera round quicker, up to `cap`.
fn speed_follow(car: &CarFrame, cap: f32) -> f32 {
    let (a, b) = (car.forward_speed.abs(), car.side_speed.abs());
    let speed = a.max(b) + 0.25 * a.min(b);
    (speed / FOLLOW_SPEED).min(cap).max(MIN_SPEED_FOLLOW)
}

/// Vertical field of view (radians) of the engine's driving cameras for a viewport of `aspect`
/// (width / height). The engine's camera value is 37.5 degrees, used as half the horizontal angle
/// of a 4:3 screen (75 degrees across, about 59.8 degrees up and down). Wider screens are taken to
/// keep the vertical angle and see more at the sides, narrower ones to keep the horizontal angle
/// (our choice; the recording is consistent with it).
pub fn engine_vertical_fov(aspect: f32) -> f32 {
    let half_h = 37.5f32.to_radians().tan();
    let half_v = if aspect >= 4.0 / 3.0 { half_h * 0.75 } else { half_h / aspect.max(0.1) };
    2.0 * half_v.atan()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn car(yaw: f32, speed: f32) -> CarFrame {
        CarFrame { centre: Vec3::new(10.0, 2.0, -5.0), rot: Quat::from_rotation_y(yaw), half_length: 2.0, forward_speed: speed, side_speed: 0.0, slope: 0.0 }
    }

    fn settled(view: &ChaseView, c: &CarFrame) -> ChaseCam {
        let mut cam = ChaseCam::start(c);
        for _ in 0..2000 {
            cam.tick(view, c);
        }
        cam
    }

    #[test]
    fn medium_view_sits_behind_and_above() {
        let c = car(0.0, 0.0);
        let cam = settled(&MEDIUM, &c);
        // 4.5 m plus the half length behind (+Z is behind a car facing -Z), 1.35 + 0.25 lift up
        assert!((cam.offset - Vec3::new(0.0, 1.6, 6.5)).length() < 1e-3, "{:?}", cam.offset);
        let eye = cam.eye(&c, None);
        assert!((eye - Vec3::new(10.0, 3.6, 1.5)).length() < 1e-3);
        assert_eq!(ChaseCam::target(&c), Vec3::new(10.0, 3.3, -5.0));
    }

    #[test]
    fn offset_turns_with_the_car() {
        let c = car(std::f32::consts::FRAC_PI_2, 0.0); // facing -X
        let cam = settled(&MEDIUM, &c);
        assert!((cam.offset - Vec3::new(6.5, 1.6, 0.0)).length() < 1e-3, "{:?}", cam.offset);
    }

    #[test]
    fn close_view_is_pushed_out_to_the_minimum_distance() {
        let mut c = car(0.0, 0.0);
        c.half_length = 0.5; // (0, 1.15, 3.8) is 4.0 m long, under the 4.94 m minimum
        let cam = settled(&CLOSE, &c);
        assert!((cam.offset.length() - MIN_DISTANCE).abs() < 1e-3, "{}", cam.offset.length());
    }

    #[test]
    fn slope_lift_is_clamped_per_view() {
        let mut c = car(0.0, 0.0);
        c.slope = 3.0;
        assert!((ChaseCam::wanted(&MEDIUM, &c).y - 2.35).abs() < 1e-5);
        assert!((ChaseCam::wanted(&FAR, &c).y - 3.15).abs() < 1e-5);
        assert!((ChaseCam::wanted(&CLOSE, &c).y - 1.15).abs() < 1e-5);
        c.slope = -3.0;
        assert!((ChaseCam::wanted(&MEDIUM, &c).y - 1.6).abs() < 1e-5);
        assert!((ChaseCam::wanted(&FAR, &c).y - 0.9).abs() < 1e-5);
    }

    #[test]
    fn faster_cars_swing_the_camera_round_quicker() {
        assert!((speed_follow(&car(0.0, 0.0), 0.1) - 0.02).abs() < 1e-6);
        assert!((speed_follow(&car(0.0, 45.0), 0.1) - 0.05).abs() < 1e-6);
        assert!((speed_follow(&car(0.0, 300.0), 0.1) - 0.1).abs() < 1e-6);
        assert!((speed_follow(&car(0.0, 300.0), 0.05) - 0.05).abs() < 1e-6);
        // after a quick turn, a fast car's camera is closer to straight behind than a slow one's
        let mut slow = settled(&MEDIUM, &car(0.0, 5.0));
        let mut fast = slow;
        let (turned_slow, turned_fast) = (car(0.5, 5.0), car(0.5, 60.0));
        for _ in 0..16 {
            slow.tick(&MEDIUM, &turned_slow);
            fast.tick(&MEDIUM, &turned_fast);
        }
        let want = ChaseCam::wanted(&MEDIUM, &turned_fast);
        assert!((fast.offset - want).length() < (slow.offset - want).length());
    }

    #[test]
    fn ground_keeps_the_camera_up() {
        let c = car(0.0, 0.0);
        let cam = settled(&MEDIUM, &c);
        assert!((cam.eye(&c, Some(5.0)).y - 6.0).abs() < 1e-5);
        assert!((cam.eye(&c, Some(0.0)).y - 3.6).abs() < 1e-5);
    }

    #[test]
    fn engine_field_of_view() {
        assert!((engine_vertical_fov(4.0 / 3.0).to_degrees() - 59.84).abs() < 0.02);
        assert!((engine_vertical_fov(16.0 / 9.0) - engine_vertical_fov(4.0 / 3.0)).abs() < 1e-6);
        // a narrower screen keeps the 75-degree horizontal angle
        let v = engine_vertical_fov(1.0);
        assert!(((v / 2.0).tan() - 37.5f32.to_radians().tan()).abs() < 1e-5);
    }
}
