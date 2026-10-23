//! Renders the player's engine mix offline at fixed engine speeds to `extracted/footage/ours_<rpm>.wav`
//! (mono, 48 kHz, 4 s each) and prints each one's layer volumes, so our levels and pitch can be
//! compared with a retail recording. Run with `NFSPU_GAME_DIR` set; `CAR=gt1` picks the car by
//! short name (default the first match), `NEAR=1` renders the first-person mix.

use nfspu_game::race_sound as rs;
use std::path::PathBuf;

fn main() {
    let dir = PathBuf::from(std::env::var("NFSPU_GAME_DIR").expect("set NFSPU_GAME_DIR"));
    let want = std::env::var("CAR").unwrap_or_else(|_| "gt1".into()).to_lowercase();
    let near = std::env::var("NEAR").is_ok();
    let cars = nfspu_game::car_load::catalogue(&dir);
    let car = cars.iter().find(|c| c.name.to_lowercase().contains(&want) || c.short.to_lowercase().contains(&want)).expect("car");
    let eng = rs::load_engine(&dir, &car.sound, &car.sim).expect("engine");
    println!("{} sound {} sim {} layers {} tuning {:?}", car.name, eng.name, car.sim, eng.layers.len(), eng.tuning);
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../extracted/footage");
    std::fs::create_dir_all(&out).unwrap();
    const SR: f32 = 48000.0;
    for frac in [0.19f32, 0.3, 0.45, 0.6, 0.8, 1.0] {
        let mut mix = rs::EngineMix::new(&eng);
        let input = rs::EngineInput { rpm_frac: frac, throttle: 1.0, shifting: false, near, distance: if near { 0.0 } else { 6.0 }, speed: 20.0 + 60.0 * frac, side: 0.0 };
        for _ in 0..200 {
            mix.tick(input);
        }
        let mut pos = vec![0.0f64; eng.layers.len()];
        let mut samples = Vec::new();
        for _ in 0..(4.0 * SR) as usize {
            let mut acc = 0.0f32;
            for (i, l) in eng.layers.iter().enumerate() {
                let g = mix.gain(i);
                let ch = l.pcm.channels as usize;
                let n = l.pcm.samples.len() / ch;
                let p = pos[i];
                let (a, b, f) = (p as usize % n, (p as usize + 1) % n, (p - p.floor()) as f32);
                let s = |k: usize| (0..ch).map(|c| l.pcm.samples[k * ch + c] as f32).sum::<f32>() / ch as f32 / 32768.0;
                acc += g * 0.7 * (s(a) * (1.0 - f) + s(b) * f);
                pos[i] = (p + (l.pcm.rate as f64 / SR as f64) * mix.speed(i) as f64) % n as f64;
            }
            samples.push((acc.clamp(-1.0, 1.0) * 32767.0) as i16);
        }
        let rms = (samples.iter().map(|&s| (s as f64 / 32768.0).powi(2)).sum::<f64>() / samples.len() as f64).sqrt();
        let active: Vec<String> = (0..mix.layers()).filter(|&i| mix.gain(i) > 0.01).map(|i| format!("{}:{:.2}@x{:.2}", eng.layers[i].sound, mix.gain(i), mix.speed(i))).collect();
        println!("frac {frac:.2} pitch {:.3} rms {:.1} dBFS layers {}", mix.pitch(), 20.0 * rms.log10(), active.join(" "));
        let wav = nfspu_formats::bnk::wav(&samples, 1, SR as u32);
        std::fs::write(out.join(format!("ours_{:03}.wav", (frac * 100.0) as u32)), wav).unwrap();
    }
}
