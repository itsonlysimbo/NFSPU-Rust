# Weather, time of day and track lighting

How retail decides and draws night, rain, snow, fog and lightning, and what the rewrite does.
Evidence is the read-only decomp (Ghidra bridge on `Porsche.exe` v3.5, addresses given so the
evidence can be found again), the install's data files and the reference let's play
(`aL_I0E5QIR0`). Code: `nfspu-formats/src/weather.rs` (readers), `nfspu-game/src/weather.rs`
(rules, overrides, grip tables), `nfspu-game/src/weather_fx.rs` (drawing). Audit:
`cargo run -p nfspu-game --example weather_audit -j 2` writes `logs/weather_audit.txt`.

## Summary: what each track gets

| Track file | Name | Night | Wet road | Precipitation (keyframes) | Fog |
|---|---|---|---|---|---|
| alps | Alps | no | no | snow up to 1.0 near the village, light lightning | orange dusk to blue, dense |
| autobahn | Autobahn | no | **yes** | rain 0-1 (off under bridges and in tunnels), lightning 0.135 at the start | blue-grey storm |
| canyon | Corsica | no | no | none | grey |
| castle | Auvergne | no | no | none | warm |
| coastal | Côte d'Azur | no | no | none | pink in places |
| farmland | Normandie | no | no | none | orange dusk |
| foothills | Pyrénées | no | no | none | lilac, thin |
| forest | Schwarzwald | no | no | none | grey |
| industrial | Zone Industrielle | **yes** | no | lightning 0.5 (storm) | sky only, dark brown |
| monaco1-5 | Monte Carlo | **yes** | no | none | none |
| skidpad | Skidpad | no | no | none | orange |

Names come from `nfs5.trk` and `festrings.loc` (the audit prints them).

## Track vertex colours are blue, green, red (verified, fixed)
The `.crp` per-corner colours are Direct3D colours: bytes B, G, R, A. The viewer used to read them
as R, G, B, which swapped warm and cold everywhere. Evidence (`vcol_probe` example, average raw
bytes of the pieces near a road slice): alps' snow section reads (146,101,72) - blue as B,G,R,
orange as R,G,B - and retail shows it blue-white; autobahn (126,107,75) is blue-grey in retail;
Normandie (117,104,125) is orange in retail. Every one only matches with B,G,R. Fixed in the
track loader (main.rs); compare `logs/weather/compare_*.png` before/after.

## Menus: no weather or time choice (verified)
No quick-race layout (`FEData/Layouts/*.lay`) has a weather or time control: the only `Track1.*`
keys are `Track`, `Direction`, `Traffic`, `Laps`. `WEATHER` in the exe's settings-name table
(0x5D3ACC) has no code references. So every track has fixed conditions; `RaceSetup` is unchanged.

## Night (verified)
- The night flag (0x628A28) is set by the layer factory (0x449B70) when it creates a headlight
  vertex-light layer: ids 0x8002 `LayerVertexLight`, 0x8003 `LayerVertexLight_Opaque`, 0x8011
  `LayerVertexLight_Ambient`. These ids sit in a track `.crp`'s render-method parts (tag `..MR`).
