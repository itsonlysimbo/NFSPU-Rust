//! The player's (and any AI's) car: the retail driving model of `car_sim.rs` plus height, tilt and
//! collision against the track. Inputs in ([`Input`]), state out (`pos`, `yaw`, `speed`,
//! [`Car::rpm`], [`Car::gear`]). The model runs on the retail clock: [`Car::step`] adds up frame
//! time and runs whole 64 Hz ticks (forces at 32 Hz). World units are metres.

use crate::car_sim::{self, Aids, Body, CarSim, Contact, Controls, Spec, FALL_G, TICK_HZ};
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
    /// The driving model.
    pub sim: CarSim,
    /// Collision box half sizes for car-against-car: length, width, height (m).
    pub half: [f32; 3],
    /// Frame time not yet run as a 64 Hz tick.
    pending: f32,
}

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
/// At most this much frame time is caught up in one step (a stalled frame does not fast-forward).
const MAX_CATCH_UP: f32 = 0.25;

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
            sim: CarSim::new(spec, aids),
            half: [2.2, 0.9, 0.65],
            pending: 0.0,
        }
    }

    /// The same car (spec and aids) standing still at a spot.
    pub fn reset(&self, pos: Vec3, yaw: f32) -> Car {
        Car { half: self.half, ..Car::with_spec(pos, yaw, self.sim.spec.clone(), self.sim.aids) }
    }

    /// Unit vector the car is facing (yaw 0 faces -Z).
    pub fn heading(&self) -> Vec3 {
        Vec3::new(-self.yaw.sin(), 0.0, -self.yaw.cos())
    }

    fn right(&self) -> Vec3 {
        Vec3::new(self.yaw.cos(), 0.0, -self.yaw.sin())
    }

    /// Engine speed for the rev counter.
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
        let (sp, cp) = self.pitch.sin_cos();
        let (sr, cr) = self.roll.sin_cos();
        let contact = Contact { front_grounded: self.on_ground, rear_grounded: self.on_ground, forward_y: sp, right_y: sr, up_y: cp * cr };
        self.sim.tick(self.yaw, ctl, contact);
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
                    let into = car_sim::wall_response(&mut self.sim, self.yaw, [push[0] / len, push[1] / len]);
                    self.impact = self.impact.max(into);
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
        let y0 = self.pos.y;
        let vy_air = self.vy - FALL_G * dt;
        let y_air = y0 + vy_air * dt;
        match support {
            Some(sy) if y_air <= sy + RIDE_HEIGHT + 0.05 => {
                let target = sy + RIDE_HEIGHT;
                self.vy = ((target - y0) / dt).min(self.speed.abs() * 0.4 + 1.0);
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
        self.speed = self.sim.forward_speed(self.yaw);
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
}
