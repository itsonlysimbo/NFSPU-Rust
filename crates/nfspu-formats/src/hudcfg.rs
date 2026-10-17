//! In-race HUD layouts: `FEData/Data/nfs5-1.hud` / `nfs5-2.hud` and the player's own copies in
//! `SaveData/<player>1.hud` / `<player>2.hud`. See `docs/provenance/hud.md`.
//!
//! The first file is a list of 480-byte layout records ("Config 1".."Config 4", plus copies made
//! for other screen sizes). The second file has one 104-byte record per layout record, in the
//! same order: two more element boxes, the screen size the layout was saved for, and the
//! on/off/style switches the HUD options screen changes.
//!
//! Positions are stored as fractions of the screen size the record was saved for; sizes are in
//! pixels and do not change with the screen size.

/// Size of one record in the `...1.hud` file.
pub const LAYOUT_RECORD: usize = 480;
/// Size of one record in the `...2.hud` file.
pub const SWITCH_RECORD: usize = 104;

/// The HUD elements that have a box in the layout record, in file order. The position table at
/// 0x188 uses the same order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Slot {
    /// Current gear (value + "gear" label).
    Gear,
    /// Digital speed (value + "km/h" / "mph" label), when the speedometer option is digital.
    SpeedDigital,
    /// Race / lap time (value + "time" label).
    Time,
    /// Damage as text (rows of part names).
    DamageText,
    /// Opponent list and "position" line, one row of this height per car.
    Position,
    /// Damage as a car picture.
    DamagePicture,
    /// Analog tachometer.
    Tach,
    /// Analog speedometer.
    Speedometer,
    /// Track map.
    Map,
    /// Race leader's picture.
    Leader,
    /// Rear-view mirror.
    Rearview,
}

impl Slot {
    pub const ALL: [Slot; 11] = [
        Slot::Gear,
        Slot::SpeedDigital,
        Slot::Time,
        Slot::DamageText,
        Slot::Position,
        Slot::DamagePicture,
        Slot::Tach,
        Slot::Speedometer,
        Slot::Map,
        Slot::Leader,
        Slot::Rearview,
    ];

    /// Byte offset of this element's (width, height) pair in a layout record.
    fn size_offset(self) -> usize {
        match self {
            Slot::Gear => 0x94,
            Slot::SpeedDigital => 0x9C,
            Slot::Time => 0xA4,
            Slot::DamageText => 0xAC,
            Slot::Position => 0xB4,
            Slot::DamagePicture => 0xBC,
            Slot::Tach => 0xC4,
            Slot::Speedometer => 0xD4,
            Slot::Map => 0xE4,
            Slot::Leader => 0xF4,
            Slot::Rearview => 0xFC,
        }
    }

    fn index(self) -> usize {
        Slot::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }
}

/// An element's box: size in pixels and top-left as a fraction of the saved screen size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HudBox {
    pub width: i32,
    pub height: i32,
    pub x: f32,
    pub y: f32,
}

/// A colour stored as a little-endian 0xAARRGGBB word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Argb(pub u32);

impl Argb {
    pub fn alpha(self) -> u8 {
        (self.0 >> 24) as u8
    }
    /// [r, g, b, a] as 0..=1 floats.
    pub fn to_f32(self) -> [f32; 4] {
        let c = |s: u32| ((self.0 >> s) & 0xFF) as f32 / 255.0;
        [c(16), c(8), c(0), c(24)]
    }
}

/// One record of the `...1.hud` file.
#[derive(Debug, Clone, PartialEq)]
pub struct HudLayout {
    pub name: String,
    /// Fill behind boxed elements (its alpha byte is the translucency option; 0 = no boxes).
    pub background: Argb,
    /// Label text colour ("gear", "time", ...).
    pub label: Argb,
    /// Second text colour (opponent names, damage rows).
    pub text: Argb,
    /// The other colours of the record (0x20..0x30), meaning not worked out yet.
    pub extra_colours: [Argb; 5],
    /// Padding inside text boxes: x at 0x34, y at 0x38.
    pub pad: [i32; 2],
    /// Big font, small font, small italic font (`.ffn` file names).
    pub fonts: [String; 3],
    boxes: [HudBox; 11],
}

impl HudLayout {
    pub fn slot(&self, slot: Slot) -> HudBox {
        self.boxes[slot.index()]
    }
}

