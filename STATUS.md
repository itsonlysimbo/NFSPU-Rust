# Latest (2026-10-16): menu music

- Menu music plays: our own decoder for the music archive (`BIGF`) and its EA ADPCM streams. It matches FFmpeg in every sample of `menu01`. Each screen's `musicfile=` picks the songs, by the rules in `docs/provenance/music.md`. The recording check agrees: `menu05` on the main menu, a change on Options, then a different menu song.
- Greg's save has `Audio.MusicLevel` 0 (music turned off in retail), so by default our menu is silent, like retail. `NFSPU_MUSIC_LEVEL=127` overrides it for testing.
- **Not tested** by ear. Unknown: the volume curve, the fade-in at song start, why retail's `menu02` was 94 s in after the switch.

---
# Latest (2026-10-16): menu sounds - resolved from a music-off recording

- The open question below is **resolved** by the author's retail recording with the music at 0. Dropdown open = slot 3 (call 2, matched at both opens), click = slot 6, and two sounds the code reading had missed: **hover** (slot 7, 41 matches as the pointer reaches each button) and **screen change** (slot 1, with clicks that change screen or open a popup). All four now play in our menu.
- Author playtest (2026-10-16): everything sounds right, including the dropdown close (slot 4), which the capture could not show. Still unidentified: slots 9/10, which play 0.3 s after each open but are not identified. Volume not modelled.

---
# Latest (2026-10-16): main-menu movie question resolved

