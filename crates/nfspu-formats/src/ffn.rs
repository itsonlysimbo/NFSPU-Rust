//! FFN bitmap fonts (`FEData/Art/Univers11b.ffn` and friends).
//!
//! Layout worked out from the shipped files (see `docs/provenance/fe.md`):
//! - header, 32 bytes: `FNTF`, u32 file size, u16 at 8 (101 in `Univers11b`), u16 record count at
//!   10 (224 = codes 32..=255), u8 line height at 19, u32 records offset at 20 (32), u32 atlas
//!   offset at 28;
//! - one 11-byte record per character: u16 code, u8 width, u8 height, u16 atlas x, u16 atlas y,
//!   u8 advance, i8 x offset, i8 y offset;
//! - the atlas: a 16-byte image header (format 0x7A, then u16 width and height at 4 and 6) and
//!   4-bit coverage, two pixels per byte.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glyph {
    pub width: u8,
    pub height: u8,
    pub x: u16,
    pub y: u16,
    /// Pen movement after this character.
    pub advance: u8,
    pub x_offset: i8,
    /// Offset of the glyph's top from the line's top.
    pub y_offset: i8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    pub line_height: u8,
    pub glyphs: BTreeMap<u16, Glyph>,
    pub atlas_width: u32,
    pub atlas_height: u32,
    /// Coverage 0..=255 per atlas pixel, top row first.
    pub coverage: Vec<u8>,
}

/// Pixel format code of the 4-bit coverage atlas.
pub const FORMAT_COVERAGE4: u8 = 0x7A;

fn u16le(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(at..at + 2)?.try_into().ok()?))
}

fn u32le(d: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?) as usize)
}

impl Font {
    /// Parses a whole `.ffn` file. `None` if it is not one or is cut short.
    pub fn parse(data: &[u8]) -> Option<Font> {
        if data.get(0..4) != Some(b"FNTF") {
            return None;
        }
        let count = u16le(data, 10)? as usize;
        let line_height = *data.get(19)?;
        let records = u32le(data, 20)?;
        let atlas = u32le(data, 28)?;

        let mut glyphs = BTreeMap::new();
        for i in 0..count {
            let r = data.get(records + i * 11..records + i * 11 + 11)?;
            glyphs.insert(
                u16::from_le_bytes([r[0], r[1]]),
                Glyph {
                    width: r[2],
                    height: r[3],
                    x: u16::from_le_bytes([r[4], r[5]]),
                    y: u16::from_le_bytes([r[6], r[7]]),
                    advance: r[8],
                    x_offset: r[9] as i8,
                    y_offset: r[10] as i8,
                },
            );
        }

        if *data.get(atlas)? != FORMAT_COVERAGE4 {
            return None;
        }
        let atlas_width = u16le(data, atlas + 4)? as u32;
        let atlas_height = u16le(data, atlas + 6)? as u32;
        let packed = data.get(atlas + 16..atlas + 16 + (atlas_width * atlas_height / 2) as usize)?;
        let mut coverage = Vec::with_capacity(packed.len() * 2);
        for &byte in packed {
            // Pixel order within a byte: high nibble first (checked by eye on the atlas).
            coverage.push((byte >> 4) * 17);
            coverage.push((byte & 15) * 17);
        }
        Some(Font { line_height, glyphs, atlas_width, atlas_height, coverage })
    }

    /// The glyph for a character, if the font has one (codes 32..=255).
    pub fn glyph(&self, c: char) -> Option<&Glyph> {
        let code = u32::from(c);
        if code > 0xFFFF {
            return None;
        }
        self.glyphs.get(&(code as u16))
    }

    /// Width of a line of text: the sum of advances (unknown characters count as nothing).
    pub fn measure(&self, text: &str) -> u32 {
        text.chars().filter_map(|c| self.glyph(c)).map(|g| g.advance as u32).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A font with one 2x2 glyph 'A' and a 4x2 atlas.
    fn tiny() -> Vec<u8> {
        let mut d = vec![0u8; 32];
        d[0..4].copy_from_slice(b"FNTF");
        d[8] = 1;
        d[10] = 1;
        d[19] = 12;
        d[20] = 32;
        let atlas = 32 + 11;
        d[28] = atlas as u8;
        d.extend_from_slice(&[b'A', 0, 2, 2, 2, 0, 0, 0, 3, 0xFF, 1]);
        let mut header = [0u8; 16];
        header[0] = FORMAT_COVERAGE4;
        header[4] = 4;
        header[6] = 2;
        d.extend_from_slice(&header);
        d.extend_from_slice(&[0x0F, 0xF0, 0x00, 0xFF]);
        let len = d.len() as u32;
        d[4..8].copy_from_slice(&len.to_le_bytes());
        d
    }

    #[test]
    fn parses_records_and_coverage() {
        let f = Font::parse(&tiny()).unwrap();
        assert_eq!(f.line_height, 12);
        assert_eq!(f.glyph('A'), Some(&Glyph { width: 2, height: 2, x: 2, y: 0, advance: 3, x_offset: -1, y_offset: 1 }));
        assert_eq!((f.atlas_width, f.atlas_height), (4, 2));
        assert_eq!(f.coverage, vec![0, 255, 255, 0, 0, 0, 255, 255]);
        assert_eq!(f.measure("AA?"), 6);
    }

    #[test]
    fn rejects_other_data_and_truncation() {
        assert!(Font::parse(b"SHPI").is_none());
        let mut d = tiny();
        d.truncate(d.len() - 2);
        assert!(Font::parse(&d).is_none());
    }
}
