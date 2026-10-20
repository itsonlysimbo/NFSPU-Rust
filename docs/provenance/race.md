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
- Which car gets which grid position: each racer's setup record has a grid-slot field (record
  offset 0x174, read by `0x410580`); the front end fills racer slots with the player first and the
  opponents after. **Footage** (let's play 10:32, an Evolution race): 5.6 s after the start the
  HUD shows the player 1st of 8 with the seven opponents 18, 30, 42, 60, 72, 90 and 114 m behind,
  i.e. the player started on pole. We put the player on pole (grid 0) in quick race too; that
  quick race does the same is not checked.
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

## Opponents

Code: `crates/nfspu-formats/src/aidata.rs` (file readers), `crates/nfspu-game/src/ai.rs` (driving
and picking), `race_mode.rs` (in the race). Static reading of the decomp plus the shipped files.

### Who races (front end, decomp `0x4ebba0`, `0x4e3160`, `0x4e3120`, `0x4e30e0`, `0x4e2f30`)

- Quick race opponents are **drivers** from `FEData/Data/nfs5.per`, loaded at start-up as
  44-byte records (35 records in the install):

  | bytes | field | evidence |
  |---|---|---|
  | 0-1 | driver id (1-37, a few skipped) | data |
  | 4-7 | name string id in `festrings.loc` (18000-18027, plus six at 2500-2505) | decomp: the name lookup returns this; data: 18000+ are names like Dylan, Wingnut |
  | 8-9 | difficulty 0 / 1 / 2 | decomp: the pick filters on it with `Game.Difficutly`; data: 7 beginner, 7 advanced, 21 expert |
  | 12-43 | four (skill, range) float pairs: slow bends, medium bends, fast road, default | decomp: written to the racer's `.SKILLSLOW`, `.SKILLRANGESLOW`, ... `.SKILLDEFAULT`, `.SKILLRANGEDEFAULT` fields in that order |

  Beginner drivers have skills 0.40-0.60, advanced 0.60-0.80, expert 0.69-0.99; ranges 0-0.06.
- For N opponents the game collects the drivers whose difficulty matches and draws N of them at
  random. Whether a driver can be drawn twice was not read (ours avoids repeats while the pool
  lasts).
- Footage: the opponent list in the HUD names drivers (Dylan, Parise, Wingnut, Steele, Leadfoot,
  Amazon, Backlash), so the names are what the race shows.

### Opponent cars (decomp `0x4f4fa0`, called from `0x4ebba0`; rule from `OpponentCar.Car`)

Seven opponent slots are always filled; each gets a random paint from the car's colour list.

- Same car: the player's car for all.
- Same era / same class / same era and class: draw `k` = random over the length of the whole car
  list, then walk round the list counting cars that match the player's era (record 0x1b0), class
  (0x1ac) or both, and take the one where the count passes `k`. In effect `matches[k mod count]`.
- Random: any car of the list.
- A named car: that car for all.

### How an opponent drives (decomp `0x40b650` speed, `0x40bfe0` steering; state `AIState_Racing`)

Per car, each tick:

1. **Look-ahead**: `n = round(min(speed m/s x 0.2 x car factor, 23))`; the number of slices to
   look at is entry `n` of the car's `.ais` look-ahead table (8 to 30 slices). The car factor
   comes from the car's spec record and was not read (ours: 1).
2. **Bend**: for slices 2..=look-ahead in front of the car's slice, the angle (degrees, acos of
   the dot product of the slice forward vectors) to the car's slice; the largest one to the left
   plus the largest one to the right (side by the sign of the slice's right vector dotted with
   the current forward). Multiplied by a per-slice-range factor table (zeros in the exe's data
   section, filled at run time; ours: 1) and a car factor (ours: 1), capped at 115.
3. **Table speed**: entry `round(bend / 3)` of the car's `.ais` speed table (m/s).
4. Divided by `1 + damage x 0.00667` (ours: no damage model, so 1).
5. **Surface/drive factor** from two exe tables by the slice surface and the car's drive type,
   clamped to 0.8-1.0 (0.5-1.0 / 0.7-1.0 in the second table, used when a weather flag is set
   and the car is not under cover). Asphalt for a rear-drive car is 1.0. Ours: 1.
6. **Skill**: when the kind of road changes (`bend / 3` over 20 = slow, over 10 = medium, else
   fast), a new factor is drawn: if a random number in 0..1 is over that kind's skill, uniform in
   `skill +- range` of that kind; otherwise uniform in the default `skill +- range`.
