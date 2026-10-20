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
  (`ran0`..`ran3`, else `rain`, both in `Render/particle.fsh`) and a camera-space splash quad
  (`SPLT` in `common.fsh`, corners at +-1, height 1.4 / 0.8, depth -0.55 / -0.95).
- Particle budget by graphics detail (0x44ED10): 60 / 140 / 220 / 300. Per frame the count is
  round(amount x budget - 0.5), none below 3 (0x459260). Amounts come from the keyframes.
- Emitter numbers: box -15..15, rain velocity (0, -0.8, 0) per tick, snow (0.01, -0.04, 0); sizes
  0.2 x 0.5 (rain), 0.2 x 0.2 (snow). Which number is which box edge was not fully worked out.
- Ours: 300-drop budget (highest detail), streaks in a box 15 m round the camera, 3 m below to
  10 m above, rain ~26-36 m/s, snow ~1.5 m/s with drift. **Box mapping and speeds are guesses.**
  Screen splashes (`SPLT`) are not drawn yet.
- No spray: `Render/carpart.ini` has road snow, dirt, grass, skid smoke and sparks, but no water
  spray system, and no rain code path emits one. Retail shows red tail-light streaks on the wet
  road (let's play, autobahn); how is not traced (likely the `REFLECTIONS` option). **Not done.**

## Lightning (verified rule, our flash is a guess)
- 0x45CCF0: keyframe lightning >= 0.04 / 0.33 / 0.66 picks strength 0 / 1 / 2; each tick a flash
  starts with chance ticks x (1/30, 1/15, 1/10). A flash (0x45CD90) lasts (rand + 1) x 60 ticks,
  is placed at (0.4/0.3/0.2 + rand x 0.2/0.1/0.1) x half the header distance (0x40) in a random
  direction, and plays thunder (sound ids 40/80/126) with chance 0.1.
- Ours: same timing; the flash brightens both fogs towards white with a flicker. Where retail
  draws the bolt is not reproduced.
- `Sky/autobahn_lightning.bin` and `industrial_lightning.bin` (version 944) are never named in the
  exe, so retail does not read them.

## Track `.map` header: car lighting (verified keys, use not traced)
`[header]` holds `tint`, `ambient`, `interior`, `interiordiffuse`, `lightdecal` as A,R,G,B floats
(0x4409D0; defaults ambient 0.3, the rest 1.0), then `[sectionN]` with `startslice`, `endslice`,
`texture1..3.open/.covered` (four-letter names of reflection pictures in the track's `.env`;
default `ENV0`), `texturesize` (6), `aspect` (1). So `.env` holds the car's reflection maps per
stretch of road, open or covered. Ours: read and logged; `lightdecal` scales the headlight pool.
Car body lighting from `ambient`/`tint` is **not applied** (the car keeps its baked shading).

## Sky colour and regions
- The region blend table moves the sky between weather region A and B along the lap; the dome
  now blends between the two regions' sky-colour pictures by it (replacing the old guessed
  direction tint). Retail draws textured, scrolling sky and cloud layers; ours is a tinted dome.
  **Clouds, sun glare (`GLAR`, `FLR0/1`, `SUNA`, `flare.dat`) and the sky map layers are not
  drawn.** No star field exists in retail data.

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
