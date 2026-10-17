//! EA "Madcow" video (`FEData/Movies/*.mad`): the front-end movies, 192x144 at about 15 fps.
//!
//! Format notes (`docs/provenance/fe.md`, checked frame by frame against FFmpeg's output):
//! - The file is a run of chunks, each `tag` (4 ASCII) + `size` (u32 LE, including these 8
//!   bytes). Video chunks are `MADk` (key frame), `MADm` and `MADe` (frames that may copy from the
//!   previous frame); other tags (audio) are skipped here.
//! - A video chunk's header: u16 frame time in ms at 14 (66 = 15.15 fps), u16 width at 16, u16
//!   height at 18, u8 quantiser scale at 21; the bit stream starts at 24, read as 16-bit
//!   little-endian words, most significant bit first.
//! - Pictures are 4:2:0, coded in 16x16 macroblocks of six 8x8 blocks (four luma, then Cb, Cr).
//!   A block is either intra coded (MPEG-1 style DC + run/level AC) or, in inter frames, a
//!   full-pixel copy from the previous frame plus a constant.

use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum MadError {
    Truncated,
    BadChunk { offset: usize },
    NoPreviousFrame,
    BadCoefficients,
}

impl fmt::Display for MadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => write!(f, "MAD data ended early"),
            Self::BadChunk { offset } => write!(f, "bad chunk at offset {offset}"),
            Self::NoPreviousFrame => write!(f, "inter frame without a previous frame"),
            Self::BadCoefficients => write!(f, "coefficient data runs past the block"),
        }
    }
}

impl std::error::Error for MadError {}

/// One chunk of the file: its tag and its full bytes (header included).
#[derive(Debug, Clone, Copy)]
pub struct Chunk<'a> {
    pub tag: [u8; 4],
    pub data: &'a [u8],
}

impl Chunk<'_> {
    pub fn is_video(&self) -> bool {
        matches!(&self.tag, b"MADk" | b"MADm" | b"MADe")
    }
}

/// Splits a file into chunks. Stops with an error at a chunk that does not fit.
pub fn chunks(file: &[u8]) -> impl Iterator<Item = Result<Chunk<'_>, MadError>> {
    let mut at = 0usize;
    std::iter::from_fn(move || {
        if at >= file.len() {
            return None;
        }
        let head = file.get(at..at + 8);
        let Some(head) = head else {
            at = file.len();
            return Some(Err(MadError::Truncated));
        };
        let size = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
        if size < 8 || at + size > file.len() {
            let offset = at;
            at = file.len();
            return Some(Err(MadError::BadChunk { offset }));
        }
        let chunk = Chunk { tag: [head[0], head[1], head[2], head[3]], data: &file[at..at + size] };
        at += size;
        Some(Ok(chunk))
    })
}

/// A decoded picture: 4:2:0 planes, rows top first.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub width: usize,
    pub height: usize,
    pub y: Vec<u8>,
    pub cb: Vec<u8>,
    pub cr: Vec<u8>,
    /// How long the frame is shown, in milliseconds.
    pub frame_ms: u16,
}

impl Picture {
    fn blank(width: usize, height: usize) -> Picture {
        let (cw, ch) = (width / 2, height / 2);
        Picture { width, height, y: vec![16; width * height], cb: vec![128; cw * ch], cr: vec![128; cw * ch], frame_ms: 0 }
    }

    /// RGBA8 using the BT.601 full-range conversion (luma used as stored, no 16..235 stretch).
    /// Matched to a retail recording: retail's luma percentiles in the movie (11 / 85 / 164 / 189
    /// at 10 / 50 / 90 / 98%) equal the stored values within a few levels, while the studio-range
    /// stretch gave 0 / 78 / 170 / 200.
    pub fn to_rgba(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.width * self.height * 4);
        let cw = self.width / 2;
        for row in 0..self.height {
            for col in 0..self.width {
                let y = self.y[row * self.width + col] as f32;
                let ci = (row / 2) * cw + col / 2;
                let u = self.cb[ci] as f32 - 128.0;
                let v = self.cr[ci] as f32 - 128.0;
                let r = y + 1.402 * v;
                let g = y - 0.344_136 * u - 0.714_136 * v;
                let b = y + 1.772 * u;
                out.extend_from_slice(&[clamp(r), clamp(g), clamp(b), 255]);
            }
        }
        out
    }
}

