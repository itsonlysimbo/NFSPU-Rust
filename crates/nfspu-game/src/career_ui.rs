//! What the career's shop screens show and do: the dealership, used-car lot, garage, paint shop and
//! parts screens, on top of `career.rs` (money rules) and `career_shop.rs` (prices, parts).
//!
//! The layouts only name accessors (`DealerCar.Car`, `Parts.AllParts` ...); this module fills them.
//! Which rows exist and what the buttons do is ours (retail's lists were not decoded), but prices,
//! value and the lot come from the verified rules in `career_shop.rs`.

use crate::career::{money, Career, Shop, ShopError};
use crate::career_shop::{self, Part, UsedCar};
use crate::catalog::{self, Car};
use crate::menu::{Choice, ListData, Row};
use nfspu_formats::fe::Locale;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The screen state the shop pages keep between visits.
#[derive(Debug, Default)]
pub struct ShopUi {
    pub parts: Vec<Part>,
    game_dir: PathBuf,
    /// Positions in the dealer list, the dealer car's colour list, the used-car lot, and the two
    /// parts lists.
    pub dealer: usize,
    pub dealer_paint: usize,
    pub used: usize,
    pub all_sel: usize,
    pub basket_sel: usize,
    /// Part ids picked for purchase.
    pub basket: Vec<u32>,
    /// The test-drive popup's location (an index into `test_tracks`) and the page showing.
    pub test_track: usize,
    pub page: String,
    paints: Mutex<HashMap<String, Vec<usize>>>,
}

/// Everything the screens read, built from the career each time it changes.
#[derive(Debug, Default)]
pub struct View {
    pub choices: BTreeMap<String, Choice>,
    pub watch: BTreeMap<String, String>,
    pub lists: BTreeMap<String, ListData>,
    pub hidden: Vec<String>,
    /// The car the 3D room shows on this page: (index into the catalogue, paint section).
    pub showcase: Option<(usize, usize)>,
}

fn kind_label(kind: u16) -> &'static str {
    match kind {
        1 => "Engine",
        2 => "Brakes",
        3 => "Gearbox",
        4 => "Springs",
        5 => "Shocks",
        6 => "Aero",
        7 => "Body",
        8 => "Sway bars",
        9 => "Tyres",
        10 => "Trim",
        _ => "Part",
    }
}

/// Which car a test drive uses and where (the menu turns it into a race with no rivals).
#[derive(Debug, Clone, PartialEq)]
pub struct TestDrive {
    /// Index into the catalogue and the paint section to drive it in.
    pub car: usize,
    pub paint: usize,
    /// `nfs5.trk` id of the location.
    pub track: i16,
    /// The owned car's position, when the drive uses one of the player's own cars (its parts count).
    pub owned: Option<usize>,
}

/// The locations a test drive offers: the tracks the front end never locks (`locked` 0, the skid
/// pad left out: the footage lists Cote d'Azur, Pyrenees, Normandie and Monte Carlo 3), then any
/// track of a tournament open now (**guess** for the later eras).
pub fn test_tracks(tracks: &[catalog::Track], open_ids: &[i16]) -> Vec<usize> {
    let mut out: Vec<usize> = (0..tracks.len()).filter(|&i| tracks[i].locked == 0 && tracks[i].file != "skidpad").collect();
    for id in open_ids {
        if let Some(i) = tracks.iter().position(|t| t.id == *id) {
            if !out.contains(&i) {
                out.push(i);
            }
        }
    }
    out
}

impl ShopUi {
    pub fn load(game_dir: &Path) -> ShopUi {
        let parts = std::fs::read(game_dir.join("FEData").join("Data").join("nfs5.prt")).map(|b| career_shop::parse_parts(&b)).unwrap_or_default();
        ShopUi { parts, game_dir: game_dir.to_path_buf(), ..Default::default() }
    }

