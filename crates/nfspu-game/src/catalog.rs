//! The front end's catalogue tables (`FEData/Data`): cars (`nfs5.car`), tracks (`nfs5.trk`) and
//! tournaments (`nfs5.trn`), plus each car model's paint list (`GameData/CarModel/<model>.clr`), and
//! the lists the quick-race screen builds from them.
//!
//! Field offsets were worked out from the shipped files and checked against what the retail
//! executable reads (described in `docs/provenance/fe.md`, "Quick race"); nothing here is taken
//! from its code. All readers are read-only.

use std::path::Path;

/// One car-catalogue record: `nfs5.car` is a plain array of 0x670-byte records.
pub const CAR_RECORD: usize = 0x670;
/// One track record: `nfs5.trk` is a plain array of 0x4a8-byte records.
pub const TRACK_RECORD: usize = 0x4a8;
/// One tournament record in `nfs5.trn`.
pub const TOURNAMENT_RECORD: usize = 0xd40;

/// A car from `nfs5.car`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Car {
    /// Catalogue id (`u16` at 0).
    pub id: u16,
    /// Full name with the year prefix, `'50 356 1100 Cabriolet` (at 2).
    pub name: String,
    /// The short name the closed dropdown shows, `356` or `356 B` (at 0x34).
    pub short: String,
    /// Model file stem in `GameData/CarModel`, `356_1` (at 0x7f).
    pub model: String,
    /// Logo archive in `FEData/Art`, `finallogos2` (at 0x147).
    pub logo_file: String,
    /// Logo entry in that archive (`u16` at 0x1f6).
    pub logo_index: u16,
    /// Class string id at 0x1ac: 501 "CLASS 3", 502 "CLASS 2", 503 "CLASS 1".
    pub class: u32,
    /// Era string id at 0x1b0: 521 classic, 522 golden, 523 modern.
    pub era: u32,
    /// 0 a road car, 1 traffic, 2 police (`u16` at 0x1f4).
    pub kind: u16,
    /// First and last model year (`u16` at 0x1f8 and 0x202).
    pub year_from: i16,
    pub year_to: i16,
    /// 0 or 2 available, 1 hidden, 3 never in the lists (`u16` at 0x204; the three factory-driver
    /// prize cars use 3).
    pub availability: u16,
}

fn text(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    nfspu_formats::fe::decode_latin1(&bytes[..end])
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    b.get(at..at + 2).map(|s| u16::from_le_bytes([s[0], s[1]])).unwrap_or(0)
}

fn i16_at(b: &[u8], at: usize) -> i16 {
    u16_at(b, at) as i16
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    b.get(at..at + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]])).unwrap_or(0)
}

impl Car {
    fn parse(r: &[u8]) -> Car {
        Car {
            id: u16_at(r, 0),
            name: text(&r[2..0x34]),
            short: text(&r[0x34..0x4d]),
            model: text(&r[0x7f..0xb1]),
            logo_file: text(&r[0x147..0x179]),
            logo_index: u16_at(r, 0x1f6),
            class: u32_at(r, 0x1ac),
            era: u32_at(r, 0x1b0),
            kind: u16_at(r, 0x1f4),
            year_from: i16_at(r, 0x1f8),
            year_to: i16_at(r, 0x202),
            availability: u16_at(r, 0x204),
        }
    }

    /// The list row: the two-digit year (`'50`) and the rest of the name. The retail list draws the
    /// first three characters in their own column and the name from the fifth.
    pub fn year_and_model(&self) -> (String, String) {
        let chars: Vec<char> = self.name.chars().collect();
        if chars.first() == Some(&'\'') && chars.len() > 4 {
            (chars[..3].iter().collect(), chars[4..].iter().collect())
        } else {
            (String::new(), self.name.clone())
        }
    }
}

/// Reads every record of `nfs5.car`.
pub fn parse_cars(data: &[u8]) -> Vec<Car> {
    data.chunks_exact(CAR_RECORD).map(Car::parse).collect()
}

/// Sorts a car list the way the front end orders its lists: by era, then class, then first model
/// year, with the executable's simple exchange sort (each slot in turn is swapped with any later
/// car that sorts before it), so cars with equal keys keep the order that sort produces.
pub fn sort_cars(cars: &[Car], list: &mut [usize]) {
    let key = |i: usize| (cars[i].era, cars[i].class, cars[i].year_from);
    for i in 0..list.len() {
        for j in i + 1..list.len() {
            if key(list[j]) < key(list[i]) {
                list.swap(i, j);
            }
        }
    }
}

