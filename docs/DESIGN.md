# Design

Goal: a standalone Rust/Bevy/wgpu program that loads one real Porsche Unleashed track from the player's own install and lets you drive around it. Long term: the original 4-point physics, then online play as new netcode.

## Decisions (from the intake)
- Type: engine rewrite (route 5 in the AI Game Modding Guides). Single-player and offline first.
- Data: read from the player's install at runtime (`NFSPU_GAME_DIR`). Nothing from the game is stored in this repo.
- Physics: faithful to the original is the target. Version 0.1 uses a simple placeholder car so driving comes early. The original physics is ported afterwards from the decomp, written in our own structure.
- Personal use only. The decomp project is read-only reference.
- Reference builds: original `Porsche.exe` v3.5 (the decomp), plus a patched `nfs5.exe` copy for runtime comparison (not yet made).

## Layout
- `crates/nfspu-formats`: plain Rust readers, no engine. RefPack, then FSH (textures), CRP (meshes, parts), track side files.
- `crates/nfspu-game`: Bevy app. Window, camera, track rendering, car, input.
- `docs/provenance/`: what we learned about each format, as prose and tables (never code from the decompile).

## Milestones
| # | Goal | How we know it works |
|---|------|----------------------|
| M0 | Bevy window opens, finds the game folder | You see a window; log says the Track folder is present |
| M1a | RefPack decompress | All 30 track `.crp`/`.fsh` decode, sizes match headers (done) |
| M1b | Parse FSH (textures) and CRP (meshes) | **Done for geometry**: all 15 tracks parse, 0 bad indices (`examples/tracks.rs`). Materials to textures still to do |
| M2 | Render one track (alps) with a free-fly camera | **Done** (user confirmed): road shape recognisable; untextured, some stray triangles |
| M3 | Road surface, placeholder car drives on it | **Done** (user confirmed on alps, 247 km/h top speed). No wall collision; car leaves the ground at crests when fast |
| M4 | Real car model in the game | **Done for the 993 (user confirmed)**: 9 body styles, 21 paint schemes, textured body, wheels, underside, soft-top window, driver, headlight and brake-light glow, left-hand-drive orientation, chase/cockpit/orbit cameras. Cabin is flat grey (its texture page is not understood) |
| M5 | Cars, sound, menus, AI, then online | Later |

## Open questions
- Which file holds the drivable road surface (`.edg`? `.map`? inside `.crp`?)
- How the `.crp` articles/parts map to meshes, and the vertex format.

## Next
- M2b: textures on the track (built, awaiting playtest).
- Find out which parts are the drivable road and which are scenery, and what the stray large triangles are.
- Then M3: placeholder car and collision with the road surface.









## Retail horizon atlas (2026-10-15)
1. Validate static reference and raw sky headers before rendering edits (done; logs/horizon_reference.txt).
2. Parse bounded 16-cell atlas configuration; generate independent quads using retail UV insets and height/radius ratios, mirrored like track geometry. Start segment belongs to colour lookup only.
3. Offline audit real configs and test malformed headers, UV flips, seams and geometry; build only required debug targets with -j 2. No live window.
4. Keep existing guessed tint behavior, explicitly labelled unconfirmed.

## Continuous fidelity work (2026-10-15)
- User now authorizes opening/closing game test runs. Start with visual validation of Alps and Monaco atlas rendering; capture observations in logs.
- Compare rendering to static reference and available retail evidence; fix verified discrepancies before extending sky features. Colour record mapping remains unresolved.
- Keep read-only install/decomp boundaries and debug-only required-target builds with -j 2.

- Add opt-in `NFSPU_SKY_CAPTURE=1` deterministic horizon captures (four headings above start, logs only, exit after saved screenshots). This makes rendered checks reproducible without changing normal play.

- New static correction: PE string addresses prove the horizon vertex resource is `hzcl`, not `horz`. SHPI lookup returns raw entry; offsets +0x10/+0x50 are its 16x2 pixel rows. Validate independent row/start/wrap assignment, then replace guessed whole-band tint with those per-vertex colours. Full sky colour and .bin record meaning remain unresolved.

## Track batch boundary correction (2026-10-15)
Live Monaco captures show mesh attribute truncation warnings. Preserve complete triangles within each batch; never concatenate leftover 1-2 corners with the next batch. Log excluded tails by article/material, test mixed malformed batches, audit all 15 real tracks and recapture Monaco to verify warnings disappear.

## Primitive topology fidelity (2026-10-15)
Static chain verified: 0044a250 returns primitive header low word; material submission carries it to renderer dispatch 0053f620. Mode 1 path 00540ce0 enumerates n-2 adjacent triples with odd/even winding; mode 3 path 00543ae0 steps independent triples. Inventory indexed modes across tracks/cars first; then normalize verified strips to triangles with all corner attributes kept aligned. Audit real assets, test degenerate-strip parity and ground/car consumers, and rebuild required targets only (-j 2).

