//! The front end's own data: screen layouts (`FEData/Layouts/*.lay`), control templates
//! (`FEData/Controls/*.ctr`) and the locale string table (`FEData/Locale/festrings.csv`).
//!
//! All three are original-text formats:
//!
//! * `.lay` - one screen per file, a list of `[section.N]` blocks. Each block is a control placed
//!   on the screen: `id` names a control template, `name` gives a locale symbol, and `pos`/`size`
//!   place it. The first block of a screen is the screen itself (`id` like `REAL.BLACK_640x480`).
//! * `.ctr` - one control *type* per file: its id, a human description, the parts it draws and the
//!   states it has. Screens reference these by id.
//! * `festrings.csv` - the string table: a numeric id, the text in seven languages, a symbol name
//!   (`DescID`) and a note about which screens use it.
//!
//! Nothing here draws anything; it is the model a menu system is built on. Verified against every
//! shipped file by `examples/fe_audit.rs`.

use std::collections::HashMap;

/// One `[section.N]` block: an ordered list of `key = value` pairs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Block {
    /// The name in brackets (`section.3`, `part.stencilshape1`, `control`).
    pub name: String,
    /// The number in `[section.N]`, when the block is numbered.
    pub index: Option<u32>,
    pub entries: Vec<(String, String)>,
}

impl Block {
    /// Reads a key, ignoring case. Keys are matched as written in the file.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str())
    }

    /// The control template id of this block (`id=`), e.g. `NFS5.GOTOBUTTON`.
    pub fn control(&self) -> Option<&str> {
        self.get("id").filter(|v| !v.is_empty())
    }

    /// The locale symbol this block shows (`name=`), e.g. `ID_QUICKRACE`.
    pub fn symbol(&self) -> Option<&str> {
        self.get("name").filter(|v| !v.is_empty())
    }

    /// The numeric locale string id (`stringID=`), when the block has one.
    pub fn string_id(&self) -> Option<u32> {
        self.get("stringID")?.trim().parse().ok()
    }

    /// A `x,y` pair such as `pos=101,71` or `size=640,480`.
    pub fn pair(&self, key: &str) -> Option<(i32, i32)> {
        let (a, b) = self.get(key)?.split_once(',')?;
        Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
    }

    /// True when this block is the screen's own header (`[section.0]`).
    ///
    /// Only the *first* block is the screen. The `REAL.` prefix means "engine-built-in control",
    /// not "screen": `REAL.LABEL` and `REAL.HOVER_PICTURE` are ordinary controls that appear after
    /// the header, so a prefix test would wrongly drop them.
    pub fn is_screen_header(&self) -> bool {
        self.index == Some(0)
    }
}

/// A parsed `.lay` file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lay {
    pub blocks: Vec<Block>,
}

impl Lay {
    pub fn parse(text: &str) -> Lay {
        Lay { blocks: parse_blocks(text) }
    }

    /// The index of the screen header: `[section.0]`, or the first block when a file is not
    /// numbered. Everything else in the file is a control.
    fn header_index(&self) -> Option<usize> {
        if self.blocks.is_empty() {
            return None;
        }
        Some(self.blocks.iter().position(|b| b.index == Some(0)).unwrap_or(0))
    }

    /// The screen header block, if the file has one.
    pub fn screen(&self) -> Option<&Block> {
        self.blocks.get(self.header_index()?)
    }

    /// A human name for the screen: the header's locale symbol, else the file's first control id.
    pub fn title_symbol(&self) -> Option<&str> {
        self.screen().and_then(|b| b.get("screen")).or_else(|| self.screen().and_then(|b| b.symbol()))
    }

    /// Every control block (everything but the screen header).
    pub fn controls(&self) -> impl Iterator<Item = &Block> {
        let header = self.header_index();
        self.blocks
            .iter()
            .enumerate()
            .filter_map(move |(i, b)| (Some(i) != header && b.control().is_some()).then_some(b))
    }

