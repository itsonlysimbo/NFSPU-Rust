//! The player's (and any AI's) car: the retail driving model of `car_sim.rs` plus height, tilt and
//! collision against the track. Inputs in ([`Input`]), state out (`pos`, `yaw`, `speed`,
//! [`Car::rpm`], [`Car::gear`]). The model runs on the retail clock: [`Car::step`] adds up frame
//! time and runs whole 64 Hz ticks (forces at 32 Hz). World units are metres.

use crate::car_damage::{self, Damage};
use crate::car_sim::{self, Aids, Body, CarSim, Contact, Controls, Spec, TICK_HZ};
use crate::collision::WallSet;
use crate::ground::Ground;
use bevy::math::Vec3;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Input {
    pub throttle: bool,
    pub brake: bool,
    pub handbrake: bool,
    /// -1.0 (right) .. 1.0 (left)
    pub steer: f32,
    /// Manual gearbox: one gear per press (holding counts as one press).
    pub gear_up: bool,
    pub gear_down: bool,
}

#[derive(Clone, Debug)]
pub struct Car {
    /// Centre of the car body.
    pub pos: Vec3,
    pub yaw: f32,
    /// Forward speed in m/s (negative = reversing). Read it; to change it use [`Car::set_speed`].
    pub speed: f32,
    pub vy: f32,
    pub pitch: f32,
    pub roll: f32,
    pub on_ground: bool,
    /// True while a wall pushed the car during the last step.
    pub hit_wall: bool,
    /// Strongest wall impact of the last step (m/s into the wall), 0 without one.
    pub impact: f32,
    /// Speed into the road (m/s) at the most recent landing.
    pub landing: f32,
    /// The road is wet (set by the race from the weather). Grip drops unless the car is under
    /// cover (retail: a surface more than 3.5 m above it).
    pub wet: bool,
    /// The road under the car was wet during the last step (wet and not under cover).
    pub road_wet: bool,
    /// The driving model.
    pub sim: CarSim,
    /// Collision box half sizes for car-against-car: length, width, height (m).
    pub half: [f32; 3],
    /// Frame time not yet run as a 64 Hz tick.
    pending: f32,
    /// Free body angles (pitch, roll) on top of the road's slope, from hits and jumps. Past its
    /// balance point a car falls onto its side or roof (see [`TIP`]).
    pub tumble: [f32; 2],
    tumble_rate: [f32; 2],
    /// Retail's crash timer in 64 Hz ticks (see [`CRASH_TICKS`]); 0 when not running.
    crash_ticks: i32,
    /// A hit over [`FORCED_HIT`]: the crash timer is not cancelled by the car landing upright.
    crash_forced: bool,
    /// Retail would put the car back on the road now. The race does it with [`Car::recover`];
    /// left alone the car is set upright where it is on the next step.
    pub wants_recover: bool,
    /// Suspension: how far each wheel's corner of the body sits above its rest place (m) and how
    /// fast it moves. Wheels 0 front left, 1 front right, 2 rear left, 3 rear right.
    susp: [f32; 4],
    susp_vel: [f32; 4],
    /// Road height under each wheel relative to the body's rest plane (m, within the travel).
    susp_road: [f32; 4],
    /// Strongest hit of the last step (impact scale) and its damage zone, for logs and sounds.
    pub last_hit: Option<(f32, usize)>,
}

/// Hits at least this strong are written to the race log.
pub const BIG_HIT_LOG: f32 = 40.0;

/// Height of the car's centre above the road surface.
pub const RIDE_HEIGHT: f32 = 0.45;
/// Collision circles (front/rear axle) around the car's line of travel.
const AXLE_OFFSET: f32 = 1.4;
const BODY_RADIUS: f32 = 1.0;
/// Ground probe tiers: how far sideways the car looks for support, and how far below it accepts a
/// surface. The first tier is the car's own footprint; the wider ones bridge the small gaps between
/// track pieces (road edge to wall base) so a rigid car never drops through them.
const PROBE_RADIUS: f32 = 1.5;
const PROBE_DROP: f32 = 1.5;
const NEAR_RADIUS: f32 = 3.0;
const NEAR_DROP: f32 = 2.0;
const FAR_RADIUS: f32 = 6.0;
const FAR_DROP: f32 = 5.0;
/// A surface may be this far above the car and still count as support (a step up).
const STEP_UP: f32 = 1.0;
/// The car counts as touching the road while its ballistic path is at most this far above it
/// (retail: 5 mm).
const AIR_GAP: f32 = 0.005;
/// Landing: a car meeting the road faster than this (m/s, relative to the road's own vertical
/// speed) bounces back up with this fraction of that speed (retail: asphalt's 0.7 x 0.4).
const HARD_LANDING: f32 = 6.0;
const BOUNCE: f32 = 0.7 * 0.4;
/// A surface this far above the road counts as cover from the rain.
const COVER_HEIGHT: f32 = 3.5;
/// At most this much frame time is caught up in one step (a stalled frame does not fast-forward).
const MAX_CATCH_UP: f32 = 0.25;
/// Balance points (pitch, roll in radians, from the road): tilted further than this the car falls
/// over; less, it drops back onto its wheels. From the box: atan(half length / ride height) and
/// atan(half width / ride height) (our estimate).
pub const TIP: [f32; 2] = [1.35, 1.06];
/// Rolled further than this the car goes on over onto its roof instead of resting on its side.
const SIDE_TO_ROOF: f32 = 2.4;
/// How hard the body is pulled to its resting angle on the ground and how fast that motion dies
/// (a guess for retail's four-point contact).
const TUMBLE_SPRING: f32 = 60.0;
const TUMBLE_DAMP: f32 = 10.0;
/// A car is upright (on its wheels) while its up axis has more than this world-up component
/// (retail's 0.75).
const UPRIGHT: f32 = 0.75;
/// Retail's crash timer: 192 ticks (3 s) after a big hit, counting ten times as fast while the
/// car is in the air. Running out with the car not upright puts it back on the road.
pub const CRASH_TICKS: i32 = 192;
/// A hit stronger than this puts the car back even if it lands on its wheels (retail, player).
const FORCED_HIT: f32 = 120.0;
/// Sliding on its side or roof the car slows at this rate (m/s^2; a guess).
const BODY_FRICTION: f32 = 6.0;
/// Suspension spring (1/s^2) and damper (1/s) for each corner: about 1.5 Hz, lightly damped
/// (guesses; retail's four-point suspension is not decoded).
const SUSP_SPRING: f32 = 90.0;
const SUSP_DAMP: f32 = 8.0;
/// How much of the computed body lean is drawn. Retail's lean felt excessive in the playtest
/// (the whole car used to roll, wheels included); the split now keeps the wheels planted, and this
/// scales the body's roll/pitch down to a look-only value. A guess until the next playtest.
const LEAN_LOOK: f32 = 0.6;

