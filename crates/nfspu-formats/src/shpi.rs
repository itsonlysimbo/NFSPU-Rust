//! SHPI texture archives (`.env`, and the decompressed contents of `.fsh`).
//!
//! Layout verified on the author's install (see `docs/provenance/formats.md`):
//! a 16-byte header (`SHPI`, total size, entry count, 4-char directory id), then one
//! 8-byte record per entry (4-char name, byte offset), then the entries themselves.
//! Each entry starts with a 16-byte header: format code, 24-bit size, width, height.
//! Only the entry header is decoded here; pixel conversion comes later.

use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum ShpiError {
    BadMagic,
    Truncated,
    EntryOutOfRange { name: String, offset: usize },
}

impl fmt::Display for ShpiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic => write!(f, "not an SHPI archive"),
            Self::Truncated => write!(f, "SHPI data ended early"),
            Self::EntryOutOfRange { name, offset } => {
                write!(f, "entry '{name}' points to offset {offset}, outside the file")
            }
        }
    }
}

impl std::error::Error for ShpiError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShpiEntry {
    /// Entry name with trailing spaces removed (stored as 4 characters).
    pub name: String,
    pub offset: usize,
    pub format: u8,
    pub width: u16,
    pub height: u16,
    /// Position of this tile inside its texture page (header offsets 12 and 14).
    pub x: u16,
    pub y: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShpiArchive {
    pub directory_id: String,
    pub declared_size: usize,
    pub entries: Vec<ShpiEntry>,
}

fn u32le(d: &[u8], at: usize) -> Result<usize, ShpiError> {
    let b = d.get(at..at + 4).ok_or(ShpiError::Truncated)?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
}

fn u16le(d: &[u8], at: usize) -> Result<u16, ShpiError> {
    let b = d.get(at..at + 2).ok_or(ShpiError::Truncated)?;
    Ok(u16::from_le_bytes([b[0], b[1]]))
}

fn text(d: &[u8]) -> String {
    d.iter().map(|&c| if (32..127).contains(&c) { c as char } else { '?' }).collect::<String>().trim_end().to_string()
}

pub fn parse(data: &[u8]) -> Result<ShpiArchive, ShpiError> {
    if data.get(0..4) != Some(b"SHPI") {
        return Err(ShpiError::BadMagic);
    }
    let declared_size = u32le(data, 4)?;
    let count = u32le(data, 8)?;
    let directory_id = text(data.get(12..16).ok_or(ShpiError::Truncated)?);

    let mut entries = Vec::with_capacity(count);
    for i in 0..count {
        let rec = 16 + i * 8;
        let name = text(data.get(rec..rec + 4).ok_or(ShpiError::Truncated)?);
        let offset = u32le(data, rec + 4)?;
        if offset + 16 > data.len() {
            return Err(ShpiError::EntryOutOfRange { name, offset });
        }
        entries.push(ShpiEntry {
            name,
            offset,
            format: data[offset],
            width: u16le(data, offset + 4)?,
            height: u16le(data, offset + 6)?,
            x: u16le(data, offset + 12)?,
            y: u16le(data, offset + 14)?,
        });
    }
    Ok(ShpiArchive { directory_id, declared_size, entries })
}

/// Pixel format code of 32-bit textures (stored as B, G, R, A bytes).
pub const FORMAT_BGRA32: u8 = 0x7D;
/// Pixel format code of 16-bit textures: 1 alpha bit, then 5 bits each of red, green, blue.
pub const FORMAT_ARGB1555: u8 = 0x7E;
/// Pixel format code of 8-bit paletted textures (driver suits and faces, car damage overlays).
pub const FORMAT_PALETTED8: u8 = 0x7B;
/// Pixel format code of 24-bit textures without alpha, stored B, G, R (traffic car drivers;
/// channel order checked by eye on the skin tones of `eas1`'s `tdri`).
pub const FORMAT_BGR24: u8 = 0x7F;
/// Pixel format code of 16-bit textures with 4 bits each of alpha, red, green, blue (front-end
/// art such as `MovieOval.fsh` and `PopUp.fsh`; checked by eye on the rendered menu frame).
pub const FORMAT_ARGB4444: u8 = 0x6D;
/// Pixel format code of 16-bit opaque textures: 5 bits red, 6 green, 5 blue (`BackButton.fsh`).
pub const FORMAT_RGB565: u8 = 0x78;

impl ShpiArchive {
    /// Finds an entry by its (trimmed) 4-character name.
    pub fn find(&self, name: &str) -> Option<&ShpiEntry> {
        self.entries.iter().find(|e| e.name == name)
    }
}

fn expand5(v: u16) -> u8 {
    ((v << 3) | (v >> 2)) as u8
}

/// Assembles the tiles of an archive into one `size` x `size` RGBA8 texture page, each tile at its
/// own (x, y). Tiles are drawn in directory order with normal alpha blending; tiles that do not fit,
/// or that use a format that cannot be decoded, are skipped.
pub fn compose_page(data: &[u8], archive: &ShpiArchive, size: u32) -> Vec<u8> {
    compose_selected(data, archive, size, None)
}

/// Like `compose_page`, but where several tiles are alternatives for the same spot (same family
/// name and position, like `top`, `top1`, `top2`) only one is drawn: the one numbered `variant`, or
/// the highest-numbered one below it if that variant does not exist.
pub fn compose_page_variant(data: &[u8], archive: &ShpiArchive, size: u32, variant: usize) -> Vec<u8> {
    let keep = select_variant(archive, variant);
    compose_selected(data, archive, size, Some(&keep))
}

/// Like `compose_page`, but draws only the tiles whose flag in `keep` is true (one flag per entry).
pub fn compose_page_filtered(data: &[u8], archive: &ShpiArchive, size: u32, keep: &[bool]) -> Vec<u8> {
    compose_selected(data, archive, size, Some(keep))
}

/// Splits a tile name into its family and variant number: `top2` -> (`top`, 2), `top` -> (`top`, 0).
pub fn split_variant(name: &str) -> (&str, usize) {
    let digits = name.chars().rev().take_while(|c| c.is_ascii_digit()).count();
    let (family, number) = name.split_at(name.len() - digits);
    (family, number.parse().unwrap_or(0))
}

/// For each entry, whether it is the chosen variant of its group. Tiles with the same family name
/// at the same (x, y) form a group; a tile alone at its spot is always kept.
pub fn select_variant(archive: &ShpiArchive, variant: usize) -> Vec<bool> {
    use std::collections::HashMap;
    let mut groups: HashMap<(&str, u16, u16), Vec<(usize, usize)>> = HashMap::new();
    for (i, e) in archive.entries.iter().enumerate() {
        let (family, number) = split_variant(&e.name);
        groups.entry((family, e.x, e.y)).or_default().push((i, number));
    }
    let mut keep = vec![true; archive.entries.len()];
    for members in groups.values().filter(|m| m.len() > 1) {
        let chosen = members.iter().filter(|(_, n)| *n <= variant).max_by_key(|(_, n)| *n).or_else(|| members.iter().min_by_key(|(_, n)| *n)).map(|(i, _)| *i);
        for (i, _) in members {
            keep[*i] = Some(*i) == chosen;
        }
    }
    keep
}

fn compose_selected(data: &[u8], archive: &ShpiArchive, size: u32, keep: Option<&[bool]>) -> Vec<u8> {
    let size = size as usize;
    let mut page = vec![0u8; size * size * 4];
    for (i, e) in archive.entries.iter().enumerate() {
        if keep.is_some_and(|k| !k[i]) {
            continue;
        }
        let Some(rgba) = decode_rgba8(data, e) else { continue };
        let (w, h, x0, y0) = (e.width as usize, e.height as usize, e.x as usize, e.y as usize);
        if x0 + w > size || y0 + h > size {
            continue;
        }
        for row in 0..h {
            for col in 0..w {
                let s = (row * w + col) * 4;
                let a = rgba[s + 3] as u32;
                if a == 0 {
                    continue;
                }
                let d = ((y0 + row) * size + x0 + col) * 4;
                if a == 255 {
                    page[d..d + 4].copy_from_slice(&rgba[s..s + 4]);
                } else {
                    let da = page[d + 3] as u32;
                    let out_a = a + da * (255 - a) / 255;
                    for k in 0..3 {
                        let c = (rgba[s + k] as u32 * a + page[d + k] as u32 * da * (255 - a) / 255) / out_a.max(1);
                        page[d + k] = c as u8;
                    }
                    page[d + 3] = out_a as u8;
                }
            }
        }
    }
    page
}

/// Decodes the full-size image of an entry to RGBA8 (width * height * 4 bytes). Smaller copies of
/// the image (mipmaps) that follow it in the file are ignored. Returns `None` for formats other
/// than 0x7D / 0x7E, or if the data is cut short.
pub fn decode_rgba8(data: &[u8], entry: &ShpiEntry) -> Option<Vec<u8>> {
    let pixels = entry.width as usize * entry.height as usize;
    let at = entry.offset + 16;
    match entry.format {
        FORMAT_BGRA32 => {
            let src = data.get(at..at + pixels * 4)?;
            let mut out = Vec::with_capacity(pixels * 4);
            for px in src.chunks_exact(4) {
                out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
            }
            Some(out)
        }
        FORMAT_ARGB1555 => {
            let src = data.get(at..at + pixels * 2)?;
            let mut out = Vec::with_capacity(pixels * 4);
            for px in src.chunks_exact(2) {
                let v = u16::from_le_bytes([px[0], px[1]]);
                out.extend_from_slice(&[expand5((v >> 10) & 31), expand5((v >> 5) & 31), expand5(v & 31), if v & 0x8000 != 0 { 255 } else { 0 }]);
            }
            Some(out)
        }
        FORMAT_ARGB4444 => {
            let src = data.get(at..at + pixels * 2)?;
            let mut out = Vec::with_capacity(pixels * 4);
            for px in src.chunks_exact(2) {
                let v = u16::from_le_bytes([px[0], px[1]]);
                let n = |shift: u16| (((v >> shift) & 15) * 17) as u8;
                out.extend_from_slice(&[n(8), n(4), n(0), n(12)]);
            }
            Some(out)
        }
        FORMAT_RGB565 => {
            let src = data.get(at..at + pixels * 2)?;
            let mut out = Vec::with_capacity(pixels * 4);
            for px in src.chunks_exact(2) {
                let v = u16::from_le_bytes([px[0], px[1]]);
                let g = ((v >> 5) & 63) as u8;
                out.extend_from_slice(&[expand5((v >> 11) & 31), (g << 2) | (g >> 4), expand5(v & 31), 255]);
            }
            Some(out)
        }
        FORMAT_BGR24 => {
            let src = data.get(at..at + pixels * 3)?;
            let mut out = Vec::with_capacity(pixels * 4);
            for px in src.chunks_exact(3) {
                out.extend_from_slice(&[px[2], px[1], px[0], 255]);
            }
            Some(out)
        }
        FORMAT_PALETTED8 => {
            // 8-bit indices, then a 16-byte attachment header and 256 palette entries. The header's
            // first byte gives the entry type: 0x2A = 4 bytes B, G, R, A; 0x29 = 16-bit RGB565
            // (front-end driver faces in `ALLdrivr.fsh`; checked by eye on the rendered faces).
            let indices = data.get(at..at + pixels)?;
            let kind = *data.get(at + pixels)?;
            let palette_at = at + pixels + 16;
            let mut out = Vec::with_capacity(pixels * 4);
            match kind {
                0x2A => {
                    let palette = data.get(palette_at..palette_at + 1024)?;
                    for &i in indices {
                        let p = &palette[i as usize * 4..i as usize * 4 + 4];
                        out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
                    }
                }
                0x29 => {
                    let palette = data.get(palette_at..palette_at + 512)?;
                    for &i in indices {
                        let v = u16::from_le_bytes([palette[i as usize * 2], palette[i as usize * 2 + 1]]);
                        let g = ((v >> 5) & 63) as u8;
                        out.extend_from_slice(&[expand5((v >> 11) & 31), (g << 2) | (g >> 4), expand5(v & 31), 255]);
                    }
                }
                _ => return None,
            }
            Some(out)
        }
        _ => None,
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a tiny archive with one 2x2 entry using the layout described above.
    fn tiny() -> Vec<u8> {
        let mut d = Vec::new();
        d.extend_from_slice(b"SHPI");
        d.extend_from_slice(&0u32.to_le_bytes()); // total size, patched below
        d.extend_from_slice(&1u32.to_le_bytes());
        d.extend_from_slice(b"GIMX");
        d.extend_from_slice(b"tex ");
        d.extend_from_slice(&24u32.to_le_bytes()); // entry starts right after the record
        d.extend_from_slice(&[0x7D, 0, 0, 0]); // format + 24-bit size
        d.extend_from_slice(&2u16.to_le_bytes());
        d.extend_from_slice(&2u16.to_le_bytes());
        d.extend_from_slice(&[0u8; 8]);
        d.extend_from_slice(&[0u8; 16]); // 2x2x4 pixels
        let len = d.len() as u32;
        d[4..8].copy_from_slice(&len.to_le_bytes());
        d
    }

    #[test]
    fn parses_tiny_archive() {
        let a = parse(&tiny()).unwrap();
        assert_eq!(a.directory_id, "GIMX");
        assert_eq!(a.entries.len(), 1);
        assert_eq!(a.entries[0], ShpiEntry { name: "tex".into(), offset: 24, format: 0x7D, width: 2, height: 2, x: 0, y: 0 });
    }

    #[test]
    fn rejects_other_data() {
        assert_eq!(parse(b"karT0000000000000"), Err(ShpiError::BadMagic));
    }

    #[test]
    fn rejects_entry_past_end() {
        let mut d = tiny();
        d[20..24].copy_from_slice(&9999u32.to_le_bytes());
        assert!(matches!(parse(&d), Err(ShpiError::EntryOutOfRange { .. })));
    }

    #[test]
    fn rejects_truncated_header() {
        assert_eq!(parse(b"SHPI\x00"), Err(ShpiError::Truncated));
    }

    #[test]
    fn decodes_bgra_to_rgba() {
        let mut d = tiny();
        // first pixel of the 2x2 image: B=1, G=2, R=3, A=4
        d[24 + 16..24 + 20].copy_from_slice(&[1, 2, 3, 4]);
        let a = parse(&d).unwrap();
        let e = a.find("tex").unwrap();
        let rgba = decode_rgba8(&d, e).unwrap();
        assert_eq!(rgba.len(), 16);
        assert_eq!(&rgba[0..4], &[3, 2, 1, 4]);
    }

    #[test]
    fn decodes_16_bit_front_end_formats() {
        let mut d = tiny();
        d[24] = FORMAT_ARGB4444;
        // 0x8F0A: alpha 8, red 15, green 0, blue 10
        d[40..42].copy_from_slice(&0x8F0Au16.to_le_bytes());
        let a = parse(&d).unwrap();
        assert_eq!(&decode_rgba8(&d, &a.entries[0]).unwrap()[0..4], &[255, 0, 170, 136]);

        d[24] = FORMAT_RGB565;
        // pure green: 6 bits all set
        d[40..42].copy_from_slice(&0x07E0u16.to_le_bytes());
        let a = parse(&d).unwrap();
        assert_eq!(&decode_rgba8(&d, &a.entries[0]).unwrap()[0..4], &[0, 255, 0, 255]);
    }

    #[test]
    fn other_formats_are_not_decoded() {
        let mut d = tiny();
        d[24] = 0x6C; // a code this reader does not handle
        let a = parse(&d).unwrap();
        assert!(decode_rgba8(&d, &a.entries[0]).is_none());
        assert!(a.find("nope").is_none());
    }

    /// A 2x2 paletted tile: indices 0, 1, 1, 0, a palette attachment, B G R A entries.
    fn paletted() -> Vec<u8> {
        let mut d = Vec::new();
        d.extend_from_slice(b"SHPI");
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(&1u32.to_le_bytes());
        d.extend_from_slice(b"GIMX");
        d.extend_from_slice(b"pal ");
        d.extend_from_slice(&24u32.to_le_bytes());
        d.extend_from_slice(&[0x7B, 0, 0, 0]);
        d.extend_from_slice(&2u16.to_le_bytes());
        d.extend_from_slice(&2u16.to_le_bytes());
        d.extend_from_slice(&[0u8; 8]);
        d.extend_from_slice(&[0, 1, 1, 0]); // indices
        d.extend_from_slice(&[0x2A, 0, 0, 0, 0, 1, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0]); // attachment header
        let mut palette = vec![0u8; 1024];
        palette[0..4].copy_from_slice(&[10, 20, 30, 255]); // entry 0: B=10 G=20 R=30 A=255
        palette[4..8].copy_from_slice(&[1, 2, 3, 128]); // entry 1
        d.extend_from_slice(&palette);
        d
    }

    #[test]
    fn decodes_24_bit_tiles() {
        let mut d = paletted();
        d[24] = FORMAT_BGR24;
        // 2x2 pixels of 3 bytes start right after the 16-byte entry header
        let at = 24 + 16;
        d[at..at + 12].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
        let a = parse(&d).unwrap();
        let rgba = decode_rgba8(&d, &a.entries[0]).unwrap();
        assert_eq!(&rgba[0..8], &[3, 2, 1, 255, 6, 5, 4, 255]);
        d.truncate(at + 5); // cut short
        assert!(decode_rgba8(&d, &a.entries[0]).is_none());
    }

    #[test]
    fn decodes_paletted_tiles() {
        let d = paletted();
        let a = parse(&d).unwrap();
        let rgba = decode_rgba8(&d, &a.entries[0]).unwrap();
        assert_eq!(&rgba[0..4], &[30, 20, 10, 255]);
        assert_eq!(&rgba[4..8], &[3, 2, 1, 128]);
        assert_eq!(&rgba[8..12], &[3, 2, 1, 128]);
        assert_eq!(&rgba[12..16], &[30, 20, 10, 255]);
    }

    #[test]
    fn paletted_tiles_without_a_palette_are_rejected() {
        let mut d = paletted();
        let at = 24 + 16 + 4; // first byte of the attachment header
        d[at] = 0x22; // an attachment kind this reader does not handle
        let a = parse(&d).unwrap();
        assert!(decode_rgba8(&d, &a.entries[0]).is_none());
        d.truncate(24 + 16 + 4 + 20); // palette cut short
        assert!(decode_rgba8(&d, &a.entries[0]).is_none());
    }

    #[test]
    fn decodes_rgb565_palettes() {
        let mut d = paletted();
        let at = 24 + 16 + 4;
        d[at] = 0x29;
        // entry 0 pure red, entry 1 pure blue (2 bytes each)
        d[at + 16..at + 20].copy_from_slice(&[0x00, 0xF8, 0x1F, 0x00]);
        let a = parse(&d).unwrap();
        let rgba = decode_rgba8(&d, &a.entries[0]).unwrap();
        assert_eq!(&rgba[0..8], &[255, 0, 0, 255, 0, 0, 255, 255]);
    }

    #[test]
    fn decodes_1555_to_rgba() {
        let mut d = tiny();
        d[24] = FORMAT_ARGB1555;
        // pixel 0: alpha set, R=31, G=0, B=16  ->  0b1_11111_00000_10000
        let v: u16 = 0b1_11111_00000_10000;
        d[24 + 16..24 + 18].copy_from_slice(&v.to_le_bytes());
        // pixel 1: alpha clear, all zero
        d[24 + 18..24 + 20].copy_from_slice(&0u16.to_le_bytes());
        let a = parse(&d).unwrap();
        let rgba = decode_rgba8(&d, &a.entries[0]).unwrap();
        assert_eq!(&rgba[0..4], &[255, 0, 132, 255]);
        assert_eq!(&rgba[4..8], &[0, 0, 0, 0]);
    }

    #[test]
    fn composes_tiles_into_a_page() {
        // the 2x2 tile from tiny(), moved to (1, 1) inside a 4x4 page, with its first pixel opaque
        let mut d = tiny();
        d[24 + 12..24 + 14].copy_from_slice(&1u16.to_le_bytes());
        d[24 + 14..24 + 16].copy_from_slice(&1u16.to_le_bytes());
        d[24 + 16..24 + 20].copy_from_slice(&[10, 20, 30, 255]); // B, G, R, A of tile pixel (0, 0)
        let a = parse(&d).unwrap();
        assert_eq!((a.entries[0].x, a.entries[0].y), (1, 1));
        let page = compose_page(&d, &a, 4);
        assert_eq!(page.len(), 4 * 4 * 4);
        let at = |x: usize, y: usize| &page[(y * 4 + x) * 4..(y * 4 + x) * 4 + 4];
        assert_eq!(at(1, 1), &[30, 20, 10, 255]); // the tile pixel, now RGBA
        assert_eq!(at(0, 0), &[0, 0, 0, 0]); // outside the tile stays transparent
        assert_eq!(at(2, 2)[3], 0); // transparent tile pixels leave the page untouched
    }

    #[test]
    fn tiles_that_do_not_fit_are_skipped() {
        let mut d = tiny();
        d[24 + 12..24 + 14].copy_from_slice(&3u16.to_le_bytes()); // x = 3, width 2 -> past a 4-wide page
        let a = parse(&d).unwrap();
        assert!(compose_page(&d, &a, 4).iter().all(|&b| b == 0));
    }

    fn entry(name: &str, x: u16, y: u16) -> ShpiEntry {
        ShpiEntry { name: name.into(), offset: 0, format: 0x7D, width: 4, height: 4, x, y }
    }

    #[test]
    fn splits_variant_numbers_off_names() {
        assert_eq!(split_variant("top2"), ("top", 2));
        assert_eq!(split_variant("top"), ("top", 0));
        assert_eq!(split_variant("det14"), ("det", 14));
        assert_eq!(split_variant("0000"), ("", 0));
    }

    #[test]
    fn picks_one_variant_per_spot() {
        let a = ShpiArchive {
            directory_id: "GIMX".into(),
            declared_size: 0,
            entries: vec![
                entry("top", 0, 128),
                entry("top1", 0, 128),
                entry("top2", 0, 128),
                entry("dec", 0, 112),  // different spot: not an alternative
                entry("dec1", 0, 32),  // different spot: not an alternative
                entry("rea", 192, 0),
                entry("rea1", 192, 0), // only two variants
            ],
        };
        assert_eq!(select_variant(&a, 2), vec![false, false, true, true, true, false, true]); // top2, both dec, rea1 (highest below 2)
        assert_eq!(select_variant(&a, 0), vec![true, false, false, true, true, true, false]);
        assert_eq!(select_variant(&a, 1), vec![false, true, false, true, true, false, true]);
    }}
