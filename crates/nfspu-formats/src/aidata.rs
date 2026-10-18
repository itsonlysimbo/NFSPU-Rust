//! Data files the opponents drive by (`docs/provenance/race.md`, "Opponents"):
//!
//! - `GameData/Simulation/AICarData/<car>.ais`, 304 bytes per car: a 32-byte name, then 24 floats
//!   (how many road slices ahead the driver looks, by speed), then 40 floats (target speed in m/s
//!   by how sharply the road bends ahead, 3 degrees per step), then 16 unused bytes.
//! - `GameData/Track/racer/work/<track><0|1>.lsp`: u32 version (1), then per road slice the x and
//!   z of the racing line (file space; the height comes from the slice). 0 = forward, 1 = backward.
//! - `FEData/Data/nfs5.per`: the opponent drivers, 44-byte records: u16 id, u16 0, u32 name string
//!   id (`festrings.loc`), u16 difficulty (0 beginner, 1 advanced, 2 expert), u16 0, then four
//!   (skill, range) float pairs: slow corners, medium corners, fast road, default.
//!
//! Layouts worked out from the shipped files; the meaning of each field from what the retail
//! executable does with it. Readers only; nothing is copied into the repository.

fn f32_at(d: &[u8], at: usize) -> Option<f32> {
    Some(f32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

pub const AIS_SIZE: usize = 304;
pub const AIS_LOOKAHEAD: usize = 24;
pub const AIS_SPEEDS: usize = 40;

/// One car's driving tables (`.ais`).
#[derive(Debug, Clone, PartialEq)]
pub struct AiSpec {
    pub name: String,
    /// Slices to look ahead, indexed by `speed (m/s) * 0.2`, capped at 23.
    pub lookahead: [f32; AIS_LOOKAHEAD],
    /// Target speed (m/s), indexed by the bend ahead in 3-degree steps.
    pub speeds: [f32; AIS_SPEEDS],
}

pub fn parse_ais(d: &[u8]) -> Option<AiSpec> {
    if d.len() < AIS_SIZE - 16 {
        return None;
    }
    let end = d[..32].iter().position(|b| *b == 0).unwrap_or(32);
    let name = d[..end].iter().map(|&b| b as char).collect();
    let mut lookahead = [0.0; AIS_LOOKAHEAD];
    for (i, v) in lookahead.iter_mut().enumerate() {
        *v = f32_at(d, 32 + i * 4)?;
    }
    let mut speeds = [0.0; AIS_SPEEDS];
    for (i, v) in speeds.iter_mut().enumerate() {
        *v = f32_at(d, 128 + i * 4)?;
    }
    Some(AiSpec { name, lookahead, speeds })
}

/// Racing line points (file space x, z), one per road slice.
pub fn parse_lsp(d: &[u8]) -> Option<Vec<[f32; 2]>> {
    if u32_at(d, 0)? != 1 || (d.len() - 4) % 8 != 0 {
        return None;
    }
    (0..(d.len() - 4) / 8).map(|i| Some([f32_at(d, 4 + i * 8)?, f32_at(d, 8 + i * 8)?])).collect()
}

pub const PER_RECORD: usize = 44;

/// Which part of the road a skill value applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stretch {
    /// Sharp bends.
    Slow = 0,
    Medium = 1,
    /// Straights and gentle bends.
    Fast = 2,
    Default = 3,
}

/// One opponent driver from `nfs5.per`.
#[derive(Debug, Clone, PartialEq)]
pub struct Driver {
    pub id: u16,
    /// String id of the name in `festrings.loc`.
    pub name_id: u32,
    /// 0 beginner, 1 advanced, 2 expert.
    pub difficulty: u16,
    /// (skill, range) for each `Stretch`, in that order.
    pub skills: [(f32, f32); 4],
}

impl Driver {
    pub fn skill(&self, s: Stretch) -> (f32, f32) {
        self.skills[s as usize]
    }
}

pub fn parse_per(d: &[u8]) -> Vec<Driver> {
    d.chunks_exact(PER_RECORD)
        .filter_map(|r| {
            let mut skills = [(0.0, 0.0); 4];
            for (i, s) in skills.iter_mut().enumerate() {
                *s = (f32_at(r, 12 + i * 8)?, f32_at(r, 16 + i * 8)?);
            }
            Some(Driver { id: u16_at(r, 0)?, name_id: u32_at(r, 4)?, difficulty: u16_at(r, 8)?, skills })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_made_up_files() {
        let mut ais = vec![0u8; AIS_SIZE];
        ais[..4].copy_from_slice(b"Test");
        for i in 0..AIS_LOOKAHEAD {
            ais[32 + i * 4..36 + i * 4].copy_from_slice(&(8.0 + i as f32).to_le_bytes());
        }
        for i in 0..AIS_SPEEDS {
            ais[128 + i * 4..132 + i * 4].copy_from_slice(&(70.0 - i as f32).to_le_bytes());
        }
        let a = parse_ais(&ais).unwrap();
        assert_eq!(a.name, "Test");
        assert_eq!((a.lookahead[0], a.lookahead[23]), (8.0, 31.0));
        assert_eq!((a.speeds[0], a.speeds[39]), (70.0, 31.0));

        let mut lsp = 1u32.to_le_bytes().to_vec();
        for v in [1.0f32, 2.0, 3.0, 4.0] {
            lsp.extend(v.to_le_bytes());
        }
        assert_eq!(parse_lsp(&lsp), Some(vec![[1.0, 2.0], [3.0, 4.0]]));
        assert_eq!(parse_lsp(&[2, 0, 0, 0]), None, "only version 1");

        let mut per = vec![0u8; PER_RECORD];
        per[0] = 9;
        per[4..8].copy_from_slice(&18007u32.to_le_bytes());
        per[8] = 1;
        for (i, v) in [0.77f32, 0.03, 0.78, 0.03, 0.77, 0.05, 0.8, 0.05].iter().enumerate() {
            per[12 + i * 4..16 + i * 4].copy_from_slice(&v.to_le_bytes());
        }
        let d = parse_per(&per);
        assert_eq!(d.len(), 1);
        assert_eq!((d[0].id, d[0].name_id, d[0].difficulty), (9, 18007, 1));
        assert_eq!(d[0].skill(Stretch::Default), (0.8, 0.05));
    }
}
