# In-race HUD: files, layout and behaviour

Findings for the in-race HUD (`crates/nfspu-formats/src/{hudcfg,loc,cardash}.rs`,
`crates/nfspu-game/src/{hud,hud_render,hud_plugin}.rs`). Sources: the shipped files on the
author's install, the decompile (read only, used as a map), the player's own saved files, and two
YouTube let's plays (`aL_I0E5QIR0`, Casual Gamer 4K, digital HUD at about 1280x720; `8JQ8djD_I6s`,
analog dials at about 1920x1080). "Verified" means checked against data or footage; "guess" means
not.

## Which files the game really uses

| File | Used? | Evidence |
|---|---|---|
| `FEData/Data/nfs5-1.hud`, `nfs5-2.hud` | yes, layout | exe loads `<savedir><player>1.hud` / `2.hud`, falling back to `XXhud1XX.hud` / `XXhud2XX.hud`; the stock copies are identical to `nfs5-1/2.hud` |
| `SaveData/<player>1.hud`, `2.hud` | yes, the player's copy | the user's `Greg1.hud` has a sixth record the game added (see anchoring below) |
| `FEData/Locale/hud.loc` | yes, HUD words | exe formats `%shud.loc`; same bytes as `GameData/DashHud/hud.loc` |
| `FEData/Data/nfs5.car` | yes, dial setup per car | values match `fe_out.txt` (see dials) |
| `GameData/DashHud/*.fsh`, `*.ffn` | yes, art and fonts | names appear in the HUD classes' constructors |
| `GameData/DashHud/hud.psh`, `ptext.eng` | **no** | `hud.psh` (an `SHPP` archive) and `ptext.eng` are not referenced anywhere in the exe; their contents (cop requests, "SPIKE BELT", Caprice, Helicopter) are NFS4 leftovers |

## Layout records (`...1.hud`, 480 bytes each)

One record per layout ("Config 1".."Config 4"), plus extra records the game appends when it lays
a config out for another screen size. Little-endian.

| Offset | Type | Meaning |
|---|---|---|
| 0x00 | char[20] | name, e.g. `Config 1` |
| 0x14 | u32 | translucency switch: only its top byte is used (non-zero = draw boxes and dark dial discs); the low bytes (0x000080 in Config 1) are **not** a colour, see "Box fill" |
| 0x18 | u32 ARGB | label colour (`b4b4b4`, grey) |
| 0x1C | u32 ARGB | second text colour |
| 0x20..0x30 | 5 x u32 ARGB | more colours (amber `fab400`, red `d70a0f`, ...), use not traced |
| 0x34, 0x38 | i32 | text padding x, y (5, 2) |
| 0x3C, 0x40 | i32 | unknown (3, 6) |
| 0x44, 0x48 | f32 | unknown (12.0, 0.08) |
| 0x4C, 0x64, 0x7C | char[24] x 3 | fonts: `UniversC12b.ffn`, `unviers9b.ffn`, `unviers9bi.ffn` |
| 0x94..0x104 | 11 x (i32 w, i32 h) | element sizes in pixels (see slots) |
| 0x104..0x10C | i32 x 2 | unknown (5, 4) |
| 0x10C | i32 | track-map grid spacing in pixels (20) |
| 0x110..0x11C | i32 x 3 | unknown (1, 8, 8) |
| 0x11C | u8 x 12 | per-element flags, not traced |
| 0x128..0x188 | i32 x 24 | unknown small numbers |
| 0x188..0x1E0 | 11 x (f32 x, f32 y) | element top-left as a fraction of the saved screen size |

