//! Offline comparison against raw headers and independently evaluated reference equations.
use nfspu_game::{sky::{HorizonLayout, Pictures}, MIRROR_X};
fn main() {
    let root = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").expect("set NFSPU_GAME_DIR"))
        .join("GameData/Track/Sky");
    let mut files: Vec<_> = std::fs::read_dir(&root).unwrap().map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "bin")).collect();
    files.sort();
    let mut checked = 0;
    for path in files {
        let b = std::fs::read(&path).unwrap();
        let Some(layout) = HorizonLayout::parse(&b) else {
            println!("{}: unsupported header (not an atlas)", path.file_name().unwrap().to_string_lossy());
            continue;
        };
        let read = |o| f32::from_le_bytes(b[o..o+4].try_into().unwrap()) as f64;
        let quads = layout.quads(400.0, MIRROR_X);
        let fsh = std::fs::read(path.with_extension("fsh")).unwrap();
        let pictures = Pictures::parse(&fsh).unwrap();
        let colours = layout.vertex_colours(&pictures).expect("hzcl 16x2 required");
        let u32_at = |o| u32::from_le_bytes(fsh[o..o+4].try_into().unwrap()) as usize;
        let entries = u32_at(8);
        let palette = (0..entries).find_map(|entry| {
            let offset = 16 + entry * 8;
            (&fsh[offset..offset+4] == b"hzcl").then(|| u32_at(offset+4))
        }).unwrap();
        assert_eq!(fsh[palette], 0x7d);
        for segment in 0..16 {
            let current = (layout.colour_start as usize + segment) % 16;
            let next = (current + 1) % 16;
            for (corner, pixel) in [current, next, 16+next, 16+current].into_iter().enumerate() {
                let at = palette + 16 + pixel * 4;
                assert_eq!(colours[segment*4+corner], [fsh[at+2], fsh[at+1], fsh[at], fsh[at+3]]);
            }
        }

        for (i, (positions, uv)) in quads.iter().enumerate() {
            let cell = b[28+i*2] as u32;
            let column = cell % layout.columns;
            let row = cell / layout.columns;
            for corner in 0..4 {
                let edge = (i + usize::from(corner == 1 || corner == 2)) % 16;
                let turn = read(4) + edge as f64 / 16.0;
                let angle = turn * std::f64::consts::TAU;
                let y = read(20) + if corner < 2 { read(24) } else { 0.0 };
                let expected = [angle.sin()*400.0*MIRROR_X as f64,y,angle.cos()*400.0];
                for axis in 0..3 { assert!((positions[corner][axis] as f64-expected[axis]).abs()<0.002); }
                let right = (corner == 1 || corner == 2) ^ (b[29+i*2] != 0);
                let u = (column as f64 + if right {1.0} else {0.0}) / layout.columns as f64;
                let v = (row as f64 + if corner < 2 {0.01} else {0.98}) / layout.rows as f64;
                assert!((uv[corner][0] as f64-u).abs()<0.000001);
                assert!((uv[corner][1] as f64-v).abs()<0.000001);
            }
            assert_eq!(positions[1],quads[(i+1)%16].0[0]);
        }
        println!("{}: PASS 16 quads / 64 vertices / hzcl colours; grid {}x{}, {} flips; colour start {} kept separate",
            path.file_name().unwrap().to_string_lossy(),layout.columns,layout.rows,
            layout.flipped.iter().filter(|x| **x).count(),layout.colour_start);
        checked += 1;
    }
    assert_eq!(checked,17,"expected all 15 track skies plus Monaco and default");
    println!("PASS: {checked} atlas headers; hzcl vertex assignment verified; full sky colours unconfirmed; retail visual parity not tested");
}