/// One record of the `...2.hud` file.
#[derive(Debug, Clone, PartialEq)]
pub struct HudSwitches {
    /// Box of the multi-purpose time (record / best lap / last lap).
    pub multi_time: HudBox,
    /// Box of the digital tachometer.
    pub tach_digital: HudBox,
    /// Screen size this record (and its layout record) was saved for.
    pub screen: [i32; 2],
    /// 1 = digital, 2 = analog dial (0x30).
    pub speedometer: i32,
    /// 1 = digital, 2 = analog dial (0x38).
    pub tachometer: i32,
    /// 1 = car picture, 2 or 3 = text rows (0x3C).
    pub damage: i32,
    /// 0 = off, 1..=3 = map styles (0x40).
    pub map: i32,
    /// Map zoom step (0x44).
    pub map_zoom: i32,
    /// Opponent list on (0x48).
    pub opponent_list: i32,
    /// Multi-purpose time box shown (0x4C). What it shows (record / best / last lap) is the
    /// separate "split times" option, not stored here.
    pub multi_time_on: i32,
    /// Value text colour: 0 = amber, 1 = red, 2 = green (0x50).
    pub display_colours: i32,
    /// Rear-view mirror on (0x5C).
    pub rearview: i32,
    /// Gear readout on (0x64).
    pub gear: i32,
    /// All 14 words from 0x30, for the ones not named above.
    pub raw: [i32; 14],
}

impl HudSwitches {
    /// Colour of value text (times, gear, speed) for `display_colours`, as 0xAARRGGBB.
    pub fn value_colour(&self) -> Argb {
        match self.display_colours {
            1 => Argb(0xCCFF_0606),
            2 => Argb(0xCC1E_E605),
            _ => Argb(0xCCFA_B400),
        }
    }
}

