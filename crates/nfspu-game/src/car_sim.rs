//! The retail car model: engine, gearbox, brakes, four tyres with weight transfer, the planar body
//! (velocity and yaw rate), wall response and car-against-car impulses.
//!
//! Everything here is plain arithmetic on `f32` with no clock or randomness from outside, so the
//! same inputs always give the same result. The timing follows the retail game
//! (`docs/provenance/physics.md`): forces are worked out at 32 Hz ([`FORCE_HZ`]) and position and
//! heading move at 64 Hz ([`TICK_HZ`]). Units: metres, seconds, m/s, m/s^2, rpm. Forces are
//! accelerations (m/s^2), as in the retail model; the car's mass only enters through the drive
//! force per lb-ft of torque.
//!
//! Directions: the body frame is (right, forward) in the ground plane. Yaw grows to the left and
//! steering is positive to the left, matching `car.rs`.

use nfspu_formats::carsim::{Induction, SimFile, TORQUE_POINTS, TORQUE_STEP_RPM};
use std::f32::consts::TAU;

/// Position and heading update rate.
pub const TICK_HZ: f32 = 64.0;
/// Force update rate (every second 64 Hz tick).
pub const FORCE_HZ: f32 = 32.0;
/// Gravity used for wheel loads.
pub const G: f32 = 9.8;
/// Gravity while airborne: retail scales g by 1.16 ...
pub const FALL_G: f32 = G * 1.16;
/// ... and adds a pull of 8 x the rear downforce setting per m/s of forward speed, while the car
/// is upright and faster than 5 m/s (an intact rear wing; damage weakens it in retail, not here).
const FALL_DOWNFORCE: f32 = 8.0;
/// Rear-up component above which the car counts as upright for that pull (a guess).
const FALL_UPRIGHT: f32 = 0.5;
/// Yaw rate limit: 0.75 turns per second.
pub const MAX_YAW_RATE: f32 = 0.75 * TAU;

/// Body lean (retail's suspension pass, per force tick): the lengthways input takes 0.175 of the
/// spring value under braking and 0.125 under power, the sideways one 0.25 of the damper value;
/// both lag by 0.8 a tick. The lean settles at about 8 x the input as a height difference over
/// the wheelbase or the track, within the suspension travel at each end. The directions are
/// physical (retail's signs are not resolved) and the track width is a guess.
const LEAN_DIVE: f32 = 0.175;
const LEAN_SQUAT: f32 = 0.125;
const LEAN_SIDE: f32 = 0.25;
const LEAN_LAG: f32 = 0.8;
const LEAN_LEVER: f32 = 8.0;
const LEAN_TRAVEL: f32 = 0.1;
const LEAN_TRACK: f32 = 1.5;

/// Gear slots: 0 reverse, 1 neutral, 2.. first and up (as in the `.sim` ratio table).
pub const REVERSE: usize = 0;
pub const NEUTRAL: usize = 1;
pub const FIRST: usize = 2;

/// Each wheel asks for the force that would cancel its sliding in two force ticks: -v * 32 / 2.
const CANCEL: f32 = 16.0;
/// Axle lever of the yaw rate, as a fraction of the wheelbase per rad/s (retail: 3.2 per turn).
const YAW_LEVER: f32 = 3.2 / TAU;
/// Yaw-rate change per force tick for one m/s^2 of axle force difference, times the wheelbase.
const YAW_STEP: f32 = TAU / (48.0 * 3.2);
/// Tyre classes: how far grip drops once sliding (force falls by 1/k of the limit).
const SLIDE_K: [f32; 3] = [8.0, 8.0, 6.0];
/// Rpm added per force tick while blipping the throttle on a downshift, by target gear slot,
/// with the brake released / pressed.
const BLIP_OPEN: [f32; 8] = [0.0, 0.0, 250.0, 200.0, 175.0, 150.0, 125.0, 0.0];
const BLIP_BRAKING: [f32; 8] = [0.0, 0.0, 200.0, 175.0, 150.0, 125.0, 100.0, 0.0];

/// Tyre compound: 0 road (most cars), 2 race (the GT1, GT2, GT3 and 935).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TyreClass {
    #[default]
    Road,
    Intermediate,
    Race,
}

impl TyreClass {
    fn index(self) -> usize {
        match self {
            TyreClass::Road => 0,
            TyreClass::Intermediate => 1,
            TyreClass::Race => 2,
        }
    }
}

/// Choices that change how a `.sim` file becomes a [`Spec`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpecOptions {
    /// Manual gearbox: retail gives it 5% more drive force.
    pub manual: bool,
    pub tyres: TyreClass,
    /// Replaces the file's front weight share (retail does this for the 959: 0.4675).
    pub front_weight: Option<f32>,
}

impl Default for SpecOptions {
    fn default() -> Self {
        SpecOptions {
            manual: false,
            tyres: TyreClass::Road,
            front_weight: None,
        }
    }
}

impl SpecOptions {
    /// The retail per-car rules, keyed on the car's AI-data name (`nfs5.car` at 0x115): race
    /// tyres for the GT1, GT2, GT3 and 935, and the 959's weight split.
    pub fn for_car(ai_name: &str, manual: bool) -> SpecOptions {
        let n = ai_name.to_ascii_uppercase();
        let tyres = if ["GT1", "GT2", "GT3", "935"].contains(&n.as_str()) {
            TyreClass::Race
        } else {
            TyreClass::Road
        };
        let front_weight = (n == "959").then_some(0.4675);
        SpecOptions { manual, tyres, front_weight }
    }
}

/// A car's simulation constants, worked out once from its `.sim` file.
#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    pub name: String,
    pub mass: f32,
    pub wheelbase: f32,
    pub induction: Induction,
    /// Forward gears; the gear slots run 0..forward_gears + 2.
    pub forward_gears: usize,
    pub shift_ticks: i32,
    pub front_drive: f32,
    /// Engine rpm per m/s of road speed in each gear slot (negative in reverse, 0 in neutral).
    pub rpm_per_ms: [f32; 8],
    /// Road speed per engine rpm (0.1 where the gear is unused).
    pub ms_per_rpm: [f32; 8],
    /// Drive force (m/s^2) per lb-ft of engine torque in each gear slot.
    pub accel_per_lbft: [f32; 8],
    pub top_speed: f32,
    pub redline: f32,
    pub engine_brake: f32,
    pub torque: [f32; TORQUE_POINTS],
    pub front_weight: f32,
    pub grip: f32,
    pub transfer_height: f32,
    pub transfer_stiffness: f32,
    pub lateral_split: f32,
    /// Steering lock in 1/256 turn, toe in steering units.
    pub steer_lock: f32,
    pub toe: f32,
    pub brake_decel: f32,
    pub front_brake: f32,
    /// Body lean per m/s^2 of lengthways and sideways acceleration (the `.sim` spring and damper
    /// values, x 0.001).
    pub lean_spring: f32,
    pub lean_damper: f32,
    /// Extra wheel load per m/s of speed (m/s^2), per front and rear wheel.
    pub downforce_front: f32,
    pub downforce_rear: f32,
    /// Air drag: deceleration = drag * v^3, chosen so that the engine just balances it at the top
    /// speed in top gear.
    pub drag: f32,
    pub circumference: f32,
    pub tyres: TyreClass,
    /// Retail drops the top gear from the automatic's choice when the car cannot reach the
    /// redline speed of the gear below it.
    pub auto_top_slot: usize,
}

impl Spec {
    /// Torque in lb-ft at an engine speed, from the 500 rpm table (clamped to 0..redline).
    pub fn torque_at(&self, rpm: f32) -> f32 {
        let rpm = rpm.clamp(0.0, self.redline);
        let i = ((rpm / TORQUE_STEP_RPM) as usize).min(TORQUE_POINTS - 2);
        let frac = (rpm - i as f32 * TORQUE_STEP_RPM) / TORQUE_STEP_RPM;
        self.torque[i] + (self.torque[i + 1] - self.torque[i]) * frac
    }

