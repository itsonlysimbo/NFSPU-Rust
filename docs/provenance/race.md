# Race rules and race path

Findings for quick race from the grid to the finish: where the retail game keeps the road path,
how it counts laps, the countdown, the start grid and the finish. Sources are the decomp (static
reading only, function addresses given so the reasoning can be re-checked), the files in the
install, and the reference let's play. Code: `crates/nfspu-formats/src/{sim,scn,keys}.rs`,
`crates/nfspu-game/src/{race_path,race}.rs`, `crates/nfspu-game/src/race_mode.rs`.

Verified = seen in data or footage, or both. Decomp = read from the decompiled code only. Guess = ours.

## Road slices (`SimD` / `SimT` in the track `.crp`)

The road path is not in a separate file. The track loader (`0x480080`) looks up two misc parts of
the `.crp` by tag: `SimD` and `SimT` (stored byte-reversed, `DmiS` / `TmiS`, like `karT`).
If `SimD` is missing it reports that no sim data was found.

`SimD`: `count` records of 80 bytes, one per **slice** (a cross-section of the road, about 6 m
apart). Verified on all 15 tracks (record size = length / count = 80).

| dword | contents | evidence |
|---|---|---|
| 0 | 0 on every slice seen | data |
| 1-3 | centre point x, y, z | data: consecutive slices 6 m apart along the road |
| 4-6 | up vector (about 0, 1, 0) | data |
| 7-9 | forward vector (unit, along the road) | data matches the slice-to-slice direction; decomp: the wrong-way test dots the car velocity with these |
| 10-12 | right vector = up x forward in the file's left-handed space | data |
| 13 | bit field, values 0x18000 (most), 0x1C000, 0x38000, 0x3C000, 0x3E000, 0x7E000... | data; meaning not decoded (surface or AI flags) |
| 14-15 | two small integers (1, 1 on most slices) | decomp: an alternate grid layout uses them as lane counts |
| 16-17 | two widths (5.0 on most) | not understood |
| 18 | left half-width | decomp: the grid puts left-side cars at -0.5 x this along the right vector |
| 19 | right half-width | decomp: right-side cars at +0.5 x this |

`SimT`: u32 group lengths that add up to the slice count; without `SimT` all slices are one
group. Verified on all 15 tracks. Group 0 is the main road; the others are alternative routes
(e.g. alps: 1795 main + 272 + 22 + 16 = 2105).

### Route graph and lap fraction (decomp `0x480140`, `0x480900`, `0x480b40`, `0x481910`, `0x4819e0`, `0x481b70`)

- A group is split where the end of another group lies within 16 m (squared distance < 256,
  measured on x/z) of one of its slices that is more than 5 slices from either of its ends.
- Group ends within 16 m of each other form one junction.
- Starting from the junction at the start of the first group, the game finds the **longest route
  in slices** to every junction (a route that comes back to the start junction is counted but not
  followed further). The longest of all is the lap length in slices.
- Each junction's lap fraction = its route length / lap length. A slice's fraction is interpolated
  linearly by slice count between the fractions of its run's two junctions; a run that ends at the
  start junction ends at 1.0 (an end fraction <= 0.01 is treated as 1.0).
- Backwards races use `1 - fraction`; on a circuit an exact 1.0 then becomes 0.0.

Our `race_path::RoadPath` implements the same idea with its own code (union-find for the
junctions, a depth-first longest-route search that skips junctions already on the current path so
it cannot loop).

**What this gives on the 15 tracks** (from `race_audit`, `logs/race_audit.txt`):

| tracks | kind |
|---|---|
| alps, autobahn, canyon, castle, coastal, farmland, foothills, forest, industrial | point-to-point: the main road does not come back; each has a start line and a finish line kilometres apart |
| monaco1-5 | circuits built from several groups |
| skidpad | one closed group (circuit) |

### How a car's slice is followed (ours)

Retail keeps the current slice per car (a short at car `+8`); how it updates it was not read. Ours
searches 12 slices either side of the previous slice, plus the first/last 12 slices of the runs
joined at a nearby junction, and takes the nearest centre point; if that is over 40 m away it
searches the whole track. Guess, tested by `race_drive`.

## Start and finish files (`<track>_fstart.scn`, `_bstart.scn`)

Text scene files (format shared with `_cameras.scn`, `_audio.scn` and the `_st*.scn` scripts). Each
element: header `<KIND>_ELEMENT <version>` (TRIGGER, GEOM, CAMERA, AUDIO seen), a position line, an
orientation line (three tab-separated rows of three), then kind-specific lines, the last one a name.

The start files hold two TRIGGER elements and two GEOM elements:

- TRIGGER version 5: parameter line, then the two end points of the trigger line, then one more
  line. The first parameter is the trigger type, the fourth the line width (e.g. alps 19.0 m).
  Type **2 = start line**, type **1 = finish line** (decomp `0x416170`: crossing a type-1 trigger
  finishes the car; a type-0 trigger moves on to the next checkpoint). Verified by the positions:
  the type-1 line sits where the main road ends, the type-2 line near where it starts; `_bstart`
  swaps them.
- GEOM: second field `1213419078` = `FNSH` as four bytes, the finish banner model, placed at the
  finish line (verified by position).
- monaco1-5 and skidpad have no start file.

The file is picked by direction: `_FStart.scn` forward, `_BStart.scn` backward (decomp `0x46d600`).