- Scanning all 15 tracks: only **industrial and monaco1-5** contain them. So those race at night,
  every other track by day. (Matches the let's play: Monte Carlo and Zone Industrielle are dark.)
- With the flag set, retail: loads `Render/night.psh` (0x452590), uses the night HUD dial art
  (`HTach`, `HSpeedometer` index by the flag), sets up the headlight light (0x452680).

### Headlight pool (verified model, our drawing is an approximation)
- `night.psh` is an `SHPP` file with one 128x128 8-bit picture `nght`, a soft round blob. It is
  **not** a star field (an older note guessed that). Retail found no star field anywhere.
- Per track vertex near the car (0x452910): transform into the car's headlight frame; row =
  round(ahead x 1.28), column = round(side x 3.62) + 64 (rows 1..126, columns -62..62, i.e.
  0.8-98 m ahead, +-17 m sideways); intensity = (map byte - |height x 32|) / 256.
- The lamp colour is 0xFF808080 (0x452680); the vertex light layers add colour x intensity to the
  baked vertex colour, clamped at 255 (`LayerVertexLight_Opaque` 0x453110).
- The headlight frame is the car transform with a -4.8 offset (0x5CEC54) and a -17 degree tilt
  (0x5CEC5C) (0x4526C0). Ours reads the offset as a height: lamps 4.8 m above the car, aimed 17
  degrees down. With the height falloff that makes a band brightest ~16 m ahead that fades out by
  ~45 m, which looks like retail's industrial frame. **That axis reading is a guess.** Retail also
  tracks the nearest police car within 180 m for red/blue light (not done).
- Ours: a ground-hugging grid in front of the car (heights from our ground), coloured by the map
  and drawn additively, so it brightens the road instead of adding to vertex colours before the
  texture. **Approximation.**
- Headlights start switched on at night (ours; retail's default switch state was not traced).

## Wet road and grip (verified)
- At race start (0x414DB0) the wet flag 0x606AE0 is set only when the track name is `autobahn`.
- While set, and the car is **not under cover** (0x473330: a surface more than 3.5 m above the car),
  the tyre-grip factor per wheel comes from the wet table instead of the dry one (0x4A1900), the
  surface sound becomes wet asphalt (type 5 to 6, 0x40FBC0), and the AI uses its wet speed table.
- Grip tables: 16 surface rows x 3 tyre classes, dry at 0x5D0D50, wet at 0x5D0E4C. Row 1 is
  asphalt: dry 1.0 / 0.9 / 1.1, wet 0.9 / 1.0 / 1.1. The wet table is the dry one with the first
  two tyre columns swapped in every row. A car flag swaps class 0 and 1 further (not traced).
- For the physics session: `nfspu_game::weather::asphalt_grip(tyre, wet)` and `surface_grip`,
  with `TrackWeather::wet`. **car_sim.rs is not changed here.** Under-cover is not implemented.

## Sky `.bin` weather keyframes (verified layout)
The 0x71DC-byte version-9 file is copied raw into the sky config (0x459660/0x459700). After the
horizon header:

| Offset | Content |
|---|---|
| 0x03C | float angle (turns) + 3 floats; 0xB3C the same again (used to rotate camera-relative sky scrolling, 0x45D030) |
| 0x0DDC | 400 x (i32 slice, f32 value): weather region blend, A=0 .. B=1 (0x45A350) |
| 0x1A5C | 400 x 48-byte keyframes (0x45A560 and accessors) |
| 0x655C | 400 x (i32 slice, u8 flag): skips a sky layer in places (0x45D470) |

Each table ends at the first negative slice; fewer than 2 entries means a built-in default.

Keyframe (48 bytes): +0 slice; +4 sky fog, +8 world fog (both stored as the cube root of the
density: the accessors 0x45A6A0/0x45A6E0 cube them); +0x10 sky fog colour, +0x14 world fog colour
(0xAARRGGBB); +0x18 rain; +0x1C lightning; +0x20 snow; the rest is zero on every track.

Interpolation (0x45A560): find keyframes i, j=i+1 (the last wraps to the first) with
slice_i <= s < slice_j; distances wrap at the track's slice count; weight of i = 1 - (s - slice_i)
/ (slice_j - slice_i). Colours only blend toward j when j has fog of its own (density > 3e-6);
otherwise i's colour holds. The slice is the camera's road slice (0x45D4F0 tracks it each frame).

Colour byte order: autobahn's `ff41556f` as ARGB is blue-grey (65,85,111), which matches the
storm sky in the let's play; farmland's `fff99147` is orange, matching Normandie's orange haze.
The old sky code read these same records as 0xFFBBGGRR "horizon colours by direction"; that was
wrong and is no longer used.

## Fog (verified states, mode inferred)
- The sky pass (0x45D4F0) sets fog from the keyframe's sky fog (0x45D2E0) before drawing the sky
  layers, then the world fog (0x45D3B0) after them. Both: render state 0x15 (fog mode) set to 4
  then 1, 0x0E density, 0x0F colour; mode 0 turns it off when the density is ~0. These are the
  states the patch's `Fog=0` setting filters (see refbuild.md), which confirms they are fog.
- Mode value 1 is D3D's exponential fog (`D3DFOG_EXP`): visibility = exp(-density x distance).
  That reading of the value is an inference from the D3D numbering. Typical densities: autobahn
  world 0.0015/m (half visible at ~460 m), alps 0.0008-0.021, Monte Carlo none.
- Ours: Bevy `DistanceFog` with `FogFalloff::Exponential` on the world camera; the sky is drawn by
  its own camera first (render layer 1) with the sky fog. Bevy blends fog in linear light and by
  view distance; retail blended in screen space by depth (small differences, **not tested**
  side by side numerically). Sky fog density is scaled by 400 / dome radius because our dome is
  3 km away and retail's horizon ring is at 400 units (**guess** that the whole sky sits there).

## Rain and snow (verified parameters, drawing approximated)
- 0x458C90 builds a snow emitter (sprite `sno0`..`sno3` by detail, else `snow`), a rain emitter
  (`ran0`..`ran3`, else `rain`, both in `Render/particle.fsh`) and a rear-window drop sheet
  (`SPLT` in `common.fsh`, car-frame corners at +-1, height 1.4 / 0.8, z -0.55 / -0.95).
- Particle budget by graphics detail (0x44ED10): 60 / 140 / 220 / 300. Per frame the count is
  round(amount x budget - 0.5), none below 3 (0x459260). Amounts come from the keyframes.
- Emitter numbers: box -15..15, rain velocity (0, -0.8, 0) per tick, snow (0.01, -0.04, 0); sizes
  0.2 x 0.5 (rain), 0.2 x 0.2 (snow). Which number is which box edge was not fully worked out.
- Ours: 300-drop budget (highest detail), streaks in a box 15 m round the camera, 3 m below to
  10 m above, rain ~26-36 m/s, snow ~1.5 m/s with drift. **Box mapping and speeds are guesses.**
  The `SPLT` drops sit on the car's rear window (see below), not on the screen.
- No spray: `Render/carpart.ini` has road snow, dirt, grass, skid smoke and sparks, but no water
  spray system, and no rain code path emits one. Retail shows red tail-light streaks on the wet
  road (let's play, autobahn); how is not traced (likely the `REFLECTIONS` option). **Not done.**

## Lightning (verified rule and bolt placement)
- 0x45CCF0: keyframe lightning >= 0.04 / 0.33 / 0.66 picks strength 0 / 1 / 2; each tick a flash
  starts with chance ticks x (1/30, 1/15, 1/10). A flash (0x45CD90) lasts (rand + 1) x 60 ticks
  and plays thunder (sound ids 40/80/126) with chance 0.1 (**sound not done**).
- **Where the bolt is** (0x45CD90): a point of the 16x16 sky grid. Half = dome size / 2;
  r = (base + rand x range) x half with base 0.4/0.3/0.2 and range 0.2/0.1/0.1 by strength; a
  random angle a; column = (half - sin(a) r) / size x 15, row = (cos(a) r + half) / size x 15,
  clamped to 0..15. So strong storms strike nearer the top of the dome.
- **What it does** (0x45DF50, 0x45E3D0, 0x45E550): every sky layer that has a sky map gets a set
  of "sparks" that brighten its point colours. Pattern by strength from 0x5CF07C ({0,0,1}, {1,1,2},
  {2,3,4}) into the table at 0x5CF018: (budget ticks, sparks, rise factor, peak, new per tick) =
  (90,35,0.82,255,6), (90,8,0.68,255,5), (90,80,0.30,255,5), (90,8,0.26,255,5), (90,33,0.11,255,18).
  Each spark: peak = rand x 255, step = max(1, round(2 peak / max(2, rand x 90 x rise))); it
  starts at a point offset from the strike by a value from 0x5CF144 per axis (strength 0: -2..2,
  1: -1..1, 2: none), rises by step per tick to its peak, falls back, and while lit adds its value
  to the point's R, G and B, each capped by the matching pixel of the track's 7x7 `ltmp` picture
  (centre pixel at offset 0; 0x45CC20 indexes it x * 7 + y). Only autobahn and industrial have
  `ltmp`. The flash ends when no spark is lit and the budget is spent; while the bolt lasts a
  layer flashes again.
- Retail does **not** brighten the fog; the old whole-sky fog flash is gone.
- Ours: as above on our 64 Hz tick, one spark update per tick (retail updates per frame with the
  frame's tick count, which only changes how much each spark adds; **guess**). The pattern pick
  uses a random index 0..2 (the exe's scale for it sits in FPU code; **guess**).
- `Sky/autobahn_lightning.bin` and `industrial_lightning.bin` (version 944) are never named in the
  exe, so retail does not read them.

## Sky layers (verified layout and maths, drawn)
Code: `nfspu-game/src/sky.rs` (`parse_level`, `SkyDome`) and `nfspu-game/src/sky_fx.rs`.

**The `.txt`** (reader 0x45FB80, level 0x45B5A0): the whole file is lowercased, then read one
non-`#` line at a time. Of the `BEGIN`..`END` blocks the one for the graphics capability is used
(`high`). In order: capability, level type (`multinormal` on every track), "blocks the sun"
(`no` sets flag 0x400), then for region A and region B: 3 sky maps, 3 X scales, 3 Y scales, the
move ratio, 3 wind speeds, 3 wind directions, warp speed, warp amount; then 3 textures. Layers =
textures that are not `none` (`flat` is a plain white texture); layer i uses texture i and sky
map i.

**Sky maps are per-point colours, not textures.** Every sky map is 16x16 and the dome is a 16x16
grid of points: pixel (column, row) is the colour of point (column, row) (0x45DD90). Region A and B
maps are mixed per byte by the region blend (0x45E160: a x (1 - t) + b x t in 16.16 fixed point).
The texture is multiplied by these colours; `sc1a`/`sc2a` carry alpha (0 along one side of the
sky), `st1a` is opaque. A layer whose texture and maps are fully opaque is drawn without blending.

**The dome** (0x460EC0/0x460F40, header from the sky `.bin`): 15 x 15 cells over a square `size`
(0x40) across, centred on the camera, turned by 0x3C turns; a point at plane position (b, a) is
x = (sin t a + cos t b) s, y = (height - (a^2 + b^2) curve) s, z = (cos t a - sin t b) s, with
curve 0x44, height 0x48 and scale s at 0xCE4. Rows run a = -size/2 .. +size/2, columns
b = +size/2 .. -size/2. Example autobahn: size 1500, curve 0.0002, height 76.7, s 0.576 - a cap
44 units high that passes the horizon about 357 units out (retail draws the horizon ring at 400).
The 0xB3C..0xB48 copy (size 1000, curve -0.0002, height -75) is used instead when a level's flag
0x200 is set - an upside-down bowl, probably for reflections; not used here. Retail draws the sky
with near 0.15, far 700 (0x45D4F0) and culls the grid in 3 x 3 blocks.

**Scrolling** (Scroller 0x45E8E0, 0x45F1B0, 0x45ED40, 0x45EC40): texture coordinates of point
(column c, row r) are u = c / Xscale + wind_u + slide_u, v = r / Yscale + wind_v + slide_v. Scales,
wind and 1 / move ratio are each mixed between region A and B by the blend.
- Wind: angle = -direction / 360 turns, drift per tick = (sin, cos)(angle) x speed x 0.00025, kept
  in 0..1; only layers with a wind speed above 0.25 in either region drift.
- Slide with the camera when either region's move ratio is above 1: slide_u = -frac(cam_x / Xscale
  / ratio), slide_v = frac(cam_z / Yscale / ratio) (retail axes).
- Warp (a random UV wobble) needs warp speed > 0.01; every shipped track has 0, so it is not drawn.

**Draw order** (0x45D4F0): sky fog on; levels that do not block the sun; the sun (0x460290); the
other levels; the horizon last; world fog. Ours sorts the sky pieces in that order by `depth_bias`
ranks, and no sky piece writes depth. The horizon band now also takes the sky fog, as retail's
does (it used to be unfogged).

**Ours vs retail**: our sky sits about 7x further out (the horizon band is at ~2.8 km; the dome
uses the same factor, band radius / 400, so the angles match); vertex colours are interpolated in
linear light (retail: gamma space), which shifts gradients slightly (**not measured**).

## Sun and lens flare (verified, drawn; blending guessed)
- `Render/flare.dat` is a database of 39 glow records, 140 bytes each: name (a dword that reads
  backwards: `lloa` = `aoll`), two picture names from `common.fsh` (or `null`), 16 zero bytes, two
  colours 0xAARRGGBB at 0x1C/0x20, two sizes (floats) at 0x34/0x38, flags at 0x4C (bit 4 = turn
  with screen position; every record sets it), blink phase/on/off ticks at 0x50/0x54/0x58 (arrow
  signs `arwa`..`arwe` chase with phases 50/40/29/21/10, on 21, off 50; warning lamps `wrna`/`wrnb`
  30/30). Names include `lamp` (SPIK + BLUR), `spot`, `strb/strg/strp/strr/stry`, `tuny`,
  `helf/helr/helw`, `trfg` and the sun set `suna`..`suni`.
- **The sun** (0x460780, 0x461090) sits on the dome surface at plane point (0xCDC, 0xCE0) of the sky
  `.bin`; (0, 0) (within 0.02) means no sun: autobahn, canyon, forest, foothills and Monte Carlo
  have none; alps, castle, coastal, farmland, industrial and skidpad have one (industrial's level
  blocks the sun, so it stays hidden there). When the sun's point is on screen, record `suna` is
  drawn there: `SUNA` size 205, white, and `BLUR` size 600, colour 0x33FF3301.
- **Sprite size** (0x451710): half-width = size x 0.1 x screen width / 640, half-height = size x
  0.1 x screen height / 480. Below intensity 0.05 nothing is drawn; below 1 the colour's alpha
  becomes the intensity. Turned by (x + y) / 1440 turns (screen pixels) when flag bit 4 is set.
- **Ghosts** (0x460460): 13 slots, record `sun` + letter b..n, slot k placed at centre + (sun -
  centre) x `flarepos.dat`[k + 1]. `flarepos.dat` is 14 floats: 1, 0.75, -0.75, 0.5, -0.5, 0.25,
  -0.25, 0, -1, then zeros (the exe has the same table built in). Records `sunj`..`sunn` do not
  exist, and `sunb`, `sunc`, `sune`, `suni` use picture `lens`, which is in no shipped pack, so only
  `sund` (FLR0 at 0.5), `sunf` (BLUR at 0.25), `sung` (SUNA + FLR0 at -0.25) and `sunh` (BLUR size
  1749 at the screen centre) are drawn. Needs the per-track switch at 0xDD8 (1 on every track) and
  the lens-flare option.
- **Visibility**: every 200 ms retail reads a 4x4 block of the depth buffer at the sun's pixel,
  counts pixels that still hold the sky's depth (reference read when the sun was first drawn) and
  uses count / 16 as the intensity. Ours casts 16 rays round the sun's pixel against the world
  meshes. Retail also gives up on the flare if the read is too slow (not copied).
- Ours: the sun sprites are camera-facing squares in the sky pass, sized to retail's screen size;
  the ghosts are drawn by a third camera (640 x 480 orthographic, render layer 2) over the picture.
  **Additive blending is our guess** (the sprite render state was not traced).

## Track glows (`flare.dat` on lamps) (verified placement, drawn)
- Each track `.crp` has parts tagged `[n, 0, 'f', 'e']` holding 44-byte entries of type 2
  (RDrawArticleBundle 0x450D10): type, a dword, position x y z w (world coordinates, w = 0), an up
  vector (0, 1, 0), the `flare.dat` record name and a random blink phase. The same glow can appear
  in two parts (kept once). Templates near the origin (farmland's and forest's `EndWall`) are not
  placed. `examples/flare_parts.rs` counts them: autobahn 431 (152 `stry` streetlights, 198 `tuno`
  and 74 `helw` tunnel lamps), industrial 654, castle 291 (`lmpy`), canyon 119 `cant`, alps 57,
  coastal 29, monaco1 152.
- 0x451940 draws each with its record: two quads round the point, half-size = record size x 0.1
  (so `stry` is a 1.5 m core and a 7.4 m halo), blinking while (record phase + entry phase + clock)
  mod (on + off) <= on, and when fog is on, alpha x (1 - distance / 300) (0x4515F0). Flag bit 4
  turns the quad by (120 x a view term + 3.6 x distance) - **not copied**; a further scale by
  -250000 x a screen constant / distance^2 sets a render state (point size?) - **not traced**.
- Ours: camera-facing quads, additive (**guess**), on the 64 Hz clock; drawn by day too, as
  nothing in the draw path checks the time of day.

## Drops on the rear window (verified behaviour, placement partly guessed)
- 0x458C90 builds the effect with picture `SPLT` (common.fsh) on a fallback quad: corners
  (+-1, 1.4, -0.55) and (+-1, 0.8, -0.95). In the engine's car frame +z is forward, so that is a
  sloping **rear** window. The player car passes its own four glass points (car +0x280, count at
  +0x27C) each frame (0x43A490); **those per-car points are not traced, ours uses the fallback**,
  placed from the car's ground point and heading. It is drawn only for the car the camera follows
  and with a render flag (0x627F43 bit 8, not traced). This matches the let's play: light specks on
  the car's rear window in the rain. (Earlier notes called this a screen-space lens splash.)
- Count (0x4582C0): trunc(trunc(min(rain + snow, 1) x 150) x min(1, glass area x 0.769)); at most
  5 new drops per update; 201 drop slots, one triangle each.
- A drop does not travel. It starts at a random point with a random alpha, then by age (ticks;
  table 0x5CEE40) grows towards length/width (0.025, 0.025) at rate 0.2 (0.03 from age 40), keeping
  its centre still, and fades towards 255 until age 60, then towards 0; it restarts below alpha 15
  or off the glass. Triangle (0x457FD0): head at the drop, two tail corners `length` back along its
  direction and `width` to each side; UVs alternate per drop.
- Direction (0x4584F0, eased by 1/60 per update): faster than 22.2 m/s (and not snow-only weather)
  away from a point a quarter of the way down the glass; when the car reverses, down the glass;
  otherwise up it.

## Track `.map` header: car lighting (verified keys, use not traced)
`[header]` holds `tint`, `ambient`, `interior`, `interiordiffuse`, `lightdecal` as A,R,G,B floats
(0x4409D0; defaults ambient 0.3, the rest 1.0), then `[sectionN]` with `startslice`, `endslice`,
`texture1..3.open/.covered` (four-letter names of reflection pictures in the track's `.env`;
default `ENV0`), `texturesize` (6), `aspect` (1). So `.env` holds the car's reflection maps per
stretch of road, open or covered. Ours: read and logged; `lightdecal` scales the headlight pool.
Car body lighting from `ambient`/`tint` is **not applied** (the car keeps its baked shading).

## Sky colour and regions
- The region blend table moves the sky between weather region A and B along the lap; every sky
  layer mixes its region A and B sky maps by it (see "Sky layers"). The old tinted dome is only a
  fallback for a track without a sky level. No star field exists in retail data.

## Testing overrides
`NFSPU_WEATHER=dry|rain|snow` and `NFSPU_TIME=day|night` override the track's own conditions
(rain also makes the road wet; snow keeps it dry, as retail has no snow grip change).

## Verification (2026-10-19)
Captures: `NFSPU_WEATHER_CAPTURE=start,0.4 target/debug/nfspu-game.exe <track>` saves
`logs/weather/ours_<track>_<shot>.png` (start = chase view at the grid, fractions = camera behind
that point of the lap) and exits. Retail frames from the let's play are `logs/weather/retail_*`,
side by side in `logs/weather/compare_*.png` (retail left). Not the same spots unless noted.

| Comparison | Result |
|---|---|
| Alps start (dusk) | brown haze and brown sky in both |
| Alps snow section | blue snow, dense blue fog, falling flakes in both; ours a little foggier far away |
| Autobahn (rain) | blue-grey storm cast, thin rain streaks in both; retail also shows tail-light streaks on the road and a lens drop (ours: guessed streaks, no lens drop) |
| Normandie | orange avenue haze in both |
| Côte d'Azur | pink sky and sea haze, purple rock in both |
| Monte Carlo 3 start (night) | lit road ahead, city-lights hillside, warm buildings in both; retail's sky is near black, ours dark slate |
| Zone Industrielle (night) | dark tunnel with a lit band of road ahead in both |

Logged, not seen: a lightning flash on industrial (`flash 0.18` in `logs/m0.log`). **Not tested:**
the flash's look, the wet-road reflection (needs lights on or braking), `NFSPU_TIME`/`NFSPU_WEATHER`
overrides in a live run, quick race through the menus with the new sky camera, performance.

## Verification of the sky work (2026-10-20)
Captures with the sky in view: `NFSPU_WEATHER_CAPTURE_UP=25 NFSPU_WEATHER_CAPTURE=0.05,0.3,0.6
target/debug/nfspu-game.exe <track>` (the new `_UP` lifts the point the capture camera looks at).
Side by side, retail left: `logs/weather/sky_compare_{coastal,autobahn,industrial,alps}.png`;
six tracks at 30 % of the lap: `logs/weather/sheet_sky_030.png`.

| Check | Result |
|---|---|
| Autobahn sky | dark grey-blue cloud deck with visible cloud texture; matches the let's play |
| Côte d'Azur | pink sky, low sun with its BLUR halo above the sea; matches retail's colour |
| Alps | brown-pink dusk with cloud texture; tunnel lamps (`tuny`) glow along the tunnel |
| Zone Industrielle | dark brown sky, coloured lamp glows; retail's tunnel lamp is a smaller, sharper star than ours |
| Autobahn streetlights | `stry` glows sit on the lamp heads |
| Lightning | bolts logged on industrial (strength 1, grid points (9, 9) and (5, 5)); **the flash itself not seen in a capture** |
| Lens flare ghosts | sun on screen on coastal (unhidden 1.00), ghosts placed; **not checked against retail** (no retail frame with the sun) |
| Rear-window drops | faint specks drawn while it rains, but on the 993 the fallback quad sits on the roof, not the rear glass (per-car points not traced) |

**Not tested**: a live race through the menus with the new sky, performance with 600+ glows,
mirrored tracks (the dome and glows use the world mirror), the region A/B blend while driving.
