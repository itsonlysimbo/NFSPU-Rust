//! Engine sound data: the crossfade tables (`CRDl`, `.ect` `.elt` `.cct` `.clt`) inside each
//! engine archive (`GameData/Sounds/sd*.viv`) and the per-car tuning files (`GameData/Sounds/
//! <spec>.spl`).
//!
//! Layout worked out from the shipped files and the executable (`docs/provenance/sound.md`):
//! - `CRDl` table: `CRDl`, version byte 2 at 4, eight signed sound numbers at 0x20 (-1 = layer
//!   unused), then per layer a u32 at 0x128 + 4i: where the layer's volume curve starts, counted
//!   from that field. A volume curve is 512 bytes, one per engine-speed step, 0..127. A second
//!   curve follows each one (offsets at 0x148 + 4i); the game's mixer never reads it.
//! - `.spl`: thirteen little-endian u32 mixer settings; see [`Tuning`].

use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum EngineError {
    BadMagic,
    Truncated,
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic => write!(f, "not a CRDl engine table"),
            Self::Truncated => write!(f, "engine table ended early"),
        }
    }
}

impl std::error::Error for EngineError {}

/// Engine-speed steps of a volume curve.
pub const STEPS: usize = 512;
/// Layers per table.
pub const LAYERS: usize = 8;

/// One crossfade layer: a sound of the engine bank and its volume over engine speed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layer {
    pub sound: usize,
    pub volume: Vec<u8>,
}

/// A `CRDl` table: up to eight layers (unused ones are `None`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub layers: Vec<Option<Layer>>,
}

fn le32(d: &[u8], at: usize) -> Result<u32, EngineError> {
    Ok(u32::from_le_bytes(d.get(at..at + 4).ok_or(EngineError::Truncated)?.try_into().unwrap()))
}

/// Reads a `CRDl` table.
pub fn parse_table(d: &[u8]) -> Result<Table, EngineError> {
    if d.get(0..4) != Some(b"CRDl") {
        return Err(EngineError::BadMagic);
    }
    let mut layers = Vec::with_capacity(LAYERS);
    for i in 0..LAYERS {
        let sound = *d.get(0x20 + i).ok_or(EngineError::Truncated)? as i8;
        if sound < 0 {
            layers.push(None);
            continue;
        }
        let field = 0x128 + 4 * i;
        let at = field + le32(d, field)? as usize;
        let volume = d.get(at..at + STEPS).ok_or(EngineError::Truncated)?.to_vec();
        layers.push(Some(Layer { sound: sound as usize, volume }));
    }
    Ok(Table { layers })
}

/// A car's engine mixer settings (`<spec>.spl`, thirteen u32). Defaults are the executable's own
/// values for a car without a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tuning {
    /// Level for the first-person view mode, 0..0x7ff scale with 64 = 1.
    pub level_near: u32,
    /// Exhaust/intake balance in that mode (0 = only the sounds numbered 64 and up).
    pub balance_near: u32,
    /// Level and balance of the alternative table set (`.cct`/`.clt`).
    pub level_alt: u32,
    pub balance_alt: u32,
    /// Balance and its two swing factors for the outside views.
    pub balance_far: u32,
    pub swing_neg: u32,
    pub swing_pos: u32,
    /// Level for the outside views.
    pub level_far: u32,
    /// Pitch scale (x / 1024 of the engine-speed term).
    pub pitch_scale: u32,
    /// Engine-speed step scale: step = scale * (rpm / max * 65536) >> 14.
    pub step_scale: u32,
    pub extra: [u32; 3],
}

impl Default for Tuning {
    fn default() -> Self {
        Tuning {
            level_near: 0x32,
            balance_near: 0,
            level_alt: 0x46,
            balance_alt: 0x14,
            balance_far: 0x40,
            swing_neg: 0x80,
            swing_pos: 0x80,
            level_far: 0xaa,
            pitch_scale: 0x50,
            step_scale: 0x68,
            extra: [0x19, 0xdc, 0x1e],
        }
    }
}

/// Reads a `.spl` file.
pub fn parse_tuning(d: &[u8]) -> Result<Tuning, EngineError> {
    let v = |i: usize| le32(d, i * 4);
    Ok(Tuning {
        level_near: v(0)?,
        balance_near: v(1)?,
        level_alt: v(2)?,
        balance_alt: v(3)?,
        balance_far: v(4)?,
        swing_neg: v(5)?,
        swing_pos: v(6)?,
        level_far: v(7)?,
        pitch_scale: v(8)?,
        step_scale: v(9)?,
        extra: [v(10)?, v(11)?, v(12)?],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_layers_and_curves() {
        let mut d = vec![0u8; 0x168 + STEPS];
        d[0..4].copy_from_slice(b"CRDl");
        d[4] = 2;
        d[0x20] = 7;
        d[0x21..0x28].fill(0xff);
        d[0x128..0x12c].copy_from_slice(&0x40u32.to_le_bytes());
        d[0x168] = 127;
        d[0x168 + 511] = 5;
        let t = parse_table(&d).unwrap();
        let l = t.layers[0].as_ref().unwrap();
        assert_eq!(l.sound, 7);
        assert_eq!((l.volume[0], l.volume[511]), (127, 5));
        assert!(t.layers[1..].iter().all(Option::is_none));
        assert_eq!(parse_table(b"nope"), Err(EngineError::BadMagic));
    }

    #[test]
    fn reads_tuning() {
        let vals = [0x83u32, 0x33, 0x99, 0x14, 0x40, 0x80, 0x80, 0xff, 0x50, 0x68, 0x0b, 2, 0x0b];
        let d: Vec<u8> = vals.iter().flat_map(|v| v.to_le_bytes()).collect();
        let t = parse_tuning(&d).unwrap();
        assert_eq!((t.level_near, t.level_far, t.pitch_scale, t.step_scale), (0x83, 0xff, 0x50, 0x68));
        assert_eq!(parse_tuning(&d[..40]), Err(EngineError::Truncated));
    }
}
