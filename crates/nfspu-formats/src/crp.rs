//! CRP track/car containers (decompressed `.crp`, magic `karT` / `raC`).
//!
//! Layout verified on `alps.crp` (see `docs/provenance/formats.md`): a 16-byte header, then
//! 16-byte article records, each pointing to a table of 16-byte part records. A part record is
//! `tag[4]`, flag byte, 24-bit length in bytes, u32 element count, u32 offset from the record.
//! Only geometry is read here: vertices, UVs, per-corner colours and triangle index tables.

use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum CrpError {
    BadMagic,
    Truncated,
    OutOfRange(&'static str),
}

impl fmt::Display for CrpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic => write!(f, "not a CRP container (expected 'karT' or ' raC')"),
            Self::Truncated => write!(f, "CRP data ended early"),
            Self::OutOfRange(what) => write!(f, "CRP {what} points outside the file"),
        }
    }
}

impl std::error::Error for CrpError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Part {
    pub tag: [u8; 4],
    pub flag: u8,
    /// Length in bytes.
    pub length: usize,
    /// Element count (vertices, UVs, colours, or indices depending on the tag).
    pub count: usize,
    /// Absolute byte offset of the part data.
    pub offset: usize,
}

#[derive(Debug, Clone)]
pub struct Article {
    pub parts: Vec<Part>,
}

pub struct Crp<'a> {
    data: &'a [u8],
    pub articles: Vec<Article>,
    /// Miscellaneous parts (materials, descriptions, ...), listed after the article records.
    pub misc: Vec<Part>,
}

/// Which variant of the model an article belongs to (from its `Base` part).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArticleBase {
    /// Which kind of piece it is (body, roof, wheel, ...); pieces with the same index are alternatives.
    pub geometry: u8,
    /// 0 = the default; other values are optional variants (other roof, spoiler, driver models, ...).
    pub type_index: u8,
}

/// A material: which texture a triangle part is drawn with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Material {
    pub index: u16,
    /// Material kind such as "Opaque", "Normal" or "FakeSort".
    pub kind: String,
    /// Name of the texture in the matching `.fsh` file; `None` if the field holds no text.
    pub texture: Option<String>,
    /// The same field read as a number. In car files it holds no name but the texture page the
    /// material is drawn with, counted from 0 (`[tpageN.details]` in the car's `.tpg` is N - 1).
    pub page: u32,
}

/// Geometry of one article at one level of detail.
#[derive(Debug, Clone, PartialEq)]
pub struct ArticleMesh {
    pub positions: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub batches: Vec<Batch>,
    /// Primitive parts left out because of invalid indices or unsupported topology.
    pub skipped_batches: usize,
}

/// One normalized triangle list that shares a material. Source strips are expanded here. `vertex_idx`, `uv_idx` and `colors` are per corner
/// (three corners per triangle). Indices are absolute positions in the article's `positions`
/// and `uvs` arrays (the per-part window base is already added).
#[derive(Debug, Clone, PartialEq)]
pub struct Batch {
    pub material: u16,
    pub vertex_idx: Vec<u16>,
    pub uv_idx: Vec<u16>,
    /// Raw colour bytes as stored: blue, green, red, alpha (a Direct3D colour).
    pub colors: Vec<[u8; 4]>,
}

/// Source corner order for the two indexed surface topologies verified in retail.
/// Degenerate strip triangles keep their place, so parity never resets inside a strip.
fn triangle_corners(mode: u16, count: usize) -> Option<Vec<usize>> {
    match mode {
        1 => {
            let mut order = Vec::with_capacity(count.saturating_sub(2) * 3);
            for first in 0..count.saturating_sub(2) {
                if first % 2 == 0 { order.extend([first, first + 1, first + 2]); }
                else { order.extend([first + 1, first, first + 2]); }
            }
            Some(order)
        }
        3 => Some((0..count / 3 * 3).collect()),
        _ => None,
    }
}

fn u32le(d: &[u8], at: usize) -> Result<usize, CrpError> {
    let b = d.get(at..at + 4).ok_or(CrpError::Truncated)?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
}