    /// The `.clr` paint sections of a catalogue car (cached).
    pub fn paint_sections(&self, car: &Car) -> Vec<usize> {
        self.paints.lock().unwrap().entry(car.model.clone()).or_insert_with(|| catalog::load_paints(&self.game_dir, &car.model).iter().map(|p| p.section).collect()).clone()
    }

    fn swatches(&self, car: &Car) -> Vec<catalog::Paint> {
        catalog::load_paints(&self.game_dir, &car.model)
    }

    fn lot(&self, c: &Career, cars: &[Car]) -> Vec<(usize, UsedCar)> {
        c.market(&Shop { cars, parts: &self.parts }, &|car| self.paint_sections(car))
    }

    fn rows_for_cars(cars: &[Car], idx: impl Iterator<Item = (usize, Option<String>)>) -> Vec<Row> {
        idx.map(|(i, extra)| {
            let (year, model) = cars[i].year_and_model();
            Row::Item { lead: Some(year), text: extra.map_or(model.clone(), |e| format!("{model}  {e}")), swatch: None, divider: false }
        })
        .collect()
    }

    /// The screens' data for the career `c` while page `page` is showing.
    pub fn view(&self, c: &Career, cars: &[Car], tracks: &[catalog::Track], open_ids: &[i16], locale: &Locale, language: u8, page: &str) -> View {
        let s = |id: u32| locale.text(id, language).map(|t| t.trim().to_string()).unwrap_or_default();
        let shop = Shop { cars, parts: &self.parts };
        let mut v = View::default();
        let car_at = |id: u16| cars.iter().position(|k| k.id == id);
        let owned_index = c.chosen_car.min(c.cars.len().saturating_sub(1));

        // Dealership: new cars of the career year.
        let dealer = career_shop::dealer_cars(cars, c.year);
        let dealer_at = self.dealer.min(dealer.len().saturating_sub(1));
        if let Some(&ci) = dealer.get(dealer_at) {
            let car = &cars[ci];
            let paints = self.swatches(car);
            let paint_at = self.dealer_paint.min(paints.len().saturating_sub(1));
            v.choices.insert("DealerCar.Car".into(), Choice { rows: Self::rows_for_cars(cars, dealer.iter().map(|i| (*i, Some(money(cars[*i].price as i64))))), selected: dealer_at, closed: car.short.clone(), closed_swatch: None });
            let swatch = |rgb: [u8; 3]| Row::Item { lead: None, text: String::new(), swatch: Some(rgb), divider: false };
            v.choices.insert("DealerCar.Colour".into(), Choice { rows: paints.iter().map(|p| swatch(p.body)).collect(), selected: paint_at, closed: String::new(), closed_swatch: paints.get(paint_at).map(|p| p.body) });
            v.watch.insert("DealerCar.Car".into(), car.name.clone());
            v.watch.insert("DealerCar.Car(CLASSLISTING)".into(), s(car.class));
            v.watch.insert("Tournament.Data(DEALERCARPRICE)".into(), money(car.price as i64));
            if page.eq_ignore_ascii_case("dealership") {
                v.showcase = Some((ci, paints.get(paint_at).map_or(0, |p| p.section)));
            }
        }
        // The interior colour, the decal package and the stripe colours have no data here: one
        // plain entry each, so the boxes do not show another list.
        let plain = |label: &str| Choice { rows: vec![Row::item(label.to_string())], selected: 0, closed: label.to_string(), closed_swatch: None };
        let grey = Choice { rows: vec![Row::Item { lead: None, text: String::new(), swatch: Some([60, 60, 60]), divider: false }], selected: 0, closed: String::new(), closed_swatch: Some([60, 60, 60]) };
        for k in ["DealerCar.IntColour", "OwnedCar.IntColour", "OwnedCar.S1Colour", "OwnedCar.S2Colour"] {
            v.choices.insert(k.into(), grey.clone());
        }
        v.choices.insert("OwnedCar.PackageNum".into(), plain("-"));

        // Used cars.
        let lot = self.lot(c, cars);
        let used_at = self.used.min(lot.len().saturating_sub(1));
        v.choices.insert(
            "UsedCar.Car".into(),
            Choice {
                rows: Self::rows_for_cars(cars, lot.iter().filter_map(|(_, u)| car_at(u.car).map(|i| (i, Some(money(u.price)))))),
                selected: used_at,
                closed: lot.get(used_at).and_then(|(_, u)| car_at(u.car)).map(|i| cars[i].short.clone()).unwrap_or_default(),
                closed_swatch: None,
            },
        );
        if let Some((_, u)) = lot.get(used_at) {
            if let Some(ci) = car_at(u.car) {
                v.watch.insert("UsedCar.Car(CLASSLISTING)".into(), format!("{}  -  {} MI  -  {}", s(cars[ci].class), u.mileage, money(u.price)));
                if page.eq_ignore_ascii_case("usedcars") {
                    v.showcase = Some((ci, u.paint));
                }
            }
        }

        // Garage, paint shop and parts: the chosen owned car.
        if let Some(owned) = c.cars.get(owned_index) {
            if let Some(ci) = car_at(owned.id) {
                let car = &cars[ci];
                v.watch.insert("OwnedCars.Car".into(), car.name.clone());
                v.watch.insert("OwnedCars.Car(CURRENTVALUE)".into(), money(c.value_of(owned_index, &shop)));
                v.watch.insert("OwnedCars.Car(PURCHASEPRICE)".into(), money(owned.bought_for));
                v.watch.insert("OwnedCars.Car(OVERALLDAMAGE)".into(), "0%".into());
                let paints = self.swatches(car);
                let at = paints.iter().position(|p| p.section == owned.paint).unwrap_or(0);
                let swatch = |rgb: [u8; 3]| Row::Item { lead: None, text: String::new(), swatch: Some(rgb), divider: false };
                v.choices.insert("OwnedCar.ExtColour".into(), Choice { rows: paints.iter().map(|p| swatch(p.body)).collect(), selected: at, closed: String::new(), closed_swatch: paints.get(at).map(|p| p.body) });
                if ["garage", "paintshop", "parts"].iter().any(|p| page.eq_ignore_ascii_case(p)) {
                    v.showcase = Some((ci, owned.paint));
                }

                // Parts: everything that fits the car and exists now; fitted parts carry a star.
                let fits = career_shop::parts_for(&self.parts, car, c.year);
                let name = |p: &Part| {
                    let n = s(p.name);
                    if n.is_empty() { p.category.clone() } else { n }
                };
                let label = |p: &Part| if p.category.is_empty() { kind_label(p.kind).to_string() } else { p.category.clone() };
                let all: Vec<[String; 3]> = fits
                    .iter()
                    .map(|p| [label(p), format!("{}{}", if owned.parts.contains(&p.id) { "* " } else { "" }, name(p)), money(c.part_price(&shop, p.id))])
                    .collect();
                v.lists.insert("Parts.AllParts".into(), ListData { selected: (!all.is_empty()).then(|| self.all_sel.min(all.len() - 1)), rows: all });
                let basket: Vec<[String; 3]> = self.basket.iter().filter_map(|id| fits.iter().find(|p| p.id == *id)).map(|p| [label(p), name(p), money(c.part_price(&shop, p.id))]).collect();
                let total: i64 = self.basket.iter().map(|id| c.part_price(&shop, *id)).sum();
                v.watch.insert("Parts.ShoppingList(TOTAL)".into(), money(total));
                v.lists.insert("Parts.ShoppingList".into(), ListData { selected: (!basket.is_empty()).then(|| self.basket_sel.min(basket.len() - 1)), rows: basket });
            }
        }
        // The test-drive popup's location list.
        if page.to_ascii_lowercase().starts_with("testdrive") {
            let list = test_tracks(tracks, open_ids);
            let at = self.test_track.min(list.len().saturating_sub(1));
            let rows: Vec<Row> = list.iter().map(|&i| Row::item(s(tracks[i].name_string))).collect();
            let closed = list.get(at).map(|&i| s(tracks[i].name_string)).unwrap_or_default();
            v.choices.insert("Track1.Track".into(), Choice { rows, selected: at, closed, closed_swatch: None });
            // The popup names the car being driven.
            let (name, _) = self.test_car(c, cars, page);
            v.watch.insert("OwnedCars.Car".into(), name.clone());
            v.watch.insert("DealerCar.Car".into(), name.clone());
            v.watch.insert("UsedCar.Car".into(), name);
        }
        // Selling and "don't sell" share a place in the layout; selling is immediate here.
        v.hidden.push("ID_REMOVEFMBUTTON".into());
        v
    }

