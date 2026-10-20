//! Weather and lighting audit over every track: night layers, wet road, sky keyframes (fog,
//! rain, lightning, snow along the lap), the `.map` car-lighting header and the headlight light map.
//! Writes `logs/weather_audit.txt`.
//! Run: `$env:NFSPU_GAME_DIR="C:\Users\CB\Downloads\NFS5 - Copy"; cargo run -p nfspu-game --example weather_audit -j 2`

use nfspu_game::weather::{TimeOverride, TrackWeather, WeatherOverride};
use std::fmt::Write as _;
use std::path::PathBuf;

fn main() {
    let game = PathBuf::from(std::env::var("NFSPU_GAME_DIR").expect("set NFSPU_GAME_DIR"));
    let tdir = game.join("GameData").join("Track");
    let fe = game.join("FEData");
    let tracks = nfspu_game::catalog::parse_tracks(&std::fs::read(fe.join("Data").join("nfs5.trk")).unwrap_or_default());
    let names = std::fs::read(fe.join("Locale").join("festrings.loc")).ok().and_then(|d| nfspu_formats::loc::parse(&d, 0)).unwrap_or_default();
    let mut files: Vec<String> = std::fs::read_dir(&tdir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".crp").map(str::to_string)))
        .collect();
    files.sort();
    let mut out = String::new();
    let _ = writeln!(out, "weather audit: {} tracks from {}", files.len(), tdir.display());
    for f in &files {
        let w = TrackWeather::load(&game, f, WeatherOverride::Track, TimeOverride::Track);
        let shown = tracks
            .iter()
            .find(|t| t.file.eq_ignore_ascii_case(f))
            .and_then(|t| names.get(&(t.name_string as u16)).cloned())
            .unwrap_or_default();
        let _ = writeln!(out, "\n== {f} ({shown})");
        let _ = writeln!(out, "  {}", w.describe());
        let l = &w.lighting;
        let _ = writeln!(
            out,
            "  map: tint {:.3} ambient {:.3} interior {:.3} interiordiffuse {:.3} lightdecal {:.3}, {} reflection sections",
            l.tint[1], l.ambient[1], l.interior[1], l.interior_diffuse[1], l.light_decal[1], l.sections.len()
        );
        // The slice count for wrapping, from the road path.
        let slices = std::fs::read(tdir.join(format!("{f}.crp")))
            .ok()
            .and_then(|raw| nfspu_formats::refpack::decompress(&raw).ok())
            .and_then(|d| nfspu_formats::crp::parse(&d).ok().and_then(|c| nfspu_formats::sim::read(&c).ok().map(|s| s.slices.len())))
            .unwrap_or(0);
        let Some(sky) = &w.sky else {
            let _ = writeln!(out, "  sky .bin: none (retail defaults: no fog, no precipitation)");
            continue;
        };
        let _ = writeln!(out, "  sky .bin: {} keyframes, {} region-blend entries, {} flags ({} set), header angle {:.4}; {} road slices",
            sky.keys.len(), sky.region_blend.len(), sky.flags.len(), sky.flags.iter().filter(|f| f.1).count(), sky.header_a[0], slices);
        // Sample the lap at 8 points.
        for k in 0..8 {
            let s = slices * k / 8;
            let c = w.at(s, slices.max(1));
            let rgb = |c: [f32; 3]| format!("{:.0},{:.0},{:.0}", c[0] * 255.0, c[1] * 255.0, c[2] * 255.0);
            let _ = writeln!(
                out,
                "    slice {s:>5}: world fog {:.5}/m ({}) half-visible at {:>5.0} m; sky fog {:.5}/m ({}); rain {:.2} lightning {:.2} snow {:.2}; region blend {:.2}",
                c.world_fog_density,
                rgb(c.world_fog_colour),
                if c.world_fog_density > 0.0 { std::f32::consts::LN_2 / c.world_fog_density } else { f32::INFINITY },
                c.sky_fog_density,
                rgb(c.sky_fog_colour),
                c.rain,
                c.lightning,
                c.snow,
                sky.region_blend_at(s as i32, slices as i32),
            );
        }
    }
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("logs").join("weather_audit.txt");
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    std::fs::write(&path, &out).unwrap();
    print!("{out}");
    println!("\nwritten to {}", path.display());
}