    /// Gravity while airborne (m/s^2), at a forward speed and the world-up part of the car's up
    /// axis.
    pub fn fall_accel(&self, v_fwd: f32, up_y: f32) -> f32 {
        let pull = if v_fwd > 5.0 && up_y > FALL_UPRIGHT { FALL_DOWNFORCE * self.downforce_rear * v_fwd } else { 0.0 };
        FALL_G + G * pull
    }

    /// The last forward gear slot.
    pub fn top_slot(&self) -> usize {
        self.forward_gears + 1
    }

    /// Converts a `.sim` file (retail units) into simulation constants. The car-setup sliders of
    /// the front end are taken at their middle positions (a guess: their defaults are not decoded).
    pub fn from_sim(sim: &SimFile, opts: SpecOptions) -> Spec {
        let circumference = sim.tyre_circumference_m();
        let slots = sim.forward_gears + 2;
        let mass = sim.mass_kg.max(1.0);
        let scale = sim.torque_scale_pct * 0.01 * if opts.manual { 1.05 } else { 1.0 };
        let mut rpm_per_ms = [0.0; 8];
        let mut ms_per_rpm = [0.1; 8];
        let mut accel_per_lbft = [0.0; 8];
        for g in 0..slots.min(8) {
            let r = sim.ratios[g] * sim.final_drive * 60.0 / circumference;
            rpm_per_ms[g] = r;
            if r != 0.0 {
                ms_per_rpm[g] = 1.0 / r;
            }
            // retail rounds 2*pi/60 (wheel radius from rpm per m/s) to 0.1
            accel_per_lbft[g] = r / mass * 0.1 * scale;
        }
        // the setup screen's grip trim: retail multiplies grip by (stiffness + 9) / 10
        let grip = sim.grip * (sim.transfer_stiffness + 9.0) * 0.1;
        let top_speed = sim.top_speed_mph * 0.446_428_57;
        let mut spec = Spec {
            name: sim.name.clone(),
            mass,
            wheelbase: sim.wheelbase_mm * 0.001,
            induction: sim.induction,
            forward_gears: sim.forward_gears,
            shift_ticks: sim.shift_ticks,
            front_drive: sim.front_drive_pct * 0.01,
            rpm_per_ms,
            ms_per_rpm,
            accel_per_lbft,
            top_speed,
            redline: sim.redline_rpm,
            engine_brake: sim.engine_brake_pct * 0.01,
            torque: sim.torque_lbft,
            front_weight: opts.front_weight.unwrap_or(sim.front_weight_pct * 0.01),
            grip,
            transfer_height: sim.transfer_height_pct * 0.01,
            transfer_stiffness: sim.transfer_stiffness,
            lateral_split: sim.lateral_split,
            steer_lock: sim.steer_lock,
            toe: sim.toe,
            brake_decel: sim.brake_decel,
            front_brake: sim.front_brake_pct * 0.01,
            downforce_front: sim.downforce_front * 0.001,
            downforce_rear: sim.downforce_rear * 0.001,
            lean_spring: sim.suspension[2] * 0.001,
            lean_damper: sim.suspension[3] * 0.001,
            drag: 0.0,
            circumference,
            tyres: opts.tyres,
            auto_top_slot: 0,
        };
        spec.finish();
        spec
    }

    /// Drag and the automatic's top gear, from the other constants.
    fn finish(&mut self) {
        let top = self.top_slot();
        let rpm = (self.top_speed * self.rpm_per_ms[top]).min(self.redline).max(0.0);
        let v3 = self.top_speed.powi(3).max(1e-3);
        self.drag = self.torque_at(rpm) * self.accel_per_lbft[top] / v3;
        let below = top - 1;
        self.auto_top_slot = if below >= FIRST && self.top_speed < self.redline * self.ms_per_rpm[below] {
            below
        } else {
            top
        };
    }

    /// A made-up mid-range car for tests and for driving without a game install (not any
    /// retail car): 1300 kg, five gears, 250 lb-ft, 240 km/h.
    pub fn generic() -> Spec {
        Spec::from_sim(&generic_file(), SpecOptions::default())
    }
}

/// The `.sim` values of [`Spec::generic`] (made up).
pub fn generic_file() -> SimFile {
    let mut torque = [0.0; TORQUE_POINTS];
    for (i, t) in torque.iter_mut().enumerate().take(15) {
        let rpm = i as f32 * 500.0;
        *t = 160.0 + 90.0 * (1.0 - ((rpm - 4500.0) / 4500.0).powi(2)).max(0.0);
    }
    let sim = SimFile {
        name: "generic".into(),
        mass_kg: 1300.0,
        wheelbase_mm: 2300.0,
        unknown_48: 0,
        induction: Induction::Natural,
        forward_gears: 5,
        shift_ticks: 6,
        torque_scale_pct: 100.0,
        front_drive_pct: 0.0,
        ratios: [-3.5, 0.0, 3.6, 2.1, 1.45, 1.1, 0.88, 0.0],
        final_drive: 3.6,
        tyre_width_mm: 225.0,
        tyre_aspect_pct: 45.0,
        rim_in: 17.0,
        top_speed_mph: 150.0,
        redline_rpm: 7000.0,
        engine_brake_pct: 20.0,
        torque_lbft: torque,
        front_weight_pct: 45.0,
        grip: 1.6,
        transfer_height_pct: 25.0,
        steer_lock: 16.0,
        toe: 0.0,
        brake_decel: 9.0,
        front_brake_pct: 65.0,
        downforce_front: 1.0,
        downforce_rear: 1.0,
        suspension_front_pct: 75.0,
        suspension_rear_pct: 75.0,
        transfer_stiffness: 1.0,
        suspension: [0.0, 100.0, 3.0, 1.5],
        lateral_split: 1.0,
        tail: [1.0; 5],
    };
    sim
}

/// Driver aids and gearbox from the race setup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Aids {
    pub automatic: bool,
    pub abs: bool,
    pub traction_control: bool,
    /// Keyboard steering: the wheel turns at most 12 of 127 units per force tick.
    pub keyboard_steering: bool,
}

impl Default for Aids {
    fn default() -> Self {
        Aids {
            automatic: true,
            abs: true,
            traction_control: false,
            keyboard_steering: true,
        }
    }
}

/// The driver's controls for one force tick. Pedals 0..1, steering -1 (right) .. 1 (left).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Controls {
    pub throttle: f32,
    pub brake: f32,
    pub steer: f32,
    pub handbrake: bool,
    pub gear_up: bool,
    pub gear_down: bool,
}

/// What the force tick needs to know about the ground under the car.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    pub front_grounded: bool,
    pub rear_grounded: bool,
    /// World-up components of the car's forward, right and up axes (slope and camber).
    pub forward_y: f32,
    pub right_y: f32,
    pub up_y: f32,
    /// A wet road (and the car not under cover): the wet grip table applies.
    pub wet: bool,
}

impl Default for Contact {
    fn default() -> Self {
        Contact {
            front_grounded: true,
            rear_grounded: true,
            forward_y: 0.0,
            right_y: 0.0,
            up_y: 1.0,
            wet: false,
        }
    }
}

/// Per-wheel state. Wheels: 0 front left, 1 front right, 2 rear left, 3 rear right.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Wheel {
    /// Grip limit (m/s^2): the wheel's share of the car's weight times the grip.
    pub load: f32,
    /// Skid amount 0..1 (for sounds and marks).
    pub slide: f32,
    /// The wheel is locked under braking this tick.
    pub locked: bool,
    /// Steering angle in radians (front wheels).
    pub angle: f32,
    /// Forces the tyre gives, body frame (m/s^2): right, forward.
    pub force: [f32; 2],
}

