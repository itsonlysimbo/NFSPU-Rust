//! For each article of a loaded car that matches a name prefix: which piece it ended up in, its
//! triangle count, centre and mean normal (viewer space: car faces -Z, origin on the ground).
//! Run: `cargo run -p nfspu-game --example car_part_info -- 993 Decal Dash Needle`
use nfspu_game::car_model::{self, CarOptions, Piece};
use std::collections::BTreeMap;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let name = a.get(1).cloned().unwrap_or_else(|| "993".to_string());
    let m = car_model::load_car(&name, &CarOptions::default()).expect("load car");
    println!("eye {:?}", m.driver_eye);
    let pieces: [(&str, &Piece); 8] = [
        ("paint", &m.paint),
        ("paint_inner", &m.paint_inner),
        ("trim", &m.trim),
        ("trim_solid", &m.trim_solid),
        ("glass", &m.glass),
        ("cabin", &m.interior),
        ("cabin_outside", &m.interior_outside),
        ("headlights", &m.headlights),
    ];
    for (label, piece) in pieces {
        // article -> (corners, sum of positions, sum of normals)
        let mut rows: BTreeMap<String, (usize, [f32; 3], [f32; 3])> = BTreeMap::new();
        for k in 0..piece.positions.len() {
            let art = &m.article_names[piece.articles[k] as usize];
            if a.len() > 2 && !a[2..].iter().any(|w| art.starts_with(w.as_str())) {
                continue;
            }
            let e = rows.entry(art.clone()).or_insert((0, [0.0; 3], [0.0; 3]));
            e.0 += 1;
            for i in 0..3 {
                e.1[i] += piece.positions[k][i];
                e.2[i] += piece.normals[k][i];
            }
        }
        // NFSPU_PART_TRIS=1: also list each triangle of the matching articles
        if std::env::var_os("NFSPU_PART_TRIS").is_some() {
            for t in 0..piece.positions.len() / 3 {
                let art = &m.article_names[piece.articles[3 * t] as usize];
                if a.len() > 2 && !a[2..].iter().any(|w| art.starts_with(w.as_str())) {
                    continue;
                }
                let c: Vec<f32> = (0..3).map(|i| (0..3).map(|q| piece.positions[3 * t + q][i]).sum::<f32>() / 3.0).collect();
                let n = piece.normals[3 * t];
                println!("  {label} {art} tri {t}: centre ({:.2},{:.2},{:.2}) normal ({:.2},{:.2},{:.2}) uv {:?}", c[0], c[1], c[2], n[0], n[1], n[2], &piece.uvs[3 * t..3 * t + 3]);
            }
        }
        for (art, (n, p, nr)) in rows {
            let f = n as f32;
            println!("{label:<13} {art:<20} {:>4} tris  centre ({:>6.2},{:>6.2},{:>6.2})  normal ({:>5.2},{:>5.2},{:>5.2})", n / 3, p[0] / f, p[1] / f, p[2] / f, nr[0] / f, nr[1] / f, nr[2] / f);
        }
    }
}
