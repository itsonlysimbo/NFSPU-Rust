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
passenger stays out; the wheel shadow quads are drawn too (they shade the tyre tops, see "Wheel
shadows are not a ground shadow" below). Decals lie on the
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
  the `.tpg` and the engine code as described above; the field of view is now the engine's (see
  "Field of view" below), not compared with the game.
- Gauge needles are drawn where the model puts them (not animated).
- The stripe package used in a real quick race (see "Not settled yet").
- Traffic cars' cabins.

Differences seen against the retail chase frames that are outside the car model: the retail
chase camera sits lower and closer behind the car, and the retail car casts a ground shadow.
This was first taken to be the `WheelShadow` quads; it is not (see the next section, which covers
the camera, the wheel shadows and our ground shadow).

## Chase camera and ground shadow

### The camera table

`Porsche.exe` v3.5 keeps 20 camera entries of 16 bytes at 0x5CF610: an offset (x, y, z) in the
engine's car frame (+z forward, origin at the car's centre) and a flag word. The flag bits are
copied into the camera's state when a view is chosen; the ones traced are bit 0 (raise the aim
point, below), bit 2 (aim at the car) and bit 3 (keep the camera out of the scenery).

| Entry | Offset (x, y, z) | Flags | Update routine | Use |
|---|---|---|---|---|
| 0 | (0, 0.4, 0.9) | 0x02 | fixed to the car | bumper/hood (not traced further) |
| 1 | (0, 0.4, 0.9) | 0x00 | fixed to the car, `drivercampos` replaces the offset | in-car |
| 2 | (0, 1.15, -3.3) | 0x2D | chase, no slope lift, fixed follow | chase (close) |
| 3 | (0, 1.35, -4.5) | 0x2D | chase, slope lift 0.25..1, speed follow up to 0.1 | chase (medium) |
| 4 | (0, 1.15, -4.8) | 0x2D | chase, slope lift -0.25..2, speed follow up to 0.05 | chase (far) |
| 5-7 | (0, 2, -6), (0, 2, -6), (0, 4.9, -6.7) | 0x2D | a second chase routine, not traced | high chase views |
| 8-19 | various | | replays, TV and other views | not used here |

The C key steps through four entries kept in the player profile (the `_CAMERA` key of the save);
the default list was not found (the profile is filled from the save file, and Ghidra was not
running to trace it). **Which entry the let's play uses is an inference**: entry 3 matches the
recording's framing best (below).

### How a chase view (entries 2-4) moves (read from the code)

All of this runs on the game's 64 Hz tick (the camera update is registered on the per-tick task
list), so the shares are per 1/64 s.

- Wanted offset: the entry's offset with **the car's half length taken off z** (the car's half
  extents are kept with the car; half length is the third), plus a slope lift on y, turned by the
  car's orientation into world space.
- Slope lift: the road height 3 slices ahead minus the height of the car's slice, over 3 (the rise
  per slice, about 6 m apart), clamped to the entry's range: none for entry 2, 0.25..1 for entry 3
  (so even on a flat road it adds 0.25 m), -0.25..2 for entry 4. "Ahead" is chosen from the car's
  direction flag and the track's direction; we use the slice direction the car points along.
- The stored offset is in world space and moves a share of the way to the wanted one each tick, so
  it trails the car's turns: x and z by the follow share, y by 0.05.
- Follow share: 0.1 for entry 2. Entries 3 and 4 follow by speed: take the car's velocity along
  its forward and side axes, `s = max + 0.25 * min` of their sizes (m/s), share = s / 900,
  clamped to 0.02 .. 0.1 (entry 3) or 0.02 .. 0.05 (entry 4). A standing car's camera swings round
  slowly; at 90 m/s it reaches the cap. (One game mode keeps 0.1; not traced which.)
- After the offset moves, if it is shorter than 4.94 m it is stretched to 4.94 m.
- For 50 ticks after a view switch both shares are 0.1 and the 4.94 m rule is off. Switching in
  from a non-chase view first puts the offset 4 m straight behind the car's centre.
