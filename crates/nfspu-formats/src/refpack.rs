//! EA "RefPack" (a.k.a. QFS) decompression.
//!
//! Layout: 2 flag bytes (`0x10`/`0x11`, `0xFB`), optional compressed size, then the
//! uncompressed size (big-endian, 3 bytes, or 4 if flag bit 0x80 is set), then commands.
//! Each command first copies 0-3 literal bytes from the input, then copies bytes from
//! earlier in the *output* (an LZ77 back-reference). Copies may overlap themselves.

use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum RefPackError {
    BadMagic,
    Truncated,
    BadReference { at: usize, offset: usize },
    SizeMismatch { expected: usize, got: usize },
}

impl fmt::Display for RefPackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic => write!(f, "not RefPack data (missing 0x10FB / 0x11FB header)"),
            Self::Truncated => write!(f, "RefPack data ended early"),
            Self::BadReference { at, offset } => {
                write!(f, "back-reference at output byte {at} points {offset} bytes before the start")
            }
            Self::SizeMismatch { expected, got } => {
                write!(f, "header promised {expected} bytes but decoding produced {got}")
            }
        }
    }
}

impl std::error::Error for RefPackError {}

/// True if the data starts with the RefPack signature.
pub fn is_refpack(data: &[u8]) -> bool {
    data.len() >= 2 && (data[0] & 0x7E) == 0x10 && data[1] == 0xFB
}

/// Reads the uncompressed size promised by the header without decoding.
pub fn uncompressed_size(data: &[u8]) -> Result<usize, RefPackError> {
    if !is_refpack(data) {
        return Err(RefPackError::BadMagic);
    }
    let big = data[0] & 0x80 != 0;
    let has_compressed = data[0] & 0x01 != 0;
    let width = if big { 4 } else { 3 };
    let start = 2 + if has_compressed { width } else { 0 };
    read_be(data, start, width).ok_or(RefPackError::Truncated)
}

fn read_be(data: &[u8], start: usize, width: usize) -> Option<usize> {
    let bytes = data.get(start..start + width)?;
    Some(bytes.iter().fold(0usize, |acc, b| (acc << 8) | *b as usize))
}

pub fn decompress(data: &[u8]) -> Result<Vec<u8>, RefPackError> {
    if !is_refpack(data) {
        return Err(RefPackError::BadMagic);
    }
    let big = data[0] & 0x80 != 0;
    let has_compressed = data[0] & 0x01 != 0;
    let width = if big { 4 } else { 3 };
    let size_pos = 2 + if has_compressed { width } else { 0 };
    let expected = read_be(data, size_pos, width).ok_or(RefPackError::Truncated)?;
    let mut pos = size_pos + width;

    let mut out: Vec<u8> = Vec::with_capacity(expected);
    let byte = |p: usize| data.get(p).copied().ok_or(RefPackError::Truncated);

    loop {
        let b0 = byte(pos)? as usize;
        pos += 1;
        let (literals, copy_len, copy_offset): (usize, usize, usize);

        if b0 < 0x80 {
            let b1 = byte(pos)? as usize;
            pos += 1;
            literals = b0 & 0x03;
            copy_len = ((b0 & 0x1C) >> 2) + 3;
            copy_offset = ((b0 & 0x60) << 3) + b1 + 1;
        } else if b0 < 0xC0 {
            let b1 = byte(pos)? as usize;
            let b2 = byte(pos + 1)? as usize;
            pos += 2;
            literals = b1 >> 6;
            copy_len = (b0 & 0x3F) + 4;
            copy_offset = ((b1 & 0x3F) << 8) + b2 + 1;
        } else if b0 < 0xE0 {
            let b1 = byte(pos)? as usize;
            let b2 = byte(pos + 1)? as usize;
            let b3 = byte(pos + 2)? as usize;
            pos += 3;
            literals = b0 & 0x03;
            copy_len = ((b0 & 0x0C) << 6) + b3 + 5;
            copy_offset = ((b0 & 0x10) << 12) + (b1 << 8) + b2 + 1;
        } else if b0 < 0xFC {
            literals = ((b0 & 0x1F) << 2) + 4;
            copy_len = 0;
            copy_offset = 0;
        } else {
            let tail = b0 & 0x03;
            let src = data.get(pos..pos + tail).ok_or(RefPackError::Truncated)?;
            out.extend_from_slice(src);
            break;
        }

        let src = data.get(pos..pos + literals).ok_or(RefPackError::Truncated)?;
        out.extend_from_slice(src);
        pos += literals;

        if copy_len > 0 {
            if copy_offset > out.len() {
                return Err(RefPackError::BadReference { at: out.len(), offset: copy_offset });
            }
            let mut from = out.len() - copy_offset;
            for _ in 0..copy_len {
                let v = out[from];
                out.push(v);
                from += 1;
            }
        }
    }

    if out.len() != expected {
        return Err(RefPackError::SizeMismatch { expected, got: out.len() });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hand-built stream: header, a 12-byte literal run, then a stop command with 2 literals.
    /// 0xE2 => ((0xE2 & 0x1F) << 2) + 4 = 12 literals. 0xFE => stop + 2 literals.
    #[test]
    fn literals_only() {
        let mut s = vec![0x10, 0xFB, 0x00, 0x00, 0x0E, 0xE2];
        s.extend_from_slice(b"hello world!");
        s.push(0xFE);
        s.extend_from_slice(b"ok");
        assert_eq!(decompress(&s).unwrap(), b"hello world!ok");
    }

    /// Two-byte command: 1 literal then copy 3 bytes from 1 byte back (overlapping run).
    /// b0 = 0b0000_0001 (literals=1, copy_len=3, offset high bits 0), b1 = 0 (offset 1).
    #[test]
    fn overlapping_copy() {
        let s = vec![0x10, 0xFB, 0x00, 0x00, 0x05, 0x01, 0x00, b'a', 0xFC];
        // 'a' literal is read *after* the command bytes: out = "a", then copy 3 from 1 back = "aaaa"
        // total 4 bytes, so promise 4.
        let mut s = s;
        s[4] = 0x04;
        assert_eq!(decompress(&s).unwrap(), b"aaaa");
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(decompress(&[1, 2, 3]), Err(RefPackError::BadMagic));
        assert!(!is_refpack(b"SHPI"));
    }

    #[test]
    fn detects_size_mismatch() {
        let mut s = vec![0x10, 0xFB, 0x00, 0x00, 0x09, 0xFC];
        s.truncate(6);
        assert!(matches!(decompress(&s), Err(RefPackError::SizeMismatch { .. })));
    }

    #[test]
    fn reads_size_from_header() {
        assert_eq!(uncompressed_size(&[0x10, 0xFB, 0x5E, 0x20, 0x00]).unwrap(), 0x5E2000);
    }
}
