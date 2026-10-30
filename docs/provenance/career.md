# Evolution career (retail "Evolution" game type)

Findings and plan for the career mode. **Verified** = read from the shipped data files or the
executable; **guess** = ours, to be checked against retail footage. Code: `career.rs` (rules, data,
our save file) and the career parts of `menu_mode.rs`. Nothing here is copied from the decompile.

## Structure (what the data says)

- **Eras** (strings 521-523, `nfs5.car` era and `nfs5.trn` era code): Classic opens in 1950, Golden
  in 1970, Modern in 1989; the career ends in 2000. The career year is the first 32-bit value of
  the save's `Tournament` section (see `fe.md`).
- **Tournaments** (`FEData/Data/nfs5.trn`, 35 records of 0xd40 bytes). 20 career tournaments (era
  codes 521-523: 6 classic, 7 golden, 7 modern) and 15 "club events" (524-526, the Factory Driver
  mode; not built). Verified fields of a record:

| Offset | Field |
|---|---|
| 0 | `u32` id (1, 2, 4, 9, 13 ... 59; not contiguous) |
| 4 | `u32` era string: 521/522/523 career, 524/525/526 club events |
| 8 | `u16` career year the era opens (1950/1970/1989; 0 for club events) |
| 10, 12 | `u16` track count and `u16` track ids. Only the classic and golden tournaments list tracks (4 and 6, all `locked=3` tracks of `nfs5.trk`); the modern ones list none |
| 0x20, 0x24, 0x28, 0x2c | `u32` string ids: short name (`356`), full name (`356 Challenge`), description, car rule (`356 models only`) |
| 0xec..0x108 | eight `u32` prizes for 1st..8th place (356 Challenge: 5000, 4000, 3000, 2500, 2000, 1500, 1000, 500; Class 3 Trophy: 150000 down to 30000) |
| 0x118 + 0x58 n | seven opponent blocks: `u16` driver id (31-37), `u16` that driver's first car id, then more ids (other cars / drivers) and colours. Meaning of the rest **not decoded** |

- The prize tables grow with the era (classic 5,000, golden up to 15,000+, modern 30,000 to 150,000),
  so money is meant to buy better cars as the eras go on.
- **Bonus tournaments** (550 A Trophy, 935 Trophy, GT1 Trophy; ids 31, 32, 33): their descriptions
  say they are bonuses for completing an era, and the winner gets a prize car (the
  `FEPrizeCarAccessor` / `FEFactoryDriverPrizeCarAccessor` classes, availability 3 cars in
  `nfs5.car`).
- **Car rules** (string at 0x2c): e.g. "356 models only", "Class 3 or Class 2", "Pre-1980 models",
  "911 turbos only". The data does not hold a car-id list for them (only the text); the rule would
  be matched on the car's model name, class and year. **Not built**: the first slice lets any owned
  car enter.
- **Cars** (`nfs5.car` 0x208): price (11000 for the first 356), era, class, year range, availability.
  Dealer prices and used-car prices come from these (`DealerCar.Car(DEALERCARPRICE)`,
  `OwnedCars.Car(PURCHASEPRICE / CURRENTVALUE)` in `dealership.lay` and `garage.lay`).
- **The save** (`3Greg.sav`, `8Driver.sav`, `XXDefXX.sav`): one long dump of named sections
  (`Missions`, `PartsMarket`, `Tournament`, `PlayerInfo`, `TrackRecords`, `UsedCarMarket`, ...).
  `Tournament` begins with the career year (1950 = 0x79e); the rest of the section is the `nfs5.trn`
  table byte for byte (compared on a played profile and the blank one: identical), so progress is
  **not** stored in the table. The owned cars sit in `PlayerInfo` (a copy of each car's catalogue
  record; the sample profile has one `'50 356 1100 Cabriolet`, paint 19). The account balance was
  not found among the readable fields. The profile list file is `player.lst`.
- **Screens** (`FEData/Layouts`): `GameSetup` ("evolution..." button, callback `SETTYPECAREERMODE`),
  then movie/intro screens (`careerfirst`, `firstlicense`, `missionbrief`, `missionmap`),
  `tournament` (event type, event, car and prize info; buttons garage / race info),
  `prerace` (location, difficulty, record time, remaining tracks, practice run, forfeit, garage,
  repair), `garage`, `dealership`, `usedcars`, `parts`, `carsetup`, `paintshop`, `repair`,
  `endofera`, `levelvictory`, `VictoryRoom` (prize car), `trackrecords`. The bottom bar shows the
  cash only in career mode (`NFS5.ACCOUNTTHING` and the "ACCOUNT" label, game type 4).

## What the first slice builds (`career.rs`, `menu_mode.rs`)

- **Profile**: our own file `settings/career/<player>.txt` (key=value, version 1; `NFSPU_SETTINGS_DIR`
  moves it). Created when the player first presses "evolution..." and rewritten after each race.
  The install and its `.sav` files are never written.