- Camera = car centre + offset. The scenery check (bit 3) then keeps the camera at least 1 m above
  the surface under it (on every track except one named "castle", where it only applies near the
  car's height). A second scenery test (wall push-out) was **not implemented**.
- Aim: the car's centre (not the lagging camera's own direction), raised: the aim direction is a
  quarter of (car - camera) with 0.325 added to its height, which is the same as aiming 1.3 m
  above the car's centre (1.6 m for entries 5-7). Up is world up.

### Field of view (read from the code)

Every camera starts with the value 37.5 (degrees). The projection setup doubles it and takes the
tangent of half, so 37.5 degrees is **half the horizontal angle** (75 degrees across a 4:3
screen); the vertical scale is multiplied by 640/480 for the full screen, which gives square pixels
and about 59.8 degrees vertically. (The value is multiplied by 1.2 for the outside views when one
global flag is set; not traced which option that is.) Bevy's default is 45 degrees vertical,
which made our view much narrower. The chase and cockpit views now use the engine's angle;
for screens wider than 4:3 we keep the vertical angle (an assumption; the recording, which is
16:9, fits it). `NFSPU_RETAIL_FOV` still selects the reference build's numbers instead.

### Matching the recording

With a 59.8-degree vertical view, the recording's 356 frame (`logs/ref_letsplay_t700_356_chase.jpg`,
picture 317 px high) has the bottom of the car's rear about 108 px below the centre and the car
about 100 px wide. Worked out for each entry with a 4.0 m long, 1.3 m high car on a flat road:
entry 2 puts that point about 134 px down, entry 3 about 104 px, entry 4 about 93 px. Entry 3 also
gives the closest car width. So mode 0 ("chase") now uses entry 3; entries 2 and 4 are in
`chase_cam.rs` as well.

### Wheel shadows are not a ground shadow

`WheelShadowFront` and `WheelShadowRear` (geometry ids 13 and 14, one type per body style) are
ordinary articles whose material (kind `CarWheel`) names page 6, the shared `shadow.fsh`
(`global=1`) on a 64 x 64 4444 page (`cargo run -p nfspu-game --example car_shadow_info -- 993`):

- Each is one **upright** square of two triangles per wheel, the wheel's size (0.63 m on the 993,
  0.58 m on the 356 A), in the plane x = +-0.81 (993 front; 0.88 rear), just outside the wheel,
  from the ground (y = 0) to the wheel's top. The same quad is used at every level of detail.
- Its UVs span the whole page, with v = 0 at the top edge.
- The page is dark grey (mean 30, 30, 30) with alpha only in its upper half: an arch-shaped patch,
  strongest (about 7/9) at the top and fading out half way down.

So the quad darkens the top of the tyre, where the wheel arch shades it, and nothing on the
ground. We now draw these quads where the model puts them, blended by the page's alpha and seen
from both sides (not compared with the game close up).

### Ground shadow

Traced in the plain decompile of `Porsche.exe` v3.5 (2026-10-19; Ghidra was not running, so
data cross-references were followed by searching the text). The car's ground shadow is **not**
cast by a sun and **not** the shadow-map layer: it is a dark rectangle the size of the car, laid
on the road under it.

How the engine draws it (the routine at 0x43B820, called from the car's draw step just before
the body):

