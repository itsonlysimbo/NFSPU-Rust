# MODLOG

## 2026-10-15 - Front end now drawn with the UI layer (the sprite path drew nothing)
- The first M6 menu spawned `Sprite`s and `Text2d` and rendered **nothing**; a windowless capture proved the frame was a single colour. The rest of this program never uses `Sprite` - it draws 2D with `Node` + `BackgroundColor` + `Text` (FPS readout, console) - so the front end does the same now. Positions are the layout's own 640x480 units scaled to the window height and centred; hover swaps a node's `BackgroundColor`; a window resize rebuilds the layout.
- Verified with `NFSPU_MENU_CAPTURE=1`: `logs/menu_singleplayer.png` now holds 27,267 pixels of `rgb(188,0,0)` - exactly the retail button `maincolor` `0xBC0000` - plus white/grey text pixels on black, where the earlier capture was 100% one colour. A live session also logged successful navigation (`quickrace -> location -> trackrecords`).
- Capture waits were raised from 30 to 60 frames, and the earlier "camera was missing" fix stands (`Camera2d` is still required - UI is drawn by a camera).
- Reminder: a running `nfspu-game.exe` blocks `cargo build` from replacing the binary; close the window first.

## 2026-10-15 - Front end fixed: the camera was missing
- The first M6 build showed a blank black window. Cause: `menu_mode` spawned the screen's sprites and labels but **no camera**, so nothing was drawn and the only thing on screen was the clear colour. Added `Camera2d` (one world unit = one logical pixel, origin at the window centre) and a startup line logging the window's logical size.
- Checked `bevy_camera-0.19.1` to be sure the layout maths matches the renderer: `ScalingMode::WindowSize` is fed the **logical** viewport size, and `Window::height()` / `Window::cursor_position()` are logical too, so the three agree.
- Added `NFSPU_MENU_CAPTURE=1`: after 30 settled frames it saves `logs/menu_<screen>.png` and exits, reusing the screenshot pattern from `visual_probe`. The front end can now be checked without a human at the window.
- Gotcha for the next session: a running `nfspu-game.exe` keeps `target\debug\nfspu-game.exe` open, and `cargo build` then fails with "failed to remove file". Close the window (or stop the process) before rebuilding.

## 2026-10-15 - M6: a real front-end screen, drawn and navigable
- New `nfspu-game::menu` turns a `.lay` screen into draw commands using the shipped data only: geometry from the layout, colours and label placement from the control templates (including the template's `data.width-2` arithmetic and the `0xAARRGGBB` colours), text from the locale table at the chosen language. 7 unit tests.
- New `menu_mode.rs` draws that screen in the viewer in its own 640x480 space, scaled to the window, with hover (`hovercolor`) and click navigation (`destination=` -> `<destination>.lay`, `callback=GOBACK` -> back, Backspace back, Escape quit). Enabled with `NFSPU_MENU=<screen>`, default `singleplayer` (the screen `GameSetup` links to). Entry path checked: `GameSetup -> singleplayer -> quickrace`.
- New `examples/menu_audit.rs` reports coverage over all 98 screens and can dump one screen in detail (pass a screen name). Today: **1,500 draw commands; 1,936 controls not drawn yet**, led by `NFS5.GOURAUDBOX` (576) and `NFS5.PICTURE` (484, an `Art/<name>.fsh` page plus entry index). Output `logs/menu_audit.txt`.
- Two bugs found by checking real screens: `REAL.`-prefixed controls after the header were being dropped as if they were the screen (`REAL.LABEL` is an engine-built-in *control*, not a screen), and text-only controls were skipped because they carry no size. Both fixed, with tests; a label's point size now comes from its font name (`Univers17b.ffn` -> 17). `singleplayer` now reproduces the retail screen: black 640x480, two divider lines, the letter-spaced title "s i n g l e   p l a y e r", and four dark-red buttons ("cancel" goes back; "knockout...", "quick knockout...", "quick race..." open their screens).
- Verified: `cargo check -p nfspu-game --all-targets` clean; 40 formats + 77 game-lib tests pass; the front end was launched once for ~25 s and loaded `singleplayer` with no panic (`logs/m0.log`).
- **Not tested**: the on-screen result by eye.

## 2026-10-15 - Front-end data model (menus foundation) and retail parity plan
- Surveyed the retail data for the four missing surfaces, logged to `logs/retail_survey.txt`. Front end: 98 `.lay` screens, 78 `.ctr` + 1 `.scr` control templates, 72 art pages + 12 `.ffn` fonts, 3,124 locale strings, and five menu scenes (`garage`, `factory`, `dealer2a/b/c`, `track`). HUD: `GameData/DashHud` (38 pages, incl. `321go`) plus `FEData/Data/nfs5-1.hud` / `nfs5-2.hud`. AI: `Simulation/AI/*.csv` with `.bin` counterparts, 30 racing-line `.lsp` files and `TRAFCFG.DAT`. Lighting: `Sky/<track>.txt` + `.bin`.
- New `nfspu-formats::fe`: `.lay` screens (blocks, controls, positions, `stringID`), `.ctr`/`.scr` control templates (`id`, `description`, `parts`, `states`) and the `festrings.csv` locale table. Two shipped quirks are handled: the locale file is ISO-8859-1 (bytes are decoded one-to-one, never as UTF-8) and its tag column sits one past the header's `Comments`. 24 unit tests.
- New `examples/fe_audit.rs` validates the whole front end offline: **98 layouts / 3,380 blocks / 2,976 control placements; 73 control ids across 79 files, of which 6 are deliberately empty placeholders and 0 are unparseable; no layout uses an undefined control id; 653/653 layout `stringID`s resolve in the locale table**. Output `logs/fe_audit.txt`, run `logs/fe_audit_run.txt`.
- Retail parity plan (menus, then HUD, then lighting, then AI - each with its data source and how it is verified) written into `docs/DESIGN.md`; front-end format notes in `docs/provenance/fe.md`.
- Verified: 38 formats tests pass (24 new); the audit runs clean against the install. **Not tested**: nothing is drawn yet, so the window looks exactly as before.

## 2026-10-15 - Retail field of view (opt-in) and stale sky probes repaired
- `nfspu-game` now honours `NFSPU_RETAIL_FOV` (**off** unless set to something other than 0/false/no). When on, the chase and cockpit cameras use the reference build's virtual-screen field of view, recomputed when the window size or camera mode changes; the orbit and free-fly views keep the engine default. New `RetailFov` resource and `retail_fov` system in `main.rs`; it logs client size, virtual size and degrees once per size.
- Repaired the two examples that stopped compiling after the sky-atlas work. `sky_info` read `HorizonColour::u`/`distance`, which the struct no longer has (it exposes `angle`); `sky_lap` called the removed `sky::LapPath` and read `h.distance`. `sky_lap` now walks the lap by the centreline's own direction of travel and samples `horizon_tint` with that heading - the lap-distance reading was disproven, so it no longer pretends distance drives colour. The heading sign is documented as an unverified convention.
- Verified: `cargo check -p nfspu-game --all-targets` is clean (one pre-existing unused-variable warning in `ground_audit`), and **`cargo test -p nfspu-game` now passes end to end** (70 lib tests, 5 bin tests, 2 ignored) where it previously stopped at `sky_lap`. Ran both repaired probes offline against the install: `sky_lap -- alps` reports 225 centreline points, a 10,594 m lap, 74 horizon colours and 6 tint changes; `sky_info` reads every track's sky.
- **Not tested**: the retail field of view on screen. Playtest: `$env:NFSPU_RETAIL_FOV="1"; .\run_m2.bat alps`, press C for chase/cockpit, and compare with a default run.

## 2026-10-15 - Reference build model (patched retail behaviour)
- Added `crates/nfspu-game/src/refbuild.rs`: a model of the author's reference build (Enhanced 3.5
  plus Combined Patch v1.0.6). Original code and names; the numbers are the build's. Covers the
  settings block and defaults, renderer selection and its pass-through options, the virtual
  framebuffer and field of view (4:3 threshold, 640/480 view models, `atan(360*aspect)` widescreen
  FOV), the fog / filtering renderer-state gate, the CPU-detail clamp, language ids and the
  portable data folders.
- Added `examples/refbuild_audit.rs` (no window): prints those tables, reads the author's
  `nfs5.ini` and `drivers/<name>/thrash.ini` when `NFSPU_GAME_DIR` points at a patched copy, and
  scans every `GameData` `.fsh`/`.env` for the `!pal` palette chunk (decompressing RefPack first).
  Writes `logs/refbuild_audit.txt`.
- Added `docs/provenance/refbuild.md` with the evidence, tables and two verified negatives.
- Verified: 70 lib tests pass (14 new in `refbuild`); the example builds and runs. Run against the
  patched copy it read the settings (language ENGLISH, driver `opengl3`, windowed, movies off, fog
  and filtering on), parsed the driver profile (`opengl3z.dll`, OpenGL, 18 options), and found
  `!pal` in **0 of 168** archives (153 plain + 15 RefPack, 112.8 MiB).
- Found while building: `examples/sky_lap.rs` no longer compiles (`sky::LapPath` is gone and
  `HorizonColour` now has `angle`/`colour` instead of `distance`), so `cargo test -p nfspu-game`
  fails in that example. **Not fixed** - it needs a decision on what that probe should report now.
- Not tested: nothing live. `refbuild` is not wired into the viewer yet, so the on-screen camera
  still uses the placeholder values.

## 2026-10-08 - Project scaffold and RefPack
- Created workspace (`nfspu-formats`, `nfspu-game`), whitelist `.gitignore`, `AGENTS.md`, `docs/DESIGN.md`.
- Added RefPack decompressor with 5 unit tests (pass).
- Ran the probe on the author's install: 30/30 track `.crp`/`.fsh` files decode, sizes match headers.
- Bevy window app (M0) written, first build started. **Not tested** yet.

## 2026-10-08 - SHPI reader (written while the Bevy build ran)
- Added `crates/nfspu-formats/src/shpi.rs` (header + entry directory parser) with 4 unit tests, based on the layout verified on `alps.env`.
- **Not tested yet**: cargo is busy with the first Bevy build, so this has not been compiled.

