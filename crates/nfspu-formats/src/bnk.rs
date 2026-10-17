//! EA sound banks (`GameData/Sounds/*.bnk`, `BNKl` version 4): the front end's effects
//! (`fesfx.bnk`) and collision sounds.
//!
//! Layout seen in the shipped files (`docs/provenance/sound.md`):
//! - header: `BNKl`, u16 version (4) at 4, u16 slot count at 6, then at 0x10 one u32 per slot:
//!   the offset of that slot's patch, counted from the slot's own position (0 or 0xFFFFFFFF =
//!   empty);
//! - a patch starts `PT\0\0` and is a list of tagged fields: a tag byte, a length byte, then that
//!   many big-endian bytes. 0x82 channels, 0x84 sample rate, 0x85 sample count, 0x88 absolute data
//!   offset. 0xFC/0xFD/0xFE are markers without a value; 0xFF ends the list. Defaults when a field
//!   is absent: 1 channel, 22050 Hz;
//! - the sample data is EA "XA" 4-bit ADPCM: per channel, 15-byte frames of 28 samples (one byte
//!   of predictor/shift, then 14 bytes of nibbles, high nibble first). Stereo is **planar** (all of
//!   the left channel's frames, then the right's): with that layout a retail recording's click
//!   sound matches the bank at every click; with alternating frames it does not.

use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum BnkError {
    BadMagic,
    Truncated,
}

impl fmt::Display for BnkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic => write!(f, "not a BNKl sound bank"),
            Self::Truncated => write!(f, "sound bank ended early"),
        }
    }
}

impl std::error::Error for BnkError {}

/// One sound in a bank.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patch {
    pub channels: u16,
    pub sample_rate: u32,
    pub samples: u32,
    pub data_offset: usize,
}

/// How a stereo sound's channel frames are laid out (see `decode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interleave {
    /// Left frame (15 bytes), then right frame, repeating. Not what the shipped banks use.
    Frames,
    /// All of the left channel, then all of the right (the shipped banks).
    Planar,
}

/// Reads the slot table. `None` entries are empty slots.
pub fn parse(data: &[u8]) -> Result<Vec<Option<Patch>>, BnkError> {
    if data.get(0..4) != Some(b"BNKl") {
        return Err(BnkError::BadMagic);
    }
    let count = u16::from_le_bytes(data.get(6..8).ok_or(BnkError::Truncated)?.try_into().unwrap()) as usize;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let slot = 0x10 + i * 4;
        let rel = u32::from_le_bytes(data.get(slot..slot + 4).ok_or(BnkError::Truncated)?.try_into().unwrap());
        if rel == 0 || rel == u32::MAX {
            out.push(None);
            continue;
        }
        out.push(parse_patch(data, slot + rel as usize));
    }
    Ok(out)
}

pub(crate) fn parse_patch(data: &[u8], at: usize) -> Option<Patch> {
    if data.get(at..at + 2)? != b"PT" {
        return None;
    }
    let mut p = Patch { channels: 1, sample_rate: 22050, samples: 0, data_offset: 0 };
    let mut q = at + 4;
    loop {
        let tag = *data.get(q)?;
        q += 1;
        match tag {
            0xFF => break,
            0xFC..=0xFE => continue,
            _ => {}
        }
        let len = *data.get(q)? as usize;
        q += 1;
        let value = data.get(q..q + len)?.iter().fold(0u64, |v, b| (v << 8) | *b as u64);
        q += len;
        match tag {
            0x82 => p.channels = value as u16,
            0x84 => p.sample_rate = value as u32,
            0x85 => p.samples = value as u32,
            0x88 => p.data_offset = value as usize,
            _ => {}
        }
    }
    Some(p)
}

/// EA XA predictor coefficients (x/256), indexed by the frame header's high nibble.
const COEF1: [i32; 4] = [0, 240, 460, 392];
const COEF2: [i32; 4] = [0, 0, -208, -220];
const FRAME: usize = 15;
const FRAME_SAMPLES: usize = 28;