- **Start**: year 1950, the starter 356 Cabriolet in the garage (id 76, paint 19), cash
  **0 (guess)**.
- **Screens wired**: "evolution..." -> `tournament`. The event-type dropdown shows the era name
  (**guess**), the event dropdown lists the open tournaments, the car dropdown the owned cars; the
  prize, times-completed, difficulty, car rule and entry-fee lines and the description are filled
  in; `prerace` shows the next track (its dropdown the remaining ones). RACE starts the next race
  of the chosen tournament with the chosen car; the account shows in the bottom bar.
- **Rules (guesses unless noted)**:
  - open tournaments: those of the current era; the bonus one only when the era's regular
    tournaments are all complete;
  - a tournament is its track list (the modern era uses the golden era's six); one race per track,
    two laps, seven opponents driving the tournament's first opponent car, difficulty of the track;
  - each race pays `prize[place - 1] / number of races` (places 1-8 pay; an unfinished race pays
    nothing and does not count);
  - finishing every race completes the tournament (counter kept); completing all regular
    tournaments of an era moves the career year to the next era;
  - entry fee: none (the field was not found, shown as `$0`).

## Pieces still to build (rows on `docs/WORK.md`)

1. Intro flow for a new career (`careerfirst` movie, `firstlicense`, missions), and `endofera`.
2. Garage, dealership and used-car market: buying and selling cars (prices from `nfs5.car`, trade-in
   values), the 3D rooms already exist.
3. Parts, car setup, paint shop and repair shops (upgrades and damage costs).
4. Car-rule matching and tournament standings (points over several races, `HTournamentStandings`),
   prize cars for the bonus tournaments, and the real payout timing.
5. Club events (Factory Driver mode, eras 524-526) and the mission/licence tests.
6. Reading a retail `.sav` profile into a career (the owned-car list is already readable).

## Retail footage of the career (2026-10-23 11-33-02.mp4, 3:40, Greg's new career)

Frames pulled to `extracted/footage/` (gitignored); contact sheets of three-second steps. **Seen**:
nothing of the paint shop, parts or repair screens, and no event was raced; the recording ends
with a test drive that bugged (no control of the car; treated as a retail bug, not copied).

| Screen | What the footage shows |
|---|---|
| Main menu -> `evolution...` | Goes straight to `tournament` (no intro movie for this profile). Account bar shows `0` with an empty level bar: the new career starts with **$0** (confirms the slice-1 guess). Banner "CLASSIC ERA 1950" lit, Golden and Modern dim |
| `tournament` | `car...` dropdown (closed text `356`), `garage...`, `event type` dropdown whose only entry is **"tournaments"** (string 470; ours showed the era name), `event` dropdown (closed `356`), `race info...`. Centre: entry fee 0, restrictions "356 models only", difficulty beginner, times completed 0, prize money 5,000, name "356 Challenge", description "A challenge for the new driver. A quick tournament showcasing Porsche's flagship sports car: the 356.", track list NORMANDIE / COTE D'AZUR (the next one in yellow) beside the track outline |
| `prerace` ("race info") | Header `race info`; `info` dropdown (track), `practice run`, `garage...`, `repair`, `forfeit` (greyed, nothing to forfeit); map picture with left/right arrows; bottom right panel: track name, difficulty, record time. **Record times: Normandie 03:09.00, Cote d'Azur 02:44.00** (beginner) |
| `garage` | Buttons car, new cars, used cars, sell, showcase, settings, repair, car setup, parts, paint shop; CONDITION bar (0 to 100, green); bottom panel `'50 356 1100 Cabriolet`, purchase price 11,000, current value **10,890** (matches our value rule). The car is the orange 356 on the lift; the rear view when entering, then it turns |
| `dealership` ("porsche centre") | `car` dropdown (356), `color`, `interior`, `buy`, `test drive`, `showcase...`, `car compare...`; bottom panel name, car price 11,000, class "Classic 3". Two cars in 1950: Cabriolet and Coupe Ferdinand. Colour list: 14 or so swatches, scrolling. Doors open on the car |
| `showcase` | Tabs `statistics` (general, chassis and drivetrain, performance data). 356: weight 790 Kg, length 3950, width 1660, height 1300, wheel base 2100 mm; layout rear-engine rear-drive, drum brakes, wheels 3.25 x 16, tyres 5.00 x 16, gears 3.09 / 1.765 / 1.227 / 0.96, final 4.428:1, reverse -3.6, 4-speed manual; 4 cylinder boxer 1086 cc, 40 bhp @ 4200, 51 lb-ft @ 2800, 5500 rpm, 7.0:1, 0-60 in 23.5 s |
| `car compare` | Two cars with five bars: acceleration, top speed, braking, handling, overall. Cabriolet 30 / 40 / 56 / 30 / 41; Coupe Ferdinand 30 / 40 / 56 / 33 / 42 |
| `car setup` | `test drive`, `save`, `load`, `default` buttons; sliders: ride height, stiffness, travel, toe-in, brake balance, downforce (50 / 50), tyre pressure (30 / 30); gear ratio bars; "part inventory" list (engine, brakes, transmission, suspension, shocks, tyres with "356 Standard ..." parts, all INSTALLED) and `uninstall part` / `parts description` |
| test drive popup | Title `test drive`, the car name, a `LOCATION` list: **Cote d'Azur, Pyrenees, Normandie, Monte Carlo Circuit 3** (the four `locked` = 0 tracks of `nfs5.trk` without the skid pad), buttons `test drive` and `cancel`. Then the loading screen with the chosen track, then the drive |
| test drive HUD | Damage car and 0 to 100 % bar top left, `record` 02:44.00 and a `time` counter (counts up) top right, rear mirror strip, two gauges, gear, the track map at the bottom left. No position or lap. The car did not move (retail bug) |

### Differences found, and what was done
- Event type list: "tournaments" (string 470), not the era name. **Fixed** (`career.rs`).
- Test drive: the whole popup and drive were missing. **Built** (below).
- The 3D showcase / car compare / car setup screens are drawn from their layouts by the generic
  screen code; their data (car statistics, comparison bars, setup sliders, part inventory) is **not
  fed** yet (see "Still open").

### Test drive (built, slice 3)
`TESTDRIVECHECK` (car setup) opens `testdrivecarsetup`; the dealership's `testdriveoval` and the used
lot's `testdrivesquare` popups hold the same location list (`Track1.Track`) and their buttons
(`TESTDRIVENEW`, `TESTDRIVEUSED`, `TESTDRIVECARSETUP`) start the drive. The pre-race `practice run`
(`TESTDRIVE`) drives the next race's track. A drive is a race with no rivals, no traffic, 8 laps (so
a point-to-point road ends after one run and the results close back to the screen the drive was
started from), in the car and paint the screen showed (an owned car brings its fitted parts; a
dealer or used car is stock). Nothing is paid and nothing is recorded. The retail HUD for a test drive
(record + running time, no position) is not copied: ours shows the race HUD (**guess**).

