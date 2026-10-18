//! Front-end room models (`FEData/Models/<room>.crp` + `<room>.tpg`): the 3D garage, factory,
//! dealer and track rooms drawn behind the menus.
//!
//! A room file is an ordinary car container (`" raC"`, RefPack-compressed) read with
//! [`crate::crp`]. What is specific to rooms is read here (see `docs/provenance/garage.md`):
//! - the `.tpg` text lists the texture pages: `fileN` is embedded archive N-1, with its size and
//!   pixel format (`565` opaque, `4444` with alpha);
//! - each material record names its page at 0x28 (not by material kind, as cars do) and has a
//!   translucent flag at 0x10C (set on the light cones and the floor spot, which use 4444 pages);
//! - an article's `Base` byte 0x4B is its animation slot (0 = static; the garage fans are 1 and 2);
//!   animated articles carry a transform giving their pivot;
//! - `mn` parts hold one normal per vertex (16 bytes, x y z and a pad).

use crate::crp::Crp;

/// One texture page from a room's `.tpg` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TexturePage {
    /// File name the page was built from, e.g. `exterior.fsh` (the archive itself is embedded).
    pub file: String,
    pub width: u32,
    pub height: u32,
    /// Pixel format as written: `565`, `4444`, ...
    pub bpp: String,
}

impl TexturePage {
    /// True if the page has an alpha channel (`4444`, `8888`, `1555`).
    pub fn has_alpha(&self) -> bool {
        matches!(self.bpp.as_str(), "4444" | "8888" | "1555")
    }
}

/// Reads the page list of a room `.tpg`, in archive order (`file1` first). Missing sizes are 0.
pub fn parse_tpg(text: &str) -> Vec<TexturePage> {
    use std::collections::BTreeMap;
    let mut files: BTreeMap<u32, String> = BTreeMap::new();
    let mut file_page: BTreeMap<u32, u32> = BTreeMap::new();
    let mut pages: BTreeMap<u32, (u32, u32, String)> = BTreeMap::new();
    let mut section = String::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.to_ascii_lowercase();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let (key, value) = (key.trim().to_ascii_lowercase(), value.trim());
        let number = |s: &str, prefix: &str| s.strip_prefix(prefix).and_then(|n| n.parse::<u32>().ok());
        if section == "header" {
            if let Some(n) = number(&key, "file") {
                files.insert(n, value.to_string());
            }
        } else if let Some(n) = section.strip_suffix(".details").and_then(|s| number(s, "tpage")) {
            let entry = pages.entry(n).or_insert((0, 0, String::new()));
            match key.as_str() {
                "width" => entry.0 = value.parse().unwrap_or(0),
                "height" => entry.1 = value.parse().unwrap_or(0),
                "bpp" => entry.2 = value.to_string(),
                _ => {}
            }
        } else if let Some(n) = section.strip_suffix(".details").and_then(|s| number(s, "file")) {
            if key == "tpage" {
                if let Ok(p) = value.parse() {
                    file_page.insert(n, p);
                }
            }
        }
    }
    files
        .into_iter()
        .map(|(n, file)| {
            let page = file_page.get(&n).copied().unwrap_or(n);
            let (width, height, bpp) = pages.get(&page).cloned().unwrap_or_default();
            TexturePage { file, width, height, bpp }
        })
        .collect()
}

/// A room material: which embedded archive (texture page) it samples and whether it blends.
#[derive(Debug, Clone, PartialEq)]
pub struct RoomMaterial {
    pub index: u16,
    /// Material kind text, e.g. `CarInt` or `CarIntShd`.
    pub kind: String,
    /// Embedded archive index (0 = the `.tpg`'s `file1`).
    pub page: u32,
    /// Drawn see-through (light cones, the spot on the floor).
    pub translucent: bool,
    /// The float at 0x12C: 1.0 on every garage material except the ceiling light rods' (20.0).
    /// Read as brightness; the rods glow in retail (**guess** at the meaning).
    pub brightness: f32,
}

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    d.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn text(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).trim().to_string()
}

/// All material records (`[index, 0, 't', 'm']` misc parts) read as room materials.
pub fn materials(crp: &Crp) -> Vec<RoomMaterial> {
    crp.misc
        .iter()
        .filter(|p| p.tag[2] == b't' && p.tag[3] == b'm')
        .filter_map(|p| {
            let d = crp.part_data(p);
            Some(RoomMaterial {
                index: u16::from_le_bytes([p.tag[0], p.tag[1]]),
                kind: text(d.get(0x10..0x20)?),
                page: u32_at(d, 0x28)?,
                translucent: u32_at(d, 0x10C)? != 0,
                brightness: f32::from_bits(u32_at(d, 0x12C)?),
            })
        })
        .collect()
}

fn part<'a>(crp: &'a Crp, article: usize, tag: [u8; 4]) -> Option<&'a [u8]> {
    let p = crp.articles.get(article)?.parts.iter().find(|p| p.tag == tag)?;
    Some(crp.part_data(p))
}

/// The article's animation slot from its `Base` part (byte 0x4B); 0 means it does not move.
pub fn animation_slot(crp: &Crp, article: usize) -> u8 {
    part(crp, article, *b"esaB").and_then(|d| d.get(0x4B).copied()).unwrap_or(0)
}