fn clamp(v: f32) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

/// Reads the bit stream: 16-bit little-endian words, each consumed from its top bit down.
struct Bits<'a> {
    data: &'a [u8],
    /// Position in bits from the start of `data`, counted in word order.
    pos: usize,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Self {
        Bits { data, pos: 0 }
    }

    fn bit(&mut self) -> Result<u32, MadError> {
        let word = self.pos / 16;
        let lo = *self.data.get(word * 2).ok_or(MadError::Truncated)?;
        let hi = *self.data.get(word * 2 + 1).ok_or(MadError::Truncated)?;
        let w = u16::from_le_bytes([lo, hi]);
        let b = (w >> (15 - self.pos % 16)) & 1;
        self.pos += 1;
        Ok(b as u32)
    }

    fn bits(&mut self, n: u32) -> Result<u32, MadError> {
        let mut v = 0;
        for _ in 0..n {
            v = (v << 1) | self.bit()?;
        }
        Ok(v)
    }

    /// `n` bits as a two's-complement signed number.
    fn signed(&mut self, n: u32) -> Result<i32, MadError> {
        let v = self.bits(n)? as i32;
        Ok(if v >= 1 << (n - 1) { v - (1 << n) } else { v })
    }
}

/// What a coefficient code means.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Coef {
    /// `run` zero coefficients skipped, then `level` (sign follows in the stream).
    RunLevel { run: u8, level: u8 },
    Escape,
    EndOfBlock,
}

