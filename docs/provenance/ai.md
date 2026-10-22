# Opponent route choice at junctions

Evidence: the retail executable (Ghidra, read-only) and the shipped `GameData/Track/<track>.jnc`
files. Code: `crates/nfspu-game/src/ai_route.rs` (ours), `nfspu_formats::aidata::parse_jnc`.
Other opponent behaviour is in `race.md`, "Opponents".

## Junction table (track load)

The track loader reads `<track>.jnc` right after building the road graph (decomp: track load
`0x4876e0` builds `"%s.jnc"`, reader `0x481080`; graph build `0x480140`, cut `0x480900`, join
`0x480b40`).

1. **Runs.** Each slice group of the track's sim data is a run (first, last slice). One pass over
   the runs as loaded: for each end of each original run, every other run (including tails added
   during the pass) is searched for its nearest slice (flat x/z distance, last slice excluded). If
   that is under 16 m (squared 256) and more than 5 slices from both of that run's ends, the run is
   cut there and the tail (next slice to old last) is appended to the list.
2. **Ends.** Every run has a start end (its first slice, kind 0) and a finish end (its last slice,
   kind 1).
3. **Junctions.** Runs in list order, start end then finish end: an end not yet in a junction
   starts a new one; every other free end (all runs, both ends, in order) within 16 m of *that
   first end* joins it, up to 8. So a dead end or a lone run end is a junction with one route.
4. **Default weights.** Each route gets 1 / (number of routes in the junction of the same kind).
5. **`.jnc`.** 36-byte records: u32 (the junction number, not read by the game), then 8 floats,
   the weights of that junction's routes in route order. Applied only when the record count equals
   the junction count; otherwise the defaults stay.

| Track | `.jnc` records |
|---|---|
| alps, coastal, foothills, forest, monaco2, monaco5 | 6 |
| autobahn | 35 |
| canyon | 17 |
| castle | 24 |
| farmland | 14 |
| industrial | 26 |
| monaco1 | 4 |
| monaco3 | 11 |
| monaco4 | 10 |
| skidpad | 1 |

Whether our build reproduces the same junction count is logged per track by `race_drive`
(`junctions: N (.jnc weights)` when it matches).

## Choosing a route (each AI tick, decomp `0x40b650`)

- The driver's look-ahead in slices comes from its `.ais` table (index speed x 0.2, max 23,
  rounded), the same number the target speed uses.
- If the current slice plus (minus, backwards) that many slices leaves the current run, the
  junction at that end of the run is "ahead" (`0x4816f0`).
- If that junction differs from the last one the driver chose at, it draws r = 16 random bits /
  65536 and picks among the routes **leaving** that way: start ends when racing forward, finish
  ends backward. The difficulty is the race's (0 beginner, 1 advanced, 2 expert):

| Difficulty | Leaving routes | Rule |
|---|---|---|
| 0, 1 | 1 | that route |
| 0, 1 | 2 | r <= 0.55 (beginner) / 0.70 (advanced): the route with weight >= 0.5; else the route with weight < 0.5 |
| 0, 1 | 3 | r <= 0.4 / 0.5: weight >= 0.4; r <= 0.7 / 0.8: weight in 0.3..0.4; else weight < 0.3 |
| 0, 1 | 4+ | the first |
| 2 | any | by weight: the first route whose running weight total reaches r |

  Constants: exe `0x5cc390` (0.55, 0.70), `0x5cc398` (0.4, 0.7, 0.5, 0.8), `0x5b23f0` 0.5,
  `0x5b261c` 0.4, `0x5b25f0` 0.3. In each case the first route in junction order that fits wins.
- If no route fits (e.g. two routes both weighted 0.5 and r above the threshold, or expert weights
  summing below r), retail leaves its previous route index unchanged. **Ours:** the last route
  leaving that way (guess; the stale index has no sensible meaning at a new junction).

## Following the route (`0x4817d0`)

Every look along the road (the steering points and the bend scan for the target speed) walks
slice by slice. At the first junction crossed it takes the chosen route; at any further junction it
takes the last route leaving that way. On entering a route the walk continues from that run's first
slice (forward) or last slice (backward), carrying the overshoot. The racing line `.lsp` has a
point for every slice, side roads included, so the line continues on each branch with no extra
data.

## Not done / guesses

- Traffic uses the same junctions but its own logic: not looked at.
- Our slice tracker and lap fractions still use our own graph (`race_path.rs`); only the opponents'
  route choice and look-ahead use the retail-ordered table.
- Retail's walk enters a run backwards one slice further than forwards (an off-by-one); ours is
  symmetric.
