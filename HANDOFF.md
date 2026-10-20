# Hand-off for the next agent

Last updated 2026-10-20 by the Claude coordinating session. Read `AGENTS.md` first: its hard rules
(no game data in the repo, whitelist `.gitignore`, read-only installs and decomp, own code only) are
not optional. This file says where things stand and how to carry on.

## What the project is
A from-scratch Rust 1.99 / Bevy 0.19.1 recreation of Need for Speed: Porsche Unleashed (2000) that
reads the player's own install at runtime. The target is retail behaviour, 1:1 where practical,
with evidence (decomp, game files, footage) kept separate from guesses.

The user is a beginner. Keep explanations short and plain, say **not tested** for anything you did
not verify, and after each change give the exact command to run and what they should see.

## State of `master` (commit `038e3de`)
Quick race works from menu to finish. The user completed full races on 2026-10-19.

| Area | State | Notes |
|---|---|---|
| Front end | Retail layouts, fonts, art, movie, sounds, music, help tips, 3D garage | `menu*.rs`, `quick_race.rs`, `menu_garage.rs`; `docs/provenance/fe.md`, `garage.md` |
| Car pick | The car, body style and paint picked are the ones raced | roof up/down passed but not drawn |
| Loading screen | Retail art and bar | `loading.rs`, `race_load.rs` |
| Race | Grid, 64 Hz clock, countdown, laps, finish, placeholder results | `race*.rs`; `race.md` |
| Opponents | Up to 7, retail drivers, racing lines, leader slow-down | `ai.rs`, `aidata.rs`; no route choice at junctions |
| Physics | Per-car `.sim` data, engine/gearbox, tyres, ABS/TC, walls, car-vs-car | `car_sim.rs`; `physics.md`; spin fix not playtested |
| HUD | Retail layout, black boxes, minimap, positions | `hud*.rs`; `hud.md` |
| Cars | Textures, cabin, in-car camera, chase camera, shadows | `car_model.rs`, `chase_cam.rs`; `car.md` |
| Weather | Night tracks, fog, rain/snow, lightning, wet road, headlights | `weather*.rs`, `sky.rs`; `weather.md` |
| Mirrored tracks | The world flips on mirrored races | `set_world_mirrored` |
| One program | Menu, loading and race are states of one app | `flow.rs`; `NFSPU_FLOW_TEST` |
| Sky | Retail sky layers, sun, lens flare, glows, window drops | `sky_fx.rs` |

**Known gaps:**
- traffic;
- suspension, roll-overs and damage;
- the retail results screen;
- the location screen's 3D track room;
- route choice at junctions;
- roof drawing;
- handling and weather feel compared with retail;
- in-race sound.

## Work in progress when Claude stopped
Nothing unmerged as of 2026-10-20: one program (`flow.rs`) and the sky layers (`sky_fx.rs`) are merged
into master. The app is now one Bevy app with states Menu -> Loading -> Race; `NFSPU_FLOW_TEST=3,15`
scripts three menu-race-menu loops (see `logs/m0.log`).

Check for new unmerged work with:
```bash
git for-each-ref --format='%(refname:short)' refs/heads | while read b; do n=$(git rev-list --count master..$b); [ "$n" -gt 0 ] && echo "$b: $n commit(s)"; done
```

## How work is organised
- **Branches:**
  - Big goals are split into parallel Claude sessions, each in its own git worktree under `.claude/worktrees/` on a `claude/*` branch.
  - At most 3-4 at once: the machine has 16 GB of RAM.
  - Each session commits to its own branch and never pushes.
- **Merging:**
  - A coordinator merges each branch into `master` as soon as it lands, then runs the full build and tests.
  - It then deletes that worktree's `target/` (build output, many GB).
- **Agents working on master:** an agent working directly in the main checkout (as you may be) edits `master` itself.
  - Commit your work when a piece is done, with a clear message.
  - If you find uncommitted changes you did not make, they are someone's work. Commit them to an `other-agent/<date>` branch and merge it; never discard them.
- **Docs:**
  - Update `MODLOG.md`, the README works/doesn't-work list and `STATUS.md` only at milestones.
  - Add format findings to `docs/provenance/` as you go.
- **Commits:** end commit messages with a `Co-Authored-By:` line naming your agent.

## Build, run, test
In PowerShell first: `$env:PATH="$env:USERPROFILE\.cargo\bin;C:\Program Files\Git\cmd;$env:PATH"`.

```bash
cargo build --workspace --all-targets -j 2
```
```bash
cargo test --workspace -j 2
```
```bash
cargo run -p nfspu-game --example race_drive --release
```
The last one is the regression check: an autopilot races all 15 tracks and should end `ALL OK`.

Play it: `.\run_menu.bat GameSetup` (the launcher sets `NFSPU_GAME_DIR=C:\Users\CB\Downloads\NFS5 - Copy`).

Useful environment variables:

| Variable | What it does |
|---|---|
| `NFSPU_MENU=<screen>` | start the front end at that screen (`GameSetup`, `quickrace`, `page+popup`) |
| `NFSPU_MENU_CAPTURE=1` | save `logs/menu_<screen>.png` and exit |
| `NFSPU_MENU_PLAY=1` | with a capture, also press RACE |
| `NFSPU_RACE=<track>,<forward\|backward[_mirrored]>,<laps>,<opponents>,<difficulty>` | start a race directly |
| `NFSPU_LOADING=<track>,<car>[,reverse][,mirror]` | show the loading screen first |
| `NFSPU_HUD=1` | draw the HUD |
| `NFSPU_MANUAL=1` | manual gearbox (X up, Z down) |
| `NFSPU_WEATHER`, `NFSPU_TIME` | force the weather or time of day |
| `NFSPU_MUSIC_LEVEL=0..127` | force the music level (Greg's save has music at 0) |

Logs go to `logs/` (`m0.log` main log, `race.log`). The user can't see what you see, so log numbers.

Builds use `rust-lld`, light debug info and sccache. For quick loops, `--features dev` links Bevy as a
DLL (not for the `.bat` launchers). Run long builds in the background; a killed link shows as
`exit code 143`.

## Evidence sources (all read-only)
- **Install:** `C:\Users\CB\Downloads\NFS5 - Copy`. The test profile `3Greg.sav` was made by the user.
- **Decomp:** `C:\Users\CB\Desktop\NFS5 Decomp`.
  - **With Ghidra open:** if the user has `Porsche.exe` open in Ghidra's CodeBrowser with GhidraMCP on, query it read-only, e.g. `curl -s "http://127.0.0.1:8080/decompile_function?address=004ae8c0"`. More paths are in `AGENTS.md`. GET requests only; never POST.
  - **Otherwise:** search `decomp/classes` and `decomp/src`, not the whole folder.
- **Footage:**
  - The let's play: https://www.youtube.com/watch?v=aL_I0E5QIR0. Watch it in a browser; don't download it.
  - The user's recordings are in `C:\Users\CB\Videos`.
  - ShareX's `C:\Program Files\ShareX\ffmpeg.exe` may be run on them (outputs to a scratch folder).

## What the user asked for next (2026-10-19)
1. Done: the picked car is driven, weather/day-night/lighting, tighter control, a black HUD with minimap and positions.
2. Done: menus and racing in one program (2026-10-20).
3. After that, the user will playtest with one recording (game sound on). Fix what they report, then
   pick from the known gaps above, starting with the retail results screen, traffic, damage and in-race sound.
