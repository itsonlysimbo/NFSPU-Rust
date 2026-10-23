//! The Evolution career: eras, tournaments, money, owned cars and progress.
//!
//! Tournament data comes from the install's `FEData/Data/nfs5.trn` (read-only); the player's
//! progress is our own text file, `career/<player>.txt` beside our other settings, never written
//! into the install or its `.sav` files. Field offsets and the rules below are described in
//! `docs/provenance/career.md`, which also says which rules are guesses.

use crate::career_shop::{self, Part, UsedCar};
use crate::menu::{Choice, Row};
use nfspu_formats::fe::Locale;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Era codes (`nfs5.car` / `nfs5.trn`, strings 521..523) with the career year each opens in.
pub const ERAS: [(u32, i32); 3] = [(521, 1950), (522, 1970), (523, 1989)];
/// The year the career ends (the last era's end; also the year the demo loop checks).
pub const END_YEAR: i32 = 2000;
/// The catalogue id of the car a new career starts with: `'50 356 1100 Cabriolet`, the one car in
/// the sample profile's garage.
pub const STARTER_CAR: u16 = 76;
/// Its purchase price, as the footage's garage shows it (the car's catalogue price).
pub const STARTER_PRICE: i64 = 11_000;
/// The starter car's paint (`.clr` section 19, the orange the sample profile has).
pub const STARTER_PAINT: usize = 19;
/// Cash a new career starts with. **Guess**: the field was not found in the save.
pub const START_CASH: u32 = 0;
/// Laps per career race. **Guess**: the quick-race default.
pub const CAREER_LAPS: u8 = 2;
/// How many places pay (the prize table has eight entries).
pub const PLACES: usize = 8;

/// One tournament record of `nfs5.trn` (0xd40 bytes). Only the fields the career uses.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tournament {
    /// Record id (`u32` at 0).
    pub id: u32,
    /// Era code (`u32` at 4): 521, 522 or 523.
    pub era: u32,
    /// The career year the era opens (`u16` at 8).
    pub year: i32,
    /// Track ids it races on (`u16` count at 10, ids from 12). Empty in the modern era.
    pub tracks: Vec<i16>,
    /// String ids: short name (0x20), full name (0x24), description (0x28), car rule (0x2c).
    pub short: u32,
    pub name: u32,
    pub description: u32,
    pub restriction: u32,
    /// Prize money for 1st..8th place (`u32` x 8 from 0xec).
    pub prizes: [u32; PLACES],
    /// The opponents' catalogue car ids, one per opponent (the first car id of each of the seven
    /// opponent blocks that start at 0x118, 0x58 bytes apart).
    pub opponent_cars: Vec<u16>,
    /// The bonus tournaments that win a prize car (550 A, 935, GT1): ids 31, 32 and 33.
    pub bonus: bool,
    /// 0 beginner, 1 advanced, 2 expert (`u16` at 0xe6; 0 for the 356 Challenge, which the footage
    /// shows as "beginner").
    pub difficulty: u8,
    /// The entry fee (`u32` at 0xdc, **guess**: 0 for the 356 Challenge as in the footage, 500 for
    /// 2000cc, 25,000 for the Class 3 Trophy; the string "not enough credits to enter this
    /// tournament" exists).
    pub entry_fee: u32,
}

const RECORD: usize = crate::catalog::TOURNAMENT_RECORD;

