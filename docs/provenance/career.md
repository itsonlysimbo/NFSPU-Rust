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
