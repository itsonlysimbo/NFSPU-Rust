# Pursuit (police) mode

Evidence: the retail executable (decomp classes `AIHigh_Cop`, `AIHigh_BasicCop`, `AIHigh_BasicPerp`,
`AIState_Chase`, read-only), the install's `Track/Cops/*.cop` and `CarModel/cop*` files.
Code: `crates/nfspu-game/src/cop.rs`, `pursuit.rs`, `nfspu_formats::aidata::parse_cop`.
Headless check: `RACE_DRIVE_PURSUIT=1|busted|time` in `race_drive`.

**Status:** the data layout and the AI's shape are read from the install and the decomp; the mode's
rules (tickets, time limit, bust test) are **guesses** because the retail rule values sit in
untyped global tables that were not decoded. Guesses are marked below.

## What the install has

- Cop cars: `CarModel/cop356`, `cop930`, `cop993` (plus `_german` variants of each), each with
  `.crp` body, `.tpg`, `D`/`L` `.fsh` textures. No separate `.sim` or `.ais` for cops was found in
  `GameData/Simulation`, so the cops presumably share the civilian cars' physics (we use the 993's).
- `Track/Cops/<track>.cop` for all 15 tracks (`alps`..`skidpad`).
- Game mode names in the exe strings: `Pursuit`, `HumanCop`, `SuperCop` (so the player can also be
  the cop; not built). Speech/input commands: `C1_SIREN`, `C1_CALLROADBLOCK`, `C2_SIREN`,
  `C2_CALLROADBLOCK` (and `COMMAND1/2`, `LONGPRESS` variants): the human cop has keys for the
  siren and for calling a roadblock. `CopSpeakBuf` is the cop radio chatter buffer. `Cop not found: %d`
  is a lookup error.

## `.cop` file format (decoded)

Loader: `0x403040` builds `"%s%s.cop"` (track folder + name) and `0x40e3b0` parses it.

| Offset | Meaning |
|---|---|
| 0 | u32 record count |
| 4 + 20 n | record n: five i32 |

Record fields: kind (always 1), **slice** (road slice the cop waits at; 100..3700), a third
value (0 almost everywhere; 1 on skidpad, farmland's and forest's last record), a fourth value
(-1, 0 or 1; read as the side of the road, **guess**), a fifth value (0 or 1, **not decoded**).

| Track | Records | Notes |
|---|---|---|
| alps 7, autobahn 21, canyon 4, castle 13, coastal 5, farmland 8, foothills 3, forest 9, industrial 4, skidpad 16 | as the header | slices step by 200 early, then spread out |
| monaco1..5 | header says 21, file holds 8 (164 bytes) | the patched files; read only the records present |

The slices run along the course in race order, so the cops are posted at intervals, a new one
roughly every 200 slices: an ambush line rather than a spawn pool. Skidpad has 16 cops on a short
loop, alternating side.

## Cop AI (decomp)

`AIHigh_Cop` is a six-state machine (state code at `+0xc`): 0 starting, 1 non-active (waiting),
2 normal driving, 3 idle, 4 chase, 5 off-road. Each tick it:

1. Clears per-tick flags and checks the target (`0x405110`).
2. In the waiting states, when the target is between 50 m and 300 m away (flat distance) it
   builds an `AIState_Chase` (`0x40cb60`, 0x94 bytes) aimed at the target car with an approach
   offset taken from a table at `0x5cbee8` (3 floats per entry, entries chosen by the cop's slot),
   and moves to state 4. A target closing faster than 4.4 m/s inside 300 m also triggers it.
3. A cop out of range or without a target goes back to normal driving / idle.

`AIState_Chase` (per tick, `0x40cf70`): computes the distance and the speed difference to the
target. If the cop is far behind (|gap| > 140 m and the speed difference small) it uses the
"catch up" branch (full throttle, the ordinary racing-line follower); if it is level with the
target at under 6.7 m/s it takes the ramming branch (`0x40d150`); otherwise it steers toward the
target (`0x40d680`) with a speed cap about 80 m/s and a lateral offset. After a hit a timer
(`+0x8c`) lowers the steering gains (0.5 and 0.75) for a while.
`AIHigh_BasicPerp` is the pursued driver's AI: it watches whether a cop is chasing it (flag 0x20/
0x200 on a car) and switches between flee modes (1 stopped, 2 reverse out, 3/4 evade) with a
minimum 640-tick gap between switches.

## What we built (guesses marked)

