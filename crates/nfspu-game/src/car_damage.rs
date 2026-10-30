//! Car damage: retail's ten damage zones, how a hit fills them and what they do to the car
//! (`docs/provenance/physics.md`, section 10). Zones 0..7 run round the car, 8 takes the hits
//! from above or below (landings, the roof):
//!
//! ```text
//!        front
//!      2   1   0
//!  left 3       7 right
//!      4   5   6
//!        rear
//! ```
//!
//! Each zone holds 0..100. A hit raises its zone to the hit's damage (never lowers it) and pulls
//! the two neighbours of the hit zone up toward the average of their own neighbours.

/// Number of damage zones (retail keeps ten; the tenth is never filled by a hit).
pub const ZONES: usize = 10;
/// The zone for hits from above or below (hard landings, landing on the roof).
pub const VERTICAL: usize = 8;
/// Hits at or below this strength do no damage (retail).
pub const MIN_HIT: f32 = 10.0;
/// A hit stronger than this is a big one: the body gets a hard kick and the crash timer starts.
pub const BIG_HIT: f32 = 80.0;

/// What one part's damage is made of: the zones whose average counts (`.PARTDAMAGE` in retail's
/// parts data, which we have not read; these masks are our guess from where the parts sit).
const ENGINE_ZONES: &[usize] = &[4, 5, 6];
const BRAKE_ZONES: &[usize] = &[0, 2, 4, 6];
const GEARBOX_ZONES: &[usize] = &[4, 5, 6];
const SUSPENSION_ZONES: &[usize] = &[0, 2, 4, 6];
const WING_ZONES: &[usize] = &[4, 5, 6];

#[derive(Clone, Debug, PartialEq)]
pub struct Damage {
    /// Damage per zone, 0..100.
    pub zones: [f32; ZONES],
    /// The race's damage option. Off: hits still shake the car but leave no damage.
    pub enabled: bool,
}

impl Default for Damage {
    fn default() -> Self {
        Damage { zones: [0.0; ZONES], enabled: true }
    }
}

impl Damage {
    /// Which zone a hit lands in, from the direction of the hit seen from the car's centre in the
    /// car's frame (`right`, `forward`, `up`; any length). A hit mostly from above or below is
    /// [`VERTICAL`]. `None` when it is too close
    /// to the centre to tell (retail: within 0.1 on both axes after normalising).
    pub fn zone_for(right: f32, forward: f32, up: f32) -> Option<usize> {
        let len = (right * right + forward * forward + up * up).sqrt();
        if len < 1e-6 {
            return None;
        }
        let (a, c, u) = (right / len, forward / len, up / len);
        if u.abs() > 0.5 {
            return Some(VERTICAL);
        }
        const T: f32 = 0.1;
        Some(match (c > T, c < -T, a > T, a < -T) {
            (true, _, false, false) => 1,
            (_, true, false, false) => 5,
            (false, false, true, _) => 7,
            (false, false, _, true) => 3,
            (true, _, true, _) => 0,
            (true, _, _, true) => 2,
            (_, true, true, _) => 6,
            (_, true, _, true) => 4,
            _ => return None,
        })
    }

    /// Retail's damage from a hit of `strength` (the impact scale of walls and car-against-car)
    /// in `zone`: half the strength, at most 100, kept if the zone already has more. The
    /// neighbours of a ring zone rise to the average of the zones on either side of them.
    pub fn hit(&mut self, strength: f32, zone: usize) {
        if !self.enabled || strength <= MIN_HIT || zone >= ZONES {
            return;
        }
        let d = (strength * 0.5).min(100.0);
        let z = &mut self.zones;
        z[zone] = z[zone].max(d);
        if zone < 8 {
            let ring = |i: isize| ((i + 8) % 8) as usize;
            for side in [-1isize, 1] {
                let n = ring(zone as isize + side);
                let beyond = ring(zone as isize + 2 * side);
                let avg = (z[zone] + z[beyond]) * 0.5;
                z[n] = z[n].max(avg);
            }
        }
    }

    fn average(&self, zones: &[usize]) -> f32 {
        zones.iter().map(|&i| self.zones[i]).sum::<f32>() / zones.len() as f32
    }

    /// Engine damage 0..100: the front centre zone plus the engine's own zones, averaged.
    pub fn engine(&self) -> f32 {
        (self.zones[1] + self.average(ENGINE_ZONES)) * 0.5
    }

    /// Drive torque multiplier: engine (1 - e/300) times gearbox (1 - g x 0.005).
    pub fn torque_factor(&self) -> f32 {
        (1.0 - self.engine() / 300.0) * (1.0 - self.average(GEARBOX_ZONES) * 0.005)
    }

    /// Brake force multiplier (1 - b x 0.005).
    pub fn brake_factor(&self) -> f32 {
        1.0 - self.average(BRAKE_ZONES) * 0.005
    }