impl Car {
    /// A car with the made-up [`Spec::generic`] and an automatic gearbox.
    pub fn new(pos: Vec3, yaw: f32) -> Self {
        Car::with_spec(pos, yaw, Spec::generic(), Aids::default())
    }

    /// A car with a real `.sim` spec and the race setup's aids.
    pub fn with_spec(pos: Vec3, yaw: f32, spec: Spec, aids: Aids) -> Self {
        Car {
            pos,
            yaw,
            speed: 0.0,
            vy: 0.0,
            pitch: 0.0,
            roll: 0.0,
            on_ground: false,
            hit_wall: false,
            impact: 0.0,
            landing: 0.0,
            wet: false,
            road_wet: false,
            sim: CarSim::new(spec, aids),
            half: [2.2, 0.9, 0.65],
            pending: 0.0,
            tumble: [0.0; 2],
            tumble_rate: [0.0; 2],
            crash_ticks: 0,
            crash_forced: false,
            wants_recover: false,
            susp: [0.0; 4],
            susp_vel: [0.0; 4],
            susp_road: [0.0; 4],
            last_hit: None,
        }
    }

    /// The same car (spec and aids) standing still at a spot.
    pub fn reset(&self, pos: Vec3, yaw: f32) -> Car {
        Car { half: self.half, wet: self.wet, ..Car::with_spec(pos, yaw, self.sim.spec.clone(), self.sim.aids) }
    }

    /// The same car put back on its wheels at a spot (retail's reset onto the road after a
    /// crash): spec, aids and damage are kept.
    pub fn recover(&self, pos: Vec3, yaw: f32) -> Car {
        let mut car = self.reset(pos, yaw);
        car.sim.damage = self.sim.damage.clone();
        car
    }

    /// Pitch and roll for the chassis (the car entity the wheels ride on): the road slope under
    /// the car plus the tumble from hits and jumps. The body lean is *not* here, so the wheels
    /// stay planted while the body rolls over them (see [`Car::lean_tilt`]).
    pub fn chassis_tilt(&self) -> (f32, f32) {
        (self.pitch + self.tumble[0], self.roll + self.tumble[1])
    }

    /// Pitch and roll of the body over the chassis: the lean from braking, power and cornering
    /// plus the suspension tilt, scaled for looks. The wheels do not take this, so they never
    /// lean into the road.
    pub fn lean_tilt(&self) -> (f32, f32) {
        let (p, r) = self.sim.body_lean();
        let s = &self.susp;
        let wb = self.sim.spec.wheelbase.max(1.0);
        let sp = ((s[0] + s[1]) - (s[2] + s[3])) * 0.5 / wb;
        let sr = ((s[1] + s[3]) - (s[0] + s[2])) * 0.5 / car_sim::LEAN_TRACK;
        ((p + sp) * LEAN_LOOK, (r + sr) * LEAN_LOOK)
    }

    /// Where to draw the body: the car's centre moved by the suspension and lifted while it lies
    /// on its side or roof (its centre is then further from the road than the ride height).
    pub fn draw_pos(&self) -> Vec3 {
        let heave = self.susp.iter().sum::<f32>() * 0.25;
        let (sp, cp) = self.tumble[0].sin_cos();
        let (sr, cr) = self.tumble[1].sin_cos();
        let up = cp * cr;
        let extent = self.half[2] * up.abs() + self.half[1] * (cp * sr).abs() + self.half[0] * sp.abs();
        let lift = (extent - RIDE_HEIGHT).max(0.0) * (1.0 - up.max(0.0));
        self.pos + Vec3::Y * (heave + lift)
    }

    /// How far each wheel hangs below its place on the body (m; negative = pushed up into the
    /// arch). Wheels 0 front left, 1 front right, 2 rear left, 3 rear right.
    pub fn wheel_drop(&self) -> [f32; 4] {
        std::array::from_fn(|i| (self.susp_road[i] - self.susp[i]).clamp(-car_sim::LEAN_TRAVEL, car_sim::LEAN_TRAVEL))
    }

