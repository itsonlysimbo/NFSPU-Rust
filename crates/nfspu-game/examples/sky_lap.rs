//! Walks a track's lap and prints the horizon tint the car would see at each point, using the
//! centreline's own direction of travel, so the warm/cold change can be checked without opening
//! the game. Run: `cargo run -p nfspu-game --example sky_lap -- alps`
//!
//! Repaired after the atlas work: the colour table's position field is no longer read as a lap
//! distance (that reading was disproven - see `sky::horizon_tint`), so the tint is now sampled by
//! **heading** rather than by distance along the lap. The heading comes from the centreline
//! tangent; the world is mirrored on import, so the sign convention below is a convention, not a
//! verified direction, and the tint itself remains an unconfirmed approximation.
use nfspu_game::{sky, track};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let name = a.get(1).cloned().unwrap_or_else(|| "alps".to_string());
    let Ok(dir) = std::env::var("NFSPU_GAME_DIR") else {
        eprintln!("set NFSPU_GAME_DIR");
        return;
    };

    let ground = track::load_ground(&name).expect("ground");
    let line: Vec<[f32; 3]> = ground.centerline.iter().map(|v| [v.x, v.y, v.z]).collect();
    if line.len() < 3 {
        println!("no centreline to measure along");
        return;
    }
    let mut distances = Vec::with_capacity(line.len());
    let mut total = 0.0f32;
    for i in 0..line.len() {
        if i > 0 {
            let (p, q) = (line[i], line[i - 1]);
            total += ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt();
        }
        distances.push(total);
    }
    println!("{name}: {} centreline points, lap {:.0} m", line.len(), total);

    let Some(s) = sky::load(std::path::Path::new(&dir), &name) else {
        println!("no sky files");
        return;
    };
    println!(
        "{} horizon colours, start angle {:.4} (a heading of {:.0} degrees)",
        s.horizon.len(),
        s.start_angle,
        s.start_angle * 360.0
    );

    // every 5% of the lap, print where we are, which way the car points there, and the tint
    let mut prev: Option<[f32; 3]> = None;
    let mut changes = 0;
    for step in 0..=20 {
        let f = step as f32 / 20.0;
        let i = ((line.len() - 1) as f32 * f).round() as usize;
        let heading = heading_turns(&line, i);
        let t = sky::horizon_tint(&s.horizon, heading - s.start_angle);
        let marker = match prev {
            Some(q) if (t[0] - q[0]).abs() + (t[1] - q[1]).abs() + (t[2] - q[2]).abs() > 0.01 => {
                changes += 1;
                " <- changed"
            }
            _ => "",
        };
        println!(
            "  {:>3.0}%  {:>6.0} m  heading {:>5.0} deg  rgb {:>5.0},{:>5.0},{:>5.0}  {}{}",
            f * 100.0,
            distances[i],
            heading * 360.0,
            t[0] * 255.0,
            t[1] * 255.0,
            t[2] * 255.0,
            if t[0] > t[2] + 0.03 {
                "warm"
            } else if t[2] > t[0] + 0.03 {
                "cold"
            } else {
                "neutral"
            },
            marker
        );
        prev = Some(t);
    }
    println!("{changes} colour changes over the lap");
}

/// The centreline's direction of travel at point `i`, as a fraction of a turn. Sign convention is
/// unverified (the world is mirrored on import), so treat the numbers as relative, not compass.
fn heading_turns(line: &[[f32; 3]], i: usize) -> f32 {
    let prev = line[i.saturating_sub(1)];
    let next = line[(i + 1).min(line.len() - 1)];
    let (dx, dz) = (next[0] - prev[0], next[2] - prev[2]);
    if dx == 0.0 && dz == 0.0 {
        return 0.0;
    }
    let turns = dx.atan2(dz) / std::f32::consts::TAU;
    turns - turns.floor()
}
