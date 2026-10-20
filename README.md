# NFS Porsche Unleashed: Rust rewrite (experimental)

A from-scratch Rust/Bevy/wgpu rewrite of Need for Speed: Porsche Unleashed (2000). It reads data from **your own installed copy** at runtime. This repository contains only original code and notes, no game files.

Unofficial fan project, not affiliated with or endorsed by EA. Single-player and offline only.

## Status at a glance
The goal is to match retail as closely as practical. This is still an experimental recreation, not a complete game.

**Works now**
- Driving: textured tracks, real Porsche and traffic-car models, paint/styles, animated wheels, lights, drivers, and chase/cockpit/orbit/fly cameras.
- Geometry: verified triangle-list and triangle-strip decoding, audited across 15 tracks and 44 car-model containers.
- Collision: terrain walls and cliffs block the car; ground probes reduce fall-through at track joins. Offline checks pass; live collision feel is **not tested**.
- Horizon: verified 16-segment atlas layout, mirroring and `hzcl` vertex colours. Alps and Monaco render successfully.
- Menus: a first-pass front end reads the retail layouts, control templates and translated text. Buttons highlight and navigate between screens. The main menu builds in layers like retail: the oval frame artwork, the page, the "select user" popup on top, and the bottom bar. Gradient boxes and menu pictures (`NFS5.PICTURE`, popup circle) draw, in the retail back-to-front order. Menu text uses the game's own `.ffn` bitmap fonts, with retail label alignment. The main menu's oval loops the retail movies (`ch6`, plus era clips as the selected profile's career unlocks them) through our own `.mad` decoder (checked against FFmpeg: worst difference 2/255), with the retail overlay of drifting lines, circle markers and "NEED FOR SPEED" lettering (some animation curves approximated). The menu is composed at its native 640x480 with retail-style (gamma-space) blending, so shadows and faint buttons match retail's darkness. The select-user popup lists saved players from `SaveData\*.sav` (click a row to select it) and shows each player's chosen face (the face number is the first character of the save's file name, e.g. `0Greg.sav`). The difficulty dropdown slides open like retail. Menu sound effects come from the game's `fesfx.bnk` (our own EA ADPCM decoder): hover, click, screen change and dropdown open/close, matched against a retail recording and confirmed by ear in a playtest; the volume level is not modelled yet. Menu music plays from `GameData/Music/zzzymus.viv` through our own EA stream decoder (identical to FFmpeg's output): each screen's `musicfile` picks the songs (a random `menu*` song, silence on Options), songs follow in archive order, and the level comes from the save's `Audio.MusicLevel` (`NFSPU_MUSIC_LEVEL` overrides it). Compared by eye with one retail screenshot; exact parity is **not tested**.

**Quick race (first pass, 2026-10-17):** `.\run_menu.bat GameSetup`. Choose single player -> quick race, pick a car and a track, press RACE. The loading screen shows, then the race: countdown, laps, finish and results, with the retail HUD. Enter on the results returns to the quick-race screen; menus and race share one window (2026-10-20). Up to 7 AI opponents race you (2026-10-18) on retail car physics, and the 3D garage with your car turns behind the menus. Traffic is not simulated. Mirrored directions flip the whole layout (track, sky, start grid and opponents), but whether retail mirrors the 3D image or the geometry is **not tested**. Driven end to end by hand: **not tested**.

**Missing or incomplete:** traffic, suspension and damage, in-race sound and music, opponents and traffic simulation, race progression, HUD, cabin textures, the 3D garage room behind the menus, gradient indicator bars. Overall sky/weather colour behaviour is unconfirmed.

**Checks** (`cargo test --workspace -j 2`, re-run 2026-10-16): 107 game-library tests, 53 formats tests and 5 binary tests pass (2 binary tests are ignored by design). The menu data audit covers 98 layouts, and all 653 referenced layout strings resolve. Exact retail visual parity is **not tested**.

**Next:** finish menu rendering and race setup, then the HUD, verified lighting/weather, and AI.

**Build and try it** (PowerShell on Windows):
```powershell
cargo build -p nfspu-game --bin nfspu-game -j 2
.\run_m2.bat alps
```
Front end in development: `.\run_menu.bat GameSetup` (the retail main menu with the player popup) or `.\run_menu.bat singleplayer`.