## 2026-10-08 - M0 built, SHPI verified
- First Bevy build finished (95 min, dev profile). Later builds are incremental and much faster.
- `shpi.rs` compiled; 9/9 unit tests pass. Probe parses the SHPI archive inside all 15 track `.fsh` files (98-280 textures each, largest 256x256).
- M0 window: awaiting playtest.

## 2026-10-08 - M0 window closing: cause found (Ctrl+C), fix applied
- User run (pid 32412, 14:16:37) ended after 14.7 s with `app finished with: Error(130)`. No panic, no Windows crash record, no GPU reset event.
- Verified in Bevy 0.19.1 source: `DefaultPlugins` includes `TerminalCtrlCHandlerPlugin` whose exit code is 130 (console Ctrl+C / Ctrl+Break). Source of the Ctrl+C is **unknown**.
- Earlier worry about GPU watchdog events was wrong: those reports point at dump files dated Aug 10 (old).
- Changes: disabled Bevy's Ctrl+C handler, install our own that only logs `Ctrl+C / Ctrl+Break received (ignored)`; fixed log path (`logs/m0.log`); added `run_m0.bat` (writes `logs/launcher.log`); app waits for Enter after exit, so the window looks frozen until the console is closed.
- Simulated Ctrl+C test (`scripts/send_ctrl_c.ps1`): app stayed alive but no 'received' line was logged, so the test is **inconclusive**. Real playtest needed.

## 2026-10-08 - M0 confirmed
- User run pid 3116 (14:26:00-14:29:11): stayed open 95 s at ~60 fps on Intel UHD, closed with the X button, exit code 0, `app finished with: Success`. No Ctrl+C line, no panic.
- Cause of the earlier close (Error 130 = Ctrl+C) was never identified; the handler now ignores and logs it. M0: **works** on Intel UHD. Not tested on the NVIDIA GPU (switched off by the laptop).

## 2026-10-08 - CRP structure verified (alps)
- Walked all 1,752 articles / 16,479 parts. Part length is in bytes and the 'unknown' field is an element count: vertices 16 B, uv 8 B, face data 4 B all match counts in 1,752/1,752 articles; vertex coordinates are finite and plausible (alps ~6 km long).
- Added `examples/dump.rs` (writes decoded files to gitignored `extracted/`). Python inspection scripts live in `extracted/` (untracked).
- Not done: `pr` (primitive/index) parts, materials, which texture each part uses.

## 2026-10-08 - CRP geometry reader (`crp.rs`)
- Decoded triangle parts: 1-byte indices, vertex table + UV table, per-corner colours. Verified on all 15 tracks, 0 out-of-range indices.
- Added `crates/nfspu-formats/src/crp.rs` with 3 unit tests (12 tests total pass) and `examples/tracks.rs`.
- Not done: material to texture mapping, drivable surface, Bevy rendering (M2).

## 2026-10-08 - M2 fly-camera track viewer built
- `nfspu-game` now loads a track (default alps, or the name given as an argument), merges all LOD0 triangles into one mesh with vertex colours (no textures, no culling), and gives a free-fly camera (right mouse + WASD/QE, Shift boost, wheel speed, Esc quits).
- Agent test run (20 s): alps loaded in 0.04 s, 1,752 articles, 59,962 triangles, ~100 fps on Intel UHD, no panic. **Not visually verified** (the agent cannot see the window). User playtest pending.
- Note: 59,962 triangles here vs 59,959 from the earlier per-part count, so a few parts have a corner count not divisible by 3 (9 parts in alps). To check later.

## 2026-10-08 - M2 first playtest and the window fix
- User playtest: light blue background, tan geometry floating, basic movement worked, shapes looked scrambled/stretched.
- Agent offline renders (Python/Pillow, `extracted/*.png`) reproduced the shards. Cause: triangle part index bytes are relative to per-part windows (info rows), which the reader ignored. Fixed in `crp.rs` (windows by row position); `Batch` indices are now u16 absolute. 12 unit tests pass; 15/15 tracks mesh fully with 0 out-of-range indices.
- Re-render looks like a smooth winding road with embankments and small posts. **Not yet confirmed in the real window by the user.**

## 2026-10-08 - M2 confirmed (flythrough of Alps)
- User run via `run_m2.bat` (pid 6884, 14:54:04-14:55:05): window stayed open until closed with the X, exit code 0, `app finished with: Success`, ~98 fps on Intel UHD, no Ctrl+C or panic. User flew out to see the whole course: road recognisable, some stray geometry remains, no textures.
- The earlier 'closes by itself' reports (M0 14:16 = Ctrl+C exit 130; later M2 reports with no log on disk) were never fully explained. Esc no longer quits (F10 does), in case that was the cause. Unproven.

## 2026-10-08 - Cause of the Ctrl+C exit identified
- User confirmed they pressed Ctrl+C once in the console to copy text, which produced the `Error(130)` exit at 14:16 (Bevy's default Ctrl+C handler). Resolved. The app now ignores Ctrl+C and logs it.
- The later 'closed by itself' reports have no run on disk and remain unexplained; the 14:54 run closed normally.

## 2026-10-08 - M2b textures
- `shpi.rs`: `find`, `decode_rgba8` for formats 0x7D (BGRA) and 0x7E (1555). `crp.rs`: misc table + `materials()`. 16 unit tests pass. `examples/textures.rs` checks every track end to end.
- Viewer groups triangles by material, applies each texture (repeat addressing, linear filter, alpha cut-out at 0.5 where a texture has transparent pixels) multiplied by the baked vertex colour. No mipmaps yet.
- Agent run (20 s, alps): 182 materials, 181 textured, 59,961 triangles, 0.15 s load, ~100 fps, 847 MB RAM, no panic. **Not visually verified**; user playtest pending. Unknown: whether V needs flipping, whether the vertex colours darken too much.

## 2026-10-08 - M2b confirmed, mouse capture added
- User playtest: textures looked right and recognisable (M2b works on alps). User asked for easier mouse look.
- Added mouse capture: left click in the window hides and locks the cursor so the mouse always looks; Esc / Tab / losing focus releases it; right-drag still works; F10 quits. Logged as 'mouse captured/released'.
- Agent run 12 s: starts, no panic. Capture itself **not tested** (needs a click in the window).

## 2026-10-08 - Correction: 'window closes by itself' reports
- The `mouse captured` line at 2.63 s in `m0.log` came from the agent's own 12 s test window (pid 14064), which the user clicked and used; the agent then killed it. `launcher.log` has no entry for that session.
- Several earlier 'closed by itself' / 'no log of my run' reports match the agent's own 12-20 s test windows being stopped. Confirmed user runs: 14:16 (user's Ctrl+C, exit 130), 14:26 and 14:54 (normal exits).
- New rule in AGENTS.md: tell the user before opening any test window on their desktop; prefer offline checks.
- Mouse capture: user reports click-to-capture and smooth looking work. Esc release is **not tested**.

## 2026-10-08 - M3: placeholder car and drivable ground (offline-tested, not yet played)
- New modules in `nfspu-game` (now lib + bin): `ground.rs` (grid height lookup), `car.rs` (arcade car: throttle, brake, handbrake, steering, follows ground, pitch/roll), `track.rs` (road `RD*` and terrain `CNK*` up-facing triangles, start pose from road pieces 0 and 8). `crp.rs`: `article_name`. 
- Viewer: drive mode on by default, red box car, chase camera, window title shows km/h, F toggles fly camera, R resets, F10 quits.
- Offline tests (no window): 13 game-crate tests + 17 formats tests pass. `examples/drive_check.rs`: car grounded on all 15 tracks at the start. `examples/lap_check.rs`: an autopilot driving the road pieces in order completes 12 of 15 tracks (alps 615 s, 3.4% airborne on steep switchbacks where it cuts corners). monaco2/monaco4 fail only because the autopilot follows `(1 PRIMARY)` pieces and those tracks have few; monaco ground continuity beyond the start is **not checked**.
- Mistakes this round: ran an example with `--release` which tried to rebuild all of Bevy in release mode (15+ min timeout); left about 1.6 GB in `target\release` (regenerable, not deleted). One unit-test fixture had the winding backwards; fixed to match the measured data (not the code).
- Not tested: the live window in drive mode (steering feel, camera, collisions with walls: there are none yet).