- **Who you play:** the pursued car (retail's `Perp`), on a normal race track with the track's
  laps. (Retail's `HumanCop` role is not built.)
- **Spawning:** one parked cop per `.cop` record, on the right of the road (`side` 1 or 0) or the
  left (-1), facing the race direction. They wake when you come within 150 m (**guess**; retail
  50..300 m) and chase.
- **Chase:** far away, follow the road toward your slice; inside 45 m aim where you will be in
  half a second; inside 14 m ram (target speed = yours + 8 m/s). Top speed 55 m/s (**guess**).
  A cop 450 m away gives up. A cop that stops for 2 s reverses out for 1.2 s.
- **Roadblock:** 45 s into a chase, then every 90 s, two cop cars park across the road 260 m
  ahead, on the outer sides with a gap in the middle (**guess**). Dropped once you are past.
- **Win:** cross the finish within the time limit (lap length x laps / 14 m/s + 30 s; **guess**).
- **Lose:** three tickets, or the clock runs out. A ticket = a cop within 8 m while you are
  slower than 9 m/s for 2.5 s; the cops back off for 6 s afterwards (all **guess**).
- **Sirens and lights:** `pursuit::siren_light` alternates red/blue about 3 times a second
  (**guess**). The siren sound and cop radio (`CopSpeakBuf`) are not wired.
- **HUD:** `pursuit::hud_line` gives "TIME m:ss  TICKETS n/3  COPS n". The retail FE layout for
  pursuit was not found; **not drawn in the game window yet**.

## Not done / next

- Wire `Squad`/`Pursuit` into the Bevy race (`race_mode.rs`): draw cop cars (cop993 model), light
  bar flashing, HUD text, siren sound, results text.
- Decode the retail rule values: the tables near `0x5cbeb0` (stride 0x14), `0x5cbee8`, `0x5cbfc0`
  and the `DAT_006573f0/f4` difficulty values, and find where tickets/time are counted.
- `HumanCop` / `SuperCop` modes: player as the cop, siren and roadblock keys.

## Evidence status of the tuned values (coordinator review, 2026-10-24)

Nothing below comes from retail. These are **guesses** chosen so the headless autopilot can run a
pursuit at all; they must be replaced when the retail tables are decoded:

- **Roadblock timing** (first after 45 s of chase, then every 90 s, 260 m ahead): guess. Retail has
  a "call roadblock" command (`C1/C2_CALLROADBLOCK`) for the human cop; how the AI cops place one
  was not found. I moved the timing later after a first draft (25 s / 40 s) trapped the autopilot;
  that is test-driven tuning, not evidence.
- **Roadblock gap** (cars lie against the road edges; on a road narrower than 14 m only one side is
  blocked): guess. Retail's geometry unknown.
- **Ramming** (aim at the player's predicted position inside 14 m, +8 m/s): guess, loosely shaped by
  the decomp's "ramming branch" at `0x40d150`, whose numbers were not read.
- **Ticket/bust/time-limit rules**: guesses (see above).
- **race_drive**: in pursuit runs the "player stuck / recovered" watch is only logged, not
  failed, because the cops ram the autopilot into walls. Normal races are unchanged and still fail
  on it. If the autopilot cannot escape retail-like cops, the escape case should report that
  (it prints the outcome and tickets) rather than be tuned until it passes.

## In the game window (claude/pursuit-window)

- **Cop cars:** the car catalogue (`nfs5.car`) marks police cars as **kind 2**: ids 131 `Cop930`,
  218 `cop356`, 219 `cop993` (+ German variants 211, 217, 220), each with its own `.sim`
  (`930Tcoupe30`, `356BcoupeGS20`, `993Tcoupe36`) and AI name. So cops do have their own physics:
  each `.cop` record gets a random non-German police car and that car's `.sim`
  (`cop.rs` variants, `race_mode::build_pursuit`). The German variants are skipped (guess: they
  are the German-language-install skins).
- **Menu entry:** this install has **no pursuit front-end layout** (`FEData/Layouts` has
  `QuickRace`, `knockout`, `quickknockout`, `singleplayer`, ... but nothing for pursuit), and the
  exe's mode table lists `Pursuit`/`HumanCop`/`SuperCop` only as names. So there is no retail
  button to copy; `NFSPU_PURSUIT=1` turns pursuit on for a menu or `NFSPU_RACE` start. Guess: retail
  reached it from a hidden/debug path.
- **Light bar:** two blocks (red, blue) on each cop roof shown in turn by `pursuit::siren_light`
  (guess: ~3 Hz alternate; retail lamp geometry/timing not read).
- **Siren:** `siren_800.bnk` from `Sounds/track.viv` as a loop, volume rising from 0 at 300 m to 0.6
  at the player (guess). Not checked against retail: that bank may be the ambient/emergency siren
  rather than the cop one.
- **HUD:** one Bevy text line at the top (`TIME m:ss  TICKETS n/3  COPS n`, then YOU ESCAPED / BUSTED /
  TIME UP). Our layout, not retail's.
- **Ending:** a lost pursuit marks the player finished so the usual race summary appears; the
  verdict is the HUD line. (Guess.)