- **Shape.** Four corners at (±half width, ±half length) in the car's own frame, turned and moved
  with the car (its orientation and position). The half sizes are half of the car model's
  bounding box (the box of one of its detail levels, measured when the car is loaded; a flag in
  the car's data shrinks them to 0.4, not traced which cars set it). The same three fields are
  also written by the physics set-up, as half the extent of the car's collision points; which of
  the two runs last for a race car was not traced. The rectangle is centred on the car's
  position, not on the middle of the model.
- **On the road.** With the camera within about 21 m the rectangle is split into a grid of
  `6 - round(d² / 100)` cells each way (6 up close, at least 2), otherwise it is one quad. Each
  grid point is looked up on the road from 1.75 m above the car; if the road found there is
  1.5 m or more off the car's own ground, the car's own ground plane is used instead. For the
  single quad each corner drops onto the ground plane under the nearest wheel. So the shadow bends
  over crests and kerbs instead of floating flat.
- **Texture coordinates** run 0..1 across the rectangle (u along the width, v along the length).
- **Picture.** The texture is looked up by the four-letter name `SHAD` in the renderer's shared
  bank, which is `GameData/Render/common.fsh` (17 pictures: flares, sun, glints, splashes and
  this). `SHAD` is 64 x 64 in 4444: every texel black, alpha 8/15 (0.53) over the whole picture,
  with the outer one or two texels softer. So the shadow is a flat, even darkening with slightly
  soft edges, not an outline of the car.
- **Draw state.** Drawn blended with a depth offset that grows as the camera comes closer (the
  value passed is proportional to `-20000 / d²`; its exact use by the renderer was not traced).
- **When.** A per-car switch turns it on for every race car (the garage turns it off). The
  player's own car skips it in one camera mode (a camera flag bit; most likely the in-car view,
  not confirmed). In one game mode (not identified; replays are a guess) the grid is always 6.

What the related pieces are, for the record:

