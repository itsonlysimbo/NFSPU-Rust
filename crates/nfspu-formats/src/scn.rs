//! Track scene files (`<track>_fstart.scn`, `_bstart.scn`, `_cameras.scn`, `_audio.scn`, `_st*.scn`).
//!
//! Plain text (`docs/provenance/race.md`). Each element starts with a header line `<KIND>_ELEMENT
//! <version>` (kinds seen: TRIGGER, GEOM, CAMERA, AUDIO), then a position line `x y z`, then an
//! orientation line of three tab-separated rows of three numbers, then kind-specific lines; the
//! last line of an element is its name. Elements run until the next header.
//!
//! Triggers (version 5): a parameter line whose first number is the trigger type (2 = start line,
//! 1 = finish line, 0 = checkpoint, others used by scripts) and whose fourth number is the line
//! width; then a line with the two end points of the trigger line.

#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub kind: String,
    pub version: u32,
    pub pos: [f32; 3],
    /// Three rows of three, as stored.
    pub orientation: [[f32; 3]; 3],
    /// The remaining lines, unparsed (the last one is the element's name).
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trigger {
    pub kind: i32,
    pub width: f32,
    pub pos: [f32; 3],
    /// The two end points of the trigger line.
    pub ends: [[f32; 3]; 2],
}

pub mod trigger_kind {
    pub const CHECKPOINT: i32 = 0;
    pub const FINISH: i32 = 1;
    pub const START: i32 = 2;
}

fn floats(line: &str) -> Vec<f32> {
    line.split_whitespace().filter_map(|t| t.parse().ok()).collect()
}

/// Splits a scene file into elements. Lines before the first header and malformed elements are
/// skipped.
pub fn parse(text: &str) -> Vec<Element> {
    let mut out = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let is_header = |l: &str| {
        let mut w = l.split_whitespace();
        matches!((w.next(), w.next(), w.next()), (Some(k), Some(v), None) if k.ends_with("_ELEMENT") && v.parse::<u32>().is_ok())
    };
    let mut i = 0;
    while i < lines.len() {
        if !is_header(lines[i]) {
            i += 1;
            continue;
        }
        let mut w = lines[i].split_whitespace();
        let kind = w.next().unwrap().trim_end_matches("_ELEMENT").to_string();
        let version = w.next().unwrap().parse().unwrap();
        let mut j = i + 1;
        while j < lines.len() && !is_header(lines[j]) {
            j += 1;
        }
        let body = &lines[i + 1..j];
        i = j;
        if body.len() < 2 {
            continue;
        }
        let p = floats(body[0]);
        let m = floats(body[1]);
        if p.len() != 3 || m.len() != 9 {
            continue;
        }
        out.push(Element {
            kind,
            version,
            pos: [p[0], p[1], p[2]],
            orientation: [[m[0], m[1], m[2]], [m[3], m[4], m[5]], [m[6], m[7], m[8]]],
            lines: body[2..].iter().map(|s| s.trim_end().to_string()).filter(|s| !s.is_empty()).collect(),
        });
    }
    out
}

impl Element {
    /// Trigger fields, for TRIGGER elements of version 5.
    pub fn trigger(&self) -> Option<Trigger> {
        if self.kind != "TRIGGER" || self.version != 5 || self.lines.len() < 2 {
            return None;
        }
        let params: Vec<&str> = self.lines[0].split_whitespace().collect();
        let kind = params.first()?.parse().ok()?;
        let width = params.get(3)?.parse().ok()?;
        let e = floats(&self.lines[1]);
        if e.len() != 6 {
            return None;
        }
        Some(Trigger { kind, width, pos: self.pos, ends: [[e[0], e[1], e[2]], [e[3], e[4], e[5]]] })
    }

    /// The four-character model tag of a GEOM element (e.g. `FNSH`, the finish banner), stored as
    /// a little-endian integer in the second field.
    pub fn geom_tag(&self) -> Option<[u8; 4]> {
        if self.kind != "GEOM" {
            return None;
        }
        let v: u32 = self.lines.first()?.split_whitespace().nth(1)?.parse().ok()?;
        Some(v.to_le_bytes())
    }
}

/// All triggers in a scene.
pub fn triggers(elements: &[Element]) -> Vec<Trigger> {
    elements.iter().filter_map(Element::trigger).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\nTRIGGER_ELEMENT 5\n1.5 0.25 -3\n1 0 0\t0 1 0\t0 0 1\n2 1 1.000000 19.5 0.25 0 200 -1 -1 -1 -1 -1 -1 0 65535 0 0\n-8 0.25 1\t11 0.25 -7\n0 0 0 0\t0 0 0 0\n\nGEOM_ELEMENT 5\n4 5 6\n1 0 0\t0 1 0\t0 0 1\n0 1213419078 0 0 -1 1 1 61.5 61.5 61.5\nGeometry Element\nCAMERA_ELEMENT 4\nbad\n";

    #[test]
    fn splits_elements_and_reads_triggers() {
        let e = parse(SAMPLE);
        assert_eq!(e.len(), 2, "the malformed camera element is skipped");
        assert_eq!((e[0].kind.as_str(), e[0].version), ("TRIGGER", 5));
        let t = e[0].trigger().unwrap();
        assert_eq!(t.kind, trigger_kind::START);
        assert_eq!(t.width, 19.5);
        assert_eq!(t.pos, [1.5, 0.25, -3.0]);
        assert_eq!(t.ends, [[-8.0, 0.25, 1.0], [11.0, 0.25, -7.0]]);
        assert_eq!(e[1].geom_tag(), Some(*b"FNSH"));
        assert_eq!(e[1].lines.last().unwrap(), "Geometry Element");
        assert!(e[1].trigger().is_none());
        assert_eq!(triggers(&e).len(), 1);
    }
}