fn u16_at(b: &[u8], at: usize) -> u16 {
    b.get(at..at + 2).map_or(0, |s| u16::from_le_bytes([s[0], s[1]]))
}
fn u32_at(b: &[u8], at: usize) -> u32 {
    b.get(at..at + 4).map_or(0, |s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// Reads the career tournaments (eras 521..=523) of `nfs5.trn`, in file order. The club events
/// (eras 524..526, the factory driver mode) are left out.
pub fn parse_tournaments(data: &[u8]) -> Vec<Tournament> {
    data.chunks_exact(RECORD)
        .filter(|r| ERAS.iter().any(|(code, _)| *code == u32_at(r, 4)))
        .map(|r| {
            let count = u16_at(r, 10).min(16) as usize;
            let id = u32_at(r, 0);
            let mut prizes = [0; PLACES];
            for (n, p) in prizes.iter_mut().enumerate() {
                *p = u32_at(r, 0xec + 4 * n);
            }
            let opponent_cars = (0..7).map(|n| u16_at(r, 0x118 + 0x58 * n + 2)).filter(|c| *c != 0).collect();
            Tournament {
                id,
                era: u32_at(r, 4),
                year: u16_at(r, 8) as i32,
                tracks: (0..count).map(|n| u16_at(r, 12 + 2 * n) as i16).collect(),
                short: u32_at(r, 0x20),
                name: u32_at(r, 0x24),
                description: u32_at(r, 0x28),
                restriction: u32_at(r, 0x2c),
                prizes,
                opponent_cars,
                bonus: (31..=33).contains(&id),
                difficulty: u16_at(r, 0xe6).min(2) as u8,
                entry_fee: u32_at(r, 0xdc),
            }
        })
        .collect()
}

/// Gives every career tournament its race list. The record's own track list (`u16` count at 10)
/// is the era's *unlock* list (the same four or six tracks on every tournament of an era, all
/// `locked` = 3 in `nfs5.trk`), not the races: the footage's 356 Challenge races Normandie then
/// Cote d'Azur. **Guess** for the rule: the tracks the front end never locks (not the skid pad)
/// that are no harder than the tournament, highest id first (Normandie 14, Cote d'Azur 8).
pub fn assign_routes(all: &mut [Tournament], tracks: &[crate::catalog::Track]) {
    for t in all.iter_mut() {
        let mut ids: Vec<i16> = tracks.iter().filter(|k| k.locked == 0 && k.file != "skidpad" && k.difficulty <= t.difficulty as i16).map(|k| k.id).collect();
        ids.sort_unstable_by(|a, b| b.cmp(a));
        if !ids.is_empty() {
            t.tracks = ids;
        }
    }
}

/// The year the era after `year` opens, or `END_YEAR` after the last.
pub fn next_year(year: i32) -> i32 {
    ERAS.iter().map(|e| e.1).find(|y| *y > year).unwrap_or(END_YEAR)
}

/// The era code a career year is in (1950 -> 521 ... 1989 and later -> 523).
pub fn era_of_year(year: i32) -> u32 {
    ERAS.iter().rev().find(|e| e.1 <= year).map_or(ERAS[0].0, |e| e.0)
}

/// Money as the screens show it: `5,000`, no currency sign (retail footage; the control draws the
/// digits with commas).
pub fn money(amount: i64) -> String {
    let digits = amount.unsigned_abs().to_string();
    let mut out = String::new();
    for (n, c) in digits.chars().enumerate() {
        if n > 0 && (digits.len() - n) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    format!("{}{out}", if amount < 0 { "-" } else { "" })
}

/// A car the player owns.
#[derive(Debug, Clone, PartialEq)]
pub struct OwnedCar {
    /// Catalogue id (`nfs5.car`).
    pub id: u16,
    /// Paint: the `.clr` section number.
    pub paint: usize,
    /// Miles driven (a bought used car arrives with its own).
    pub mileage: u32,
    /// The used car's "extras" percentage (adds to its value).
    pub extras: u32,
    /// What the player paid.
    pub bought_for: i64,
    /// Installed part ids (`nfs5.prt`).
    pub parts: Vec<u32>,
}

impl OwnedCar {
    /// A car with nothing on it yet.
    pub fn stock(id: u16, paint: usize, bought_for: i64) -> OwnedCar {
        OwnedCar { id, paint, mileage: 0, extras: 0, bought_for, parts: Vec::new() }
    }
}

/// The catalogues the money rules need.
#[derive(Clone, Copy)]
pub struct Shop<'a> {
    pub cars: &'a [crate::catalog::Car],
    pub parts: &'a [Part],
}

/// Why a purchase did not go through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShopError {
    NotEnoughCash,
    UnknownItem,
    /// The last car cannot be sold.
    LastCar,
}

/// How far the player got in one tournament.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Progress {
    /// Races of the current run that are finished (0 when not started or just completed).
    pub races_done: u8,
    /// Times the whole tournament has been completed.
    pub completed: u32,
    /// The best finishing place in any race of it (1-based; 0: none yet).
    pub best_place: u8,
}

/// What a finished race paid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payout {
    pub place: usize,
    pub amount: u32,
    pub tournament_done: bool,
    /// The career year if this race finished the era (the era changed).
    pub new_year: Option<i32>,
}

/// One player's career.
#[derive(Debug, Clone, PartialEq)]
pub struct Career {
    pub player: String,
    pub cash: i64,
    /// The career year (1950 at the start; raised when an era is finished).
    pub year: i32,
    pub cars: Vec<OwnedCar>,
    /// Progress by tournament id.
    pub progress: BTreeMap<u32, Progress>,
    /// The tournament and owned car the menu has chosen (indices into the open list and `cars`).
    pub chosen_tournament: usize,
    pub chosen_car: usize,
    /// The used-car lot's round (it is made again after each race) and the lot cars already bought
    /// in this round.
    pub market_round: u32,
    pub taken: Vec<usize>,
}

impl Career {
    /// A new profile: the starter car, the start cash and 1950.
    pub fn new(player: &str) -> Career {
        Career {
            player: player.to_string(),
            cash: START_CASH as i64,
            year: ERAS[0].1,
            cars: vec![OwnedCar::stock(STARTER_CAR, STARTER_PAINT, STARTER_PRICE)],
            progress: BTreeMap::new(),
            chosen_tournament: 0,
            chosen_car: 0,
            market_round: 0,
            taken: Vec::new(),
        }
    }