/// The "factory stock" cars a quick race offers (outside career mode): road cars whose
/// availability is 0 or 2, sorted. Indices into `cars`.
pub fn quick_race_cars(cars: &[Car]) -> Vec<usize> {
    let mut list: Vec<usize> = (0..cars.len()).filter(|&i| cars[i].kind == 0 && matches!(cars[i].availability, 0 | 2)).collect();
    sort_cars(cars, &mut list);
    list
}

/// The cars an opponent can be given explicitly: every road car whose model file is installed,
/// sorted (the retail list also has the prize cars with availability 3).
pub fn opponent_cars(cars: &[Car], has_model: impl Fn(&str) -> bool) -> Vec<usize> {
    let mut list: Vec<usize> = (0..cars.len()).filter(|&i| cars[i].kind == 0 && has_model(&cars[i].model)).collect();
    sort_cars(cars, &mut list);
    list
}

/// The cars a player owns, found in their save. The `PlayerInfo` section holds a copy of each
/// owned car's catalogue record; each copy starts with the car's id followed by its full name.
/// Finding them by that pattern is a **guess** at the section's layout (it finds Greg's one car).
pub fn owned_cars(save: &[u8], cars: &[Car]) -> Vec<usize> {
    let find = |name: &[u8], from: usize| save.get(from..).and_then(|s| s.windows(name.len()).position(|w| w == name)).map(|p| p + from);
    let Some(start) = find(b"PlayerInfo\0", 0) else { return Vec::new() };
    let end = find(b"TrackRecords\0", start).unwrap_or(save.len());
    let section = &save[start..end];
    let mut found: Vec<(usize, usize)> = Vec::new();
    for (i, car) in cars.iter().enumerate() {
        let mut pattern = car.id.to_le_bytes().to_vec();
        pattern.extend(car.name.chars().map(|c| c as u32 as u8));
        pattern.push(0);
        let mut from = 0;
        while let Some(p) = section.get(from..).and_then(|s| s.windows(pattern.len()).position(|w| w == pattern.as_slice())) {
            found.push((from + p, i));
            from += p + 1;
        }
    }
    found.sort();
    found.into_iter().map(|(_, i)| i).collect()
}

/// A track from `nfs5.trk`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Track {
    /// Track id (`i16` at 0), the number tournaments list.
    pub id: i16,
    /// Locale string of the full name (`u32` at 4); the short name is this + 40.
    pub name_string: u32,
    /// File stem in `GameData/Track` (at 8).
    pub file: String,
    /// `FEData/TrackArt` pictures (at 0x30, 0x44, 0x58): main, `Sp` and `SpSm`.
    pub art: [String; 3],
    /// 3: only in quick race once a tournament that uses it is open (`i16` at 0x6c).
    pub locked: i16,
    /// 0 beginner, 1 advanced, 2 expert (`i16` at 0x78; shown as strings 450-452).
    pub difficulty: i16,
}

/// Reads every record of `nfs5.trk` (the copy in `FEData/Data`; the one in the save folder holds
/// the same records with the player's best times).
pub fn parse_tracks(data: &[u8]) -> Vec<Track> {
    data.chunks_exact(TRACK_RECORD)
        .map(|r| Track {
            id: i16_at(r, 0),
            name_string: u32_at(r, 4),
            file: text(&r[8..0x30]),
            art: [text(&r[0x30..0x44]), text(&r[0x44..0x58]), text(&r[0x58..0x6c])],
            locked: i16_at(r, 0x6c),
            difficulty: i16_at(r, 0x78),
        })
        .collect()
}

/// A tournament: the era code that opens it and the tracks it uses.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tournament {
    /// 521, 522, 523 open in 1970, 1989, 2000 (career year); other codes are open from the start.
    pub era: u32,
    pub tracks: Vec<i16>,
}

/// Reads `nfs5.trn`: era code at 4, track count at 10, track ids from 12.
pub fn parse_tournaments(data: &[u8]) -> Vec<Tournament> {
    data.chunks_exact(TOURNAMENT_RECORD)
        .map(|r| {
            let count = i16_at(r, 10).clamp(0, 32) as usize;
            Tournament { era: u32_at(r, 4), tracks: (0..count).map(|i| i16_at(r, 12 + i * 2)).collect() }
        })
        .collect()
}

/// The career year a tournament's era code needs (0: open from the start).
pub fn era_year(era: u32) -> i32 {
    match era {
        521 => 1970,
        522 => 1989,
        523 => 2000,
        _ => 0,
    }
}

