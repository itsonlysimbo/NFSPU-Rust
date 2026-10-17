# Race loading screen

What retail shows between pressing RACE and the race intro, and how the rewrite reproduces it
(`crates/nfspu-game/src/loading.rs`, driven by `crates/nfspu-game/src/race_load.rs`).

Sources: the Ghidra decompile and disassembly of `Porsche.exe` (v3.5), the files of the author's
install (`NFS5 - Copy`), and the let's play `aL_I0E5QIR0` (3:54-4:01, an Evolution race on
Normandie). Addresses are given only so the evidence can be found again.

## Verified

| What | Value | Evidence |
|---|---|---|
| Driver | one routine called with a stage number 0..10 as loading goes on; 0 puts the screen up, 10 takes it down | call sites in the race start-up function, in this order: 0 (when the front end closes), 1, 2, 3 (inside scenery setup), 4, 5, *track load*, 6, 7, 8, 9 (inside car/AI setup), 10 |
| Picture size | a 640x480 surface, shown unscaled in the middle of the screen (`(screen - 640) / 2`, `(screen - 480) / 2`), black around it | disassembly; the let's play (captured at 1280x720) shows the picture covering half the width |
| Background, stage 0 | `FEData/Art/era<N>.fsh`, 45 tiles: 20 of 32x32 at y 0, 10 of 64x64 at y 32, then 5 of 128x128 at y 96, 224 and 352 | tile loops in the routine; the files have exactly these 45 entries with these sizes and page positions |
| Background, stage 5 | `FEData/Art/era<N>tr.fsh`, same tiling: a blurred version of the same picture so the map reads well. Both versions carry the word LOADING and the bar's groove (x 302-426) at the bottom | routine; let's play frame; our composed `era0` / `era0tr` pictures |
| Stages 1-4 and 6-9 | the background is not redrawn (it stays on the surface); only the bar is drawn again | routine |
| Era N | 0, except in one game mode (mode value 4) where N = 1, 2 or 3 from the current era | routine. Quick race therefore uses `era0` |
| Track art | stage 5 also draws `FEData/TrackArt/<track>.fsh` (unless the mode value is 9): entry 0 = map, entry 1 = track name, then entry 2 (forwards) or entry 3 (backwards) = finish flag. Each is drawn at the screen position stored in its header (bytes 8 and 10, absolute values), at its own size | routine; e.g. `alps.fsh`: map 414x132 at (115,177), name 40x14 at (301,306), flags 16x16 at (525,169) and (113,216): the two ends of the map |
| Mirrored track | map and name are drawn flipped left to right at the same place; the flag's x becomes `2 * name.x + name.width - flag.x` (the name is centred on the screen, so this reflects about the centre line; the flag's own width is not accounted for) | disassembly of the x calculation; the mirror flag is the one that also flips the 3D projection's x |
| Bar | filled rectangle at x 302, y 467, height 6, width `stage * 12.4`, colour `0xFFBB00`; full length 124 at stage 10 | disassembly (`12.4` is a float constant in `.rdata`) |
| Translated label | if the language is not English, entry `language - 1` of `FEData/Art/LoadText.fsh` (French, German, Italian, Spanish, Swedish, Portuguese, all at y 456) is drawn over the background's English LOADING | routine; entry names; language order from `docs/provenance/fe.md` |
| End | automatic at stage 10; no key press. The race intro camera follows within a second | routine; let's play 4:00 -> 4:01 |
| Car art | none | routine draws no car picture |
| Multiplayer | a different picture (`MLoading.fsh`) and one bar per player | routine; not needed for single player |

`FEData/Art/Loading.fsh` (45 tiles, compressed entries) and `loadscrn.fsh` are not used by this
routine; `loadscrn.fsh` belongs to the start-up screen (it is loaded next to the EA intro movie).

## In the rewrite

- `loading::plan` turns (setup, stage, header data) into a draw list; `LoadingArt::compose` paints
  it into a 640x480 picture with the front end's compositor; `LoadingScreenPlugin` shows it 1:1
  in physical pixels in the middle of the window over its own camera (order 100, clears to black).
- Stage mapping (`race_load.rs`): 0 screen up and a worker thread starts reading the track;
  1-4 one frame each (retail does audio and front-end work there; there is nothing to do yet);
  5 track art, wait for the worker; 6 track read; 7 the viewer's normal `setup` runs (meshes,
  sky, ground, walls, car); 8-9 one frame each; 10 the screen goes and driving starts. Every
  stage is on screen for at least two frames.
- Entries with format bit 0x80 set (`LoadText.fsh`) are RefPack-packed on their own; they are
  unpacked before decoding. Not checked against retail pixels (no non-English capture).

## Checks done (2026-10-17)

- Unit tests (`cargo test -p nfspu-game --lib loading`): 7 pass (spec parsing, file choice per
  stage, bar length, the 45 tiles cover 640x480 exactly once, track art from stage 5, flag choice
  for direction and mirror, translated label).
- `loading_preview` example composed alps (stages 0, 5, 9) and monaco1 reversed in French into
  `logs/loading_*_stageNN.png`. Compared by eye with the let's play frame at 3:57: same layout
  (map in the middle, track name under it, flag at the finish end, LOADING and the bar at the
  bottom). The let's play uses another era's background, so the pictures themselves differ there.
  The French label covers the English word exactly, so packed entries decode correctly.
- Live run `NFSPU_LOADING=alps,993`: the screen appeared 1:1 in the middle of the window over
  black, then driving started with the car on the start line. Log: track read on the worker in
  0.19 s, viewer `setup` 0.23 s, whole screen 1.55 s (stages
  `0@0.00 1@0.08 2@0.90 3@0.94 4@0.98 5@1.01 6@1.04 7@1.07 8@1.47 9@1.51 10@1.55`; the
  0.8 s before stage 2 is the first frames' GPU start-up).
- Missing track art (unknown track) logs a miss and shows the background and bar only.

## Not known yet / guesses

- Which game mode is value 4 (Evolution is the guess: the let's play is Evolution and shows a
  356 background, not era 0's).
- The meaning of mode value 9 (no track art).
- Whether quick race can be mirrored at all; the mirror drawing is implemented from the code only.
- Retail's pacing between stages (it depends on disk speed); ours depends on how long the
  track takes to read.

## How a race start uses it

- `nfspu_game::loading::RaceSetup { track, car, reversed, mirrored, era, language }` describes the
  choice (`RaceSetup::parse("alps,993,reverse")` reads the `NFSPU_LOADING` form).
- In the viewer binary, `race_load::RaceLoad(Some(setup))` replaces the plain start-up: it puts
  the screen up, reads the track on a worker thread, runs the viewer's own `setup`, and removes
  the screen. `RaceLoad(None)` is the old behaviour.
- Anything that should start when the race is on screen (countdown, HUD) can read the
  `loading::LoadingFinished` message. Other loaders can drive the screen themselves:
  `loading::begin(&mut commands, game_dir, setup)`, then raise `LoadingStage` to 10.
- `reversed` and `mirrored` only change the loading picture so far; the track itself is not yet
  loaded reversed or mirrored.