## Start grid (decomp `0x410730`, `0x410580`)

- Start slice: the slice nearest the start trigger. Without a start trigger: slice 1; on a
  point-to-point road with 100+ slices, slice 50.
- Every car is placed **ahead** of the start slice in the race direction:
  `(cars - grid_position) * 3 + 1` slices with more than two cars, 1 slice otherwise. So pole
  (grid position 0) is furthest ahead and each place is 3 slices (~18 m) further back.
- Grid position even = left of centre by half the left half-width, odd = right by half the right
  half-width; 0.5 m up; facing along the slice.
- Backwards the sides are swapped in our code (decomp flips the side when a direction product is
  -1; which global that is was not identified, so this is a guess).
- Where the player starts in quick race was not checked; we put the player last (guess).
- Retail has a second layout using the lane counts (dwords 14-15) and a cop-chase layout; not used.

## Clock and countdown (decomp `0x414ff0`, `0x4152e0`, `0x4a45d0`, `0x565030`, `H321Go`)

- The timer callback runs at 128 Hz (timer set up with 0x80). It counts sim ticks at half that:
  **race ticks are 64 Hz**. The earlier "128 Hz" note applies to the front end clock only.
- The race tick counter starts at 0 when the race loads.
- Countdown overlay (`H321Go`): shown for ticks 256-447; digit "3" for 256-319, "2" for 320-383,
  "1" for 384-447. A table in the exe holds the ticks 256, 320, 384, 448 at which the four
  countdown events (sounds) fire. Tick 448 starts the race.
- Times: lap and race times are tick differences, printed `mm:ss.hh` as minutes = t / 3840,
  seconds = t / 64 mod 60, hundredths = (t mod 64) x 100 / 64 (the network "Finish %02d:%02d.%02d,
  Best Lap ..." message). The race time of a car counts from tick 448.
- **Verified in footage** (let's play at 10:20-10:27): the race scene appears about 4 s before the
  "3"; "3", "2", "1" one second each; at the start the "1" disappears and the HUD appears. **There
  is no "GO" graphic.**

## Laps, progress and finish (decomp `0x416310`, `0x417440`)

Per car: lap count, owed laps, the previous and current progress.

- Progress = laps + fraction - owed.
- Circuit: when the fraction wraps from over 0.5 to under 0.1, the car crossed the line forwards:
  if it owes a lap the debt is paid, otherwise the lap count goes up. A wrap from under 0.1 to over
  0.5 is a backwards crossing and adds an owed lap.
- When the lap count changes, the lap time (tick - lap start) is stored and the lap restarts.
- When the lap count reaches the race's laps the car is finished (state 2) and its finish tick is
  kept.
- Point-to-point: finished on crossing the finish trigger (`0x416170`). Retail also resets a car
  that falls more than 0.005 behind its start fraction (not done by us).
- Running order: by progress (finished cars first, in finishing order). The HUD's own position
  code (`HPosition`) was not read; order by progress is our reading.

## Wrong way (decomp `0x4a7660`, `HWrongWay`)

- Each tick, the car's velocity is dotted with its slice's forward vector (negated when racing
  backwards). Below zero (and the car not finished, not crashed/reset) a counter goes up; at zero
  or above it resets. The warning shows while the counter is not zero, only after tick 448.
- `HWrongWay` blinks: shown 64 ticks (1 s), hidden 32 ticks (0.5 s).
- A voice/sound is triggered from 64 ticks on, every 32 ticks, up to tick 148 of the counter.
- Ours adds a 1 m/s minimum speed so physics jitter at standstill does not trigger it (guess).

## `<track>_forward.key` / `_backwards.key` (decomp `0x44fb40`, `0x44fc30`, `0x44fcb0`)

Not the race path. 3,208 bytes: u32 version (must be 1; older ones are rejected), u32 number of
used keys, then 400 slots of (i32 first slice, f32 value); unused slots hold (-1, 400.0) and the
last used key has slice 100000. The value of the key whose slice range contains the current slice
is used. The loader's fallback table is a single value 600, and the memory tag is `CRdrKeys`
("renderer keys"), so it is most likely the **draw distance in metres by slice**. Not used yet.

## Results screen

Not checked: the let's play race at 10:20 goes straight back to the menus. Ours is a placeholder
panel 2 s after the finish (the delay is a guess).

## Checks run (2026-10-17)

- `cargo run -p nfspu-game --example race_audit --release`: all 15 tracks, both directions load,
  grid and finish line sane, 0 problems (`logs/race_audit.txt`). It also counts road slices that
  have no ground from our ground loader under them.
- `cargo run -p nfspu-game --example race_drive --release`: an autopilot drives the real car
  physics through the race rules. Forward: alps, canyon, castle, coastal, farmland, foothills,
  forest (point-to-point) and monaco5, skidpad (2 laps) finish with the countdown, hold and lap
  times as expected. Backward: alps, monaco5 (2 laps), skidpad (3 laps) finish. autobahn,
  industrial and monaco1-4 fail because the car falls through holes in our ground mesh (the audit
  lists 32-183 road slices without ground on those tracks); that is the ground loader, not the
  race rules. **Not tested** by driving: those six tracks.
- Live (real window, `NFSPU_RACE=skidpad,forward,1`): the countdown events came exactly 1 s apart
  and the car stayed on the grid. Driving a full race by hand in the window: **not tested**.