- The earlier open question (recording showed `er2pc-av` though Greg's save says 1950) was **my mistake**. `mad_match` (new example) compares each recording frame with every frame of every clip, decoded by our decoder. Both of the author's retail recordings (2026-10-15 and 2026-10-16, Greg selected) are `ch6` throughout, at 15 frames per second, with scores of 0.95-1.00. `ch6` has a colour section (frames ~800-1200) that includes a red 914 head-on shot, like the one in `er2`, so my match by eye picked the wrong clip.
- The verified demo-loop rule stands: Greg (career started, year 1950) gives `ch6` then `er1pc-av`. Neither recording reached the end of `ch6`, so the `er1` step is **not observed** yet.
- The movie keeps playing from when the main menu is first built, including under the select-user popup. In the 2026-10-16 recording `ch6` was already at frame 472 when the popup closed.
- **Colour fix** from the correct pairing (retail frame vs `ch6` frame 848): retail shows the movie's luma as stored (full-range BT.601). Our studio-range stretch had too much contrast. Now: luma average 87.8 vs retail 88.7, percentiles within 3 levels, average RGB 104/82/76 vs 106/82/77.

---
# Latest (2026-10-15): M6 - a real front-end screen, drawn and navigable

- New `nfspu-game::menu` + `menu_mode.rs`: a `.lay` screen is drawn in its own 640x480 space (scaled to the window) from the shipped data alone - layout geometry, template colours and label placement, locale text at the chosen language. Hover uses `hovercolor`; clicking follows `destination=` to `<destination>.lay`, `callback=GOBACK` (or Backspace) goes back, Escape quits.
- Entry path verified statically: `GameSetup -> singleplayer -> quickrace`. Turn it on with `NFSPU_MENU=<screen>` (default `singleplayer`).
- Coverage across all 98 screens (`examples/menu_audit.rs`, `logs/menu_audit.txt`): **1,500 draw commands; 1,936 controls not drawn yet**, led by `NFS5.GOURAUDBOX` (576, four-corner gradient) and `NFS5.PICTURE` (484, `Art/<name>.fsh` + entry index). Those two are the next step. Pass a screen name to dump one screen's draw list (`cargo run -p nfspu-game --example menu_audit -- singleplayer`).
- Two bugs found while checking real screens, both fixed with tests: `REAL.`-prefixed controls after the header were dropped as if they were the screen (`REAL.LABEL` is an engine-built-in control), and text-only controls were skipped for having no size. Text size now comes from the control's font name (`Univers17b.ffn` -> 17).
- Verified: `cargo check --all-targets` clean; 77 game-lib + 40 formats tests pass; the front end was launched once for ~25 s and loaded `singleplayer` without a panic (`logs/m0.log`). **Not tested**: the on-screen result by eye.
- Playtest: `.\run_menu.bat singleplayer` (build the bin first, and close any previous window - a running `nfspu-game.exe` blocks the build). Expect the retail screen: black 640x480 area, two thin divider lines, the letter-spaced title "s i n g l e   p l a y e r", and four dark-red buttons - "cancel" (goes back), "knockout...", "quick knockout...", "quick race..." - that turn orange under the pointer. Clicking opens that screen; Backspace goes back; Escape quits.
- Drawn with the **UI layer** (`Node` + `BackgroundColor` + `Text`), the same path the FPS readout and console use. (The first attempt used `Sprite`/`Text2d` and rendered nothing; `NFSPU_MENU_CAPTURE=1` caught it - the capture was a single colour.)
- Verified by capture: `logs/menu_singleplayer.png` contains 27,267 pixels of `rgb(188,0,0)` (= the retail button `maincolor` `0xBC0000`) and text pixels on black. Live navigation was observed too (`quickrace -> location -> trackrecords`).
- Playtest: `.\run_menu.bat singleplayer`. Hover a button to see it turn orange, click to open its screen, Backspace goes back, Escape quits. **Not tested**: how close the typography is to retail (the shipped `.ffn` fonts are not used yet - labels are drawn with the engine's default font).

---

# Front-end data model and the retail parity plan

- New `nfspu-formats::fe` reads the front end's own data: `.lay` screens, `.ctr`/`.scr` control templates and the `festrings.csv` locale table (ISO-8859-1, with the shipped column quirk). 24 unit tests.
- `examples/fe_audit.rs` audits the whole front end offline: **98 layouts / 3,380 blocks / 2,976 control placements; 73 control ids across 79 files (6 deliberately empty, 0 unparseable); no undefined control ids; 653/653 layout `stringID`s resolve**. Output `logs/fe_audit.txt`.
- Retail parity plan in `docs/DESIGN.md`: **M6 menus -> M7 HUD -> M8 lighting -> M9 AI**, ordered that way because menus are what turn the window into the game and M9 needs a race-setup flow. Front-end format notes in `docs/provenance/fe.md`; data survey in `logs/retail_survey.txt`.
- **Not tested**: nothing is drawn yet, so the viewer is unchanged. Next step is M6's visual half: draw a screen from `Art/*.fsh` and navigate it.

---

# Reference-build model, opt-in retail field of view, stale probes repaired

- New `crates/nfspu-game/src/refbuild.rs` models the patched reference build (Enhanced 3.5 + Combined Patch v1.0.6): the settings block and defaults, renderer selection and pass-through options, the retail virtual framebuffer and field of view, the fog/filtering renderer gate, the CPU-detail clamp, language ids and the portable data folders. Numbers, tables and evidence are in `docs/provenance/refbuild.md`.
- `NFSPU_RETAIL_FOV` (off by default) now hands the chase and cockpit cameras the reference build's field of view; the orbit and free-fly views keep the engine default. `logs/m0.log` records the client size, virtual size and degrees whenever they change.
- Repaired `examples/sky_info.rs` and `examples/sky_lap.rs`, which no longer compiled against the current sky API (`HorizonColour::u`/`distance` are gone; `sky::LapPath` was removed). `sky_lap` now samples the tint by the centreline's heading instead of by lap distance, so it no longer uses the disproven distance reading.
- Verification: `cargo check -p nfspu-game --all-targets` is clean (one pre-existing unused-variable warning in `ground_audit`), and **`cargo test -p nfspu-game` passes end to end** (70 lib + 5 bin tests), where it previously stopped at `sky_lap`. Offline probes run: `sky_lap -- alps` = 225 centreline points, 10,594 m lap, 74 horizon colours, 6 tint changes; `sky_info` reads every track's sky.
- Audit (reads the author's patched copy, no window, no game run): `$env:NFSPU_GAME_DIR="C:\Users\CB\Downloads\NFS5 - Copy"; cargo run -p nfspu-game --example refbuild_audit -j 2` -> `logs/refbuild_audit.txt`. It read the settings (language ENGLISH, driver `opengl3`, windowed, movies off, fog and filtering on), parsed the driver profile (`opengl3z.dll`, OpenGL, 18 options) and found `!pal` in **0 of 168** texture archives.
- Playtest (**not tested**): `$env:NFSPU_RETAIL_FOV="1"; .\run_m2.bat alps`, press C for chase/cockpit and compare against a default run. Expect a narrower view in a 4:3 window and a wider one when the window is widescreen.

---

# Latest fidelity work (2026-10-15): verified triangle strips, validation complete
- Indexed mode 1 = triangle strip (alternating winding), mode 3 = triangle list. Reader expands both into independent triangle lists with aligned vertex/UV/colour corners; unknown indexed modes count as skipped. Earlier tail-trimming remains a boundary check only.
- Offline: 28 formats tests, 51 game-lib tests, 5 binary tests pass; install-backed all-15-track audit passes (e.g. monaco1: 242 groups / 28,911 complete triangles). Full 59-container normalization audit: 34,171 meshes / 1,417,518 triangles, 0 skipped batches; only modes 1 (497 batches) and 3 (89,665) observed.
- Live: Monaco1 (strip surfaces: tunnel, walls, overpass, intersection) and Alps four-heading captures exit Success on RTX 4050/Vulkan with no WARN/ERROR/PANIC. Matched retail visual parity remains **not tested**.
- Run: `$env:NFSPU_GAME_DIR="C:\Users\CB\Downloads\NFS5"; cargo run -p nfspu-formats --example primitive_audit -j 2`. Playtest: `$env:NFSPU_SKY_CAPTURE="0"; .\run_m2.bat monaco1`.
- User authorizes opening/closing test games as needed; target is close retail recreation. No commit, no install/decomp writes, no graphics-setting changes.
- Wall collision v1 (same day): steep RD/CNK faces are kept as `walls`; axle-circle push-out stops the car at embankments (55 lib + 5 binary tests pass; alps live capture Success, 20,394 wall triangles; drive_check unchanged). Live wall behaviour **not tested**. Next: crest stickiness and handling fidelity.
- Collision v2 (same day): drivable surface limited to 40 deg slopes (`classify`), so cliffsides are walls now (road triangles unchanged on all 15 tracks); `Ground::surface_near` plus tiered probes (1.5/3/6 m) bridge the 1-4 m pockets at wall bases that let the car fall through on monaco. 57 lib tests and the new all-15-track fall-through test pass; lap_check now completes alps 225/225. Playtest `.\run_m2.bat alps` / `monaco1`; live feel **not tested**.

---
Earlier handoffs follow. The latest correction above supersedes their horizon-colour statements.

# Current handoff (2026-10-15): retail horizon atlas

- Implemented the statically verified 16-segment atlas, replacing the guessed repeating whole picture. Reads row-major cells and flips, exact vertical UV insets, start angle and height/radius ratios. Separate quads preserve UV discontinuities. X is mirrored like the track; the ring is scaled uniformly to fit inside the dome.
- Validated raw headers against read-only SkyHorizon reference before rendering edits. Offline audit passes 17 atlas headers (15 track sets, Monaco, default); two lightning files have another format. Monaco has **three**, not five, flipped segments. Start segment indexes colour resources, not texture order or geometry.
- Sky colour remains **unconfirmed**. Existing direction tint is still a guess; no colour mapping change. Historical claims below about lap distances, directions and a global colour template are superseded hypotheses.
- Verification: 50 library tests, 3 console tests plus 1 horizon mesh test, and 26 formats tests pass. Final dev viewer build passed with `-j 2`. Live appearance **not tested**. No game window launched or running game stopped. No commit.
- Logs: `logs/horizon_reference.txt`, `logs/horizon_audit.txt`, `logs/horizon_lib_tests.txt`, `logs/horizon_console_tests.txt`, `logs/horizon_formats_tests.txt`, `logs/horizon_build.txt`.
- Offline audit: `$env:NFSPU_GAME_DIR="C:\Users\CB\Downloads\NFS5"; cargo run -p nfspu-game --example horizon_audit -j 2` (17 PASS, two unsupported lightning headers).
- Playtest after build: `cargo build -p nfspu-game --bin nfspu-game -j 2`, then `.\run_m2.bat alps` or `.\run_m2.bat monaco1`. Expect mountains composed from configured atlas cells at track-specific heights. Colour correctness and any remaining sliver are **not tested**.

---
Historical status follows; use the current handoff above for sky conclusions.

# STATUS (2026-10-09, all cars pass)

## Works (confirmed by the author on alps, Intel UHD graphics)
- Textured track viewer, mouse-look fly camera (F), and a driveable car: `run_m2.bat [track] [car] [style] [driver] [paint]` (default alps 993 1 1 4; H headlights, C camera; 993 styles 1 coupe, 2 targa, 3 Carrera4 cab, 4 Carrera4 coupe, 5 Carrera4S, 6 cabrio, 7 S3.6, 8 GT2, 9 turbo). W/S/A/D or arrows, Space, R reset, F fly camera, F10 quit.
- The real 993 model: textured body per style, spinning/steering wheels, underside, translucent glass and soft-top window, the driver (racing suit, helmet), a flat-grey cabin; cameras chase / cockpit / orbit (C). The world is mirrored on import (`MIRROR_X`) so the steering wheel is on the left.
- Formats crate (23 tests): RefPack, SHPI (0x7D, 0x7E; tiles, pages, filtering), CRP (geometry, materials, names, base, transforms, embedded archives). Game crate (22 tests): ground, car physics, track and car loading, `.tpg` styles.

## Not done / known issues
- Cabin textures (flat grey now), front skirt (style wants a piece type the file lacks), decals/badges, damage overlays, stripe colours (`color2..4` in the `.clr` are unused).
- Cars confirmed live: 993, 928, Boxster, 959, GT3, 964. All 43 cars (27 Porsches incl. police and factory versions, 16 traffic cars) pass the offline audit (`car_audit`); the others still need a look in the game (N / B in the viewer). `postship` is not a car.
- Sky: each track's `.env` pictures are drawn on a dome around the camera, but without the retail game's time-of-day tint they look dark and blurry; the sun glare sprites and the night star field (`Render/night.psh`) are not drawn yet. Also not done: falling snow and other weather particles, traffic cars on the road (templates at the origin are still skipped).
- No walls or guard rails; car launches off crests at high speed; only alps seen live; NVIDIA GPU untested (switched off in the laptop).
- `target\release` holds about 1.6 GB of leftover build cache (safe to delete, say the word).
- The `.edg` side file is unused.







## Sky, after the first on-screen test (2026-10-09)
- Working: per-track sky loading from `GameData/Track/Sky/` for all 15 sets; dome tinted with the weather region's sky colour; `horz` on the horizon; offline tools `sky_info`, `sky_lap`, `lap_order`.
- Broken / unknown:
  - `horz` aspect: the band geometry is wrong by about 60:1, so the mountains are thin streaks. Needs the band's real height and the picture's real coverage.
  - Warm/cold change: the `.bin` table only spans 0..2104 m of alps' 10594 m lap (and 0..2790 m of castle's 6172 m lap), so it cannot explain the change over a whole lap. Verified offline: 6 changes in the first 20% of alps, then flat. The retail frames show the sky changing all the way round, so there is a second driver we have not found - most likely the engine's time-of-day advancing during the race (`WEATHER` is a race setting, and `Render/night.psh` holds a star field for night). **Unresolved.**
  - Not drawn at all: cloud layers (`cc1a`/`cm1a`), `hzcl` horizon haze, sun glare sprites, night stars, weather particles (snow).
  - Floating lamp in the alps village (track bug, not sky).

## Horizon mountains: three attempts, stopped (2026-10-09)
Attempts on the thin sliver above the mountains, and what each one taught us:
1. **One band for the whole picture.** Fixed the 60:1 stretch by repeating the picture so texels stay square. Result: mountains look like real mountains, but a thin stretched sliver sits above them. Kept.
2. **One ring per content run** (`sky::Pictures::content_runs`). The picture holds two ranges, rows 1..126 and 139..254 on alps, each getting its own ring. Result: sliver still there. Kept, because the two ranges are real and 10 tracks have them.
3. **Trim the near-transparent top rows** (only count a row when 15% of it is opaque). Result: **worse** - it cut the mountain peaks off and left flat-topped ridges, because the peaks live in exactly those sparse top rows. Reverted.

So the sliver is still unexplained. What we know: it is not the transparent top margin (attempt 3 disproved that), and an offline render of the same geometry looks clean, so it is something about how the ring is drawn on screen rather than the numbers we put in. **Unresolved - do not keep guessing at it without new information.**
Next thing worth trying, if wanted: draw the ring with a vertical colour gradient that fades to the sky colour at its top edge, so any hard edge is invisible regardless of cause.
- Sky colour: **unconfirmed.** Direction-based tint is implemented and does change with heading, but the file's values only cover about 73 degrees of the horizon and the field we read as a direction is probably a ring position rather than a direction. Reading `SkyHorizon::vfn03_00459d30` properly is the next step; it derives the ring from two segment counts in the config rather than from stored directions.
- Sky colour: after reading `SkyHorizon::vfn03_00459d30`, `vfn01_0045a1a0` and chasing `FUN_00460c60`, the ring is understood (a grid of quads over the `horz` texture, 16 segments, column order table at `cfg+0x1c`, flip flags at `cfg+0x1d`, cols/rows at `cfg+0xc` and `cfg+0x10`) but **the mapping from `<track>.bin` records to the 16 segments is still unknown**. The last idea - a global 16-colour template - was wrong; that accessor is a resource cache. Further progress needs either running the retail game to observe the change directly, or a much deeper read of the loader. **Recommendation: park this rather than keep guessing.**
- Sky colour: search closed. Everything about the ring is now understood and verified against the raw files (version, start angle, start segment, columns, rows, the 16-byte column order and the 16 flip flags - Monaco proves the table really reorders and mirrors the picture). The one thing left is that the 16 per-segment colours are generated when the `horz` resource is **built**, from the per-track `.bin`, and the builder is in the resource-loading path, not the sky classes. Not located. Our current direction-based colour is a **guess** and must be labelled as one until that builder is found.
