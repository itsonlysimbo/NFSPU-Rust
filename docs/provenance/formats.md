# File formats: findings

Sources: community format notes (CRPLib `CRPFormat.md`, OpenNFS/LibOpenNFS, read as documentation only, no code copied) and direct inspection of the author's own install. Everything is written in our own words.

## Track folder (`GameData/Track`)
15 tracks: alps, autobahn, canyon, castle, coastal, farmland, foothills, forest, industrial, monaco1-5, skidpad.

| Extension | Notes | Status |
|---|---|---|
| `.crp` | RefPack-compressed. Decoded starts with `karT` ("Trak"). Container of articles and parts (meshes, materials, embedded FSH) | Decompress verified on all 15 |
| `.fsh` | RefPack-compressed. Decoded starts with `SHPI` (EA shape/texture archive) | Decompress verified on all 15 |
| `.env` | Starts `SHPI` directly (uncompressed texture archive) | Not parsed yet |
| `.map` | Plain text, begins `[header]` / `count=` | Not parsed yet |
| `.edg`, `.jnc`, `.dtx`, `.scn`, `.key` | Unknown | To investigate |

## RefPack (verified)
Header `10 FB` (or `11 FB`), optional compressed size, then big-endian uncompressed size (3 bytes, or 4 when bit 0x80 of byte 0 is set). Commands copy 0-3 literal bytes then a back-reference into the output. All 30 track files decode and the output length equals the header size. Implemented in `crates/nfspu-formats/src/refpack.rs` with unit tests.

Note: an earlier note in the decomp work said no RefPack constants were found in the exe. The data files prove RefPack is used; the loader just does not show up as an obvious constant.

## SHPI texture archive (inspected on `alps.env`, not yet parsed in code)
- 16-byte header: `SHPI`, total size (u32 LE), entry count (u32 LE), 4-char directory id (`GIMX` here).
- Then one 8-byte record per entry: 4-char name (e.g. `1`, `BT1`, `YT2`) and the entry's byte offset (u32 LE).
- Each entry starts with a 16-byte header: format code byte (all 7 here are `0x7D`), 24-bit size (0 here), width u16, height u16, then 8 more bytes (zero here).
- Check: entries are 16,400 bytes apart = 16-byte header + 64 x 64 x 4 bytes, so `0x7D` looks like 32-bit pixels. Pixel channel order still to be confirmed.

## CRP track container (`alps.crp` decoded; checked against all 1,752 articles)
Matches the community notes: 16-byte header (`karT`, header info, misc-part count, article-table offset), then 16-byte article records (`Arti`) each pointing to a table of 16-byte part records. Offsets are in 16-byte units from the record; part offsets are in bytes from the part record.

Part record (16 bytes): 4-byte tag, 1 flag byte, 3-byte **length in bytes** (not 16-byte units), 4-byte **element count**, 4-byte offset from the record.

Tags are stored reversed (little-endian). Names below are my reading of the tags, not confirmed:
| Tag as stored | Reads as | Per element | Verified |
|---|---|---|---|
| `Base` / `Name` | article base data / name | - | present in all 1,752 articles |
| `tv` | vertices (vt) | 16 bytes: three floats (x, y, z) + 4 more bytes | count == length/16 in 1,752/1,752; all xyz finite |
| `vu` | texture coords (uv) | 8 bytes: two floats | count == length/8 in 1,752/1,752 |
| `fd` | per-face data (?) | 4 bytes | count == length/4 in 1,752/1,752 |
| `pr` | primitives / indices (?) | unknown | **not decoded yet** |
Extra copies of fd/pr/uv/vt with a leading index byte 1, 2, ... (297 and 279 articles) look like lower detail levels.

Alps coordinate range over all vertices: x -1268..437, y -36..414, z -5748..495 (the track runs about 6 km along z). UVs reach 0..2 and 0..7, so textures tile. 286 misc parts (`Desc` 59, `RMth` 14, `CamN` 5, ...), the `mt` material parts, and `Anim` / `Anqt` parts are not parsed yet.

## CRP triangle parts (`pr`), now decoded
Layout matches the community notes' "TriangleData", with one difference for this PC version: **index tables use 1 byte per index**, not 2.
- Per-corner colours (`fd` parts) are stored B, G, R, A (Direct3D colours); see weather.md.
- Offset 0x04: material index (u16). 0x28: number of info rows (NIF). 0x2C: number of index rows (NID). Then NIF x 16-byte info rows, then NID x 8-byte index rows, then the tables.
- Index row: u16 id, 2-char tag (`vI` = vertex indices, `uI` = UV indices), u32 offset of its table (from the end of the header). Each table holds `count` bytes. Standard parts: NIF = 3, NID = 2, header = 112 bytes.
- `count` is the number of triangle corners (3 per triangle). Colours in `fd` (4 bytes, RGBA-looking) are one per corner, numbered consecutively over an article's `pr` parts in part-index order. Sum of `pr` counts equals the `fd` count in all 1,752 alps articles.
- Header floats at 0x08..0x18 look like a bounding sphere (centre xyz, radius).
- Part tags: `pr` stores part index in the low 12 bits and level of detail in the top 4; `tv` / `vu` / `fd` store the level of detail in the low nibble of the first byte.
- 56 `pr` parts in alps have NIF = 4, NID = 0 (no index tables) and are skipped for now (articles 1568+, next to `Anim` parts).
Checked on all 15 tracks with `examples/tracks.rs`: 0 out-of-range vertex or UV indices. Triangles per track at LOD0: alps 59,959; castle 117,080 (largest); skidpad 9,297 (smallest).
Still unknown: how the material index maps to a texture, the info rows, the `Base` / `Desc` parts, and which parts are drivable road versus scenery.

## Triangle part windows (fixes the spikes)
The first three info rows of a triangle part (16 bytes each: id, start byte, length | flags, terminator) select **windows** into the article's shared arrays, always in this order: **colours** (4 B each), **UVs** (8 B), **vertices** (16 B). The 1-byte indices are relative to those windows, so absolute index = window start / element size + byte. Row ids differ between layouts (9/10/11, 8/9/0xFFFFFFFF, 14/15/16) so read by position, not id.
- Colour row length equals the part's corner count x 4 in 4,338 of 4,338 alps parts with the 9/10/11 layout.
- Applying the windows cut triangles with an edge longer than 60 units from 5.31% to 0.96% in alps. Remaining long triangles look like real terrain and a few large strays; not yet understood.
- Window starts are non-zero for vertices in 816, UVs in 1,227 and colours in 2,764 alps parts, so ignoring them scrambles geometry.