    /// The car a test-drive popup opened from `page` drives: its shown name and the car.
    fn test_car(&self, c: &Career, cars: &[Car], page: &str) -> (String, Option<(usize, usize, Option<usize>)>) {
        let page = page.to_ascii_lowercase();
        let found = if page == "testdriveoval" {
            let list = career_shop::dealer_cars(cars, c.year);
            list.get(self.dealer.min(list.len().saturating_sub(1))).map(|&ci| (ci, self.paint_sections(&cars[ci]).get(self.dealer_paint).copied().unwrap_or(0), None))
        } else if page == "testdrivesquare" {
            let lot = self.lot(c, cars);
            lot.get(self.used.min(lot.len().saturating_sub(1))).and_then(|(_, u)| cars.iter().position(|k| k.id == u.car).map(|ci| (ci, u.paint, None)))
        } else {
            let at = c.chosen_car.min(c.cars.len().saturating_sub(1));
            c.cars.get(at).and_then(|o| cars.iter().position(|k| k.id == o.id).map(|ci| (ci, o.paint, Some(at))))
        };
        (found.map(|(ci, _, _)| cars[ci].name.clone()).unwrap_or_default(), found)
    }

    /// The test drive a button asks for, with the location picked in the popup. `page` is the
    /// screen or popup it was pressed on. Test driving costs nothing and changes nothing.
    pub fn test_drive(&self, c: &Career, cars: &[Car], tracks: &[catalog::Track], open_ids: &[i16], page: &str) -> Option<TestDrive> {
        let (_, found) = self.test_car(c, cars, page);
        let (car, paint, owned) = found?;
        let list = test_tracks(tracks, open_ids);
        let track = tracks[*list.get(self.test_track.min(list.len().saturating_sub(1)))?].id;
        Some(TestDrive { car, paint, track, owned })
    }