- **The shadow-map layer (`R3DShadowMapTextureLayer`, layer id 0x8102).** One of the car's own
  material layers, alongside the environment map (0x8100), the sky map (0x8101) and two lighting
  layers. Each frame the car looks up the nearest "type 4" article record of the track (within
  the record's radius plus 2.5 m, octagonal distance on the ground plane); each record holds a
  position, a radius, a texture and a 2 x 4 projection. That projection gives the **car's**
  vertices a second set of texture coordinates from their world positions, so the car picks up
  the track's baked shade (trees, bridges) as it drives through it. It does not shade the road.
  It needs the highest car-detail setting and is skipped when the track's records are switched
  off. Not implemented.
- **The `WheelShadow` quads** (see above): the tyre tops under the arches.
- **The car lighting direction** is the track's sun position (from the track's sky settings:
  two offsets turned by the sky's angle, height from a paraboloid) normalised and negated, or
  straight down under cover (tunnels). It lights the car only.

The "offset to the right rear" in the recording's frames is most likely the rectangle itself
(our reading, not tested in the game): it lies on the ground as long as the car's box, so from
the raised chase camera its rear edge shows below the bumper, and when the car is turned
relative to the camera the near rear corner pokes out to that side. Zoomed in, its edges are
straight and its darkness even, as a flat rectangle would be.

Ours (`car_model::GroundShadow`, `lay_ground_shadows` in `main.rs`) follows that: the rectangle
is half the model's extent each way round the model's origin (the model's box without wheels,
driver or shadow quads; the engine's box comes from a detail level whose exact contents were not
checked), split by the same camera-distance rule, each grid point dropped onto our ground mesh
from 1.75 m above the model's ground unless the surface there is 1.5 m or more off (then the
model's own ground), raised 3 cm, painted with `SHAD` read from the player's install and drawn
blended with a depth bias. The single-quad case uses the same road lookup instead of the wheels'
ground planes. In the cockpit view the player's shadow is hidden.

Blending: Bevy blends in linear light, the engine's renderer (like every renderer of its time)
on screen values. A black overlay of alpha `a` on a dark road changes the screen value much less
in linear light, so both shadows convert their alpha first: `1 - (1 - a)^2.2` (the screen value
then drops by about the factor `1 - a`, as it would have in the game). For `SHAD`'s 136/255 the
screen value is kept at about 0.47 of the road's.

### Verification (2026-10-18)

`NFSPU_CAR_CAPTURE=993,901` on alps (the capture now waits 1.5 s on the chase view so the camera
has swung in). Side by side with the recording: `logs/compare_993_vs_356_chase.png` and
`logs/compare_901_vs_911_chase.png` (reference frame left, ours right, same size).

- Camera framing: verified by eye and by measuring. 901 against the recording's 911: car width
  0.27 of the picture height in both; bottom of the rear bumper 0.35 below the centre (ours) and
  0.34 (recording). The 356 frame is mid-corner (the car sits left of centre because the camera
  trails the turn), which ours does not show in a still capture.
- The trailing in corners, the speed follow, the slope lift and the 1 m ground clearance are
  covered by unit tests only; **not tested** against the game in motion.
- Ground shadow (redone 2026-10-19 from the trace above; `logs/compare_901_vs_911_shadow_zoom.png`
  and `logs/compare_993_vs_356_shadow_zoom.png`, the recording left, ours right, both cropped
  round the car): ours is now the same kind of shape as the recording's, a flat, even,
  straight-edged dark band that shows past the rear bumper and along the sides. In the recording
  the band reaches somewhat further behind the car and spills further to one side; both frames
  have the car turned relative to the camera (the 911 shows its left flank, the 356 is
  mid-corner), which moves the rectangle's near corner out that way, while our still capture is
  straight behind. That explanation and the exact size (the engine's box may also be the
  collision hull's, which is set from a second place; not settled) are **not tested**. Still hard
  to see on alps, where our road renders much darker than the recording's grey asphalt. Bending
  over crests and the grid steps with distance are covered by unit tests and by reading the code
  only; **not tested** in motion.
- Wheel shadow quads: drawn, but too dark against our black tyres to check by eye; **not tested**
  against the game.
- Field of view: the engine's 75 degrees across at 4:3 kept vertical on wider screens is an
  assumption for widescreen; the recording (16:9) fits it.

## Smooth shading: the files carry per-vertex normals (2026-10-20)

Playtest note: "the 356 and other car models are very polygonal". We shaded every triangle with
its flat face normal. The car files have more:

| Part tag | Size | Holds |
|---|---|---|
| `[lod, 0, 't', 'v']` | 16 bytes per entry | vertex positions |
| `[lod, 0, 'm', 'n']` | 16 bytes per entry, same count as the vertices | **one unit normal per vertex** (x, y, z, unused) |
| `[lod, 0, 'n', '$']` | 16 bytes per triangle | a plane per triangle (normal and distance) |

In every triangle part checked (all parts of the 928, 993, 356 A, 914 and eas1), the info row
of kind 1 (normals) starts at the same offset as the row of kind 0 (vertices). So a corner's
normal is the normal with the same index as its vertex. The modellers' shared vertices therefore
give smooth shading, and hard edges are where vertices are split (two vertices, same place,
different normals). Nothing is computed with a crease angle.

The loader now gives every corner the file's vertex normal. It is turned by the part's matrix
and mirrored in x like the positions, and the viewer's baked lighting uses it. The flat face
normal is kept as a fallback where a part has no normals, or where a vertex normal points away
from its triangle; the audit prints how many corners used each.

### Body reflections (2026-10-22)

Retail draws the paint (`CarExtEnv`) with two extra layers, both computed per vertex on the CPU and
added to the colour. Both pick one of three detail levels from the `Graphics.Reflections` option
(a 0-1 slider: below 0.05 level 0, from 0.95 level 2, otherwise 1). With the option off the glow
textures are not set and the glow layer is skipped.

**Sun highlight.** The light direction is mirrored about the vertex normal and compared with the
direction from the car to the camera (one direction per car, not per vertex). Above zero the
cosine is raised to the 8th power and multiplied by the car's light colour (the light colour
times the track's `.map` colour) and a per-level strength, then added. The level table holds
0.2, 0.8 and 1.0 (same for red, green and blue).

**Light glow.** The `.map` file's `[sectionN]` blocks (already parsed as `ReflectionSection`)
give a slice range, three texture names per level in an "open" and a "covered" variant, a
`texturesize` and an `aspect`. The names are four-character codes stored as numbers, and they
name pictures in the track's `<track>.env` (`1` black, `2`/`3` clusters of lights, `BT1`/`BT2`
blue and `YT1`/`YT2` yellow lamp glows, `TT*`, `WT*`, `B2T*`, `T1M*` on other tracks). So the
`.env` pictures are not sun sprites but the lamps' reflections. The car looks up the section of
the slice it is on. A body point, pushed along its normal by a strength, is taken relative to
the slice's centre; its world x divided by `texturesize` and its z times `aspect` / `texturesize`,
plus 0.5, are the picture coordinates. So the glow is a patch laid flat around the slice
(5-7 m across, about a fifth as deep on lamp sections) that slides over the body as the car
passes. Section counts per track: alps 39, autobahn 63, canyon 69, castle 19, coastal 24,
farmland 4, foothills 14, forest 13, industrial 29, monaco1-5 4/4/21/23/23, skidpad 1.

**Ours** (`crates/nfspu-game/src/car_shine.rs`): a small material for the outer paint pieces
(page texture times the baked light, cut-outs kept) whose shader adds both layers per pixel, at
the top detail level. The glow follows each car's own nearest slice; the sun highlight uses
`.map` `tint` as its colour. The car meshes now carry their normals for this.
`NFSPU_SHINE=<gain>` scales both layers for tests, `NFSPU_SHINE=0` turns them off.

Guesses, to check against retail:
- The normal push (we use 0.5 m): the engine reads it from a field we did not see set.
- Sun direction: we use the baked light's (up, to one side); retail's comes from the track's
  light, which we do not read yet. At night we scale the highlight by 0.35.
- Glow blend: we add the picture's colour times its alpha. Two small offsets the engine adds to
  the picture coordinates (from distances computed with the slice) are left at 0.
- **Covered flag (traced 2026-10-23).** The car's per-frame lighting update asks the track for the
  zone object above the car (a list of zone objects on the track, nearest one above, tag 1) and
  reads a u16 at +10 of it; `covered` is that value `& 0xC000 != 0`. Covered picks the covered
  picture of each level (index `level + 3` in the section record) and lights the car straight
  down. Which file holds the zone objects is **not found**, so we stand in: a slice is covered when
  a flat down-facing face (tunnel roof, bridge underside) lies 2-15 m above it
  (`track::covered_points`; **guess**). Monaco 3-5 use glows only in the covered variant.
- Only the paint shines; glass and trim get neither layer (not checked what retail does).
- Per-pixel view direction instead of retail's one direction per car.

Checks: `logs/refl/before/` and `logs/refl/g1/` (farmland, 993, `NFSPU_SHINE=0` vs default): a
soft highlight on the roof and rear wings in the chase view. `logs/refl/g20/` shows the same at
20x gain, to show where it falls. The glow was **not tested** by eye (the viewer starts on slice 0,
which has no glow); the log line `car glow: slice N, picture ...` shows when a car enters a glow
section.

## Paint colours are stored as H/S/V (2026-10-20)

Playtest note: the 914 offered "only white and black". The 914's `.clr` has 20 sections, and all
20 were listed, but only `[section1]` has real `colorK.r/.g/.b` numbers. Sections 2-20 write 0
there and keep their colours only as `colorK.h/.s/.v`. Each of those is 0-255, with hue 255 a full
turn. Section 1 shows the two forms agree: h 51, s 13, v 255 is RGB (252, 255, 242), and on the
993 h 141, s 255, v 106 is (0, 72, 106). We read only RGB, so 19 of the 914's paints came out
black: the swatches and the cars both. Both readers (`car_model::parse_clr` and
`catalog::parse_paints`) now use H/S/V when a colour has all three values and RGB otherwise.

### Custom colour (ours, not retail)

After the model's own paints, the colour list has a `CUSTOM` title and 42 more swatches (12 hues
at three strengths, six greys). Picking one keeps the model's first paint scheme for stripes and
cabin and replaces only the body colour. It travels in the race setup as `custom_colour = r,g,b`
(`none` otherwise; older setups without the line still load) and reaches the garage car and the
player's race car through `CarOptions::body`. Opponents keep their own retail paints.