Slots (size offset, element class from the decompile's RTTI name, Config 1 box at 640x480):

| Size at | Element | Config 1 box |
|---|---|---|
| 0x94 | gear (value + "gear") | 85x35 at (555, 444) |
| 0x9C | digital speed | 73x35 at (391, 406) |
| 0xA4 | race time ("time") | 73x35 at (427, 33) |
| 0xAC | damage as text | 124x100 at (0, 0) |
| 0xB4 | opponent list + position, one row per car | 138x15 at (502, 1) |
| 0xBC | damage as car picture | 127x105 at (0, 0) |
| 0xC4 | analog tach | 128x128 at (512, 306) |
| 0xD4 | analog speedometer | 128x128 at (384, 318) |
| 0xE4 | track map (also writes "lap X/Y") | 120x120 at (0, 358) |
| 0xF4 | leader picture | 59x85 at (125, 0) |
| 0xFC | rear-view mirror | 240x84 at (185, 0) |

Fractions become pixels by truncation (`0.6395833 * 480 = 306.99` gives 306). The player's saved
record for Config 1 shows exactly the truncated values.

## Switch records (`...2.hud`, 104 bytes each, same order as the layout records)

| Offset | Meaning |
|---|---|
| 0x00 | multi-purpose time box: i32 w, h, u32 flag, f32 x, y (73x34 at (427, 0) in Config 1) |
| 0x14 | digital tach box, same shape (73x35 at (520, 394)) |
| 0x28, 0x2C | i32 screen width, height the pair of records was saved for (640x480; one stock pair is 1600x1200) |
| 0x30 | speedometer: 1 digital, 2 analog |
| 0x38 | tachometer: 1 digital, 2 analog |
| 0x3C | damage: 1 picture, 2/3 text |
| 0x40 | map: 0 off, 1..3 styles; 0x44 map zoom step |
| 0x48 | opponent list on |
| 0x4C | multi-purpose time on (what it shows is the separate "split times" option: record lap / best lap / last lap) |
| 0x50 | value colour: 0 amber `ccfab400`, 1 red `ccff0606`, 2 green `cc1ee605` (alpha 0xCC); verified: footage A shows green, B red |
| 0x5C | rear-view mirror on |
| 0x64 | gear readout on |

The game picks the first record pair whose name matches the chosen config **and** whose screen
size matches the current one.

## Other screen sizes: anchored, not scaled (verified)

When no record exists for the current screen, the game copies the 640x480 one and moves each
box, keeping its pixel size:

- if the screen is wider than 640: a box that straddles x = 320 is centred; a box starting right
  of x = 320 moves right by `width - 640`;
- a box starting below y = 240 moves down by `height - 480`;
- boxes are then pulled back inside the screen.

Evidence: the user's `SaveData/Greg1.hud` has a sixth record, "Config 1", saved by the game at
1440x900 (its switch record says 1440x900). Every box in it equals the 640x480 box moved by the
rules above: tach (512, 307) -> (1312, 727), speedometer (384, 318) -> (1184, 738), mirror centred
at (600, 0), leader unchanged at (125, 0). The let's plays agree: at ~1920x1080 the 128-pixel
dials are about 1/15 of the screen width. Our `NFSPU_HUD_SCALE` (default 1 = retail) is an
addition, not retail.

## Dials (`nfs5.car`, verified against `fe_out.txt`)

`nfs5.car` is 109 records of 1648 bytes. Text fields: full name at +2, short name at +52, model
file at +127 (`993`), spec at +177 (`993targa36`), sound at +377. At +768, three dials of six i16
each: km/h speedometer, mph speedometer, tach. Each: face entry index, units, needle entry index,
start angle (degrees), thousandths of a degree per unit, needle pivot (pixels). The 993 has
(4, 300, 2, -40, 910, 18), (5, 200, 2, -38, 1353, 18), (7, 760, 2, -22, 310, 18); the front end
wrote exactly these to `fe_out.txt` (`speedostylekmh = 4`, `speedostartangle = -40`, ...,
`needlepivot = 18`) for the user's 993 race.

Drawing, from the dial class's constructor and draw call:
- art: tach face `tach.fsh` (night `tachN.fsh`), speedo face `speedK.fsh` / `speedM.fsh` (+`N`),
  tach needle `needle.fsh`, speedo needle `needle2.fsh` (+`N`). Entry 5 of a needle file (`cent`,
  20x20) is the centre cap, drawn only for needle 0 (the plain 5x74 needle);
- needle sizes (w x length) by needle index: 5x74, 19x60, 18x65, 17x71, 17x64;
- the dark disc `circletach.fsh` is drawn behind the face only when the box fill alpha is not 0;
- angle for value v (km/h, mph or rpm / 10), clamped to `units - 1`: `start + (units - 1 - v) *
  millidegrees / 1000`, 0 = pointing right, counter-clockwise; so the needle sweeps clockwise.
  993: 0 km/h at 232 degrees (lower left), 299 km/h at -40 (lower right). The game converts with
  3.14 instead of pi; we use pi (0.05% difference);
- the needle is a quad: half-width `(w + 1) / 2`, from `pivot` pixels behind the centre to
  `length - pivot` in front, tip = top of the picture. Centre = box top-left + half box size.

Footage B (944 Turbo S, 1920x1080) shows: transparent faces with thin black rings, red needles,
speedometer below-left of the tach, no speed digits inside the dials, and a red gear digit with a
grey "gear" label at the bottom right.

## Text boxes (gear, time, speed)

Value over label in the element's box: value in the value colour, right-aligned on the top half,
label grey, left-aligned on the bottom half; the black box (see "Box fill") behind it when the switch is not
0. Footage: "02:44.00" over "record", "01:46.14" over "time". Font: `UniversC12b.ffn` (both lines;
exact offsets of the label are a guess). Times are `mm:ss.cc`.
Gear: `hud.loc` "R" for reverse, "N" for neutral, digits for forward gears.

## Box fill (fixed after the first playtest: it was drawn blue)

We first filled boxes with the layout word at 0x14 read as ARGB, which for Config 1 (0x80000080)
is navy at 50%. The box code (tray draw, 0x434110) only tests that word's top byte. The fill is a
four-corner quad whose colours come from a fixed table in the exe: colour 000000 for all corners,
alphas 63, 63, 255, 255 (at 0x5B32E4 / 0x5B3304), scaled by a fade percentage (100 normally); a
low-end-card flag makes all four opaque. The corner order in the vertex setup (0x434310) is
top-left, top-right, bottom-right, bottom-left, which would put the faint edge at the top, but the
let's play (10:46) shows the boxes darkest at the top and lighter at the bottom over an even
background, so the 2D pass must have y pointing up. We follow the footage: black, opaque at the top,
25% at the bottom.

## Opponent list (verified against the let's play at 10:46)

One box at the opponent-list slot (138 wide; each row as high as the slot, 15 px) holding one row
per car in running order: a square in the car's body colour, the driver's name in grey (small
font), and the gap to the player right-aligned in the value colour (small italic font). Under a
thin grey line: "position" and "N/M" in grey, big font. Gap text by the "opponent list" option
(`opponentlist` in `fe_out.txt`, 1 for the user): 1 = progress difference in whole metres
("-384"; "-----" for the player and for 0); 2 = time, the metres divided by the faster car's speed
(at least half the other's), `-mm:ss.cc`, "-----" while the player is under 13.39 m/s. Car
colours whose channels are all below 0x14 are drawn 0x202020.

## Track map (`HTrackMap`)

- Box: the map slot (120x120 in Config 1), drawn into its own target. Translucency on: a dark
  backing quad (its colours are set at run time and not traced; we use black at 50%).
- Grid: lines every `grid` (0x10C, 20) pixels, fixed on screen, colour = the layout colour at 0x30
  (`4cffffff`, white at 30%). Verified against footage: a fixed 6x6 grid.
- Road: the track's slice centres (x, z), thinned by accumulated turning (a point is kept when the
  turn since the last kept point reaches 0.5% of the track's total turning, at most 400 points),
  joined by white lines within each road run. We thin by distance on screen instead (1 px).
- View: centred on the player, an orthographic scale of 1 / range with range = half the box width
  in metres: zoom step (switch record 0x44) 0/1 -> 185, 2 -> 322, 3 -> 459, 4 -> 600. Map style
  (0x40): 1 turns with the player's heading (Config 1), 2 keeps north (+z of the game files) up,
  3 shows the whole track. The let's play player used a north-up style.
- Cars: `radar.fsh` (`Car`, 26x63, front at the top) on a quad 20 times two of the car's size
  fields, tinted by its body colour, drawn from the last car to the player so the player is on top.
  Which size fields is not traced; our 20 x (0.42 m, 1.0 m) half extents are fitted to the footage
  (a car about 0.65 of a grid cell long).
- "lap X/Y" is written by the map element; its exact place is not traced (we use the box's top row).

## Words (`hud.loc`)

`LOCH` header (u32 header size 0x2C, version 1, 7 languages, 7 block offsets), a shared `LOCI` id
table (u16 id, u16 string index), and one `LOCL` block per language (u32 offsets from the tag,
zero-terminated Windows-1252 strings). Ids used: 10 gear, 11 km/h, 12 damage, 17 position,
18 time, 19 lap, 20 mph, 21 record, 22 record lap, 23 best lap, 24 last lap, 18144 rpm,
18145 R, 18146 N, 18143 CONGRATULATIONS, 18159 WRONG WAY. `hudstrings.h` beside it lists other
string ids but not these.

## Countdown (partly verified)

Retail runs it while an internal tick counter is between 256 and 448: three steps of 64 ticks
(tick rate not traced). Art: `circlefull.fsh` tinted `80fab400` (amber, 50%) and
`circleoutline.fsh` tinted `3ffab400` (25%), both 128x128 in the screen centre; digits from
`321go.fsh` (`2Sol`, `3Sol`, `3OL`, `2OL`) and `321go2.fsh` (`1Sol`, `1OL`), about 45x58. There is
no "GO" picture. Footage A (t = 623-627 s): the rest of the HUD is hidden, the solid digit sits in
the amber disc, and a large faded outline copy of the digit appears around it; each digit lasts
about 1.3 s there. Our curve (outline grows 1x -> 8x while fading from 50% to 0, one second per
digit) is a guess from those frames. The race code supplies the seconds left.

## Finish (not verified)

The congratulations element uses `circlefull`, `congrats1.fsh` (`cutc`, 128x128) and
`circleoutline` in the screen centre plus the finishing place: `congrats3.fsh` `1.tg`,
`congrats4.fsh` `2.tg` / `3.tg`, `congrats2.fsh` `4OL`..`7OL`, `congrats3.fsh` `8OL`. Timing and
any text are not traced; we show it for 5 seconds.

## Not done yet

Damage display, leader picture, rear-view mirror, wrong-way text, the two text lines the tach
class creates inside the dial (content not traced), digital tach/speed exact text offsets.
