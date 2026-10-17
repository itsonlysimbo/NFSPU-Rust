//! Checks whether a track's centreline is in driving order: prints the first few points and how
//! far each one is from the next, plus what `sky::LapPath` makes of a few known positions.
//! Run: `cargo run -p nfspu-game --example lap_order -- alps`
use nfspu_game::track;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let name = a.get(1).cloned().unwrap_or_else(|| "alps".to_string());
    let ground = track::load_ground(&name).expect("ground");
    let c = &ground.centerline;
    println!("{}: {} points", name, c.len());
    println!("first 12 points and the stretch to the next:");
    for i in 0..12.min(c.len().saturating_sub(1)) {
        let d = c[i].distance(c[i + 1]);
        println!("   {i:>4} {:?} -> next {d:>8.1} m", c[i]);
    }
    // A driving-order centreline has short, even stretches. A name-ordered one has big jumps.
    let mut gaps: Vec<f32> = (0..c.len() - 1).map(|i| c[i].distance(c[i + 1])).collect();
    gaps.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mid = gaps.len() / 2;
    println!(
        "stretches: shortest {:.1}, median {:.1}, longest {:.1} m",
        gaps[0],
        gaps[mid],
        gaps[gaps.len() - 1]
    );
    let big = gaps.iter().filter(|g| **g > 200.0).count();
    println!("stretches over 200 m: {big} of {}", gaps.len());
    println!("start position {:?} yaw {:.3}", ground.start_pos, ground.start_yaw);
    println!("start line is nearest centreline point {}", {
        let mut best = (f32::MAX, 0usize);
        for (i, p) in c.iter().enumerate() {
            let d = p.distance(ground.start_pos);
            if d < best.0 {
                best = (d, i);
            }
        }
        format!("{} ({} m away)", best.1, best.0)
    });
}