/// The MPEG-1 DCT coefficient codes (ISO/IEC 11172-2 Table B.5c, the form used after the first
/// coefficient), as (bit string, run, level). The sign bit is not part of the string. Values
/// checked against FFmpeg's tables; layout is ours.
const COEFFICIENT_CODES: &[(&str, u8, u8)] = &[
    ("11", 0, 1), ("0100", 0, 2), ("00101", 0, 3), ("0000110", 0, 4),
    ("00100110", 0, 5), ("00100001", 0, 6), ("0000001010", 0, 7), ("000000011101", 0, 8),
    ("000000011000", 0, 9), ("000000010011", 0, 10), ("000000010000", 0, 11), ("0000000011010", 0, 12),
    ("0000000011001", 0, 13), ("0000000011000", 0, 14), ("0000000010111", 0, 15), ("00000000011111", 0, 16),
    ("00000000011110", 0, 17), ("00000000011101", 0, 18), ("00000000011100", 0, 19), ("00000000011011", 0, 20),
    ("00000000011010", 0, 21), ("00000000011001", 0, 22), ("00000000011000", 0, 23), ("00000000010111", 0, 24),
    ("00000000010110", 0, 25), ("00000000010101", 0, 26), ("00000000010100", 0, 27), ("00000000010011", 0, 28),
    ("00000000010010", 0, 29), ("00000000010001", 0, 30), ("00000000010000", 0, 31), ("000000000011000", 0, 32),
    ("000000000010111", 0, 33), ("000000000010110", 0, 34), ("000000000010101", 0, 35), ("000000000010100", 0, 36),
    ("000000000010011", 0, 37), ("000000000010010", 0, 38), ("000000000010001", 0, 39), ("000000000010000", 0, 40),
    ("011", 1, 1), ("000110", 1, 2), ("00100101", 1, 3), ("0000001100", 1, 4),
    ("000000011011", 1, 5), ("0000000010110", 1, 6), ("0000000010101", 1, 7), ("000000000011111", 1, 8),
    ("000000000011110", 1, 9), ("000000000011101", 1, 10), ("000000000011100", 1, 11), ("000000000011011", 1, 12),
    ("000000000011010", 1, 13), ("000000000011001", 1, 14), ("0000000000010011", 1, 15), ("0000000000010010", 1, 16),
    ("0000000000010001", 1, 17), ("0000000000010000", 1, 18), ("0101", 2, 1), ("0000100", 2, 2),
    ("0000001011", 2, 3), ("000000010100", 2, 4), ("0000000010100", 2, 5), ("00111", 3, 1),
    ("00100100", 3, 2), ("000000011100", 3, 3), ("0000000010011", 3, 4), ("00110", 4, 1),
    ("0000001111", 4, 2), ("000000010010", 4, 3), ("000111", 5, 1), ("0000001001", 5, 2),
    ("0000000010010", 5, 3), ("000101", 6, 1), ("000000011110", 6, 2), ("0000000000010100", 6, 3),
    ("000100", 7, 1), ("000000010101", 7, 2), ("0000111", 8, 1), ("000000010001", 8, 2),
    ("0000101", 9, 1), ("0000000010001", 9, 2), ("00100111", 10, 1), ("0000000010000", 10, 2),
    ("00100011", 11, 1), ("0000000000011010", 11, 2), ("00100010", 12, 1), ("0000000000011001", 12, 2),
    ("00100000", 13, 1), ("0000000000011000", 13, 2), ("0000001110", 14, 1), ("0000000000010111", 14, 2),
    ("0000001101", 15, 1), ("0000000000010110", 15, 2), ("0000001000", 16, 1), ("0000000000010101", 16, 2),
    ("000000011111", 17, 1), ("000000011010", 18, 1), ("000000011001", 19, 1), ("000000010111", 20, 1),
    ("000000010110", 21, 1), ("0000000011111", 22, 1), ("0000000011110", 23, 1), ("0000000011101", 24, 1),
    ("0000000011100", 25, 1), ("0000000011011", 26, 1), ("0000000000011111", 27, 1), ("0000000000011110", 28, 1),
    ("0000000000011101", 29, 1), ("0000000000011100", 30, 1), ("0000000000011011", 31, 1),
];
const ESCAPE: &str = "000001";
const END_OF_BLOCK: &str = "10";

/// A prefix-code lookup keyed by (length, value); codes are at most 16 bits.
struct CodeTable {
    by_len: Vec<Vec<(u32, Coef)>>,
}

impl CodeTable {
    fn new() -> CodeTable {
        let mut by_len = vec![Vec::new(); 17];
        let mut add = |s: &str, c: Coef| by_len[s.len()].push((u32::from_str_radix(s, 2).unwrap(), c));
        for &(s, run, level) in COEFFICIENT_CODES {
            add(s, Coef::RunLevel { run, level });
        }
        add(ESCAPE, Coef::Escape);
        add(END_OF_BLOCK, Coef::EndOfBlock);
        CodeTable { by_len }
    }

    fn read(&self, bits: &mut Bits) -> Result<Coef, MadError> {
        let mut v = 0u32;
        for len in 1..=16 {
            v = (v << 1) | bits.bit()?;
            if let Some(&(_, c)) = self.by_len[len].iter().find(|(code, _)| *code == v) {
                return Ok(c);
            }
        }
        Err(MadError::BadCoefficients)
    }
}

/// Zig-zag scan: position k of the stream is raster index ZIGZAG[k].
const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57,
    50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