    /// A dropdown pick on a shop screen. True when the accessor is one of ours.
    pub fn pick(&mut self, accessor: &str, index: usize, c: &mut Career, cars: &[Car]) -> bool {
        match accessor {
            "DealerCar.Car" => {
                self.dealer = index;
                self.dealer_paint = 0;
            }
            "DealerCar.Colour" => self.dealer_paint = index,
            "Track1.Track" if self.page.to_ascii_lowercase().starts_with("testdrive") => self.test_track = index,
            "UsedCar.Car" => self.used = index,
            "OwnedCar.ExtColour" => {
                let at = c.chosen_car.min(c.cars.len().saturating_sub(1));
                if let Some(car) = c.cars.get(at).and_then(|o| cars.iter().find(|k| k.id == o.id)) {
                    if let Some(p) = self.swatches(car).get(index) {
                        c.repaint(at, p.section);
                    }
                }
            }
            "DealerCar.IntColour" | "OwnedCar.IntColour" | "OwnedCar.S1Colour" | "OwnedCar.S2Colour" | "OwnedCar.PackageNum" => {}
            _ => return false,
        }
        true
    }

    /// A list row click on the parts screen.
    pub fn list_pick(&mut self, accessor: &str, index: usize) -> bool {
        match accessor {
            "Parts.AllParts" => self.all_sel = index,
            "Parts.ShoppingList" => self.basket_sel = index,
            _ => return false,
        }
        true
    }