/// The engine and gearbox state.
#[derive(Clone, Debug, PartialEq)]
pub struct Drive {
    pub rpm: f32,
    /// Current gear slot (see [`REVERSE`], [`NEUTRAL`], [`FIRST`]).
    pub slot: usize,
    prev_slot: usize,
    requested: usize,
    /// Force ticks left in the current gear change (no drive while above 0).
    pub shift_left: i32,
    downshift: bool,
    /// Rev limiter: force ticks of cut fuel.
    cut: i32,
    /// Automatic: the brake pedal drives backwards after stopping in reverse.
    reverse_mode: bool,
    spin_front: bool,
    spin_rear: bool,
    rng: u32,
}

impl Drive {
    fn new() -> Drive {
        Drive {
            rpm: 0.0,
            slot: NEUTRAL,
            prev_slot: NEUTRAL,
            requested: NEUTRAL,
            shift_left: 0,
            downshift: false,
            cut: 0,
            reverse_mode: false,
            spin_front: false,
            spin_rear: false,
            rng: 0x2545,
        }
    }

    /// A repeatable random number in 0..1 (own generator; retail uses a 16-bit multiplier one).
    fn random(&mut self) -> f32 {
        self.rng = self.rng.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        ((self.rng >> 16) & 0x7fff) as f32 / 32768.0
    }

    fn change_to(&mut self, slot: usize, downshift: bool, shift_ticks: i32) {
        self.prev_slot = self.slot;
        self.slot = slot;
        self.requested = slot;
        self.downshift = downshift;
        self.shift_left = shift_ticks;
    }
}

/// The planar car: velocity, yaw rate, drivetrain and tyres. `car.rs` adds position, height and
/// tilt around it.
#[derive(Clone, Debug, PartialEq)]
pub struct CarSim {
    pub spec: Spec,
    pub aids: Aids,
    /// World velocity in the ground plane (x, z).
    pub vel: [f32; 2],
    /// Yaw rate in rad/s, positive to the left.
    pub yaw_rate: f32,
    pub drive: Drive,
    pub wheels: [Wheel; 4],
    /// Steering in retail units (-127 right .. 127 left) after the keyboard rate limit.
    steer_units: f32,
    /// Last force tick's body accelerations (right, forward), for weight transfer.
    accel: [f32; 2],
    /// Lagged lean inputs (lengthways, sideways) behind the body's pitch and roll.
    lean: [f32; 2],
    /// Effective pedals of the last force tick.
    throttle: f32,
    brake: f32,
    last_up: bool,
    last_down: bool,
    /// On the grid before the start: brake held on, the automatic waits in neutral (the engine
    /// can be revved). Retail puts first gear in at the start (race tick 448).
    pub held: bool,
    was_held: bool,
    /// 64 Hz tick counter (ABS pulses on every fourth).
    tick: u32,
}

fn heading(yaw: f32) -> [f32; 2] {
    [-yaw.sin(), -yaw.cos()]
}

fn right_of(yaw: f32) -> [f32; 2] {
    [yaw.cos(), -yaw.sin()]
}