7. **Narrow road**: times `min(1, (left + right half-width) / 12)`. (12 m is the default; some
   track settings set 10 or 9; not followed.)
8. **Lead slowdown** (only when the game mode is not 3): if this car's progress minus the player's
   is more than `[1.0, 0.07, 0.03]` laps (beginner, advanced, expert) and the player's progress
   since the start x 640000 / race ticks is over 35 (a lap in under about 286 s), the speed is
   multiplied by `[1.0, 0.98, 0.95]`. Read from the exe at 0x5cc384 and 0x5cc378. So beginner
   never slows down; expert slows the most once ahead. No speed-up for cars behind was found.
9. **Throttle / brake**: throttle in proportion to `target - speed` (times a gear factor from the
   car spec); brake when the speed is over the target. Ours: on/off (the car's pedals are on/off),
   braking over target + 2 m/s; no throttle while sliding sideways over 3 m/s, or above 15 m/s
   with the steering past 70% (the target speed rises inside a bend as less of it is left ahead,
   and without this the car runs wide at the exit). A finished opponent brakes to a stop (ours;
   on point-to-point roads they otherwise drive off the end of the last slice).
10. **Steering**: the target point is the average of four racing-line points (`.lsp`, x and z,
    height from the slice) starting some slices ahead (how many is computed in a part the
    decompiler hid; ours: speed x 0.6 s / 6 m, 2-12 slices). Cars close ahead (within 11 m, or
    20 m when 3 m/s slower) push the target point sideways by about the two cars' widths. The
    steer value is the normalised direction's sideways part x 256, clamped to +-126 (full lock
    near 30 degrees). Ours: gain 3 instead of 2 (with our look-ahead, 2 let the car drift 7 m wide
    at a corner exit) minus 0.25 x the yaw rate (rad/s) so the retail tyre model does not swing
    the car round; the avoidance is our simpler version (aim 4 m to the freer side, and keep under
    3 m/s faster than a car under 7 m ahead).
11. **Stuck**: a counter goes up while the car is under 2 m/s after the start (it resets at 2 m/s
    or more); over 160 ticks (2.5 s), or facing more than 45 degrees backwards under 10 m/s, the
    car switches to a recovery state (value 5). What state 5 does was not read. Ours: reverse
    1.5 s with opposite lock; if the car has not gone along the road at over 5 m/s for 10 s, or has
    fallen 30 m below the road, it is put back on the racing line (guess).
12. Junctions: at a junction the car picks a route at random with difficulty-dependent
    thresholds (exe 0x5cc390, 0x5cc398) or by route weights. Ours: always the main route.

### Data files

- `GameData/Simulation/AICarData/<name>.ais` (22 files, 304 bytes): 32-byte name (`Porsche 993
  Final` style), 24 floats look-ahead in slices (8 ... 30), 40 floats target speed in m/s by bend
  (993: 72.7 down to 11.3; default: 70.7 down to 13.7), 16 unused bytes. The file name comes from
  the car's AI-data name in `nfs5.car` (record 0x115, e.g. `993` for a Carrera, `993T` for the
  Turbo, empty for traffic cars), `default.ais` when there is no such file (decomp `0x49ca30`);
  ours does the same (`ai::ais_stem`). At load the speed table is stretched around its minimum by
  two car factors (not read; ours: as stored).
- `GameData/Track/racer/work/<track><0|1>.lsp`: u32 version 1, then x and z per slice (the
  racing line), 0 forward and 1 backward. The decomp builds the name as `%s%s%d.lsp` and sizes it
  `slices x 8 + 4`; all 30 files match their track's slice count (`race_audit`).
- `GameData/Simulation/AI/glue.bin` / `hhglue.bin` (21 fixed-point factors 1.25 ... 0.82 by the
  player's distance in 60 m steps, from the CSV next to them) and `tuning.bin` (difficulty
  multipliers 0.8 / 0.975 / 1.03 per old car type) are loaded, but no reader of either table was
  found in the decomp; they look like leftovers of the earlier games' catch-up. Not used.
- `prsonal.bin`, `scripts.bin`: personalities and reaction scripts (horn, swipe...), loaded into
  each car (`0x407ef0`); not used yet.

Opponents drive the same retail car physics as the player (`car_sim.rs`): the car's own `.sim`
(`car_load`, by catalogue id), an automatic gearbox, the brake held on the grid until GO, and the
retail car-to-car impulse against each other and the player (`car::collide_cars`, every tick).

