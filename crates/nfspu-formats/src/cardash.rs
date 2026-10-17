//! The instrument part of the front end's car list, `FEData/Data/nfs5.car`: one fixed-size record
//! per car, holding (among much else) how its analog speedometers and tachometer are drawn. The
//! front end copies these numbers into `fe_out.txt` for the race (`speedostylekmh`,
//! `speedostartangle`, ... `needlepivot`), which is how the fields were identified. See
//! `docs/provenance/hud.md`.

/// Size of one car record.
pub const RECORD: usize = 1648;

/// How one analog dial is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DialSpec {
    /// Face picture: entry index in `speedK.fsh` / `speedM.fsh` / `tach.fsh`.
    pub face: u16,
    /// Number of needle steps: km/h, mph, or rpm / 10.
    pub units: i16,
    /// Needle picture: entry index in `needle.fsh` / `needle2.fsh`.
    pub needle: u16,
    /// Needle angle at the top of the scale, in degrees (0 = pointing right, counter-clockwise).
    pub start_angle: i16,
    /// Thousandths of a degree the needle turns per unit.
    pub millidegrees_per_unit: i16,
    /// Distance from the needle's rotation point to its back end, in pixels.
    pub pivot: i16,
}

/// The dials of one car.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarDash {
    /// Full display name, e.g. `'96 911 Carrera Targa (993)`.
    pub name: String,
    /// Model file name, e.g. `993` (`GameData/CarModel/993.crp`).
    pub model: String,
    /// Physics spec name, e.g. `993targa36` (as `car[0].specname` in `fe_out.txt`).
    pub spec: String,
    pub speedo_kmh: DialSpec,
    pub speedo_mph: DialSpec,
    pub tach: DialSpec,
}

fn text(d: &[u8]) -> String {
    let end = d.iter().position(|&b| b == 0).unwrap_or(d.len());
    d[..end].iter().map(|&b| b as char).collect()
}

fn dial(r: &[u8], at: usize) -> DialSpec {
    let w = |i: usize| i16::from_le_bytes([r[at + i * 2], r[at + i * 2 + 1]]);
    DialSpec { face: w(0) as u16, units: w(1), needle: w(2) as u16, start_angle: w(3), millidegrees_per_unit: w(4), pivot: w(5) }
}

/// Reads every whole record.
pub fn parse(data: &[u8]) -> Vec<CarDash> {
    data.chunks_exact(RECORD)
        .map(|r| CarDash {
            name: text(&r[2..52]),
            model: text(&r[127..177]),
            spec: text(&r[177..227]),
            speedo_kmh: dial(r, 768),
            speedo_mph: dial(r, 780),
            tach: dial(r, 792),
        })
        .collect()
}

/// The first car whose model file name matches (case-insensitive).
pub fn by_model<'a>(cars: &'a [CarDash], model: &str) -> Option<&'a CarDash> {
    cars.iter().find(|c| c.model.eq_ignore_ascii_case(model))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_names_and_dials() {
        let mut r = vec![0u8; RECORD];
        r[2..10].copy_from_slice(b"Test 993");
        r[127..130].copy_from_slice(b"993");
        r[177..187].copy_from_slice(b"993targa36");
        for (i, v) in [4i16, 300, 2, -40, 910, 18, 5, 200, 2, -38, 1353, 18, 7, 760, 2, -22, 310, 18].iter().enumerate() {
            r[768 + i * 2..770 + i * 2].copy_from_slice(&v.to_le_bytes());
        }
        let cars = parse(&r);
        assert_eq!(cars.len(), 1);
        let c = by_model(&cars, "993").unwrap();
        assert_eq!(c.name, "Test 993");
        assert_eq!(c.spec, "993targa36");
        assert_eq!(c.speedo_kmh, DialSpec { face: 4, units: 300, needle: 2, start_angle: -40, millidegrees_per_unit: 910, pivot: 18 });
        assert_eq!(c.speedo_mph.units, 200);
        assert_eq!(c.tach, DialSpec { face: 7, units: 760, needle: 2, start_angle: -22, millidegrees_per_unit: 310, pivot: 18 });
        assert!(by_model(&cars, "356").is_none());
    }

    #[test]
    #[ignore = "reads the player's install; run with NFSPU_GAME_DIR set"]
    fn reads_the_installed_car_list() {
        let path = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").expect("NFSPU_GAME_DIR")).join("FEData").join("Data").join("nfs5.car");
        let cars = parse(&std::fs::read(path).unwrap());
        assert_eq!(cars.len(), 109);
        let c = by_model(&cars, "993").unwrap();
        // The numbers the front end wrote to fe_out.txt for the user's 993 race.
        assert_eq!(c.speedo_kmh, DialSpec { face: 4, units: 300, needle: 2, start_angle: -40, millidegrees_per_unit: 910, pivot: 18 });
        assert_eq!(c.tach, DialSpec { face: 7, units: 760, needle: 2, start_angle: -22, millidegrees_per_unit: 310, pivot: 18 });
    }
}