    /// Downforce multiplier for a damaged wing or splitter (1 - a x 0.005).
    pub fn aero_factor(&self) -> f32 {
        1.0 - self.average(WING_ZONES) * 0.005
    }

    /// Suspension damage 0..100 (makes the steering wobble).
    pub fn suspension(&self) -> f32 {
        self.average(SUSPENSION_ZONES)
    }

    /// Steady steering pull in retail steering units (-127..127, positive left): a quarter of
    /// the difference between the two front corners. A crushed right corner pulls to the right.
    pub fn steer_pull(&self) -> f32 {
        (self.zones[2] - self.zones[0]) * 0.25
    }

    /// How much of the steering angle a front wheel still gets: 1 - zone / 512 for its corner
    /// (`left`: zone 2, right: zone 0).
    pub fn wheel_steer(&self, left: bool) -> f32 {
        1.0 - self.zones[if left { 2 } else { 0 }] / 512.0
    }

    /// Damage 0..100 of the nine parts the repair screens list, in the order engine, brakes,
    /// transmission, suspension, shocks, tires, sway bars, bodywork, cosmetic. Engine, brakes,
    /// transmission and suspension use the zone masks above; the other five are **guesses**
    /// (shocks follow the suspension, tires the four corners, sway bars the two sides, bodywork
    /// and cosmetic the whole ring).
    pub fn components(&self) -> [f32; 9] {
        let ring = self.average(&[0, 1, 2, 3, 4, 5, 6, 7]);
        [
            self.engine(),
            self.average(BRAKE_ZONES),
            self.average(GEARBOX_ZONES),
            self.suspension(),
            self.suspension(),
            self.average(&[0, 2, 4, 6]),
            self.average(&[3, 7]),
            ring,
            ring,
        ]
    }

    /// The zones whose damage makes up repairable part `k` (the order of `components`); repairing
    /// the part clears them (a **guess**: retail's per-part repair is not read).
    pub fn component_zones(k: usize) -> &'static [usize] {
        match k {
            0 => &[1, 4, 5, 6],
            1 => BRAKE_ZONES,
            2 => GEARBOX_ZONES,
            3 | 4 => SUSPENSION_ZONES,
            5 => &[0, 2, 4, 6],
            6 => &[3, 7],
            _ => &[0, 1, 2, 3, 4, 5, 6, 7],
        }
    }

    /// Any damage at all (for drawing).
    pub fn any(&self) -> bool {
        self.zones.iter().any(|&z| z > 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zones_round_the_car() {
        assert_eq!(Damage::zone_for(0.0, 1.0, 0.0), Some(1));
        assert_eq!(Damage::zone_for(0.0, -1.0, 0.0), Some(5));
        assert_eq!(Damage::zone_for(1.0, 0.0, 0.0), Some(7));
        assert_eq!(Damage::zone_for(-1.0, 0.0, 0.0), Some(3));
        assert_eq!(Damage::zone_for(1.0, 1.0, 0.0), Some(0));
        assert_eq!(Damage::zone_for(-1.0, 1.0, 0.0), Some(2));
        assert_eq!(Damage::zone_for(-1.0, -1.0, 0.0), Some(4));
        assert_eq!(Damage::zone_for(1.0, -1.0, 0.0), Some(6));
        assert_eq!(Damage::zone_for(0.0, 0.0, 1.0), Some(VERTICAL));
        assert_eq!(Damage::zone_for(0.0, 0.0, 0.0), None);
    }

    #[test]
    fn a_hit_fills_its_zone_and_spreads() {
        let mut d = Damage::default();
        d.hit(5.0, 1);
        assert!(!d.any(), "too soft");
        d.hit(60.0, 1);
        assert_eq!(d.zones[1], 30.0);
        assert_eq!(d.zones[0], 15.0);
        assert_eq!(d.zones[2], 15.0);
        d.hit(40.0, 1);
        assert_eq!(d.zones[1], 30.0, "a softer hit never lowers a zone");
        d.hit(400.0, 7);
        assert_eq!(d.zones[7], 100.0);
        // zone 0 sits between 7 and 1: (100 + 30) / 2
        assert_eq!(d.zones[0], 65.0);
        let mut off = Damage { enabled: false, ..Default::default() };
        off.hit(200.0, 1);
        assert!(!off.any());
    }

    #[test]
    fn damage_weakens_the_car() {
        let mut d = Damage::default();
        assert_eq!(d.torque_factor(), 1.0);
        assert_eq!(d.steer_pull(), 0.0);
        d.hit(200.0, 0);
        assert!(d.steer_pull() < 0.0, "a crushed right corner pulls right");
        assert!(d.wheel_steer(false) < 1.0 && d.wheel_steer(true) > d.wheel_steer(false));
        d.hit(200.0, 5);
        assert!(d.torque_factor() < 0.9 && d.aero_factor() < 1.0 && d.brake_factor() < 1.0);
    }
}