    /// Our folder for career files: `NFSPU_SETTINGS_DIR/career`, else `settings/career` in the
    /// project (never the install).
    pub fn dir() -> PathBuf {
        std::env::var_os("NFSPU_SETTINGS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("settings"))
            .join("career")
    }

    fn file_name(player: &str) -> String {
        let safe: String = player.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
        format!("{safe}.txt")
    }

    /// The player's file in `dir`.
    pub fn path_in(dir: &std::path::Path, player: &str) -> PathBuf {
        dir.join(Self::file_name(player))
    }

    /// Whether a career has been started for this player in `dir`.
    pub fn exists_in(dir: &std::path::Path, player: &str) -> bool {
        Self::path_in(dir, player).is_file()
    }

    pub fn to_text(&self) -> String {
        let mut t = String::from("# NFSPU career profile (our own format)\nversion = 1\n");
        t += &format!("player = {}\ncash = {}\nyear = {}\n", self.player, self.cash, self.year);
        for c in &self.cars {
            let parts: Vec<String> = c.parts.iter().map(u32::to_string).collect();
            t += &format!("car = {},{},{},{},{},{}\n", c.id, c.paint, c.mileage, c.extras, c.bought_for, parts.join(";"));
        }
        t += &format!("market = {}\n", self.market_round);
        if !self.taken.is_empty() {
            t += &format!("taken = {}\n", self.taken.iter().map(usize::to_string).collect::<Vec<_>>().join(","));
        }
        for (id, p) in &self.progress {
            t += &format!("progress = {id},{},{},{}\n", p.races_done, p.completed, p.best_place);
        }
        t += &format!("chosen = {},{}\n", self.chosen_tournament, self.chosen_car);
        t
    }

    pub fn parse(text: &str) -> Option<Career> {
        let mut career = Career { player: String::new(), cash: 0, year: ERAS[0].1, cars: Vec::new(), progress: BTreeMap::new(), chosen_tournament: 0, chosen_car: 0, market_round: 0, taken: Vec::new() };
        let mut seen = false;
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
            let Some((key, value)) = line.split_once('=') else { continue };
            let (key, value) = (key.trim(), value.trim());
            let nums = |v: &str| v.split(',').map(|n| n.trim().parse::<i64>().unwrap_or(0)).collect::<Vec<_>>();
            match key {
                "version" => seen = true,
                "player" => career.player = value.to_string(),
                "cash" => career.cash = value.parse().unwrap_or(0),
                "year" => career.year = value.parse().unwrap_or(ERAS[0].1),
                "car" => {
                    // id,paint[,mileage,extras,bought_for,part;part...] (the first slice wrote two numbers)
                    let fields: Vec<&str> = value.split(',').collect();
                    let n = nums(&fields[..fields.len().min(5)].join(","));
                    if n.len() >= 2 {
                        career.cars.push(OwnedCar {
                            id: n[0] as u16,
                            paint: n[1] as usize,
                            mileage: n.get(2).copied().unwrap_or(0) as u32,
                            extras: n.get(3).copied().unwrap_or(0) as u32,
                            bought_for: n.get(4).copied().unwrap_or(0),
                            parts: fields.get(5).map(|p| p.split(';').filter_map(|x| x.trim().parse().ok()).collect()).unwrap_or_default(),
                        });
                    }
                }
                "market" => career.market_round = value.parse().unwrap_or(0),
                "taken" => career.taken = value.split(',').filter_map(|x| x.trim().parse().ok()).collect(),
                "progress" => {
                    let n = nums(value);
                    if n.len() == 4 {
                        career.progress.insert(n[0] as u32, Progress { races_done: n[1] as u8, completed: n[2] as u32, best_place: n[3] as u8 });
                    }
                }
                "chosen" => {
                    let n = nums(value);
                    if n.len() == 2 {
                        career.chosen_tournament = n[0] as usize;
                        career.chosen_car = n[1] as usize;
                    }
                }
                _ => {}
            }
        }
        (seen && !career.cars.is_empty()).then_some(career)
    }

    /// Loads the player's career from `dir`, if one was started.
    pub fn load_from(dir: &std::path::Path, player: &str) -> Option<Career> {
        Self::parse(&std::fs::read_to_string(Self::path_in(dir, player)).ok()?)
    }

    pub fn save_to(&self, dir: &std::path::Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        std::fs::write(Self::path_in(dir, &self.player), self.to_text())
    }