/// MPEG-1 default intra quantiser matrix (ISO/IEC 11172-2), raster order.
const INTRA_MATRIX: [u16; 64] = [
    8, 16, 19, 22, 26, 27, 29, 34, 16, 16, 22, 24, 27, 29, 34, 37, 19, 22, 26, 27, 29, 34, 34, 38, 22, 22, 26, 27, 29, 34, 37, 40, 22, 26, 27,
    29, 32, 35, 40, 48, 26, 27, 29, 32, 35, 40, 48, 58, 26, 27, 29, 34, 38, 46, 56, 69, 27, 29, 35, 38, 46, 56, 69, 83,
];

/// Decodes the video chunks of one file in order.
pub struct Decoder {
    codes: CodeTable,
    cosines: [[f64; 8]; 8],
    previous: Option<Picture>,
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder {
    pub fn new() -> Decoder {
        let mut cosines = [[0.0; 8]; 8];
        for (x, row) in cosines.iter_mut().enumerate() {
            for (u, c) in row.iter_mut().enumerate() {
                let scale = if u == 0 { std::f64::consts::FRAC_1_SQRT_2 } else { 1.0 };
                *c = scale * (((2 * x + 1) * u) as f64 * std::f64::consts::PI / 16.0).cos() / 2.0;
            }
        }
        Decoder { codes: CodeTable::new(), cosines, previous: None }
    }

    /// Decodes one video chunk (header included) and returns the picture.
    pub fn decode(&mut self, chunk: &Chunk) -> Result<&Picture, MadError> {
        let d = chunk.data;
        let inter = matches!(&chunk.tag, b"MADm" | b"MADe");
        let field = |at: usize| d.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or(MadError::Truncated);
        let frame_ms = field(14)?;
        let (width, height) = (field(16)? as usize, field(18)? as usize);
        let quant = *d.get(21).ok_or(MadError::Truncated)? as i32;
        let body = d.get(24..).ok_or(MadError::Truncated)?;
        let body = &body[..body.len() & !1];

        let previous = self.previous.take();
        if inter && previous.is_none() {
            return Err(MadError::NoPreviousFrame);
        }
        let mut pic = match &previous {
            Some(p) if p.width == width && p.height == height => p.clone(),
            _ => Picture::blank(width, height),
        };
        pic.frame_ms = frame_ms;
        let mut bits = Bits::new(body);
        let result = (|| {
            for mby in 0..height.div_ceil(16) {
                for mbx in 0..width.div_ceil(16) {
                    self.macroblock(&mut bits, &mut pic, previous.as_ref(), mbx, mby, inter, quant)?;
                }
            }
            Ok(())
        })();
        self.previous = Some(pic);
        result.map(|_| self.previous.as_ref().unwrap())
    }

    #[allow(clippy::too_many_arguments)]
    fn macroblock(&self, bits: &mut Bits, pic: &mut Picture, previous: Option<&Picture>, mbx: usize, mby: usize, inter: bool, quant: i32) -> Result<(), MadError> {
        // In inter frames a short prefix says which blocks are copies: "1" = all six, "01" =
        // a 6-bit mask follows, "00" = none. Copies share one motion vector.
        let mut copy_mask = 0u32;
        let (mut mvx, mut mvy) = (0, 0);
        if inter {
            let kind = if bits.bit()? == 1 { 0 } else { 2 - bits.bit()? };
            if kind < 2 {
                copy_mask = if kind == 1 { bits.bits(6)? } else { 63 };
                mvx = motion(bits)?;
                mvy = motion(bits)?;
            }
        }
        for block in 0..6 {
            if copy_mask & (1 << block) != 0 {
                let add = 2 * motion(bits)?;
                if let Some(prev) = previous {
                    copy_block(pic, prev, mbx, mby, block, mvx, mvy, add);
                }
            } else {
                let coeffs = self.intra_block(bits, quant)?;
                self.put_block(pic, mbx, mby, block, &coeffs);
            }
        }
        Ok(())
    }

    /// Reads one intra block and returns its dequantised DCT coefficients (raster order).
    fn intra_block(&self, bits: &mut Bits, quant: i32) -> Result<[f64; 64], MadError> {
        let mut c = [0.0f64; 64];
        // DC: 8-bit signed offset from mid grey; the flat block value is 128 + dc. In the scaled
        // domain its step is 16 (scale 1), which is 8 in true DCT units.
        c[0] = 8.0 * (128 + bits.signed(8)?) as f64;
        let steps = scaled_steps(quant);
        let mut k = 0usize;
        loop {
            let (run, level) = match self.codes.read(bits)? {
                Coef::EndOfBlock => break,
                Coef::RunLevel { run, level } => {
                    let negative = bits.bit()? == 1;
                    (run as usize + 1, if negative { -(level as i32) } else { level as i32 })
                }
                // Escape: a signed 10-bit level, then a 6-bit run.
                Coef::Escape => {
                    let level = bits.signed(10)?;
                    let run = bits.bits(6)? as usize + 1;
                    (run, level)
                }
            };
            k += run;
            if k > 63 {
                return Err(MadError::BadCoefficients);
            }
            let at = ZIGZAG[k];
            c[at] = dequantise(level, steps[at]) as f64 * AAN[at / 8] * AAN[at % 8] / 2.0;
        }
        Ok(c)
    }

    /// Inverse DCT of one block into the picture (luma blocks 0-3, then Cb, Cr).
    fn put_block(&self, pic: &mut Picture, mbx: usize, mby: usize, block: usize, c: &[f64; 64]) {
        let mut tmp = [0.0f64; 64];
        for y in 0..8 {
            for x in 0..8 {
                let mut s = 0.0;
                for u in 0..8 {
                    s += self.cosines[x][u] * c[y * 8 + u];
                }
                tmp[y * 8 + x] = s;
            }
        }
        let (plane, stride, ox, oy) = block_target(pic, mbx, mby, block);
        for y in 0..8 {
            for x in 0..8 {
                let mut s = 0.0;
                for v in 0..8 {
                    s += self.cosines[y][v] * tmp[v * 8 + x];
                }
                let (px, py) = (ox + x, oy + y);
                if px < stride && py * stride + px < plane.len() {
                    plane[py * stride + px] = s.round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
}

/// The scaled-DCT factors s(k) = sqrt(2) * cos(k * pi / 16), with s(0) = 1. A fast (Arai, Agui
/// and Nakajima) inverse DCT takes coefficients pre-divided by s(u) * s(v).
const AAN: [f64; 8] = [1.0, 1.387_039_845_322_148, 1.306_562_964_876_377, 1.175_875_602_419_359, 1.0, 0.785_694_958_387_102, 0.541_196_100_146_197, 0.275_899_379_282_943];

/// Per-coefficient dequantisation steps in that scaled domain, in 1/16 units: the MPEG-1 intra
/// matrix times the frame's scale, times 4 / (s(u) * s(v)) with the reciprocal held as a 12-bit
/// fixed-point integer. Done this way (rather than in true DCT units) because the rounding has
/// to match the reference decoder's.
fn scaled_steps(quant: i32) -> [i32; 64] {
    let mut steps = [0i32; 64];
    for (i, step) in steps.iter_mut().enumerate() {
        let inverse = (4096.0 / (AAN[i / 8] * AAN[i % 8])).round() as i32;
        *step = (inverse * INTRA_MATRIX[i] as i32 * quant + 32) >> 10;
    }
    steps
}

/// Dequantises an AC level in the scaled domain and makes it odd (MPEG-1 mismatch control).
fn dequantise(level: i32, step: i32) -> i32 {
    let v = ((level.abs() * step) >> 4) - 1 | 1;
    if level < 0 { -v } else { v }
}

/// A motion component: "0" = 0; "1" then a sign-ish bit and 4 bits.
fn motion(bits: &mut Bits) -> Result<i32, MadError> {
    if bits.bit()? == 0 {
        return Ok(0);
    }
    let base = if bits.bit()? == 1 { -17 } else { 0 };
    Ok(base + bits.bits(4)? as i32 + 1)
}

/// The plane, its stride and the top-left pixel of block `block` of macroblock (mbx, mby).
fn block_target(pic: &mut Picture, mbx: usize, mby: usize, block: usize) -> (&mut Vec<u8>, usize, usize, usize) {
    match block {
        0..=3 => (&mut pic.y, pic.width, mbx * 16 + (block & 1) * 8, mby * 16 + (block >> 1) * 8),
        4 => (&mut pic.cb, pic.width / 2, mbx * 8, mby * 8),
        _ => (&mut pic.cr, pic.width / 2, mbx * 8, mby * 8),
    }
}

/// Copies an 8x8 block from the previous picture, moved by the motion vector (halved, toward
/// zero, for chroma), adding `add` to every sample. A source outside the picture copies nothing.
#[allow(clippy::too_many_arguments)]
fn copy_block(pic: &mut Picture, prev: &Picture, mbx: usize, mby: usize, block: usize, mvx: i32, mvy: i32, add: i32) {
    let (mvx, mvy) = if block < 4 { (mvx, mvy) } else { (mvx / 2, mvy / 2) };
    let (src, w, h) = match block {
        0..=3 => (&prev.y, prev.width, prev.height),
        4 => (&prev.cb, prev.width / 2, prev.height / 2),
        _ => (&prev.cr, prev.width / 2, prev.height / 2),
    };
    let (plane, stride, ox, oy) = block_target(pic, mbx, mby, block);
    let (sx, sy) = (ox as i32 + mvx, oy as i32 + mvy);
    if sx < 0 || sy < 0 || sx as usize + 8 > w || sy as usize + 8 > h {
        return;
    }
    for y in 0..8 {
        for x in 0..8 {
            let v = src[(sy as usize + y) * w + sx as usize + x] as i32 + add;
            let at = (oy + y) * stride + ox + x;
            if at < plane.len() {
                plane[at] = v.clamp(0, 255) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_code_table_is_prefix_free_and_complete() {
        assert_eq!(COEFFICIENT_CODES.len(), 111);
        let mut all: Vec<&str> = COEFFICIENT_CODES.iter().map(|c| c.0).collect();
        all.push(ESCAPE);
        all.push(END_OF_BLOCK);
        for a in &all {
            for b in &all {
                if a != b {
                    assert!(!b.starts_with(a), "{a} is a prefix of {b}");
                }
            }
        }
    }

    #[test]
    fn bits_read_words_high_bit_first() {
        // Word 0x8001 stored little-endian: 01 80.
        let data = [0x01, 0x80];
        let mut b = Bits::new(&data);
        assert_eq!(b.bit().unwrap(), 1);
        assert_eq!(b.bits(14).unwrap(), 0);
        assert_eq!(b.bit().unwrap(), 1);
        assert!(b.bit().is_err());
    }

    #[test]
    fn chunks_split_on_sizes() {
        let mut f = Vec::new();
        f.extend_from_slice(b"MADk");
        f.extend_from_slice(&10u32.to_le_bytes());
        f.extend_from_slice(&[0, 0]);
        f.extend_from_slice(b"SCDl");
        f.extend_from_slice(&8u32.to_le_bytes());
        let tags: Vec<[u8; 4]> = chunks(&f).map(|c| c.unwrap().tag).collect();
        assert_eq!(tags, vec![*b"MADk", *b"SCDl"]);
        assert!(chunks(&f[..9]).any(|c| c.is_err()));
    }

    #[test]
    fn a_dc_only_block_is_flat() {
        let d = Decoder::new();
        let mut pic = Picture::blank(16, 16);
        let mut c = [0.0; 64];
        c[0] = 8.0 * 200.0;
        d.put_block(&mut pic, 0, 0, 0, &c);
        assert!(pic.y[..8].iter().all(|&v| v == 200));
    }
}