## Materials and textures
- Material parts live in the CRP's misc table, stored as `[index_lo, index_hi, 't', 'm']`. Offset 0x10 (16 bytes) holds a kind ("Opaque", "Normal", "FakeSort", "NormMatState", "FullAmbMatState", "FullAmbient"). Offset 0x28 holds the **4-character name of a texture entry in the track's `.fsh`** (not a number; the doc's "texture page index" is these four ASCII bytes). A triangle part's material index (u16 at offset 4) selects the material.
- Alps: 204 materials, 203 name a texture that exists in `alps.fsh`; one holds non-text bytes (treated as untextured).
- Texture entry = 16-byte header (format code, size, width, height, ...) then the full-size image, then smaller copies (mipmaps). Formats seen: `0x7D` = 32-bit stored B, G, R, A; `0x7E` = 16-bit, bit 15 alpha, then 5 bits each red, green, blue. Verified by eye on decoded images (BGRA gave natural grass and asphalt; 1555 gave clean walls and a sign with a transparent background; 565 and 4444 gave noise).
- Checked on all 15 tracks: every material used by a triangle resolves to a decodable texture, except one untextured material each in alps and farmland and one texture name missing from industrial's `.fsh`.
- Texture sizes are mostly 128 x 128, also 64, 32, 16 and 256.

## Article names: road, terrain, scenery
Every article has a `Name` part (tag `emaN`, a zero-terminated string). In alps (1,752 articles):
- `RD####C (N PRIMARY)`: **road surface pieces**, 259 of them, numbered in steps of 8 along the track (RD0000C, RD0008C, ...). Drawn alone they form one continuous thin ribbon along the whole course (one gap near the top-left of the alps render). 225 are `(1 PRIMARY)` and 34 are `(28 PRIMARY)`, which sit at the same positions (surface variants). Median road width is 12 units, so one unit is about one metre. Pieces are 48 units apart along the road.
- `CNK####L` / `CNK####R (...)`: side **chunks** (1,075 articles, 46,000 triangles): terrain, cliffs, bushes, guard rails (`GRDRAILNPOST_LOD0`), numbered by position along the track.
- `OBJECT`, `MESH`, `SHRUB(S)`, `Box`, `OBJLCU` / `OBJRCU` etc.: small objects and scenery (a few hundred triangles each).
- Monaco tracks use several route variants (`(1 PRIMARY)`, `(2 PRIMARY)`, `(5 PRIMARY)`, `(6 PRIMARY)`, `(20 PRIMARY)`); monaco2 has only 6 `(1 PRIMARY)` pieces, so the main-route rule is alps-specific. Not worked out.
- **Winding:** all 8,846 alps road triangles have `(b - a) x (c - a)` pointing down (-Y), so up-facing triangles are the ones with a negative Y normal under this cross-product order (clockwise from above, the Direct3D convention).
- **Road on other articles (found 2026-10-17):** the road does not always sit on `RD`/`CNK` articles. Checked by putting three points on every road slice (`SimD`: the centre and half way to each edge) and asking which article's drivable face is closest in height above or below each point (within 8 m):

  | track | articles carrying road that are not `RD`/`CNK` |
  |---|---|
  | monaco1-4 | `CASINOENTRANCE_A_D`, `_B`, `_C`, `CASINOCOURTYARD` (the start grid of monaco2 sits on them) |
  | industrial | 24 articles: `OBJECT01`-`07`, `MESH04`, `MESH30`, `Mesh01`, `STEELSIDE`, `STEELMILLFRONT`, `FACTORYFRONTSIDE`, `BIGWAREHOUSE16`, `ALLEYJUNCTION5`, `246246426` (street blocks; some names repeat) |
  | castle | 9: `OBJECT02/04/34-37`, `MESH05/06`, `MESHA` |
  | alps | `SECONDARY`, `SECONDARY01`, `SECONDARY04` (side roads) |
  | forest | `CHUNK1`-`3` |
  | autobahn, coastal | `TERRAIN_MID1_68`, `ALLYENTRANCE_SECONDARY` |
  | canyon, farmland, foothills, monaco5, skidpad | none |

  The loader now finds these articles this way and loads them like road pieces (their flat faces are ground, steep ones walls). Before, road slices with no ground within 3 m: autobahn 102, industrial 183, monaco1 32, monaco2 36, monaco3 65, monaco4 49 (race_audit). After: no slice on any of the 15 tracks lacks ground.
- **Road slices are not always at road height.** Even with every article loaded, some slices sit 3-7 m above or below the nearest road face: autobahn 102 (ramps; e.g. slices 226-265 are about 3 m above `RD0032C`-`RD0064C (3 PRIMARY)`), industrial 58, farmland 17, forest 15 (a side-road stub, slices 1947-1961, dips up to 10.6 m under the terrain), castle 12, monaco1-4 3-11. Faces were searched at all four levels of detail and no surface exists at slice height there, so the slice heights simply do not follow the mesh in these places (why is not known; the slice `kind` bits there are often 0x3C000 / 0x7E000 rather than the usual 0x18000).
- **Some road faces have the opposite winding.** Autobahn's `RD0016C (2 PRIMARY)` has flat road faces whose normal points up under the file's winding (facing down); `CNK0048E (2 ONOFFRAMPB_SECONDARY_LOD0)` and `CNK0264E (7 ONOFFRAMPD_...)` mix both windings in ramp surfaces. Treated as walls they made invisible walls across the road. Now a flat face that faces down is never a wall; inside an `RD` piece it counts as road. How many such faces exist was not counted.
- **Ramp slices with the road on one side:** autobahn on/off-ramps have slices with left half-width 0 and right half-width 9 (`kind` 0x8000), so the slice line runs along the road's edge.
- `.edg` (side file): a 4-byte header, then 28-byte records of a u32 and two 3D points (7,694 of them in alps), probably edge line segments. Not used yet. `.map` is a text file with 39 numbered sections (start/end slice, tints, texture ids). `.jnc` (216 bytes of floats) and `.dtx` (11 bytes) not understood.

## Car files (`GameData/CarModel/<car>.crp`)
- **Newer notes in `car.md`** replace several points below: the texture page of each triangle is a
  number in its material (not the material kind), page pixel formats decide what alpha means, and
  colour bands (not an alpha paint mask) apply the paint, stripe and cabin colours.
- Container magic is the bytes ` raC` (a space first), i.e. ""Car "" stored reversed. Most are RefPack-compressed; the 928 is stored plain. 993: 172 articles, 67 misc parts (48 materials).
- **Levels of detail start at 1** (the first byte of a part tag): per the community notes 1 = garage model, 2 = in-car view, 3 and up = exterior. The 993 at level 3 has 77 articles and 2,623 triangles. Variant bits (animation, damage) sit in the high nibble of that byte (17 = level 1, variant 1).
- Car articles carry **no baked vertex colours** (no `fd` part) and the material texture-name field (0x28) is all zeros. Material kinds: `CarExtEnv` (body paint with environment map), `CarExt` (trim, underbody), `CarWindow`, `CarWheel`, `CarInt` (interior). Car textures are in `<car>D.fsh` / `<car>L.fsh` as 8-bit paletted images (format 0x7B, palette format not decoded) with names `0000`..`0008`; `carcmn.fsh` holds shared 32-bit items. The material-to-texture link is not understood yet (`.tpg` and `.clr` files unexamined).
- **Base part** (tag `esaB`): offset 0x44 = geometry index (which piece: 1 body, 5 skirt, 7 roof, 11 underbody, 12 wheel well, 13 front wheel, 14 rear wheel, 19-21 lights, ...), 0x45 = type index (0 = default, others are variants such as Body1, Roof1, a spoiler, a sunroof, cabrio roofs, ten driver models). Pieces with the same geometry index are alternatives, so only type 0 is drawn.
- **Transform part** (tag `rt`, 64 bytes): a 4x4 matrix, row-vector convention (translation in the last row). Body parts are already in car space; wheels are modelled around the origin and placed by their matrix, e.g. front (+-0.69, 0.31, -1.19) and rear (+-0.74, 0.31, 1.09) on the 993. The car faces -Z, +X is the right side, the origin is at ground level, units are metres (body 4.23 long, 1.86 wide with mirrors).
- **Info rows in triangle parts**: car parts can have 4 rows (a leading extra row). Rows are identified by the *kind* in the high 16 bits of their third value: 3 = colour window, 2 = UV window, 0 = vertex window, 1 = probably normals. Reading rows by position (my earlier rule) was wrong for cars; by kind works for both (all 15 tracks unchanged).
- Loads 44 of 45 car files with plausible sizes; `postship` has no body mesh; `f901` has no wheels (a partial model).

## Car textures: where they really are (corrects the notes above)
- **The car textures are embedded in the `.crp`.** The container's misc table has parts tagged `fs` (`[index_lo, index_hi, 'f', 's']`), each holding a complete `SHPI` archive. The 993 has five: index 0 (595 KB, 38 tiles, the **exterior** body art: panels, lights, crest, decals), 1 (interior: dashboard, gauges), 2 (trim and **wheels**: alloy wheel face, underbody), 3 (glass, one 64 x 64 tile), 7 (a 64 x 64 price-tag tile). All tiles are 32-bit (0x7D). All 44 loadable cars have embedded archives 0, 2 and 3.
- The `<car>D.fsh` / `<car>L.fsh` files next to the `.crp` are **damage** overlays (grey wrinkled patterns): 37 paletted tiles (format 0x7B, 8 bits per pixel, then a 16-byte header and a 256 x 4-byte palette). My earlier count of 9 entries was wrong: the directory repeats names and I had keyed by name. Not used yet.
- A texture archive is made of **tiles placed on a page**: bytes 12-13 and 14-15 of an entry header are the tile's x and y on the page. Pages are assembled by pasting the tiles in directory order (alpha blended). UVs are in page units (0..1 over 256 pixels for the exterior and trim pages, 64 for glass).
- **Which page a piece uses**, tested by eye on the 993: body paint pieces (material kind `CarExtEnv`) use page 0; trim and wheels (`CarExt`, `CarWheel`) use page 2; glass (`CarWindow`) uses page 3. Putting trim on page 0 puts tail-light art on the wheel wells. The materials do not carry a texture name (field is zero), so the link is by kind. Untested on other cars; no explanation yet for how paint style (`.tpg` styles, `.clr`) selects tile variants (several tiles share the same x, y, e.g. `det` and `det1` at (0, 0)).
- `.tpg` is a plain **text** file ([header], numfiles, numtpages, file1..file10 names, tpageN details with sizes and bit depths, fileN details, [styleN] with geometry/type/texture choices, [damagezones]). `.clr` (about 7 KB) not examined.
- The body page is white and grey: the paint colour is meant to be applied on top. Not done yet.

## Texture page sizes: read them from the .tpg (corrects the earlier 256 guess)
- The trim and wheel page (embedded archive 2, `extint`) is **128 x 128** on the 993, not 256: every tile on it fits exactly (`bott` at (0, 96) is 128 x 32, `lice` at (48, 64) is 80 x 32, `stee` at (64, 0) is 64 x 64), the wheel face quad's UVs span 0..0.5 which is the 64-pixel `faf` tile only on a 128 page, and the tread's UVs land exactly on `tref` at (32, 64). The `.tpg` file says `[tpage4.details] width=128`.
- `.tpg` maps archives to pages: `[fileN.details] tpage=M` (file N is embedded archive N-1) and `[tpageM.details] width=W`. The loader reads this and falls back to 256 / 256 / 128 / 64 if there is no `.tpg`.
- A wheel is an 8-sided prism: 16 triangles of tread (tile `tref`/`treg`), a round-looking outer face (64 x 64 tile `faf`, alpha cut-out) and an inner face (32 x 32 `whei`): 20 triangles. The underbody (`Bottom`) uses the `bott` tile. With the wrong page size both showed as grey squares or vanished.
- `CarInt` pieces (`Interior`, `InteriorRoof`, `Dash`, `Seat`, `Steer`) are drawn with the interior page (embedded archive 1, 256). The steering wheel tile (`stee`) lives on the trim page, so the page is not purely by material kind; the steering wheel probably looks wrong. Not yet checked live.

## Which page each car piece uses: findings and open questions (993)
- Method: record each triangle's article and material, then see which tile rectangle (on each page) contains the article's whole UV box. Confirmed: `Exhaust` -> `exha`, `Light1/2/3` -> `lit1/2/3` (all on the **exterior** page 0), `Roof` -> `top*` (page 0), `Handle` -> `hand` (page 0), `Bottom` -> `bott` (trim page 2), `License` -> `lice` (**page 2**, although its material kind is CarExtEnv), so the page is not purely a function of the material kind.
- Material records: a row table at 0xB4..0xF4; the last value is 3 for CarExt, 5 for CarExtEnv and CarWindow, 2 for CarInt and CarWheel (CarExtEnv also has 7, 8, 6 before it, CarWindow 7, 6). CarWheel and CarExt use the same page although their values differ, so these numbers are not simply archive indices. Unresolved.
- **Cabin (CarInt) pieces are unresolved:** `Dash`, `DoorIn`, `Interior`, `Seat`, `Steer` have UVs covering nearly a whole page and no single tile on any page contains them; against the interior page (archive 1, 256) only 3-37% of their UV points hit opaque pixels. The cabin comes from level 1 (garage model; 1,051 triangles vs 181 at level 3) and is drawn flat dark grey for now.
- Driver textures live in separate shared files next to the cars: `Suit.fsh` (`cas1`-`cas8`, `suit`) and `Head.fsh` (`fac1`-`fac8`, `helm`), 128 x 128, 8-bit paletted (format 0x7B); `INTGLASS.FSH` (`glai`, `glat`, 64 x 64 interior glass), `Cabrio.fsh`, `Price.fsh`, `Shadow.fsh`. The driver pieces (`DriverBody1`-`10`, `Legs`, `LeftArm`, `RightArm`, `LeftHand`, `RightHand`) exist at levels 1 and 2. Not drawn yet.

## Body styles and artwork rules: the `.tpg` file in full (resolves the variant questions)
- `[styleN]` sections name a body style and list, per piece kind, which **type** to use: `geometryK=<geometry id>` with `typeK=<type>` (geometry id and type are the Base part's geometry and type index), and, per artwork family, `textureK=<texture id>` with `typeK=<type>`. Ids that are not listed use type 0. The 993 has nine styles: Carreracoupe, Carreratarga, Carrera4cab, Carrera4coupe, Carrera4Scoupe, Carreracabrio, CarreraS3.6coupe, GT2 Racecar, turbocoupe. Example: the cabrio uses piece 37 type 1 (Cabrio) and 41 type 1 (CabrioUp, the black soft top) and no hard roof; the targa uses piece 7 type 1 (Roof1) and 8 type 1 (SunRoof, glass); the coupe uses neither.
- `[fileN.tile]` sections say **when each texture tile is drawn**: any listed (geometry, type) or (texture, type) pair matching the style. For example `[file1.top] geometry 7 type 0 or 1`, `[file1.top2] geometry 7 type 3` (the black roof for the cabrio styles), `[file1.sid/sid1/sid2] texture 1 type 0/1/2`, `[file1.det..det5] texture 22 type 0..5` (badge lettering), `[file3.whg..whg4] texture 13 type 0..4` (wheel faces). `frontend=` / `testdrive=` flags mark tiles for the garage and test-drive views (`whf`/`faf` front end, `fag` test drive, `whg` in a race). `racedecal=1` is for race cars only. `fileN` is embedded archive N-1. Tiles with no rule are always drawn. Layering all alternatives (my earlier approach) mixed roof and badge variants.
- Driver: `[file9.suit]` and `[file10.helm]` apply to driver piece type 1 and 6; `cas1..cas8` / `fac1..fac8` to types 2-5 and 7-10. Styles list `geometry 9 type 1` (racing suit and helmet) for the coupe. `Suit.fsh` is file 9 and `Head.fsh` file 10 (global files).
- Open question: some styles list piece types that do not exist in this `.crp` (for example style 1 wants piece 5 type 3, the file only has types 0 and 1), so that piece is simply not drawn. The front skirt `SkirtF` is therefore missing in every style. Not verified against the original.

## Soft-top rear window (cabrio styles)
In `CabrioUp` (and the other `Cabrio*` articles) two identical 19-triangle shells use the roof tile (`top2`, black) and a third 8-triangle batch (material 41 on the 993) is the **rear window**: its UVs span most of the 0..1 range, so no tile on the body page contains it. Its texture is the single 32 x 32 tile `sgla` in the shared `CarModel/Cabrio.fsh` (tpage 8 in the `.tpg`, file 7). The loader treats any batch of a `Cabrio*` article whose UV box is wider and taller than 0.5 as the window. A heuristic, tested only on the 993.

## Handedness: the files are left-handed, the world is mirrored on import
- The 993's steering wheel (centre x +0.39) and driver (x +0.41) sit on the +X side in the car file, but the car's `.tpg` puts the driver camera at `drivercampos x = -0.38`. The game's coordinates are left-handed (Direct3D), Bevy's are right-handed, so loading the numbers as they are mirrors the world left to right: the steering wheel would be on the right and text and signs would read backwards. The loaders now multiply x by `MIRROR_X = -1` (`crates/nfspu-game/src/lib.rs`): the visual track mesh, the ground triangles and the start position, the car (positions, normals, wheel offsets). After the change the steering wheel is at x -0.39. Normal vectors are mirrored too. The up-facing and outward-normal tests still use the file's clockwise winding and run before the mirror. Autopilot laps are unchanged (distances preserved). Revert by setting the constant to 1.0.
- The in-game driver is picked by its own type (not the body style): `LODDriver{T}` (piece 9, type T), with tiles chosen by the `.tpg` rules: type 1 = `suit` + `helm` (racing suit and helmet), 2-5 and 7-10 = `cas1..8` + `fac1..8`. Suit and head come from the shared `Suit.fsh` / `Head.fsh` (128 x 128, 8-bit paletted, palette entries stored B, G, R, A). The head is the driver batch that sits higher up.

## Alpha on the body page is a paint mask
On the exterior page (embedded archive 0) the alpha channel is not transparency: the body tiles are 100% partial alpha (`top`/`fro`/`rea`/`hooi` mean about 200 of 255, never 0 or 255), the side tile `sid` has 79.5% partial (mean 140) and 20.5% fully clear texels (the wheel arches). Cutting out texels below 50% punched holes in the side panels between the wheels; about 9% of the body's triangles sampled such texels. Only alpha 0 is a real hole. Lights (`lit1-3`) and the wheel faces do have genuine transparent corners. The mask probably decides where the paint colour applies (not used yet).

## Paint colours and lights
- `<car>.clr` is a plain-text list of paint schemes: `[sectionN]` (N = 1..21 on the 993, not in file order) with `stripepackage` and four colours `color1..4` as `.h/.s/.v` and `.r/.g/.b`. `color1` is the body colour (1 dark silver, 2 white, 4 red, 6 yellow, 13 dark red, 21 black, ...), the others look like stripe and trim colours. `color2..4` are not used yet.
- **Paint mask:** on the body page alpha 255 means not painted (soft top, exhaust, lamp lenses), 0 is a hole (wheel arches, lamp corners), and partial values (about 140-220) are the paintable body. The loader multiplies those texels toward the scheme's colour by alpha / 224. Checked by looking at the painted page.
- Lamp articles on the 993 (Base geometry 19, 20, 21): `Light1` front headlights (`lit1`), `Light3` front low lights (`lit3`), `Light2` rear tail lights (`lit2`). There is no separate lit version of the artwork that I found; the viewer brightens the material to make them glow.

## Levels of detail and the cabin
A car's levels are separate models that don't nest: on the 928 the level-1 cabin is 0.89 m wide at the door's front edge, while the level-3 body there is only 0.86 m, so the detailed cabin pokes through the simpler body. The loader therefore keeps two cabins: level 1 for the cockpit view and the exterior's level for outside views. `car_articles` (example) lists each article's piece id, type and triangles per level.
Lamp articles differ per car (993: `Light1`-`3`; 928: `HeadLight`, `Light4` front, `Light2` rear, `Light6` side markers). They're classified by name prefix and position along the car.

## Track objects, signs, plates, handles
- Track articles with a level-0 transform (row-vector, `p * M`) are objects. About 111 roadside signs on alps (`OBJ####L/R (n <kind>_Rigid1_SGN)`) carry their real position. Templates (traffic cars, cones, people, e.g. `Sawdude1` at (5, 68, 6), about 138 m tall in the file) sit within 50 m of the origin horizontally and are placed by data not read yet; they're skipped.
- A sign has a front board (`sgn1` artwork), a back board 2 cm behind it facing the other way (`sgn2`), and a flat post. Signs ending `R` stand on the right of a driver going in increasing road-piece order and face them; `L` signs are for the other direction. Back-to-back batches are drawn one-sided; other faces (posts) double-sided.
- Licence plates: `[file3.lice] license=1` in the `.tpg` makes tile `lice` on the trim page the plate; the game prints the number using `Render/license.psh` (`SHPP`, format 0x40: `blnk` 48x22 plus glyphs a-z, 0-9 at 7x12). The `License` article's material says body page but it belongs on the trim page. Numbers not drawn yet.
- Door handles: the same material kind (`CarExt`) means the trim page on the 928 but a `hand` tile on the body page on most cars; chosen by which tile contains the UVs. Handles, plates and lamps lie on the body surface and are lifted 5 mm.

- Wheel-arch liners (`WheelWell`) sample a single texel in tile `stee` on the trim page whose alpha is 0 (colour 110,110,110). Alpha on the trim page is therefore not always transparency; the liners are drawn opaque. Wheel faces still use the cut-out.

## Texture page roles and traffic cars
- A car's `.tpg` `[header]` lists its texture files (`fileN=`); embedded archive N-1 is that file. Porsches: exterior, interior, extint (trim, wheels), glass, (intglass, shadow, cabrio, price, suit, head). Traffic cars (`eas*`, `eden*`): exterior, interior, shadow, driver - no trim page. Pages are matched by name, not position.
- Traffic layout: `exterior` (128 px) holds body panels and tyre (`fro rea sid top trun whei tref`...), `interior` (128 px) holds lamps (`lit1 lit2`), glass (`glas`), plate (`lice`), wheel faces (`whe`/`whg`), underside (`bott`), mirrors and cabin bits. A single batch can mix both (`eas10` tyres). Their levels stop at 3, which is very crude; level 1 is used. Their driver is `DriverBody0` (piece 9, type 0) with texture `tdri` in `driver.fsh`, format 0x7F (24-bit, B,G,R order).
- `postship.crp` has only wheels, `Bottom` and a large `Decal` at level 1: not a drivable car.

## Track skies (`.env`) and time of day
- Each track's `<track>.env` is an SHPI file with seven 64x64 pictures: `1` black (under the world), `2` and `3` dark cloud layers, and `BT1`/`BT2`/`YT1`/`YT2` - two blue and two orange sun glare sprites. It is not a cube map.
- The retail game tints those layers by the time of day: the reference footage of alps shows an orange sunset at the start line, blue daylight later, and a night sky with falling snow at the village. `GameData/Render/night.psh` (SHPP, 128x128, format 0x40) holds that star field (`nght`).
- So the pictures alone look dark and blurry; the colour comes from the time-of-day lighting, which we do not read yet. Our dome (`spawn_sky`) uses `3` round the sides, `2` overhead and `1` below; the glare sprites are not drawn because which way the sun is has not been established.

## Sky data (found 2026-10-09)
The engine's strings (`%sSky\%s.txt`, `%sSky\%s.bin`, `%sSky\%s.fsh`, `night.psh`) point at `GameData/Track/Sky/`:
- `<track>.fsh` (SHPI): `st1a`/`st1b` small sky maps, `sc1a`/`sc1b` sky colours, `cc1a`/`cm1a` white cloud layers (32x32, plain white), `hzcl` (16x2) and **`horz` (256x256) - a painted panorama of snow-covered mountains for the far horizon**. This is what puts mountains behind the track in the retail frames; ours has none yet.
- `<track>.txt`: two blocks (HIGH and MEDIUM capability). Each names the sky level type (`MULTINORMAL`), whether the layer blocks the sun, two **weather regions** (A and B) with their sky map, scale factors, move ratio, wind speed and direction, warp, and up to three cloud textures. So the sky is built from regions, not one cube.
- `<track>.bin`: binary, starts with a count (9 for alps) then floats - looks like per-region bounds (values like -101, 346, 1000, 125) and possibly the warm/cold colour key. **Structure not decoded yet.**
- Colour: the retail frames show alps going orange at the start line, blue in daylight and starry night at the village. The clouds are white layers (`cc1a`) tinted by `sc1a`-style sky colours, so the warm/cold change likely comes from that sky colour map or the `.bin` key, switched per region as you drive.

## Sky data, decoded (2026-10-09)

The engine's strings (`%sSky\%s.txt`, `%sSky\%s.bin`, `%sSky\%s.fsh`, `CSkyCnfig`, `night.psh`) point at `GameData/Track/Sky/`. There are 15 sets there (one per track, plus `default`), so every track gets its own sky from the same three files.

`<track>.fsh` (SHPI, all pictures BGRA32 `0x7D`):
- `horz` 256x256 - a painted panorama of the mountains behind the track, transparent above the ridge. This is what the retail frames show on the horizon. **Not** a full 360 panorama: the left and right edges differ more than the middle does, so it is one segment. (Some tracks also have `hor0`-`hor3`.)
- `hzcl` 16x2 - horizon haze colour (light blue over darker blue).
- `st1a`/`st1b` 16x16 - sky map for weather region A / B, dark blue-grey and opaque.
- `sc1a`/`sc1b` 16x16 - sky colour for region A / B, with varying alpha. **This is the warm/cold difference between the two regions**: alps region A averages 0.71,0.68,0.70 and region B 0.41,0.44,0.50.
- `cc1a`/`cm1a` etc 32x32 - cloud layers. `cc1a` is white with an alpha ramp (the cloud shape); `cm1a` is opaque pinkish (209,184,184), a sunset-lit cloud colour.
- Others seen: `ltmp` 7x7, `fct1` (Monaco), `suna` 64x64 (Farmland, a sun sprite), `sm1b`/`wm1b` (Coastal), `sky1`-`sky6`/`cld2`/`cld6` in `default`.

`<track>.txt` - plain text, one `BEGIN`..`END` block per graphics capability (HIGH then MEDIUM; we read the first). Comments name each group of values: the capability, the sky level type (`MULTINORMAL`/`basic`), and then per weather region a sky map + sky colour + cloud name, the X and Y scale factors, the move ratio, the wind speed and the wind direction. The cloud textures are listed once for the whole block. The warp settings and the "does this layer block the sun" flag are skipped.

`<track>.bin` - the sky colours, and this is the time-of-day key:
- Header: a count (9 for alps, 7 for default/Monaco), some floats, then pairs of small integers.
- Then a table of **12-dword records**, each: distance along the track (`0xFFFFFFFF` when there is none), two floats, then the horizon colour **twice** as packed `0xFFBBGGRR`. Found by looking for the first dword where that pattern holds, which skips the header and the scrolling parameters without needing their layout.
- The colours run from cold blue-grey `(87,103,139)` to warm orange `(109,69,53)` and back. alps has 74 of them (48 warm, 21 cold); Autobahn 280 all warm; Castle, Farmland and Skidpad all cold blue `(77,149,254)`; Industrial a dark blue `(0,33,51)`. This is the warm-to-cold change along the lap.
- Records with the same distance in pairs are left and right of the road.
- `0xCBCBCBCB` runs in the file are uninitialised memory (0xCDCD), i.e. arrays this track's configuration never filled.
- The 7-record files (default, Monaco) have no colour table at all - their sky is a flat colour.

`WEATHER` is a race setting in the engine's settings table, not per-track data, so the weather is not chosen from a file.

### Reading the historical sky notes
The sections below retain investigation history. Later corrections supersede earlier claims:
record positions are **not verified directions or lap distances**, and there is no verified
"global colour template". Geometry/atlas evidence does not solve the colour mapping.
See the 2026-10-15 verified layout and vertex-colour corrections at the end.

### Sky horizon geometry (from the decomp, 2026-10-09)
`decomp/classes/SkyHorizon.c` shows how `<track>.bin` is used:
- `SkyHorizon::vfn03_00459d30` builds the horizon ring. It loops with a counter masked `& 0xF`, so the horizon is **16 segments**, and it steps a vertex buffer by 16 bytes (two vertices of position + colour each) until it has covered `0x100` bytes.
- The colours come from the loaded config at offsets `+0x50 + (i & 0xF) * 4` and `+0x50 + ((i+1) & 0xF) * 4` - i.e. **16 packed colours indexed by segment number, wrapping at 16**, not by distance along the track. Each segment's vertex takes the colour of its own index and of the next one.
- Positions come from bytes at `+0x1c + (i & 0xF) * 2`: one byte is a segment number within a division `*(int *)(cfg + 0xc)`, the neighbouring byte says whether to add 1. This builds the ring's shape from a segment list.
- `SkyHorizon::ctor_dtor_00459b50` reads `DAT_005cef14 + DAT_00657d9c * 6` (a per-track table, 6 bytes per entry) and loads `horz`/`hzcl`.
- `FUN_00459700` in `porsche_00450000.c` is the fallback when a track has no `.bin`: it writes a default config with `puVar1[0] = 9` (the same 9 we read as the first dword) and `puVar1[3] = 2`, `puVar1[4] = 2`, then a run of 32 zero bytes, then `0x447a0000` (1000.0), `0x39d1b717`, `0x42960000` (75.0). This is why `default.bin` is a small fixed-size file.
- `WEATHER` is a race setting name in the engine's settings table, alongside `TIME`, `MIRROR`, `TRAFFIC_DENSITY`; it is not per-track sky data.

**Consequence for us:** the 16-segment ring explains why our first attempt looked wrong, but it means the warm/cold colours are keyed by *segment around the horizon*, not by position on the track. Our current distance-based tint is therefore the wrong model. Not yet worked out.
- The colour records on alps number 36 from dword 119, not 16, and the distances come in near-pairs (436/439, 473/474, 503/504, 538/539, 556/557, 578/579, 603/605, 619/620, 644/648) - most likely the two sides of the road. Colours rise monotonically from cold (87,103,139) to warm (119,70,46) as the distance rises. How this maps onto the decomp's 16-segment ring is **not yet worked out**.

### What makes the sky change warm and cold (found 2026-10-09)

`SkyHorizon::vfn01_0045a1a0(undefined4 param_1, float param_2)` in `decomp/classes/SkyHorizon.c` is the answer, and it shows our model was wrong:

`
param_2 = param_2 - *(float *)(cfg + 4);      // cfg[4] is a start angle; alps has 0.8017
if (param_2 < 0.0) { param_2 = param_2 + 1.0; }
iVar1 = ROUND((param_2 - 0.125) * 16.0);
iVar4 = iVar1 - 1;
iVar2 = ROUND((param_2 + 0.125) * 16.0);
iVar3 = iVar2 + 1;
if (iVar4 < 0) { iVar4 = iVar1 + 15; iVar3 = iVar2 + 17; }
`

- `param_2` is an **angle as a fraction of a full turn** (it wraps at 1.0, not at a distance in metres). Subtracting `cfg[4]` rotates the horizon so it lines up with the track's start line - the `0.8017` and `689.6552` we saw at dwords 15-16 of alps' `.bin`.
- `0.125` is one sixteenth of a turn, and the horizon is 16 segments, so each segment is `1/16` of a turn (22.5 degrees). The code finds the two segments either side of the view and draws them plus one either side, so the ring is always built around where you are looking.
- `iVar4` wraps backwards by 15 and `iVar3` forwards by 17, which is the ring wrapping round.

So: the sky colour is keyed by **which way the camera is facing**, in sixteenths of a turn, relative to the track's start direction. That is why the colours come in near-pairs in the file (two segments seen at once), and why they only cover part of a lap's distance - our "distance along the lap" reading of the first field was simply the wrong interpretation.

**To implement:** we need the camera's yaw, subtract the start yaw, wrap to 0..1, multiply by 16 to get the segment, and take the colours for the segments around it from the `.bin` table. Our current code measures distance along the centreline instead, which is why the change only appeared in the first fifth of the lap.
- Correction on the sky colour directions: alps' `.bin` does **not** hide extra colour tables. Scanning every packed colour and every 12-dword record finds 67 records, and splitting them where the stored float goes backwards gives five groups - but groups 0 and 3 are identical, and groups 1, 2 and 4 all sit inside the same narrow arc. Every direction in the file lies between 0.057 and 0.260 of a turn, about 73 degrees. So either that is genuinely all the data, or the float at record offset +1 is **not** the direction.
- It is probably the latter. In `SkyHorizon::vfn03_00459d30` the ring's vertex positions are not read from the file at all: they are computed as a segment number divided by two counts in the config, `1 / *(int *)(cfg + 0xc)` and `1 / *(int *)(cfg + 0x10)`, with a branch on a flag byte at `cfg + 0x1d`. So the two floats in each record are more likely a normalised position on the ring than a direction, and the `+1` float we took as "direction" is probably one of those position components. **Our direction model is unconfirmed.**

### How the horizon ring is actually built (`SkyHorizon::vfn03_00459d30`)
The ring is a **grid of quads over the `horz` picture's texture space**, not a set of directions:

`
fVar1 = 1.0 / (float)*(int *)(cfg + 0xc);      // number of columns
fVar2 = 1.0 / (float)*(int *)(cfg + 0x10);     // number of rows
for each segment i:
    uVar11 = (byte)(cfg + 0x1c + (i & 0xF) * 2);   // column order table, 16 bytes
    x0     = (uVar11 %  cols) / cols;                // left edge, in texture space
    row    =  uVar11 /      cols;
    if ((byte)(cfg + 0x1d + (i & 0xF) * 2) == 0) { left = x0;      right = x0 + 1/cols; }
    else                                     { left = x0 + 1/cols; right = x0;      }
    top    = (row + 0.01) / rows;
    bottom = (row + 0.98) / rows;
    // two vertices: (left, top) and (right, top), stepping 8 floats = 2 vertices
`

So:
- The 16 bytes at `cfg + 0x1c` are a **column order table**: which column of the `horz` grid each of the 16 segments shows, and they are not in order, which is how the picture is rearranged round the horizon.
- The bytes at `cfg + 0x1d` are a **flip flag** per segment, saying whether that segment's columns run left-to-right or right-to-left.
- `cfg + 0xc` and `cfg + 0x10` are the column and row counts. On alps the header dwords 3 and 4 are 4 and 2.
- The 16 colours are read from `+0x50 + (i & 0xF) * 4` of a **global horizon template** (`DAT_005cef74` via `FUN_00460c60`), not from the per-track config directly, and `cfg + 8` is the starting segment index.

**Still unknown:** how the 67 records in `<track>.bin` map onto those 16 template colours. Their first field is a byte-ish index (0, 436, 439, 473, 474, ... up to 2104) and the two floats look like normalised ring positions rather than directions. Until that mapping is pinned down, our sky colour is a guess and should be labelled as one.
- Correction: `FUN_00460c60` (used as `FUN_00460c60(&DAT_005cef74)`) is **not** a colour template builder - it is a name-keyed resource cache lookup, the same family as `FUN_00460b50` which the horizon constructor uses to fetch its pictures by name. So there is no separate global 16-colour template; `iVar7` in `vfn03_00459d30` is a loaded resource object, and the `+0x10` / `+0x50` offsets are into that object rather than into a colour array we can read. **How the `.bin` records map onto the 16 segments is still unknown**, and reading the decomp has not closed that gap.

### The sky config IS the raw `.bin` (verified)
`FUN_00459660` loads `Sky\<track>.bin` (falling back to `Sky\default.bin`) and stores the
**raw file pointer** at `in_ECX + 0x104` with no transformation, so the offsets the decomp uses
are byte offsets straight into the file. Reading them:

| offset | meaning | alps | Monaco |
|---|---|---|---|
| `+0x00` | version | 9 | 7 |
| `+0x04` | float: horizon zero point | 0.8621 | 0.7820 |
| `+0x08` | int: start segment | 0 | 5 |
| `+0x0c` | int: columns | 4 | 4 |
| `+0x10` | int: rows | 2 | 1 |
| `+0x14` | float | -101.31 | -64.80 |
| `+0x18` | float | 346.71 | 253.27 |
| `+0x1c` | 16 bytes: column order, one per segment | 0,1,2,3,4,5,6,7,0,1,2,3,4,5,6,7 | 0,1,2,3,3,3,3,3,2,1,0,0,0,0,0,0 |
| `+0x1d` | 16 bytes: flip flag per segment | all 0 | 0,0,0,0,0,0,0,0,**1,1**,0,0,**1**,0,0,0 |

alps has 4 x 2 = 8 cells and its 16 segments use cells 0..7 twice in order, so its ring is a plain
repeat. **Monaco is the proof**: 4 x 1 = 4 cells, and its 16 segments use cells 0,1,2,3,3,3,3,3,2,1,0,0,0,0,0,0 with five segments flipped - the table really does rearrange and mirror the picture round the horizon. This part we can now reproduce exactly.

`+0x50` onwards in the file is not colours: it is `0, 0x219, 0, 0x233, 0.5182, 0x25c, 1.0`
followed by alternating `0xffffffff, 0`, i.e. a sentinel list. **The per-cell colours are therefore
built at run time, and where from is still the open question.**
- Final piece of the chain: `FUN_00460c60` just calls `FUN_00460b50` (a name-keyed resource lookup that returns an **index**, or -1). `FUN_00460c70(index)` then returns `*(this + 8 + index * 4)` - the resource's data pointer. So in `vfn03_00459d30` the `iVar7 + 0x50 + (i & 0xF) * 4` colours are **not** in the `.bin` at all: they live in the built `horz` resource object, alongside the 16 vertex positions at `+0x10`. From the exe string order the constructor's first lookup (`DAT_005cef74`) is `horz` and the second (`DAT_005cef6c`) is `hzcl`, with the per-track 6-byte table third.
- So the 16 per-segment colours are **generated when the `horz` resource is built**, using the per-track `.bin` as input, and the builder lives in the resource-loading path rather than in the sky classes. Not located. **This is the single remaining unknown for the sky colour.**

### Verified retail atlas layout (2026-10-15)
Evidence: read-only exported `decomp/classes/SkyHorizon.c`, procedure at 00459d30,
and `decomp/src/porsche_00530000.c` at 005327e0/005327f0 (turn-based sine/cosine).
These are static reference observations, not a runtime comparison or REA Evidence output.
REA tools were not connected; the existing export was sufficient for this bounded check.
Raw install header audit before rendering edits: `logs/horizon_reference.txt`.

| Field | Verified use |
|---|---|
| +04 | initial angle in turns; first original-space vertex X = sin(angle)*400, Z = cos(angle)*400 |
| +08 | initial resource colour/index lookup; **does not rotate atlas cell order or geometry** |
| +0c / +10 | atlas columns / rows |
| +14 / +18 | bottom height / vertical extent, relative to radius 400 |
| +1c + 2*i | row-major atlas cell for segment i, 0..15 |
| +1d + 2*i | nonzero swaps U endpoints |

Sixteen straight-sided quads step forward by 1/16 turn, closing at the first vertex.
For each cell, U spans column/columns to (column+1)/columns; V spans
(row+0.01)/rows to (row+0.98)/rows. Separate vertices at each quad preserve cell and
mirror discontinuities. The rewrite reflects X exactly as it reflects track geometry and
scales all coordinates uniformly inside its existing camera-centred sky dome. The cylindrical
radius is reduced according to the largest absolute height, so every corner stays within
95% of the dome radius and cannot be hidden behind the opaque dome.
The old guessed height and square-texel whole-image repeat are superseded.

Raw-file audit: 17 atlas headers (15 track sets, Monaco, default); two lightning files use
another format and are rejected. Monaco's 16 entries contain **three** flips (segments 8,
9, 12), not five as an earlier sentence claimed. Coastal has eight reversed segments.
Alps has 4x2 cells repeated twice, rather than the whole 256x256 image repeated twelve times.

Not established: retail per-segment colour builder, record-position interpretation, haze,
clouds, or visual agreement in the live viewer. The existing tint is still a guess.

### Horizon vertex-colour correction (2026-10-15, supersedes resource-name inference)
Direct PE virtual-address-to-file mapping in the installed `Porsche.exe` establishes:

| Address | Null-terminated resource name |
|---|---|
| 005cef74 | hzcl |
| 005cef6c | horz |
| 005cef14 | hor0, hor1, hor2, hor3 at six-byte intervals |

Earlier notes inferred those names in reverse from string order. That was incorrect.
The lookup at 0053c680 searches an SHPI directory and returns archive base plus entry offset;
00460c70 returns that cached raw entry pointer. Consequently, the horizon constructor's
reads at entry+0x10 and entry+0x50 refer to **the two pixel rows of hzcl**, not generated
horz resource colours and not the heuristically scanned .bin colour records.
All 17 sky archives contain hzcl as a 16x2 BGRA32 image (format 0x7d).
Raw numbers and executable SHA256: `logs/horizon_colour_reference.txt`.

For segment s, let i=(start+s) modulo 16 and j=(i+1) modulo 16. Quad colours in
(top-left, top-right, bottom-right, bottom-left) order are row0[i], row0[j], row1[j],
row1[i]. Cell order and flip flags affect UVs, never these colour indices.
The rewrite decodes BGRA with its SHPI reader and converts sRGB vertex colours to Bevy's
linear representation. Exact legacy gamma/interpolation parity is **not tested**.

This establishes the static horizon vertex assignment only. The .bin record meaning,
overall sky/time/weather behaviour, dome generation and the reported warm/cold effect
remain **unconfirmed**. Matching retail screenshots is still required for visual parity.

### Triangle batch boundaries and unresolved primitive kinds (2026-10-15)
Monaco1 live rendering emitted normal/position/UV/colour count mismatches because the
viewer concatenated every batch's corners, then computed normals in triples across the
whole material group. A trailing one/two corners could join the next independent batch.
The viewer now appends only complete triples from each batch, logs excluded corners, and
keeps the raw format reader unchanged. All 15 tracks pass attribute-size and complete-triple
checks. This prevents cross-batch triangles but **does not prove the excluded corners are
invalid game data**.

Raw indexed LOD0 Alps headers show all nine non-multiple-of-three parts have low header
word 1 (full first dword 0x02020001); ordinary triangle-looking batches have low word 3.
There are also four kind-1 batches whose counts happen to be divisible by three. Those
are evidence that a different primitive type may be incorrectly treated as triangles,
not evidence for blindly trimming the retail format. The kind-1 articles on Alps include
1729/1734, which are in the skipped template range for the current viewer.
Logs: `logs/track_indexed_primitive_headers.txt`, `logs/track_batch_diagnostics.log`.
**Next investigation:** verify primitive mode semantics in the reference and raw data,
then handle lines/strips/other kinds explicitly. Mode interpretation is **unconfirmed**.

### Verified indexed primitive topology (2026-10-15)
This supersedes the "unconfirmed mode-1" note above and the viewer's temporary tail trimming
as a topology solution. The former trim remains a defensive boundary check only.

| Static source | Observation |
|---|---|
| 0044a250 | Returns the signed low 16 bits of the primitive header as the submitted mode |
| 0053f1b0 / 0053f620 | Carries that submitted mode into renderer state and dispatches modes 1/3 separately |
| 00540ce0 (mode 1) | Enumerates n-2 consecutive triples and alternates index winding with triangle parity |
| 00543ae0 (mode 3) | Walks independent groups of three corners |

Thus mode 1 is a triangle strip, mode 3 a triangle list. An even strip triangle takes
corners (i,i+1,i+2); an odd one takes (i+1,i,i+2), cyclically equivalent to the reference's
reversed order. Degenerate triangles retain their place so parity never resets.
All vertex, UV and colour references must follow the same source-corner expansion. A strip's
source count still advances the raw colour cursor; the expanded triangle count must not.

Inventory of 59 installed CRP containers (15 tracks, 44 CarModel files), all indexed parts
and LODs: mode 1 = 497 batches / 9,832 corners; mode 3 = 89,665 batches / 4,226,040 corners.
No other indexed mode was observed. CarModel containers contain only indexed mode 3.
Monaco1 uses strips in a tunnel entrance, high walls, an overpass/building and an intersection;
these are visible track surfaces, not just traffic templates.

The Rust reader now normalizes both verified modes into independent triangle lists, keeping
all three corner attributes aligned. Unverified indexed modes are counted as skipped instead
of being guessed to be triangle lists. Nonindexed primitives remain outside this reader's
supported scope; their semantics are **not tested**.

Offline raw-header count and attribute-range audit: 34,171 meshes, 1,417,518 normalized
triangles across those 59 containers, zero skipped indexed batches. This is an indexed-geometry
check, not proof that every retail surface is rendered. Logs:
`logs/primitive_inventory.txt`, `logs/primitive_normalization_audit.txt`.
Fixtures verify strips with repeated indices, winding parity, UV/colour alignment, short
inputs and unsupported modes. Matched retail visual parity remains **not tested**.
