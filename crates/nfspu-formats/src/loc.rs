//! `.loc` string tables (`FEData/Locale/hud.loc`). Layout worked out from the shipped file
//! (see `docs/provenance/hud.md`):
//! - `LOCH` header: u32 header size (0x2C), u32 version (1), u32 language count, then one u32
//!   offset per language;
//! - at the header's end, `LOCI`: u32 block size, u32 entry count, u32 0, then one (u16 string
//!   id, u16 string index) pair per entry, shared by all languages;
//! - at each language offset, `LOCL`: u32 block size, u32 0, u32 string count, then one u32
//!   offset per string, relative to the `LOCL` tag; strings are zero-terminated, Windows-1252.

use std::collections::BTreeMap;

fn u32le(d: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?) as usize)
}

fn u16le(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(at..at + 2)?.try_into().ok()?))
}

/// Decodes Windows-1252 bytes (the curly quotes and dashes the game uses map to Unicode).
fn cp1252(bytes: &[u8]) -> String {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž', '\u{8F}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™',
        'š', '›', 'œ', '\u{9D}', 'ž', 'Ÿ',
    ];
    bytes.iter().map(|&b| if (0x80..0xA0).contains(&b) { HIGH[(b - 0x80) as usize] } else { b as char }).collect()
}

/// The strings of one language, by id.
pub fn parse(data: &[u8], language: usize) -> Option<BTreeMap<u16, String>> {
    if data.get(0..4) != Some(b"LOCH") {
        return None;
    }
    let header = u32le(data, 4)?;
    let languages = u32le(data, 12)?;
    if language >= languages {
        return None;
    }
    let block = u32le(data, 16 + language * 4)?;
    if data.get(header..header + 4) != Some(b"LOCI") || data.get(block..block + 4) != Some(b"LOCL") {
        return None;
    }
    let entries = u32le(data, header + 8)?;
    let strings = u32le(data, block + 12)?;
    let mut out = BTreeMap::new();
    for e in 0..entries {
        let id = u16le(data, header + 16 + e * 4)?;
        let index = u16le(data, header + 18 + e * 4)? as usize;
        if index >= strings {
            return None;
        }
        let start = block + u32le(data, block + 16 + index * 4)?;
        let tail = data.get(start..)?;
        let end = tail.iter().position(|&b| b == 0)?;
        out.insert(id, cp1252(&tail[..end]));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One language, two ids pointing at strings "gear" and "R".
    fn tiny() -> Vec<u8> {
        let mut d = Vec::new();
        d.extend_from_slice(b"LOCH");
        d.extend_from_slice(&20u32.to_le_bytes()); // header size
        d.extend_from_slice(&1u32.to_le_bytes());
        d.extend_from_slice(&1u32.to_le_bytes()); // one language
        let lang_at = 20 + 16 + 8;
        d.extend_from_slice(&(lang_at as u32).to_le_bytes());
        d.extend_from_slice(b"LOCI");
        d.extend_from_slice(&24u32.to_le_bytes());
        d.extend_from_slice(&2u32.to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(&[10, 0, 1, 0, 0xE1, 0x46, 0, 0]); // id 10 -> string 1, id 18145 -> string 0
        assert_eq!(d.len(), lang_at);
        d.extend_from_slice(b"LOCL");
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(&2u32.to_le_bytes());
        d.extend_from_slice(&24u32.to_le_bytes());
        d.extend_from_slice(&26u32.to_le_bytes());
        d.extend_from_slice(b"R\0gear\x92\0");
        d
    }

    #[test]
    fn reads_strings_by_id() {
        let s = parse(&tiny(), 0).unwrap();
        assert_eq!(s[&18145], "R");
        assert_eq!(s[&10], "gear’");
        assert!(parse(&tiny(), 1).is_none());
        assert!(parse(b"SHPI", 0).is_none());
    }

    #[test]
    #[ignore = "reads the player's install; run with NFSPU_GAME_DIR set"]
    fn reads_the_installed_hud_words() {
        let path = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").expect("NFSPU_GAME_DIR")).join("FEData").join("Locale").join("hud.loc");
        let s = parse(&std::fs::read(path).unwrap(), 0).unwrap();
        assert_eq!(s[&10], "gear");
        assert_eq!(s[&19], "lap");
        assert_eq!(s[&18146], "N");
        assert_eq!(s.len(), 252);
    }
}
