//! Offline primitive-header inventory; reads the install without writing assets.
use nfspu_formats::{crp, refpack};
use std::{collections::{BTreeMap, BTreeSet}, env, fs, path::PathBuf};
fn main() {
    let root = PathBuf::from(env::var("NFSPU_GAME_DIR").expect("set NFSPU_GAME_DIR")).join("GameData");
    let mut total = BTreeMap::<u16,(usize,usize)>::new();
    for folder in ["Track", "CarModel"] {
        let mut files: Vec<_> = fs::read_dir(root.join(folder)).unwrap().map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|s| s.eq_ignore_ascii_case("crp"))).collect();
        files.sort();
        for file in files {
            let raw = fs::read(&file).unwrap();
            let data = if refpack::is_refpack(&raw) { refpack::decompress(&raw).unwrap() } else { raw };
            let model = crp::parse(&data).unwrap();
            let mut kinds = BTreeMap::<u16,(usize,usize)>::new();
            for (article_index, article) in model.articles.iter().enumerate() {
                for part in &article.parts {
                    if part.tag[2..] != *b"rp" { continue; }
                    let bytes = model.part_data(part);
                    if bytes.len() < 48 { continue; }
                    let index_tables = u32::from_le_bytes(bytes[44..48].try_into().unwrap());
                    if index_tables < 2 { continue; }
                    let mode = u16::from_le_bytes(bytes[..2].try_into().unwrap());
                    let record = kinds.entry(mode).or_default();
                    record.0 += 1; record.1 += part.count;
                    let record = total.entry(mode).or_default();
                    record.0 += 1; record.1 += part.count;
                    if mode != 3 && (folder == "Track" && (part.tag[1] >> 4) == 0) {
                        println!("DETAIL {} article={} {:?} lod={} mode={} count={}", file.file_stem().unwrap().to_string_lossy(),
                            article_index, model.article_name(article_index),part.tag[1] >> 4,mode,part.count);
                    }
                }
            }
            let mut checked_meshes = 0;
            let mut checked_triangles = 0;
            let mut prior_skips = 0;
            for (article_index, article) in model.articles.iter().enumerate() {
                let levels: BTreeSet<_> = article.parts.iter().filter(|p| p.tag[2..] == *b"rp")
                    .map(|p| p.tag[1] >> 4).collect();
                for level in levels {
                    let Some(mesh) = model.article_mesh(article_index, level) else { continue };
                    let mut expected_triangles = 0;
                    for part in &article.parts {
                        if part.tag[2..] != *b"rp" || part.tag[1] >> 4 != level { continue; }
                        let bytes = model.part_data(part);
                        if bytes.len() < 48 { continue; }
                        let tables = u32::from_le_bytes(bytes[44..48].try_into().unwrap());
                        if tables < 2 { continue; }
                        let mode = u16::from_le_bytes(bytes[..2].try_into().unwrap());
                        expected_triangles += match mode {
                            1 => part.count.saturating_sub(2),
                            3 => part.count / 3,
                            _ => panic!("unverified indexed topology {mode}"),
                        };
                    }
                    let mut actual_triangles = 0;
                    for batch in &mesh.batches {
                        assert_eq!(batch.vertex_idx.len()%3,0);
                        assert_eq!(batch.vertex_idx.len(),batch.uv_idx.len());
                        assert_eq!(batch.vertex_idx.len(),batch.colors.len());
                        assert!(batch.vertex_idx.iter().all(|&v| (v as usize) < mesh.positions.len()));
                        assert!(batch.uv_idx.iter().all(|&v| (v as usize) < mesh.uvs.len()));
                        actual_triangles += batch.vertex_idx.len()/3;
                    }
                    if mesh.skipped_batches == 0 {
                        assert_eq!(actual_triangles,expected_triangles,"{} article {article_index} level {level}",file.display());
                    } else { prior_skips += mesh.skipped_batches; }
                    checked_meshes += 1;
                    checked_triangles += actual_triangles;
                }
            }
            println!("CHECK {}: {checked_meshes} meshes / {checked_triangles} normalized triangles / {prior_skips} pre-existing invalid batches skipped",file.file_name().unwrap().to_string_lossy());
            println!("{folder}/{} indexed modes (batches,corners): {kinds:?}",file.file_name().unwrap().to_string_lossy());
        }
    }
    println!("TOTAL indexed modes (batches,corners): {total:?}");
}
