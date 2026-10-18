# Car models: textures, cabin and the in-car camera

Findings from the car files in the player's install (`GameData/CarModel`), the engine's own data
(read-only `Porsche.exe` v3.5) and the decompile used as reference. Earlier car notes are in
`formats.md`; this file corrects several of them. Everything here is in our own words.

## Which texture page a triangle uses (corrects "the link is by kind")

A car material record (misc part `[index, 't', 'm']`) has its kind name at 0x10 (`CarExtEnv`,
`CarExt`, `CarInt`, ...) and, at 0x28, a **32-bit page number counted from 0**. Tracks keep a
4-letter texture name in that same field; in cars it was read as "empty" because the number is
small. The page number is an index into the `.tpg`'s `[tpageN.details]` list (page `N - 1`).

On the 928 (stored uncompressed, so easy to read by hand) the values are:

| Material kind | Page field | `.tpg` page | Filled from |
|---|---|---|---|
| CarExtEnv (body) | 0 | tpage1 | `exterior.fsh` (embedded archive 0) |
| CarExtEnv (body, other side) | 1 | tpage2 | `sourcetpage=1`: the same picture |
| CarInt (cabin) | 2 | tpage3 | `interior.fsh` (embedded archive 1) |
| CarExt, CarWheel (trim, wheels) | 3 | tpage4 | `extint.fsh` (embedded archive 2) |
| CarWindow | 4 | tpage5 | `glass.fsh` (embedded archive 3), `window=1` |
| CarInt (interior glass) | 5 | tpage6 | shared `intglass.fsh` (`global=2`) |
| CarWheel (wheel shadows) | 6 | tpage7 | shared `shadow.fsh` (`global=1`) |
| CarInt (driver) | 9, 10 | tpage10, 11 | shared `suit.fsh`, `head.fsh` (`global=3`, `4`) |

So the cabin, the steering wheel, the licence plate, the door handles, the soft top's window and
the driver each simply name their page. The earlier rules (page by kind, then exceptions by
article name or by which tile contains the UVs) were approximations of this field.

### How the engine fills the pages

Read from the car loader in the decompile (reading the `.tpg`):

- `[header] numtpages` pages are created, each `width` x `height` from `[tpageN.details]`.
- Each `fileN` names a texture file; `[fileN.details] tpage=M` says which page its tiles are pasted
  on, at `offsetx`, `offsety` plus each tile's own position. Tiles are filtered by the body style's
  `[fileN.tile]` rules (see `formats.md`). The paste is a plain copy.
- `global=K` means the file is not embedded in the car but shared by every car. The engine loads
  five shared files at start-up, in this order: 1 `shadow`, 2 `intglass`, 3 `suit`, 4 `head`,
  5 `cabrio` (a five-entry name list in the exe's data). Otherwise `fileN` is the car's embedded
  archive N - 1.
- A page with `sourcetpage=M` gets no tiles of its own; it reuses page M's picture under its own
  page number. Every Porsche's tpage2 is such a twin of the body page (`mirrortpage=2` on tpage1).
  What the engine does differently when drawing the twin is not settled (see below).
- Traffic cars use the same scheme with four files (exterior, interior, shadow, driver), so the
  special two-page atlas the loader used for them is no longer needed.

### The twin body page

Page 1 (`sourcetpage=1`) is used by exactly the same parts as page 0, with identical UVs, but on
the car's other side: on the 993 the page-0 `Body` batches average x = +0.59 in the file and the
page-1 batches -0.59. The engine's tile copy does not flip anything (the copy loop steps forward
through each row), so the twin shows the same picture. Every page carries a `damage` flag
(default on), so the most likely reason for the twin is that each side keeps its own damage
overlay; that part is a guess. The viewer draws page 1 with page 0's picture.

## Page pixel formats decide what alpha means

`[tpageN.details] bpp=` is a decimal code; without it the engine uses 1555 (the default value in
the code is 0x613 = 1555). Other values seen: 4444 and 32. The engine pastes tiles into a page of
that format, so:

| `bpp` | Alpha kept | Used for |
|---|---|---|
| 1555 (default) | 1 bit: alpha 0 is see-through, anything else is solid | body, twin, cabin, trim and wheels, driver |
| 4444 | 4 bits | shadow (`global=1`), cabrio window (`global=5`) |
| 32 | 8 bits | glass, interior glass, price tag; traffic cars' second page |

This explains two old puzzles: the body artwork's alpha (about 140-220) and the cabin artwork's
alpha (about 40-60) are not transparency, they are solid on a 1555 page. Before this, the cabin
pieces sampled "transparent" texels 3-100% of the time (the audit listed `Seat 280 of 280` on the
993) and were drawn flat grey.

