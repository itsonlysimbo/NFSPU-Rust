# Retail oracle capture

How reference traces for the behaviour oracle (`docs/ORACLE.md`) are recorded from the retail
game. Prose and numbers only; no game data is stored. Status 2026-10-25: the method works, no
retail trace has been committed yet (see "Next").

## Safety
- Only the **copy** of the install at `extracted/retail_copy` (gitignored; made with robocopy from
  `C:\Users\CB\Downloads\NFS5 - Copy`, SaveData copied too) is ever launched. The installs stay untouched.
- The tool opens the running `nfs5.exe` with `PROCESS_VM_READ | PROCESS_QUERY_INFORMATION` only:
  it **never writes** game memory, attaches no debugger and patches nothing. The only change to the game
  is keyboard and mouse input sent with `SendInput`, which is refused unless the game window is the
  foreground window (`scripts/oracle_capture_lib.py`), so input never reaches the user's other apps.
- Both scripts are our own Python (ctypes, standard library only); no unsigned third-party tool.

## Where the numbers live in the running game (read from the decomp, checked live)
`0x5e8e2c` is a table of car pointers, `0x65741c` the index of the player. Offsets from the car:

| Offset | Type | Meaning | State |
|---|---|---|---|
| `0x88` | i32 | 64 Hz physics tick counter; 0 when the race loads, the race clock starts at tick 448 | verified (counts 64 per second) |
| `0xec4` | f32 | speed in m/s (the speedometer multiplies by 3.6 or 2.237) | verified against the gauge code |
| `0xdb4` | f32 | engine rpm (the tachometer divides by 10 after a scale) | verified: idle 130, 5800 at the shift |
| `0xd82` | u8 | gear shown on the HUD (0 reverse/neutral text, 2 = first, 3 = second ...) | seen 2, 3, 4 climbing in a launch |
| `0x7f8` | 3 x f32 | live body position (x, y, z); y is exactly 0 on the flat skid pad | verified: moves by speed x time |
| `0x3d0`, `0x400` | f32 | heading vector and a position copy | **stale**: refreshed now and then, only good at standstill |

The car pointer changes between the menu and a race, so it is re-read every sample.
Orientation while driving was not found; the tool uses the direction of travel over 16 ticks as the
yaw, which is wrong for a drifting car (a steer case needs a real yaw).

## Menu path (640 x 480 client, left clicks; the race itself switches to 1920 x 1080)
Splash: click anywhere; `Continue` (320, 401) for the selected profile; `factory driver...` (130, 104);
`done` (320, 133) on the briefing; `RACE` (355, 467). In the race `Esc` opens the pause menu: `race again`
(960, 533) restarts with tick 0. Quick race: `singleplayer` (232, 55), `quick race` (320, 263),
opponents dropdown (555, 128) then `0`, car dropdown (230, 55).

## Findings from the first live run (not yet turned into oracle cases)
- Factory Driver mission 0 runs on the **flat skid pad** (y = 0) but its car is catalogue id 161
  with 13 tuning parts, so it is not a stock-spec car; its traces cannot gate our stock `car` sim.
- Côte d'Azur quick race (car '95 911 Turbo 3.6 Coupé 993, 0 opponents) starts on a 10 % downhill,
  so it is not a flat-ground reference either. The profile's location list had only four tracks
  (Côte d'Azur, Pyrénées, Normandie, Monte Carlo Circuit 3), no skid pad.
- A full-throttle launch (mission car, keyboard Up from the GO tick) read: 10.4 km/h at tick 64,
  27.9 at 128, 49.1 at 192, 62.8 at 256, 75.2 at 320, 99.9 at 448, 125.1 at 640; rpm 1108 at tick 64,
  shifts to gear 3 near tick 230. Our untuned 993 self-trace has 10.68 km/h and 1139 rpm at tick 64, close.
  These are one uncontrolled run, **not** a committed reference.

## Captured (2026-10-25)
`oracle/ref/sim_mission0_standing_start.csv` and `sim_mission0_accel_then_brake.csv` (512 ticks, every 64):
retail, Factory Driver mission 0 on the flat skid pad, keyboard throttle from GO (brake from tick 320 in the
second). The skid pad is not offered by quick race at all (track-list rule in `fe.md`), so the mission is the
only flat start. The car is the mission's tuned Carrera (catalogue id 161, 13 parts), **not** a stock 993, so
both cases are `pending = "true"` with loose tolerances until we know the tune. The trace is only valid for
~512 ticks: the pad ends near tick 520. Two runs agreed to about 0.4 km/h (tick-sampling noise: the tick counter
is read between frames and interpolated). Retail braking from 75 km/h took ~190 ticks to 3.5 km/h.
Start frame: the stale heading snapshot is replaced by the first 192 ticks of travel, so these cases have no steer.
The copy's `3Greg.sav` was edited once (career year; backup `3Greg.sav.bak`) and restored; running a mission to
its end advances the copy's factory progress, so restore the backup before re-running mission 0.

## Next
1. Identify the mission car's tune (id 161, 13 parts) so a stock-spec comparison is fair, or find a flat stock-car start; then flip the two cases to active.
2. Find a live orientation (yaw) in the car struct for the steer case; the sim's steer is analog,
   retail keyboard steering ramps, so a retail steer case needs the same ramp in our input.
3. Capture, review `oracle/ref/*.csv`, set `reference = "retail"` in the case and the tolerances; the
   coordinator reviews that protected diff.

Run: `python scripts/oracle_capture.py <name> <ticks> <sample_every> "0:1:0:0" ...` from the repo
root with the copy already inside a race and in the foreground (it presses Esc, picks `race again`,
then drives the script and writes `oracle/ref/<name>.csv` and `logs/oracle_capture_<name>_raw.csv`).