fn dot(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

/// Rotates a (right, forward) vector into a frame turned `angle` radians to the left.
fn into_frame(v: [f32; 2], angle: f32) -> [f32; 2] {
    let (s, c) = angle.sin_cos();
    [v[0] * c + v[1] * s, -v[0] * s + v[1] * c]
}

fn out_of_frame(v: [f32; 2], angle: f32) -> [f32; 2] {
    let (s, c) = angle.sin_cos();
    [v[0] * c - v[1] * s, v[0] * s + v[1] * c]
}

/// Inputs the tyre model needs besides the wheel itself.
struct TyreCtx {
    throttle: f32,
    throttle_pedal: f32,
    brake_pedal: f32,
    slot: usize,
    v_fwd: f32,
    slip_ratio: f32,
    handbrake: bool,
    abs: bool,
    tcs: bool,
    abs_pulse: bool,
    /// The engine marked wheelspin on this wheel's axle.
    spin_flag: bool,
    /// The engine marked rear wheelspin (retail's lock check looks at this one for every wheel).
    spin_rear: bool,
    k: f32,
    grip: f32,
}

impl CarSim {
    pub fn new(spec: Spec, aids: Aids) -> CarSim {
        CarSim {
            spec,
            aids,
            vel: [0.0; 2],
            yaw_rate: 0.0,
            drive: Drive::new(),
            wheels: [Wheel::default(); 4],
            steer_units: 0.0,
            accel: [0.0; 2],
            lean: [0.0; 2],
            throttle: 0.0,
            brake: 0.0,
            last_up: false,
            last_down: false,
            held: false,
            was_held: false,
            tick: 0,
        }
    }

    /// Body lean from the car's accelerations: (pitch, roll) in radians, nose up and right side
    /// up positive. It dives under braking, squats under power and rolls to the outside of a
    /// turn; it is for looks only and does not change the driving.
    pub fn body_lean(&self) -> (f32, f32) {
        let travel = 2.0 * LEAN_TRAVEL;
        let pitch = (LEAN_LEVER * self.lean[0]).clamp(-travel, travel) / self.spec.wheelbase.max(1.0);
        let side = LEAN_LEVER * (1.0 - 0.2 * self.throttle) * self.lean[1];
        let roll = side.clamp(-travel, travel) / LEAN_TRACK;
        (pitch.atan(), roll.atan())
    }

    /// Forward speed (m/s, negative reversing) for a heading.
    pub fn forward_speed(&self, yaw: f32) -> f32 {
        dot(self.vel, heading(yaw))
    }

    pub fn lateral_speed(&self, yaw: f32) -> f32 {
        dot(self.vel, right_of(yaw))
    }

    /// Ground speed (m/s).
    pub fn speed(&self) -> f32 {
        self.vel[0].hypot(self.vel[1])
    }

    /// The gear for display: -1 reverse, 0 neutral, 1.. forward.
    pub fn gear(&self) -> i32 {
        self.drive.slot as i32 - 1
    }

    /// Puts the gearbox straight into a gear slot with the engine at road speed (tests and
    /// teleports; no gear change time). For a forward speed `v_fwd` in m/s.
    pub fn put_in_gear(&mut self, slot: usize, v_fwd: f32) {
        let slot = slot.min(self.spec.top_slot());
        let d = &mut self.drive;
        d.slot = slot;
        d.prev_slot = slot;
        d.requested = slot;
        d.shift_left = 0;
        d.rpm = (v_fwd * self.spec.rpm_per_ms[slot]).abs();
    }

    /// The forward gear slot that puts the engine nearest to `fraction` of the redline at `v_fwd`.
    pub fn gear_for_speed(&self, v_fwd: f32, fraction: f32) -> usize {
        let want = self.spec.redline * fraction;
        (FIRST..=self.spec.top_slot())
            .filter(|&g| v_fwd * self.spec.rpm_per_ms[g] <= self.spec.redline)
            .min_by(|&a, &b| {
                let ea = (v_fwd * self.spec.rpm_per_ms[a] - want).abs();
                let eb = (v_fwd * self.spec.rpm_per_ms[b] - want).abs();
                ea.total_cmp(&eb)
            })
            .unwrap_or(self.spec.top_slot())
    }

    /// Body accelerations (right, forward) of the last force tick, m/s^2.
    pub fn accel(&self) -> [f32; 2] {
        self.accel
    }

    /// One 64 Hz tick: on even ticks the forces are worked out first. Returns true when it was a
    /// force tick. Position and heading are the caller's (`car.rs` moves them by `vel` and
    /// `yaw_rate` / 64 afterwards).
    pub fn tick(&mut self, yaw: f32, ctl: Controls, contact: Contact) -> bool {
        let force = self.tick % 2 == 0;
        if force {
            self.force_tick(yaw, ctl, contact);
        }
        self.tick = self.tick.wrapping_add(1);
        force
    }

    /// Reads the controls for a force tick: gear requests, the automatic's reverse logic, the
    /// steering rate limit. Returns (throttle, brake) after the reverse swap.
    fn read_controls(&mut self, ctl: &Controls, v_fwd: f32) -> (f32, f32) {
        let d = &mut self.drive;
        if d.shift_left > 0 {
            d.shift_left -= 1;
        }
        let mut throttle = ctl.throttle.clamp(0.0, 1.0);
        let mut brake = ctl.brake.clamp(0.0, 1.0);
        let released = self.was_held && !self.held;
        self.was_held = self.held;
        if self.held {
            brake = 1.0;
        }
        let top = self.spec.top_slot();
        if self.aids.automatic && self.held {
            // waiting for the start in neutral
        } else if self.aids.automatic {
            let still = v_fwd.abs() < 0.2;
            if d.requested == d.slot {
                if d.slot == NEUTRAL && (released || throttle > 0.0) {
                    // retail requests first gear at the start; outside a race (own rule) the
                    // throttle does it
                    d.requested = FIRST;
                } else if still && brake > 0.508 && throttle == 0.0 && d.slot >= FIRST && !d.reverse_mode {
                    d.requested = REVERSE;
                    d.reverse_mode = true;
                } else if still && throttle > 0.508 && d.slot == REVERSE && d.reverse_mode {
                    d.requested = FIRST;
                    d.reverse_mode = false;
                }
            }
            if d.requested != d.slot {
                if d.requested < FIRST {
                    let r = d.requested;
                    d.change_to(r, false, self.spec.shift_ticks);
                } else if d.slot < FIRST {
                    // out of neutral or reverse: the highest gear whose redline speed is below
                    // the current speed (first gear when slow)
                    let mut pick = FIRST;
                    for g in FIRST..=top {
                        if self.spec.redline * self.spec.ms_per_rpm[g] < v_fwd {
                            pick = g;
                        }
                    }
                    d.change_to(pick, false, self.spec.shift_ticks);
                } else {
                    d.requested = d.slot;
                }
            }
            if d.slot == REVERSE && d.reverse_mode {
                std::mem::swap(&mut throttle, &mut brake);
            }
        } else {
            // manual: one slot per press; no downshift while over the redline
            let up = ctl.gear_up && !self.last_up;
            let down = ctl.gear_down && !self.last_down;
            let want = if up && d.slot < top {
                d.slot + 1
            } else if down && d.slot > REVERSE {
                d.slot - 1
            } else {
                d.slot
            };
            if want != d.slot && (d.rpm <= self.spec.redline || want > d.slot) {
                let downshift = want < d.slot && want >= FIRST;
                d.change_to(want, downshift, self.spec.shift_ticks);
            }
        }
        self.last_up = ctl.gear_up;
        self.last_down = ctl.gear_down;
        // steering in retail units, rate limited for keyboards
        let target = (ctl.steer.clamp(-1.0, 1.0) * 127.0).round();
        if self.aids.keyboard_steering {
            self.steer_units += (target - self.steer_units).clamp(-12.0, 12.0);
        } else {
            self.steer_units = target;
        }
        (throttle, brake)
    }

    /// The force tick (32 Hz).
    fn force_tick(&mut self, yaw: f32, ctl: Controls, c: Contact) {
        let h = heading(yaw);
        let r = right_of(yaw);
        let v_fwd = dot(self.vel, h);
        let v_right = dot(self.vel, r);
        let speed = self.speed();
        let (throttle_pedal, brake) = self.read_controls(&ctl, v_fwd);
        let slip_ratio = if v_fwd > 5.0 { v_right / v_fwd } else { 0.0 };
        let spec = self.spec.clone();
        let l = spec.wheelbase.max(0.5);

        // what each axle would need to stop sliding sideways, and every wheel lengthways
        let yaw_k = if speed >= 10.0 {
            (speed + 70.0) * 0.0125
        } else {
            (speed + 30.0) * 0.025
        };
        let base = -CANCEL * v_right;
        let turn = CANCEL * YAW_LEVER * l * self.yaw_rate * yaw_k;
        let want_lat = [base + turn, base + turn, base - turn, base - turn];
        let want_long = -CANCEL * v_fwd;

        // steering angle: the lock shrinks with speed; camber pulls the wheel downhill
        let bank = (G * c.right_y * 8.0).round().clamp(-64.0, 64.0);
        let units = (self.steer_units + bank).clamp(-127.0, 127.0);
        let lock = if speed < 30.0 {
            spec.steer_lock * (1.75 - speed * 0.025)
        } else {
            spec.steer_lock
        }
        .min(20.0);
        let mut steer256 = [(units + spec.toe) / 127.0 * lock, (units - spec.toe) / 127.0 * lock];
        if v_fwd > 80.0 {
            for s in &mut steer256 {
                *s /= v_fwd * 0.0125;
            }
        }
        self.wheels[0].angle = steer256[0] / 256.0 * TAU;
        self.wheels[1].angle = steer256[1] / 256.0 * TAU;
        self.wheels[2].angle = 0.0;
        self.wheels[3].angle = 0.0;

        let mut throttle = throttle_pedal;
        let engine = self.engine(&mut throttle, throttle_pedal, brake, v_fwd, slip_ratio, c.rear_grounded);
        self.throttle = throttle;
        self.brake = brake;
        if !c.front_grounded && !c.rear_grounded {
            for w in &mut self.wheels {
                w.slide = 0.0;
                w.force = [0.0; 2];
            }
            self.accel = [0.0; 2];
            return;
        }

        // brakes: never more than what stops the car in this tick
        let mut braking = spec.brake_decel * brake;
        braking = braking.min(v_fwd.abs() * 32.0);
        if v_fwd > 0.0 {
            braking = -braking;
        }
        let brake_front = braking * spec.front_brake;

        self.update_loads(&spec, c, v_fwd, slip_ratio, throttle);
        let w = &self.wheels;
        let front_sum = w[0].load + w[1].load;
        let rear_sum = w[2].load + w[3].load;
        let share = |load: f32, sum: f32| if sum.abs() > 1e-6 { load / sum } else { 0.5 };
        let drive = [
            engine * spec.front_drive * share(w[0].load, front_sum) + brake_front * 0.5,
            engine * spec.front_drive * share(w[1].load, front_sum) + brake_front * 0.5,
            engine * (1.0 - spec.front_drive) * share(w[2].load, rear_sum) + (braking - brake_front) * 0.5,
            engine * (1.0 - spec.front_drive) * share(w[3].load, rear_sum) + (braking - brake_front) * 0.5,
        ];

        let k = SLIDE_K[spec.tyres.index()];
        let abs_pulse = self.tick % 4 == 0;
        for i in 0..4 {
            let front = i < 2;
            if (front && !c.front_grounded) || (!front && !c.rear_grounded) {
                self.wheels[i].force = [0.0; 2];
                self.wheels[i].slide = 0.0;
                continue;
            }
            let ctx = TyreCtx {
                throttle,
                throttle_pedal,
                brake_pedal: brake,
                slot: self.drive.slot,
                v_fwd,
                slip_ratio,
                handbrake: ctl.handbrake,
                abs: self.aids.abs,
                tcs: self.aids.traction_control,
                abs_pulse,
                spin_flag: if front { self.drive.spin_front } else { self.drive.spin_rear },
                spin_rear: self.drive.spin_rear,
                k,
                grip: spec.grip,
            };
            tyre(&mut self.wheels[i], front, want_lat[i], want_long, drive[i], &ctx);
        }

        let w = &self.wheels;
        let lat: f32 = w.iter().map(|w| w.force[0]).sum();
        let long_scaled: f32 = w.iter().map(|w| w.force[1]).sum();
        let mut a_fwd = long_scaled / spec.grip;
        let mut a_right = lat;
        let lat_front = w[0].force[0] + w[1].force[0];
        let lat_rear = w[2].force[0] + w[3].force[0];
        let long_left = w[0].force[1] + w[2].force[1];
        let long_right = w[1].force[1] + w[3].force[1];
        // slope pull (retail: half g lengthways unless braking, two g sideways when sliding or tilted)
        if brake < 0.254 {
            a_fwd += -4.9 * c.forward_y;
        }
        if v_right.abs() > 1.0 || c.up_y < 0.833 {
            a_right += -19.6 * c.right_y;
        }
        self.accel = [a_right, a_fwd];
        let dive = if a_fwd <= 0.0 { LEAN_DIVE } else { LEAN_SQUAT };
        self.lean[0] = spec.lean_spring * a_fwd * dive + LEAN_LAG * self.lean[0];
        self.lean[1] = spec.lean_damper * a_right * LEAN_SIDE + LEAN_LAG * self.lean[1];
        // a front force to the right turns the nose right (negative yaw), a rear one the other
        // way. The left/right difference of the lengthways forces counts with retail's sign: the
        // wheels that gain load (the outside ones) get more of the drive, and that turns the car
        // OUT of the corner, so power steadies it (the opposite of a real car; verified in the
        // decompiled yaw sum, and taking the physical sign makes every powerful car spin)
        let yaw_moment = -(lat_front - lat_rear) + (long_left - long_right);
        self.vel[0] += (h[0] * a_fwd + r[0] * a_right) / FORCE_HZ;
        self.vel[1] += (h[1] * a_fwd + r[1] * a_right) / FORCE_HZ;
        self.yaw_rate = (self.yaw_rate + YAW_STEP / l * yaw_moment).clamp(-MAX_YAW_RATE, MAX_YAW_RATE);

        // retail's low-speed and neutral damping
        let slot = self.drive.slot;
        if slot >= FIRST && throttle < 0.25 && speed < 30.0 && spec.grip > 2.5 {
            let f = 1.0 - (self.yaw_rate / TAU).abs() * 0.05;
            self.vel = [self.vel[0] * f, self.vel[1] * f];
            return;
        }
        if slot == NEUTRAL && (G * c.forward_y).abs() < 0.5 {
            let f = if v_fwd.abs() >= 20.0 && self.steer_units.abs() <= 32.0 {
                0.998
            } else {
                0.99
            };
            self.vel = [self.vel[0] * f, self.vel[1] * f];
            self.yaw_rate *= f;
            return;
        }
        if ctl.handbrake && speed < 1.0 {
            self.vel = [self.vel[0] * 0.5, self.vel[1] * 0.5];
        }
    }

    /// Wheel loads from weight, the last tick's accelerations and downforce.
    fn update_loads(&mut self, spec: &Spec, c: Contact, v_fwd: f32, slip_ratio: f32, throttle: f32) {
        let weight = c.up_y * spec.grip * G;
        let surface = crate::weather::asphalt_grip(spec.tyres.index(), c.wet);
        let front = surface * spec.front_weight * weight * 0.5;
        let rear = surface * (1.0 - spec.front_weight) * weight * 0.5;
        let mut loads = [front, front, rear, rear];
        let k = spec.transfer_height * c.up_y * spec.grip * 0.2 / spec.transfer_stiffness.max(0.01);
        let mut shift = self.accel[1] * k;
        if throttle > 0.0 && v_fwd > 0.0 {
            shift *= (1.0 - slip_ratio.abs() * 10.0).max(0.0);
        }
        loads[0] -= shift;
        loads[1] -= shift;
        loads[2] += shift;
        loads[3] += shift;
        // cornering moves weight to the outside wheels
        let side = -self.accel[0] * k;
        let split = spec.lateral_split.max(0.01);
        let (left, right) = if side <= 0.0 { (-side, side / split) } else { (-side / split, side) };
        loads[0] += left;
        loads[2] += left;
        loads[1] += right;
        loads[3] += right;
        if v_fwd > 5.0 {
            loads[0] += spec.downforce_front * v_fwd;
            loads[1] += spec.downforce_front * v_fwd;
            loads[2] += spec.downforce_rear * v_fwd;
            loads[3] += spec.downforce_rear * v_fwd;
        }
        for (w, l) in self.wheels.iter_mut().zip(loads) {
            w.load = l;
        }
    }

    /// Engine and gearbox for one force tick: moves the rpm and returns the drive force (m/s^2,
    /// air drag included). `throttle` is zeroed while the rev limiter cuts in.
    fn engine(&mut self, throttle: &mut f32, pedal: f32, brake: f32, v_fwd: f32, slip_ratio: f32, rear_grounded: bool) -> f32 {
        let spec = &self.spec;
        let redline = spec.redline;
        let drag = spec.drag * v_fwd * v_fwd * v_fwd;
        let d = &mut self.drive;
        let free = d.slot == NEUTRAL || !rear_grounded;
        let mut target = (redline + if free { 250.0 } else { 100.0 }) * *throttle;
        if d.rpm >= redline && d.rpm < redline + 250.0 {
            d.rpm = redline + 50.0;
            let roll = d.random();
            d.cut = if d.slot < FIRST || !rear_grounded {
                2 + (roll * 2.0) as i32
            } else {
                (roll * 2.0) as i32 + d.slot as i32 * 3
            };
        }
        if d.cut > 0 {
            *throttle = 0.0;
            target = 0.0;
            d.cut -= 1;
        }
        d.spin_front = false;
        d.spin_rear = false;

        if d.slot == NEUTRAL || d.shift_left > 0 || !rear_grounded {
            if target <= d.rpm || d.shift_left > 0 {
                if d.shift_left < 1 || d.prev_slot == NEUTRAL {
                    if d.rpm >= target {
                        d.rpm = (d.rpm - 200.0).max(target);
                    }
                } else if !d.downshift {
                    d.rpm = (d.rpm - if d.slot < 4 { 100.0 } else { 200.0 }).max(0.0);
                } else {
                    let blip = if brake < 0.258 { BLIP_OPEN } else { BLIP_BRAKING };
                    d.rpm = (d.rpm + blip[d.slot.min(7)]).min(redline);
                }
            } else {
                d.rpm = (d.rpm + 250.0).min(target);
            }
            return -drag;
        }

        if self.aids.automatic {
            self.auto_shift(*throttle, v_fwd, slip_ratio);
        }
        let spec = &self.spec;
        let d = &mut self.drive;
        let slot = d.slot;
        let wheel_rpm = spec.rpm_per_ms[slot] * v_fwd;
        let full = spec.torque_at(d.rpm) * spec.accel_per_lbft[slot];
        let mut gap = target - wheel_rpm.abs();
        if gap.abs() < 125.0 && target <= redline - 300.0 {
            gap = 0.0;
        }
        let slip_rpm = d.rpm - wheel_rpm.abs();
        let clutch = (slip_rpm > 250.0 && d.shift_left == 0) || (slot >= FIRST && v_fwd < -0.1) || (slot == REVERSE && v_fwd > 0.1);
        let loads: Vec<f32> = self.wheels.iter().map(|w| w.load).collect();
        let mut force;
        if clutch {
            // the clutch slips: full torque, the engine is pulled down toward the wheels as far
            // as the grip allows
            force = full;
            let demand = (15.0 * full).abs();
            let grip_room = if spec.front_drive <= 0.0 {
                100.0 - (demand - (loads[2] + loads[3]))
            } else {
                120.0 - (demand - loads.iter().sum::<f32>())
            };
            let max_drop = (1.0 - *throttle) * 100.0 + grip_room / pedal.max(0.3);
            if spec.shift_ticks < 5 && slot > FIRST {
                d.rpm -= slip_rpm / 6.0;
            }
            if *throttle < 0.5 {
                d.rpm -= slip_rpm / 6.0;
            } else {
                if slip_rpm > redline * 0.125 {
                    d.spin_front = spec.front_drive > 0.0;
                    d.spin_rear = true;
                }
                d.rpm -= slip_rpm.min(max_drop);
            }
        } else if gap >= 0.0 {
            if gap == 0.0 {
                // cruising at the pedal's speed: the engine just holds the car against drag
                d.rpm = wheel_rpm;
                force = drag;
            } else {
                if slip_rpm > 200.0 {
                    d.rpm -= 200.0;
                } else if slip_rpm < -200.0 {
                    d.rpm += 200.0;
                } else {
                    d.rpm = wheel_rpm;
                }
                force = full * *throttle;
                d.rpm = d.rpm.min(target);
            }
        } else {
            // the wheels turn the engine faster than the pedal asks: engine braking
            let step = spec.ms_per_rpm[slot] * 81920.0;
            let mut rise = wheel_rpm.abs() - d.rpm;
            if slot == REVERSE {
                rise = rise.max(step);
            } else {
                rise = rise.min(step);
            }
            if wheel_rpm.abs() <= redline + 500.0 {
                let held = if d.cut > 0 { full * 4.0 } else { full };
                force = -(held * spec.engine_brake);
                d.rpm += rise;
                if d.rpm <= target {
                    d.rpm = target;
                }
            } else {
                force = -full;
                d.rpm += rise;
            }
        }
        if d.rpm >= 0.0 {
            d.rpm = d.rpm.min(redline * 1.25);
        } else {
            let stop = -v_fwd * 32.0;
            let pushing_on = (force > 0.0 && stop > 0.0 && force - stop > 0.0) || (force < 0.0 && stop < 0.0 && force - stop < 0.0);
            if !pushing_on {
                d.rpm = 0.0;
                force = stop;
            }
        }
        force - drag
    }

    /// The automatic gearbox (retail rules): up near the redline, down when the lower gear would
    /// stay under 80% of it.
    fn auto_shift(&mut self, throttle: f32, v_fwd: f32, slip_ratio: f32) {
        let spec = &self.spec;
        let d = &mut self.drive;
        let slot = d.slot;
        if slot < FIRST {
            return;
        }
        let top = spec.auto_top_slot;
        let up = if slot < top { slot + 1 } else { slot };
        let down = if slot > FIRST { slot - 1 } else { slot };
        let v = v_fwd;
        let mut rpm = v * spec.rpm_per_ms[slot];
        if d.rpm < rpm * 1.2 {
            rpm = d.rpm;
        }
        let rpm_down = v * spec.rpm_per_ms[down];
        let low = spec.redline * 0.8;
        let ticks = spec.shift_ticks;
        if throttle <= 0.5 && rpm.abs() <= spec.redline {
            if rpm_down < low && down >= FIRST && slip_ratio.abs() < 0.1 && down != slot {
                d.change_to(down, true, ticks);
            }
            return;
        }
        let point = spec.redline * (0.9 + 0.1 * d.random());
        if rpm <= point || slot >= top {
            if rpm_down < low && down != slot {
                d.change_to(down, true, ticks);
            }
        } else if up != slot {
            d.change_to(up, false, ticks);
        }
    }
}

/// One tyre: turns the wanted forces and the drive/brake force into what the tyre can give.
fn tyre(w: &mut Wheel, front: bool, want_lat: f32, want_long: f32, drive: f32, c: &TyreCtx) {
    let load = w.load;
    let [lat_w, long_w] = into_frame([want_lat, want_long], w.angle);
    w.locked = false;
    let mut f = drive;
    let mut braking = false;
    if f >= 0.0 || long_w >= 0.0 {
        if f > 0.0 && long_w > 0.0 && (c.throttle <= 0.25 || c.slot < FIRST) {
            f = f.min(long_w);
            braking = true;
        }
    } else if c.throttle <= 0.25 || c.slot != REVERSE {
        f = long_w.max(f);
        braking = true;
    }
    f *= c.grip;

    let mut lock = false;
    let over = braking && f.abs() > load;
    let handbrake_rear = c.handbrake && !front && c.v_fwd.abs() > 0.5;
    if over || handbrake_rear {
        if c.handbrake {
            lock = true;
        } else if !c.abs && c.v_fwd <= 2.0 * (16.0 - load) && c.brake_pedal > 235.0 / 252.0 && (c.v_fwd.abs() >= 5.0 || c.spin_rear) {
            lock = true;
        }
        if !lock {
            f = f.clamp(-load, load);
            if c.abs && c.v_fwd < 25.0 && c.abs_pulse {
                lock = true;
            }
        }
    }
    if lock {
        // a sliding (locked) tyre pushes against the slide with its kinetic grip
        let m = lat_w.abs().max(long_w.abs()) + 0.25 * lat_w.abs().min(long_w.abs());
        w.slide = if m > load {
            ((w.slide * 3.0 + (m - load)) * 0.25 * 0.1).min(1.0)
        } else {
            0.0
        };
        let s = if m > 0.01 { (load - load / c.k) / m } else { 0.0 };
        w.force = out_of_frame([s * lat_w, s * long_w], w.angle);
        w.locked = true;
        return;
    }

    // sideways: the slip angle, in 1/256 turn (retail doubles the sideways part)
    let slip = if long_w == 0.0 {
        0.0
    } else {
        let a = lat_w.atan2(-0.5 * long_w) / TAU;
        if long_w > 0.0 && lat_w > 0.0 {
            (0.5 - a) * 256.0
        } else if long_w > 0.0 && lat_w < 0.0 {
            (-0.5 - a) * 256.0
        } else {
            a * 256.0
        }
    };
    let capped = |mag: f32| if lat_w <= 0.0 { (-mag).max(lat_w) } else { mag.min(lat_w) };
    let lat = if !front {
        capped(slip.abs().clamp(1.0, 2.0) * load * 0.5)
    } else {
        let mag = slip.abs().min(16.0) * load / 12.0;
        if lat_w.abs() + long_w.abs() < 32.0 {
            capped(mag)
        } else {
            mag.copysign(slip)
        }
    };
    let mut out = [lat, f];

    // the friction circle: lengthways counts 0.9, the smaller part a quarter
    let a = (out[1] * 0.9).abs();
    let b = out[0].abs();
    let m = a.max(b) + 0.25 * a.min(b);
    let spin = (out[1].abs() > load && c.throttle_pedal > 128.0 / 252.0) || c.spin_flag;
    let k = if !front && spin { 4.0 } else { c.k };
    if m <= load {
        w.slide = 0.0;
    } else if !c.tcs || front || c.slip_ratio.abs() >= 0.15 {
        let drop = ((m - load) / k).min(load / k);
        let s = (load - drop) / m;
        w.slide = (w.slide * 3.0 + (m - load) * if front { 0.5 } else { 12.0 }) * 0.25;
        out = [out[0] * s, out[1] * s];
    } else {
        // traction control (rear wheels): trim the drive to the limit without sliding
        w.slide = 0.0;
        if c.slot > FIRST {
            out[0] *= load / m;
        }
        out[1] *= load / m;
    }
    if !front && spin && !c.tcs {
        w.slide = m;
    }
    w.slide = w.slide.min(1.0);
    if c.tcs {
        w.slide *= 0.5;
    }
    w.force = out_of_frame(out, w.angle);
}

/// Below this speed into a wall (m/s) the wall does not swing the car round (own rule).
pub const WALL_SWING_MIN: f32 = 1.0;

/// Retail's wall response for a hit with outward normal `n` (unit, ground plane, pointing away
/// from the wall). Changes the velocity and yaw rate in place and returns the impact speed into
/// the wall (0 when the car is moving away). Push the car out by 1.05 times the overlap as well.
pub fn wall_response(sim: &mut CarSim, yaw: f32, n: [f32; 2]) -> f32 {
    let into = -dot(sim.vel, n);
    if into <= 0.0 {
        return 0.0;
    }
    // the speed into the wall is removed and 10% of it comes back
    sim.vel[0] += n[0] * into * 1.1;
    sim.vel[1] += n[1] * into * 1.1;
    let v_fwd = sim.forward_speed(yaw);
    // retail swings the car round on every touch; ours skips gentle pressing (under 1 m/s), or a
    // car pushing into the corner of two of our mesh walls could never steer out (own rule)
    if (into < 15.0 || v_fwd < 20.0) && into > WALL_SWING_MIN {
        // the car swings round to run along the wall
        let c = dot(n, heading(yaw));
        let s = dot(n, right_of(yaw));
        let m = c.abs().min(s.abs()) * 0.25 * TAU;
        sim.yaw_rate = if c < 0.0 {
            if s < 0.0 {
                m
            } else {
                -m
            }
        } else if s < 0.0 {
            -m * 0.5
        } else {
            m * 0.5
        };
    }
    sim.vel = [sim.vel[0] * 0.998, sim.vel[1] * 0.998];
    into
}

/// A car's collision box and motion, for [`collide`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    pub pos: [f32; 2],
    pub yaw: f32,
    /// Half length (forward), half width (sideways) and half height of the box, metres.
    pub half: [f32; 3],
}

