//! Prints the parts list the garage's parts shop would show for a car (default the '50 356 1100
//! Cabriolet) with raw price, today's buy price and the value back, for comparing against footage.
//! Run: `cargo run -p nfspu-game --example shop_check --release [car id]` (needs NFSPU_GAME_DIR).
use nfspu_game::{career_shop, catalog};
use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(std::env::var("NFSPU_GAME_DIR").expect("set NFSPU_GAME_DIR"));
    let data = root.join("FEData").join("Data");
    let cars = catalog::parse_cars(&std::fs::read(data.join("nfs5.car")).expect("nfs5.car"));
    let parts = career_shop::parse_parts(&std::fs::read(data.join("nfs5.prt")).expect("nfs5.prt"));
    let want = std::env::args().nth(1).and_then(|a| a.parse::<u16>().ok());
    let car = cars.iter().find(|c| want.map_or(c.name.contains("356 1100 Cabriolet"), |w| c.id == w)).expect("car");
    let year = 1956;
    println!("{} (id {}, price {}) parts in {year}:", car.name, car.id, car.price);
    for p in career_shop::parts_for(&parts, car, year) {
        println!("  kind {:2} {:12} id {:4} year {} price {:6} -> value {:6}", p.kind, p.category, p.id, p.year, p.price, career_shop::part_value(p.price, year - p.year));
    }
}