fn f32le(d: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

pub fn parse(data: &[u8]) -> Result<Crp<'_>, CrpError> {
    match data.get(0..4) {
        Some(b"karT") | Some(b" raC") => {} // tracks ("Trak") and cars ("Car ") are stored byte-reversed
        Some(_) => return Err(CrpError::BadMagic),
        None => return Err(CrpError::Truncated),
    }
    let n = u32le(data, 4)? >> 5;
    let table = u32le(data, 12)? * 16;
    let mut articles = Vec::with_capacity(n);
    for i in 0..n {
        let rec = table + 16 * i;
        let part_count = u32le(data, rec + 8)?;
        let part_table = rec + u32le(data, rec + 12)? * 16;
        let mut parts = Vec::with_capacity(part_count);
        for j in 0..part_count {
            let p = part_table + 16 * j;
            let raw = data.get(p..p + 16).ok_or(CrpError::OutOfRange("part table"))?;
            let length = raw[5] as usize | (raw[6] as usize) << 8 | (raw[7] as usize) << 16;
            let offset = p + u32le(data, p + 12)?;
            if offset + length > data.len() {
                return Err(CrpError::OutOfRange("part data"));
            }
            parts.push(Part {
                tag: [raw[0], raw[1], raw[2], raw[3]],
                flag: raw[4],
                length,
                count: u32le(data, p + 8)?,
                offset,
            });
        }
        articles.push(Article { parts });
    }

    let misc_count = u32le(data, 8)?;
    let misc_table = 0x10 + 16 * n;
    let mut misc = Vec::with_capacity(misc_count);
    for j in 0..misc_count {
        let p = misc_table + 16 * j;
        let raw = data.get(p..p + 16).ok_or(CrpError::OutOfRange("misc table"))?;
        let length = raw[5] as usize | (raw[6] as usize) << 8 | (raw[7] as usize) << 16;
        let offset = p + u32le(data, p + 12)?;
        if offset + length > data.len() {
            return Err(CrpError::OutOfRange("misc part data"));
        }
        misc.push(Part { tag: [raw[0], raw[1], raw[2], raw[3]], flag: raw[4], length, count: u32le(data, p + 8)?, offset });
    }
    Ok(Crp { data, articles, misc })
}

fn text_field(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    bytes[..end].iter().map(|&b| if (32..127).contains(&b) { b as char } else { '?' }).collect::<String>().trim().to_string()
}