impl Body {
    /// Retail mass for impulses: the box volume over two (it ignores the `.sim` mass).
    fn mass(&self) -> f32 {
        4.0 * self.half[0] * self.half[1] * self.half[2]
    }

    fn corners(&self) -> [[f32; 2]; 4] {
        let h = heading(self.yaw);
        let r = right_of(self.yaw);
        let (l, w) = (self.half[0], self.half[1]);
        let at = |a: f32, b: f32| [self.pos[0] + h[0] * a + r[0] * b, self.pos[1] + h[1] * a + r[1] * b];
        [at(l, w), at(l, -w), at(-l, -w), at(-l, w)]
    }
}

/// Overlap of two boxes in the ground plane: (normal from `b` toward `a`, depth, contact point).
fn overlap(a: &Body, b: &Body) -> Option<([f32; 2], f32, [f32; 2])> {
    let axes = [heading(a.yaw), right_of(a.yaw), heading(b.yaw), right_of(b.yaw)];
    let ca = a.corners();
    let cb = b.corners();
    let mut best: Option<([f32; 2], f32)> = None;
    for ax in axes {
        let pa: Vec<f32> = ca.iter().map(|p| dot(*p, ax)).collect();
        let pb: Vec<f32> = cb.iter().map(|p| dot(*p, ax)).collect();
        let (amin, amax) = pa.iter().fold((f32::MAX, f32::MIN), |m, &v| (m.0.min(v), m.1.max(v)));
        let (bmin, bmax) = pb.iter().fold((f32::MAX, f32::MIN), |m, &v| (m.0.min(v), m.1.max(v)));
        let depth = amax.min(bmax) - amin.max(bmin);
        if depth <= 0.0 {
            return None;
        }
        if best.is_none_or(|(_, d)| depth < d) {
            let towards_a = dot([a.pos[0] - b.pos[0], a.pos[1] - b.pos[1]], ax) >= 0.0;
            best = Some((if towards_a { ax } else { [-ax[0], -ax[1]] }, depth));
        }
    }
    let (n, depth) = best?;
    // contact: the deepest corner of either box inside the other
    let inside = |p: &[f32; 2], o: &Body| {
        let d = [p[0] - o.pos[0], p[1] - o.pos[1]];
        dot(d, heading(o.yaw)).abs() <= o.half[0] && dot(d, right_of(o.yaw)).abs() <= o.half[1]
    };
    let pts: Vec<[f32; 2]> = ca
        .iter()
        .filter(|p| inside(p, b))
        .chain(cb.iter().filter(|p| inside(p, a)))
        .copied()
        .collect();
    let point = if pts.is_empty() {
        [(a.pos[0] + b.pos[0]) * 0.5, (a.pos[1] + b.pos[1]) * 0.5]
    } else {
        let s = pts.iter().fold([0.0, 0.0], |s, p| [s[0] + p[0], s[1] + p[1]]);
        [s[0] / pts.len() as f32, s[1] / pts.len() as f32]
    };
    Some((n, depth, point))
}