fn i32le(d: &[u8], at: usize) -> i32 {
    i32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn f32le(d: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn cstr(d: &[u8]) -> String {
    let end = d.iter().position(|&b| b == 0).unwrap_or(d.len());
    d[..end].iter().map(|&b| b as char).collect()
}

/// Parses every whole record of a `...1.hud` file (a trailing partial record is ignored).
pub fn parse_layouts(data: &[u8]) -> Vec<HudLayout> {
    data.chunks_exact(LAYOUT_RECORD)
        .map(|r| {
            let colour = |at: usize| Argb(i32le(r, at) as u32);
            let boxes = Slot::ALL.map(|s| {
                let at = s.size_offset();
                let pos = 0x188 + s.index() * 8;
                HudBox { width: i32le(r, at), height: i32le(r, at + 4), x: f32le(r, pos), y: f32le(r, pos + 4) }
            });
            HudLayout {
                name: cstr(&r[0..0x14]),
                background: colour(0x14),
                label: colour(0x18),
                text: colour(0x1C),
                extra_colours: [colour(0x20), colour(0x24), colour(0x28), colour(0x2C), colour(0x30)],
                pad: [i32le(r, 0x34), i32le(r, 0x38)],
                fonts: [cstr(&r[0x4C..0x64]), cstr(&r[0x64..0x7C]), cstr(&r[0x7C..0x94])],
                boxes,
            }
        })
        .collect()
}

/// Parses every whole record of a `...2.hud` file.
pub fn parse_switches(data: &[u8]) -> Vec<HudSwitches> {
    data.chunks_exact(SWITCH_RECORD)
        .map(|r| {
            let b = |at: usize| HudBox { width: i32le(r, at), height: i32le(r, at + 4), x: f32le(r, at + 12), y: f32le(r, at + 16) };
            let raw: [i32; 14] = std::array::from_fn(|i| i32le(r, 0x30 + i * 4));
            HudSwitches {
                multi_time: b(0),
                tach_digital: b(0x14),
                screen: [i32le(r, 0x28), i32le(r, 0x2C)],
                speedometer: raw[0],
                tachometer: raw[2],
                damage: raw[3],
                map: raw[4],
                map_zoom: raw[5],
                opponent_list: raw[6],
                multi_time_on: raw[7],
                display_colours: raw[8],
                rearview: raw[11],
                gear: raw[13],
                raw,
            }
        })
        .collect()
}

/// Picks the record for layout `name` saved for `screen`, as the game does: the first record pair
/// whose name and screen size both match. `None` if there is none (the game then makes one from
/// the 640x480 record).
pub fn find(layouts: &[HudLayout], switches: &[HudSwitches], name: &str, screen: [i32; 2]) -> Option<usize> {
    (0..layouts.len().min(switches.len())).find(|&i| layouts[i].name == name && switches[i].screen == screen)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout_record() -> Vec<u8> {
        let mut r = vec![0u8; LAYOUT_RECORD];
        r[..8].copy_from_slice(b"Config 1");
        r[0x14..0x18].copy_from_slice(&0x8000_0080u32.to_le_bytes());
        r[0x18..0x1C].copy_from_slice(&[0xB4, 0xB4, 0xB4, 0xFF]);
        r[0x34..0x38].copy_from_slice(&5i32.to_le_bytes());
        r[0x38..0x3C].copy_from_slice(&2i32.to_le_bytes());
        r[0x4C..0x4C + 15].copy_from_slice(b"UniversC12b.ffn");
        r[0x64..0x64 + 13].copy_from_slice(b"unviers9b.ffn");
        // Tach: 128 x 128 at (0.8, 0.5)
        r[0xC4..0xC8].copy_from_slice(&128i32.to_le_bytes());
        r[0xC8..0xCC].copy_from_slice(&128i32.to_le_bytes());
        r[0x1B8..0x1BC].copy_from_slice(&0.8f32.to_le_bytes());
        r[0x1BC..0x1C0].copy_from_slice(&0.5f32.to_le_bytes());
        r
    }

    #[test]
    fn reads_layout_fields() {
        let mut data = layout_record();
        data.extend_from_slice(&[1, 2, 3]); // partial trailing record
        let all = parse_layouts(&data);
        assert_eq!(all.len(), 1);
        let l = &all[0];
        assert_eq!(l.name, "Config 1");
        assert_eq!(l.background.alpha(), 0x80);
        assert_eq!(l.label.to_f32()[0], 0xB4 as f32 / 255.0);
        assert_eq!(l.pad, [5, 2]);
        assert_eq!(l.fonts[0], "UniversC12b.ffn");
        assert_eq!(l.fonts[1], "unviers9b.ffn");
        assert_eq!(l.fonts[2], "");
        assert_eq!(l.slot(Slot::Tach), HudBox { width: 128, height: 128, x: 0.8, y: 0.5 });
        assert_eq!(l.slot(Slot::Gear).width, 0);
    }

    #[test]
    fn reads_switches_and_finds_by_name_and_screen() {
        let mut r = vec![0u8; SWITCH_RECORD];
        r[0x28..0x2C].copy_from_slice(&640i32.to_le_bytes());
        r[0x2C..0x30].copy_from_slice(&480i32.to_le_bytes());
        r[0x30..0x34].copy_from_slice(&2i32.to_le_bytes());
        r[0x50..0x54].copy_from_slice(&1i32.to_le_bytes());
        r[0x64..0x68].copy_from_slice(&1i32.to_le_bytes());
        let s = parse_switches(&r);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].screen, [640, 480]);
        assert_eq!(s[0].speedometer, 2);
        assert_eq!(s[0].gear, 1);
        assert_eq!(s[0].value_colour(), Argb(0xCCFF_0606));
        let layouts = parse_layouts(&layout_record());
        assert_eq!(find(&layouts, &s, "Config 1", [640, 480]), Some(0));
        assert_eq!(find(&layouts, &s, "Config 1", [1440, 900]), None);
        assert_eq!(find(&layouts, &s, "Config 2", [640, 480]), None);
    }

    #[test]
    #[ignore = "reads the player's install; run with NFSPU_GAME_DIR set"]
    fn reads_the_installed_layouts() {
        let dir = std::path::PathBuf::from(std::env::var("NFSPU_GAME_DIR").expect("NFSPU_GAME_DIR")).join("FEData").join("Data");
        let layouts = parse_layouts(&std::fs::read(dir.join("nfs5-1.hud")).unwrap());
        let switches = parse_switches(&std::fs::read(dir.join("nfs5-2.hud")).unwrap());
        assert_eq!((layouts.len(), switches.len()), (5, 5));
        let i = find(&layouts, &switches, "Config 1", [640, 480]).unwrap();
        let tach = layouts[i].slot(Slot::Tach);
        assert_eq!((tach.width, tach.height, (tach.x * 640.0) as i32), (128, 128, 512));
        assert_eq!(layouts[i].fonts[0], "UniversC12b.ffn");
        assert_eq!((switches[i].speedometer, switches[i].tachometer, switches[i].gear), (2, 2, 1));
        assert_eq!(find(&layouts, &switches, "Config 2", [1600, 1200]), Some(4));
    }
}
