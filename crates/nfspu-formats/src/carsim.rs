//! Car simulation files: `GameData/Simulation/CarData/<name>.sim`, one per car (the catalogue
//! record names it), plus `default.sim` used when a car's own file is missing.
//!
//! Every file is 328 bytes of little-endian values (`docs/provenance/physics.md`): a 64-byte name,
//! then integers and floats in the units the file uses (kg, mm, inches, mph, rpm, lb-ft and
//! percent). This reader keeps those raw units; turning them into simulation constants is the
//! game's job (`nfspu-game` `car_sim.rs`).

use std::fmt;

/// Size of every `.sim` file.
pub const SIM_SIZE: usize = 0x148;
/// Number of torque-curve points: 0, 500, 1000, ... 10000 rpm.
pub const TORQUE_POINTS: usize = 21;
/// Spacing of the torque curve in rpm.
pub const TORQUE_STEP_RPM: f32 = 500.0;

#[derive(Debug, PartialEq, Eq)]
pub enum SimFileError {
    /// The file is shorter than [`SIM_SIZE`].
    Short(usize),
    /// The forward gear count is outside 1..=6.
    BadGears(i32),
}

impl fmt::Display for SimFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Short(n) => write!(f, ".sim file is {n} bytes, expected {SIM_SIZE}"),
            Self::BadGears(n) => write!(f, ".sim file has {n} forward gears"),
        }
    }
}

impl std::error::Error for SimFileError {}

/// Forced induction (`0x4c`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Induction {
    Natural,
    Turbo,
    Supercharged,
    Other(i32),
}

/// One `.sim` file, in the file's own units. Offsets are byte offsets into the file.
#[derive(Debug, Clone, PartialEq)]
pub struct SimFile {
    /// `0x00`: display name, `1987 959 coupe 2.9L`.
    pub name: String,
    /// `0x40`: mass in kg.
    pub mass_kg: f32,
    /// `0x44`: wheelbase in mm.
    pub wheelbase_mm: f32,
    /// `0x48`: unknown, zero in every shipped file.
    pub unknown_48: u32,
    /// `0x4c`.
    pub induction: Induction,
    /// `0x50`: forward gears (1..=6).
    pub forward_gears: usize,
    /// `0x54`: gear change time in 32 Hz simulation ticks.
    pub shift_ticks: i32,
    /// `0x58`: drive-force scale in percent.
    pub torque_scale_pct: f32,
    /// `0x5c`: share of the drive force sent to the front axle, percent (0 rear-wheel drive).
    pub front_drive_pct: f32,
    /// `0x60`: gear ratios: reverse (negative), neutral (0), then first..sixth (0 when unused).
    pub ratios: [f32; 8],
    /// `0x80`: final drive ratio.
    pub final_drive: f32,
    /// `0x84`, `0x88`, `0x8c`: tyre width (mm), aspect ratio (percent) and rim (inches).
    pub tyre_width_mm: f32,
    pub tyre_aspect_pct: f32,
    pub rim_in: f32,
    /// `0x90`: top speed in mph.
    pub top_speed_mph: f32,
    /// `0x94`: redline in rpm.
    pub redline_rpm: f32,
    /// `0x98`: engine braking, percent of the torque at the current rpm.
    pub engine_brake_pct: f32,
    /// `0x9c`: torque in lb-ft at 0, 500, ... 10000 rpm.
    pub torque_lbft: [f32; TORQUE_POINTS],
    /// `0xf0`: weight on the front axle, percent.
    pub front_weight_pct: f32,
    /// `0xf4`: tyre grip (lateral friction coefficient).
    pub grip: f32,
    /// `0xf8`: weight-transfer height, percent.
    pub transfer_height_pct: f32,
    /// `0xfc`: steering lock in 1/256 turn.
    pub steer_lock: f32,
    /// `0x100`: toe, in steering-input units (0 in every shipped file).
    pub toe: f32,
    /// `0x104`: braking deceleration at full pedal, m/s^2.
    pub brake_decel: f32,
    /// `0x108`: front brake share, percent.
    pub front_brake_pct: f32,
    /// `0x10c`, `0x110`: front and rear downforce, per mille of a m/s^2 per m/s.
    pub downforce_front: f32,
    pub downforce_rear: f32,
    /// `0x114`, `0x118`: front and rear suspension values in percent (meaning not decoded).
    pub suspension_front_pct: f32,
    pub suspension_rear_pct: f32,
    /// `0x11c`: weight-transfer stiffness (divides the transfer).
    pub transfer_stiffness: f32,
    /// `0x120`..`0x12c`: suspension values in mm-scale units (ride height, travel, spring,
    /// damper; not decoded further).
    pub suspension: [f32; 4],
    /// `0x130`: left/right split of the lateral weight transfer.
    pub lateral_split: f32,
    /// `0x134`..`0x144`: five values, mostly 1.0. The fourth (`0x140`) is scaled by the AI-data
    /// loader, so it is probably an AI value; the others are not decoded.
    pub tail: [f32; 5],
}

