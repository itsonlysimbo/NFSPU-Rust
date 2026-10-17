//! Loads every car the way the viewer does and reports likely visual problems:
//! triangles that land on transparent texels (holes), cabin parts poking out through the body,
//! missing wheels, lamps, driver or plate, and odd sizes.
//! Run: `cargo run -p nfspu-game --example car_audit [-- car ...]`

use nfspu_game::car_model::{self, CarModel, CarOptions, Page, Piece};
use std::collections::BTreeMap;

/// Alpha at a UV position (nearest texel, wrapping like the viewer's sampler).
fn alpha(page: &Page, uv: [f32; 2]) -> u8 {
    let s = page.size as f32;
    let x = ((uv[0].rem_euclid(1.0) * s) as u32).min(page.size - 1);
    let y = ((uv[1].rem_euclid(1.0) * s) as u32).min(page.size - 1);
    page.rgba[((y * page.size + x) * 4 + 3) as usize]
}

/// Share of a triangle's sample points (centroid and three inner points) that fall on cut-out texels.
fn hole_share(page: &Page, uv: &[[f32; 2]]) -> f32 {
    let w = [[1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0], [0.7, 0.15, 0.15], [0.15, 0.7, 0.15], [0.15, 0.15, 0.7]];
    let hits = w
        .iter()
        .filter(|k| {
            let u = k[0] * uv[0][0] + k[1] * uv[1][0] + k[2] * uv[2][0];
            let v = k[0] * uv[0][1] + k[1] * uv[1][1] + k[2] * uv[2][1];
            alpha(page, [u, v]) < 128
        })
        .count();
    hits as f32 / w.len() as f32
}

/// Per article: (triangles, triangles that are mostly holes).
fn holes(m: &CarModel, piece: &Piece, page: &Option<Page>, out: &mut BTreeMap<String, (usize, usize)>, tag: &str) {
    let Some(page) = page else { return };
    for t in 0..piece.positions.len() / 3 {
        let uv = [piece.uvs[3 * t], piece.uvs[3 * t + 1], piece.uvs[3 * t + 2]];
        let name = m.article_names.get(piece.articles[3 * t] as usize).cloned().unwrap_or_default();
        let e = out.entry(format!("{tag}:{name}")).or_default();
        e.0 += 1;
        if hole_share(page, &uv) >= 0.5 {
            e.1 += 1;
        }
    }
}

/// Cabin vertices sticking out more than 1 cm beyond the body's outermost |x| at the same height
/// and length (10 cm bins).
fn poking(body: &[&Piece], cabin: &Piece) -> usize {
    let key = |p: &[f32; 3]| ((p[1] * 10.0).floor() as i32, (p[2] * 10.0).floor() as i32);
    let mut hull: BTreeMap<(i32, i32), f32> = BTreeMap::new();
    for piece in body {
        for p in &piece.positions {
            let e = hull.entry(key(p)).or_insert(0.0);
            *e = e.max(p[0].abs());
        }
    }
    cabin
        .positions
        .iter()
        .filter(|p| {
            let (y, z) = key(p);
            // look at the neighbouring bins too, so sparse body triangles still count
            let mut best: Option<f32> = None;
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(h) = hull.get(&(y + dy, z + dz)) {
                        best = Some(best.map_or(*h, |b: f32| b.max(*h)));
                    }
                }
            }
            best.is_some_and(|h| p[0].abs() > h + 0.01)
        })
        .count()
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").expect("NFSPU_GAME_DIR")).join("GameData").join("CarModel");
    let mut cars: Vec<String> = std::env::args().skip(1).collect();
    if cars.is_empty() {
        cars = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("crp")))
            .map(|p| p.file_stem().unwrap().to_string_lossy().to_string())
            .collect();
        cars.sort();
    }
    for name in &cars {
        let m = match car_model::load_car(name, &CarOptions::default()) {
            Ok(m) => m,
            Err(e) => {
                println!("== {name}: LOAD FAILED: {e}");
                continue;
            }
        };
        let mut issues: Vec<String> = Vec::new();
        let mut per: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        holes(&m, &m.paint, &m.exterior, &mut per, "body");
        holes(&m, &m.headlights, &m.exterior, &mut per, "head");
        holes(&m, &m.taillights, &m.exterior, &mut per, "tail");
        holes(&m, &m.trim, &m.extint, &mut per, "trim");
        holes(&m, &m.glass, &m.glass_page, &mut per, "glass");
        holes(&m, &m.driver_suit, &m.suit_page, &mut per, "suit");
        holes(&m, &m.driver_head, &m.head_page, &mut per, "head");
        for w in &m.wheels {
            holes(&m, &w.piece, &m.extint, &mut per, "wheel");
        }
        for (k, (n, h)) in &per {
            // handles (recessed grips) and lamp lenses have transparent artwork around their shape
            // by design, and the paint behind shows through: not holes
            if k.ends_with(":Handle") || k.starts_with("head:Light") || k.starts_with("tail:Light") || k.starts_with("head:HeadLight") {
                continue;
            }
            // wheels have genuine transparent corners; flag only when most of an article is holes
            let limit = if k.starts_with("wheel:") { 0.6 } else { 0.25 };
            if *h as f32 > limit * *n as f32 && *h >= 2 {
                issues.push(format!("holes in {k}: {h} of {n} triangles"));
            }
        }
        let body = [&m.paint, &m.trim, &m.trim_solid, &m.glass, &m.headlights, &m.taillights];
        let out_cabin = poking(&body, &m.interior_outside);
        if out_cabin > 3 {
            issues.push(format!("outside cabin pokes through the body: {out_cabin} vertices"));
        }
        if m.wheels.len() != 4 {
            issues.push(format!("{} wheels", m.wheels.len()));
        }
        if m.headlights.positions.is_empty() {
            issues.push("no headlights".into());
        }
        if m.taillights.positions.is_empty() {
            issues.push("no tail lights".into());
        }
        if m.driver_suit.positions.is_empty() {
            issues.push("no driver".into());
        }
        let plate = per.keys().any(|k| k.contains(":License"));
        if !plate {
            issues.push("no plate".into());
        }
        for (label, p) in [("exterior", &m.exterior), ("trim", &m.extint), ("glass", &m.glass_page)] {
            if p.is_none() {
                issues.push(format!("no {label} page"));
            }
        }
        let [w, h, l] = m.size;
        if !(1.3..2.6).contains(&w) || !(0.9..3.5).contains(&h) || !(3.0..13.0).contains(&l) {
            issues.push(format!("odd size {w:.2} x {h:.2} x {l:.2}"));
        }
        println!(
            "== {name:<14} style '{}', {} tris, {:.2} x {:.2} x {:.2} m: {}",
            m.style_name,
            m.triangles(),
            w,
            h,
            l,
            if issues.is_empty() { "ok".to_string() } else { issues.join("; ") }
        );
    }
}