## What works
- Indexed triangle lists and triangle strips decoded with verified winding and aligned corner colours/UVs. Full indexed-geometry audit passes on 15 tracks and 44 CarModel containers. Nonindexed geometry and matched retail parity remain **not tested**.
- Retail horizon vertex colours from the verified 16x2 `hzcl` image; row/start/wrap assignment audited on all 17 sky sets. Four-heading live captures tested on Alps and Monaco1 (RTX 4050/Vulkan).
- Retail horizon atlas: 16 configured segments, cell order/mirroring, UV row insets, start angle and height ratios; offline checked on all 17 sky headers. Live rendering checked on Alps and Monaco1; matched retail parity **not tested**.
- M0 window opens and finds the game folder (tested, Intel UHD).
- SHPI texture archive directory parsing (tested on all 15 tracks).
- CRP track geometry reader (vertices, UVs, triangles, per-corner colours) tested on all 15 tracks.
- Viewer with a placeholder car (`run_m2.bat [track]`; drive with W/A/S/D or arrows, Space handbrake, R reset, F fly camera, C camera (chase/cockpit/orbit), H headlights, N/B next/previous car, V body style; tested by the author on alps). Fly-camera viewer: shows a real track with its textures (textures confirmed by the author on alps). Tested by the author on alps (Intel UHD graphics). Click in the window to capture the mouse and look around (Esc/Tab releases it; right-drag also looks), WASD/QE = move, Shift = fast, wheel = speed, F10 = quit.
- RefPack decompression of the track `.crp` and `.fsh` files (tested on all 15 tracks).
- Reference-build model (`nfspu-game::refbuild`): the patched build's settings and defaults, renderer selection (`drivers/<name>/thrash.ini`), the retail virtual framebuffer and field of view (4:3 threshold, 37.5 degrees narrow, `atan(360*aspect)` widescreen), the fog/filtering renderer gate, the CPU-detail clamp, language ids and the portable data folders. Audit: `$env:NFSPU_GAME_DIR="C:\path\to\patched copy"; cargo run -p nfspu-game --example refbuild_audit -j 2` (tested; writes `logs/refbuild_audit.txt`).
- Front end, first pass (`NFSPU_MENU=<screen>`, default `singleplayer`; or `.\run_menu.bat`): a retail `.lay` screen drawn in its 640x480 space from the shipped layout, control templates and locale text, with hover and click navigation (`destination=` -> `<destination>.lay`, `GOBACK`/Backspace back, Escape quit). `singleplayer` reproduces the retail screen (title "s i n g l e   p l a y e r", four dark-red buttons, orange hover). Drawn with the UI layer (`Node`/`BackgroundColor`/`Text`), verified by capture (`logs/menu_singleplayer.png` holds the retail `maincolor` `0xBC0000`). `NFSPU_MENU_CAPTURE=1` saves `logs/menu_<screen>.png` and exits for a windowless check. Coverage across all 98 screens is in `logs/menu_audit.txt` (1,500 draw commands; 1,936 controls not drawn yet - `NFS5.GOURAUDBOX` and `NFS5.PICTURE` lead). Labels currently use the engine's default font rather than the shipped `.ffn` fonts.
- Front-end data model (`nfspu-formats::fe`): `.lay` screens, `.ctr`/`.scr` control templates and the `festrings.csv` locale table, with the shipped Latin-1 encoding and column quirk handled. Audit: `$env:NFSPU_GAME_DIR="C:\path\to\game"; cargo run -p nfspu-formats --example fe_audit -j 2` (tested; 98 layouts, 0 undefined control ids, 653/653 layout strings resolve; writes `logs/fe_audit.txt`).
- Optional retail field of view: set `NFSPU_RETAIL_FOV=1` and the chase and cockpit cameras use that retail field of view (narrower at 4:3, wider when the window is widescreen), recomputed on resize; orbit and free-fly keep the engine default. Off unless the variable is set, so a normal run is unchanged. On-screen look **not tested**.

## What does not work yet
- Sky and weather: the track's `<track>.bin` per-slice weather tables (rain, snow, colours) are decoded and sampled by the player's slice; they tint the sky dome and drive a fog whose amount follows the precipitation. The blend-to-region mapping and the fog scale are **guesses**. Clouds, sun glare, stars and rain/snow particles are not drawn, and the track materials are still unlit (no time-of-day lighting yet). Horizon visual agreement is **not tested**.
- Wall collision v2: embankments, tunnel sides and cliffsides (anything steeper than 40Â°) block the car, and a rigid three-tier ground probe bridges the small gaps at wall bases that used to swallow it. Verified offline on all 15 tracks; live feel **not tested**. Signs, roadside objects and car-vs-car collision are missing. Car still launches off crests at speed; handling is arcade, not retail physics. No sound or menus; cabin textures and some car details remain incomplete.

## Run
```
$env:NFSPU_GAME_DIR="C:\path\to\your\game\folder"
cargo run -p nfspu-game --bin nfspu-game -j 2
```
Format probe: `cargo run -p nfspu-formats --example probe -j 2`

## Known issues
- Ctrl+C in the console previously closed the viewer; it is now ignored and logged.
- Only tested on Intel UHD graphics.
- `examples/ground_audit.rs` carries a pre-existing unused-variable warning.











Offline horizon check (no window):
```powershell
$env:NFSPU_GAME_DIR="C:\Users\CB\Downloads\NFS5"
cargo run -p nfspu-game --example horizon_audit -j 2
```
Expected: 17 atlas headers pass; the two lightning files are reported as unsupported.
To playtest after building: `cargo build -p nfspu-game --bin nfspu-game -j 2`, then
`.\run_m2.bat alps` or `.\run_m2.bat monaco1`. Expect a horizon assembled from atlas
cells with track-specific heights, instead of repeated whole pictures. Sky colours remain a guess.

Rendered horizon captures (opens and intentionally closes a test window):
```powershell
$env:NFSPU_GAME_DIR="C:\Users\CB\Downloads\NFS5"
$env:NFSPU_SKY_CAPTURE="1"
.\target\debug\nfspu-game.exe alps
$env:NFSPU_SKY_CAPTURE="0"
```
Expected: four `logs/horizon_alps_*.png` files from fixed headings. Use `monaco1` for
its reordered/mirrored atlas. Overall sky colours and matched retail parity remain unconfirmed.
