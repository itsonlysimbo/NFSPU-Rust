//! EA audio streams (`SCHl` ... `SCEl`), as stored for each song in `zzzymus.viv`.
//!
//! Layout seen in the shipped music (`docs/provenance/music.md`): a chain of blocks, each a 4-byte
//! id and a u32 little-endian size that includes the 8-byte block header.
//! - `SCHl`: `PT\0\0` and the same tagged fields as a sound-bank patch (channels, rate, samples);
//! - `SCCl`: u32 number of data blocks;
//! - `SCDl`: u32 sample count of this block, then one u32 per channel: where that channel's data
//!   starts, counted from just after this table. Each channel's data is two i16 starting values
//!   (newest, then previous) and 15-byte frames of 28 samples: a header byte (high nibble =
//!   predictor 0-3, low nibble = scale), then 14 bytes of nibbles, high nibble first;
//! - `SCEl`: end.
//!
//! The sample rule differs from the sound banks: the nibble is shifted to `20 - scale` and the sum
//! is shifted down without rounding.

use crate::bnk::{parse_patch, Patch};
use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum AsfError {
    BadMagic,
    Truncated,
}

impl fmt::Display for AsfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic => write!(f, "not an EA SCHl audio stream"),
            Self::Truncated => write!(f, "audio stream ended early"),
        }
    }
}

impl std::error::Error for AsfError {}

const COEF1: [i32; 4] = [0, 240, 460, 392];
const COEF2: [i32; 4] = [0, 0, -208, -220];

/// A decoded stream: interleaved 16-bit samples.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Audio {
    pub channels: u16,
    pub sample_rate: u32,
    pub samples: Vec<i16>,
}

fn le32(d: &[u8], at: usize) -> Result<u32, AsfError> {
    Ok(u32::from_le_bytes(d.get(at..at + 4).ok_or(AsfError::Truncated)?.try_into().unwrap()))
}

/// Reads the stream header (`SCHl`).
pub fn header(data: &[u8]) -> Result<Patch, AsfError> {
    if data.get(0..4) != Some(b"SCHl") {
        return Err(AsfError::BadMagic);
    }
    parse_patch(data, 8).ok_or(AsfError::Truncated)
}

/// Decodes one channel of one data block into `out` (every `stride`-th slot from `first`).
fn decode_channel(d: &[u8], at: usize, n: usize, out: &mut [i16], first: usize, stride: usize) -> Result<(), AsfError> {
    let start = d.get(at..at + 4).ok_or(AsfError::Truncated)?;
    let mut h1 = i16::from_le_bytes([start[0], start[1]]) as i32;
    let mut h2 = i16::from_le_bytes([start[2], start[3]]) as i32;
    let mut q = at + 4;
    let mut k = 0;
    while k < n {
        let frame = d.get(q..q + 15).ok_or(AsfError::Truncated)?;
        q += 15;
        let p = (frame[0] >> 4) as usize & 3;
        let shift = 20 - (frame[0] & 0x0F) as i32;
        for i in 0..28.min(n - k) {
            let b = frame[1 + i / 2];
            let nib = (if i % 2 == 0 { b >> 4 } else { b & 0x0F }) as i32;
            // Sign-extend the nibble, then scale it into 1/256 units with the prediction.
            let delta = ((nib << 28) >> 28) << shift.clamp(0, 31);
            let s = ((delta + COEF1[p] * h1 + COEF2[p] * h2) >> 8).clamp(-32768, 32767);
            h2 = h1;
            h1 = s;
            out[(first + k + i) * stride] = s as i16;
        }
        k += 28;
    }
    Ok(())
}

/// Decodes a whole stream.
pub fn decode(data: &[u8]) -> Result<Audio, AsfError> {
    let head = header(data)?;
    let ch = head.channels.max(1) as usize;
    let mut samples = vec![0i16; head.samples as usize * ch];
    let mut done = 0usize;
    let mut at = 0usize;
    while at + 8 <= data.len() {
        let id = &data[at..at + 4];
        let size = le32(data, at + 4)? as usize;
        if id == b"SCEl" || size < 8 {
            break;
        }
        if id == b"SCDl" {
            let body = at + 8;
            let n = (le32(data, body)? as usize).min(head.samples as usize - done);
            let table = body + 4;
            for c in 0..ch {
                let off = le32(data, table + c * 4)? as usize;
                decode_channel(data, table + ch * 4 + off, n, &mut samples[c..], done, ch)?;
            }
            done += n;
        }
        at += size;
    }
    samples.truncate(done * ch);
    Ok(Audio { channels: ch as u16, sample_rate: head.sample_rate, samples })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream() -> Vec<u8> {
        let mut d = b"SCHl".to_vec();
        let pt = [b'P', b'T', 0, 0, 0x82, 1, 1, 0x84, 2, 0x7D, 0x00, 0x85, 1, 28, 0xFF, 0];
        d.extend_from_slice(&(8 + pt.len() as u32).to_le_bytes());
        d.extend_from_slice(&pt);
        // One data block: 28 samples, one channel at offset 0, start values 100 and 0.
        let mut body = 28u32.to_le_bytes().to_vec();
        body.extend_from_slice(&0u32.to_le_bytes());
        body.extend_from_slice(&100i16.to_le_bytes());
        body.extend_from_slice(&0i16.to_le_bytes());
        // Predictor 1 (240/256), scale 12 (shift 8): first nibble 1 adds 1, the rest add 0.
        body.push(0x1C);
        body.push(0x10);
        body.extend_from_slice(&[0; 13]);
        d.extend_from_slice(b"SCDl");
        d.extend_from_slice(&(8 + body.len() as u32).to_le_bytes());
        d.extend_from_slice(&body);
        d.extend_from_slice(b"SCEl");
        d.extend_from_slice(&8u32.to_le_bytes());
        d
    }

    #[test]
    fn reads_header_and_decodes_block() {
        let d = stream();
        let h = header(&d).unwrap();
        assert_eq!((h.channels, h.sample_rate, h.samples), (1, 32000, 28));
        let a = decode(&d).unwrap();
        assert_eq!(a.samples.len(), 28);
        // 1 + 100 * 240/256 = 94.75 -> 94 (shifted down, no rounding); then 94 * 240/256 = 88.
        assert_eq!(&a.samples[0..2], &[94, 88]);
        assert_eq!(decode(b"nope"), Err(AsfError::BadMagic));
    }
}
