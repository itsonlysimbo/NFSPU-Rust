//! A simple arcade placeholder car: it accelerates, brakes, steers, follows the ground height and
//! tilts with the slope. This is NOT the original physics model; that comes later.
//! World units are metres (the road is about 12 units wide, a car about 4.4 long).

use crate::collision::WallSet;
use crate::ground::Ground;
use bevy::math::Vec3;

#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    pub throttle: bool,
    pub brake: bool,
    pub handbrake: bool,
    /// -1.0 (right) .. 1.0 (left)
    pub steer: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Car {
    /// Centre of the car body.
    pub pos: Vec3,
    pub yaw: f32,
    /// Forward speed in m/s (negative = reversing).
    pub speed: f32,
    pub vy: f32,
    pub pitch: f32,
    pub roll: f32,
    pub on_ground: bool,
    /// True while a wall is pushing the car this step.
    pub hit_wall: bool,
}

/// Height of the car's centre above the road surface.
pub const RIDE_HEIGHT: f32 = 0.45;
pub const MAX_SPEED: f32 = 70.0;
const GRAVITY: f32 = 30.0;
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

impl Car {
    pub fn new(pos: Vec3, yaw: f32) -> Self {
        Car { pos, yaw, speed: 0.0, vy: 0.0, pitch: 0.0, roll: 0.0, on_ground: false, hit_wall: false }
    }

    /// Unit vector the car is facing (yaw 0 faces -Z).
    pub fn heading(&self) -> Vec3 {
        Vec3::new(-self.yaw.sin(), 0.0, -self.yaw.cos())
    }

    fn right(&self) -> Vec3 {
        Vec3::new(self.yaw.cos(), 0.0, -self.yaw.sin())
    }