    /// Height of each wheel relative to its place on the drawn chassis (m), so it stays on the road
    /// while the body moves on its springs. Wheels 0 front left, 1 front right, 2 rear left, 3
    /// rear right.
    pub fn wheel_heights(&self) -> [f32; 4] {
        let heave = self.susp.iter().sum::<f32>() * 0.25;
        std::array::from_fn(|i| (self.susp_road[i] - heave).clamp(-2.0 * car_sim::LEAN_TRAVEL, 2.0 * car_sim::LEAN_TRAVEL))
    }

    /// The car's up axis has this world-up component (1 upright, -1 on its roof).
    pub fn up_y(&self) -> f32 {
        let (p, r) = (self.pitch + self.tumble[0], self.roll + self.tumble[1]);
        p.cos() * r.cos()
    }

    /// The car is on its wheels (retail: up component over 0.75).
    pub fn upright(&self) -> bool {
        self.up_y() > UPRIGHT
    }

    /// Takes a hit of `strength` (impact scale: wall speed in m/s, car-against-car strength) from
    /// the direction (`right`, `forward`, `up`) in the car's frame: retail's damage, a kick to the
    /// body (small hits wobble it, big ones can throw it over) and, for a big hit, the crash timer.
    pub fn take_hit(&mut self, strength: f32, right: f32, forward: f32, up: f32) {
        if strength <= car_damage::MIN_HIT {
            return;
        }
        let Some(zone) = Damage::zone_for(right, forward, up) else { return };
        self.sim.damage.hit(strength, zone);
        if self.last_hit.is_none_or(|(s, _)| strength > s) {
            self.last_hit = Some((strength, zone));
        }
        let big = strength > car_damage::BIG_HIT;
        let (cap, k) = if big { (2.0, 1.2) } else { (0.33, 1.0) };
        let kick = (strength / 32.0).min(cap) * k * 0.5;
        // retail kicks the pitch one way for the front zones and the other way for the rest, and
        // the roll one way for the left zones and the other way for the rest. Which way is our
        // guess (a front hit lifts the nose, a hit on the left lifts the left side).
        self.tumble_rate[0] += if zone <= 2 { kick } else { -kick };
        self.tumble_rate[1] += if (2..=4).contains(&zone) { -kick } else { kick };
        if big {
            self.crash_ticks = CRASH_TICKS;
            if strength > FORCED_HIT {
                self.crash_forced = true;
            }
        }
    }

    /// Unit vector the car is facing (yaw 0 faces -Z).
    pub fn heading(&self) -> Vec3 {
        Vec3::new(-self.yaw.sin(), 0.0, -self.yaw.cos())
    }

    fn right(&self) -> Vec3 {
        Vec3::new(self.yaw.cos(), 0.0, -self.yaw.sin())
    }

    /// Engine speed for the rev counter.
    /// The strongest wheel slide, 0..1 (a locked wheel counts as full), 0 in the air: what the
    /// skid sounds follow.
    pub fn skid(&self) -> f32 {
        if !self.on_ground {
            return 0.0;
        }
        self.sim.wheels.iter().map(|w| if w.locked { 1.0 } else { w.slide }).fold(0.0f32, f32::max)
    }

    pub fn rpm(&self) -> f32 {
        self.sim.drive.rpm
    }

    /// -1 reverse, 0 neutral, 1.. forward.
    pub fn gear(&self) -> i32 {
        self.sim.gear()
    }

    /// Sets the speed along the heading (tests, teleports).
    pub fn set_speed(&mut self, v: f32) {
        let h = self.heading();
        self.sim.vel = [h.x * v, h.z * v];
        self.speed = v;
    }

    fn body(&self) -> Body {
        Body { pos: [self.pos.x, self.pos.z], yaw: self.yaw, half: self.half }
    }

    /// Advances by `dt` seconds of frame time: runs every whole 64 Hz tick that fits.
    pub fn step(&mut self, input: Input, dt: f32, ground: &Ground, walls: &WallSet) {
        self.pending = (self.pending + dt.max(0.0)).min(MAX_CATCH_UP);
        let tick = 1.0 / TICK_HZ;
        self.hit_wall = false;
        self.impact = 0.0;
        self.last_hit = None;
        if self.wants_recover {
            // nobody put the car back on the road: set it on its wheels where it is
            *self = self.recover(self.pos + Vec3::Y, self.yaw);
        }
        let ctl = Controls {
            throttle: if input.throttle { 1.0 } else { 0.0 },
            brake: if input.brake { 1.0 } else { 0.0 },
            steer: input.steer,
            handbrake: input.handbrake,
            gear_up: input.gear_up,
            gear_down: input.gear_down,
        };
        while self.pending >= tick {
            self.pending -= tick;
            self.tick(ctl, ground, walls);
        }
    }

