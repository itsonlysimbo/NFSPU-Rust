//! `BIGF` archives (`.viv`): a big-endian index of named files stored back to back. The music
//! (`GameData/Music/zzzymus.viv`) is one.
//!
//! Layout (`docs/provenance/music.md`): `BIGF`, u32 total size (little-endian, unused here), u32
//! big-endian entry count, u32 big-endian header size, then per entry: u32 BE offset (from the
//! start of the archive), u32 BE size, and a NUL-terminated name.

use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum VivError {
    BadMagic,
    Truncated,
}

impl fmt::Display for VivError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic => write!(f, "not a BIGF archive"),
            Self::Truncated => write!(f, "archive index ended early"),
        }
    }
}

impl std::error::Error for VivError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub offset: u64,
    pub size: u64,
}

fn be32(d: &[u8], at: usize) -> Result<u32, VivError> {
    Ok(u32::from_be_bytes(d.get(at..at + 4).ok_or(VivError::Truncated)?.try_into().unwrap()))
}

/// Reads the index from the start of an archive (`head` must hold at least the whole index; the
/// header size field says how long that is).
pub fn index(head: &[u8]) -> Result<Vec<Entry>, VivError> {
    if head.get(0..4) != Some(b"BIGF") {
        return Err(VivError::BadMagic);
    }
    let count = be32(head, 8)? as usize;
    let mut at = 16;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let offset = be32(head, at)? as u64;
        let size = be32(head, at + 4)? as u64;
        at += 8;
        let len = head.get(at..).ok_or(VivError::Truncated)?.iter().position(|b| *b == 0).ok_or(VivError::Truncated)?;
        let name = String::from_utf8_lossy(&head[at..at + len]).into_owned();
        at += len + 1;
        out.push(Entry { name, offset, size });
    }
    Ok(out)
}

/// The index size a `head` buffer needs, from the first 16 bytes.
pub fn header_size(first16: &[u8]) -> Result<usize, VivError> {
    if first16.get(0..4) != Some(b"BIGF") {
        return Err(VivError::BadMagic);
    }
    Ok(be32(first16, 12)? as usize)
}

/// Reads one entry's bytes from an open archive.
pub fn read_entry(file: &mut (impl std::io::Read + std::io::Seek), entry: &Entry) -> std::io::Result<Vec<u8>> {
    file.seek(std::io::SeekFrom::Start(entry.offset))?;
    let mut buf = vec![0; entry.size as usize];
    file.read_exact(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_index() {
        let mut d = b"BIGF".to_vec();
        d.extend_from_slice(&[0; 4]);
        d.extend_from_slice(&2u32.to_be_bytes());
        d.extend_from_slice(&40u32.to_be_bytes());
        d.extend_from_slice(&40u32.to_be_bytes());
        d.extend_from_slice(&3u32.to_be_bytes());
        d.extend_from_slice(b"a\0");
        d.extend_from_slice(&43u32.to_be_bytes());
        d.extend_from_slice(&1u32.to_be_bytes());
        d.extend_from_slice(b"bc\0");
        assert_eq!(header_size(&d).unwrap(), 40);
        let e = index(&d).unwrap();
        assert_eq!(e[0], Entry { name: "a".into(), offset: 40, size: 3 });
        assert_eq!(e[1].name, "bc");
        assert_eq!(index(b"nope"), Err(VivError::BadMagic));
    }
}