    /// The distinct control template ids this screen uses, in first-seen order.
    pub fn control_ids(&self) -> Vec<&str> {
        let mut seen = Vec::new();
        for id in self.controls().filter_map(|b| b.control()) {
            if !seen.contains(&id) {
                seen.push(id);
            }
        }
        seen
    }
}

/// A parsed `.ctr` control template.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ControlTemplate {
    /// `NFS5.BOTTOMBARBUTTON`.
    pub id: String,
    pub description: String,
    /// The drawable parts, from `parts=`.
    pub parts: Vec<String>,
    /// The states a control of this type can be in, from `states=`.
    pub states: Vec<String>,
    pub blocks: Vec<Block>,
}

impl ControlTemplate {
    /// Reads the `[control]` block. Returns `None` when there is no id.
    pub fn parse(text: &str) -> Option<ControlTemplate> {
        let blocks = parse_blocks(text);
        let info = blocks.iter().find(|b| b.get("id").is_some() && b.get("description").is_some())?;
        let id = info.get("id")?.trim().to_string();
        if id.is_empty() {
            return None;
        }
        let list = |key: &str| -> Vec<String> {
            info.get(key)
                .map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
                .unwrap_or_default()
        };
        Some(ControlTemplate {
            description: info.get("description").unwrap_or("").trim().to_string(),
            parts: list("parts"),
            states: list("states"),
            id,
            blocks,
        })
    }
}

/// One row of the locale table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaleString {
    pub id: u32,
    /// The `DescID` symbol, e.g. `IDS_QUICKRACE`.
    pub symbol: String,
    /// Text per language, indexed the same way as the game's language ids (0 English .. 6 Portuguese).
    pub text: Vec<String>,
    /// The `Comments` column, which names the screens that use the string.
    pub note: String,
}

/// The whole locale string table.
#[derive(Debug, Clone, Default)]
pub struct Locale {
    pub strings: Vec<LocaleString>,
    by_id: HashMap<u32, usize>,
    by_symbol: HashMap<String, usize>,
}

/// Decodes bytes one-to-one as Latin-1. The shipped front-end text files are ISO-8859-1, not
/// UTF-8 (the locale table contains accented text like `défi`), so they must not be read as UTF-8.
pub fn decode_latin1(data: &[u8]) -> String {
    data.iter().map(|&b| b as char).collect()
}

impl Locale {
    /// Reads the table from raw bytes, decoding the file's Latin-1 text.
    pub fn parse_csv_bytes(data: &[u8]) -> Locale {
        Locale::parse_csv(&decode_latin1(data))
    }

    /// Parses `festrings.csv`. The header row decides which column is the id, which seven are the
    /// languages and where the symbol and comment columns sit, so a reordered export still loads.
    pub fn parse_csv(text: &str) -> Locale {
        let rows = parse_csv(text);
        let mut out = Locale::default();
        let Some(header) = rows.first() else { return out };
        let column = |name: &str| header.iter().position(|h| h.eq_ignore_ascii_case(name));
        let id_col = column("String ID").unwrap_or(0);
        let symbol_col = column("DescID");
        // The shipped file's header names one column "Comments", but the tags that matter (like
        // `!modeselectionbox`, naming the screens a string belongs to) sit one column further along,
        // under the header's "OrigFile". Read Comments, then fall back to the next column, so both
        // this file and a tidied export work.
        let note_cols: Vec<usize> = match column("Comments") {
            Some(c) => vec![c, c + 1],
            None => Vec::new(),
        };
        // The language columns are the seven between the id and the symbol column, in file order.
        let first = id_col + 1;
        let last = symbol_col.unwrap_or(header.len()).min(header.len());
        let language_cols: Vec<usize> = (first..last).collect();

        for row in rows.iter().skip(1) {
            let Some(id) = row.get(id_col).and_then(|v| v.trim().parse::<u32>().ok()) else { continue };
            let text: Vec<String> = language_cols.iter().map(|c| row.get(*c).cloned().unwrap_or_default()).collect();
            let symbol = symbol_col.and_then(|c| row.get(c)).cloned().unwrap_or_default().trim().to_string();
            let note = note_cols
                .iter()
                .find_map(|c| row.get(*c).map(|v| v.trim()).filter(|v| !v.is_empty()))
                .unwrap_or_default()
                .to_string();
            out.insert(LocaleString { id, symbol, text, note });
        }
        out
    }