/// The tracks a quick race offers, in file order: every track whose `.crp` is installed, except
/// the skid pad, and a `locked` track only when an open tournament uses it. Indices into `tracks`.
pub fn quick_race_tracks(tracks: &[Track], tournaments: &[Tournament], career_year: i32, has_track: impl Fn(&str) -> bool) -> Vec<usize> {
    (0..tracks.len())
        .filter(|&i| {
            let t = &tracks[i];
            let open = t.locked != 3 || tournaments.iter().any(|tour| era_year(tour.era) <= career_year && tour.tracks.contains(&t.id));
            open && has_track(&t.file) && !t.file.eq_ignore_ascii_case("skidpad")
        })
        .collect()
}

/// The best time stored for a track in the save folder's `nfs5.trk`: the first entry of its
/// record table, a `u32` at 0x1f8 in 1/128 s ticks (the front end's clock rate). Which table the
/// "record time" line shows is a **guess**.
pub fn record_ticks(records: &[u8], track_id: i16) -> Option<u32> {
    records.chunks_exact(TRACK_RECORD).find(|r| i16_at(r, 0) == track_id).map(|r| u32_at(r, 0x1f8)).filter(|t| *t > 0)
}

/// `m:ss.hh` from 1/128 s ticks, capped at 59:59.99 like the retail time formatter.
pub fn format_ticks(ticks: u32) -> String {
    let hundredths = (ticks as u64 * 100 / 128).min(59 * 6000 + 5999);
    format!("{:02}:{:02}.{:02}", hundredths / 6000, hundredths / 100 % 60, hundredths % 100)
}

/// One paint scheme of a car model: `[sectionN]` of its `.clr`, `color1` being the body colour.
#[derive(Debug, Clone, PartialEq)]
pub struct Paint {
    pub body: [u8; 3],
}

/// Reads a `.clr` (plain `key=value` text, one `[sectionN]` per paint).
pub fn parse_paints(text: &str) -> Vec<Paint> {
    let mut out: Vec<Paint> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            out.push(Paint { body: [0; 3] });
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        let Some(paint) = out.last_mut() else { continue };
        let Ok(v) = v.trim().parse::<u32>() else { continue };
        match k.trim() {
            "color1.r" => paint.body[0] = v.min(255) as u8,
            "color1.g" => paint.body[1] = v.min(255) as u8,
            "color1.b" => paint.body[2] = v.min(255) as u8,
            _ => {}
        }
    }
    out
}

/// A car model's paints from `GameData/CarModel/<model>.clr` (empty when missing).
pub fn load_paints(game_dir: &Path, model: &str) -> Vec<Paint> {
    let dir = game_dir.join("GameData").join("CarModel");
    let wanted = format!("{model}.clr").to_ascii_lowercase();
    std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .find(|e| e.file_name().to_string_lossy().to_ascii_lowercase() == wanted)
        .and_then(|e| std::fs::read(e.path()).ok())
        .map(|b| parse_paints(&String::from_utf8_lossy(&b)))
        .unwrap_or_default()
}

