//! Reads a retail `.sav` career profile into our own [`Career`] (read-only: the save is never
//! written). Layout notes and what is still unread: `docs/provenance/career.md`, "Retail save".
use crate::career::{Career, OwnedCar as CareerCar, ERAS};
use crate::catalog::{self, Car};

/// What could be read from a retail save.
#[derive(Debug, Clone, PartialEq)]
pub struct RetailProfile {
    /// The career year (`Tournament` section, first value).
    pub year: i32,
    /// The account balance (`PlayerInfo`, second value): **guess**, from the blank profile holding
    /// 11000 (the first 356's price) and the played one, which bought that car, holding 0.
    pub cash: i64,
    pub cars: Vec<catalog::OwnedCar>,
}

fn section(save: &[u8], name: &str) -> Option<usize> {
    let mut pat = name.as_bytes().to_vec();
    pat.push(0);
    save.windows(pat.len()).position(|w| w == pat.as_slice()).map(|p| p + pat.len())
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// Reads the year, cash and owned cars. `None` when it has no `Tournament` or `PlayerInfo`
/// section, or the year is outside the career's range.
pub fn read(save: &[u8], cars: &[Car]) -> Option<RetailProfile> {
    let year = u32_at(save, section(save, "Tournament")?)? as i32;
    if year < ERAS[0].1 || year > crate::career::END_YEAR {
        return None;
    }
    let cash = u32_at(save, section(save, "PlayerInfo")? + 4)? as i64;
    Some(RetailProfile { year, cash, cars: catalog::owned_cars(save, cars) })
}

impl Career {
    /// A profile continuing a retail save: its year, cash and cars. Tournament progress is not read
    /// yet (the `Missions` section is undecoded), so every tournament of the era starts fresh.
    pub fn from_retail(player: &str, p: &RetailProfile, cars: &[Car]) -> Career {
        let mut c = Career::new(player);
        c.year = p.year;
        c.cash = p.cash;
        let owned: Vec<CareerCar> = p.cars.iter().filter_map(|o| cars.get(o.index).map(|car| CareerCar::stock(car.id, o.paint as usize, car.price as i64))).collect();
        if !owned.is_empty() {
            c.cars = owned;
        }
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn save(year: u32, cash: u32) -> Vec<u8> {
        let mut s = b"Tournament\0".to_vec();
        s.extend(year.to_le_bytes());
        s.extend(b"PlayerInfo\0");
        s.extend(1u32.to_le_bytes());
        s.extend(cash.to_le_bytes());
        s
    }

    #[test]
    fn year_and_cash_are_read() {
        let p = read(&save(1956, 0), &[]).unwrap();
        assert_eq!((p.year, p.cash, p.cars.len()), (1956, 0, 0));
        assert_eq!(read(&save(1950, 11000), &[]).unwrap().cash, 11000);
    }

    #[test]
    fn nonsense_is_refused() {
        assert!(read(&save(3, 0), &[]).is_none());
        assert!(read(b"nothing here", &[]).is_none());
    }

    #[test]
    fn an_empty_import_keeps_the_starter_car() {
        let p = RetailProfile { year: 1962, cash: 500, cars: vec![] };
        let c = Career::from_retail("A", &p, &[]);
        assert_eq!((c.year, c.cash, c.cars.len()), (1962, 500, 1));
    }

    /// The user's played profile and the blank template, when the install is at hand.
    #[test]
    fn the_real_saves_read_as_recorded() {
        let Some(dir) = std::env::var_os("NFSPU_GAME_DIR") else { return };
        let dir = std::path::Path::new(&dir);
        let cars = std::fs::read(dir.join("FEData").join("Data").join("nfs5.car")).map(|b| catalog::parse_cars(&b)).unwrap_or_default();
        let (Ok(greg), Ok(blank)) = (std::fs::read(dir.join("SaveData").join("3Greg.sav")), std::fs::read(dir.join("SaveData").join("XXDefXX.sav"))) else { return };
        let (g, b) = (read(&greg, &cars).unwrap(), read(&blank, &cars).unwrap());
        assert_eq!((g.year, g.cash, g.cars.len()), (1956, 0, 1));
        assert_eq!((b.year, b.cash, b.cars.len()), (1950, 11000, 0));
    }
}