### Era change (built, slice 3)
When the last regular tournament of an era completes, the year moves to the next era (slice 1 rule)
and the `VictoryMessage` popup opens over the tournament screen: header "YOU HAVE PROGRESSED INTO
GOLDEN ERA!" (string 19001; the other eras use the era name, **guess**) and the text of strings
19005 + 19002 + 19003 + 19004 + 19006 + 19000: an offer to win the 1978 935 "Moby Dick" in a bonus
race at Monaco, with a practice-run hint. The prize-car race (`VictoryRoom`, 3D `ID_VICTORY` room,
`PrizeCars.Car`) is **not built**; the bonus tournament simply opens (slice 1 rule).

### Slice 3 additions and gaps (2026-10-23)
- **Tournament screen**: era banner (code-drawn: the three era names across the top, the current one
  lit with the year under it; part of the account control's bar), word-wrapped description (10 point
  font, 1.5 line spacing, centred), `TrackSmall2dSpline` (the next race's `SpSm` picture and the name
  list, next one lit). Money prints without `$`.
- **Tournament record fields**: u16 at 0xe6 is the difficulty (0 for the 356 Challenge = beginner,
  as the footage shows; Class 3 Trophy 2); u32 at 0xdc looks like the entry fee (0 for the 356
  Challenge, 500 for 2000cc, 25,000 for Class 3 Trophy; **guess**, only the zero is confirmed by
  footage). The fee is charged for the first race of a tournament and refunded if the race is not
  finished. The u16 at 0xda (10, 12, 3 ...) is unexplained (races? route set?).
- **Tournament routes**: the record's track list (identical on every tournament of an era, all
  `locked` = 3) is the era's *unlock* list. The footage's 356 Challenge races Normandie then Cote
  d'Azur (ids 14, 8; beginner, never locked). Slice 3 guesses a route = the never-locked tracks
  (not the skid pad) no harder than the tournament, highest id first. The real source was not found.
- **Record times** on race info: lap record from the save folder's `nfs5.trk` (0x1f8, 1/128 s) times
  the career's two laps (Cote d'Azur 10496 ticks x 2 = 02:44.00; Normandie 12096 x 2 = 03:09.00,
  both **verified** against the footage).
- **Starter car**: purchase price 11,000 (verified), current value 10,890 (verified).
- **Dealership room**: `dealer2a` drawn for `ID_DEALER` (see garage.md).
- **Still open (seen in the footage, data not fed)**: `showcase` statistics (general, chassis and
  drivetrain, performance data from the car record), `car compare` bars, `car setup` sliders and the
  part inventory list, the garage CONDITION bar value, damage; retail's test-drive HUD; paint shop,
  parts and repair screens were not on the recording (ours come from slice 2).