## Colour bands: how paint, stripes and the cabin colour are applied

The engine's tile copy (in the decompile, the routine called for every kept tile of every file)
does more than copy when the file is tinted (`tint=` in `[fileN.details]`, overridable per tile in
`[fileN.tile]`, default on) and the target page is 1555 or 32-bit:

- For each texel it computes a step `(alpha + 1) * 20 / 256` (0..20).
- Steps 0, 10 and 20 copy the texel unchanged (alpha 0 = hole, alpha about 130 and alpha 255 =
  the artwork's own colour, e.g. the soft top, lamp lenses, exhaust).
- Steps 11-19 select colour **band** 8 down to 0, steps 1-9 band 17 down to 9.
- A banded texel becomes the band's colour multiplied by the texel's brightness (its red channel;
  car artwork is grey) / 256, and is solid.
- Paletted tiles (the shared suit and head files) get the same treatment through their palette.

The 18 band colours come from the chosen paint scheme. `<car>.clr [sectionN]` has four colours
and a `stripepackage` (counted from 0; values 0 and 2 occur). `CarModel/packages.ini` lists
packages `[package1]`..`[package8]`, each with `bandK=C` lines saying which of the four colours band
K takes (0 = `color1`). Bands not listed use colour 0, except bands 14 and 15, which use colour 3
(`color4`) - that default is in the engine's loader. So:

| Texel alpha | Band | Default colour | Seen on |
|---|---|---|---|
| about 200 | 4 | `color1` (body) | body panels |
| about 140 | 8 | `color1` | side panel |
| about 40-60 | 14, 15 | `color4` | seats, door cards, dash top (the cabin colour) |
| other bands | 1, 5, 6, 7, 12 | per stripe package | stripes and accents |

Example, 993 scheme 4 (red): `color1` (255, 0, 0), `color2` (0, 72, 106), `color3` white, `color4`
(197, 161, 98) tan, `stripepackage=2` = `[package3]`: band 1 takes `color2`, band 7 `color3`.
The cabin is therefore tan. A style with `stripe=1` also replaces band 6 with `color2` (read from
the code, not used yet).

Some `.tpg` files (930, 935, 944, gt1, the cop 930s) also have `[colorsN]` sections with 18
colours as signed ARGB numbers; what selects them was not traced. Traffic cars have no `.clr`
and are drawn with their artwork's own colours.

This replaces the earlier guess that tinted only the body colour by `alpha / 224`.

### Not settled yet

- Whether the in-race quick-race car uses the `.clr`'s `stripepackage` or a package the player
  picked in the front end.

## The in-car camera

- The `.tpg` `[header]` has `drivercampos=x,y,z` and `drivercamrot=x,y,z`, and some cars have
  per-style versions `drivercamposstyleK` / `drivercamrotstyleK`. K counts styles from 0: the 356 A
  has 12 styles and overrides `style0` to `style11`; the 928 and 550 have one style and override
  `style0`. Without any entry the engine uses position (-0.36, 0.4, -0.25) and no rotation.
- The engine keeps one offset per camera mode in a 16-byte-per-entry table in its data
  (0x5CF610 in `Porsche.exe` v3.5). Entry 1 (the in-car view) starts as (0, 0.4, 0.9) and is
  replaced by the car's `drivercampos` when the car is loaded. The chase entries have negative z
  (for example (0, 1.15, -3.3) and (0, 2.0, -6.0)), so in the engine's car frame **+z is forward**,
  while the model files face -z: the car frame is the model turned half a turn. The x of the
  driver camera (-0.38 on the 993) therefore lands on the model's +x side, where the steering
  wheel is (+0.39), which agrees with the handedness note in `formats.md`.
- The camera position is the car's position plus the offset turned by the car's orientation
  (read from the code). Where the car's position sits on the model is an **inference**: the engine
  stores half extents of a model's bounding box (`(max - min) / 2`), and when it converts the
  model's bound points into the car frame it subtracts the half height from y only. So the car's
  origin is taken to be half the model's height above the model's origin (the ground; the wheels
  touch y = 0). The eye is then `half height + drivercampos.y` above the ground,
  `drivercampos.z` forward and `drivercampos.x` to the side. Our loader takes the half height from
  the exterior model's top: 993 = 0.63 + 0.40 = 1.03 m up and 0.155 m behind the car's middle,
  logged as `eye [-0.38, 1.03, 0.16]`. Which level of detail the engine measures was not traced.
- `drivercamrot` is in degrees (the engine multiplies it by 1/360 of a turn) and is applied as a
  turn about x, then y, then z on top of the car's orientation. x is a pitch; with the engine's
  left-handed axes a positive value tips the view down (3.65 degrees on the 993). The y value is a
  small yaw (under half a degree on every car); its sign after our mirror is a guess.
- The field of view used for the in-car view has not been traced separately from the chase view.

## Faces are one-sided

The cabin has its own inward-facing panels (`Interior`, `InteriorRoof`, `DoorIn`) and its own
inside glass (`InteriorGlass` on the shared `intglass` page), and wheels have separate outer and
inner faces. That only makes sense if the engine culls back faces, so the viewer now draws every
car part one-sided. The file's front faces are clockwise; after our x mirror they are
anticlockwise, which is Bevy's front.

## Which articles are drawn

The old loader skipped every article whose name starts with `Decal`, `Gauge`, `Needle`, `Cap` or
`Ant`. Those include the gauge faces (`DecalGauge`, on the cabin page), the cabin badges
(`DecalInterior`), the trunk lettering (`DecalTrunk` types 0-5, one per badge text), the fuel cap
and the aerial. All of them are alternatives picked by the body style's piece types, and a decal
whose artwork the style does not use lands on transparent texels, so they are now drawn. Only the
wheel shadow quads (`WheelShadow*`, shadow page) and the passenger stay out. Decals lie on the
surface below them (the 993's gauge faces are 3 cm in front of the instrument panel, the badges on
the body skin) and are lifted 5 mm like handles and plates.

## Audit (all 44 car files, `cargo run -p nfspu-game --example car_audit`)

Counts are triangles whose texels are mostly see-through on the page their material names.
Before this work, with pages chosen by material kind and alpha below 50% treated as a hole:
Porsche cabins were the worst (993 `Seat` 280 of 280, `Steer` 168 of 168, `Dash` 109 of 128;
every Porsche flagged several cabin parts) and the cabin was drawn flat grey.
After (`logs/car_audit_after.txt`): no Porsche part is flagged. Remaining notes:

- `356b`, `cop356`: 2 downward-facing lamp triangles land on transparent lamp corners (same in
  the game, which cuts those texels out too).
- Traffic cars (`eas*`, `eden*`): their seats, steering wheel and some cabin panels sample texels
  with alpha below 50% on their 32-bit second page. Not checked against the game; seen only
  through the windows.
- "Outside cabin pokes through the body" on several cars: the simplified cabin used for outside
  views is up to a few cm wider than the body at the doors. Its panels face inwards and are drawn
  one-sided, so from outside they are culled; no visible effect found in the captures.
- `postship` is not a car (wheels, bottom and a decal only).

## Verification (2026-10-17)

Captures with `NFSPU_CAR_CAPTURE=993,959,boxster,356a,901,993/6` (our viewer on alps; it takes a
chase, cockpit and four orbit shots of each car and exits) are in `logs/car_<car>_s<style>_*.png`.
Retail reference frames from the let's play (chase view only, the player never uses the in-car
view) are `logs/ref_letsplay_t700_356_chase.jpg` and `ref_letsplay_t7200_911_chase.jpg`, and
side by side with ours in `logs/compare_356_chase.png` and `compare_911_chase.png`.

Verified by eye in the captures:
- 993 coupe and cabrio (soft top up), 959, Boxster, 356 A cabrio, 901: no missing textures and
  no gaps or see-through panels from any of the six views; badges ("Carrera", "PORSCHE 911") read
  the right way round; lamps, plates, handles and wheel faces are textured.
- The cabin shows in the cockpit view with its artwork in the scheme's `color4` (tan in the red
  993 scheme, black on the 356 A and Boxster schemes): seats, door cards, dashboard, console,
  vents, gauge faces and steering wheel.
- Body detail level matches the retail frames (same panel art, grille, plate, lamps).

Not verified:
- **The in-car camera against the game.** No retail in-car footage was available (the let's
  play uses the chase view only; the user's recordings show menus). Position and pitch come from
  the `.tpg` and the engine code as described above; the field of view is still Bevy's default
  (45 degrees vertical).
- Gauge needles are drawn where the model puts them (not animated).
- The stripe package used in a real quick race (see "Not settled yet").
- Traffic cars' cabins.

Differences seen against the retail chase frames that are outside the car model: the retail
chase camera sits lower and closer behind the car, and the retail car casts a shadow (the
`WheelShadow` quads on the shared shadow page, which we leave out).
