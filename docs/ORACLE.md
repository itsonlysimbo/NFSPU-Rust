# Behaviour oracle

A machine check that answers PASS or FAIL: does our game behave like the retail game? It is
the same idea as the byte-matching oracle in matching decompilations (momo5502, 2026-10-09),
adapted to a rewrite. We never compare or copy code (AGENTS.md rule 7); we compare **numbers the
games produce** for the same input.

## Pieces

| Piece | Where | What it is |
|---|---|---|
| Cases | `oracle/cases/*.toml` | One input setup each: track, car, start, input script (throttle/steer/brake per tick), how long, which values to record, tolerances. Written by us. |
| Reference traces | `oracle/ref/*.csv` | What retail produced for that case: per-tick numbers only (speed, rpm, gear, position, lap time...). Small, our own measurements, no game file content. |
| Runner | `crates/nfspu-game/examples/oracle.rs` | Runs every case headless in our sim, compares with its reference, prints one line per case and ends `ORACLE PASS` or `ORACLE FAIL`. |
| Verified list | `oracle/verified.txt` | Cases that passed. Every merge re-runs them; a case leaving the list is a regression and blocks the merge. |
| Retail capture | `docs/provenance/oracle-capture.md` + a capture tool | How reference traces were recorded from the retail game, run in a separate copy of the install (rule 4), reading values while it plays the same input script. |

Format checks (parsed values from game files: car specs, track counts) are cases too, with the
reference taken from the decomp's reading of the same file.

## Rules for agents
- A task touching a checked area is done only when its cases pass. Don't loosen a tolerance or
  edit a reference to make a case pass.
- Cases, references, tolerances and the runner are **protected** (`scripts/protected.txt`).
  Only a task whose board row says *oracle* may change them, and the coordinator reviews that
  diff by hand. `scripts/merge_guard.sh <branch>` fails a merge that changes protected files
  otherwise, including `protected.txt` and the guard itself.
- No special cases in game code for oracle runs (no "if oracle" branches, no hard-coded
  reference values).

## Guesses (not tested)
- Committing reference traces is allowed: they are per-tick measurements, like the tables in
  `docs/provenance/`, not extracted game data. If any trace would hold game file content,
  it goes in the gitignored `extracted/oracle/` instead.
- Retail capture by reading the running game's memory (addresses from the decomp) in a copy of
  the install. If that needs an unsigned tool or a patch, the capture task stops and asks.
- First tolerances: position 0.5 m, speed 1 km/h, rpm 100, gear exact, lap time 0.1 s.

## Runner status (2026-10-24, claude/oracle-runner)
`cargo run -p nfspu-game --example oracle` (NFSPU_GAME_DIR set) runs `oracle/cases/*.toml`, ends
`ORACLE PASS` / `ORACLE FAIL`, and logs to `logs/oracle.txt`. Case keys are flat `key = value`
lines. `kind = "format"` cases (`check` = `front_weight`, `mass_range`, `map_header_count`,
`map_sections`, `file_size`, `track_count`) name the provenance note they come from in `source`.
`kind = "sim"` cases drive `car` on flat ground with `script = ["from_tick:throttle:brake:steer", ...]`
and compare with `oracle/ref/<name>.csv` (tick, speed km/h, rpm, gear, x, z, yaw); a case with
`pending = "true"` is skipped. `ORACLE_RECORD=1` rewrites self-reference traces, `ORACLE_UPDATE=1`
rewrites `oracle/verified.txt` (only when all cases pass).
- The four sim traces so far are `reference = "self"`: our own numbers from this commit, a
  regression guard and **not retail**. Replace each with a retail trace (`reference = "retail"`)
  when the capture task delivers one; do not re-record to make a failing case pass.
- Quirk seen in the traces: holding brake at standstill drives the car backwards (-80 km/h in
  `sim_993_accel_then_brake`); check against retail once there is a trace.
- Checked that a damaged reference gives `FAIL (REGRESSION)` and exit code 1.