impl<'a> Crp<'a> {
    pub fn part_data(&self, part: &Part) -> &'a [u8] {
        &self.data[part.offset..part.offset + part.length]
    }

    /// Texture archives stored inside the container (misc parts tagged `fs`), with their index.
    /// Cars keep their textures this way (exterior, interior, wheels and trim, glass, ...).
    pub fn embedded_archives(&self) -> Vec<(u16, &'a [u8])> {
        self.misc
            .iter()
            .filter(|p| p.tag[2] == b'f' && p.tag[3] == b's')
            .map(|p| (u16::from_le_bytes([p.tag[0], p.tag[1]]), self.part_data(p)))
            .collect()
    }

    /// All materials in the container, in file order. Material parts are stored as
    /// `[index_lo, index_hi, 't', 'm']`; the description is at 0x10 (16 bytes) and the
    /// 4-character texture name (tracks) or texture page number (cars) at 0x28.
    pub fn materials(&self) -> Vec<Material> {
        self.misc
            .iter()
            .filter(|p| p.tag[2] == b't' && p.tag[3] == b'm')
            .filter_map(|p| {
                let d = self.data.get(p.offset..)?;
                let name = d.get(0x28..0x2C)?;
                let texture = name.iter().all(|b| (32..127).contains(b)).then(|| text_field(name)).filter(|t| !t.is_empty());
                let page = u32::from_le_bytes([name[0], name[1], name[2], name[3]]);
                Some(Material { index: u16::from_le_bytes([p.tag[0], p.tag[1]]), kind: text_field(d.get(0x10..0x20)?), texture, page })
            })
            .collect()
    }

    /// Geometry and type index of an article, read from its `Base` part (offsets 0x44 and 0x45).
    pub fn article_base(&self, article: usize) -> Option<ArticleBase> {
        let p = self.find(article, *b"esaB")?;
        let d = self.part_data(p);
        Some(ArticleBase { geometry: *d.get(0x44)?, type_index: *d.get(0x45)? })
    }

    /// The article's 4x4 transform at a level of detail (row-vector convention: the translation is
    /// the last row, so a point moves as `p * M`). `None` if the article has no transform there.
    pub fn article_transform(&self, article: usize, lod: u8) -> Option<[f32; 16]> {
        let p = self.find(article, [lod, 0, b'r', b't'])?;
        let d = self.part_data(p);
        let mut m = [0.0f32; 16];
        for (k, v) in m.iter_mut().enumerate() {
            *v = f32le(d.get(k * 4..k * 4 + 4)?, 0);
        }
        Some(m)
    }

    /// The article's name, e.g. `RD0008C (1 PRIMARY)` (road piece) or `CNK0016L (...)` (side chunk).
    pub fn article_name(&self, article: usize) -> Option<String> {
        let p = self.find(article, *b"emaN")?;
        Some(text_field(self.part_data(p)))
    }

    fn find(&self, article: usize, tag: [u8; 4]) -> Option<&Part> {
        self.articles.get(article)?.parts.iter().find(|p| p.tag == tag)
    }

    /// Per-vertex normals of an article at a level of detail (part `[lod, 0, 'm', 'n']`, 16 bytes per
    /// normal: x, y, z and an unused fourth float), one per entry of `article_mesh(..).positions`.
    /// Triangle parts place their normal window at the same start as their vertex window, so a
    /// corner's normal is `normals[vertex index]` (checked on every part of the 928, 993, 356 A, 914
    /// and eas1). `None` when the part is missing or its count differs from the vertex count.
    pub fn article_normals(&self, article: usize, lod: u8) -> Option<Vec<[f32; 3]>> {
        let nm = self.find(article, [lod, 0, b'm', b'n'])?;
        let vt = self.find(article, [lod, 0, b't', b'v'])?;
        if nm.length / 16 != vt.length / 16 {
            return None;
        }
        let d = self.part_data(nm);
        Some((0..nm.length / 16).map(|k| [f32le(d, k * 16), f32le(d, k * 16 + 4), f32le(d, k * 16 + 8)]).collect())
    }

    /// Reads the geometry of one article. Returns `None` if the article has no vertex data at
    /// that level of detail. Triangle parts without index tables (a different part kind) are skipped.
    pub fn article_mesh(&self, article: usize, lod: u8) -> Option<ArticleMesh> {
        let vt = self.find(article, [lod, 0, b't', b'v'])?;
        let uv = self.find(article, [lod, 0, b'v', b'u'])?;
        // Track articles carry one baked colour per corner (`fd`); car articles have none (white).
        let fd = self.find(article, [lod, 0, b'f', b'd']);
        let vd = self.part_data(vt);
        let ud = self.part_data(uv);
        let cd: &[u8] = fd.map_or(&[], |p| self.part_data(p));
        let positions: Vec<[f32; 3]> =
            (0..vt.length / 16).map(|k| [f32le(vd, k * 16), f32le(vd, k * 16 + 4), f32le(vd, k * 16 + 8)]).collect();
        let uvs: Vec<[f32; 2]> = (0..uv.length / 8).map(|k| [f32le(ud, k * 8), f32le(ud, k * 8 + 4)]).collect();

        // Triangle parts for this level of detail: part index in the low 12 bits, LOD in the top 4.
        let mut prs: Vec<(usize, &Part)> = self.articles[article]
            .parts
            .iter()
            .filter(|p| p.tag[2] == b'r' && p.tag[3] == b'p' && (p.tag[1] >> 4) == lod)
            .map(|p| (p.tag[0] as usize | ((p.tag[1] & 0x0F) as usize) << 8, p))
            .collect();
        prs.sort_by_key(|(index, _)| *index);

        let mut batches = Vec::new();
        let mut skipped_batches = 0usize;
        let mut corner = 0usize;
        for (_, p) in prs {
            let d = self.part_data(p);
            let count = p.count;
            let (nif, nid) = (u32le(d, 0x28).ok()?, u32le(d, 0x2C).ok()?);
            if nid < 2 {
                corner += count; // keep colour numbering aligned even when skipping
                continue;
            }
            let header = 0x30 + nif * 16 + nid * 8;
            let rows = 0x30 + nif * 16;
            // Info rows (id, start byte, length | kind << 16, ..) select windows into the article's
            // shared arrays. The row ids vary between layouts and car parts have an extra row, but the
            // kind in the high half of the third value is stable: 3 = colours (4 B each), 2 = UVs
            // (8 B), 0 = vertices (16 B). Other kinds (normals, extra records) are ignored.
            // The byte indices in the index tables are relative to those windows.
            let (mut c_base, mut u_base, mut v_base) = (corner, 0usize, 0usize);
            for k in 0..nif {
                let r = 0x30 + 16 * k;
                let (Ok(start), Ok(third)) = (u32le(d, r + 4), u32le(d, r + 8)) else { continue };
                match third >> 16 {
                    3 => c_base = start / 4,
                    2 => u_base = start / 8,
                    0 => v_base = start / 16,
                    _ => {}
                }
            }
            let table_at = |tag: &[u8; 2]| -> Option<usize> {
                (0..nid).find_map(|k| {
                    let r = rows + 8 * k;
                    (d.get(r + 2..r + 4)? == tag).then(|| u32le(d, r + 4).ok()).flatten()
                })
            };
            let (vi, ui) = (table_at(b"vI")?, table_at(b"uI")?);
            let vertex_idx: Vec<u16> = d.get(header + vi..header + vi + count)?.iter().map(|&b| (v_base + b as usize) as u16).collect();
            let uv_idx: Vec<u16> = d.get(header + ui..header + ui + count)?.iter().map(|&b| (u_base + b as usize) as u16).collect();
            if vertex_idx.iter().any(|&i| i as usize >= positions.len()) || uv_idx.iter().any(|&i| i as usize >= uvs.len()) {
                skipped_batches += 1;
                corner += count;
                continue;
            }
            let colors = if fd.is_none() {
                vec![[255, 255, 255, 255]; count]
            } else {
                (c_base..c_base + count)
                    .map(|c| cd.get(c * 4..c * 4 + 4).map(|b| [b[0], b[1], b[2], b[3]]))
                    .collect::<Option<Vec<_>>>()?
            };
            corner += count;
            let mode = u16::from_le_bytes([d[0], d[1]]);
            let Some(order) = triangle_corners(mode, count) else {
                skipped_batches += 1;
                continue;
            };
            batches.push(Batch {
                material: u16::from_le_bytes([d[4], d[5]]),
                vertex_idx: order.iter().map(|&i| vertex_idx[i]).collect(),
                uv_idx: order.iter().map(|&i| uv_idx[i]).collect(),
                colors: order.iter().map(|&i| colors[i]).collect(),
            });
        }
        Some(ArticleMesh { positions, uvs, batches, skipped_batches })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(tag: [u8; 4], len: usize, count: usize, data_at: usize, rec_at: usize) -> [u8; 16] {
        let mut r = [0u8; 16];
        r[0..4].copy_from_slice(&tag);
        r[5..8].copy_from_slice(&(len as u32).to_le_bytes()[..3]);
        r[8..12].copy_from_slice(&(count as u32).to_le_bytes());
        r[12..16].copy_from_slice(&((data_at - rec_at) as u32).to_le_bytes());
        r
    }

    /// One article with 2 junk entries in front of each array, so the part's info-row windows matter:
    /// vertices 5 (window starts at 2), UVs 4 (window starts at 1), colours 4 (window starts at 1).
    /// One triangle part with a vertex table and a UV table of 1-byte indices.
    fn tiny() -> Vec<u8> { tiny_corners(3, &[0, 1, 2]) }

    fn tiny_corners(mode: u16, indices: &[u8]) -> Vec<u8> {
        let mut d = vec![0u8; 0x10];
        d[0..4].copy_from_slice(b"karT");
        d[4..8].copy_from_slice(&((1u32 << 5) | 0x1A).to_le_bytes());
        d[12..16].copy_from_slice(&1u32.to_le_bytes()); // article table at 0x10
        let mut art = [0u8; 16];
        art[0..4].copy_from_slice(b"itrA");
        art[8..12].copy_from_slice(&4u32.to_le_bytes()); // 4 parts
        art[12..16].copy_from_slice(&1u32.to_le_bytes()); // part table at record + 16
        d.extend_from_slice(&art);

        let verts: Vec<u8> = [[9.0f32, 9.0, 9.0], [9.0, 9.0, 9.0], [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 5.0]]
            .iter()
            .flat_map(|v| v.iter().chain(&[1.0f32]).flat_map(|f| f.to_le_bytes()))
            .collect();
        let uvs: Vec<u8> = [[7.0f32, 7.0], [0.0, 0.0], [1.0, 0.0], [0.0, 1.0]].iter().flatten().flat_map(|f| f.to_le_bytes()).collect();
        let palette = [[255,0,0,255], [0,255,0,255], [0,0,255,255]];
        let mut cols = vec![1,1,1,1];
        for i in 0..indices.len() { cols.extend(palette[i % 3]); }
        // Triangle part: 0x30 header + 3 info rows (48) + 2 index rows (16) = 112 bytes, then 2 tables of 3 bytes.
        let mut pr = vec![0u8; 0x70];
        pr[..2].copy_from_slice(&mode.to_le_bytes());
        pr[4..6].copy_from_slice(&7u16.to_le_bytes()); // material 7
        pr[0x28..0x2C].copy_from_slice(&3u32.to_le_bytes());
        pr[0x2C..0x30].copy_from_slice(&2u32.to_le_bytes());
        // kinds as in the real data: 3 = colours, 2 = UVs, 0 = vertices (kind in the high half of the third value)
        for (k, (id, start, len, kind)) in [(9u32, 4u32, 12u32, 3u32), (0xA, 8, 24, 2), (0xB, 32, 48, 0)].into_iter().enumerate() {
            let r = 0x30 + 16 * k;
            pr[r..r + 4].copy_from_slice(&id.to_le_bytes());
            pr[r + 4..r + 8].copy_from_slice(&start.to_le_bytes());
            pr[r + 8..r + 12].copy_from_slice(&(len | (kind << 16)).to_le_bytes());
        }
        pr[0x60..0x62].copy_from_slice(&1u16.to_le_bytes());
        pr[0x62..0x64].copy_from_slice(b"vI");
        pr[0x64..0x68].copy_from_slice(&0u32.to_le_bytes());
        pr[0x68..0x6A].copy_from_slice(&1u16.to_le_bytes());
        pr[0x6A..0x6C].copy_from_slice(b"uI");
        pr[0x6C..0x70].copy_from_slice(&(indices.len() as u32).to_le_bytes());
        pr.extend_from_slice(indices);
        pr.extend_from_slice(indices);

        let table_at = d.len();
        let data_start = table_at + 4 * 16;
        let mut at = data_start;
        let mut recs = Vec::new();
        let mut blob = Vec::new();
        for (i, (tag, bytes, count)) in [
            ([0, 0, b't', b'v'], &verts, 5),
            ([0, 0, b'v', b'u'], &uvs, 4),
            ([0, 0, b'f', b'd'], &cols, indices.len()+1),
            ([0, 0, b'r', b'p'], &pr, indices.len()),
        ]
        .into_iter()
        .enumerate()
        {
            recs.extend_from_slice(&part(tag, bytes.len(), count, at, table_at + 16 * i));
            blob.extend_from_slice(bytes);
            at += bytes.len();
        }
        d.extend_from_slice(&recs);
        d.extend_from_slice(&blob);
        d
    }
    #[test]
    fn reads_a_triangle() {
        let data = tiny();
        let crp = parse(&data).unwrap();
        assert_eq!(crp.articles.len(), 1);
        let m = crp.article_mesh(0, 0).unwrap();
        assert_eq!(m.positions.len(), 5);
        assert_eq!(m.uvs.len(), 4);
        assert_eq!(m.batches.len(), 1);
        assert_eq!(m.batches[0].material, 7);
        // window bases: vertices start at 2, UVs at 1, colours at 1
        assert_eq!(m.batches[0].vertex_idx, vec![2, 3, 4]);
        let b = &m.batches[0];
        assert_eq!(m.positions[b.vertex_idx[2] as usize], [0.0, 1.0, 5.0]);
        assert_eq!(m.batches[0].uv_idx, vec![1, 2, 3]);
        assert_eq!(m.batches[0].colors[0], [255, 0, 0, 255]);
        assert_eq!(m.batches[0].colors[1], [0, 255, 0, 255]);
    }

    #[test]
    fn strip_corner_attributes_and_winding_follow_each_source_corner() {
        // Repeated vertex indices create degenerates; those must not reset winding parity.
        let data = tiny_corners(1, &[0,1,2,2,0,1]);
        let mesh = parse(&data).unwrap().article_mesh(0,0).unwrap();
        let batch = &mesh.batches[0];
        assert_eq!(batch.vertex_idx, [2,3,4, 4,3,4, 4,4,2, 2,4,3]);
        assert_eq!(batch.uv_idx, [1,2,3, 3,2,3, 3,3,1, 1,3,2]);
        let palette = [[255,0,0,255], [0,255,0,255], [0,0,255,255]];
        let source_order = [0,1,2, 2,1,3, 2,3,4, 4,3,5];
        assert_eq!(batch.colors, source_order.map(|i| palette[i%3]));
        assert_eq!(mesh.skipped_batches,0);
    }

    #[test]
    fn strip_short_inputs_and_unsupported_modes_do_not_invent_triangles() {
        for count in 0..3 { assert_eq!(triangle_corners(1,count),Some(vec![])); }
        assert_eq!(triangle_corners(1,4),Some(vec![0,1,2,2,1,3]));
        assert_eq!(triangle_corners(3,4),Some(vec![0,1,2]));
        let data = tiny_corners(99, &[0,1,2]);
        let mesh = parse(&data).unwrap().article_mesh(0,0).unwrap();
        assert!(mesh.batches.is_empty());
        assert_eq!(mesh.skipped_batches,1);
    }

    #[test]
    fn missing_lod_is_none() {
        let data = tiny();
        assert!(parse(&data).unwrap().article_mesh(0, 1).is_none());
    }

    #[test]
    fn rejects_other_data() {
        assert_eq!(parse(b"SHPI0000").err(), Some(CrpError::BadMagic));
        assert_eq!(parse(b"ka").err(), Some(CrpError::Truncated));
    }

    /// Minimal container: no articles, two material parts (one with a real texture name, one with garbage).
    #[test]
    fn reads_materials() {
        let mut d = vec![0u8; 0x10];
        d[0..4].copy_from_slice(b"karT");
        d[8..12].copy_from_slice(&2u32.to_le_bytes()); // two misc parts
        d[12..16].copy_from_slice(&1u32.to_le_bytes());
        for k in 0..2usize {
            let mut rec = [0u8; 16];
            rec[0] = 7 + k as u8;
            rec[2] = b't';
            rec[3] = b'm';
            rec[5..8].copy_from_slice(&0x4Cu32.to_le_bytes()[..3]);
            let data_at = 0x30 + 0x50 * k;
            rec[12..16].copy_from_slice(&((data_at - (0x10 + 16 * k)) as u32).to_le_bytes());
            d.extend_from_slice(&rec);
        }
        for name in [*b"al01", *b"\x01\x84\xa0\x10"] {
            let mut m = vec![0u8; 0x50];
            m[0x10..0x16].copy_from_slice(b"Opaque");
            m[0x28..0x2C].copy_from_slice(&name);
            d.extend_from_slice(&m);
        }
        let crp = parse(&d).unwrap();
        let mats = crp.materials();
        assert_eq!(mats.len(), 2);
        assert_eq!(mats[0], Material { index: 7, kind: "Opaque".into(), texture: Some("al01".into()), page: u32::from_le_bytes(*b"al01") });
        assert_eq!(mats[1].index, 8);
        assert_eq!(mats[1].texture, None);
    }

    /// One article that has only a name part.
    #[test]
    fn reads_article_name() {
        let mut d = vec![0u8; 0x10];
        d[0..4].copy_from_slice(b"karT");
        d[4..8].copy_from_slice(&((1u32 << 5) | 0x1A).to_le_bytes());
        d[12..16].copy_from_slice(&1u32.to_le_bytes());
        let mut art = [0u8; 16];
        art[0..4].copy_from_slice(b"itrA");
        art[8..12].copy_from_slice(&1u32.to_le_bytes()); // one part
        art[12..16].copy_from_slice(&1u32.to_le_bytes()); // part table right after the record
        d.extend_from_slice(&art);
        let name = b"RD0008C (1 PRIMARY)\0";
        let mut rec = [0u8; 16];
        rec[0..4].copy_from_slice(b"emaN");
        rec[5..8].copy_from_slice(&(name.len() as u32).to_le_bytes()[..3]);
        rec[12..16].copy_from_slice(&16u32.to_le_bytes()); // data right after this record
        d.extend_from_slice(&rec);
        d.extend_from_slice(name);
        let crp = parse(&d).unwrap();
        assert_eq!(crp.article_name(0).as_deref(), Some("RD0008C (1 PRIMARY)"));
        assert_eq!(crp.article_name(5), None);
    }

    #[test]
    fn reads_base_and_transform() {
        // one article with a Base part (type 2, geometry 13) and a level-3 transform that moves by (1, 2, 3)
        let mut d = vec![0u8; 0x10];
        d[0..4].copy_from_slice(b" raC");
        d[4..8].copy_from_slice(&((1u32 << 5) | 0x1A).to_le_bytes());
        d[12..16].copy_from_slice(&1u32.to_le_bytes());
        let mut art = [0u8; 16];
        art[0..4].copy_from_slice(b"itrA");
        art[8..12].copy_from_slice(&2u32.to_le_bytes());
        art[12..16].copy_from_slice(&1u32.to_le_bytes());
        d.extend_from_slice(&art);
        let mut base = vec![0u8; 0x70];
        base[0x44] = 13;
        base[0x45] = 2;
        let mut m = [0.0f32; 16];
        for k in 0..4 { m[k * 5] = 1.0; }
        m[12] = 1.0; m[13] = 2.0; m[14] = 3.0;
        let matrix: Vec<u8> = m.iter().flat_map(|f| f.to_le_bytes()).collect();
        let table_at = d.len();
        let mut recs = Vec::new();
        for (i, (tag, len)) in [(*b"esaB", base.len()), ([3u8, 0, b'r', b't'], matrix.len())].into_iter().enumerate() {
            let mut r = [0u8; 16];
            r[0..4].copy_from_slice(&tag);
            r[5..8].copy_from_slice(&(len as u32).to_le_bytes()[..3]);
            let data_at = table_at + 32 + if i == 0 { 0 } else { base.len() };
            r[12..16].copy_from_slice(&((data_at - (table_at + 16 * i)) as u32).to_le_bytes());
            recs.extend_from_slice(&r);
        }
        d.extend_from_slice(&recs);
        d.extend_from_slice(&base);
        d.extend_from_slice(&matrix);
        let crp = parse(&d).unwrap();
        assert_eq!(crp.article_base(0), Some(ArticleBase { geometry: 13, type_index: 2 }));
        let t = crp.article_transform(0, 3).unwrap();
        assert_eq!(&t[12..15], &[1.0, 2.0, 3.0]);
        assert_eq!(crp.article_transform(0, 4), None);
    }

    #[test]
    fn articles_without_colour_part_get_white() {
        let mut d = tiny();
        // rename the colour part's tag so the article has none (car articles are like this)
        let at = d.windows(4).position(|w| w == [0, 0, b'f', b'd']).unwrap();
        d[at + 2] = b'x';
        d[at + 3] = b'x';
        let crp = parse(&d).unwrap();
        let m = crp.article_mesh(0, 0).unwrap();
        assert_eq!(m.batches.len(), 1);
        assert_eq!(m.batches[0].colors, vec![[255, 255, 255, 255]; 3]);
    }}