/// Decodes one channel's frames starting at `data[at]`, `stride` bytes apart, into `out`.
fn decode_channel(data: &[u8], mut at: usize, stride: usize, samples: usize, out: &mut Vec<i16>) {
    let (mut h1, mut h2) = (0i32, 0i32);
    while out.len() < samples {
        let Some(frame) = data.get(at..at + FRAME) else { break };
        let index = (frame[0] >> 4) as usize & 3;
        let shift = (frame[0] & 0x0F) as i32 + 8;
        for i in 0..FRAME_SAMPLES {
            if out.len() >= samples {
                break;
            }
            let byte = frame[1 + i / 2];
            let nibble = if i % 2 == 0 { byte >> 4 } else { byte & 0x0F } as i32;
            // Sign-extend the nibble into the top of 32 bits and shift it down by the frame's scale;
            // the sum with the prediction is in 1/256 units (so a step is nibble * 2^(12 - scale)).
            let delta = (nibble << 28) >> shift;
            let s = ((delta + COEF1[index] * h1 + COEF2[index] * h2 + 128) >> 8).clamp(-32768, 32767);
            h2 = h1;
            h1 = s;
            out.push(s as i16);
        }
        at += stride;
    }
}

/// Decodes a patch to interleaved 16-bit samples (L R L R ... for stereo).
pub fn decode(data: &[u8], patch: &Patch, layout: Interleave) -> Vec<i16> {
    let n = patch.samples as usize;
    let ch = patch.channels.max(1) as usize;
    let frames = n.div_ceil(FRAME_SAMPLES);
    let mut channels: Vec<Vec<i16>> = Vec::new();
    for c in 0..ch {
        let (start, stride) = match layout {
            Interleave::Frames => (patch.data_offset + c * FRAME, FRAME * ch),
            Interleave::Planar => (patch.data_offset + c * frames * FRAME, FRAME),
        };
        let mut v = Vec::with_capacity(n);
        decode_channel(data, start, stride, n, &mut v);
        channels.push(v);
    }
    let len = channels.iter().map(Vec::len).min().unwrap_or(0);
    let mut out = Vec::with_capacity(len * ch);
    for i in 0..len {
        for c in &channels {
            out.push(c[i]);
        }
    }
    out
}

/// A 16-bit PCM WAV file of interleaved samples (for listening checks).
pub fn wav(samples: &[i16], channels: u16, rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut w = Vec::with_capacity(44 + data_len as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data_len).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&channels.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * channels as u32 * 2).to_le_bytes());
    w.extend_from_slice(&(channels * 2).to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        w.extend_from_slice(&s.to_le_bytes());
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bank() -> Vec<u8> {
        let mut d = vec![0u8; 0x18];
        d[0..4].copy_from_slice(b"BNKl");
        d[4] = 4;
        d[6] = 2;
        // Slot 0 empty, slot 1 -> patch at 0x18 (relative to 0x14 = 4).
        d[0x10..0x14].copy_from_slice(&u32::MAX.to_le_bytes());
        d[0x14..0x18].copy_from_slice(&4u32.to_le_bytes());
        d.extend_from_slice(b"PT\0\0");
        d.extend_from_slice(&[0xFD, 0x82, 1, 1, 0x84, 2, 0x56, 0x22, 0x85, 1, 28, 0x88, 1, 48, 0xFF]);
        while d.len() < 48 {
            d.push(0);
        }
        // One frame: predictor 0, shift 0+8 -> nibble 1 gives (1<<28)>>8 = 1<<20, clamped.
        d.push(0x0C);
        d.extend_from_slice(&[0x10; 14]);
        d
    }

    #[test]
    fn parses_slots_and_patch_fields() {
        let d = bank();
        let slots = parse(&d).unwrap();
        assert_eq!(slots[0], None);
        assert_eq!(slots[1], Some(Patch { channels: 1, sample_rate: 22050, samples: 28, data_offset: 48 }));
        assert_eq!(parse(b"nope"), Err(BnkError::BadMagic));
    }

    #[test]
    fn decodes_xa_frames() {
        let d = bank();
        let p = parse(&d).unwrap()[1].clone().unwrap();
        let s = decode(&d, &p, Interleave::Frames);
        assert_eq!(s.len(), 28);
        // Scale 12: nibble 1 -> ((1 << 28) >> 20 + 128) >> 8 = 1; nibble 0 -> 0 (no prediction).
        assert_eq!(&s[0..2], &[1, 0]);
    }

    #[test]
    fn wav_header_is_well_formed() {
        let w = wav(&[0, 1, -1], 1, 22050);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(w.len(), 44 + 6);
    }
}
