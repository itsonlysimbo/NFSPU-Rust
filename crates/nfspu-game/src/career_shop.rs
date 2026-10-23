//! The career's money rules: what a car is worth, the dealership and used-car lot, the parts
//! catalogue (`FEData/Data/nfs5.prt`) and what installed parts do to a car's handling.
//!
//! **Verified** against the executable (see `docs/provenance/career.md`): the shape of the value
//! formula, its mileage and damage tables, the parts inflation table, the used-car lot's chances,
//! mileage rolls and 24-car cap. **Guess**: the base price (we use the catalogue price), the
//! meaning of the "extras" percentage, and every part effect in [`effects`].

use crate::catalog::Car;
use crate::car_sim::Tuning;

/// Bytes per record of `nfs5.prt`.
pub const PART_RECORD: usize = 0x168;

/// Yearly price growth in percent: a part `n` years old costs `1 + sum(first n) / 100` times its
/// list price to buy new today (retail's table, 62 entries).
const INFLATION: [u32; 62] = [
    1, 2, 2, 2, 2, 3, 4, 5, 4, 5, 6, 7, 8, 12, 4, 5, 4, 2, 3, 2, 3, 4, 3, 5, 6, 7, 2, 4, 5, 8, 9, 2, 5, 4, 6, 3, 7, 4, 7, 6, 6, 4, 7, 3, 6, 8, 3, 4, 5, 2, 4, 3, 4, 2, 5, 2, 1, 3, 7, 5, 4, 3,
];

/// How likely a car of a given age (years since its model year) is to reach the used-car lot:
/// it enters when `AGE_GATE[age] <= roll % 60`. Retail's table of 30 entries.
const AGE_GATE: [u32; 30] = [5, 10, 15, 20, 25, 35, 50, 70, 80, 80, 70, 50, 40, 30, 20, 10, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5];

/// The used-car lot holds at most this many cars.
pub const MARKET_CAP: usize = 24;
/// A car is never worth less than this share of its price.
const FLOOR_PCT: i64 = 5;
/// Miles added to the car by each finished race (**guess**).
pub const RACE_MILES: u32 = 12;

/// Percent added to the price for a car's mileage: (from, to, percent).
const MILEAGE_PCT: [(u32, u32, i64); 5] = [(0, 5000, 0), (5001, 10000, -10), (10001, 15000, -15), (15001, 20000, -25), (20001, 1_000_000, -30)];
/// Percent added for the car's overall damage (0-100): (from, to, percent).
const DAMAGE_PCT: [(u32, u32, i64); 5] = [(0, 5, -1), (6, 10, -6), (11, 15, -11), (16, 25, -16), (26, 100, -26)];
/// How a used car's mileage is rolled (roll 0-99): (from, to, min miles, max miles).
const MILEAGE_ROLL: [(u32, u32, u32, u32); 4] = [(0, 5, 0, 0), (6, 20, 2000, 15000), (21, 80, 15000, 25000), (81, 100, 25000, 100_000)];
/// The same for the used car's "extras" percentage.
const EXTRAS_ROLL: [(u32, u32, u32, u32); 4] = [(0, 5, 0, 0), (6, 20, 1, 2), (21, 80, 3, 10), (81, 100, 11, 50)];

fn table_pct(table: &[(u32, u32, i64)], value: u32) -> i64 {
    table.iter().find(|(lo, hi, _)| *lo <= value && value <= *hi).map_or(0, |t| t.2)
}

/// A part's worth now: its price grown by the inflation table over the years since its model year.
pub fn part_value(price: i64, years_old: i32) -> i64 {
    let n = years_old.clamp(0, INFLATION.len() as i32) as usize;
    let growth: i64 = INFLATION[..n].iter().map(|p| *p as i64).sum();
    price * (100 + growth) / 100
}

/// What a car is worth: the base price, less the mileage and damage steps, plus its extras and the
/// value of its installed parts; never below 5% of the price.
pub fn car_value(base: i64, mileage: u32, extras_pct: u32, damage: u32, parts: i64) -> i64 {
    let value = base + base * table_pct(&MILEAGE_PCT, mileage) / 100 + base * extras_pct as i64 / 100 + base * table_pct(&DAMAGE_PCT, damage) / 100 + parts;
    value.max(base * FLOOR_PCT / 100)
}

/// A small deterministic random source (the lot must come out the same for the same seed).
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03)
    }
    pub fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 16) as u32
    }
    fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            0
        } else {
            self.next() % n
        }
    }
}

/// A car on the used-car lot.
#[derive(Debug, Clone, PartialEq)]
pub struct UsedCar {
    pub car: u16,
    pub paint: usize,
    pub mileage: u32,
    pub extras: u32,
    pub price: i64,
}

fn roll(table: &[(u32, u32, u32, u32)], rng: &mut Rng) -> u32 {
    let r = rng.below(100);
    match table.iter().find(|t| t.0 <= r && r <= t.1) {
        Some((_, _, lo, hi)) => lo + rng.below(hi.saturating_sub(*lo)),
        None => 0,
    }
}