    fn insert(&mut self, s: LocaleString) {
        self.by_id.insert(s.id, self.strings.len());
        if !s.symbol.is_empty() {
            self.by_symbol.insert(s.symbol.clone(), self.strings.len());
        }
        self.strings.push(s);
    }

    pub fn len(&self) -> usize {
        self.strings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }

    /// The text of a string id in a language (0 English .. 6 Portuguese).
    pub fn text(&self, id: u32, language: u8) -> Option<&str> {
        let s = self.strings.get(*self.by_id.get(&id)?)?;
        let t = s.text.get(language as usize)?;
        if t.is_empty() {
            s.text.first().map(String::as_str)
        } else {
            Some(t)
        }
    }

    /// The text of a symbol such as `IDS_QUICKRACE` in a language.
    pub fn text_by_symbol(&self, symbol: &str, language: u8) -> Option<&str> {
        let id = self.strings.get(*self.by_symbol.get(symbol)?)?.id;
        self.text(id, language)
    }

    /// Looks a string up by id.
    pub fn get(&self, id: u32) -> Option<&LocaleString> {
        self.strings.get(*self.by_id.get(&id)?)
    }
}

/// Reads `const int NAME = value;` lines, which the shipped `hudstrings.h` / `festrings.h` use for
/// both the language ids and the string symbols.
pub fn parse_symbol_constants(text: &str) -> Vec<(String, i64)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("const int") else { continue };
        let Some((name, value)) = rest.split_once('=') else { continue };
        let name = name.trim();
        let value = value.trim().trim_end_matches(';').trim();
        if name.is_empty() || name.contains(' ') {
            continue;
        }
        if let Ok(v) = value.parse::<i64>() {
            out.push((name.to_string(), v));
        }
    }
    out
}

/// Splits a `key = value` text into `[section]` blocks. Comments start with `;` or `#`.
fn parse_blocks(text: &str) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut current: Option<Block> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            if let Some(b) = current.take() {
                blocks.push(b);
            }
            let index = name.trim().rsplit('.').next().and_then(|n| n.parse::<u32>().ok());
            current = Some(Block { name: name.trim().to_string(), index, entries: Vec::new() });
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let block = current.get_or_insert_with(Block::default);
            block.entries.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    if let Some(b) = current {
        blocks.push(b);
    }
    blocks
}

/// A small RFC 4180 reader: quoted fields, doubled quotes, and quoted newlines all work.
fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' => quoted = true,
            ',' => row.push(std::mem::take(&mut field)),
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            _ => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows.retain(|r| r.iter().any(|f| !f.trim().is_empty()));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAY: &str = "\
[section.0]
id=REAL.BLACK_640x480
name=ID_SCREEN
size=640,480
screen=ID_SCREEN

[section.1]
id=NFS5.GOTOBUTTON
name=ID_MERGEDLISTCARSETUP
pos=101,71
stringID=14707
destination=garage

