//! A headless pass of the Evolution career from 1950 to 2000: for every open tournament it picks
//! a car the tournament's rule accepts (buying the cheapest one when the garage has none, selling
//! spare cars when cash is short), pays the entry fee, "races" every race at a fixed finishing
//! place (`RUN_PLACE`, default 1) through `Career::finish_race`, and goes on until the last era is
//! finished or a step is blocked. Every step and every blocked step goes to
//! `logs/career_runthrough.txt` and stdout; the last line is `RUNTHROUGH OK` or `RUNTHROUGH BLOCKED`.
//!
//! The races themselves are not driven here (`race_drive` drives every track); this checks the
//! career rules end to end: money, car rules, fees, era changes, bonus races, prize cars.
//! Run: `cargo run -p nfspu-game --example career_runthrough` (needs NFSPU_GAME_DIR).

use nfspu_formats::fe::Locale;
use nfspu_game::career::{self, Career, Shop};
use nfspu_game::{career_shop, catalog};
use std::fmt::Write as _;
use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(std::env::var("NFSPU_GAME_DIR").expect("set NFSPU_GAME_DIR"));
    let place: usize = std::env::var("RUN_PLACE").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    let locale = std::fs::read(root.join("FEData").join("Locale").join("festrings.csv")).map(|b| Locale::parse_csv_bytes(&b)).unwrap_or_default();
    let trn = std::fs::read(root.join("FEData").join("Data").join("nfs5.trn")).expect("nfs5.trn");
    let cars: Vec<catalog::Car> = catalog::parse_cars(&std::fs::read(root.join("GameData").join("nfs5.car")).or_else(|_| std::fs::read(root.join("FEData").join("Data").join("nfs5.car"))).expect("nfs5.car")).into_iter().filter(|c| c.kind == 0).collect();
    let parts = std::fs::read(root.join("FEData").join("Data").join("nfs5.prt")).map(|d| career_shop::parse_parts(&d)).unwrap_or_default();
    let tracks = std::fs::read(root.join("FEData").join("Data").join("nfs5.trk")).map(|d| catalog::parse_tracks(&d)).unwrap_or_default();
    let mut all = career::parse_tournaments(&trn);
    career::assign_routes(&mut all, &tracks);
    let shop = Shop { cars: &cars, parts: &parts };
    let text = |id: u32| locale.text(id, 0).unwrap_or_default().trim().to_string();

    let mut out = String::new();
    let mut c = Career::new("runthrough");
    let mut blocked = false;
    let mut races = 0;
    let mut replays = 0;
    let _ = writeln!(out, "career run-through, every race finished {}{}, {} tournaments, {} cars in the catalogue", place, if place == 1 { "st" } else { "th" }, all.len(), cars.len());
    'career: for round in 0..400 {
        if c.year >= career::END_YEAR {
            break;
        }
        let open: Vec<_> = c.open_tournaments(&all).into_iter().cloned().collect();
        // Prefer an incomplete tournament the garage already qualifies for; else the incomplete one
        // that is cheapest to get into (car + fee); when none is affordable, replay a completed
        // tournament the garage qualifies for to earn the money (retail allows repeats).
        let qualifies = |t: &career::Tournament, c: &Career| {
            let rule = text(t.restriction);
            c.cars.iter().position(|o| cars.iter().find(|k| k.id == o.id).is_some_and(|k| t.allows(&rule, k)))
        };
        let cost = |t: &career::Tournament, c: &Career| -> i64 {
            let rule = text(t.restriction);
            if qualifies(t, c).is_some() {
                return t.entry_fee as i64;
            }
            cars.iter().filter(|k| k.price > 0 && t.allows(&rule, k)).map(|k| k.price as i64).min().unwrap_or(i64::MAX / 4) + t.entry_fee as i64
        };
        // cash plus what spare cars would sell for (the last car cannot be sold)
        let wealth = |c: &Career| {
            let values: Vec<i64> = (0..c.cars.len()).map(|i| c.value_of(i, &shop)).collect();
            c.cash + values.iter().sum::<i64>() - values.iter().copied().min().unwrap_or(0)
        };
        let todo: Vec<&career::Tournament> = open.iter().filter(|t| c.progress(t.id).completed == 0).collect();
        if todo.is_empty() {
            let _ = writeln!(out, "BLOCKED year {}: every open tournament is complete but the era did not end", c.year);
            blocked = true;
            break;
        }
        let affordable = todo.iter().filter(|t| cost(t, &c) <= wealth(&c)).min_by_key(|t| cost(t, &c)).copied();
        let t = match affordable {
            Some(t) => t,
            None => {
                let Some(r) = open.iter().filter(|t| qualifies(t, &c).is_some() && !t.bonus).max_by_key(|t| t.prizes[0]) else {
                    let cheapest = todo.iter().min_by_key(|t| cost(t, &c)).unwrap();
                    let _ = writeln!(out, "BLOCKED year {}: cannot afford '{}' (needs {}, wealth {}) and no tournament to replay", c.year, text(cheapest.name), career::money(cost(cheapest, &c)), career::money(wealth(&c)));
                    blocked = true;
                    break;
                };
                replays += 1;
                if replays > 300 {
                    let _ = writeln!(out, "BLOCKED year {}: 300 replays did not earn enough", c.year);
                    blocked = true;
                    break;
                }
                r
            }
        };
        let t = &t.clone();
        let name = text(t.name);
        let rule = text(t.restriction);
        let tracks_of = Career::race_tracks(t, &all);
        if tracks_of.is_empty() {
            let _ = writeln!(out, "BLOCKED year {}: '{name}' (id {}) has no tracks", c.year, t.id);
            blocked = true;
            break;
        }
        // a car that meets the rule
        let owned = c.cars.iter().position(|o| cars.iter().find(|k| k.id == o.id).is_some_and(|k| t.allows(&rule, k)));
        let fee = t.entry_fee as i64;
        let at = match owned {
            Some(i) => i,
            None => {
                // the cheapest new car that meets the rule; sell spare cars while short
                let mut options: Vec<&catalog::Car> = cars.iter().filter(|k| k.price > 0 && t.allows(&rule, k)).collect();
                options.sort_by_key(|k| k.price);
                let Some(want) = options.first().copied() else {
                    let _ = writeln!(out, "BLOCKED year {}: '{name}' rule \"{rule}\" lets no purchasable car in", c.year);
                    blocked = true;
                    break 'career;
                };
                while c.cash < want.price as i64 + fee && c.cars.len() > 1 {
                    let best = (0..c.cars.len()).max_by_key(|i| c.value_of(*i, &shop)).unwrap();
                    let sold = c.sell(best, &shop).unwrap_or(0);
                    let _ = writeln!(out, "  sold a car for {} to afford '{}'", career::money(sold), want.name);
                }
                if c.cash < want.price as i64 + fee {
                    let _ = writeln!(out, "BLOCKED year {}: '{name}' needs {} ('{}' {} + fee {}), cash {}", c.year, career::money(want.price as i64 + fee), want.name, career::money(want.price as i64), career::money(fee), career::money(c.cash));
                    blocked = true;
                    break 'career;
                }
                let paid = c.buy_new(want.id, 0, &shop).expect("bought");
                let _ = writeln!(out, "  bought '{}' for {}", want.name, career::money(paid));
                c.cars.len() - 1
            }
        };
        let keep = c.cars[at].id;
        // short of the fee: sell the other cars, best first
        while c.cash < fee && c.cars.len() > 1 {
            let spare = (0..c.cars.len()).filter(|i| c.cars[*i].id != keep).max_by_key(|i| c.value_of(*i, &shop)).unwrap();
            let sold = c.sell(spare, &shop).unwrap_or(0);
            let _ = writeln!(out, "  sold a spare car for {} to pay the entry fee", career::money(sold));
        }
        let at = c.cars.iter().position(|o| o.id == keep).unwrap_or(0);
        c.chosen_car = at;
        if c.cash < fee {
            let _ = writeln!(out, "BLOCKED year {}: '{name}' entry fee {} but cash {}", c.year, career::money(fee), career::money(c.cash));
            blocked = true;
            break;
        }
        c.cash -= fee;
        let _ = writeln!(out, "year {} era {}: '{name}' (id {}{}) with '{}', fee {}, {} races", c.year, t.era, t.id, if t.bonus { ", bonus" } else { "" }, cars.iter().find(|k| k.id == c.cars[at].id).map_or("?", |k| k.name.as_str()), career::money(fee), tracks_of.len());
        loop {
            let pay = c.finish_race(t, &all, place);
            races += 1;
            let _ = writeln!(out, "    race {} on track {}: place {place} paid {}, cash {}{}{}{}", c.progress(t.id).races_done.max(if pay.tournament_done { tracks_of.len() as u8 } else { 0 }), tracks_of[(c.progress(t.id).races_done as usize + tracks_of.len() - 1) % tracks_of.len()], pay.amount, career::money(c.cash), if pay.prize_car.is_some() { " PRIZE CAR" } else { "" }, if pay.bonus_open { " (bonus open)" } else { "" }, pay.new_year.map_or(String::new(), |y| format!(" NEW YEAR {y}")));
            if pay.tournament_done {
                break;
            }
        }
        if round == 399 {
            let _ = writeln!(out, "BLOCKED: too many rounds");
            blocked = true;
        }
    }
    let _ = writeln!(out, "end: year {}, cash {}, {} cars, {races} races, {replays} replays", c.year, career::money(c.cash), c.cars.len());
    if c.year < career::END_YEAR {
        blocked = true;
    }
    let _ = writeln!(out, "{}", if blocked { "RUNTHROUGH BLOCKED" } else { "RUNTHROUGH OK" });
    print!("{out}");
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("logs");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join("career_runthrough.txt"), out);
}