/// The cars the dealership sells new in `year` (indices into `cars`): road cars whose model years
/// include it (**guess**: retail's dealer list rule was not traced).
pub fn dealer_cars(cars: &[Car], year: i32) -> Vec<usize> {
    cars.iter().enumerate().filter(|(_, c)| c.kind == 0 && matches!(c.availability, 0 | 2) && c.year_from as i32 <= year && year <= c.year_to as i32 && c.price > 0).map(|(i, _)| i).collect()
}

/// The used-car lot of `year` for a seed. Each older road car (model year before last year) joins by
/// age-dependent chance, at most [`MARKET_CAP`] cars; mileage and extras are rolled as retail does
/// and the asking price is the car's value. `paints(car)` lists the paint sections a car can have.
pub fn used_market(cars: &[Car], year: i32, seed: u64, paints: &dyn Fn(&Car) -> Vec<usize>) -> Vec<UsedCar> {
    let mut rng = Rng::new(seed ^ ((year as u64) << 32));
    let mut out = Vec::new();
    for c in cars.iter().filter(|c| c.kind == 0 && c.availability == 0 && c.price > 0 && (c.year_from as i32) < year - 1) {
        let age = (year - c.year_from as i32) as usize;
        if AGE_GATE[age.min(AGE_GATE.len() - 1)] > rng.below(60) {
            continue;
        }
        let mileage = roll(&MILEAGE_ROLL, &mut rng);
        let extras = roll(&EXTRAS_ROLL, &mut rng);
        let sections = paints(c);
        let paint = if sections.is_empty() { 0 } else { sections[rng.below(sections.len() as u32) as usize] };
        out.push(UsedCar { car: c.id, paint, mileage, extras, price: car_value(c.price as i64, mileage, extras, 0, 0) });
        if out.len() >= MARKET_CAP {
            break;
        }
    }
    out
}

// ---- parts ---------------------------------------------------------------------------------

/// One part of the catalogue.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Part {
    pub id: u32,
    /// Name string id (`u32` at 4).
    pub name: u32,
    /// The car family it fits, `356`, `911 (930)` ... (text at 8).
    pub group: String,
    /// Part type (`u16` at 0x26): 1 engine side, 2 brakes, 3 gearbox, 4 springs, 5 shocks, 6 aero,
    /// 7 body panels, 8 sway bars, 9 tyres, 10 body/rims/roof.
    pub kind: u16,
    /// Category text (at 0x28): `Turbo`, `Exhaust`, `Rims` ... (empty for some kinds).
    pub category: String,
    /// Model year (`u16` at 0x46) and list price (`i32` at 0x5c).
    pub year: i32,
    pub price: i64,
}

fn text(b: &[u8]) -> String {
    let end = b.iter().position(|c| *c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).trim().to_string()
}

/// Reads `nfs5.prt`.
pub fn parse_parts(data: &[u8]) -> Vec<Part> {
    data.chunks_exact(PART_RECORD)
        .map(|r| Part {
            id: u32::from_le_bytes([r[0], r[1], r[2], r[3]]),
            name: u32::from_le_bytes([r[4], r[5], r[6], r[7]]),
            group: text(&r[8..0x20]),
            kind: u16::from_le_bytes([r[0x26], r[0x27]]),
            category: text(&r[0x28..0x44]),
            year: u16::from_le_bytes([r[0x46], r[0x47]]) as i32,
            price: i32::from_le_bytes([r[0x5c], r[0x5d], r[0x5e], r[0x5f]]) as i64,
        })
        .filter(|p| p.id != 0)
        .collect()
}

/// The part family a catalogue car uses (its AI-data name mapped to the parts file's spelling).
pub fn part_group(ai_name: &str) -> String {
    match ai_name.to_ascii_uppercase().as_str() {
        "901" => "911".into(),
        "930T" => "911 (930)".into(),
        "964" | "965T" => "911 (964)".into(),
        "993" | "993T" => "911 (993)".into(),
        "996" | "996T" => "911 (996)".into(),
        "BOXSTER" => "Boxster".into(),
        other => other.to_string(),
    }
}

/// The parts that fit a car and exist by `year`, cheapest first within their kind and category.
pub fn parts_for<'a>(parts: &'a [Part], car: &Car, year: i32) -> Vec<&'a Part> {
    let group = part_group(&car.ai_name);
    let mut list: Vec<&Part> = parts.iter().filter(|p| p.group.eq_ignore_ascii_case(&group) && p.year <= year).collect();
    list.sort_by(|a, b| (a.kind, &a.category, a.price, a.id).cmp(&(b.kind, &b.category, b.price, b.id)));
    list
}