[section.2]
; a comment
id=NFS5.BOTTOMBARBUTTON
name=ID_QUICKRACE
pos=4,450
";

    #[test]
    fn parses_a_layout_into_blocks() {
        let lay = Lay::parse(LAY);
        assert_eq!(lay.blocks.len(), 3);
        assert_eq!(lay.blocks[1].index, Some(1));
        assert_eq!(lay.screen().unwrap().control(), Some("REAL.BLACK_640x480"));
        assert_eq!(lay.title_symbol(), Some("ID_SCREEN"));
        assert_eq!(lay.screen().unwrap().pair("size"), Some((640, 480)));
    }

    #[test]
    fn lists_controls_and_their_templates() {
        let lay = Lay::parse(LAY);
        let controls: Vec<&str> = lay.controls().map(|b| b.control().unwrap()).collect();
        assert_eq!(controls, vec!["NFS5.GOTOBUTTON", "NFS5.BOTTOMBARBUTTON"]);
        assert_eq!(lay.control_ids(), vec!["NFS5.GOTOBUTTON", "NFS5.BOTTOMBARBUTTON"]);
        assert_eq!(lay.controls().next().unwrap().pair("pos"), Some((101, 71)));
        assert_eq!(lay.controls().next().unwrap().string_id(), Some(14707));
    }

    #[test]
    fn a_real_prefix_control_after_the_header_is_not_a_screen() {
        let text = "\
[section.0]
id=REAL.BLACK_640x480
size=640,480

[section.1]
id=REAL.LABEL
pos=154,175
stringID=15706
";
        let lay = Lay::parse(text);
        assert_eq!(lay.screen().unwrap().control(), Some("REAL.BLACK_640x480"));
        let controls: Vec<&str> = lay.controls().map(|b| b.control().unwrap()).collect();
        assert_eq!(controls, vec!["REAL.LABEL"], "REAL. means built-in control, not screen");
    }

    #[test]
    fn a_file_whose_first_section_is_not_zero_still_finds_a_screen() {
        // The shipped layouts always start at `[section.0]`; this covers a renumbered file.
        let text = "[section.1]\nid=REAL.BLACK_640x480\nsize=640,480\n\n[section.2]\nid=REAL.LABEL\npos=1,2\n";
        let lay = Lay::parse(text);
        assert_eq!(lay.screen().unwrap().control(), Some("REAL.BLACK_640x480"));
        assert_eq!(lay.controls().count(), 1);
    }

    const CTR: &str = "\
;;;;;;;;; a banner comment ;;;;;;;;;

[control]
id=NFS5.BOTTOMBARBUTTON
description=Bottom Bar Button