    pub fn step(&mut self, input: Input, dt: f32, ground: &Ground, walls: &WallSet) {
        let dt = dt.clamp(0.0, 0.05);

        // speed
        if input.throttle {
            if self.speed < 0.0 {
                self.speed += 30.0 * dt;
            } else {
                self.speed += 14.0 * (1.0 - (self.speed / MAX_SPEED).clamp(0.0, 1.0)) * dt;
            }
        }
        if input.brake {
            if self.speed > 0.5 {
                self.speed -= 28.0 * dt;
            } else {
                self.speed = (self.speed - 8.0 * dt).max(-10.0);
            }
        }
        if !input.throttle && !input.brake {
            let drag = (2.5 + 0.004 * self.speed * self.speed) * dt;
            if self.speed.abs() <= drag {
                self.speed = 0.0;
            } else {
                self.speed -= self.speed.signum() * drag;
            }
        }
        if input.handbrake {
            let b = 18.0 * dt;
            if self.speed.abs() <= b {
                self.speed = 0.0;
            } else {
                self.speed -= self.speed.signum() * b;
            }
        }

        // steering: needs some speed, and turns less sharply the faster you go
        let turn = 1.7 * input.steer.clamp(-1.0, 1.0) * (self.speed.abs() / 5.0).min(1.0) / (1.0 + self.speed.abs() / 25.0);
        self.yaw += turn * dt * self.speed.signum();

        let h = self.heading();
        self.pos.x += h.x * self.speed * dt;
        self.pos.z += h.z * self.speed * dt;

        // walls: push the front and rear axle circles out, scrub speed while scraping
        self.hit_wall = false;
        let (y_min, y_max) = (self.pos.y - RIDE_HEIGHT - 0.3, self.pos.y + 1.0);
        for s in [AXLE_OFFSET, -AXLE_OFFSET] {
            let (cx, cz) = (self.pos.x + h.x * s, self.pos.z + h.z * s);
            if let Some(push) = walls.resolve(cx, cz, y_min, y_max, BODY_RADIUS) {
                self.pos.x += push[0];
                self.pos.z += push[1];
                self.hit_wall = true;
            }
        }
        if self.hit_wall {
            self.speed *= (1.0 - 2.0 * dt).max(0.0);
            if self.speed.abs() < 0.3 {
                self.speed = 0.0;
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
        match support {
            Some(sy) => {
                let target = sy + RIDE_HEIGHT;
                if self.pos.y <= target + 0.05 {
                    self.pos.y = target;
                    self.vy = 0.0;
                    self.on_ground = true;
                } else {
                    self.on_ground = false;
                    self.vy -= GRAVITY * dt;
                    self.pos.y += self.vy * dt;
                    if self.pos.y < target {
                        self.pos.y = target;
                        self.vy = 0.0;
                    }
                }
            }
            None => {
                self.on_ground = false;
                self.vy -= GRAVITY * dt;
                self.pos.y += self.vy * dt;
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ground::Tri;

    fn plane(y0: f32, slope_x: f32) -> Ground {
        // a big square, height = y0 + slope_x * x
        let (a, b) = (-1000.0f32, 1000.0f32);
        let h = |x: f32| y0 + slope_x * x;
        let t: Vec<Tri> = vec![
            [[a, h(a), a], [b, h(b), a], [a, h(a), b]],
            [[b, h(b), a], [b, h(b), b], [a, h(a), b]],
        ];
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
    fn accelerates_and_stays_on_flat_ground() {
        let g = plane(0.0, 0.0);
        let mut car = Car::new(Vec3::new(0.0, 0.45, 0.0), 0.0);
        run(&mut car, Input { throttle: true, ..Default::default() }, 5.0, &g);
        assert!(car.speed > 25.0 && car.speed < MAX_SPEED, "speed {}", car.speed);
        assert!(car.on_ground);
        assert!((car.pos.y - RIDE_HEIGHT).abs() < 1e-3);
        assert!(car.pos.z < -50.0, "should have driven toward -Z, z = {}", car.pos.z);
    }

    #[test]
    fn braking_stops_the_car() {
        let g = plane(0.0, 0.0);
        let mut car = Car::new(Vec3::new(0.0, 0.45, 0.0), 0.0);
        car.speed = 30.0;
        run(&mut car, Input { brake: true, ..Default::default() }, 2.0, &g);
        assert!(car.speed <= 0.0 + 1e-3 || car.speed > -10.01);
        assert!(car.speed < 5.0, "speed {}", car.speed);
    }

    #[test]
    fn steering_left_turns_toward_minus_x() {
        let g = plane(0.0, 0.0);
        let mut car = Car::new(Vec3::new(0.0, 0.45, 0.0), 0.0);
        car.speed = 20.0;
        run(&mut car, Input { throttle: true, steer: 1.0, ..Default::default() }, 2.0, &g);
        assert!(car.yaw > 0.3, "yaw {}", car.yaw);
        assert!(car.pos.x < -3.0, "x {}", car.pos.x);
    }

    #[test]
    fn follows_a_slope_and_tilts() {
        let g = plane(0.0, 0.1); // rises toward +X
        let mut car = Car::new(Vec3::new(0.0, 0.45, 0.0), -std::f32::consts::FRAC_PI_2); // faces +X
        run(&mut car, Input { throttle: true, ..Default::default() }, 3.0, &g);
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
        assert_eq!(car.yaw, 0.0);
    }

    #[test]
    fn wall_stops_the_car() {
        let g = plane(0.0, 0.0);
        // a wall across x = 5, facing the car that drives toward +X
        let walls = WallSet::new(
            vec![
                [[5.0, -1.0, -5.0], [5.0, -1.0, 5.0], [5.0, 3.0, -5.0]],
                [[5.0, -1.0, 5.0], [5.0, 3.0, 5.0], [5.0, 3.0, -5.0]],
            ],
            4.0,
        );
        let mut car = Car::new(Vec3::new(0.0, 0.45, 0.0), -std::f32::consts::FRAC_PI_2);
        car.speed = 30.0;
        let mut hit = false;
        for _ in 0..(4.0 / 0.016) as usize {
            car.step(Input::default(), 0.016, &g, &walls);
            hit |= car.hit_wall;
            assert!(car.pos.x < 5.0, "drove through the wall, x {}", car.pos.x);
        }
        assert!(hit, "never touched the wall");
        assert!(car.speed < 30.0, "wall should scrub speed, {}", car.speed);
    }

    #[test]
    fn bridges_a_gap_between_track_pieces() {
        // a flat road with a 1.5 m wide hole in it, straddling x = 14
        let (a, b) = (-1000.0f32, 1000.0f32);
        let mut tris: Vec<Tri> = Vec::new();
        for (x0, x1) in [(a, 13.25f32), (14.75, b)] {
            let len = x1 - x0;
            tris.push([[x0, 0.0, a], [x1, 0.0, a], [x0, 0.0, b]]);
            tris.push([[x1, 0.0, a], [x1, 0.0, b], [x0, 0.0, b]]);
            let _ = len;
        }
        let g = Ground::new(tris, 16.0);
        let walls = WallSet::new(vec![], 4.0);
        let mut car = Car::new(Vec3::new(0.0, RIDE_HEIGHT, 0.0), -std::f32::consts::FRAC_PI_2); // faces +X
        car.speed = 20.0;
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
        car.speed = 20.0;
        for _ in 0..(0.4 / 0.016) as usize {
            car.step(Input::default(), 0.016, &g, &walls);
        }
        // just short of the upper road's end: still on the top deck, not dropped to -20
        assert!(car.pos.y > RIDE_HEIGHT - 0.1, "dropped to a lower deck: y {}", car.pos.y);
    }
}