    /// The tournaments the player can enter now, in file order. An era's regular tournaments open
    /// when the career year reaches it; its bonus tournament opens once every regular one of the
    /// era is complete (**guess**: from the descriptions, "a bonus for completing the era").
    pub fn open_tournaments<'a>(&self, all: &'a [Tournament]) -> Vec<&'a Tournament> {
        let era = era_of_year(self.year);
        all.iter()
            .filter(|t| t.era == era)
            .filter(|t| !t.bonus || all.iter().filter(|o| o.era == era && !o.bonus).all(|o| self.progress(o.id).completed > 0))
            .collect()
    }

    pub fn progress(&self, id: u32) -> Progress {
        self.progress.get(&id).copied().unwrap_or_default()
    }

    /// The tracks a tournament races on, in order. The modern era's records list none; those use
    /// the 1970 era's six (**guess**).
    pub fn race_tracks(t: &Tournament, all: &[Tournament]) -> Vec<i16> {
        if !t.tracks.is_empty() {
            return t.tracks.clone();
        }
        all.iter().find(|o| o.era == ERAS[1].0 && !o.tracks.is_empty()).map(|o| o.tracks.clone()).unwrap_or_default()
    }

    /// Money for finishing `place` (1-based) in one race of `t`: the place's prize shared over the
    /// tournament's races (**guess**: retail's payout timing was not found).
    pub fn race_prize(t: &Tournament, races: usize, place: usize) -> u32 {
        match place.checked_sub(1).and_then(|p| t.prizes.get(p)) {
            Some(prize) => prize / races.max(1) as u32,
            None => 0,
        }
    }

    /// Records a finished race of `t` and pays the player. `place` is 1-based; 0 means the race
    /// was not finished (no pay, no progress).
    pub fn finish_race(&mut self, t: &Tournament, all: &[Tournament], place: usize) -> Payout {
        if place == 0 {
            return Payout { place, amount: 0, tournament_done: false, new_year: None };
        }
        let races = Self::race_tracks(t, all).len();
        let amount = Self::race_prize(t, races, place);
        self.cash += amount as i64;
        // The raced car gains miles and the used-car lot gets new stock.
        let at = self.chosen_car.min(self.cars.len().saturating_sub(1));
        if let Some(c) = self.cars.get_mut(at) {
            c.mileage += career_shop::RACE_MILES;
        }
        self.market_round += 1;
        self.taken.clear();
        let p = self.progress.entry(t.id).or_default();
        p.best_place = if p.best_place == 0 { place.min(255) as u8 } else { p.best_place.min(place.min(255) as u8) };
        p.races_done += 1;
        let done = p.races_done as usize >= races;
        if done {
            p.races_done = 0;
            p.completed += 1;
        }
        // The era ends when all its regular tournaments are complete (**guess**).
        let era = era_of_year(self.year);
        let mut new_year = None;
        if done && self.year < END_YEAR && all.iter().filter(|o| o.era == era && !o.bonus).all(|o| self.progress(o.id).completed > 0) {
            let year = next_year(self.year);
            self.year = year;
            new_year = Some(year);
        }
        Payout { place, amount, tournament_done: done, new_year }
    }

    /// The race the player enters next in `t` (0-based into its tracks).
    pub fn next_race(&self, t: &Tournament) -> usize {
        self.progress(t.id).races_done as usize
    }

    // ---- the money rules --------------------------------------------------------------------

    fn catalogue_car<'a>(&self, shop: &Shop<'a>, id: u16) -> Option<&'a crate::catalog::Car> {
        shop.cars.iter().find(|c| c.id == id)
    }

    /// What an installed part is worth now (its list price grown by the years since its model year).
    fn installed_value(&self, shop: &Shop, id: u32) -> i64 {
        shop.parts.iter().find(|p| p.id == id).map_or(0, |p| career_shop::part_value(p.price, self.year - p.year))
    }

    /// What owned car `i` would sell for (retail's value rule, `career_shop::car_value`).
    pub fn value_of(&self, i: usize, shop: &Shop) -> i64 {
        let Some(car) = self.cars.get(i) else { return 0 };
        let base = self.catalogue_car(shop, car.id).map_or(0, |c| c.price as i64);
        let parts: i64 = car.parts.iter().map(|p| self.installed_value(shop, *p)).sum();
        career_shop::car_value(base, car.mileage, car.extras, 0, parts)
    }

    /// The used-car lot for now: this round's cars that are still there.
    pub fn market(&self, shop: &Shop, paints: &dyn Fn(&crate::catalog::Car) -> Vec<usize>) -> Vec<(usize, UsedCar)> {
        let seed = (self.player.bytes().fold(7u64, |h, b| h.wrapping_mul(31).wrapping_add(b as u64))) ^ ((self.market_round as u64) << 20);
        career_shop::used_market(shop.cars, self.year, seed, paints).into_iter().enumerate().filter(|(i, _)| !self.taken.contains(i)).collect()
    }

    fn pay(&mut self, price: i64) -> Result<(), ShopError> {
        if self.cash < price {
            return Err(ShopError::NotEnoughCash);
        }
        self.cash -= price;
        Ok(())
    }

    /// Buys a new car from the dealership at its list price; it becomes the chosen car.
    pub fn buy_new(&mut self, car_id: u16, paint: usize, shop: &Shop) -> Result<i64, ShopError> {
        let price = self.catalogue_car(shop, car_id).map(|c| c.price as i64).filter(|p| *p > 0).ok_or(ShopError::UnknownItem)?;
        self.pay(price)?;
        self.cars.push(OwnedCar::stock(car_id, paint, price));
        self.chosen_car = self.cars.len() - 1;
        Ok(price)
    }

    /// Buys lot car `lot_index` (its index in the round's lot) at its asking price.
    pub fn buy_used(&mut self, lot_index: usize, lot: &[(usize, UsedCar)]) -> Result<i64, ShopError> {
        let (_, used) = lot.iter().find(|(i, _)| *i == lot_index).ok_or(ShopError::UnknownItem)?;
        self.pay(used.price)?;
        let mut car = OwnedCar::stock(used.car, used.paint, used.price);
        car.mileage = used.mileage;
        car.extras = used.extras;
        self.cars.push(car);
        self.taken.push(lot_index);
        self.chosen_car = self.cars.len() - 1;
        Ok(used.price)
    }

    /// Sells owned car `i` for its value. The last car cannot be sold.
    pub fn sell(&mut self, i: usize, shop: &Shop) -> Result<i64, ShopError> {
        if self.cars.len() <= 1 {
            return Err(ShopError::LastCar);
        }
        if i >= self.cars.len() {
            return Err(ShopError::UnknownItem);
        }
        let value = self.value_of(i, shop);
        self.cars.remove(i);
        self.cash += value;
        self.chosen_car = self.chosen_car.min(self.cars.len() - 1);
        Ok(value)
    }

    /// The price of buying part `id` now.
    pub fn part_price(&self, shop: &Shop, id: u32) -> i64 {
        self.installed_value(shop, id)
    }

    /// Buys and fits parts on car `i`. A part replaces the one fitted in its kind and category (which
    /// is sold back at its value). All or nothing: the total must be affordable.
    pub fn fit_parts(&mut self, i: usize, ids: &[u32], shop: &Shop) -> Result<i64, ShopError> {
        if i >= self.cars.len() {
            return Err(ShopError::UnknownItem);
        }
        let slot = |id: u32| shop.parts.iter().find(|p| p.id == id).map(|p| (p.kind, p.category.clone()));
        let mut refund = 0;
        let mut total = 0;
        let mut parts = self.cars[i].parts.clone();
        for id in ids {
            let Some(s) = slot(*id) else { return Err(ShopError::UnknownItem) };
            if parts.contains(id) {
                continue;
            }
            parts.retain(|old| {
                let same = slot(*old).as_ref() == Some(&s);
                if same {
                    refund += self.installed_value(shop, *old);
                }
                !same
            });
            total += self.installed_value(shop, *id);
            parts.push(*id);
        }
        if self.cash + refund < total {
            return Err(ShopError::NotEnoughCash);
        }
        self.cash += refund - total;
        self.cars[i].parts = parts;
        Ok(total - refund)
    }

    /// Takes part `id` off car `i` and pays its value.
    pub fn sell_part(&mut self, i: usize, id: u32, shop: &Shop) -> Result<i64, ShopError> {
        let value = self.installed_value(shop, id);
        let car = self.cars.get_mut(i).ok_or(ShopError::UnknownItem)?;
        let before = car.parts.len();
        car.parts.retain(|p| *p != id);
        if car.parts.len() == before {
            return Err(ShopError::UnknownItem);
        }
        self.cash += value;
        Ok(value)
    }

    /// The handling changes of car `i`'s fitted parts.
    pub fn tuning_of(&self, i: usize, shop: &Shop) -> crate::car_sim::Tuning {
        let Some(owned) = self.cars.get(i) else { return Default::default() };
        let Some(car) = self.catalogue_car(shop, owned.id) else { return Default::default() };
        career_shop::tuning(&career_shop::parts_for(shop.parts, car, self.year), &owned.parts)
    }

    /// Paints owned car `i` (the paint shop; **guess**: free).
    pub fn repaint(&mut self, i: usize, paint: usize) {
        if let Some(c) = self.cars.get_mut(i) {
            c.paint = paint;
        }
    }

    // ---- what the screens show -------------------------------------------------------------

    /// The Tournament screen's dropdowns, keyed by accessor (`Tournament.EventType`,
    /// `Tournament.Tournament`, `OwnedCars.Car`, `Tournament.RemainingTracks`).
    pub fn choices(&self, all: &[Tournament], cars: &[crate::catalog::Car], tracks: &[crate::catalog::Track], locale: &Locale, language: u8) -> BTreeMap<String, Choice> {
        let s = |id: u32| locale.text(id, language).map(|t| t.trim().to_string()).unwrap_or_default();
        let open = self.open_tournaments(all);
        let mut out = BTreeMap::new();
        // The event type (the tournament's short name) and the tournament (its full name) are one
        // list seen two ways.
        let chosen = self.chosen_tournament.min(open.len().saturating_sub(1));
        let rows = |f: &dyn Fn(&Tournament) -> u32| open.iter().map(|t| Row::item(s(f(t)))).collect::<Vec<_>>();
        let closed = |f: &dyn Fn(&Tournament) -> u32| open.get(chosen).map(|t| s(f(t))).unwrap_or_default();
        // The event type list holds one entry, "tournaments" (string 470; retail footage). The
        // era name is shown by the screen's era banner instead.
        let era = s(470);
        out.insert("Tournament.EventType".into(), Choice { rows: vec![Row::item(era.clone())], selected: 0, closed: era, closed_swatch: None });
        out.insert("Tournament.Tournament".into(), Choice { rows: rows(&|t| t.name), selected: chosen, closed: closed(&|t| t.short), closed_swatch: None });
        let owned: Vec<String> = self.cars.iter().map(|c| cars.iter().find(|k| k.id == c.id).map(|k| k.name.clone()).unwrap_or_default()).collect();
        let car = self.chosen_car.min(owned.len().saturating_sub(1));
        let short = self.cars.get(car).and_then(|c| cars.iter().find(|k| k.id == c.id)).map(|k| k.short.clone()).unwrap_or_default();
        out.insert("OwnedCars.Car".into(), Choice { rows: owned.iter().map(|n| Row::item(n.clone())).collect(), selected: car, closed: short, closed_swatch: None });
        if let Some(t) = open.get(chosen) {
            let names: Vec<String> = Self::race_tracks(t, all)
                .iter()
                .skip(self.next_race(t))
                .filter_map(|id| tracks.iter().find(|k| k.id == *id))
                .map(|k| s(k.name_string))
                .collect();
            out.insert("Tournament.RemainingTracks".into(), Choice { closed: names.first().cloned().unwrap_or_default(), rows: names.into_iter().map(Row::item).collect(), selected: 0, closed_swatch: None });
        }
        out
    }

    /// The `AccessorWatch` texts of the Tournament and Pre-race screens, plus the account.
    pub fn watches(&self, all: &[Tournament], tracks: &[crate::catalog::Track], locale: &Locale, language: u8) -> BTreeMap<String, String> {
        let s = |id: u32| locale.text(id, language).map(|t| t.trim().to_string()).unwrap_or_default();
        let mut out = BTreeMap::new();
        out.insert("Tournament.Data(ACCOUNT)".into(), money(self.cash));
        // The era banner at the top of every career screen: "CLASSIC ERA / 1950", the other two dim.
        out.insert("Career.Era".into(), era_of_year(self.year).to_string());
        out.insert("Career.Year".into(), self.year.to_string());
        for (n, (code, _)) in ERAS.iter().enumerate() {
            out.insert(format!("Career.EraName{n}"), s(*code));
        }
        // The era-change message (`VictoryMessage`): the retail strings 19000-19006 are the golden
        // era's text (header, the prize car offer, the bonus race at Monaco); the other eras reuse
        // them with their own name in the header (**guess**: only golden's strings exist).
        let era = era_of_year(self.year);
        let header = if era == 522 { s(19001) } else { format!("YOU HAVE PROGRESSED INTO {}!", s(era)) };
        out.insert("Tournament.Data(ENDERAHEADER)".into(), header);
        let text: Vec<String> = [19005, 19002, 19003, 19004, 19006, 19000].iter().map(|id| s(*id)).collect();
        out.insert("Tournament.Data(ENDERADESCRIP)".into(), text.join(" "));
        let open = self.open_tournaments(all);
        let Some(t) = open.get(self.chosen_tournament.min(open.len().saturating_sub(1))) else { return out };
        out.insert("Tournament.Tournament".into(), s(t.name));
        out.insert("Tournament.Tournament(SHORT)".into(), s(t.short));
        out.insert("Tournament.Tournament(DESCRIPTION)".into(), s(t.description));
        out.insert("Tournament.Tournament(CARRESTRICTION)".into(), s(t.restriction));
        out.insert("Tournament.Tournament(PRIZEMONEY)".into(), money(t.prizes[0] as i64));
        out.insert("Tournament.Tournament(ENTRYFEE)".into(), money(t.entry_fee as i64));
        out.insert("Tournament.Tournament(TIMESCOMPLETED)".into(), self.progress(t.id).completed.to_string());
        let race_tracks = Self::race_tracks(t, all);
        let next = race_tracks.get(self.next_race(t)).and_then(|id| tracks.iter().find(|k| k.id == *id));
        let difficulty = s(crate::quick_race::DIFFICULTY_FIRST + t.difficulty.min(2) as u32);
        out.insert("Tournament.Tournament(DIFFICULTY)".into(), difficulty);
        if let Some(k) = next {
            out.insert("Tournament.Tournament(LOCATION)".into(), s(k.name_string + 60));
            // The tournament screen's small track picture and the list of its locations (the next
            // one lit), drawn by the `TrackSmall2dSpline` control.
            if !k.art[2].is_empty() {
                out.insert("Career.Spline".into(), format!("TrackArt/{}.fsh", k.art[2]));
            }
        }
        let names: Vec<String> = race_tracks
            .iter()
            .filter_map(|id| tracks.iter().find(|k| k.id == *id))
            .map(|k| {
                let upper = s(k.name_string + 60);
                if upper.is_empty() { s(k.name_string).to_uppercase() } else { upper }
            })
            .collect();
        out.insert("Career.TrackList".into(), names.join("|"));
        out.insert("Career.TrackNext".into(), self.next_race(t).to_string());
        out.insert("Tournament.Tournament(RECORDTIME)".into(), "--:--.--".into());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: u32, era: u32, tracks: &[u16], first_prize: u32) -> Vec<u8> {
        let mut r = vec![0u8; RECORD];
        r[0..4].copy_from_slice(&id.to_le_bytes());
        r[4..8].copy_from_slice(&era.to_le_bytes());
        r[10..12].copy_from_slice(&(tracks.len() as u16).to_le_bytes());
        for (n, t) in tracks.iter().enumerate() {
            r[12 + 2 * n..14 + 2 * n].copy_from_slice(&t.to_le_bytes());
        }
        for n in 0..PLACES {
            r[0xec + 4 * n..0xf0 + 4 * n].copy_from_slice(&(first_prize.saturating_sub(500 * n as u32)).to_le_bytes());
        }
        r[0x118 + 2..0x118 + 4].copy_from_slice(&76u16.to_le_bytes());
        r
    }

    fn table() -> Vec<Tournament> {
        let mut data = record(1, 521, &[9, 16], 4000);
        data.extend(record(2, 521, &[9], 3000));
        data.extend(record(31, 521, &[9], 1000));
        data.extend(record(3, 522, &[11], 8000));
        data.extend(record(40, 524, &[], 100));
        parse_tournaments(&data)
    }

    #[test]
    fn parses_career_tournaments_only() {
        let t = table();
        assert_eq!(t.len(), 4, "the club event (era 524) is left out");
        assert_eq!((t[0].id, t[0].era, t[0].tracks.clone()), (1, 521, vec![9, 16]));
        assert_eq!(t[0].prizes[0], 4000);
        assert_eq!(t[0].prizes[7], 500);
        assert_eq!(t[0].opponent_cars[0], 76);
        assert!(t[2].bonus && !t[0].bonus);
    }

    #[test]
    fn money_has_commas() {
        assert_eq!(money(0), "0");
        assert_eq!(money(5000), "5,000");
        assert_eq!(money(1234567), "1,234,567");
        assert_eq!(money(-250), "-250");
    }

    #[test]
    fn save_text_round_trips() {
        let t = table();
        let mut c = Career::new("Greg");
        c.finish_race(&t[0], &t, 2);
        c.chosen_tournament = 1;
        let back = Career::parse(&c.to_text()).unwrap();
        assert_eq!(back, c);
        assert!(Career::parse("player = x").is_none(), "needs the version line and a car");
    }

    #[test]
    fn a_new_career_opens_the_first_era_without_the_bonus() {
        let t = table();
        let c = Career::new("A");
        assert_eq!(c.year, 1950);
        let open: Vec<u32> = c.open_tournaments(&t).iter().map(|t| t.id).collect();
        assert_eq!(open, vec![1, 2]);
        assert_eq!(c.cars, vec![OwnedCar::stock(STARTER_CAR, STARTER_PAINT, STARTER_PRICE)]);
    }

    #[test]
    fn places_pay_the_prize_table_shared_over_the_races() {
        let t = table();
        let mut c = Career::new("A");
        let p = c.finish_race(&t[0], &t, 1);
        assert_eq!((p.amount, p.tournament_done), (2000, false), "4000 over two races");
        assert_eq!(c.cash, 2000);
        let p = c.finish_race(&t[0], &t, 3);
        assert_eq!(p.amount, 1500);
        assert!(p.tournament_done);
        assert_eq!((c.progress(1).completed, c.progress(1).races_done, c.progress(1).best_place), (1, 0, 1));
        assert_eq!(c.finish_race(&t[1], &t, 0).amount, 0, "an unfinished race pays nothing");
        assert_eq!(Career::race_prize(&t[0], 2, 9), 0, "ninth place is not paid");
    }

    #[test]
    fn finishing_an_era_opens_the_next_and_the_bonus() {
        let t = table();
        let mut c = Career::new("A");
        c.finish_race(&t[0], &t, 1);
        c.finish_race(&t[0], &t, 1);
        assert_eq!(c.year, 1950, "tournament 2 is still open");
        assert_eq!(c.open_tournaments(&t).iter().map(|t| t.id).collect::<Vec<_>>(), vec![1, 2]);
        let p = c.finish_race(&t[1], &t, 4);
        assert_eq!(p.new_year, Some(1970));
        assert_eq!(c.year, 1970);
        assert_eq!(c.open_tournaments(&t).iter().map(|t| t.id).collect::<Vec<_>>(), vec![3]);
    }

    #[test]
    fn the_bonus_waits_for_the_regular_tournaments() {
        let t = table();
        let mut c = Career::new("A");
        c.progress.insert(1, Progress { completed: 1, ..Default::default() });
        assert!(!c.open_tournaments(&t).iter().any(|t| t.bonus));
        c.progress.insert(2, Progress { completed: 1, ..Default::default() });
        assert!(c.open_tournaments(&t).iter().any(|t| t.bonus));
    }

    fn shop_data() -> (Vec<crate::catalog::Car>, Vec<Part>) {
        let car = |id: u16, price: u32| crate::catalog::Car { id, price, year_from: 1950, year_to: 1960, ai_name: "356".into(), ..Default::default() };
        let part = |id: u32, kind: u16, price: i64| Part { id, group: "356".into(), kind, category: "Turbo".into(), year: 1950, price, ..Default::default() };
        (vec![car(76, 11_000), car(100, 12_700)], vec![part(1, 1, 500), part(2, 1, 900), part(3, 2, 300)])
    }

    #[test]
    fn buying_selling_and_the_last_car() {
        let (cars, parts) = shop_data();
        let shop = Shop { cars: &cars, parts: &parts };
        let mut c = Career::new("A");
        assert_eq!(c.buy_new(100, 5, &shop), Err(ShopError::NotEnoughCash));
        c.cash = 20_000;
        assert_eq!(c.buy_new(100, 5, &shop), Ok(12_700));
        assert_eq!((c.cash, c.cars.len(), c.chosen_car), (7_300, 2, 1));
        assert_eq!(c.buy_new(9, 1, &shop), Err(ShopError::UnknownItem));
        // the starter car (11,000, no miles) sells for the price less the first damage step
        assert_eq!(c.sell(0, &shop), Ok(10_890));
        assert_eq!((c.cash, c.cars.len(), c.chosen_car), (18_190, 1, 0));
        assert_eq!(c.sell(0, &shop), Err(ShopError::LastCar));
    }

    #[test]
    fn used_cars_keep_their_miles_and_leave_the_lot() {
        let (cars, parts) = shop_data();
        let shop = Shop { cars: &cars, parts: &parts };
        let mut c = Career::new("A");
        c.year = 1975;
        c.cash = 1_000_000;
        let paints = |_: &crate::catalog::Car| vec![2usize];
        let lot = c.market(&shop, &paints);
        assert!(!lot.is_empty());
        let (index, used) = lot[0].clone();
        assert_eq!(c.buy_used(index, &lot), Ok(used.price));
        assert_eq!(c.cars.last().unwrap().mileage, used.mileage);
        assert!(!c.market(&shop, &paints).iter().any(|(i, _)| *i == index), "bought cars leave the lot");
        assert_eq!(c.buy_used(index, &c.market(&shop, &paints)), Err(ShopError::UnknownItem));
    }

    #[test]
    fn parts_replace_each_other_and_change_the_value_and_handling() {
        let (cars, parts) = shop_data();
        let shop = Shop { cars: &cars, parts: &parts };
        let mut c = Career::new("A");
        c.cash = 2_000;
        let stock_value = c.value_of(0, &shop);
        assert_eq!(c.fit_parts(0, &[1, 3], &shop), Ok(800));
        assert_eq!(c.value_of(0, &shop), stock_value + 800);
        let low = c.tuning_of(0, &shop);
        assert!(low.power > 1.0 && low.brake > 1.0);
        // the dearer turbo replaces the cheaper one, which is sold back
        assert_eq!(c.fit_parts(0, &[2], &shop), Ok(400));
        assert_eq!(c.cars[0].parts.len(), 2);
        assert!(c.tuning_of(0, &shop).power > low.power);
        assert_eq!(c.cash, 2_000 - 800 - 400);
        assert_eq!(c.fit_parts(0, &[2], &shop), Ok(0), "already fitted");
        c.cash = 0;
        assert_eq!(c.sell_part(0, 3, &shop), Ok(300));
        assert_eq!(c.cash, 300);
        assert_eq!(c.fit_parts(0, &[1], &shop), Ok(-400), "swapping back to the cheaper part pays the difference");
    }

    #[test]
    fn racing_adds_miles_and_refreshes_the_lot() {
        let t = table();
        let mut c = Career::new("A");
        c.taken = vec![1];
        c.finish_race(&t[0], &t, 1);
        assert_eq!(c.cars[0].mileage, career_shop::RACE_MILES);
        assert_eq!((c.market_round, c.taken.len()), (1, 0));
        let mut d = Career::new("B");
        d.cars.push(OwnedCar { id: 5, paint: 2, mileage: 123, extras: 4, bought_for: 900, parts: vec![7, 8] });
        d.taken = vec![0, 3];
        assert_eq!(Career::parse(&d.to_text()).unwrap(), d);
        assert_eq!(Career::parse("version = 1
player = x
car = 76,19").unwrap().cars[0], OwnedCar::stock(76, 19, 0), "first-slice files still load");
    }

    #[test]
    fn years_map_to_eras() {
        assert_eq!((era_of_year(1950), era_of_year(1975), era_of_year(1989), era_of_year(2000)), (521, 522, 523, 523));
        assert_eq!((next_year(1950), next_year(1970), next_year(1989)), (1970, 1989, END_YEAR));
    }
}
