//! Road slices: the simulation path stored inside a track `.crp`.
//!
//! Layout (`docs/provenance/race.md`): a misc part tagged `SimD` (stored byte-reversed as `DmiS`)
//! holds `count` records of 80 bytes, one per road slice about 6 m apart. An optional `SimT` part
//! holds u32 group lengths; the groups split the slices into runs (the main road first, then
//! alternative routes). Without `SimT` all slices are one group.
//!
//! Record (20 little-endian dwords): 0 unknown (zero), 1-3 centre point, 4-6 up vector, 7-9
//! forward vector, 10-12 right vector (up x forward in the file's left-handed space), 13 a bit
//! field (surface kind, not decoded), 14-15 two small integers (lane counts, used by an alternate
//! grid layout), 16-17 two widths not yet understood, 18 left half-width, 19 right half-width.

use crate::crp::Crp;
use std::fmt;

pub const RECORD_SIZE: usize = 80;

#[derive(Debug, PartialEq, Eq)]
pub enum SimError {
    Missing,
    Truncated,
    BadGroups { groups: usize, slices: usize },
}

impl fmt::Display for SimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => write!(f, "track has no SimD slice data"),
            Self::Truncated => write!(f, "SimD slice data is shorter than its count"),
            Self::BadGroups { groups, slices } => write!(f, "SimT groups cover {groups} slices but there are {slices}"),
        }
    }
}

impl std::error::Error for SimError {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slice {
    pub pos: [f32; 3],
    pub up: [f32; 3],
    pub forward: [f32; 3],
    pub right: [f32; 3],
    /// Dword 13, a bit field. Values seen: 0x18000 (most), 0x1C000, 0x38000, 0x3C000, ...
    pub kind: u32,
    /// Dwords 14-15.
    pub lanes: [u32; 2],
    /// Dwords 16-17, meaning unknown (5.0 on most slices).
    pub extra_widths: [f32; 2],
    /// Road half-width to the left of the centre point (dword 18).
    pub left_width: f32,
    /// Road half-width to the right of the centre point (dword 19).
    pub right_width: f32,
}

#[derive(Debug, Clone, Default)]
pub struct SimData {
    pub slices: Vec<Slice>,
    /// Each group as (first slice, number of slices). The groups are contiguous and in order.
    pub groups: Vec<(usize, usize)>,
}

fn dw(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn fl(d: &[u8], at: usize) -> f32 {
    f32::from_bits(dw(d, at))
}

fn v3(d: &[u8], at: usize) -> [f32; 3] {
    [fl(d, at), fl(d, at + 4), fl(d, at + 8)]
}

/// Parses raw `SimD` bytes (`count` records) and optional raw `SimT` bytes.
pub fn parse(simd: &[u8], count: usize, simt: Option<&[u8]>) -> Result<SimData, SimError> {
    if simd.len() < count * RECORD_SIZE {
        return Err(SimError::Truncated);
    }
    let slices = (0..count)
        .map(|i| {
            let r = &simd[i * RECORD_SIZE..(i + 1) * RECORD_SIZE];
            Slice {
                pos: v3(r, 4),
                up: v3(r, 16),
                forward: v3(r, 28),
                right: v3(r, 40),
                kind: dw(r, 52),
                lanes: [dw(r, 56), dw(r, 60)],
                extra_widths: [fl(r, 64), fl(r, 68)],
                left_width: fl(r, 72),
                right_width: fl(r, 76),
            }
        })
        .collect();
    let lengths: Vec<usize> = match simt {
        Some(t) => t.chunks_exact(4).map(|b| dw(b, 0) as usize).collect(),
        None => vec![count],
    };
    let total: usize = lengths.iter().sum();
    if total != count {
        return Err(SimError::BadGroups { groups: total, slices: count });
    }
    let mut groups = Vec::with_capacity(lengths.len());
    let mut first = 0;
    for len in lengths {
        groups.push((first, len));
        first += len;
    }
    Ok(SimData { slices, groups })
}

/// Reads the slices from a parsed track container.
pub fn read(crp: &Crp) -> Result<SimData, SimError> {
    let simd = crp.misc.iter().find(|p| &p.tag == b"DmiS").ok_or(SimError::Missing)?;
    let simt = crp.misc.iter().find(|p| &p.tag == b"TmiS");
    parse(crp.part_data(simd), simd.count, simt.map(|p| crp.part_data(p)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(x: f32, z: f32) -> Vec<u8> {
        let mut dws = [0u32; 20];
        dws[1] = x.to_bits();
        dws[3] = z.to_bits();
        dws[5] = 1.0f32.to_bits();
        dws[9] = (-1.0f32).to_bits();
        dws[10] = (-1.0f32).to_bits();
        dws[13] = 0x18000;
        dws[14] = 1;
        dws[15] = 2;
        dws[18] = 4.5f32.to_bits();
        dws[19] = 5.5f32.to_bits();
        dws.iter().flat_map(|d| d.to_le_bytes()).collect()
    }

    #[test]
    fn reads_records_and_groups() {
        let mut d = record(1.0, 2.0);
        d.extend(record(3.0, -4.0));
        d.extend(record(5.0, -10.0));
        let t: Vec<u8> = [2u32, 1].iter().flat_map(|v| v.to_le_bytes()).collect();
        let s = parse(&d, 3, Some(&t)).unwrap();
        assert_eq!(s.slices.len(), 3);
        assert_eq!(s.slices[1].pos, [3.0, 0.0, -4.0]);
        assert_eq!(s.slices[0].up, [0.0, 1.0, 0.0]);
        assert_eq!(s.slices[0].forward, [0.0, 0.0, -1.0]);
        assert_eq!(s.slices[0].right, [-1.0, 0.0, 0.0]);
        assert_eq!(s.slices[2].kind, 0x18000);
        assert_eq!(s.slices[2].lanes, [1, 2]);
        assert_eq!((s.slices[0].left_width, s.slices[0].right_width), (4.5, 5.5));
        assert_eq!(s.groups, vec![(0, 2), (2, 1)]);
    }

    #[test]
    fn one_group_without_simt_and_errors() {
        let d = record(0.0, 0.0);
        assert_eq!(parse(&d, 1, None).unwrap().groups, vec![(0, 1)]);
        assert_eq!(parse(&d, 2, None).unwrap_err(), SimError::Truncated);
        let t = 5u32.to_le_bytes();
        assert_eq!(parse(&d, 1, Some(&t)).unwrap_err(), SimError::BadGroups { groups: 5, slices: 1 });
    }
}