/// One normal per vertex of the article at a level of detail (same order as its positions), or
/// `None` if it has no `mn` part there.
pub fn vertex_normals(crp: &Crp, article: usize, lod: u8) -> Option<Vec<[f32; 3]>> {
    let d = part(crp, article, [lod, 0, b'm', b'n'])?;
    let f = |at: usize| f32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]]);
    Some((0..d.len() / 16).map(|k| [f(k * 16), f(k * 16 + 4), f(k * 16 + 8)]).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crp;

    const TPG: &str = "[header]\nnumfiles=2\nnumtpages=2\nfile1=exterior.fsh\nfile2=spotlite.fsh\n\n\
        [tpage1.details]\nwidth=256\nheight=256\nbpp=565\ntile=1\n\n\
        [tpage2.details]\nwidth=128\nheight=64\nbpp=4444\ntint=0\n\n\
        [file1.details]\ntpage=1\n\n[file2.details]\ntpage=2\n";

    #[test]
    fn reads_tpg_pages_in_file_order() {
        let pages = parse_tpg(TPG);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0], TexturePage { file: "exterior.fsh".into(), width: 256, height: 256, bpp: "565".into() });
        assert_eq!((pages[1].width, pages[1].height), (128, 64));
        assert!(!pages[0].has_alpha());
        assert!(pages[1].has_alpha());
    }

    #[test]
    fn tpg_file_can_point_at_another_page() {
        let text = TPG.replace("[file1.details]\ntpage=1", "[file1.details]\ntpage=2");
        assert_eq!(parse_tpg(&text)[0].width, 128);
    }

    /// A container with one article (a Base part with slot 2 and an `mn` part of two normals) and
    /// two material records.
    fn sample() -> Vec<u8> {
        let mut d = vec![0u8; 0x10];
        d[0..4].copy_from_slice(b" raC");
        d[4..8].copy_from_slice(&(1u32 << 5).to_le_bytes());
        d[8..12].copy_from_slice(&2u32.to_le_bytes()); // two misc parts, listed after the article record
        d[12..16].copy_from_slice(&1u32.to_le_bytes()); // article record at 0x10, misc records after it
        let mut base = vec![0u8; 0x70];
        base[0x4B] = 2;
        let normals: Vec<u8> = [[0.0f32, 1.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]].iter().flatten().flat_map(|f| f.to_le_bytes()).collect();
        let mut mats = Vec::new();
        for (page, translucent) in [(1u32, 0u32), (4, 1)] {
            let mut m = vec![0u8; 0x138];
            m[0x10..0x16].copy_from_slice(b"CarInt");
            m[0x28..0x2C].copy_from_slice(&page.to_le_bytes());
            m[0x10C..0x110].copy_from_slice(&translucent.to_le_bytes());
            m[0x12C..0x130].copy_from_slice(&(if translucent == 0 { 20.0f32 } else { 1.0 }).to_le_bytes());
            mats.push(m);
        }
        // article record at 0x10, misc records at 0x20 and 0x30, the article's part table at 0x40
        let data_at = 0x60;
        let blobs = [&mats[0], &mats[1], &base, &normals];
        let mut offsets = Vec::new();
        let mut at = data_at;
        for b in blobs {
            offsets.push(at);
            at += b.len();
        }
        let rec = |tag: [u8; 4], len: usize, from: usize, to: usize| {
            let mut r = [0u8; 16];
            r[0..4].copy_from_slice(&tag);
            r[5..8].copy_from_slice(&(len as u32).to_le_bytes()[..3]);
            r[12..16].copy_from_slice(&((to - from) as u32).to_le_bytes());
            r
        };
        let mut art = [0u8; 16];
        art[8..12].copy_from_slice(&2u32.to_le_bytes());
        art[12..16].copy_from_slice(&3u32.to_le_bytes()); // part table 3 records further on
        d.extend_from_slice(&art);
        d.extend_from_slice(&rec([3, 0, b't', b'm'], mats[0].len(), 0x20, offsets[0]));
        d.extend_from_slice(&rec([5, 0, b't', b'm'], mats[1].len(), 0x30, offsets[1]));
        d.extend_from_slice(&rec(*b"esaB", base.len(), 0x40, offsets[2]));
        d.extend_from_slice(&rec([1, 0, b'm', b'n'], normals.len(), 0x50, offsets[3]));
        for b in blobs {
            d.extend_from_slice(b);
        }
        d
    }

    #[test]
    fn reads_room_materials() {
        let data = sample();
        let c = crp::parse(&data).unwrap();
        let m = materials(&c);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0], RoomMaterial { index: 3, kind: "CarInt".into(), page: 1, translucent: false, brightness: 20.0 });
        assert_eq!(m[1].brightness, 1.0);
        assert_eq!((m[1].index, m[1].page, m[1].translucent), (5, 4, true));
    }

    #[test]
    fn reads_slot_and_normals() {
        let data = sample();
        let c = crp::parse(&data).unwrap();
        assert_eq!(animation_slot(&c, 0), 2);
        assert_eq!(animation_slot(&c, 9), 0);
        assert_eq!(vertex_normals(&c, 0, 1), Some(vec![[0.0, 1.0, 0.0], [1.0, 0.0, 0.0]]));
        assert_eq!(vertex_normals(&c, 0, 2), None);
    }
}