/// Car against car (retail impulse, planar). Pushes the bodies apart and changes both cars'
/// velocities and yaw rates. Returns the impact strength (retail scale; 80 and up is a big hit)
/// or `None` when the boxes do not touch.
pub fn collide(a: &mut CarSim, ab: &mut Body, b: &mut CarSim, bb: &mut Body) -> Option<f32> {
    let (n, depth, p) = overlap(ab, bb)?;
    let (ma, mb) = (ab.mass(), bb.mass());
    let (ia, ib) = (1.0 / ma, 1.0 / mb);
    let (ja, jb) = (ia / 6.0, ib / 6.0);
    // separate in proportion to the other car's mass (own rule; retail re-checks walls)
    let share_a = mb / (ma + mb);
    ab.pos[0] += n[0] * depth * share_a;
    ab.pos[1] += n[1] * depth * share_a;
    bb.pos[0] -= n[0] * depth * (1.0 - share_a);
    bb.pos[1] -= n[1] * depth * (1.0 - share_a);
    let ra = [p[0] - ab.pos[0], p[1] - ab.pos[1]];
    let rb = [p[0] - bb.pos[0], p[1] - bb.pos[1]];
    // planar cross product (ground plane, yaw about the vertical): r x n
    let cross = |r: [f32; 2], v: [f32; 2]| r[0] * v[1] - r[1] * v[0];
    // velocity of a point from yaw: yaw is positive to the left (counter-clockwise seen from
    // above in x/z with -z forward), so the point moves by w x r
    let spin_vel = |w: f32, r: [f32; 2]| [w * r[1], -w * r[0]];
    let va = {
        let s = spin_vel(a.yaw_rate, ra);
        [a.vel[0] + s[0], a.vel[1] + s[1]]
    };
    let vb = {
        let s = spin_vel(b.yaw_rate, rb);
        [b.vel[0] + s[0], b.vel[1] + s[1]]
    };
    let closing = dot(vb, n) - dot(va, n);
    if closing < 0.0 {
        return Some(0.0);
    }
    let (xa, xb) = (cross(ra, n), cross(rb, n));
    let j = closing / (0.5 * ia + 0.5 * ib + 0.5 * jb * xb * xb + 0.5 * ja * xa * xa);
    let impulse = j * 0.5;
    let rel = [a.vel[0] - b.vel[0], a.vel[1] - b.vel[1]];
    let strength = ((ma + mb) * (dot(a.vel, n) - dot(b.vel, n)) * ia).abs().min(4.0 * rel[0].hypot(rel[1]));
    let boost = if strength >= 80.0 { 1.25 } else { 1.0 };
    let spin = if strength > 80.0 { 3.0 } else { 1.5 };
    a.vel[0] += n[0] * impulse * ia * boost;
    a.vel[1] += n[1] * impulse * ia * boost;
    b.vel[0] -= n[0] * impulse * ib * boost;
    b.vel[1] -= n[1] * impulse * ib * boost;
    // yaw from the off-centre hit; sign: a push along n at ra turns the car by -(ra x n) in this
    // left-positive yaw convention
    a.yaw_rate = (a.yaw_rate - xa * impulse * ja * spin).clamp(-MAX_YAW_RATE, MAX_YAW_RATE);
    b.yaw_rate = (b.yaw_rate + xb * impulse * jb * spin).clamp(-MAX_YAW_RATE, MAX_YAW_RATE);
    Some(strength)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(sim: &mut CarSim, ctl: Controls, seconds: f32) {
        for _ in 0..(seconds * TICK_HZ) as usize {
            sim.tick(0.0, ctl, Contact::default());
        }
    }

    fn full_throttle() -> Controls {
        Controls {
            throttle: 1.0,
            ..Default::default()
        }
    }

    #[test]
    fn torque_table_interpolates_and_clamps() {
        let s = Spec::generic();
        let t0 = s.torque[8];
        let t1 = s.torque[9];
        assert!((s.torque_at(4250.0) - (t0 + t1) / 2.0).abs() < 1e-3);
        assert_eq!(s.torque_at(-100.0), s.torque[0]);
        assert_eq!(s.torque_at(99999.0), s.torque_at(s.redline));
    }

    #[test]
    fn drag_balances_the_engine_at_top_speed() {
        let s = Spec::generic();
        let top = s.top_slot();
        let rpm = (s.top_speed * s.rpm_per_ms[top]).min(s.redline);
        let engine = s.torque_at(rpm) * s.accel_per_lbft[top];
        assert!((s.drag * s.top_speed.powi(3) - engine).abs() < 1e-3);
    }

    #[test]
    fn manual_gets_five_percent_more_drive() {
        let auto = Spec::from_sim(&generic_file(), SpecOptions::default());
        let manual = Spec::from_sim(
            &generic_file(),
            SpecOptions {
                manual: true,
                ..Default::default()
            },
        );
        assert!((manual.accel_per_lbft[FIRST] / auto.accel_per_lbft[FIRST] - 1.05).abs() < 1e-4);
    }

    #[test]
    fn per_car_rules() {
        assert_eq!(SpecOptions::for_car("gt2", false).tyres, TyreClass::Race);
        assert_eq!(SpecOptions::for_car("959", true).front_weight, Some(0.4675));
        assert_eq!(SpecOptions::for_car("993T", false), SpecOptions::default());
    }

    #[test]
    fn automatic_pulls_away_and_shifts_up() {
        let mut sim = CarSim::new(Spec::generic(), Aids::default());
        run(&mut sim, full_throttle(), 0.5);
        assert_eq!(sim.drive.slot, FIRST, "first gear after half a second");
        assert!(sim.forward_speed(0.0) > 1.0);
        let mut top_gear = 0;
        for _ in 0..(20.0 * TICK_HZ) as usize {
            sim.tick(0.0, full_throttle(), Contact::default());
            top_gear = top_gear.max(sim.gear());
            assert!(sim.drive.rpm <= sim.spec.redline * 1.25 + 1.0);
        }
        assert!(top_gear >= 4, "reached gear {top_gear}");
        assert!(sim.forward_speed(0.0) > 40.0, "speed {}", sim.forward_speed(0.0));
    }

    #[test]
    fn top_speed_is_close_to_the_spec() {
        let mut sim = CarSim::new(Spec::generic(), Aids::default());
        run(&mut sim, full_throttle(), 90.0);
        let v = sim.forward_speed(0.0);
        let top = sim.spec.top_speed;
        assert!(v > top * 0.93 && v < top * 1.02, "v {v} top {top}");
    }

    #[test]
    fn braking_stops_the_car_in_a_straight_line() {
        let mut sim = CarSim::new(Spec::generic(), Aids::default());
        sim.vel = [0.0, -30.0];
        sim.drive.slot = 4;
        let mut t = 0.0;
        while sim.forward_speed(0.0) > 0.1 && t < 10.0 {
            sim.tick(
                0.0,
                Controls {
                    brake: 1.0,
                    ..Default::default()
                },
                Contact::default(),
            );
            t += 1.0 / TICK_HZ;
        }
        // 9 m/s^2 brakes from 30 m/s: a bit over 3.3 s if the tyres hold
        assert!(t > 2.8 && t < 5.0, "stopped after {t} s");
        assert!(sim.vel[0].abs() < 0.2, "drifted sideways {:?}", sim.vel);
    }

    #[test]
    fn manual_starts_in_neutral_and_needs_a_gear() {
        let aids = Aids {
            automatic: false,
            ..Default::default()
        };
        let mut sim = CarSim::new(Spec::generic(), aids);
        run(&mut sim, full_throttle(), 1.0);
        assert_eq!(sim.gear(), 0);
        assert!(sim.forward_speed(0.0).abs() < 0.01);
        assert!(sim.drive.rpm > 5000.0, "revs freely in neutral: {}", sim.drive.rpm);
        // one press of gear up
        sim.tick(
            0.0,
            Controls {
                throttle: 1.0,
                gear_up: true,
                ..Default::default()
            },
            Contact::default(),
        );
        sim.tick(
            0.0,
            Controls {
                throttle: 1.0,
                gear_up: true,
                ..Default::default()
            },
            Contact::default(),
        );
        assert_eq!(sim.gear(), 1);
        run(&mut sim, full_throttle(), 2.0);
        assert!(sim.forward_speed(0.0) > 5.0);
        assert_eq!(sim.gear(), 1, "a manual stays in its gear");
    }

    #[test]
    fn steering_left_turns_left_and_settles() {
        let mut sim = CarSim::new(Spec::generic(), Aids::default());
        sim.vel = [0.0, -20.0];
        sim.drive.slot = 3;
        let mut yaw = 0.0f32;
        for _ in 0..(3.0 * TICK_HZ) as usize {
            sim.tick(
                yaw,
                Controls {
                    throttle: 0.4,
                    steer: 0.5,
                    ..Default::default()
                },
                Contact::default(),
            );
            yaw += sim.yaw_rate / TICK_HZ;
        }
        assert!(sim.yaw_rate > 0.1, "yaw rate {}", sim.yaw_rate);
        assert!(yaw > 0.5, "yaw {yaw}");
        let lateral_g = sim.accel()[0].abs() / G;
        assert!(lateral_g < sim.spec.grip * 1.2, "lateral {lateral_g} g");
    }

    #[test]
    fn wall_hit_keeps_the_sliding_speed_and_bounces_a_little() {
        let mut sim = CarSim::new(Spec::generic(), Aids::default());
        // moving forward (-z) and to the right (+x) into a wall facing -x
        sim.vel = [10.0, -30.0];
        let into = wall_response(&mut sim, 0.0, [-1.0, 0.0]);
        assert!((into - 10.0).abs() < 1e-4);
        assert!((sim.vel[0] - -1.0 * 0.998).abs() < 1e-3, "bounce {:?}", sim.vel);
        assert!((sim.vel[1] - -30.0 * 0.998).abs() < 1e-3);
        assert_eq!(sim.yaw_rate, 0.0, "running parallel to the wall: no turn");
        assert_eq!(wall_response(&mut sim, 0.0, [-1.0, 0.0]), 0.0, "moving away: no second hit");
        // nose first at an angle (heading turned right toward the wall): swings left, away from it
        let mut sim = CarSim::new(Spec::generic(), Aids::default());
        sim.vel = [8.0, -15.0];
        wall_response(&mut sim, -0.3, [-1.0, 0.0]);
        assert!(sim.yaw_rate > 0.0, "turns left: {}", sim.yaw_rate);
    }

    #[test]
    fn cars_bump_and_part() {
        let mut a = CarSim::new(Spec::generic(), Aids::default());
        let mut b = CarSim::new(Spec::generic(), Aids::default());
        a.vel = [0.0, -30.0];
        b.vel = [0.0, -20.0];
        let mut ab = Body {
            pos: [0.0, -4.2],
            yaw: 0.0,
            half: [2.2, 0.9, 0.65],
        };
        let mut bb = Body {
            pos: [0.0, -8.3],
            yaw: 0.0,
            half: [2.2, 0.9, 0.65],
        };
        let hit = collide(&mut a, &mut ab, &mut b, &mut bb).expect("touching");
        assert!(hit > 0.0);
        assert!(a.vel[1] > -30.0 && b.vel[1] < -20.0, "a {:?} b {:?}", a.vel, b.vel);
        let total = a.vel[1] + b.vel[1];
        assert!((total - -50.0).abs() < 0.5, "momentum kept (equal boxes): {total}");
        assert!((ab.pos[1] - bb.pos[1]).abs() >= 4.4 - 1e-3, "pushed apart");
        let far = Body { pos: [50.0, 0.0], ..bb };
        assert!(collide(&mut a, &mut ab, &mut b, &mut far.clone()).is_none());
    }

    #[test]
    fn deterministic() {
        let mut a = CarSim::new(Spec::generic(), Aids::default());
        let mut b = a.clone();
        let ctl = Controls {
            throttle: 1.0,
            steer: 0.3,
            ..Default::default()
        };
        run(&mut a, ctl, 10.0);
        run(&mut b, ctl, 10.0);
        assert_eq!(a, b);
    }
}

