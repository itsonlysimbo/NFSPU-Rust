//! `<track>_forward.key` / `_backwards.key`: per-slice keyframes for the renderer.
//!
//! Layout (`docs/provenance/race.md`, all 3,208 bytes): u32 version (must be 1), u32 number of
//! used keys, then 400 slots of (i32 first slice, f32 value). Unused slots are (-1, 400.0). The
//! last used key has slice 100000 as an end marker. A key's value applies from its slice up to the
//! next key's slice. Values run about 150-600; the game's fallback table is a single 600, which
//! points to a view (draw) distance in metres. These keys are **not** the race path.

#[derive(Debug, Clone, PartialEq)]
pub struct Keys {
    pub keys: Vec<(i32, f32)>,
}

pub fn parse(d: &[u8]) -> Option<Keys> {
    let rd = |at: usize| d.get(at..at + 4).map(|b| [b[0], b[1], b[2], b[3]]);
    if u32::from_le_bytes(rd(0)?) != 1 {
        return None;
    }
    let used = u32::from_le_bytes(rd(4)?) as usize;
    let mut keys = Vec::with_capacity(used);
    for i in 0..used.min(400) {
        let at = 8 + i * 8;
        keys.push((i32::from_le_bytes(rd(at)?), f32::from_le_bytes(rd(at + 4)?)));
    }
    Some(Keys { keys })
}

impl Keys {
    /// The value in force at `slice` (the last key whose slice is at or before it).
    pub fn value_at(&self, slice: i32) -> Option<f32> {
        self.keys.iter().take_while(|k| k.0 <= slice).last().map(|k| k.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_keys_and_looks_up() {
        let mut d = Vec::new();
        for v in [1u32, 3] {
            d.extend(v.to_le_bytes());
        }
        for (s, v) in [(0i32, 400.0f32), (10, 250.0), (100000, 600.0), (-1, 400.0)] {
            d.extend(s.to_le_bytes());
            d.extend(v.to_le_bytes());
        }
        let k = parse(&d).unwrap();
        assert_eq!(k.keys.len(), 3);
        assert_eq!(k.value_at(5), Some(400.0));
        assert_eq!(k.value_at(10), Some(250.0));
        assert_eq!(k.value_at(2000), Some(250.0));
        assert_eq!(k.value_at(-3), None);
        d[0] = 2;
        assert!(parse(&d).is_none());
    }
}
