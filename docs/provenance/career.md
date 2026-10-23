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