    /// One 64 Hz tick.
    fn tick(&mut self, ctl: Controls, ground: &Ground, walls: &WallSet) {
        let dt = 1.0 / TICK_HZ;
        let (sp, cp) = (self.pitch + self.tumble[0]).sin_cos();
        let (sr, cr) = (self.roll + self.tumble[1]).sin_cos();
        let wet = self.wet && {
            let base = self.pos.y - RIDE_HEIGHT;
            ground.surface_near(self.pos.x, self.pos.z, 0.5, base + COVER_HEIGHT, base + 60.0).is_none()
        };
        // on its side or roof the wheels are off the road: no grip, the body slides to a stop
        let wheels_down = self.on_ground && cp * cr > 0.5;
        self.road_wet = wet;
        let contact = Contact { front_grounded: wheels_down, rear_grounded: wheels_down, forward_y: sp, right_y: sr, up_y: cp * cr, wet };
        self.sim.tick(self.yaw, ctl, contact);
        if self.on_ground && !wheels_down {
            let v = self.sim.vel[0].hypot(self.sim.vel[1]);
            let k = if v > 1e-4 { (1.0 - BODY_FRICTION * dt / v).max(0.0) } else { 0.0 };
            self.sim.vel = [self.sim.vel[0] * k, self.sim.vel[1] * k];
            self.sim.yaw_rate *= 0.95;
        }
        self.yaw += self.sim.yaw_rate * dt;
        self.pos.x += self.sim.vel[0] * dt;
        self.pos.z += self.sim.vel[1] * dt;

        // walls: push the front and rear axle circles out by 1.05 times the overlap (retail's
        // margin) and let the model take the hit
        let h = self.heading();
        let (y_min, y_max) = (self.pos.y - RIDE_HEIGHT - 0.3, self.pos.y + 1.0);
        for s in [AXLE_OFFSET, -AXLE_OFFSET] {
            let (cx, cz) = (self.pos.x + h.x * s, self.pos.z + h.z * s);
            if let Some(push) = walls.resolve(cx, cz, y_min, y_max, BODY_RADIUS) {
                let len = push[0].hypot(push[1]);
                if len > 1e-6 {
                    let n = [push[0] / len, push[1] / len];
                    let into = car_sim::wall_response(&mut self.sim, self.yaw, n);
                    self.impact = self.impact.max(into);
                    // the hit comes from the wall's side, against the push
                    let (right, fwd) = (-(n[0] * self.right().x + n[1] * self.right().z), -(n[0] * h.x + n[1] * h.z));
                    self.take_hit(into, right, fwd, 0.0);
                }
                self.pos.x += push[0] * 1.05;
                self.pos.z += push[1] * 1.05;
                self.hit_wall = true;
            }
        }

        // vertical: the car is rigid, so it looks for the surface under it in widening tiers. The
        // nearest surface wins; a gap between track pieces (road edge to wall base) is bridged by
        // the wider tiers instead of swallowing the car. Only when nothing is near does it fall.
        let base = self.pos.y - RIDE_HEIGHT;
        let r = self.right();
        let probe = |radius: f32, drop: f32| ground.surface_near(self.pos.x, self.pos.z, radius, base - drop, base + STEP_UP);
        let mut support = probe(PROBE_RADIUS, PROBE_DROP);
        if support.is_none() {
            support = probe(NEAR_RADIUS, NEAR_DROP);
        }
        if support.is_none() {
            support = probe(FAR_RADIUS, FAR_DROP);
        }
        // While on the ground the car moves up and down with the road, so it keeps that vertical
        // speed when the road drops away and only leaves a crest that falls faster than gravity
        // (retail's body carries its vertical speed the same way). Sudden steps (seams between
        // track pieces) are capped so they do not launch the car.
        // Airborne, retail's gravity applies (stronger at speed through the rear downforce). A car
        // landing hard bounces back up; otherwise it takes the road's vertical speed.
        let y0 = self.pos.y;
        let was_on_ground = self.on_ground;
        let fall = self.sim.spec.fall_accel(self.sim.forward_speed(self.yaw) * self.sim.damage.aero_factor(), cp * cr);
        let vy_air = self.vy - fall * dt;
        let y_air = y0 + vy_air * dt;
        match support {
            Some(sy) if y_air <= sy + RIDE_HEIGHT + AIR_GAP => {
                let target = sy + RIDE_HEIGHT;
                let cap = self.speed.abs() * 0.4 + 1.0;
                if self.on_ground {
                    self.vy = ((target - y0) / dt).min(cap);
                } else {
                    // the road's own vertical speed under the car, from its slope along the path
                    let (vx, vz) = (self.sim.vel[0], self.sim.vel[1]);
                    let ahead = Vec3::new(self.pos.x + vx * 0.1, 0.0, self.pos.z + vz * 0.1);
                    let road_vy = ground
                        .surface_near(ahead.x, ahead.z, PROBE_RADIUS, sy - 2.0, sy + 2.0)
                        .map_or(0.0, |ay| (ay - sy) / 0.1)
                        .min(cap);
                    let into = road_vy - vy_air;
                    self.vy = if into > HARD_LANDING { road_vy + BOUNCE * into } else { road_vy };
                    self.landing = into.max(0.0);
                }
                self.pos.y = target;
                self.on_ground = true;
            }
            _ => {
                self.on_ground = false;
                self.vy = vy_air;
                self.pos.y = y_air;
            }
        }

        // tilt with the slope under the car
        if self.on_ground {
            let base = self.pos.y - RIDE_HEIGHT;
            let sample = |p: Vec3| ground.surface_near(p.x, p.z, PROBE_RADIUS, base - 1.0, base + 1.5).unwrap_or(base);
            let front = sample(self.pos + h * 2.0);
            let back = sample(self.pos - h * 2.0);
            let left = sample(self.pos - r);
            let rightside = sample(self.pos + r);
            let want_pitch = ((front - back) / 4.0).atan().clamp(-0.5, 0.5);
            let want_roll = ((rightside - left) / 2.0).atan().clamp(-0.5, 0.5);
            let k = (12.0 * dt).min(1.0);
            self.pitch += (want_pitch - self.pitch) * k;
            self.roll += (want_roll - self.roll) * k;
        }
        let was_upright = cp * cr > UPRIGHT;
        let landed = self.on_ground && !was_on_ground;
        if landed && !was_upright {
            // came down on its side or roof: that landing is a hit from above
            self.take_hit(self.landing + self.speed.abs() * 0.5, 0.0, 0.0, 1.0);
        } else if landed && self.landing > HARD_LANDING {
            self.take_hit(self.landing, 0.0, 0.0, -1.0);
        }
        self.tumble_tick(dt);
        self.crash_tick();
        self.suspension_tick(dt, landed, ground);
        self.speed = self.sim.forward_speed(self.yaw);
    }