    /// A shop button. Returns the log line when it was one of ours (the career changed or the
    /// request was refused); `None` for any other callback.
    pub fn callback(&mut self, name: &str, page: &str, c: &mut Career, cars: &[Car]) -> Option<String> {
        let shop_parts = self.parts.clone();
        let shop = Shop { cars, parts: &shop_parts };
        let said = |r: Result<i64, ShopError>, what: &str, c: &Career| match r {
            Ok(amount) => format!("career: {what} for {} (cash {})", money(amount), money(c.cash)),
            Err(e) => format!("career: {what} refused: {e:?} (cash {})", money(c.cash)),
        };
        let owned = c.chosen_car.min(c.cars.len().saturating_sub(1));
        match name {
            "DEALERCARINC" | "DEALERCARDEC" => {
                let n = career_shop::dealer_cars(cars, c.year).len().max(1);
                self.dealer = if name == "DEALERCARINC" { (self.dealer + 1) % n } else { (self.dealer + n - 1) % n };
                self.dealer_paint = 0;
                Some(format!("career: dealership car {}", self.dealer))
            }
            "BUYCAR" if page.eq_ignore_ascii_case("dealership") => {
                let list = career_shop::dealer_cars(cars, c.year);
                let ci = *list.get(self.dealer.min(list.len().saturating_sub(1)))?;
                let paint = self.paint_sections(&cars[ci]).get(self.dealer_paint).copied().unwrap_or(0);
                let r = c.buy_new(cars[ci].id, paint, &shop);
                Some(said(r, &format!("bought a new {}", cars[ci].name), c))
            }
            "BUYCAR" => {
                let lot = self.lot(c, cars);
                let (index, used) = lot.get(self.used.min(lot.len().saturating_sub(1)))?.clone();
                let r = c.buy_used(index, &lot);
                let car = cars.iter().find(|k| k.id == used.car).map(|k| k.name.clone()).unwrap_or_default();
                self.used = self.used.saturating_sub(1);
                Some(said(r, &format!("bought a used {car} ({} mi)", used.mileage), c))
            }
            "SELLCAR" => {
                let car = c.cars.get(owned).and_then(|o| cars.iter().find(|k| k.id == o.id)).map(|k| k.name.clone()).unwrap_or_default();
                let r = c.sell(owned, &shop);
                Some(said(r, &format!("sold the {car}"), c))
            }
            "DONTSELLCAR" | "CHANGEDECAL" => Some(format!("career: {name} (nothing to do)")),
            "BUYPARTCALLBACK" => {
                let car = c.cars.get(owned).and_then(|o| cars.iter().find(|k| k.id == o.id))?;
                let fits = career_shop::parts_for(&self.parts, car, c.year);
                let part = fits.get(self.all_sel.min(fits.len().saturating_sub(1)))?;
                if !self.basket.contains(&part.id) && !c.cars[owned].parts.contains(&part.id) {
                    self.basket.push(part.id);
                }
                Some(format!("career: part {} in the shopping list", part.id))
            }
            "SELLPARTCALLBACK" => {
                let car = c.cars.get(owned).and_then(|o| cars.iter().find(|k| k.id == o.id))?;
                let fits = career_shop::parts_for(&self.parts, car, c.year);
                let id = fits.get(self.all_sel.min(fits.len().saturating_sub(1)))?.id;
                let r = c.sell_part(owned, id, &shop);
                Some(said(r, &format!("sold part {id}"), c))
            }
            "CLEARLISTCALLBACK" => {
                self.basket.clear();
                Some("career: shopping list cleared".into())
            }
            "REMOVEITEMCALLBACK" => {
                if self.basket_sel < self.basket.len() {
                    self.basket.remove(self.basket_sel);
                }
                Some("career: removed from the shopping list".into())
            }
            "PURCHASECALLBACK" => {
                let ids = std::mem::take(&mut self.basket);
                let r = c.fit_parts(owned, &ids, &shop);
                if r.is_err() {
                    self.basket = ids;
                }
                Some(said(r, "bought parts", c))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cars() -> Vec<Car> {
        let car = |id: u16, price: u32, from: i16, to: i16| Car { id, price, year_from: from, year_to: to, name: format!("'50 Car {id}"), short: format!("C{id}"), ai_name: "356".into(), ..Default::default() };
        vec![car(76, 11_000, 1950, 1954), car(100, 12_700, 1950, 1960)]
    }

    #[test]
    fn the_dealer_buy_button_buys_the_chosen_car() {
        let cars = cars();
        let mut ui = ShopUi::default();
        let mut c = Career::new("T");
        c.cash = 12_000;
        ui.dealer = 1;
        let line = ui.callback("BUYCAR", "dealership", &mut c, &cars).unwrap();
        assert!(line.contains("refused"), "{line}");
        c.cash = 13_000;
        let line = ui.callback("BUYCAR", "dealership", &mut c, &cars).unwrap();
        assert!(line.contains("bought a new"), "{line}");
        assert_eq!((c.cars.len(), c.cash, c.chosen_car), (2, 300, 1));
        assert!(ui.callback("SELLCAR", "garage", &mut c, &cars).unwrap().contains("sold"));
        assert_eq!(c.cars.len(), 1);
        assert!(ui.callback("NOPE", "garage", &mut c, &cars).is_none());
    }

    #[test]
    fn the_view_names_the_screens_data() {
        let cars = cars();
        let ui = ShopUi::default();
        let c = Career::new("T");
        let v = ui.view(&c, &cars, &[], &[], &Locale::default(), 0, "garage");
        assert_eq!(v.watch["OwnedCars.Car(PURCHASEPRICE)"], "$0");
        assert_eq!(v.watch["OwnedCars.Car(CURRENTVALUE)"], "$10,890");
        assert_eq!(v.watch["Tournament.Data(DEALERCARPRICE)"], "$11,000");
        assert_eq!(v.showcase, Some((0, 19)));
        assert!(v.hidden.contains(&"ID_REMOVEFMBUTTON".to_string()));
        assert_eq!(ui.view(&c, &cars, &[], &[], &Locale::default(), 0, "tournament").showcase, None);
    }

    #[test]
    fn a_test_drive_lists_the_open_locations_and_drives_the_chosen_car() {
        let track = |id: i16, file: &str, locked: i16| catalog::Track { id, file: file.into(), locked, ..Default::default() };
        let tracks = vec![track(8, "coastal", 0), track(11, "alps", 3), track(32, "skidpad", 0), track(14, "farmland", 0)];
        assert_eq!(test_tracks(&tracks, &[]), vec![0, 3]);
        assert_eq!(test_tracks(&tracks, &[11, 8]), vec![0, 3, 1]);
        let cars = cars();
        let mut ui = ShopUi::default();
        ui.test_track = 1;
        let c = Career::new("T");
        let drive = ui.test_drive(&c, &cars, &tracks, &[], "testdrivecarsetup").unwrap();
        assert_eq!((drive.car, drive.paint, drive.track, drive.owned), (0, 19, 14, Some(0)));
        let v = ui.view(&c, &cars, &tracks, &[], &Locale::default(), 0, "testdrivecarsetup");
        assert_eq!(v.choices["Track1.Track"].rows.len(), 2);
        ui.page = "testdrivecarsetup".into();
        let mut c2 = c.clone();
        assert!(ui.pick("Track1.Track", 0, &mut c2, &cars));
        assert_eq!(ui.test_track, 0);
    }
}
