//! Prints the reference-build model (view sizes, field of view, renderer gates, CPU class) and
//! checks it against the author's copy when one is present. Also scans the install's texture
//! archives for the palette chunk the reference build looks for. No window, no game run.
//!
//! Run:
//! `$env:NFSPU_GAME_DIR="C:\Users\CB\Downloads\NFS5"; cargo run -p nfspu-game --example refbuild_audit -j 2`
//!
//! Writes `logs/refbuild_audit.txt`.

use nfspu_game::refbuild::{
    cpu_detail_class, DriverProfile, Language, RendererBackend, RendererStateGate, Settings, VirtualScreen,
    NARROW_FOV_DEGREES, WIDESCREEN_ASPECT,
};
use nfspu_formats::{refpack, shpi};
use std::{env, fs, io::Write, path::PathBuf};

fn main() {
    let mut out = String::new();
    let mut say = |line: String| {
        println!("{line}");
        out.push_str(&line);
        out.push('\n');
    };

    say("Reference build model (Enhanced 3.5 + Combined Patch v1.0.6)".into());
    say(format!("  widescreen threshold : aspect > {WIDESCREEN_ASPECT:.6} (4:3 stays narrow)"));
    say(format!("  narrow field of view : {NARROW_FOV_DEGREES} degrees"));

    say(String::new());
    say("View model per client size".into());
    say(format!("  {:>10}  {:>10}  {:>8}  {:>12}  {:>8}", "client", "virtual", "aspect", "fov(deg)", "wide"));
    for (w, h) in [
        (640u32, 480u32),
        (800, 600),
        (1280, 960),
        (1280, 720),
        (1600, 900),
        (1920, 1080),
        (1920, 1200),
        (2560, 1080),
        (3440, 1440),
        (3840, 2160),
    ] {
        let v = VirtualScreen::for_client(w, h);
        say(format!(
            "  {:>5}x{:<4}  {:>5.0}x{:<4}  {:>8.4}  {:>12.4}  {:>8}",
            w,
            h,
            v.width,
            v.height,
            v.aspect(),
            v.fov_degrees,
            if v.widescreen { "yes" } else { "no" }
        ));
    }

    say(String::new());
    say("Renderer state gate (fog / filtering options off)".into());
    let off = RendererStateGate { fog: false, filtering: false };
    for (label, states) in [
        ("filtering off -> value 0", RendererStateGate::FILTER_STATES.to_vec()),
        ("fog off -> call skipped", RendererStateGate::FOG_SKIPPED_STATES.to_vec()),
        ("fog off -> value 0", RendererStateGate::FOG_ZEROED_STATES.to_vec()),
    ] {
        let list: Vec<String> = states.iter().map(|s| format!("{s:#04x}")).collect();
        say(format!("  {label:<24} {}", list.join(" ")));
    }
    say(format!("  a state outside those sets passes through unchanged (sample: {:?})", off.gate(0x20, 77)));

    say(String::new());
    say("CPU detail class (measured, secondary) -> class".into());
    for (m, s) in [(3u8, 16u32), (3, 32), (2, 16), (1, 0)] {
        say(format!("  ({m}, {s}) -> {}", cpu_detail_class(m, s)));
    }

    let languages: Vec<String> = Language::ALL.iter().map(|l| format!("{}={}", l.id(), l.config_name())).collect();
    say(format!("Languages: {}", languages.join(" ")));
    let backends: Vec<&str> = [RendererBackend::Direct3d, RendererBackend::Glide, RendererBackend::OpenGl, RendererBackend::Software]
        .iter()
        .map(|b| b.config_token())
        .collect();
    say(format!("Renderer types: {}", backends.join(" ")));

    // ---- optional: the author's own files, if a folder is pointed at ----
    if let Ok(dir) = env::var("NFSPU_GAME_DIR") {
        let dir = PathBuf::from(dir);
        let ini = dir.join("nfs5.ini");
        if ini.is_file() {
            let text = fs::read_to_string(&ini).unwrap_or_default();
            let s = Settings::parse(&text);
            say(String::new());
            say(format!("Author settings read from {}", ini.display()));
            say(format!(
                "  language={} driver={} windowed={} affinity={} cpu_limit={} movies={} fog={} filtering={}",
                s.language.config_name(),
                s.driver,
                s.windowed,
                s.single_processor_affinity,
                s.cpu_mhz_limit,
                s.movies,
                s.fog,
                s.filtering
            ));
            let profile = dir.join("drivers").join(&s.driver).join("thrash.ini");
            if profile.is_file() {
                let p = DriverProfile::parse(&fs::read_to_string(&profile).unwrap_or_default());
                match p {
                    Some(p) => say(format!(
                        "  driver profile: {} ({}) with {} pass-through options",
                        p.module,
                        p.backend.label(),
                        p.options.len()
                    )),
                    None => say(format!("  driver profile {} did not parse", profile.display())),
                }
            } else {
                say(format!("  no driver profile at {}", profile.display()));
            }
        } else {
            say(String::new());
            say(format!("No nfs5.ini in {} (that folder may be the unpatched build).", dir.display()));
        }

        scan_palette_chunk(&dir, &mut say);
    } else {
        say(String::new());
        say("NFSPU_GAME_DIR not set, so the author's files were not read.".into());
    }

    if let Err(e) = fs::create_dir_all("logs").and_then(|_| fs::File::create("logs/refbuild_audit.txt").map(|_| ())) {
        eprintln!("could not prepare logs/: {e}");
    }
    match fs::File::create("logs/refbuild_audit.txt").and_then(|mut f| f.write_all(out.as_bytes())) {
        Ok(()) => println!("\nwrote logs/refbuild_audit.txt"),
        Err(e) => eprintln!("could not write logs/refbuild_audit.txt: {e}"),
    }
}

/// Counts texture archives containing the palette chunk name the reference build matches on.
/// Archives are decompressed first when they are RefPack, so this covers compressed files too.
fn scan_palette_chunk(dir: &PathBuf, say: &mut impl FnMut(String)) {
    const NEEDLE: &[u8] = b"!pal";
    let root = dir.join("GameData");
    if !root.is_dir() {
        return;
    }
    let (mut files, mut plain, mut compressed, mut with_chunk, mut bytes) = (0usize, 0usize, 0usize, 0usize, 0u64);
    let mut stack = vec![root];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
            if ext != "fsh" && ext != "env" {
                continue;
            }
            let Ok(raw) = fs::read(&p) else { continue };
            files += 1;
            bytes += raw.len() as u64;
            let decoded = if refpack::is_refpack(&raw) {
                compressed += 1;
                match refpack::decompress(&raw) {
                    Ok(d) => d,
                    Err(_) => continue,
                }
            } else {
                plain += 1;
                raw
            };
            // Only report a hit inside a real archive, so stray bytes elsewhere do not count.
            if decoded.windows(4).any(|w| w == NEEDLE) && shpi::parse(&decoded).is_ok() {
                with_chunk += 1;
            }
        }
    }
    say(String::new());
    say("Palette chunk scan (GameData, .fsh/.env)".into());
    say(format!(
        "  {files} archives ({plain} plain, {compressed} RefPack), {:.1} MiB on disk",
        bytes as f64 / (1024.0 * 1024.0)
    ));
    say(format!("  archives containing the '!pal' chunk name: {with_chunk}"));
    say("  (the reference build's palette-rewrite path matches this name; a zero count means it is dormant for shipped art)".into());
}