    /// The free body angles: in the air they keep turning; on the road the body falls to the
    /// nearest resting angle (on its wheels, its side or its roof) past the balance points.
    fn tumble_tick(&mut self, dt: f32) {
        use std::f32::consts::{FRAC_PI_2, PI};
        for i in 0..2 {
            let a = self.tumble[i];
            if self.on_ground {
                let rest = if a.abs() < TIP[i] {
                    0.0
                } else if i == 1 && a.abs() < SIDE_TO_ROOF {
                    FRAC_PI_2 * a.signum()
                } else {
                    PI * a.signum()
                };
                self.tumble_rate[i] += ((rest - a) * TUMBLE_SPRING - self.tumble_rate[i] * TUMBLE_DAMP) * dt;
            }
            self.tumble[i] = (a + self.tumble_rate[i] * dt).clamp(-PI, PI);
            if self.tumble[i].abs() >= PI {
                self.tumble_rate[i] = 0.0;
            }
        }
    }

    /// Retail's crash timer. Our own addition: a car resting off its wheels starts the timer too
    /// (retail starts it only on big hits; not settled whether a slow roll-over resets).
    fn crash_tick(&mut self) {
        let upright = self.upright();
        if self.crash_ticks == 0 && !upright && self.on_ground {
            self.crash_ticks = CRASH_TICKS;
        }
        if self.crash_ticks <= 0 {
            return;
        }
        self.crash_ticks -= if self.on_ground { 1 } else { 10 };
        if upright && !self.crash_forced {
            self.crash_ticks = 0;
        } else if self.crash_ticks < 1 {
            self.crash_ticks = 0;
            self.crash_forced = false;
            let settled = upright && self.on_ground && self.tumble_rate[0].abs() < 1.0 && self.tumble_rate[1].abs() < 1.0;
            if !settled {
                self.wants_recover = true;
            }
        }
    }

    /// The four corners on springs: each follows the road under its wheel within the travel,
    /// overshooting a little; a landing pushes them all down.
    fn suspension_tick(&mut self, dt: f32, landed: bool, ground: &Ground) {
        let travel = car_sim::LEAN_TRAVEL;
        let h = self.heading();
        let r = self.right();
        let half_wb = self.sim.spec.wheelbase.max(1.0) * 0.5;
        let half_track = car_sim::LEAN_TRACK * 0.5;
        let base = self.pos.y - RIDE_HEIGHT;
        let (tp, tr) = (self.pitch.tan(), self.roll.tan());
        for i in 0..4 {
            let f = if i < 2 { half_wb } else { -half_wb };
            let s = if i % 2 == 0 { -half_track } else { half_track };
            let target = if self.on_ground && self.upright() {
                let p = self.pos + h * f + r * s;
                let plane = base + f * tp + s * tr;
                ground.surface_near(p.x, p.z, 0.4, plane - 0.5, plane + 0.5).map_or(-travel, |y| y - plane).clamp(-travel, travel)
            } else {
                -travel
            };
            self.susp_road[i] = target;
            if landed {
                self.susp_vel[i] -= self.landing.min(8.0) * 0.5;
            }
            self.susp_vel[i] += ((target - self.susp[i]) * SUSP_SPRING - self.susp_vel[i] * SUSP_DAMP) * dt;
            self.susp[i] = (self.susp[i] + self.susp_vel[i] * dt).clamp(target - travel, target + travel);
        }
    }
}

