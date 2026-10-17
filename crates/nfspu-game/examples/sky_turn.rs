//! Sweeps the camera's heading through a full turn and prints the sky colour it would give, so the
//! warm/cold change can be checked without opening the game.
//! Run: `cargo run -p nfspu-game --example sky_turn -- alps`
use nfspu_game::sky;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let name = a.get(1).cloned().unwrap_or_else(|| "alps".to_string());
    let Ok(dir) = std::env::var("NFSPU_GAME_DIR") else {
        eprintln!("set NFSPU_GAME_DIR");
        return;
    };
    let Some(s) = sky::load(std::path::Path::new(&dir), &name) else {
        println!("no sky files for {name}");
        return;
    };
    println!(
        "{name}: {} horizon colours, start angle {:.4} (a heading of {:.0} degrees)",
        s.horizon.len(),
        s.start_angle,
        s.start_angle * 360.0
    );
    let angles: Vec<f32> = s.horizon.iter().map(|h| h.angle).collect();
    println!(
        "colour directions: {:.3} .. {:.3} (a turn is 1.0)",
        angles.iter().cloned().fold(f32::MAX, f32::min),
        angles.iter().cloned().fold(f32::MIN, f32::max)
    );

    let mut prev: Option<[f32; 3]> = None;
    let mut changes = 0;
    for step in 0..24 {
        let deg = step as f32 * 15.0;
        let t = sky::horizon_tint(&s.horizon, deg / 360.0 - s.start_angle);
        let marker = match prev {
            Some(q) if (t[0] - q[0]).abs() + (t[1] - q[1]).abs() + (t[2] - q[2]).abs() > 0.01 => {
                changes += 1;
                " <- changed"
            }
            _ => "",
        };
        println!(
            "  heading {deg:>5.0} deg   rgb {:>5.0},{:>5.0},{:>5.0}  {}{}",
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
    println!("{changes} colour changes over a full turn");
}