Not done: traffic, retail's proportional throttle and its AI-side physics parameters
(`TRAFCFG.DES` lists "in control" / "out of control" AI physics), damage, junction route choice,
personalities and reaction scripts.

## Mirrored direction (2026-10-19)

`Track1.Direction` values 2 and 3 are "forward/mirrored" and "backward/mirrored". The loading
screen has always mirrored its map for them; the race itself did not mirror the track until now.
What retail does with the flag is only partly known: the loading notes say it is "the mirror flag
that also flips the 3D projection's x", read from disassembly, but flipping only the projection
would also mirror the car model and sign text on screen, and it would invert the steering feel.

Our implementation mirrors the **world geometry**, not the projection: the file-to-world x factor
for track, ground, walls, sky, road slices, start grid, finish line and AI racing lines is `+1`
for a mirrored race instead of `-1` (`nfspu_game::set_world_mirrored` / `world_mirror_x`). Car
models and garage rooms keep the constant `MIRROR_X`. Culling follows the factor's sign, so the
one-sided track faces stay correct. The choice reaches the race child in both `NFSPU_LOADING`
(`…,mirror`) and `NFSPU_RACE` (`forward_mirrored` / `backward_mirrored`).

Verified offline (`NFSPU_MIRROR=1` on the examples):
- `race_audit`: all 15 tracks, both directions, 0 problems and 0 road slices without ground
  (`logs/race_audit_mirror.txt`).
- `race_drive coastal forward 2 7 1`: seven opponents and the player finish, ALL OK.
- Live: the race log shows `mirrored: true` and the grid is the exact x/yaw negation of the
  unmirrored run. Captures at the same countdown (`logs/hud_capture_unmirrored.png` and
  `logs/hud_capture_mirrored.png`) are left-right mirrors of each other: the cliff with the
  balcony house and the guardrail swap sides, and the car itself stays unmirrored.

**Not tested**: whether retail's image is mirrored by the projection instead (the difference is
only the car model and sign lettering, plus steering feel); the live feel of a mirrored race.

## Results screen

Not checked: the let's play race at 10:20 goes straight back to the menus. Ours is a placeholder
panel 2 s after the finish (the delay is a guess).

## Checks run (2026-10-18, opponents)

- Unit tests: `cargo test -p nfspu-game -p nfspu-formats`: all pass (9 in `ai`, the readers in
  `aidata`, request parsing in `race_mode`).
- `race_audit`: all 30 `.lsp` files have one point per slice; the line is 1.0-4.7 m off centre on
  average, at most 6 m along the slice (so the x/z reading and the x mirror are right), but some
  points lie outside the slice half-widths (worst 36 m on foothills), which is why the driving line
  is clamped 1 m inside the road (`ai::driving_line`, ours).
- `race_drive <track> <dir> <laps> 7 <difficulty>` (7 opponents in 993s, retail physics): monaco5
  (2 laps, expert), coastal (beginner), forest (expert), alps backward (advanced), skidpad (3 laps,
  expert): every opponent finishes. Expert laps on monaco5 1:26-1:36, skidpad 0:26-0:28;
  beginners on coastal 4:08-5:09. On monaco5 one or two cars a race get stuck after a pile-up
  around slices 170-222 and are put back on the road. On skidpad the example's own simple player
  autopilot got stuck after being hit (the opponents were fine).
- Live (real window, `NFSPU_RACE=monaco5,forward,1,7,2`,
  `NFSPU_OPPONENTS=154,157,161,170,153,159,160`, seed 777, the player standing still on pole):
  all seven opponents finished, in order Frank (GT1) 1:18.40, Machine (993 Turbo) 1:23.12,
  Tbone, Rogue, Oconnell 1:30.76-1:31.93, Billy 1:45.15, Boomer 3:02.28 (stuck for 82 s near
  slice 171 before being put back). The reset rule was then changed to count only progress along
  the road; the same race again: Boomer put back after 10 s and 1:55.96, the others within 0.4 s
  of before. The running order and the player's position (8 of 8) updated every tick.
  **Not tested**: a race where the player finishes against opponents (the results panel with
  driver names), and quick race started from the menu with opponents (`run_flow` passes
  `NFSPU_RACE=...,<opponents>,<difficulty>` and `NFSPU_OPPONENTS=<catalogue ids>`).

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