/// Car against car: retail's impulse (`car_sim::collide`). Moves both cars apart and changes their
/// motion; returns the impact strength (80 and up is a big hit), or `None` when they do not touch.
/// Cars more than a body height apart vertically never touch.
pub fn collide_cars(a: &mut Car, b: &mut Car) -> Option<f32> {
    if (a.pos.y - b.pos.y).abs() > a.half[2] + b.half[2] {
        return None;
    }
    let (mut ab, mut bb) = (a.body(), b.body());
    let hit = car_sim::collide(&mut a.sim, &mut ab, &mut b.sim, &mut bb)?;
    a.pos.x = ab.pos[0];
    a.pos.z = ab.pos[1];
    b.pos.x = bb.pos[0];
    b.pos.z = bb.pos[1];
    a.speed = a.sim.forward_speed(a.yaw);
    b.speed = b.sim.forward_speed(b.yaw);
    // each car is hit from the other's side
    let d = b.pos - a.pos;
    let (ah, ar, bh, br) = (a.heading(), a.right(), b.heading(), b.right());
    a.take_hit(hit, d.dot(ar), d.dot(ah), 0.0);
    b.take_hit(hit, -d.dot(br), -d.dot(bh), 0.0);
    Some(hit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ground::Tri;

    fn plane(y0: f32, slope_x: f32) -> Ground {
        // a big square, height = y0 + slope_x * x
        let (a, b) = (-1000.0f32, 1000.0f32);
        let h = |x: f32| y0 + slope_x * x;
        let t: Vec<Tri> = vec![[[a, h(a), a], [b, h(b), a], [a, h(a), b]], [[b, h(b), a], [b, h(b), b], [a, h(a), b]]];
        Ground::new(t, 16.0)
    }

    fn run(car: &mut Car, input: Input, seconds: f32, g: &Ground) {
        let walls = WallSet::new(vec![], 4.0);
        let steps = (seconds / 0.016) as usize;
        for _ in 0..steps {
            car.step(input, 0.016, g, &walls);
        }
    }

    #[test]
    fn accelerates_shifts_and_stays_on_flat_ground() {
        let g = plane(0.0, 0.0);
        let mut car = Car::new(Vec3::new(0.0, 0.45, 0.0), 0.0);
        run(&mut car, Input { throttle: true, ..Default::default() }, 8.0, &g);
        assert!(car.speed > 25.0 && car.speed < 70.0, "speed {}", car.speed);
        assert!(car.gear() >= 2, "gear {}", car.gear());
        assert!(car.rpm() > 1000.0, "rpm {}", car.rpm());
        assert!(car.on_ground);
        assert!((car.pos.y - RIDE_HEIGHT).abs() < 1e-3);
        assert!(car.pos.z < -50.0, "should have driven toward -Z, z = {}", car.pos.z);
    }

    #[test]
    fn braking_stops_the_car() {
        let g = plane(0.0, 0.0);
        let mut car = Car::new(Vec3::new(0.0, 0.45, 0.0), 0.0);
        car.set_speed(30.0);
        run(&mut car, Input { brake: true, ..Default::default() }, 4.0, &g);
        assert!(car.speed.abs() < 1.0, "speed {}", car.speed);
    }

    #[test]
    fn steering_left_turns_toward_minus_x() {
        let g = plane(0.0, 0.0);
        let mut car = Car::new(Vec3::new(0.0, 0.45, 0.0), 0.0);
        car.set_speed(20.0);
        run(&mut car, Input { throttle: true, steer: 1.0, ..Default::default() }, 2.0, &g);
        assert!(car.yaw > 0.3, "yaw {}", car.yaw);
        assert!(car.pos.x < -3.0, "x {}", car.pos.x);
    }

    #[test]
    fn follows_a_slope_and_tilts() {
        let g = plane(0.0, 0.1); // rises toward +X
        let mut car = Car::new(Vec3::new(0.0, 0.45, 0.0), -std::f32::consts::FRAC_PI_2); // faces +X
        run(&mut car, Input { throttle: true, ..Default::default() }, 4.0, &g);
        assert!(car.pos.x > 10.0, "x {}", car.pos.x);
        assert!((car.pos.y - (0.1 * car.pos.x + RIDE_HEIGHT)).abs() < 0.05, "y {} x {}", car.pos.y, car.pos.x);
        assert!(car.pitch > 0.05, "nose should be up, pitch {}", car.pitch);
    }

    #[test]
    fn falls_where_there_is_no_ground() {
        let g = plane(0.0, 0.0);
        let mut car = Car::new(Vec3::new(5000.0, 0.45, 5000.0), 0.0);
        run(&mut car, Input::default(), 1.0, &g);
        assert!(car.pos.y < -5.0, "y {}", car.pos.y);
        assert!(!car.on_ground);
    }

    #[test]
    fn cannot_turn_when_stopped() {
        let g = plane(0.0, 0.0);
        let mut car = Car::new(Vec3::new(0.0, 0.45, 0.0), 0.0);
        run(&mut car, Input { steer: 1.0, ..Default::default() }, 1.0, &g);
        assert!(car.yaw.abs() < 1e-4, "yaw {}", car.yaw);
    }

    #[test]
    fn wall_stops_the_car() {
        let g = plane(0.0, 0.0);
        // a wall across x = 5, facing the car that drives toward +X
        let walls = WallSet::new(
            vec![[[5.0, -1.0, -5.0], [5.0, -1.0, 5.0], [5.0, 3.0, -5.0]], [[5.0, -1.0, 5.0], [5.0, 3.0, 5.0], [5.0, 3.0, -5.0]]],
            4.0,
        );
        let mut car = Car::new(Vec3::new(0.0, 0.45, 0.0), -std::f32::consts::FRAC_PI_2);
        car.set_speed(30.0);
        let mut hit = false;
        let mut impact: f32 = 0.0;
        for _ in 0..(4.0 / 0.016) as usize {
            car.step(Input::default(), 0.016, &g, &walls);
            hit |= car.hit_wall;
            impact = impact.max(car.impact);
            assert!(car.pos.x < 5.0, "drove through the wall, x {}", car.pos.x);
        }
        assert!(hit, "never touched the wall");
        assert!(impact > 25.0, "head-on impact {impact}");
        assert!(car.speed.abs() < 5.0, "a head-on hit leaves only the 10% bounce, {}", car.speed);
    }

    #[test]
    fn bridges_a_gap_between_track_pieces() {
        // a flat road with a 1.5 m wide hole in it, straddling x = 14
        let (a, b) = (-1000.0f32, 1000.0f32);
        let mut tris: Vec<Tri> = Vec::new();
        for (x0, x1) in [(a, 13.25f32), (14.75, b)] {
            tris.push([[x0, 0.0, a], [x1, 0.0, a], [x0, 0.0, b]]);
            tris.push([[x1, 0.0, a], [x1, 0.0, b], [x0, 0.0, b]]);
        }
        let g = Ground::new(tris, 16.0);
        let walls = WallSet::new(vec![], 4.0);
        let mut car = Car::new(Vec3::new(0.0, RIDE_HEIGHT, 0.0), -std::f32::consts::FRAC_PI_2); // faces +X
        car.set_speed(20.0);
        let mut lowest = f32::MAX;
        for _ in 0..(3.0 / 0.016) as usize {
            car.step(Input::default(), 0.016, &g, &walls);
            lowest = lowest.min(car.pos.y);
        }
        assert!(car.pos.x > 20.0, "should have crossed the hole, x {}", car.pos.x);
        assert!(lowest > RIDE_HEIGHT - 0.05, "fell into the hole: lowest y {lowest}");
    }

    #[test]
    fn does_not_drop_onto_a_lower_deck() {
        // the upper road has a gap; a second deck 20 m below must not become the support
        let (a, b) = (-1000.0f32, 1000.0f32);
        let mut tris: Vec<Tri> = Vec::new();
        for y in [0.0f32, -20.0] {
            let (x0, x1) = if y > 0.0 { (a, 10.0f32) } else { (a, b) };
            tris.push([[x0, y, a], [x1, y, a], [x0, y, b]]);
            tris.push([[x1, y, a], [x1, y, b], [x0, y, b]]);
        }
        let g = Ground::new(tris, 16.0);
        let walls = WallSet::new(vec![], 4.0);
        let mut car = Car::new(Vec3::new(0.0, RIDE_HEIGHT, 0.0), -std::f32::consts::FRAC_PI_2);
        car.set_speed(20.0);
        for _ in 0..(0.4 / 0.016) as usize {
            car.step(Input::default(), 0.016, &g, &walls);
        }
        // just short of the upper road's end: still on the top deck, not dropped to -20
        assert!(car.pos.y > RIDE_HEIGHT - 0.1, "dropped to a lower deck: y {}", car.pos.y);
    }

    #[test]
    fn frame_rate_does_not_change_the_result() {
        let g = plane(0.0, 0.0);
        let walls = WallSet::new(vec![], 4.0);
        let input = Input { throttle: true, steer: 0.2, ..Default::default() };
        let mut a = Car::new(Vec3::new(0.0, 0.45, 0.0), 0.0);
        let mut b = a.clone();
        for _ in 0..(5.0 * 32.0) as usize {
            a.step(input, 1.0 / 32.0, &g, &walls);
        }
        for _ in 0..(5.0 * 128.0) as usize {
            b.step(input, 1.0 / 128.0, &g, &walls);
        }
        assert!((a.pos - b.pos).length() < 1e-3, "{:?} vs {:?}", a.pos, b.pos);
    }

    #[test]
    fn two_cars_bump() {
        let mut a = Car::new(Vec3::new(0.0, 0.45, 0.0), 0.0);
        let mut b = Car::new(Vec3::new(0.0, 0.45, -4.3), 0.0);
        a.set_speed(30.0);
        b.set_speed(10.0);
        let hit = collide_cars(&mut a, &mut b).expect("touching");
        assert!(hit > 0.0);
        assert!(a.speed < 30.0 && b.speed > 10.0, "a {} b {}", a.speed, b.speed);
        b.pos.y += 5.0;
        assert!(collide_cars(&mut a, &mut b).is_none(), "different heights");
    }

    #[test]
    fn falls_with_retail_gravity_and_bounces_on_a_hard_landing() {
        let g = plane(0.0, 0.0);
        let walls = WallSet::new(vec![], 4.0);
        // standing still: g x 1.16
        let mut car = Car::new(Vec3::new(0.0, 3.0, 0.0), 0.0);
        car.step(Input::default(), 1.0 / 64.0, &g, &walls);
        assert!((car.vy + 9.8 * 1.16 / 64.0).abs() < 1e-4, "vy {}", car.vy);
        // fast and upright: the rear downforce pulls harder
        assert!(car.sim.spec.fall_accel(40.0, 1.0) > car.sim.spec.fall_accel(4.0, 1.0));
        assert_eq!(car.sim.spec.fall_accel(40.0, 0.0), car.sim.spec.fall_accel(4.0, 1.0));
        // dropped from 3 m it lands at about 8 m/s and bounces back up
        let mut bounced = false;
        for _ in 0..64 {
            car.step(Input::default(), 1.0 / 64.0, &g, &walls);
            bounced |= car.on_ground && car.vy > 1.0;
        }
        assert!(car.landing > 6.0, "landing {}", car.landing);
        assert!(bounced, "no bounce");
        // a soft landing does not bounce
        let mut car = Car::new(Vec3::new(0.0, 1.0, 0.0), 0.0);
        for _ in 0..64 {
            car.step(Input::default(), 1.0 / 64.0, &g, &walls);
            assert!(car.vy < 0.5, "vy {}", car.vy);
        }
    }

    #[test]
    fn wet_road_grips_less_and_body_leans() {
        let g = plane(0.0, 0.0);
        let input = Input { throttle: true, steer: 1.0, ..Default::default() };
        let mut dry = Car::new(Vec3::new(0.0, 0.45, 0.0), 0.0);
        dry.set_speed(25.0);
        let mut wet = dry.clone();
        wet.wet = true;
        run(&mut dry, input, 3.0, &g);
        run(&mut wet, input, 3.0, &g);
        let turned = |c: &Car| c.yaw.abs();
        assert!(turned(&wet) != turned(&dry), "wet {} dry {}", turned(&wet), turned(&dry));
        // braking dives the nose, a left turn rolls the right side down
        let mut car = Car::new(Vec3::new(0.0, 0.45, 0.0), 0.0);
        car.set_speed(30.0);
        run(&mut car, Input { brake: true, ..Default::default() }, 0.5, &g);
        assert!(car.sim.body_lean().0 < -0.01, "lean {:?}", car.sim.body_lean());
        let mut car = Car::new(Vec3::new(0.0, 0.45, 0.0), 0.0);
        car.set_speed(25.0);
        run(&mut car, Input { steer: 1.0, ..Default::default() }, 1.0, &g);
        assert!(car.sim.body_lean().1 < -0.01, "lean {:?}", car.sim.body_lean());
    }

    /// The body lean stays off the chassis (which the wheels ride on), so on a flat road the
    /// chassis tilt is zero while the body still rolls in a turn. The wheels never lean with it.
    #[test]
    fn body_lean_stays_off_the_planted_wheels() {
        let g = plane(0.0, 0.0);
        let mut car = Car::new(Vec3::new(0.0, 0.45, 0.0), 0.0);
        car.set_speed(25.0);
        run(&mut car, Input { steer: 1.0, ..Default::default() }, 1.0, &g);
        let (chassis_pitch, chassis_roll) = car.chassis_tilt();
        let (lean_pitch, lean_roll) = car.lean_tilt();
        assert!(chassis_roll.abs() < 1e-4, "chassis roll must not take the lean: {chassis_roll}");
        assert!(chassis_pitch.abs() < 1e-4, "chassis pitch must not take the lean: {chassis_pitch}");
        assert!(lean_roll.abs() > 0.01, "the body should roll in the turn: {lean_roll}");
        assert!(lean_pitch.abs() < 0.05, "only a slight dive in a flat turn: {lean_pitch}");
    }

    #[test]
    fn tipped_past_the_balance_point_it_rolls_over_and_is_recovered() {
        let g = plane(0.0, 0.0);
        let walls = WallSet::new(vec![], 4.0);
        // a small tilt drops back onto the wheels
        let mut car = Car::new(Vec3::new(0.0, RIDE_HEIGHT, 0.0), 0.0);
        car.on_ground = true;
        car.tumble = [0.0, 0.6];
        run(&mut car, Input::default(), 2.0, &g);
        assert!(car.upright() && car.tumble[1].abs() < 0.05, "tumble {:?}", car.tumble);
        // past it the car ends on its side, slides to a stop and the crash timer asks for a reset
        let mut car = Car::new(Vec3::new(0.0, RIDE_HEIGHT, 0.0), 0.0);
        car.on_ground = true;
        car.set_speed(15.0);
        car.tumble = [0.0, 1.3];
        let mut asked = false;
        for _ in 0..(4.0 * 64.0) as usize {
            car.step(Input { throttle: true, ..Default::default() }, 1.0 / 64.0, &g, &walls);
            assert!(!car.upright() || asked);
            asked |= car.wants_recover;
            if asked {
                break;
            }
        }
        assert!(asked, "no reset; tumble {:?}", car.tumble);
        assert!((car.tumble[1] - std::f32::consts::FRAC_PI_2).abs() < 0.2, "should lie on its side: {:?}", car.tumble);
        assert!(car.speed.abs() < 1.0, "should have slid to a stop: {}", car.speed);
        car.sim.damage.hit(60.0, 1);
        let back = car.recover(Vec3::new(0.0, RIDE_HEIGHT, 0.0), 0.0);
        assert!(back.upright() && !back.wants_recover && back.sim.damage.zones[1] == 30.0);
    }

    #[test]
    fn big_hits_damage_and_kick_small_ones_do_not_reset() {
        let mut car = Car::new(Vec3::new(0.0, RIDE_HEIGHT, 0.0), 0.0);
        car.take_hit(30.0, 0.0, 1.0, 0.0);
        assert_eq!(car.sim.damage.zones[1], 15.0);
        assert_eq!(car.crash_ticks, 0);
        car.take_hit(100.0, -1.0, 0.0, 0.0);
        assert_eq!(car.sim.damage.zones[3], 50.0);
        assert_eq!(car.crash_ticks, CRASH_TICKS);
        assert!(car.tumble_rate[1] < -1.0, "left hit lifts the left side: {:?}", car.tumble_rate);
        // upright on the road the timer is cancelled
        let g = plane(0.0, 0.0);
        run(&mut car, Input::default(), 0.5, &g);
        assert_eq!(car.crash_ticks, 0);
        assert!(!car.wants_recover);
    }

    #[test]
    fn suspension_dips_on_landing_and_settles() {
        let g = plane(0.0, 0.0);
        let walls = WallSet::new(vec![], 4.0);
        let mut car = Car::new(Vec3::new(0.0, 2.0, 0.0), 0.0);
        let mut lowest = 0.0f32;
        for _ in 0..128 {
            car.step(Input::default(), 1.0 / 64.0, &g, &walls);
            lowest = lowest.min(car.draw_pos().y - car.pos.y);
        }
        assert!(lowest < -0.03, "body should dip on landing: {lowest}");
        assert!((car.draw_pos().y - car.pos.y).abs() < 0.01, "settled: {}", car.draw_pos().y - car.pos.y);
        assert!(car.wheel_heights().iter().all(|w| w.abs() < 0.02), "{:?}", car.wheel_heights());
    }
}