## Factory Driver (the second career; `factory.rs`)

Retail's `SETTYPEFACTORYDRIVER` game type: the player is a Porsche test driver and plays 34
missions in a fixed order, in three ranks. **Verified** = read from `nfs5.fac`, the scene files or
the executable; **guess** = ours. Layouts: `factorydriver` (list/brief), `missionbrief`,
`missionmap`, `factwelcome`, `factfirst`, `factdone`, `FactoryVictory`; art in `FEData/Factory`
(`0M01.fsh`, `1M01.fsh`..`3M12.fsh`, one picture per mission) and `GameData/DashHud/FactoryPeople.fsh`
(the narrators).

### `nfs5.fac` (34 records of 0xf0 bytes, verified)
| Offset | Field |
|---|---|
| 0 | `u32` briefing string id (3001..; `%s` is the player name) |
| 8 | `u16` rank 1-3 (Junior Test Driver, Test Driver, Chief Test Driver) |
| 0xc | `u32` promotion text shown after the rank's last mission (3191 / 3342 / 3495) |
| 0x10 | `u32` rank-title code (221-223) |
| 0x1c, 0x1e | `u16` file position, `u16` narrator (0 Billy, 1 Stephanie, 2 Dieter, 3 Frank, 4 Klaus, 5 Rolf; name is string 2500 + n) |
| 0x20 | `i16` track id in `nfs5.trk` (32 skid pad, 12 autobahn, 8 coastal, 14 farmland, ...) |
| 0x22 | `u16` route code of the race missions on circuits (2 or 3; 0 elsewhere) |
| 0x24 | `u16` scene number: the scene file is `<track file>_st<n>.scn` (rank*10+k; 0 for the circuit missions) |
| 0x3f | picture/script stem (`0M01`, `1M07`...) |
| 0x58, 0x5c, 0x60 | pass-speech string ids (retail picks one at random; 0 = unused) |
| 0x64, 0x68, 0x6c | fail-speech string ids |
| 0x70, 0x72, 0x74 | car catalogue id, part count, part ids (the mission car comes tuned) |
| 0xa0, 0xb0 | paint colours (bytes as stored) |
| 0xc1 / 0xc2 | time limit enabled / seconds (checked against the five briefings that name a time: 26, 32, 28, 50, 58) |
| 0xc4 / 0xc6 | damage rule enabled / limit (5 on the "don't scratch her" missions; other values unused) |
| 0xc8, 0xca, 0xcc, 0xce | cone rule enabled, kind (0 limit, 1 penalty), limit, penalty (1, 2 or 4) |
| 0xd0 / 0xd2 | race rule enabled / lowest passing place (1 in every race mission) |
| 0xd4.. | five opponent slots: `u16` driver id, `u16` car id |
| 0xe8..0xee | **runtime, saved in the profile**: 0xe8 passed flag, 0xea place, 0xec average damage, 0xee time |

### Mission list (file order = play order, verified in the progress code)
Retail's "current mission" is the first record whose 0xe8 is 0; `Factory.MissionCheat` jumps to
number n by marking the n-1 before it done. A **pass** sets 0xe8 and stores place / damage / time; a
**fail** leaves it 0, so the same mission is offered again.

| # | Rank | Scene | What |
|---|---|---|---|
| 0 | 1 | skidpad st1 | Entrance test (Rolf): Boxster round the skid pad cones, 27 s |
| 1-4 | 1 | skidpad st11, canyon st12, skidpad st13, autobahn st14 | Short slalom 26 s; S-curve 32 s in the Carrera RS; handbrake 360 spin, 15 s; highway slalom 28 s (Frank's bet) |
| 5, 6 | 1 | coastal st16, farmland st15 | 911 Turbo delivery across Cote d'Azur (police on patrol, damage limit 5); extended slalom, old town to forest, 50 s |
| 7-9 | 1 | skidpad st17, industrial st18, canyon st19 | Skid pad course 58 s; Boxster delivery to the docks (180 s, damage 5); rally cone hunt, 12 cones in order, 240 s |
| 10-12 | 1 | skidpad st110, canyon st111, autobahn st112 | 993 Turbo handbrake spins 36 s; 996 demo through the canyons (240 s, damage 5); standard promotion course 59 s |
| 13-22 | 2 | castle st21, alps st22, industrial st23, skidpad st24, foothills st25, skidpad st26, foothills st27, castle st28, skidpad st29, canyon st210 | Customer delivery; 4WD test in snow; warehouse parts run; show Billy the skid pad; **race Billy** (hill); skid pad stunt course; **race Billy again** (country roads); capture the flag in town (385 s); 180-slide stunt course; **race Frank** (canyon) |
| 23-33 | 3 | industrial st31, industrial st32, forest st33, skidpad st34, alps st35, foothills st36, skidpad st37, then four Monaco missions without a scene | Shake-down course; drive Dieter to the train; back-road slalom; Stephanie's skid pad; Alps switchbacks (race Stephanie); evening race with Billy; commercial stunts (180 slides, reverse, 360); GT1 at Monaco, 3 laps, no damage (route 3); race-team test drive against four drivers (route 3); 996 Turbo break-in drive; final race against Stephanie (route 2) |

Records 17, 19, 22 and 28 are races with opponents on the scene's road (start line, one finish
gate); 30-33 run on Monaco circuits and have no scene file.