/// Effects of one part at full tier: (power, grip, brake, mass) added as fractions (**guess**; the
/// part files' effect curves were not decoded). Scaled by the part's tier within its own kind and
/// category: the best (dearest) part of the family gets the full amount.
fn effects(kind: u16, category: &str) -> (f32, f32, f32, f32) {
    match (kind, category) {
        (1, "Engine") | (1, "Engine (Standard)") => (0.10, 0.0, 0.0, 0.0),
        (1, "Turbo") | (1, "Supercharger") => (0.12, 0.0, 0.0, 0.0),
        (1, "Flywheel") | (1, "Exhaust") | (1, "Air Intake") | (1, "Computer Chip") => (0.04, 0.0, 0.0, 0.0),
        (1, _) => (0.03, 0.0, 0.0, 0.0),
        (2, _) => (0.0, 0.0, 0.25, 0.0),
        (3, _) => (0.02, 0.0, 0.0, 0.0),
        (4, _) | (5, _) | (8, _) => (0.0, 0.03, 0.0, 0.0),
        (6, _) => (0.0, 0.03, 0.0, 0.0),
        (7, _) => (0.0, 0.0, 0.0, -0.02),
        (9, _) => (0.0, 0.10, 0.0, 0.0),
        (10, "Rims") | (10, "Body") => (0.0, 0.0, 0.0, -0.01),
        _ => (0.0, 0.0, 0.0, 0.0),
    }
}

/// The handling changes an installed set of parts makes. `catalogue` is the car's whole parts list
/// (to rank each part within its family).
pub fn tuning(catalogue: &[&Part], installed: &[u32]) -> Tuning {
    let (mut power, mut grip, mut brake, mut mass) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for part in catalogue.iter().filter(|p| installed.contains(&p.id)) {
        let family: Vec<&&Part> = catalogue.iter().filter(|o| o.kind == part.kind && o.category == part.category).collect();
        let rank = family.iter().filter(|o| o.price <= part.price).count() as f32;
        let tier = rank / family.len().max(1) as f32;
        let (p, g, b, m) = effects(part.kind, &part.category);
        power += p * tier;
        grip += g * tier;
        brake += b * tier;
        mass += m * tier;
    }
    Tuning { power: 1.0 + power.min(0.5), grip: 1.0 + grip.min(0.25), brake: 1.0 + brake.min(0.4), mass: 1.0 + mass.max(-0.1) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn car(id: u16, from: i16, to: i16, price: u32) -> Car {
        Car { id, year_from: from, year_to: to, price, ..Default::default() }
    }

    #[test]
    fn value_steps_down_with_mileage_and_has_a_floor() {
        assert_eq!(car_value(10_000, 0, 0, 0, 0), 9_900, "even an undamaged car loses the first damage step");
        assert_eq!(car_value(10_000, 7_000, 0, 0, 0), 8_900);
        assert_eq!(car_value(10_000, 30_000, 10, 30, 0), 10_000 - 3_000 + 1_000 - 2_600);
        assert_eq!(car_value(10_000, 30_000, 0, 30, 0), 10_000 - 3_000 - 2_600);
        assert_eq!(car_value(1_000, 90_000, 0, 90, -5_000), 50, "never under 5%");
    }

    #[test]
    fn parts_grow_in_price_with_age() {
        assert_eq!(part_value(1_000, 0), 1_000);
        assert_eq!(part_value(1_000, 3), 1_050);
        assert_eq!(part_value(1_000, 500), part_value(1_000, 62));
    }

    #[test]
    fn the_lot_is_repeatable_and_capped() {
        let cars: Vec<Car> = (0..60).map(|i| car(i + 1, 1950, 1960, 10_000 + i as u32 * 100)).collect();
        let paints = |_: &Car| vec![3usize, 4, 5];
        let a = used_market(&cars, 1975, 7, &paints);
        assert_eq!(a, used_market(&cars, 1975, 7, &paints));
        assert_ne!(a, used_market(&cars, 1975, 8, &paints));
        assert!(!a.is_empty() && a.len() <= MARKET_CAP);
        assert!(a.iter().all(|u| [3, 4, 5].contains(&u.paint) && u.price > 0));
        assert!(used_market(&cars, 1950, 7, &paints).is_empty(), "nothing is old enough yet");
    }

    #[test]
    fn the_dealer_sells_cars_of_the_year() {
        let cars = vec![car(1, 1950, 1954, 11_000), car(2, 1956, 1960, 12_700), car(3, 1950, 1954, 0)];
        assert_eq!(dealer_cars(&cars, 1952), vec![0]);
        assert!(dealer_cars(&cars, 1955).is_empty());
    }

    #[test]
    fn dearer_parts_tune_more() {
        let part = |id: u32, price: i64| Part { id, group: "356".into(), kind: 1, category: "Turbo".into(), year: 1950, price, ..Default::default() };
        let all = [part(1, 100), part(2, 200)];
        let refs: Vec<&Part> = all.iter().collect();
        let none = tuning(&refs, &[]);
        let low = tuning(&refs, &[1]);
        let high = tuning(&refs, &[2]);
        assert_eq!(none, Tuning::default());
        assert!(low.power > none.power && high.power > low.power);
        assert!((high.power - 1.12).abs() < 1e-5);
    }

    #[test]
    fn part_families_map_to_the_parts_file() {
        assert_eq!(part_group("930T"), "911 (930)");
        assert_eq!(part_group("boxster"), "Boxster");
        assert_eq!(part_group("356A"), "356A");
    }
}