## Physics and collision (2026-10-15, v1 walls)
Goal: stop the car at real geometry instead of driving through hillsides. Survey first
(`wall_survey` example): road pieces are almost entirely flat, so rails live in the terrain:
steep CNK faces stand right next to most of the centerline on every track.
1. `track.rs` keeps near-vertical RD/CNK faces as `walls` alongside the up-facing ground.
2. New `collision.rs`: XZ grid over wall triangles; each car axle circle (r = 1 m) is pushed
   out of overlapping walls whose height range meets the body, so bridges and high signs
   are ignored. Pure Rust, unit-tested (push-out, vertical overlap, empty set).
3. `car.rs` resolves front/rear axle circles after the ground follow and scrubs speed while
   scraping; `hit_wall` reports contact. drive_check/lap_check and the viewer pass walls
   through; console track-reload rebuilds them.

## Collision v2 (2026-10-15): slope limit and rigid ground probe
Two live reports: cliffsides were still driveable on alps, and monaco had spots the car fell
through. Measured both offline (`ground_audit` example, `logs/ground_audit.txt`).
1. The drivable set included up-facing faces up to 76 degrees (alps: 8,389 faces over 40 deg),
   so cliff sides counted as road. `track.rs` now classifies by slope: up-facing and shallower
   than 40 deg (`DRIVE_SLOPE_COS`, in `classify`) is ground, everything else is a wall. Road
   triangles are unaffected on all 15 tracks (roads are flat); only steep terrain moves to walls.
2. Monaco's fall-through spots were 1-4 m pockets between the road edge and the base of the high
   walls (22 pockets on monaco1, e.g. (739.4, 351.7)). The single-point height query fell into
   them. `ground.rs` gained `surface_near`, which finds the nearest surface within a radius and
   uses its closest outline point, and `car.rs` probes in three widening tiers (1.5 m / 3 m / 6 m)
   before it lets the car fall - a rigid car bridges piece joins instead of dropping through.
3. No road face in any track is wound the other way (0 inverted road triangles in the audit), so
   the winding-based facing test stays; it is now combined with the slope limit.
Verification: 57 lib tests (slope classification, gap bridging, no lower-deck drop, wall stop),
install-backed `real_tracks_have_no_fall_through_on_the_road` passes all 15 tracks (0 of 14624
monaco1 road points and 0 of 54840 points up to 5.5 m off the road lack support). Autopilot
(`lap_check`) completes alps for the first time (225/225, wall contact 1.6%), plus monaco1/3/5 and
skidpad; monaco2/4 still defeat the autopilot (they have only 6 primary road pieces, so its
waypoints are too sparse - not a collision failure). Live monaco1 run exits Success, 15959 wall
triangles, no warnings. Live feel is **not tested**: the user playtests.
Next (not started): crest stickiness, handling fidelity, collision with signs/objects, car-vs-car.

## Retail parity plan (2026-10-15)

The rewrite is still a tech demo: one track, a placeholder car, a fly/chase camera, a guessed sky
and no front end. Closing that gap means building the four retail surfaces, in this order, each on
evidence read from the player's install and each auditable offline. Counts are from
`logs/retail_survey.txt` and `logs/fe_audit.txt`.

| # | Milestone | Where the data is | How we know it works |
|---|-----------|-------------------|----------------------|
| M6 | **Front end**: screens, navigation, text | 98 `.lay`, 73 control templates (`.ctr`/`.scr`), 3,124 strings, 72 art pages + 12 `.ffn` fonts, `Models/garage|factory|dealer|track` scenes | `fe_audit` passes (0 undefined control ids, 653/653 layout strings resolve); screens now load and navigate in the viewer, with per-screen coverage in `logs/menu_audit.txt`. Still to come: `Art/*.fsh` pictures and the four-corner gradient boxes |
| M7 | **In-race HUD** | `DashHud/*.fsh` (38 pages incl. `321go`), `FEData/Data/nfs5-1.hud` / `nfs5-2.hud`, `SaveData/*.hud`, `hudstrings.h` | gauge/dash elements drawn from the retail pages; the `.hud` element table (id, position, colour) is understood and audited across every shipped layout |
| M8 | **Lighting** | `Sky/<track>.txt` + `.bin` (weather regions, sky colour, cloud layers, wind), `Render/*.ini`, time of day | ambient and sun per weather region come from the file instead of the current guess; the direction tint stops being labelled unconfirmed |
| M9 | **AI opponents and traffic** | `Simulation/AI/*.csv` + `.bin` (scripts, tuning, personalities, spread, glue), `Track/racer/work/<track>{0,1}.lsp` racing lines, `CarData`/`aicardata`, `TRAFCFG.DAT` | offline: every AI sample sits on the shipped racing line and on drivable ground; then a race in-window with opponents |

Why menus before AI, even though the request listed AI first: the front end is what makes the
window *the game* (choosing track, car and race), and M9 needs that flow to exist. M9 is also the
largest single piece (path following, personalities, catch-up, traffic). Each milestone keeps the
project's rule: parse and audit the data first, keep guesses labelled, draw last.