### Scene files (`GameData/Track/<track>_st<n>.scn`, verified)
Plain text like the other scene files. TRIGGER elements: kind 2 = the start (its last line gives the
heading, `mode x y z`), kind 0 = waypoint gate, kind 1 = the finish ("End"); the 8th parameter number
is the order in which gates must be passed (the file order is not the pass order: canyon st19 lists
seq 12 before 11). A gate is a line (the two end points) plus a heading hint (`mode x y z` twice; the
spin missions use it to demand a sideways pass: **not decoded**, we only check the crossing). GEOM
elements tagged `CONE` (1162760003 read as little-endian text) are the cones, 2 to 39 per scene.
Narrow gates (3 m) sit on one cone: the rally and cone-hunt missions. Scenes with a 20 m gate and
few cones are the delivery runs.

### Pass/fail (verified shape, guessed numbers)
Judged when the finish gate is crossed (`HMissionSummary`): fail if the time (driving time plus
cones * penalty) is over the limit; fail if the average of the ten car-zone damages is over the
damage limit; fail if more cones than the cone limit fell; in a race, fail if the place is lower
than the minimum. Else pass. **Guesses**: the penalty unit is seconds; the damage unit is percent of
`car_damage` zone damage (0-100); a cone falls when the car centre passes within 1.3 m; we also fail
the moment the clock passes the limit (retail may let the player finish); a gate counts when the car
crosses its line extended by 1 m, narrow gates when the car is within 2.5 m.