### Checks (2026-10-20)

- Audit (`logs/car_audit_after.txt`): 97-98% of corners now use the file's vertex normals (993
  6,164 smooth / 163 flat; 356 A, 901, 914, 959 and Boxster similar).
- `logs/compare_356a_smoothing.png`: the 356 A front before and after (before was also the old
  black paint). The bonnet and wing curves are now shaded smoothly.
- `logs/compare_914_paints.png`: the 914 in section 1 (white, red stripes), section 7 (yellow,
  red and green stripes, red cabin; black before the H/S/V fix) and a custom blue.
- `logs/menu_colour_list.png`: the quick-race colour list for the 356 1100 shows its 21 real
  colours (it showed black bars). `logs/menu_custom_colour.png`: a custom blue picked from the
  palette, on the garage car and in the closed colour box.
- Race: pressing RACE from that setup writes `custom_colour = 0,0,255` and the race logs
  `race car '356_1': paint section 1, custom body colour Some([0, 0, 255])`. The race car was
  checked by log only (not tested by eye).
- Not tested: the palette rows' look in the open list below the retail paints; retail frames to
  compare the smooth shading with (the let's play is at too low a resolution to judge it).

## Cockpit driver and steering wheel (`Steer`, arms and hands), 2026-10-22

From the 993's article list (`car_articles`) and the loaded model (`car_part_info`):

- The detailed cabin (level 1) has its own `Steer` article: the steering wheel, 168 triangles. It
  is separate geometry from the dash and can be turned on its own.
- The detailed driver (level 1) is **not** the `LODDriver` article used for the exterior: it is
  split into `DriverBodyN`, `LegsN`, `LeftArmN`, `RightArmN`, `LeftHandN`, `RightHandN`, where N
  is the driver type (1 = racing suit and helmet). The arms are 28 triangles each, the hands 257
  each. The hands are authored on the wheel rim (on the 993, the right hand on the upper-right of
  the rim and the left on the lower-left), so showing them with the wheel looks like hands on it.
- Our cockpit draws the arms (static, low-poly) and the hands (on the wheel pivot, so they turn
  with it), and turns the wheel around its column. The column axis is the mean of the wheel's face
  normals from the geometry: on the 993 about `(0, 0.28, 0.96)` in car space (mostly +Z, the wheel
  facing the driver and tilted up). The wheel turns about 90 degrees at full lock (**guess**, not
  traced). The drawn wheel follows the input at a limited rate (full lock in about 0.6 s) because keyboard input is 0/1 and used to snap it to 90 degrees. Hands ride the rim, arms stay static (guess). `NFSPU_STEER=-1..1` forces the input for captures.
- Arms (2026-10-22): each arm article is a shoulder block (about 8 triangles) and a forearm block near its hand, joined by a few middle triangles. Static arms detach from hands that turn with the wheel, so the cockpit blends each arm vertex between fixed and wheel-rotated by its position along the shoulder-to-hand line (fixed below 0.35, fully turned above 0.85). Retail's real arm animation is not traced (the Ghidra strings have no arm or hand names), so this is a guess. Captures: `logs/hands_steer_{-1,0,1}.png`.
- The exterior view still draws the `LODDriver` and the level-3 cabin's own static wheel; the
  detailed arms and the turning wheel show only in the cockpit view. **Not playtested.**

## Visual damage (`<car>D.fsh`), 2026-10-22

Each car has `<name>D.fsh` (8-bit paletted pictures). Entry names are the damage zone ("0000" to
"0008"); the header word at offset 8 is the page (0 the body page, 1 its twin, the car's other
side: this settles why the twin page exists), the position fields are where the picture sits on
the 256 x 256 page. Retail copies the page and paints each picture over it at alpha zone damage x
3.5 (capped at 255); an entry with a threshold is drawn solid once the zone passes it (not seen in
the 993's file). `[damagezones]` in the `.tpg` gives the zone box sizes (width, length, front width
and length in percent, corner lengths, bottom, height); not used by us yet. `<name>L.fsh` holds a
similar set (purpose unknown; perhaps a second level of damage). Ours paints `D.fsh` the same way
(`car_model::paint_damage`), repainting when a zone changes by 2 or more.

## Roof up and down
Open cars carry the folding top as piece 41 with paired types: `CabrioUp` (type 1, or 3 and 5 on the
356 A/B variants) and `CabrioDown` (the next type: 2, 4, 6). The body style's `.tpg` lists the up
type, so roof down = that type + 1 (`CarOptions::roof_up`, `piece_type`). Checked by article lists
on 356_1, 356a, 356b, 944, 964, 993, 996 and boxster: every one follows the pairing. Piece 37
(`Cabrio`, the body) is the same both ways. Coupes list no piece 41, so the setting does nothing there;
targa roofs (piece 7 and 8) have no up/down pair. Garage, race and cockpit use the same selection
(the cockpit camera sits inside the same model). Which pieces move, and the folding animation, are not
traced in the decomp (not tested). Captures: `NFSPU_CAR_ROOF=down` with `NFSPU_CAR_CAPTURE=993/6`
writes `logs/car_993_s6_roofdown_*.png`.

## Camera angles seen in the 2026-10-23 race footage (GT1, Cote d'Azur)
Frames in `extracted/footage/race` (not in the repo). Seen: chase medium and far (car low in
frame, road ahead), a high close chase looking down at the car from above and behind, and the
cockpit (wheel with both hands near 10 and 2, forearms entering from the lower left and right,
wiper visible). These match the four table entries in "The camera table"; position, FOV and lag
were **not** measured from the footage and our hands/arms were **not** compared (not tested).
Still to measure (footage local only): per-view camera position (car width in frame gives distance),
FOV and lag (frames after a steer input until the view follows); cockpit hand pixel positions on
the wheel and forearm angle against ours (`main.rs` arm skinning, `car_model.rs`). **Not measured.**

### Measured 2026-10-23 from the race video (GT1; fractions of the 1436x920 game window; by eye)
- **Cockpit hands** (wheel centred, 180-210 s): left hand at about 8:30-9 o'clock on the rim, right
  hand about 3-3:30, both palms over the rim, thumbs inward; forearms enter from the lower-left and
  lower-right corners and run up to the hands. With the wheel turned right the right hand rides
  to about 11-12 o'clock and the left to about 7: the hands stay on the rim and turn with it
  (no hand-over-hand). Wheel centre sits at about 0.50 of the width and 0.73 of the height.
- **Chase (near/medium)**: the GT1 shows about 0.23 of the window width, its centre about 0.80 of
  the height, the horizon about 0.45; wing level with the camera slightly above it.
- **Chase (high)**: camera above and behind looking down; the car fills about 0.3 of the width at
  about 0.85 of the height; road ahead fills the frame.
- **Cockpit**: wiper at upper left, mirror/pillar at right, dash gauges centred.
- FOV and lag are **not** measurable from single frames without a reference: **not measured**.
  The table in "The camera table" and `engine_vertical_fov` stay as the best evidence.

### Our captures against the 2026-10-23 numbers (`NFSPU_CAR_CAPTURE=GT1`, `logs/car_GT1_s1_{chase,cockpit}.png`)
- Chase: car width 0.22 of the window (measured 0.23), horizon about 0.47 (0.45): fine. Car centre
  about 0.72 of the height against 0.80 measured (not changed: one still against a moving camera).
- Cockpit: our wheel hub sits at about 0.86 of the height, 0.52 of the width, against 0.73 / 0.50 in
  the footage, and our right arm/hand shows as a large block at the right edge instead of on the
  rim at 3 o'clock. Not changed: no retail eye pitch or arm pose is traced, so this stays a
  recorded gap (next step: lift the cockpit eye pitch/height and place the hands on the rim).