fn dw(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn fl(d: &[u8], at: usize) -> f32 {
    f32::from_bits(dw(d, at))
}

fn floats<const N: usize>(d: &[u8], at: usize) -> [f32; N] {
    std::array::from_fn(|i| fl(d, at + 4 * i))
}

/// Parses one `.sim` file.
pub fn parse(d: &[u8]) -> Result<SimFile, SimFileError> {
    if d.len() < SIM_SIZE {
        return Err(SimFileError::Short(d.len()));
    }
    let end = d[..0x40].iter().position(|b| *b == 0).unwrap_or(0x40);
    let name = d[..end].iter().map(|&b| b as char).collect::<String>().trim_end().to_string();
    let gears = dw(d, 0x50) as i32;
    if !(1..=6).contains(&gears) {
        return Err(SimFileError::BadGears(gears));
    }
    let induction = match dw(d, 0x4c) as i32 {
        0 => Induction::Natural,
        1 => Induction::Turbo,
        2 => Induction::Supercharged,
        n => Induction::Other(n),
    };
    Ok(SimFile {
        name,
        mass_kg: fl(d, 0x40),
        wheelbase_mm: fl(d, 0x44),
        unknown_48: dw(d, 0x48),
        induction,
        forward_gears: gears as usize,
        shift_ticks: dw(d, 0x54) as i32,
        torque_scale_pct: fl(d, 0x58),
        front_drive_pct: fl(d, 0x5c),
        ratios: floats(d, 0x60),
        final_drive: fl(d, 0x80),
        tyre_width_mm: fl(d, 0x84),
        tyre_aspect_pct: fl(d, 0x88),
        rim_in: fl(d, 0x8c),
        top_speed_mph: fl(d, 0x90),
        redline_rpm: fl(d, 0x94),
        engine_brake_pct: fl(d, 0x98),
        torque_lbft: floats(d, 0x9c),
        front_weight_pct: fl(d, 0xf0),
        grip: fl(d, 0xf4),
        transfer_height_pct: fl(d, 0xf8),
        steer_lock: fl(d, 0xfc),
        toe: fl(d, 0x100),
        brake_decel: fl(d, 0x104),
        front_brake_pct: fl(d, 0x108),
        downforce_front: fl(d, 0x10c),
        downforce_rear: fl(d, 0x110),
        suspension_front_pct: fl(d, 0x114),
        suspension_rear_pct: fl(d, 0x118),
        transfer_stiffness: fl(d, 0x11c),
        suspension: floats(d, 0x120),
        lateral_split: fl(d, 0x130),
        tail: floats(d, 0x134),
    })
}

impl SimFile {
    /// Rolling circumference of the driven tyre in metres: rim diameter plus two sidewalls.
    pub fn tyre_circumference_m(&self) -> f32 {
        let diameter_mm = self.rim_in * 25.4 + 2.0 * self.tyre_width_mm * self.tyre_aspect_pct / 100.0;
        diameter_mm * std::f32::consts::PI / 1000.0
    }

    /// Peak of the torque curve in lb-ft and the rpm where it is reached.
    pub fn peak_torque(&self) -> (f32, f32) {
        let (i, t) = self.torque_lbft.iter().enumerate().fold((0, 0.0f32), |best, (i, &t)| if t > best.1 { (i, t) } else { best });
        (t, i as f32 * TORQUE_STEP_RPM)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A made-up car (not taken from any shipped file).
    pub fn sample() -> Vec<u8> {
        let mut d = vec![0u8; SIM_SIZE];
        d[..10].copy_from_slice(b"Test car  ");
        let mut put = |at: usize, v: u32| d[at..at + 4].copy_from_slice(&v.to_le_bytes());
        let f = |v: f32| v.to_bits();
        put(0x40, f(1000.0));
        put(0x44, f(2400.0));
        put(0x4c, 1);
        put(0x50, 5);
        put(0x54, 7);
        put(0x58, f(100.0));
        put(0x5c, f(40.0));
        for (i, r) in [-3.0f32, 0.0, 3.2, 2.0, 1.4, 1.1, 0.9, 0.0].iter().enumerate() {
            put(0x60 + 4 * i, f(*r));
        }
        put(0x80, f(4.0));
        put(0x84, f(200.0));
        put(0x88, f(50.0));
        put(0x8c, f(16.0));
        put(0x90, f(150.0));
        put(0x94, f(6500.0));
        put(0x98, f(20.0));
        for i in 0..TORQUE_POINTS {
            put(0x9c + 4 * i, f(if i <= 13 { 100.0 + 10.0 * i as f32 } else { 0.0 }));
        }
        put(0xf0, f(45.0));
        put(0xf4, f(1.5));
        put(0x104, f(8.0));
        put(0x130, f(1.0));
        d
    }

    #[test]
    fn reads_fields_in_file_units() {
        let s = parse(&sample()).unwrap();
        assert_eq!(s.name, "Test car");
        assert_eq!(s.mass_kg, 1000.0);
        assert_eq!(s.wheelbase_mm, 2400.0);
        assert_eq!(s.induction, Induction::Turbo);
        assert_eq!(s.forward_gears, 5);
        assert_eq!(s.shift_ticks, 7);
        assert_eq!(s.ratios[0], -3.0);
        assert_eq!(s.ratios[6], 0.9);
        assert_eq!(s.final_drive, 4.0);
        assert_eq!(s.redline_rpm, 6500.0);
        assert_eq!(s.torque_lbft[13], 230.0);
        assert_eq!(s.torque_lbft[14], 0.0);
        assert_eq!(s.front_weight_pct, 45.0);
        assert_eq!(s.grip, 1.5);
        assert_eq!(s.brake_decel, 8.0);
        assert_eq!(s.peak_torque(), (230.0, 6500.0));
    }

    #[test]
    fn tyre_circumference_from_size() {
        let s = parse(&sample()).unwrap();
        // 16 in rim = 406.4 mm, two 100 mm sidewalls: 606.4 mm diameter
        assert!((s.tyre_circumference_m() - 0.6064 * std::f32::consts::PI).abs() < 1e-4);
    }

    #[test]
    fn rejects_short_files_and_bad_gear_counts() {
        assert_eq!(parse(&[0u8; 10]).unwrap_err(), SimFileError::Short(10));
        let mut d = sample();
        d[0x50] = 9;
        assert_eq!(parse(&d).unwrap_err(), SimFileError::BadGears(9));
    }
}