### Rewards and unlocks (verified text)
No cash. Each rank-up shows its promotion text and hands over a company car (the `availability` = 3
cars of `nfs5.car`, `FEFactoryDriverPrizeCarAccessor`): the custom 1973 911 Carrera RS (rank 2), a
Boxster (rank 3) and, after the last mission, a 2000 996 Turbo ("you bumped Stephanie out of her spot
as Ace Test Driver"). Ace is the end rank.

### Progress (ours)
Retail keeps it in the `.sav` `Missions` section (the 34 records with 0xe8-0xee). Ours is
`settings/factory/<player>.txt`: `version`, `player`, `missions`, and `done = <index>,<place>,<damage>,<ticks>`
lines. `Progress::current` is the first mission not done; only a pass is kept.

### Built / not built
Built (`factory.rs`, tests, `RACE_DRIVE_FACTORY=1 race_drive`): the table, scene reader, gate / cone /
time / damage / place rules, progress file, a scripted driver that runs every scene mission and
checks pass, slow-fail and the cone penalty (log `logs/factory_drive.txt`).

Live in the game (2026-10-24, branch `claude/factory-screens`): `menu_factory.rs` (the
`SETTYPEFACTORYDRIVER` entry, the `Factory.Data(...)` watches of the current mission, start and
settle, the promotion screen `levelvictory`), `race_mission.rs` (the car placed at the scene's start
pose, the scene's cones as cone objects that fall when hit, a banner with time / gates / cones, the
verdict handed back through `factory_live.rs`), the progress file, and a flow-test hook.
**Verified headless** (`NFSPU_MENU=factorydriver NFSPU_FACTORY_AUTO=40 NFSPU_FLOW_TEST=1,60`): mission
0 drives on the skid pad, passes in 14.05 s with 1 cone, saves `done = 0,1,0,842`, logs the pass
speech and returns to the mission list; mission 1 passes in 16.02 s (10 gates), mission 12 in 28.59 s
(19 gates, autobahn) and mission 16 in 21.42 s (9 gates). Passing mission 12 (the last of rank 1)
logs "Congratulations on your earned promotion to Test Driver" and opens `levelvictory`
(`NFSPU_MENU=levelvictory NFSPU_FACTORY_MISSION=13 NFSPU_MENU_CAPTURE=1` saves
`logs/menu_levelvictory.png`: Rolf's portrait, the promotion text, `done`). **Found on the way**: a
mission car must not be given the road's rules — the crash reset, the stuck / out-of-bounds watch and
the wrong-way test put it back on the road every second on the skid pad, so no gate ever counted;
with a mission active they are skipped (`race_mode::race_tick`) and the physics still rights the car
where it is.

**Not built**: the mission pictures and maps (`FACTORYPIC`, `FACTORYMAP`), the pass/fail speech on
the screen (it is only logged), the race missions (opponents, minimum place), the mission cars with
their parts, the prize cars, the spin-direction rule, the police of mission 5, snow in the Alps
mission.

## Tournament car rules (2026-10-25, branch `claude/career-eras`)

Each record in `nfs5.trn` has a rule string id at `+0x2c` (shown by the screen's
`Tournament.Tournament(CARRESTRICTION)` watch). Retail only ever shows the **text**; the 20 texts
are matched against the catalogue by `career::car_allowed` (our reading, **guess** where a text is
vague). `cargo run -p nfspu-game --example career_rules --release` lists each tournament with its
fee, prize and the cars the rule accepts (`logs/career_rules.txt`).

| Rule text | Reading | Cars accepted |
|---|---|---|
| 356 models only / 356 A / 356B limited to 1600cc | name after the year starts `356` / `356A` / `356B`, engine <= 1600 | 35 / 12 / 12 |
| 2000cc 356B or 911 | any 356 B, or any name with 911 | 50 |
| Any 911 / Non-turbo 911s / 911 turbos only | name contains 911 (without / with Turbo) | 37 / 30 / 7 |
| 911 Carrera RS 2.7, 911 (996) non-turbos, 2000 911 turbos | name matches | 2 / 4 / 2 |
| 944s, 914s, Boxsters, 550 A Spyders, 935s, 911 GT1s | model name | 5 / 3 / 3 / 1 / 1 / 1 |
| Class 3 or 2 / Class N only | catalogue class 501 = 3, 502 = 2, 503 = 1 | 71 / 40, 31, 11 |

Entry: `menu_mode::start_career_race` refuses a car that fails the rule (logged; **guess**: retail's
refusal message was not found). Entry fee (`+0xdc`) and prizes (`+0xec`) were already wired.
Not tested in the window; unit test `car_rules_match_the_catalogue_names`.

## Prizes and opponents by era (2026-10-25)

Read from `nfs5.trn` for all 20 career records: prizes at `+0xec` (1st..8th) and fee at `+0xdc`
rise with the era (classic 1st place 5,000-12,000 and fees 0-1,000; golden 15,000-60,000 and
3,500-20,000; modern 100,000-200,000 and 25,000-50,000; the three prize-car trophies pay 0 and
cost 0). The seven opponent blocks (`+0x118`, 0x58 apart, car id at +2) give **each opponent its
own car**: e.g. Mixed Class Challenge uses 124-128, 911 Anniversary Cup 162-165, single-car
tournaments repeat one id seven times. Before, the game gave all seven opponents the first
block's car; now `OpponentCars::Cars` hands them out in turn (`ai::pick_cars`). Era progression
(`finish_race`: the next era opens when all its regular tournaments are complete, the bonus
trophy opens after them) was already in place and is unchanged. Per-race payout (prize / races) stays
a **guess**. Not tested in the window.

## Retail save (reading it into a career)

`save_import.rs` reads, never writes, a `.sav`. Each section is `name ` followed by values.

| Where | Value | Evidence |
|---|---|---|
| `Tournament ` +0, u32 | career year | blank 1950, played profile 1956 |
| `PlayerInfo ` +4, u32 | account balance (**guess**) | blank 11000 (the first 356's price), played 0 after it bought that car |
| `PlayerInfo` car copies | owned cars and paint | `catalog::owned_cars` |

Not read yet: tournament and mission progress (`Missions` section, undecoded), parts, damage, used-car lot. When RACE-side "evolution..." finds no profile of ours but a retail save of that player exists, the career starts from these values. Not tested in the game window.

## Evolution shops against the footage (2026-10-27, `2026-10-24 20-55-17.mp4`, branch `claude/evo-shops`)

Frames read at 1 per 4 s (`extracted/evo/`, untracked). `cargo run -p nfspu-game --example shop_check
--release` prints the parts list our shop builds for the '50 356 1100 Cabriolet in 1956.

**Matches retail (verified against the footage):**
- Parts shop list prices for the 356 in 1956 are identical to ours (the table-driven `part_value`):
  flywheel 172, lightened flywheel 219, carbs 178 / medium 259, exhaust 327 / low-restriction 463,
  standard engine 336, the engine upgrades 1,090 / 1,184 / 1,284 / 1,522, brakes 204 / 273 / 341,
  gearbox 491, springs 163 / 190 / 218, shocks 191 / 231, hood 110, quarter panels 150, sway bar 109,
  tyres 134 / 179. Buying subtracts the price from the account (4,388 -> 4,129 for the 259 carbs).
- The 356 1100 Cabriolet is shown as purchase price 11,000 (= catalogue price).

**Fixed:** a part sold back pays **80 %** of its buying price, not 100 %: the carbs bought for 259
were offered at 207 in the parts inventory (259 x 0.8 = 207.2). `career_shop::part_resale`, used by
`sell_part` and by the refund when a fitted part is replaced (the replacement refund is a **guess**
that it uses the same rate).

**Gaps seen, not built (screens or flow we lack or do differently):**
- *Sell car:* retail asks "Please enter the offer price for the car:" (pre-filled with the current
  value, e.g. 12,028) and lists the car on the market; the garage menu then shows "remove from
  market" in place of "sell", and the car stays owned. We sell instantly at the value. When a listed
  car sells was not in the footage.
- *Garage header:* "purchase price 11,000 / current value 9,890" for a worn car; our `car_value`
  rule (mileage -10 %, damage -1 %) gives 9,790 for the same inputs, so the footage's value uses
  something else (damage % from the repair table, **not decoded**).
- *Repair:* an "advanced" panel with condition %, repair cost and a fix tick per part (engine,
  brakes, transmission, suspension, shocks, weight reduction, sway bars, tyres, bodywork, overall);
  we have no repair screen (cars have no condition).
- *Paint shop:* trim package (none, package 1-7), trim 1 and trim 2 colour lists, a custom colour
  editor, a decal number prompt ("Please enter decal number", race number shown on the doors); we
  only offer exterior colour.
- *Parts shop:* the screen has a description popup per part (`parts description`, text from the part's
  string), a transaction list with `accept` / `clear list` / `remove item`, and a parts inventory
  section listing owned parts at their resale price with a `sell` button; ours marks owned parts
  with `*` in the one list.
- *Car setup:* the part-inventory table with INSTALLED / UNINSTALLED per part and `uninstall part`;
  the sliders (ride height, stiffness, travel, toe-in, brake balance, downforce, tyre pressure, gear
  ratios) with `test drive`, `save`, `load`, `default`.

### Race missions (2026-10-30, branch `claude/factory-missions-f3406f`)
Eight records are judged by the race, not by gates: 17, 19, 22, 28 (point-to-point roads: foothills, foothills,
canyon, foothills; the scene file only holds the start and the finish gate; one to three opponents, minimum
place 1) and 30-33 (Monaco circuits: monaco2, monaco1, monaco5, monaco3; no scene file). 30 and 32 have no
opponents but a time limit (220 s, 150 s) and the damage limit 5; 31 has four opponents, 33 one (Stephanie).
Our reading: the race is a quick race on the record's track with the record's opponent count (their cars and
driver ids are not matched yet; difficulty 1 is a **guess**), 3 laps for mission 30 (its briefing says so),
2 laps elsewhere (**guess**; roads run once). The place, the race time and the average zone damage at the
finish go to `Run::finish_race`, the same rules as the gate missions (`race_mission::race_mission`). The route
code (2, 3) of 30-33 is not decoded: we race the track forward. Headless: `RACE_DRIVE_FACTORY=1` also drives
each of the eight with the autopilot and judges it (`logs/factory_drive.txt`); the autopilot (about 80 km/h)
is far slower than the time limits, so it reports `Failed(Time)` on 30 and 32, which shows the judge works.
On screen (not driven by a person): mission 19 loads, races with the opponent and leaves normally.
The banner and cones of the gate missions were seen in `logs/mission_shot.png`
(`NFSPU_MISSION_SHOT=<seconds>`): the banner is centred at the top; cones are small orange cones on the pad.

## Repair and damage (2026-10-30, branch `claude/evo-shops-2`)
Layouts: `repair.lay` (quick fix) watches `OwnedCars.Car(<PART>DAMAGE)` / `(<PART>DAMAGECOST)` for engine, brakes, transmission,
suspension, shocks, tires, swaybars, bodywork, cosmetic plus `OVERALLDAMAGE` / `OVERALLDAMAGECOST` and the `QUICKFIX` button;
`advancedrepair.lay` has the shopping list (`Parts.RepairShoppingList`, `Parts.OwnedDamageRepairList`) with `fixpart`,
`replacepart`, `acceptlist`, `removeitem`, `clearlist` (**not built**: only the quick fix is).
**Verified by the footage table** (full-resolution frame of `2026-10-24 20-55-17.mp4`, '52 356 1500, catalogue price 12,700):
conditions 71 65 64 81 100 100 100 63 75 -> OVERALL 80 (the **mean of the nine**), costs 889 142 353 72 0 0 0 99 341 -> OVERALL 1,896
(the **sum**).
Built: `OwnedCar.damage` (the ten `car_damage` zones, saved as a 7th field of the `car =` line), `Career::component_damage`
(`Damage::components`), `overall_damage`, `repair_costs`, `quick_fix`, `keep_race_damage`; the car's value now takes the overall damage.
After a career race the player's zones are max-merged into the raced car; career races run with damage on.
**Guesses:** the cost of a part = car price x per-part share x damage (shares engine 24.1 %, brakes 3.2, transmission 7.7,
suspension 3.0, tires 2.1, bodywork 10.7 of the price at 100 % damage, calibrated on that single table and so only a fit;
shocks 2.0, sway bars 1.0, cosmetic 4.0 are invented); the zone masks of shocks, tires, sway bars, bodywork and cosmetic; a car
starts a race without its saved damage (only the race's own damage is kept); the garage's `current value` 9,890 for the 11,000
car still differs from our rule (9,790 for -10 % mileage, -1 % damage), so the value's damage step is **not** settled.
Not tested in the window. Still not built: sell-car offers (instant sale remains), paint trims/decals/custom colour, parts
inventory with resale list and transaction list, car setup sliders.

## Parts screen: inventory and descriptions (2026-10-30, `claude/evo-shops-2`)
Built: `Parts.AllParts` now lists the parts that fit the car, then an `INVENTORY` block with the fitted parts at their
resale price (80 % of the buying price, footage: 259 -> 207); the sell button only sells inventory rows (an available row does
nothing); the buy button only adds available rows to the shopping list (the transaction list, accept / clear / remove were
already there). `partsdesc` gets `Parts.AllParts` (the part's name) and `Parts.AllParts(DESC)`: the description string id is the
**name id + 500** (checked: standard carbs 5085 -> 5585, medium carbs 5086 -> 5586, lightened flywheel 5102 -> 5602, all the footage's texts).
Guess: the inventory is a block at the end of the one list (retail shows it as a section with its own header row after
`PARTS AVAILABLE FOR ...`); owned parts keep their `*` in the available block too.
Not built: paint trims (package, trim 1 / 2, custom colour, race number) and sell-car offers. Paint needs a per-car colour scheme
(`PaintScheme.stripe_package` plus four colours) carried from `OwnedCar` through the showcase and `RaceSetup` into
`car_model` (today only a section number or one custom body colour travels), and a door-number decal; the 3D preview in the
paint shop depends on the same path. Sell offers need evidence of when a listed car sells.

## Paint shop: package and trims (2026-10-30, `claude/evo-shops-3`)
`paintshop.lay` accessors: `OwnedCar.PackageNum` (combo), `OwnedCar.S1Colour` / `S2Colour` (trim 1 / 2 colour boxes), `OwnedCar.ExtColour`,
`OwnedCar.IntColour`, `CHANGEDECAL` (race number). Footage: the package combo lists none, package 1 to 7; picking one stripes the 3D car at once;
trim colours come from a swatch list with a `custom...` entry (the custom colour editor); the decal prompt "Please enter decal number" puts
the number on both doors.
Built: `car_model::PaintTweak` (package override and two trim colours) applied to the paint scheme before the bands are made, carried
`OwnedCar.tweak` (saved as the 8th field of the `car =` line) -> `View.showcase_tweak` -> `quick.showcase_tweak` -> garage preview
(`wanted_car`) and `RaceSetup.tweak` (`paint_package`, `trim1`, `trim2` lines, optional when read) -> the race car's `CarOptions`.
The paint shop's combos now pick the package and the trims (from the car's body colours), and the choice sticks on the car.
**Guesses:** combo entry `package N` is `[packageN]` of `packages.ini` (stripe index N - 1) and `none` is no stripes; trim 1 / 2 replace the scheme's
`color2` / `color3`; the shop shows `none` for a car that never picked a package even if its own scheme has stripes.
**Not built:** the custom colour editor, the interior colour, and the race-number decal (door numbers need text drawn on the car's texture).
Unit tests: `the_paint_shop_tweak_changes_package_and_trims`, `paint_tweaks_round_trip_and_may_be_absent`, the career save round trip.
Seen on screen: `NFSPU_MENU=paintshop NFSPU_CAREER=1 NFSPU_CAREER_DO="pick:OwnedCar.PackageNum=3;pick:OwnedCar.S1Colour=4" NFSPU_MENU_CAPTURE=1` (`logs/menu_paintshop.png`): the orange 356 wears package 3 stripes with a red trim 1 and the combos show `package 3` and the colour boxes. Not compared with retail pixel by pixel.

## Sell-car offers

Retail (FEdaTrack, run after each race for every owned car): a car flagged as listed (record +0x6a0)
gets an offer flag (+0x692) and an offer amount (+0x694). A roll against a chance read from age and
mileage tables (in the exe at 0x5b7c88..0x5b7dfc) decides: a hit offers the asking price (+0x68c), a
miss offers a random lower bid, clamped to 0..asking. Accepting removes the car and adds the offer to the
account; the garage shows "remove from market" for a listed car.

Ours (guess, not compared with retail): SELLCAR lists the car at its value; after each race a listed car
without an offer gets one (40% the full asking price, else 70-95% of it); SELLCAR with an offer pending
accepts it; DONTSELLCAR takes the car off. Saved as the 9th field of the `car =` line. The retail chance
tables and the owner-typed asking price (dialog 800) are not read yet. Not tested in game.