/// The lower-cased file names in a folder, for "is this model / track installed" checks.
pub fn file_names(dir: &Path) -> std::collections::BTreeSet<String> {
    std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.file_name().to_string_lossy().to_ascii_lowercase()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn car_record(id: u16, name: &str, model: &str, era: u32, class: u32, year: i16, kind: u16, avail: u16) -> Vec<u8> {
        let mut r = vec![0u8; CAR_RECORD];
        r[0..2].copy_from_slice(&id.to_le_bytes());
        r[2..2 + name.len()].copy_from_slice(name.as_bytes());
        r[0x34..0x37].copy_from_slice(b"356");
        r[0x7f..0x7f + model.len()].copy_from_slice(model.as_bytes());
        r[0x1ac..0x1b0].copy_from_slice(&class.to_le_bytes());
        r[0x1b0..0x1b4].copy_from_slice(&era.to_le_bytes());
        r[0x1f4..0x1f6].copy_from_slice(&kind.to_le_bytes());
        r[0x1f6..0x1f8].copy_from_slice(&7u16.to_le_bytes());
        r[0x1f8..0x1fa].copy_from_slice(&year.to_le_bytes());
        r[0x204..0x206].copy_from_slice(&avail.to_le_bytes());
        r
    }

    #[test]
    fn cars_parse_and_sort_by_era_class_year() {
        let mut data = Vec::new();
        data.extend(car_record(1, "'70 Late", "a", 522, 501, 1970, 0, 0));
        data.extend(car_record(2, "'55 Early", "b", 521, 502, 1955, 0, 0));
        data.extend(car_record(3, "'50 First", "c", 521, 501, 1950, 0, 0));
        data.extend(car_record(4, "Cop", "d", 521, 501, 1950, 2, 1));
        data.extend(car_record(5, "'99 Prize", "e", 523, 501, 1999, 0, 3));
        let cars = parse_cars(&data);
        assert_eq!(cars.len(), 5);
        assert_eq!(cars[0].short, "356");
        assert_eq!(cars[0].logo_index, 7);
        assert_eq!(cars[2].year_and_model(), ("'50".to_string(), "First".to_string()));
        assert_eq!(quick_race_cars(&cars), vec![2, 1, 0], "classic class 3, classic class 2, golden; no cop, no prize car");
        assert_eq!(opponent_cars(&cars, |m| m != "c"), vec![1, 0, 4], "installed road cars, prize car included");
    }

    #[test]
    fn owned_cars_are_found_in_player_info() {
        let mut data = Vec::new();
        data.extend(car_record(76, "'50 356 1100 Cabriolet", "356_1", 521, 501, 1950, 0, 0));
        data.extend(car_record(77, "'50 356 1100 Coup", "356_1", 521, 501, 1950, 0, 0));
        let cars = parse_cars(&data);
        let mut save = b"Missions\0....PlayerInfo\0\x01\0\0\0\0\0\0\0".to_vec();
        save.extend(76u16.to_le_bytes());
        save.extend(b"'50 356 1100 Cabriolet\0rest");
        save.extend(b"TrackRecords\0");
        save.extend(77u16.to_le_bytes());
        save.extend(b"'50 356 1100 Coup\0");
        assert_eq!(owned_cars(&save, &cars), vec![0], "only inside PlayerInfo");
        assert!(owned_cars(b"nothing", &cars).is_empty());
    }

    fn track_record(id: i16, file: &str, locked: i16) -> Vec<u8> {
        let mut r = vec![0u8; TRACK_RECORD];
        r[0..2].copy_from_slice(&id.to_le_bytes());
        r[4..8].copy_from_slice(&300u32.to_le_bytes());
        r[8..8 + file.len()].copy_from_slice(file.as_bytes());
        r[0x30..0x33].copy_from_slice(b"big");
        r[0x6c..0x6e].copy_from_slice(&locked.to_le_bytes());
        r[0x78..0x7a].copy_from_slice(&2i16.to_le_bytes());
        r[0x1f8..0x1fc].copy_from_slice(&10496u32.to_le_bytes());
        r
    }

    #[test]
    fn quick_race_tracks_follow_tournaments_and_the_career_year() {
        let mut data = Vec::new();
        data.extend(track_record(8, "coastal", 0));
        data.extend(track_record(9, "Forest", 3));
        data.extend(track_record(32, "skidpad", 0));
        data.extend(track_record(11, "alps", 3));
        let tracks = parse_tracks(&data);
        assert_eq!(tracks[0].art[0], "big");
        assert_eq!(tracks[0].difficulty, 2);
        let mut trn = vec![0u8; TOURNAMENT_RECORD];
        trn[4..8].copy_from_slice(&521u32.to_le_bytes());
        trn[10..12].copy_from_slice(&1i16.to_le_bytes());
        trn[12..14].copy_from_slice(&9i16.to_le_bytes());
        let tours = parse_tournaments(&trn);
        assert_eq!(tours, vec![Tournament { era: 521, tracks: vec![9] }]);
        assert_eq!(quick_race_tracks(&tracks, &tours, 1950, |_| true), vec![0], "forest waits for 1970, skid pad never");
        assert_eq!(quick_race_tracks(&tracks, &tours, 1970, |_| true), vec![0, 1]);
        assert_eq!(quick_race_tracks(&tracks, &tours, 1970, |f| f != "Forest"), vec![0], "not installed");
        assert_eq!(record_ticks(&data, 8), Some(10496));
        assert_eq!(format_ticks(10496), "01:22.00");
        assert_eq!(format_ticks(u32::MAX), "59:59.99");
    }

    #[test]
    fn paints_read_the_body_colour_of_each_section() {
        let paints = parse_paints("[section1]\r\ncolor1.r=252\r\ncolor1.g=255\r\ncolor1.b=242\r\ncolor2.r=1\r\n\r\n[section2]\r\ncolor1.r=255\r\n");
        assert_eq!(paints, vec![Paint { body: [252, 255, 242] }, Paint { body: [255, 0, 0] }]);
    }
}