parts=label,bg,data,hoverlabel,disabledlabel,picture
states=normal,hidden,hover,disabled
size.value=(label.size.width+picture.size.width+data.space),data.height
size.flags=HIDDEN|READONLY
";

    #[test]
    fn parses_a_control_template() {
        let t = ControlTemplate::parse(CTR).unwrap();
        assert_eq!(t.id, "NFS5.BOTTOMBARBUTTON");
        assert_eq!(t.description, "Bottom Bar Button");
        assert_eq!(t.parts.len(), 6);
        assert_eq!(t.states, vec!["normal", "hidden", "hover", "disabled"]);
        assert_eq!(t.blocks[0].get("size.flags"), Some("HIDDEN|READONLY"));
    }

    #[test]
    fn a_template_without_an_id_is_rejected() {
        assert!(ControlTemplate::parse("[control]\ndescription=Nothing\n").is_none());
    }

    const CSV: &str = "\
String ID,ENGLISH_IDS,FRENCH_IDS,GERMAN_IDS,ITALIAN_IDS,SPANISH_IDS,SWEDISH_IDS,PORTUGUESE_ID,DescID,Conditionals,Comments,OrigFile,,,,
0,\"Hello, World!\",,,,,,,IDS_HelloWorld,fe|needh,,!None,DO NOT TRANSLATE,,, 
2,\"\"\"This string is in quotes\"\"\",,,,,,,IDS_Quotes,fe|needh,,!None,,,,
30,knockout,défi Porsche,Knockout,knockout,a muerte,utslagstävling,eliminatória,IDS_KNOCKOUT,fe|needh,,!modeselectionbox,,,,
31,quick race,course immédiate,Schnelles Rennen,gara veloce,carrera rápida,snabblopp,corrida rápida,IDS_QUICKRACE,fe|needh,,!modeselectionbox,,,,
";

    #[test]
    fn reads_the_locale_table() {
        let l = Locale::parse_csv(CSV);
        assert_eq!(l.len(), 4);
        assert_eq!(l.text(0, 0), Some("Hello, World!"));
        assert_eq!(l.text(2, 0), Some("\"This string is in quotes\""));
        assert_eq!(l.text(31, 0), Some("quick race"));
        assert_eq!(l.text(31, 2), Some("Schnelles Rennen"));
        assert_eq!(l.text(31, 6), Some("corrida rápida"));
        assert_eq!(l.text_by_symbol("IDS_QUICKRACE", 1), Some("course immédiate"));
        // In the shipped file the tag sits one column past the header's "Comments"; the fixture
        // matches that, so this exercises the fallback.
        assert_eq!(l.get(30).unwrap().note, "!modeselectionbox");
        assert_eq!(l.get(30).unwrap().symbol, "IDS_KNOCKOUT");
    }

    #[test]
    fn a_note_in_the_comments_column_is_read_directly() {
        let csv = "String ID,ENGLISH_IDS,FRENCH_IDS,GERMAN_IDS,ITALIAN_IDS,SPANISH_IDS,SWEDISH_IDS,PORTUGUESE_ID,DescID,Conditionals,Comments,OrigFile\n\
5,Five,,,,,,,IDS_FIVE,,!somewhere,\n";
        let l = Locale::parse_csv(csv);
        assert_eq!(l.get(5).unwrap().note, "!somewhere");
        assert_eq!(l.text(5, 0), Some("Five"));
    }

    #[test]
    fn an_empty_language_falls_back_to_english() {
        let l = Locale::parse_csv(CSV);
        assert_eq!(l.text(0, 3), Some("Hello, World!"), "no Italian text, so English is used");
        assert_eq!(l.text(999, 0), None);
    }

    #[test]
    fn quoted_commas_and_newlines_survive() {
        let rows = parse_csv("a,b\n\"x,\ny\",2\n");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1], vec!["x,\ny", "2"]);
        assert_eq!(parse_csv("a,\"b\"\"c\"\n")[0], vec!["a", "b\"c"]);
    }

    #[test]
    fn latin1_text_is_decoded_not_rejected() {
        // `défi` in ISO-8859-1, which is not valid UTF-8.
        let bytes: &[u8] = b"String ID,ENGLISH_IDS,FRENCH_IDS,GERMAN_IDS,ITALIAN_IDS,SPANISH_IDS,SWEDISH_IDS,PORTUGUESE_ID,DescID,Conditionals,Comments,OrigFile\n\
30,knockout,d\xe9fi Porsche,Knockout,knockout,a muerte,utslagst\xe4vling,eliminat\xf3ria,IDS_KNOCKOUT,fe|needh,,!modeselectionbox,\n";
        let l = Locale::parse_csv_bytes(bytes);
        assert_eq!(l.len(), 1);
        assert_eq!(l.text(30, 2), Some("Knockout"));
        assert_eq!(decode_latin1(&[0xE9, 0xE4, 0xF3]), "éäó");
    }

    #[test]
    fn reads_symbol_constants() {
        let text = "\
const int ENGLISH_IDS = 0;
const int PORTUGUESE_ID = 6;
const int LOCALE_LANGUAGE_COUNT = 7;
const int IDS_KNOCKOUT = 30;
not a constant
";
        let c = parse_symbol_constants(text);
        assert_eq!(c[0], ("ENGLISH_IDS".into(), 0));
        assert_eq!(c[1], ("PORTUGUESE_ID".into(), 6));
        assert_eq!(c.last().unwrap(), &("IDS_KNOCKOUT".into(), 30));
    }
}
