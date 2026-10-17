//! Finds track geometry that sits well above the ground under it, which is what a floating object
//! looks like. Scans triangles in a box around a point and compares each one's height with the
//! ground height from the track's own ground data.
//! Run: `cargo run -p nfspu-game --example floating -- alps -100 -200 200 200`
use nfspu_game::ground::Ground;
use nfspu_game::track;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let name = a.get(1).cloned().unwrap_or_else(|| "alps".to_string());
    let f = |i: usize, d: f32| a.get(i).and_then(|s| s.parse().ok()).unwrap_or(d);
    let (cx, cz) = (f(2, -100.0), f(3, -200.0));
    let (rx, rz) = (f(4, 200.0), f(5, 200.0));

    let g = track::load_ground(&name).expect("ground");
    let height = Ground::new(g.tris.clone(), 16.0);

    // group triangles by how far they float above the ground, and report the worst
    let mut worst: Vec<(f32, [f32; 3], usize)> = Vec::new();
    for (i, t) in g.tris.iter().enumerate() {
        let c = [(t[0][0] + t[1][0] + t[2][0]) / 3.0, (t[0][1] + t[1][1] + t[2][1]) / 3.0, (t[0][2] + t[1][2] + t[2][2]) / 3.0];
        if (c[0] - cx).abs() > rx || (c[2] - cz).abs() > rz {
            continue;
        }
        // how tall is this triangle's own face? a flat panel floats as one piece
        let dy = (t[0][1] - t[1][1]).abs().max((t[1][1] - t[2][1]).abs()).max((t[0][1] - t[2][1]).abs());
        if dy < 0.05 {
            continue; // flat ground, not an object
        }
        let below = height.height_at(c[0], c[2], c[1], 2.0);
        let Some(below) = below else { continue };
        let gap = c[1] - below;
        if gap > 2.0 {
            worst.push((gap, c, i));
        }
    }
    worst.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    println!("{name}: {} ground triangles, box around ({cx},{cz}) +-{rx}x{rz}", g.tris.len());
    println!("{} pieces of geometry floating more than 2 m above the ground:", worst.len());
    for (gap, c, i) in worst.iter().take(25) {
        println!("   tri {i:<7} at ({:>8.1},{:>7.1},{:>8.1})  {gap:>6.2} m up", c[0], c[1], c[2]);
    }
}