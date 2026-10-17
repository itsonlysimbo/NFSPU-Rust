//! Offline check (no window): load every car in the install and report its shape.
//! A normal car is about 3.5-4.8 m long, 1.6-2.0 m wide, 1.0-1.5 m tall, with 4 wheels.
//! Run: `cargo run -p nfspu-game --example car_check`   (optionally a car name as argument)

use nfspu_game::car_model;

fn main() {
    let only = std::env::args().nth(1);
    let dir = car_model::car_dir().expect("set NFSPU_GAME_DIR");
    let mut names: Vec<String> = std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok())
        .filter_map(|e| { let p = e.path(); (p.extension()? == "crp").then(|| p.file_stem().unwrap().to_string_lossy().to_string()) }).collect();
    names.sort();
    println!("{:<18} {:>8} {:>7} {:>6}  {:>22}  {:<14} note", "car", "articles", "tris", "wheels", "width x height x length", "ext/trim/glass");
    for n in names {
        if let Some(o) = &only { if &n != o { continue; } }
        match car_model::load_car(&n, &car_model::CarOptions::default()) {
            Err(e) => println!("{n:<18} ERROR {e}"),
            Ok(m) => {
                let [w, h, l] = m.size;
                let ok = (1.4..2.2).contains(&w) && (0.9..1.8).contains(&h) && (3.2..5.2).contains(&l) && m.wheels.len() == 4;
                let pages = format!("{}/{}/{}", if m.exterior.is_some() { "yes" } else { "NO" }, if m.extint.is_some() { "yes" } else { "NO" }, if m.glass_page.is_some() { "yes" } else { "NO" });
                println!("{:<18} {:>8} {:>7} {:>6}  {:>6.2} x {:>5.2} x {:>6.2}  {:<14} {}", n, m.articles_used, m.triangles(), m.wheels.len(), w, h, l, pages, if ok { "" } else { "<- unusual" });
            }
        }
    }
}