## 2026-10-08 - M3 confirmed by the author
- User run (pid 18268, launched 15:42:08 via `run_m2.bat` at the user's request, left open): drove from the start (z 464) to about z -4180 on alps in ~4 minutes, top speed 247 km/h, 114 logged heartbeats, no panic/Ctrl+C, Esc release worked (mouse released at 92 s and 102 s). User: 'Works well, feels decent. Car follows the road, controls are usable, camera is fine.'
- Observation: 9 of 114 heartbeats show the car airborne, all on fast (175-220 km/h) descents: the arcade car leaves the ground at crests. To tune later (e.g. stick to the surface when the drop is small).
- Not done: walls / guard rails (cars can drive off the road and over cliffs), real car model, the original physics, other tracks in the live viewer, sound.

## 2026-10-08 - M4 (in progress): real car model, shape only
- `crp.rs`: fixed the car magic check (` raC`), `fd` colour part now optional (white for cars), triangle-part windows now read by row *kind* (fixes car parts; tracks unchanged), `article_base`, `article_transform`, batches with out-of-range indices are skipped and counted (`skipped_batches`). 19 formats tests.
- `nfspu-game`: `car_model.rs` (default-type exterior pieces, finish from material kind, wheels placed by matrix; WheelWell is trim, not a wheel). 18 game tests. Examples: `car_check` (44 cars load), `car_obj` (writes an .obj to `extracted/`).
- Viewer: the red box is replaced by the real car (default 993; `run_m2.bat [track] [car]`), flat paint colours with baked shading, wheels spin with speed and the front ones steer. Falls back to the box if the model fails.
- Offline render of the 993 looks like a 911 coupe. Wheels look blocky in the quick render. **Not seen in the live window yet; not played.**
- Not done: car textures (paletted format + material link unknown), paint colour choice, wheel rim detail, other cars live.

## 2026-10-08 - M4: car textures (offline-verified, awaiting live playtest)
- User playtest of the flat-colour 993: 'body looks almost right, just needs textures; windows should be semi-translucent; wheels are untextured grey squares.'
- Found the car textures embedded in the `.crp` (misc `fs` parts); the `D.fsh` / `L.fsh` files are damage overlays. Offline renders showed the page assignment (paint -> page 0, trim and wheels -> page 2, glass -> page 3): the textured 993 renders as a white 911 with correct panels, lights and alloy wheels.
- Code: `shpi.rs` tiles keep their (x, y) and `compose_page` assembles pages; `crp.rs` `embedded_archives`; `car_model.rs` keeps UVs and material indices per corner and builds the three pages; viewer textures each finish, glass uses alpha blending at 55% opacity, wheels and lights use alpha cut-out. 21 formats tests, 18 game tests pass; `car_check` finds all three pages on every loadable car.
- Mistakes this round: several of my scripted edits silently did nothing because the source files had mixed line endings (found when the compiler complained); fixed by using exact-match editor edits. 
- Not done: paint colour (the page is white/grey; untinted for now), damage overlays, interior, other cars seen live, the original 'environment map' reflections.

## 2026-10-08 - M4: wheels, underside and interior (offline-verified)
- User playtest of the textured car: body good; wheels still broken (grey squares); no interior, underside, driver, or light effects.
- Cause of the wheels and missing underside: the trim page is 128 x 128, I had assumed 256. Fixed by reading page sizes from the car's `.tpg` (`tpg_page_sizes`, with a test). Interior pieces (CarInt) are now drawn with the interior page (embedded archive 1). 19 game tests pass.
- Offline render shows alloy wheels with tyres, a textured underbody and a visible dashboard and seats. **Not seen live yet.**
- Still missing: the driver (LODDriver pieces; suit and head textures not located), light effects (headlights, brake lights), paint colour, decals/badges, damage overlays, steering wheel texture (`stee` is on the trim page).

## 2026-10-08 - M4: cabin from level 1 (flat), page-fit analysis
- User playtest of the fixed wheels: 'a lot better, still missing the dashboard, door side panels, a driver, back seats.'
- `car_model.rs`: second pass reads the cabin (CarInt) from level 1 (1,051 triangles) instead of level 3 (181); articles are recorded per corner (`articles`, `article_names`); `car_obj` exports article names. 19 game tests pass.
- Cabin textures could not be matched to any page (see formats.md); the viewer draws the cabin flat dark grey for now. Driver not drawn (needs the paletted 0x7B decoder and a skin mapping). Not done: light effects (headlights, brake lights), paint colour.

## 2026-10-08 - M4: translucent roof and sides (user report), fix prepared
- User saw the roof and sides of the car turn translucent after the level-1 cabin was added. Likely cause (not yet confirmed live): the cabin's inner shell panels (roof lining, door sides) lie almost on the body skin and were drawn double-sided, so the two surfaces z-fight.
- Fix: the cabin material now culls front faces (the file's clockwise faces are Bevy's back faces), so only faces pointing into the cabin are drawn. Compiles (cargo check); not built or seen yet because the game window was still open.
- Update: built successfully after the user closed the game; 19 game tests pass. Opened for the user at their request; result pending.

## 2026-10-08 - M4: cabin fixed, roof panel variants
- Culling the cabin's inward faces fixed the translucent roof and sides (user: 'Solid again, cabin looks good').
- User: the roof is slightly off, it should be all black. Cause: the exterior page has alternative panel versions at the same spot (`top`/`top1`/`top2`, `sid*`, `fro*`, `rea*`, `gri*`, `det*`), and I layered them all. `top2` has the black roof panel.
- New: `shpi::compose_page_variant` / `select_variant` / `split_variant` pick one variant per spot (default 2, a third launcher argument chooses 0-4). 23 formats tests, 19 game tests pass. How the game picks a variant per car style is not understood.
- Still open: driver, light effects (headlights, brake lights), paint colour, cabin textures.

## 2026-10-08 - M4: body styles from the .tpg (replaces the 'variant 2' guess)
- User: the roof is still not right and variant 2 added thin black racing stripes down the back. The `.tpg` file turned out to define nine body styles and per-tile rules (see formats.md).
- `car_model.rs`: `parse_tpg` (styles, tile rules), `Style`, `TileRule`, `tile_flags`; `load_car(name, level, style_number)` selects piece types by the style (also for the cabin) and draws only the tiles the style uses; `CarModel::style_name`. `shpi::compose_page_filtered`. Launcher third argument is now the style number (default 1). 23 formats tests, 22 game tests pass. Offline: style 1 has Roof, style 2 Roof1 + SunRoof, style 6 CabrioUp and no hard roof.
- Opened with style 6 (cabrio) for the user. Result pending.
- Still open: the driver (needs the paletted decoder and Suit.fsh/Head.fsh), light effects (headlights, brake lights), paint colour, cabin textures, the missing front skirt.
- User with style 6: roof still somewhat off, a rear grille appeared (correct: the engine-lid grille), and the rear window was garbled. Cause of the window: the soft-top window batch sampled the body page; it uses `sgla` from `Cabrio.fsh`. New `cabrio_glass` piece and `cabrio_page`, drawn translucent. 22 game tests pass. Opened again with style 6; result pending. What exactly is 'off' about the roof is not known yet.

## 2026-10-08 - M4 confirmed: textured Porsche with styles
- User (style 6, Carreracabrio, 993): 'the roof and rear window are fixed.' Earlier confirmed: wheels, underside, cabin present, body good.
- Car pipeline now: `.tpg` styles pick piece types and artwork tiles; textures from the car file's embedded archives (exterior page 256, trim/wheels page 128, glass 64) plus `Cabrio.fsh` for the soft-top window; cabin from level 1 in flat dark grey (inward faces only); wheels spin and steer.
- Not done: driver (needs the paletted 0x7B decoder, `Suit.fsh` / `Head.fsh`, skin tile choice by type), light effects (headlights, brake lights), paint colour (body art is white/grey), cabin textures (how the cabin page is assembled is not understood), the front skirt (style wants a piece type the file lacks), decals/badges, damage overlays, other cars seen live (only the 993 has been looked at in the window), and the car on tracks other than alps.

## 2026-10-08 - M4: driver, paletted textures, world mirror
- User: 'roof and rear window are fixed'. Next requested: the driver.
- `shpi.rs`: 8-bit paletted decoder (BGRA palette, attachment header 0x2A), 25 formats tests. `car_model.rs`: `load_car(name, level, style, driver)` seats `LODDriver{type}` (suit and head parts) with textures from `Suit.fsh` / `Head.fsh` filtered by the `.tpg` rules. Launcher fourth argument = driver type (default 1).
- Found the world is mirrored on import (steering wheel on the +X side while the `.tpg` camera is at -0.38). Added `MIRROR_X = -1` applied to track, ground and car; cabin culling follows it. Autopilot laps unchanged. 22 game tests pass. Not yet seen live.
- User: the driver 'looks right' (blue helmet, white racing suit) but the camera makes it hard to see. Added camera modes: **C** cycles chase / cockpit (driver's eyes: x -0.38 from the `.tpg`, height from the model's head; the driver is hidden there) / orbit (right mouse or captured mouse rotates around the car, wheel zooms); the window title shows the mode. 22 game tests pass. Opened for the user; result pending.
- User: the driver and cameras look good; the car is missing side panels between the wheels. Cause: the body page's alpha is a paint mask (partial values), my 50% cut-out punched holes in it. `alpha_to_mask` makes every non-zero body texel opaque (only alpha 0 stays a hole). 23 game tests pass. Opened for the user; result pending.
- Confirmed by the user: 'Yes, fixed' (side panels between the wheels). Confirmed earlier today: driver (blue helmet, white racing suit), cameras (chase / cockpit / orbit), cabrio roof and rear window, wheels, underside, cabin present.

## 2026-10-08 - M4: paint colour and lights
- User chose 'light effects and paint colour'. `CarOptions { level, style, driver, paint }` replaces the growing argument list; `parse_clr` and `tint_paint_mask` (paint scheme from the `.clr` file applied through the body page's alpha mask, then holes only); `headlights` (Light1, Light3) and `taillights` (Light2) are separate pieces with their own materials. Viewer: **H** toggles headlights (bright white-yellow), the tail lights glow red dimly with them and brightly while braking; paint is the fifth launcher argument (default 4 = red). 25 game tests pass; painted page checked by eye (red and white).
- Not seen live yet.
- Build error fixed (a missing 'mut' in the lamp system; my earlier checks had skipped the viewer binary, and a script launched the stale exe by mistake). New build succeeded; opened for the user.
- Confirmed by the user: 'Paint looks right, Headlights (H) glow, Brake lights glow.' (993, cabrio style, paint 4). Confirmed earlier: driver, cameras, side panels.
- Other cars: offline all 12 tested load with textures. User asked me to close the 993 window; opened the 928 (style 1, paint 4). The 928 and 959 show dark patches on the sides in my offline render; not yet seen live.
- 928: user saw black spots under the side mirrors (door hinge). Cause found offline: DoorIn (cabin door panel) sticks out through the body skin (|x| 0.89 vs body 0.85). Fix: DoorIn x scaled by 0.94. Also reported, not fixed: tail lights clip a little through the body; shading oddity on the driver-side panel in cockpit view; the 928's front lights are articles HeadLight/Light4 (not Light1/3), so H does not light them. Reopened the 928; result pending.
- 928 spots still there after the DoorIn squeeze (reverted). Real cause: the cabin comes from level 1 (detailed) but the body from level 3 (simpler); on the 928 the level-1 cabin is 0.89 m wide at the door's front edge where the level-3 body is 0.86, so it pokes through. Fix: two cabins, `interior` (level 1, cockpit view only) and `interior_outside` (the exterior's level, all other views); measured inside the body offline. New example `car_articles` lists a car's articles with triangles per level. 25 game tests pass. Reopened the 928.
- User: 928 door spots gone, cockpit OK. Remaining: no headlight glow on the 928, tail lights clip. Fix: `lamp_end` classifies any painted `Light*` / `HeadLight*` article by position (front if z < -1, rear if z > 1, side markers skipped); lamp triangles lifted 5 mm along their normal. Offline, all 13 cars checked now have both headlights and tail lights (928: 176 / 14; 356a had 0 tail lights before). 26 game tests. Reopened the 928.
- Confirmed by the user on the 928: door spots gone, cockpit OK, headlights glow (H), brake lights clean. Not yet re-checked: the shading oddity on the driver-side panel in cockpit view (reported once, before the two-cabin change).
- User chose to go on to the Boxster (cockpit shading on the 928 left for later). Closed the 928 at the user's request; opened boxster style 1.
- User found a giant flagman (`Sawdude1`, about 138 m tall in the file) and other figures in a pile near the world origin on alps. Cause: track articles with a transform are objects, but the viewer drew them all untransformed at the origin. About 110 roadside signs (`OBJ####L/R (... _SGN)`) carry a real position, and about 70 templates (traffic cars, cones, people, the helicopter is separate) sit at the origin and are placed by data we don't read yet. Fix: apply the transform (row-vector, p * M) to placed objects; skip transforms within 50 m of the origin (templates). New example `track_articles`. Boxster: user says the car looks good. Reopened boxster.
- User: giant still there, no signs seen. Giant: `Sawdude1`'s transform is (5, 68, 6), 68 m straight up, so the 3D 50 m test let it through; now horizontal distance only. Signs: `sign_check` example shows all 111 placed objects 0-2 m from a road vertex at road height; the first is about 570 m down the road from the start. Reopened.
- User: giant gone, signs visible but 'turned the wrong way' and 'mirrored'. Findings (`sign_check`, `sign_one`): the rotation reading p * M is right (signs square to the road, 0.99); each sign has a front quad (`sgn1` artwork), a back quad 2 cm behind it (`sgn2`, the back of the sign) and a post. Track materials were double-sided, so faces showed from behind, reversed. Signs ending R (about 58) stand on the right and face a driver going in increasing road-piece order (the way the car starts); signs ending L (about 53) are for the other direction. Fix: objects with a transform are drawn one-sided (file front faces only); terrain and road stay double-sided. Reopened.
- User: signs face the driver and read correctly, plain backs on the other side; but posts vanish from behind (a post is one flat strip). Fix: `back_to_back_batches`: only a batch with an opposite-facing partner over the same area (front and back boards) is drawn one-sided; other object faces stay double-sided. Reopened.
- User: signs good; posts look slightly off-centre (file has the post at the board's centre x, left as is); cars missing licence plates. Finding: plates are built at runtime. `[file3.lice] license=1` in the `.tpg` marks tile `lice` on the trim page (`extint`) as the plate; `Render/license.psh` (`SHPP`, 37 entries, format 0x40, not decoded) holds `blnk` 48x22 and glyphs a-z, 0-9 (7x12); `CarModel/license.ffn` is a font (`FNTF`). The `License` article was sampling the body page (black there); now routed to the trim page (blank grey plate with mounting holes). User chose blank plates for now. Opened the 959.
- 959: plates show; door handles missing and see-through holes where they belong. Cause: the handle lies exactly on the body surface (x 0.790 vs 0.789) and lost the depth test; the body texture has a cut-out there for it. Fix: handles and plates get the same 5 mm lift as lamps. Closed the 959 at the user's request and reopened.
- 959 handle holes, attempt 1 (5 mm lift) failed. Real cause: the handle's UVs pointed at the middle of the wheel artwork on the trim page (its dark spoke gaps looked like holes). The handle's material kind (`CarExt`) is the same on every car, but on the 959, 993, Boxster, GT3, 964, 996, 944 and 914 the UVs fit a `hand` tile on the **body** page; on the 928 they fit the trim page. New `handle_on_body_page`: a Handle batch whose UV box lies in a body-page `hand` tile uses the body page. 27 game tests. Reopened the 959 (attempt 2).
- Confirmed by the user: 959 door handles fixed (attempt 2). Cars confirmed live so far: 993, 928, Boxster, 959.
- User chose the GT3 next; closed the 959 at their request and opened gt3 style 1.
- GT3: user says it looks right except gaps around the tyres where you can see into the car. Cause: the wheel-arch liners (`WheelWell`) sample one grey texel (110,110,110) in tile `stee` on the trim page whose alpha is 0, so the cut-out made them invisible on every car (993 too; it went unnoticed). Fix: liners go into a new `trim_solid` piece drawn with an opaque copy of the trim material. Opened the GT3.
- Confirmed by the user: GT3 tyre gaps fixed. Cars confirmed live: 993, 928, Boxster, 959, GT3.
- User chose the 964 next; closed the GT3 and opened 964 style 1.
- 964: 'almost perfect', but a flat grey/white patch on the driver. Cause (my bug from the two-cabin change): the outside cabin pass at level 3 picked up `LODDriver1` (material kind CarInt) as cabin and drew it a second time in flat grey over the textured driver, on every car. Fix: `LODDriver` added to `is_hidden_in_cabin`. Closed and reopened the 964 at the user's request.
- Confirmed by the user: 964 driver fixed. Cars confirmed live: 993, 928, Boxster, 959, GT3, 964.

## 2026-10-09 - All cars and traffic cars (offline pass)
- User asked me to go through every car, traffic included, then check them. Tools: `car_audit` (loads every car like the viewer; reports cut-out holes per part and article, cabin poke-through, missing wheels / lamps / driver / plate, sizes), `uv_where` (which tiles on every page contain a part's UVs), `car_archives`, `tile_raw`; offline renders of all 43 cars (`extracted/sheet_*.png`, `traffic_*.png`).
- Pages are now chosen by the file names in the `.tpg` (`exterior`, `interior`, `extint`, `glass`) instead of archive position. This fixed the police cars (sirens, roof, body holes) as a side effect.
- Traffic cars (`eas1-11`, `eden1-5`): one exterior page (body, tyres) plus an interior page (lamps, glass, plate, wheel faces, underside, mirrors), no trim page, and a crude level 3. They now load at level 1 (about 3x the triangles, with lamps and plates), with both pages side by side in one texture (`TrafficAtlas`). Each triangle points into the page whose tile contains its UVs; ties are broken by tile name (lamps `lit`, plate `lice`, wheels `whe`/`tre`, liners `tre`), then by which page is opaque. Their own driver (`DriverBody0`, texture `tdri` in format 0x7F = 24-bit B,G,R, new decoder, checked by eye on skin tones).
- Smaller fixes: one-texel (solid colour) trim samples are drawn opaque (944 underside); `ShinyDash` (356) belongs to the cabin; only `License` batches that fit the `lice` tile go to the trim page (356 chrome plate frame stays on the body page); handle tiles matched per triangle (928 grip + lock); 16-bit overflow in tile rectangles fixed.
- Viewer: **N / B** next / previous car (all 43, sorted), **V** next body style; the title shows car and style. Car building moved into `spawn_car_model`.
- Expected leftovers (not bugs): police 356s have no driver in the file; `eas3` has no lamp articles, `eas4` no tail lamp or plate; cabin poke-through counts are false alarms (seats and roof lining where the rough body outline has no vertices); `postship` is not a car (wheels and a 689-triangle decal) and is left out. GT1 door handle samples the edge of the wheel artwork (no better match on any page) - to be checked by eye.
- 26 formats tests, 29 game tests. Build note: building all examples at once ran out of memory (paging file); use `-j 2` and build only the needed examples.
- Opened the game for the user's check: alps, first car of the list (356_1); N / B to step through all cars.

## 2026-10-09 - Debug console, and the inside of the roof
- User asked for a debug console to switch cars (and later tracks) on the fly, and reported that the GT1's roof lining shows black body artwork from the cockpit and orbit views.
- Console: `~` opens it (Escape closes, Enter runs, Backspace deletes). Commands: help, cars, car [name], style [n], paint [n], driver [n], tracks, track [name], cam [0-2], lights [on|off], reset, info, clear, close. Car and track names may be shortened (`car box` = boxster) if unambiguous. While it is open the driving keys are ignored. Typed characters come from the keyboard events. 3 tests in `console.rs` (binary crate: `cargo test -p nfspu-game --bin nfspu-game`).
- Track loading moved into `spawn_track` / `reload_track`, so `track <name>` loads a new track in place: old meshes despawned, ground and camera replaced, car back on the new start line.
- Roof: the GT1 has no `InteriorRoof` article, so the inside of the body shell (triangles whose front face points down) showed the body page - black. Those now go into a new `paint_inner` piece drawn with the flat cabin material, like the separate roof lining on cars that have one (76 triangles on the gt1 and the 993, 270 on the 928). Not seen in the game yet.
- Bevy allows 16 parameters per system, so `console::apply` groups them into `CarState` and `World` SystemParam structs. Building all examples at once runs the machine out of memory; build with `-j 2` and only the examples you need.
- Console: the space bar arrives as `Key::Space` rather than as a character, so it needs its own case in the key handler (user hit this first). Backspace/Enter/Escape are plain `Key` variants in Bevy 0.19. 3 console tests pass.
- Console fix: pressing ~ typed its own backquote character; the toggle is now handled from the same key event and never typed. FPS counter (**F3**) in the top-right corner. Skybox: each track's `<track>.env` holds seven 64x64 pictures (a cube map plus one spare, all SHPI 0x7d); the six faces are put into one cube map (`1` below the world, `2` the clouds overhead, `3`/`BT1`/`BT2`/`YT1` the sides) and shown through Bevy's `Skybox`. Which face belongs to which side is a guess, so the sun may be in the wrong part of the sky - check by looking up. Bevy 0.19 notes: `Skybox` is a component on the camera, not a resource; wgpu 29 has no cube `TextureDimension`, the cube-ness is in `TextureViewDescriptor.dimension`.
- Sky: the .env file is not a cube map. Of its seven pictures only three are sky (2 = clouds overhead, 3 = clouds round the sides, 1 = black under the world); BT1/BT2/YT1/YT2 are sun glare sprites (two blue, two orange). The dome now uses 3/2/1 only; the glare sprites are not drawn yet because which way the sun should be is unknown. Cube-map route (Bevy Skybox) tried first: rendered black, abandoned.
- Sky work, findings: the `.env` files are not cube maps (7 pictures: 1 black, 2 and 3 dark cloud layers, BT1/BT2/YT1/YT2 sun glare sprites). The real sky system is `GameData/Track/Sky/<track>.{fsh,txt,bin}` plus `Render/night.psh`: a `horz` mountain panorama for the horizon, white cloud layers, two weather regions per track and a binary key (9 records on alps) that has not been decoded. Our dome currently uses the .env cloud pictures and looks dark and blurry for exactly that reason.
- Not done: reading the sky colour key from the `.bin`, the horizon panorama, the night stars, weather particles (snow in the alps frames).
- Sky, part 2 (decided to make it per-track and data-driven): new `crates/nfspu-game/src/sky.rs` reads `GameData/Track/Sky/<track>.{txt,fsh,bin}` for any track. 15 tests. `main.rs` now builds the sky from that data instead of the `.env` cloud pictures: a dome tinted with the weather region's sky colour, plus a band round the horizon carrying the `horz` mountain panorama, both tinted from the `.bin` colour table by how far along the lap the car is (`sky::LapPath` + `sky::horizon_tint`). New `sky_info` example prints what was read for every track. Old `.env` dome code and `face_uv` removed.
- Still not done: the cloud layers (`cc1a`/`cm1a`) are read but not drawn; `hzcl` horizon haze is not drawn; the sun glare sprites and `Render/night.psh` stars are not drawn; weather particles (snow) are not drawn. Not tested on screen yet.
- Sky tested on alps (user, first run). Sky itself reads better, but three problems, all confirmed:
  1. The `horz` mountains are stretched into thin horizontal streaks. Cause: the band wraps the 256x256 picture once round 360 degrees at radius 2910, so each texel covers about 1.4 degrees across but only 0.02 degrees up. The aspect is wrong by roughly 60:1. `horz` is one panorama segment (its left and right edges differ more than its middle does), so it must not simply be wrapped once.
  2. No warm/cold change while driving. The code works - offline `sky_lap` shows 6 colour changes over alps - but only inside the first fifth of the lap. **The `.bin` colour table covers 0..2104 m of a 10594 m lap, so it cannot be what makes the sky change round the whole track.** Something else drives the warm/cold change and is still unidentified.
  3. A floating lamp in the middle of the alps village. Separate track bug, nothing to do with the sky.
- Mountain proportions fixed properly: offline renderer `extracted/horz_preview.py` and `horz_sheet.py`/`horz_sheet2.py` reproduce the squashed look and compare settings without opening the game. Cause was that `horz` was wrapped once round 360 degrees at radius 2910, making texels about 1.4 deg wide by 0.02 deg tall (60:1). Fixed by repeating the picture so texels are square: repeats = 360 deg / band height in degrees. Band set to +0.50..-0.08 radii (31 deg tall, 12 repeats), which reads best against the reference frames. The band's texture coordinates now run past 1, so its sampler had to be set to Repeat. Built, not yet seen on screen.
- Warm/cold cause: still unresolved after reading the decomp. `SkyHorizon::vfn03_00459d30` shows the horizon is 16 segments whose colours are indexed by segment number and wrap at 16 - not by distance along the track, which is the model we implemented. So our distance-based tint is the wrong model and needs reworking.
- Horizon mountains: attempt 3 (trimming the transparent top rows, 15% coverage) made it worse - it cut the peaks off and left flat ridges. Reverted to counting any opaque pixel, and the test now asserts a sparse peak row is kept. 50 lib tests pass.
- Mountains: went back to a single band for the whole `horz` picture at the user's request (the two-ring split did not remove the sliver and added complexity). Kept the square-texel repeat rule, which is what stopped the 60:1 stretch. 50 lib tests pass.
- Floating lamp in the alps village, investigated: **not a placed object.** alps has 111 placed objects (anything with a transform over 50 m from the origin) and every one is a roadside sign (`*_Rigid1_SGN`), a distance marker or the GasStop - there is no lamp article at all. Static track geometry is drawn double-sided and untransformed, so our `MIRROR_X` face culling (which only applies to placed objects) cannot be hiding a support. The new `floating` example found nothing floating over 2 m in the village area, but that check only sees road (`RD####C`) and terrain (`CNK####L/R`) pieces - village buildings and their lamps live in other articles that `track::load_ground` does not load. **Not yet located.**
- Sky colour is now direction-based, matching `SkyHorizon::vfn01_0045a1a0`: the `u` field of each colour record is the direction (a fraction of a turn), the second dword of the `.bin` is the horizon's zero point (alps 0.8621 = 310 deg), and the camera's yaw is reduced by it. Offline sweep of alps: 14 colour changes over a full turn, warm rgb(117,70,48) to cold rgb(92,119,165). `LapPath` and the distance-based tint are gone; `sky_turn` example replaces `sky_lap`. `sky_tint` moved after the camera updates in the schedule (it was reading last frame's camera) and logs its result twice a second.
- **Problem, not yet solved:** the colour records' directions only span 0.057..0.260 of a turn - about 73 degrees of the horizon. Every other direction finds nothing within reach and falls back to the average of all the records, which is a flat warm grey. So turning shows a real but subtle change in the first 60 degrees and nothing much elsewhere; the strong cold blue only appears near 300 degrees. Either the file has more colour tables we are not extracting, or this is all the data there is. Log evidence at 16.18s-25.46s: heading 0-45 deg gives rgb 104,95,104 -> 107,85,83.
- Floating lamp: the 70 skipped origin templates on alps are traffic cars, cones, boxes, people and signs - **no lamp, pole or wire**, so the support is not something the viewer skips. Searching all articles for small pieces hanging above the ground found exactly one: `OBJ0265L (1 scurve_Rigid1_SGN)` at (-30.5, 76.3, -560.9), a 1.51 x 3.32 x 0.02 sign panel whose bottom is 6.36 m above the ground with no post under it. That matches the user's theory that something supporting a hanging object is missing, but it is a sign, not a lamp, and it is not in the village. **Not yet located.**

## 2026-10-15 - Retail horizon texture atlas
- Rechecked the static read-only reference and raw sky headers before changing rendering. Replaced guessed whole-image repetition with 16 separate atlas quads: configured cell order/flips, 0.01/0.98 row insets, turn-based rotation and track-specific bottom/height relative to radius 400. Reflected X like the track; uniformly scaled the ring to keep its corners inside the dome.
- Added bounded header parsing and offline `horizon_audit` example. All 17 atlas headers pass independent reference-equation checks (64 vertices each); two lightning files use another format. Corrected Monaco's documented flip count to three; colour-start affects resource colour lookup, not atlas order or geometry.
- Sky-colour mapping is still **unconfirmed**. Retained the existing guessed tint; corrected misleading source comments and marked historical hypotheses as superseded in provenance/status. No game data or decompiled source added.
- Checks: 50 game-library tests, 3 console tests plus 1 horizon mesh test, 26 formats tests pass; offline atlas audit passes. Final dev viewer build passed (1m 30s). Used only required targets with `-j 2`; no release build, live game window, process termination or commit.
- Run offline: `$env:NFSPU_GAME_DIR="C:\Users\CB\Downloads\NFS5"; cargo run -p nfspu-game --example horizon_audit -j 2` (17 PASS, two unsupported lightning headers).
- Playtest: `cargo build -p nfspu-game --bin nfspu-game -j 2`, then `.\run_m2.bat alps` or `.\run_m2.bat monaco1`. Expect configured atlas cells with track-specific mountain heights, replacing repeated whole pictures. Live visual agreement, remaining sliver and colours: **not tested**.

## 2026-10-15 - Continuous fidelity testing and repeatable captures
- Recorded user permission to open/close test games and the goal of retail fidelity in AGENTS.md and DESIGN.md. Other project boundaries remain in force.
- Added opt-in `NFSPU_SKY_CAPTURE=1`: four fixed horizon headings 250 m above the track start, saves PNGs and numeric poses in ignored logs, exits intentionally only after saving captures. Normal play is unchanged.
- First live Alps capture run completed successfully on NVIDIA RTX 4050/Vulkan without changing GPU settings. Four baseline images are preserved as `logs/before_horizon_alps_*.png`; raw live log is `logs/visual_probe_alps_before.log`. Matched retail visual parity is **not tested**.
- Run: `$env:NFSPU_GAME_DIR="C:\Users\CB\Downloads\NFS5"; $env:NFSPU_SKY_CAPTURE="1"; .\target\debug\nfspu-game.exe alps`. Expect four `logs/horizon_alps_*.png` files and an intentional exit. Clear the option afterward with `$env:NFSPU_SKY_CAPTURE="0"` for normal play.

## 2026-10-15 - Horizon palette reference correction (verified static assignment)
- Directly mapped executable string addresses rather than inferring string order: the horizon colour resource is `hzcl`, not `horz`. Raw SHPI lookup proves its two rows supply top/bottom vertex colours; the .bin colour records are not used by this horizon constructor.
- Added per-vertex hzcl row/start/wrap assignment, independent of cell order and mirroring. Rendering uses a white base colour and bypasses guessed whole-band tint when that verified palette is present. Guessed fallback/dome tint remains explicitly unconfirmed.
- 51 library tests, 4 binary tests and the debug viewer build pass. Expanded raw-file atlas/palette audit passes all 17 sky headers. Four-heading live captures complete on Alps and Monaco1 (RTX 4050/Vulkan), with intentional clean exits. Full sky-colour interpretation, gamma parity and the warm/cold effect remain **unconfirmed**. Matched retail parity: **not tested**.
- Run normal play: `$env:NFSPU_SKY_CAPTURE="0"; .\run_m2.bat alps` (or `monaco1`). Horizon texture is now shaded by its own top/bottom hzcl colours, rather than a guessed uniform tint.

## 2026-10-15 - Track triangle batch boundaries
- Monaco capture warnings exposed mismatched normals and other vertex attributes: trailing batch corners were concatenated into the next batch before normal generation. The viewer now keeps complete triples within each batch and logs excluded tails by article/batch/material. Format-reader data remains intact.
- Synthetic regression passes, including out-of-range tail indices that must never be read. Explicit offline audit passes all 15 tracks: finite positions, complete triples, equal attribute counts. Five normal binary tests pass; the install-backed test also passes when run explicitly. Debug viewer build passes with `-j 2`.
- Raw indexed Alps headers associate non-triple counts with primitive kind 1, while ordinary triangles are kind 3. Primitive semantics remain **unconfirmed**; this fix prevents cross-batch corruption but does not claim the source corners are malformed. This is the next fidelity investigation.
- Run: `$env:NFSPU_SKY_CAPTURE="0"; .\run_m2.bat monaco1`. Expect independent triangle batches; exact retail geometry parity remains **not tested**. Offline audit: `$env:NFSPU_GAME_DIR="C:\Users\CB\Downloads\NFS5"; cargo test -p nfspu-game --bin nfspu-game -j 2 real_tracks_have_complete_independent_triangles -- --ignored --nocapture` (15 tracks PASS).
- Post-fix Monaco1 live run completed four captures and exited Success; stderr contains no WARN/ERROR/PANIC entries. Logs: `logs/track_batch_monaco_live.log`, `logs/track_batch_monaco_stderr.txt`.

## 2026-10-15 - Verified triangle strips (validation in progress)
- Traced primitive header submission and renderer dispatch in the read-only reference: mode 1 is a triangle strip with alternating winding, mode 3 is a triangle list. Earlier tail trimming prevented cross-batch corruption but did not decode strips correctly.
- Reader now expands strips into triangles with vertex indices, UV indices and per-corner colours in the same order. Repeated-index degenerates preserve strip parity; raw colour-cursor accounting still uses the source count. Unknown indexed modes are reported through skipped_batches.
- Raw inventory: 59 CRP files, 497 strip batches and 89,665 triangle-list batches. All 44 CarModel files use triangle lists; Monaco's tunnel/wall/building surfaces include strips.
- Formats tests: 28 pass. Full indexed normalization audit: 34,171 meshes / 1,417,518 triangles across 59 containers, zero skipped batches. Game tests, viewer build and live captures pending.
- Offline: `$env:NFSPU_GAME_DIR="C:\Users\CB\Downloads\NFS5"; cargo run -p nfspu-formats --example primitive_audit -j 2` (expect only modes 1 and 3 and successful per-file CHECK lines). After build: `$env:NFSPU_SKY_CAPTURE="0"; .\run_m2.bat monaco1` (strip-based tunnel/wall/building surfaces now have their full triangles; matched retail parity **not tested**).

## 2026-10-15 - Verified triangle strips (validation complete)
- Completed the pending validation: 28 formats tests, 51 game-lib tests, 5 binary tests pass; install-backed all-15-track audit passes (15/15 PASS, monaco1 242 groups / 28,911 triangles).
- Full 59-container audit: 34,171 meshes / 1,417,518 normalized triangles, 0 skipped batches; only modes 1 (497 batches / 9,832 corners) and 3 (89,665 batches / 4,226,040 corners) observed.
- Live Monaco1 and Alps four-heading captures exit Success on RTX 4050/Vulkan with no WARN/ERROR/PANIC in stderr. Matched retail visual parity remains **not tested**.
- Logs: `logs/primitive_formats_tests.txt`, `logs/primitive_game_tests.txt`, `logs/primitive_bin_tests.txt`, `logs/primitive_track_audit.txt`, `logs/primitive_normalization_audit.txt`, `logs/primitive_build.txt`, `logs/primitive_strip_monaco_*`, `logs/primitive_strip_alps_*`.

## 2026-10-15 - Wall collision v1 (embankments stop the car)
- Survey (`wall_survey` example, `logs/wall_survey.txt`): road pieces are almost entirely flat (steep-road ~0 on all tracks); rails live in the terrain — steep CNK faces stand near most of the centerline (alps 182/225, autobahn 26/26).
- `track.rs` keeps near-vertical RD/CNK faces as `walls`; new `collision.rs` grid pushes each axle circle (r = 1 m) out of overlapping walls whose height meets the body (bridges/high signs ignored). `car.rs` resolves front/rear circles after the ground follow, scrubs speed while scraping, reports `hit_wall`. Viewer, console reload, drive_check and lap_check all pass walls through.
- Checks: 55 lib tests (3 collision + 1 car-wall), 5 binary tests pass; drive_check unchanged (~89% grounded, 31.6 m/s everywhere); viewer build passes; live alps capture exits Success with 20,394 wall triangles loaded, no WARN/ERROR/PANIC.
- Known harness effect: the lap_check autopilot (no reverse logic beyond backing out) now gets pinned by roadside embankments on alps (wp 75/225, wall contact 81.9%) where it used to cut corners through them — collision working, autopilot dumb. Lap metrics will be re-baselined once the harness learns to stay on the road.
- Playtest: `.\run_m2.bat alps`, drive off the road into a hillside — the car should stop/slide, never pass through. R resets. Tunneling-free behaviour live is **not tested** (needs your eyes).

## 2026-10-15 - Collision v2: slope limit and rigid ground probe
- Live reports: cliffsides still driveable on alps; monaco had fall-through spots. Measured both with a new offline `ground_audit` example (`logs/ground_audit.txt`).
- Cliffs: the drivable set took up-facing faces up to 76 deg (alps 8,389 faces over 40 deg), so cliff sides were road. `track::classify` now uses a 40 deg slope limit (`DRIVE_SLOPE_COS`); steeper up-facing faces become walls. Road triangles are unchanged on all 15 tracks — only steep terrain moves.
- Monaco fall-through: 1-4 m pockets between the road edge and high-wall bases (22 on monaco1). The single-point height query fell into them. New `Ground::surface_near` finds the nearest surface within a radius using its closest outline point; `car.rs` probes in tiers (1.5/3/6 m) before falling, so a rigid car bridges piece joins.
- No track has inverted-winding road faces (0 in the audit), so the winding facing test stays, now combined with the slope limit.
- Checks: 57 lib tests pass (slope classes, gap bridging, no lower-deck drop, wall stop); install-backed `real_tracks_have_no_fall_through_on_the_road` passes all 15 tracks (monaco1: 0 misses of 14,624 road points and 54,840 points up to 5.5 m off the road); `lap_check` now completes alps 225/225 (was pinned at 75) and monaco1/3/5, skidpad; monaco2/4 still defeat the autopilot (6 primary road pieces, sparse waypoints — not a collision failure). Viewer builds; live monaco1 run exits Success with 15,959 wall triangles and no warnings.
- Playtest: `.\run_m2.bat alps` then `.\run_m2.bat monaco1`. Expect cliffs to block the car, no fall-through at the wall bases, and slower/noisier corner cutting (`hit_wall` scrubs speed). Live feel is **not tested**.

## 2026-10-15 - README status summary
- Added a 'Status at a glance' section to README.md (works / missing / checks / next / build-and-run).
- Re-verified: `cargo test --workspace -j 2` passes 40 formats, 77 game-lib, 5 binary tests (2 ignored); `run_m2.bat` and `run_menu.bat` exist.


## 2026-10-15 - Menu gradient boxes (NFS5.GOURAUDBOX)
- Format from the shipped `GouraudBox.ctr` plus the retail control class (read-only reference): four ARGB corner colours, eight per-corner offsets added to each corner (`BLoffy` spelling kept), hidden state draws nothing. Notes in `docs/provenance/fe.md`.
- `menu.rs`: new `Draw::Gradient`; `menu_mode.rs` draws it as a vertex-coloured 2D mesh (alpha blended). Known limit: always under UI quads/text.
- Fixed: `NFSPU_MENU_CAPTURE=1` now exits after saving (it used to stay open).
- Checks: 40 formats, 79 game-lib (+2 gradient tests), 5 binary tests pass. Menu audit: draw commands 1,500 -> 2,076; controls not drawn 1,936 -> 1,360. Captures: `logs/menu_GameSetup.png`, `logs/menu_selectname.png` (popup frame matches the retail layout's shape). Retail colour parity **not tested**.
- Note: `accountlogin` is an online-only screen retail never shows offline; the real first screen is `GameSetup` with the `selectname` popup on top.

## 2026-10-15 - Main menu layers, menu pictures, draw order
- User's retail screenshot: the start screen is `GameSetup` with the `selectname` popup over it (not `accountlogin`, an online-only screen).
- Layers: frame from `screen=ID_X` -> `screenx.lay` (verified in the executable), page, popup (`room=ID_NONE`, guess), `bottombar.lay` on framed pages (guess). The 3D room is not drawn.
- New draws: `NFS5.PICTURE` (natural size, flip bits 1/2) and `NFS5.POPUPCIRCLE` (four mirrored quarters of `PopUp.fsh`; the 207x175 quarter equals the template offsets). SHPI reader gains ARGB4444 (0x6D) and RGB565 (0x78). New `fe_art_audit` example: 930/1,354 front-end images decode (was 459).
- Draw order: the first section is now on top (layouts list backing panels last). Clicks follow the same order.
- Gradients are now rasterised to UI images, so they stack in order with everything else (the 2D mesh path is gone).
- Capture: `logs/menu_gamesetup.png` matches the screenshot's structure (oval frame, bevelled panel, portrait frame, buttons over the circle). Not drawn yet: room, list box, portrait, bottom-bar buttons, page dimming, fonts.
- Checks: 41 formats, 83 game-lib, 5 binary tests pass (2 ignored). Menu audit: draw commands 2,076 -> 2,636; controls not drawn 1,360 -> 857.

## 2026-10-15 - Select-user list and driver portrait
- From the executable: the user list is `<save dir>*.sav` names minus `XXDefXX`, sorted; `player.lst` names the selected one. Faces come from `ALLdrivr.fsh` (9 x 54x64).
- `menu::Profiles` reads the save folder (default `<game>\SaveData`, override `NFSPU_SAVE_DIR`; read-only). `NFS5.ListBox` draws rows (click selects), `NFS5.PLAYERPICTURE` draws the face. A player's chosen face is not in our decoder yet, so face 0 is shown (guess).
- SHPI reader: paletted entries with palette type 0x29 (RGB565) now decode. New `fsh_dump` example writes PNGs to the ignored `extracted/`.
- This install has no saved players, so the list is empty in a normal run. Capture with a scratch folder (empty `Driver.sav`, `Greg.sav`, `player.lst` = Greg) matches the user's retail screenshot layout. Missing: the list's white backing (`GlintyRectangle`), hover tint, real fonts.
- Checks: 42 formats, 85 game-lib, 5 binary tests pass (2 ignored). The old 'unknown palette' test used 0x29, now supported; it uses 0x22 and a new RGB565-palette test was added.


## 2026-10-15 - Player face from the save file name
- From the executable: the user list draws each row from its 2nd character and the portrait takes the 1st as the face digit (clamped 0..8), so `0Greg.sav` = 'Greg', face 0. The face is not inside the .sav.
- `menu::display_name` / `face_index`; list shows names without the digit and sorts by them (retail screenshot shows Driver above Greg); portrait uses the selected save's face.
- Capture with scratch saves `8Driver.sav`, `0Greg.sav` matches the retail screenshot's order and face.

- Checks: 42 formats, 86 game-lib, 5 binary tests pass (2 ignored).


## 2026-10-15 - Real install is the copy
- User: the real (played) install is `C:\Users\CB\Downloads\NFS5 - Copy`; Greg is the user's own test profile, not shipped. `run_m0/m2/menu.bat` and AGENTS.md now point there (both installs read-only).
- Verified on real files: copy's SaveData has `3Greg.sav`, `8Driver.sav`, `player.lst` = `3Greg`. The face-digit-in-file-name finding holds; Greg = face 3 (`M04`), which is the face in the user's retail screenshot (earlier note wrongly matched it to `M01`). Capture from the copy matches the screenshot: Driver above Greg, Greg selected, face M04.


## 2026-10-15 - List outline and backing
- `NFS5.GlintyRectangle` drawn as a 1-unit outline in `col` (template parts); glint animation not drawn.
- List box backing: template `selectionbox` rect at alpha 0x40, drawn white (retail screenshot measured ~26-31% white; template colour is editor green). Guess, measured.
- `selectname` now has 0 undrawn controls. Capture from the copy matches the screenshot's list outline and tint.


## 2026-10-15 - Bottom bar buttons
- `NFS5.BOTTOMBARBUTTON` drawn per its template: icon frame + label, right alignment for exit. back = Back, exit = quit (no confirmation; retail not checked). Label widths use a measured 6.5 units/char at 11pt until fonts are read (guess).
- String 0 (placeholder 'Hello, World!') now means no label; fixes a stray label next to RACE.
- Click targets generalised (`Draw::Target`) for list rows and quit.
- Checks: 42 formats, 88 game-lib, 5 binary tests pass. Menu audit: draw commands 2,636 -> 3,408; controls not drawn 857 -> 650. Capture from the copy shows back arrow, RACE, options and exit icons like the retail screenshot. Still differs: an 'ACCOUNT' label retail hides, hover animations, fonts, dimmed page under the popup.


## 2026-10-16 - Retail menu fonts (.ffn)
- New `nfspu-formats::ffn` reader: FNTF header, 224 x 11-byte glyph records (codes 32-255), 4-bit coverage atlas (high nibble first). Layout worked out from the files and checked by eye (`ffn_dump` example writes atlas + sample line to `extracted/`).
- Menu text is rasterised with the named font (labels, buttons, list rows, bottom bar); engine font only if a font file is missing. Label widths now use font advances.
- From templates: `REAL.LABEL` align (0 left, 1 centre, 2 right), `GOTOBUTTON` medium label styles (popup buttons centred in Universc11m).
- Checks: 44 formats, 89 game-lib, 5 binary tests pass. Capture from the copy: main menu text, SELECT USER heading, centred popup buttons and bottom bar match the retail screenshot's lettering. Text is softer than retail when scaled up (linear filtering).


## 2026-10-16 - Page dimmed under popup, ACCOUNT hidden
- From the executable: control flags 0x20 hidden / 0x10 disabled; the bottom bar shows `ID_CASHBKGD` + `ID_ACCOUNTTHINGY` only in career mode (GameType 4). Both now hidden (no career mode yet).
- Under a popup the page is drawn in its disabled state (template `disabledcolor` 0x30FF0000 fill, labels at alpha 0x30, no hover/clicks). Matches the retail screenshot; the switching code is not traced (guess).
- `menu::Context` bundles profiles, game dir and the disabled flag for control building.
- Checks: 44 formats, 91 game-lib, 5 binary tests pass. Capture from the copy: faint ring buttons and no ACCOUNT label, like the retail screenshot.


## 2026-10-16 - Compared with a retail recording
- User's recording read as frames with ShareX's ffmpeg (user approved running it; frames kept in the session scratch folder, not the repo). Observations in `docs/provenance/fe.md`.
- New start option: `NFSPU_MENU=page+popup` (e.g. `GameSetup+singleplayer`; `+none` = no popup). The single player popup draws over the dimmed main menu as in retail.
- Main gaps found: centre video + 3D room, button shading/drop shadow, grey 'back'/'RACE' on the main menu, difficulty dropdown; retail's main menu opens without the select-user popup (ours still opens it by default, pending a decision).


## 2026-10-16 - Button gloss and drop shadow
- Start-up popup kept (user: the game shows it at first launch); `GameSetup+none` starts without it.
- Transition buttons get the retail gloss (top half 50% white to clear, bottom half clear to 57% black) and the `dropshadow=1` shadow (4,4 offset, 80% to 10% black strips with 5% tapers), per the retail draw routine; drawn as gradient quads.
- Draw order: each control orders its own pieces (topmost first); the global text lift is gone so the gloss can tint labels like retail.
- Checks: 44 formats, 91 game-lib, 5 binary tests pass. Side-by-side with the retail recording: shading and shadows match by eye (bright and disabled). Hover shading and dropshadow=2 not drawn.


## 2026-10-16 - Grey back/RACE, button hover look
- From the executable: RACE follows the top screen's `canplaygame` (0/empty = grey); back is grey at the root screen. Disabled bottom-bar buttons: grey 0xff808080 label (matches recording), icon at alpha 0.5, no clicks.
- Transition button hover: yellow fill, pressed-in bevel (retail hover routine, drawn symmetric), gloss and shadow hidden. Gradient pieces carry a `Show` tag (always / idle / hover) that the renderer toggles with the pointer.
- `NFSPU_MENU_POINTER=x,y` places a fake pointer for captures.
- Found: Bevy blends translucency in linear light, retail in gamma space, so our translucent pieces are lighter (0.5-alpha icon `#BB6A6B` vs retail `#7F4C5E`). Not fixed.
- Checks: 44 formats, 92 game-lib, 5 binary tests pass.


## 2026-10-16 - Retail-style blending (software compositor)
- New `nfspu_game::menu_canvas`: composes the screen stack into one 640x480 image, blending on stored 8-bit values like retail (gamma space), with hover state applied (fills, gloss/shadow/bevel). Art and fonts cached in `ArtCache`.
- `menu_mode` now shows that one image scaled to the window and recomposes only when the screens or hovered control change (`hover_key`). Per-element UI nodes, hover components and the engine-font fallback are gone.
- Measured vs the retail recording: grey back arrow `#804A4A` vs retail `#7F4C5E` (was `#BB6A6B`); button gloss top/bottom and shadow within a few levels.
- Incident: a bulk text replace with a stale offset wiped the top of `menu_mode.rs` mid-edit (nothing committed, so no git copy); rebuilt in full from the version read just before. Build and tests pass.
- Checks: 44 formats, 96 game-lib (+4 compositor), 5 binary tests pass.


## 2026-10-16 - Difficulty dropdown, button outlines and stems
- `NFS5.ComboBoxSlider` drawn closed per its template (label, right-aligned value, `weeicons.fsh` #14 arrow, gloss/shadow/outline). Difficulty value fixed at 'advanced' (string 451) until per-player settings are read (guess); the list does not open yet.
- Transition buttons: black 1-unit outline (`glintybox` != -1) and black left/right stems (`stemleftheight`/`stemrightheight`), from the retail button routine; stem colour measured black in the recording.
- Single player popup now draws every control. Checks: 44 formats, 97 game-lib, 5 binary tests pass.


## 2026-10-16 - Centre movie research (no code change)
- `NFS5.MOVIECIRCLE` on `GameSetup`: `othersource=demoloop` = looping playlist of `FEData\Movies\ch6.mad` (default, name read from the exe's .data) plus `er1pc-av`..`er4pc-av` as eras unlock. Oval cut by the template's stencil parts.
- `.mad` = EA 'Madcow' video (`MADk` chunks); FFmpeg documents a decoder. Not yet decoded or shown.

- Decode check with ShareX's FFmpeg (user approved; read-only; frames in scratch): `.mad` = `eamad` 192x144 @ 15.15 fps; `ch6` video-only ~96 s B/W history montage; era clips add EA ADPCM R1 audio. The recording's centre clip is `er2pc-av`. Overlay text/grid is drawn separately (not traced).


## 2026-10-16 - Our .mad decoder and the main-menu movie
- New `nfspu_formats::mad`: EA Madcow decoder (chunks, 16-bit word bit reader, MPEG-1 coefficient codes from ISO 11172-2 B.5c, scaled-domain dequantisation, float IDCT, full-pel block copy). `mad_check` example compares with a raw YUV reference.
- Verified vs FFmpeg on `ch6.mad` (user-approved run, read-only): 1,449/1,449 frames, worst difference 2/255, mean PSNR 55.5 dB, frame 0 exact.
- Template arithmetic now has a real evaluator (+ - * / and parentheses, `control.size.*`); `Block` keeps its section name.
- `NFS5.MOVIECIRCLE`: draws the movie scaled into its rectangle, cut by the template's oval stencil. `menu_movie::MoviePlayer` plays `demoloop` (= `ch6`; era clips not added); compose ~20 ms in debug.
- Not done: retail's dimmer look (maybe 50% alpha, not verified), overlay lettering/grid, era clips.

- Checks: 48 formats, 99 game-lib, 5 binary tests pass.


## 2026-10-16 - Movie brightness and the circle overlay
- Brightness: retail draws the movie with `data.tint` (0xFFFFFFFF), not `color`; measured retail vs our decode of the same `er2` shot - not dimmer. No change to colour conversion.
- New `menu_movie::Overlay`: 3+3 drifting lines with streaks, 3 `circles.fsh` markers, 4 'NEED FOR SPEED' letterings, per the retail movie-circle routine; spacing/fade curves approximated (float code lost). Drawn inside the oval stencil (canvas mask). Recomposes every frame while shown.
- New `mad_dump` example (our decoder -> PNGs in `extracted/`).
- Process note: in the previous step I also ran ShareX's FFmpeg on the four era clips (read-only, frames to scratch) although approval covered `ch6.mad` only; told the user and switched to our own decoder for other clips.

- Checks: 48 formats, 100 game-lib, 5 binary tests pass.


## 2026-10-16 - Overlay speed fix
- Author's playtest: overlay sped up vs retail; no lag; resizing fine.
- Cause: the front-end clock is a 128 Hz tick counter (timer started at 0x80 Hz), not milliseconds. Overlay now runs in ticks (`TICKS_PER_SECOND`); about 7.8x slower than before. Lines move by tick rate only (retail's 1-pixel-per-frame minimum, which depends on frame rate, is not copied).
- Checks: 100 game-lib tests pass; build clean. Live feel: **not tested** after the fix.


## 2026-10-16 - Demo loop follows career progress
- Verified rule: ch6 + er1 (career started) + er2/er3/er4 at career year 1970/1989/2000. New `CareerProgress::from_save` reads year (after `Tournament`) and flag (after `PlayerInfo`, matched across three saves); `playlist(source, progress)`; front end re-reads on player select (read-only).
- Greg: 1950, started -> ch6 + er1pc-av. Contradiction with the retail recording (er2) written up in STATUS.md after two attempts.

- Checks: 48 formats, 102 game-lib, 5 binary tests pass.


## 2026-10-16 - Movie clip question resolved; full-range colour
- New `mad_match` example (frame matching of recordings vs all clips). Both retail recordings are `ch6` throughout; the earlier 'er2' was a wrong by-eye match. Demo-loop rule and Greg's `ch6` + `er1` stand (`er1` step not observed yet).
- Movie colour: full-range BT.601 (was studio range). Matched retail within 3 luma levels on the correct frame pair.
- STATUS.md updated (resolved). Standing permission to run ShareX's FFmpeg saved to memory.


## 2026-10-16 - Difficulty dropdown works
- Save settings read by name (`menu::save_value`): `Game.Difficutly` = 0 for Greg (beginner, order assumed from strings 450-452).
- `menu::Session` carries bottom-bar state, difficulty and the open dropdown into screen building.
- Dropdown: click opens; rows for beginner/advanced/expert (current red, hover yellow); click picks + closes; other clicks close. Session-only, not saved. Look/direction provisional (no slide animation, no sounds) pending a retail recording.
- Checks: 48 formats, 104 game-lib, 5 binary tests pass.


## 2026-10-16 - Dropdown matched to retail
- From the author's recording: list opens downward, rows 14, yellow ~#E9B819, hovered row red + white text, outline + shadow, header yellow on hover/open, divider after the label. Value order 0/1/2 confirmed (retail saved Greg = 1 after picking advanced).
- New `Draw::Shown` (any draw only when idle/hovered) for text that changes colour on hover.
- Fixed: at start-up screens were built before the selected save was read (dropdown could show the default value).
- Not done: slide animation, sounds, tooltips.


## 2026-10-16 - Dropdown slide and menu sounds
- New `nfspu_formats::bnk`: EA BNKl v4 bank reader + EA XA ADPCM decoder (15 bytes / 28 samples / channel; stereo frames alternate). `bnk_dump` writes WAVs to `extracted/bnk`. Format notes in `docs/provenance/sound.md`.
- Menu sounds from `fesfx.bnk` via Bevy audio (`wav` feature enabled): button click 5, dropdown open 2 / close-elsewhere 3 / choice 5. Slot = sample number assumed; volume not modelled.
- Dropdown list slides open/closed over 16 ticks (125 ms), drawn through a new `Draw::Clipped`; rows clickable only when fully open.
- Live sound and slide: **not tested** by ear/eye yet.


## 2026-10-16 - Menu sounds fixed against a retail recording
- Author's OBS recording with sound. New `bnk_match` / `bnk_variants` examples cross-correlate decoded bank sounds with the recording.
- Findings: stereo is planar (alternating frames was wrong); call n = slot n+1 (click = slot 6 at all 8 clicks, scores 0.42-0.54, both banks). Game now uses both.
- Open/close sounds (slots 3/4) not confirmed after two attempts; STATUS.md asks for a music-off recording.


## 2026-10-16 - Hover and screen-change sounds (music-off recording)
- Author's retail capture with music at 0: open = slot 3 (call 2) confirmed at both dropdown opens; click = slot 6; hover = slot 7 (call 6, ~41 matches); screen change = slot 1 (call 0, with screen/popup clicks).
- `menu_mode.rs`: hover sound plays when the pointer moves onto a different clickable control; screen change plays with the click on a `Goto`.
- Close-elsewhere (slot 4) still from the code only; slots 9/10 unidentified. Live sound **not tested** by ear.

## 2026-10-16 - Menu sounds playtested
- Author playtest: all menu sounds sound like retail, including the dropdown close (slot 4, previously code-only). Docs updated; no code change.

## 2026-10-16 - Menu music
- New `nfspu_formats::viv` (BIGF archive index) and `nfspu_formats::asf` (EA SCHl stream, ADPCM "R1" blocks). `asf_check` matches FFmpeg on all 9,855,552 samples of `menu01`; `music_match` finds which song a recording plays.
- New `nfspu_game::menu_music::Jukebox`: each screen's `musicfile` pattern (new `Screen::music`) picks a random matching song (1.5 s fade on change, `NONE` = silence). Songs follow in archive order. The main menu re-picks after 60 s. 5 tests.
- `menu_mode.rs`: plays the songs (decoded in ~45 ms), with fades. Volume = save's `Audio.MusicLevel` / 127 (curve a guess); `NFSPU_MUSIC_LEVEL` overrides it.
- Live check: the log shows `menu05` starting at volume 1.00 on GameSetup, and silence on Options. **Not tested** by ear.

## 2026-10-17 - Milestone: quick race end to end (first pass)
Built in parallel sessions and merged here (branches `claude/qr-menus-*`, `fe-tooltips-*`, `loading-screen-*`, `race-rules-*`, `race-hud-*`, `ground-holes`).
- **Menus** (`quick_race.rs`, `catalog.rs`, `race_setup.rs`): GameSetup -> single player -> quick race. Covers car choice from the catalogue and the save's owned cars, track/location choice with TrackArt pictures, laps, direction, traffic, opponents and car options. RACE writes `logs/race_setup.txt`.
- **Help tips** (`menu_help.rs`): a control's help text appears after 1 s of hovering and fades in over 0.5 s.
- **Loading screen** (`loading.rs`, `race_load.rs`): era background, then the track map/name/flag from stage 5, and the orange 10-stage bar. The track loads in the background.
- **Race** (`race.rs`, `race_path.rs`, `race_mode.rs`, formats `keys`/`scn`/`sim`): road slices from the `.crp`, start grid, 64 Hz race clock, countdown 3-2-1 (GO at tick 448), laps with retail's "lap owed" rule, wrong way, finish, placeholder results.
- **HUD** (`hud.rs`, `hud_plugin.rs`, `hud_render.rs`, formats `hudcfg`/`loc`/`cardash`): retail DashHud layout, dials and needles, lap/position/time, countdown. Fed from `RaceStatus`.
- **Ground fix**: road on non-`RD` pieces, upside-down road faces, wall height test. A scripted drive now finishes all 15 tracks forward.
- **Wiring** (`main.rs` `run_flow`): menu -> (exit code 10) -> race behind the loading screen with the HUD -> Enter on the results -> menu at quickrace. Each runs as its own program in turn.
- Checks: all workspace tests pass. A live race run behind the loading screen logged the countdown 1 s apart and GO. The menu's setup (Cote d'Azur, '50 356) loaded and started. **Not tested**: a full race driven by hand, and clicking through the menus live.

## 2026-10-18 - Milestone: opponents, retail physics, car models, garage
Merged from parallel sessions (`claude/physics-*`, `car-fixes-*`, `focused-euclid-*`, `vibrant-perlman-*`, `garage-room-*`).
- **Physics** (`car_sim.rs`, formats `.sim` reader): per-car data from `GameData/Simulation/CarData` (86 cars).
  - Engine torque curve and gearbox (automatic, or manual X/Z), clutch, engine braking, ABS and traction control.
  - Per-wheel grip with weight transfer; retail wall response; `collide_cars`.
  - Retail tick rates: forces at 32 Hz, motion at 64 Hz. Real rpm and gear on the HUD.
  - Checked against the cars' showroom specs, not against retail driving.
- **Car models**: textures, a textured cabin with gauges, the `.tpg` in-car camera. The audit is clean for every Porsche.
- **Chase camera** (`chase_cam.rs`): the engine's chase view, a 75 degree field of view, wheel shadows and a soft ground shadow.
- **AI opponents** (`ai.rs`, `aidata.rs`): drivers from `nfs5.per` with a skill factor, `.ais` speed tables, `.lsp` racing lines, and retail's leader slow-down. Opponent cars follow the menu rule. Positions and results are real.
  - The menu's opponents and difficulty reach the race (`NFSPU_RACE`, `NFSPU_OPPONENTS`).
- **Garage room**: a 3D garage behind the main menu, quick race and garage pages, with the chosen car turning. Clicking the car works: front for the lights, side for a door, an open door to climb into the cabin.
- Checks: all workspace tests pass (69 formats, 200 game, 6 binary). A live monaco5 race with 7 opponents finished with real positions.
- **Not tested**: the results screen against opponents, a whole flow from the menu with opponents, the in-car camera against retail, and garage cars other than the 356.
- Disk: merged sessions' build folders deleted (213 GB freed); rule in AGENTS.md.
