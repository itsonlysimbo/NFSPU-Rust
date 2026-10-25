# Car physics (retail model)

What the retail game does when it drives a car, worked out from the decompiled `Porsche.exe` (Ghidra,
static reading plus exact disassembly where the decompiler lost x87 values) and the shipped data
files. Written as notes and tables; the Rust model (`crates/nfspu-game/src/car_sim.rs`) is our own
code built from these notes. "Verified" below means read directly from the executable or the data;
"guess" means inferred and not checked against the running game.

## 1. Files

| File | What |
|---|---|
| `GameData/Simulation/CarData/<sim>.sim` | one per car, 328 bytes (86 shipped incl. `default.sim`) |
| `GameData/Simulation/CarData/default.sim` | used when a car's own file is missing (verified: the loader tries `<sim>.sim`, then `default.sim`) |
| `GameData/Simulation/AICarData/<ais>.ais` | 304-byte AI tuning per model (not decoded here) |
| `FEData/Data/nfs5.car` | catalogue: the `.sim` stem is at record offset 0xb1, the AI-data name at 0x115, the locale file name at 0x4d |
| `FEData/Locale/<0x4d name>.loc` | showroom specs (string 7: 0-60 mph seconds, 8: top speed mph, 11: weight) |

Reader: `crates/nfspu-formats/src/carsim.rs`. Audit of every file: `cargo run -p nfspu-formats --example sim_audit` (log `logs/sim_audit.txt`).

### `.sim` layout (verified offsets; units from use in the code)

All little-endian, floats unless noted.

| Offset | Field | Units / notes |
|---|---|---|
| 0x00 | name | 64 bytes text, `1987 959 coupe 2.9L` |
| 0x40 | mass | kg (550..1620) |
| 0x44 | wheelbase | mm |
| 0x48 | u32 | 0 in all files, not used by the loader |
| 0x4c | u32 induction | 0 none, 1 turbo, 2 supercharger (a "Turbo"/"Supercharged" engine part sets it too) |
| 0x50 | u32 forward gears | the loader adds 2 (reverse and neutral slots) |
| 0x54 | u32 shift time | 32 Hz ticks with no drive during a gear change (4..8) |
| 0x58 | drive-force scale | percent |
| 0x5c | front drive share | percent (0 = rear drive; 959 30, 993 C4 and Turbo 40) |
| 0x60 | 8 ratios | reverse (negative), neutral (0), 1st..6th (0 = unused) |
| 0x80 | final drive | |
| 0x84, 0x88, 0x8c | tyre | width mm, aspect percent, rim inches; circumference = (rim x 25.4 + 2 x width x aspect / 100) x pi / 1000 m |
| 0x90 | top speed | mph (the loader converts with x 0.4464) |
| 0x94 | redline | rpm |
| 0x98 | engine braking | percent of the torque at the current rpm |
| 0x9c | 21 torque points | lb-ft at 0, 500, ... 10000 rpm (they match the showroom lb-ft figures) |
| 0xf0 | front weight | percent (the 959 is overridden to 46.75%) |
| 0xf4 | grip | lateral friction coefficient (1.1..2.75; race cars highest) |
| 0xf8 | weight-transfer height | percent |
| 0xfc | steering lock | 1/256 of a turn (16.5 = 23 degrees) |
| 0x100 | toe | steering units, 0 in all files |
| 0x104 | brakes | deceleration at full pedal, m/s^2 (6.2..14) |
| 0x108 | front brake share | percent |
| 0x10c, 0x110 | downforce front / rear | x 0.001: extra wheel load in m/s^2 per m/s of speed |
| 0x114, 0x118 | suspension front / rear | percent; used by the tumbling (airborne/roll-over) code, not decoded |
| 0x11c | transfer stiffness | divides weight transfer; also scales grip by (x + 9) / 10 at load |
| 0x120..0x12c | suspension | x 0.001: ride height (overwritten by the setup screen), travel, spring, damper; spring and damper set the body lean (section 7b) |
| 0x130 | lateral split | left/right weight-transfer split (1.0..1.1) |
| 0x134..0x144 | five values | mostly 1.0; 0x140 is scaled by the AI-data loader (AI value) |

The loader copies the file into a 0x1c4-byte block and appends derived values: tyre circumference,
1/redline, per gear "engine rpm per m/s" = ratio x final x 60 / circumference, its inverse, and
per gear "drive force per lb-ft" = rpm-per-m/s / mass x 0.1 x scale (0.1 stands in for 2 pi / 60),
plus the drag constant (below), 3.2 x wheelbase (the yaw lever, 3.2 standing in for pi) and 1/grip.

### Load-time rules (verified)

- Manual gearbox: drive force x 1.05 (the player record's "automatic" flag is 0).
- Tyre class 2 ("race") for cars whose AI-data name is GT1, GT2, GT3 or 935; otherwise class 0.
  Class 1 is set by a tyre upgrade part (and swapped with 0 in some wet/AI cases).
- Grip x (transfer stiffness + 9) / 10.
- Drag: deceleration = k v^3 with k = torque(min(top speed x rpm-per-m/s of top gear, redline))
  x drive-per-lb-ft of top gear / top speed^3. So the engine exactly balances drag at the file's top
  speed in top gear: **top speed is built into the data**.
- The automatic leaves out the top gear when the top speed is below the redline speed of the gear
  under it.
- Two cheat driver names (`SILSPD`, `ROLLEY`) multiply grip and drive; not reproduced.
- Upgrade parts (20 slots: engine, brakes, gearbox, suspension, tyres, aero...) and the car-setup
  sliders adjust many fields. Our model takes **no parts and every slider at its neutral point**
  (guess: neutral = gear offsets 0, downforce and brake-balance sliders 50, stiffness 1, tyre
  sliders 30 - the formulas are identity there; the real defaults are not decoded).

## 2. Clock (verified)

The race timer counts 64 Hz ticks (see `race.md`). Each tick runs a second task list every time and
the first task list on even ticks only:

| Rate | Order | Work |
|---|---|---|
| 32 Hz | 1 | controls (keyboard / AI) |
| 32 Hz | 2 | gear requests |
| 32 Hz | 3 | forces: engine, brakes, tyres; velocity += force / 32, yaw rate += ... |
| 32 Hz | 4 | ground and suspension |
| 32 Hz | 5 | walls, then car against car |
| 64 Hz | 6 | position += velocity / 64 |
| 64 Hz | 7 | orientation turned by angular velocity / 64 (angular velocity in turns per second) |

Angles in the physics are in turns (sin(x * 2 pi)); steering in 1/256 turn.

## 3. Controls (verified)

- Pedals and steering arrive as bytes: throttle and brake 0..254 (divided by 252 and capped at 1),
  steering about -127..127. Full keyboard throttle is 1.0.
- Keys read as 0 or 255 at once (no ramp in the input layer); steering = (right - left) / 4, doubled
  in the car to about +-126.
- The steering moves at most 12 units per 32 Hz tick toward the input when the player record's
  **RAMPS** field is set (18 with the TINYCAR cheat), else jumps (verified; the field names come from
  the exe's config table). The player's install has `car[0].ramps = 1` in `fe.txt`, so retail ramps
  the steering; we always do. **RAMPG** and **RAMPB** (gas and brake ramps) are only stored with
  replays: the car code never reads them, so retail throttle and brake are instant on keys.
- Player record (0x580 bytes from 0x65808c) fields used by the physics: 0x0c TRANSMISSION (1 =
  automatic), 0x14 RAMPS, 0x20 TRACTION, 0x24 ABS, 0xf0.. FINALRATIO and the gear ratios, 0x114/0x118
  front/rear DOWNFORCE, 0x11c BRAKEBIAS, 0x120 RIDEHEIGHT, 0x124 SUSPENSIONTRAVEL, 0x128 STEERINGTOE,
  0x134 SHOCKRATE, 0x13c/0x140 front/rear TIREPRESSURE, 0x518 TINYCAR (cheat).
- Game globals: 0x6573f8 is **RALLY** mode (more front grip, arcade friction circle, steadier slides;
  off in normal races and in ours), 0x6573f4 SKILL (read by the AI only, not by the physics).
- Camber pulls the steering downhill: round(9.8 x right-axis height x 8), capped at +-64 units.
- Automatic: at race start (tick 448) first gear is requested. Holding the brake at a standstill
  (speed under 0.2 m/s, brake over half, no throttle) selects reverse and then the pedals swap
  (brake drives backwards); throttle at a standstill in reverse selects first again.
- Manual: a gear-up / gear-down press moves one slot; a downshift is refused while over the redline.

## 4. Engine and gearbox (verified, 32 Hz)

- Rev limiter: entering the band redline..redline+250 sets rpm to redline + 50 and cuts the
  throttle for a random number of ticks: 2-3 in neutral, 0-1 + 3 x gear slot in gear (6-22 ticks).
- Neutral, mid-shift, or rear wheels in the air: the engine revs freely toward (redline + 250, or +100
  in gear) x throttle at +250 rpm/tick and falls at 200/tick; during an upshift the rpm drops
  100/tick (200 from 3rd up), during a downshift it blips up 100-250/tick by target gear. No drive.
- In gear: wheel rpm = rpm-per-m/s x forward speed; full force = torque(rpm) x drive-per-lb-ft.
  - Clutch slipping (engine more than 250 rpm above the wheels, or rolling against the gear): full
    force regardless of the pedal, and the engine is pulled toward the wheels by at most a grip-based
    amount per tick; a big gap (more than redline / 8) marks wheelspin for the tyres.
  - Wheels below the pedal's target: force = full x throttle, rpm follows the wheels (200/tick
    max). Within 125 rpm of a part-throttle target the car holds its speed (force exactly cancels drag).
  - Wheels above the target: engine braking = engine-brake share x torque (x4 while the limiter cuts).
- rpm is capped at 1.25 x redline.
- Air drag k v^3 is taken off the engine force; it then reaches the road through the tyres.
- Slipstream: drag is reduced behind another car (up to 50%, cars within a third of a car's speed in
  metres and well aligned). **Not in our model yet.**
- Automatic shifting: up when rpm passes 90-100% of the redline (random each check), down when the
  lower gear would stay under 80% of the redline (at low throttle only if not sliding).

## 5. Wheels and tyres (verified, 32 Hz)

Forces are accelerations (m/s^2). Wheel 0/1 front, 2/3 rear.

- Load (the grip limit) = surface factor x weight share x 9.8 x up-axis height x grip / 2 per wheel.
  Surface factors: a table by surface type and tyre class; dry asphalt 1.0 (road), 0.9, 1.1 (race);
  gravel/grass 0.6-0.85; a wet table swaps columns. **Our ground has no surface types yet: always
  asphalt.**
- Weight transfer from the previous tick's accelerations: a x height x up x grip x 0.2 / stiffness,
  front to rear (reduced while sliding under throttle) and to the outside wheels (split by 0x130).
- Downforce above 5 m/s: load + downforce x speed.
- Each wheel asks for the force that would cancel its slide in two ticks: -16 x velocity, using the
  axle's sideways speed (yaw rate x 3.2 x wheelbase per turn/s, stretched by speed: x(v+70)/80 above
  10 m/s, x(v+30)/40 below) and the car's forward speed. Front wheels turn these into the wheel frame
  by the steering angle.
- Drive/brake per wheel: engine force split front/rear by the drive share and between wheels by load;
  brakes = brake decel x pedal (capped at what stops the car this tick), split by the brake share.
  A decelerating force is capped at the cancel force (no reversing by braking).
- Locking (sliding at (1 - 1/k) of the limit, k = 8 road, 6 race): handbrake on the rear wheels;
  without ABS when the brake pedal is over 235/252 and the force exceeds the limit at speeds under
  about 2 x (16 - load) m/s (not at high speed!); with ABS a pulse every fourth tick below 25 m/s.
  Otherwise the brake force is clamped to the limit.
- Sideways force: slip angle = atan2(sideways, -0.5 x lengthways) in 1/256 turn. Rear tyres give
  0.5..1.0 x load between 1 and 2 units (they saturate fast); front tyres give slip/12 x load up to 16
  units. Never more than the cancel force.
- Friction circle: max(0.9 x lengthways, sideways) + 0.25 x the smaller must stay under the load;
  above it both are scaled to load - min(excess, load) / k (k = 4 for a spinning rear wheel).
  Traction control (rear wheels, slip angle ratio under 0.15) trims to the limit without sliding.
- The lengthways sum is divided by grip again: **lengthways grip is about 1 g whatever the tyre; the
  grip value only widens the sideways limit.**
- Body: velocity += (sum of forces) / 32; yaw rate += ((front sideways - rear sideways) - (wheels
  0+2 lengthways - wheels 1+3 lengthways)) / (3.2 wheelbase) / 48 turns/s per tick, capped at 0.75
  turns/s (sign checked in the disassembly at 0x49f9a0). Wheels 0+2 are the ones that gain load when
  the sideways force points along the car's right axis, i.e. the outside wheels. Drive is split by
  load, so under power the outside wheels push harder and the yaw sum turns the car **out** of the
  corner (power steadies it); braking or lifting (negative forces, the outside wheels braking harder
  once the inside ones reach their grip) turns it **in**. That is the opposite of a real car's
  left/right effect, and three times as strong (same lever as the sideways forces, forces in the
  x-grip scale). Our first port used the physical sign, which made powerful cars spin under power
  (the user's 2026-10-19 playtest); fixed to retail's.
- Slopes: -4.9 x forward-axis height lengthways (unless braking), -19.6 x right-axis height sideways
  (only when sliding over 1 m/s or tilted).
- Extras: race-grip cars (grip over 2.5) lose speed in tight turns off throttle; in neutral the car
  coasts with x0.998 (x0.99 slow or steering) per tick; handbrake under 1 m/s halves the speed.

## 6. Walls (verified)

The track edge is the boundary of the drivable polygons. Eight corners of the car box are tested
against it at 32 Hz (from the previous position). On a hit with inward normal n:

- the car is pushed out by 1.05 x the overlap;
- the speed into the wall is removed and 10% of it comes back (v += n x into x 1.1); the speed
  along the wall is kept, apart from x0.998;
- if the hit is slower than 15 m/s, or the car slower than 20 m/s forward, the yaw rate is **set** to
  turn the car along the wall: 0.25 x min(|n . forward|, |n . right|) turns/s, half that when the
  rear hits;
- the sound is a crash if the speed into the wall is at least 1/8 of the car's speed, else a scrape;
  impacts over 20 count as damage.

Our model applies the same response to the wall triangles of `collision.rs` (two axle circles,
every 64 Hz tick rather than 8 corners at 32 Hz).

## 7. Car against car (verified)

Bounding-box test, then up to four rounds of: impulse along the contact normal with masses equal to
the box volume / 2 (the `.sim` mass is not used) and inertia 1/(6 m); the impulse is half the
perfectly inelastic one (x1.25 for hits of 80 and over); yaw kicks x1.5 (x3 for big hits); walls are
re-checked between rounds. Ours: one round, planar, then the boxes are separated by mass.

## 7b. Gravity, landings and body lean (verified, 32 Hz)

The vertical pass runs once per force tick for the player (the level-of-detail tables give rate 32
and one tick).

- **Gravity** applies only while the body is more than 5 mm above the road: 9.8 m/s^2 times a
  factor. The factor is 1.16, plus, while the car is upright and faster than 5 m/s,
  8 x rear downforce (`.sim` 0x110 x 0.001) x forward speed x a wing-damage term (1 for an intact
  car; damage is not modelled). With the usual rear downforce of 1.0-1.1 that is 1.16 at a crawl,
  about 1.5 at 140 km/h and 1.7 at 220 km/h: a fast car comes down noticeably harder than g.
- **On the road** the body takes the road's vertical speed (the change of the four wheels' average
  contact height). When both the car and the road are moving up, the larger wins, so a car
  leaving a ramp keeps its upward speed.
- **Hard landing**: when the road comes up at the car more than 6 m/s faster than the car moves,
  the car bounces: vertical speed = road speed + restitution x 0.4 x that difference. Restitution
  comes from a per-surface table (asphalt 0.7), so on asphalt the bounce keeps 28% of the impact.
- **Body lean**, per tick, from the last tick's accelerations: lengthways input = spring (0x128)
  x acceleration x (0.175 braking, 0.125 under power) + 0.8 x last value; sideways input = damper
  (0x12c) x sideways acceleration x 0.25 + 0.8 x last value. The pitch and roll rates add these to
  the height differences of the wheels (/8), are clamped by the setup's shock rates and settle with
  a 0.75 x 0.925 decay (pitch); the roll input is scaled by 1 - throttle / 1275 (throttle 0..255).
  Each end is limited by the suspension travel (0.1 m). In steady state that is a lean of about
  8 x the input as a height difference over the wheelbase (pitch) or the track (roll): about 5
  degrees nose down under hard braking (the travel limit), 1-2 degrees of squat and up to about
  7.5 degrees of roll at 1 g (again the travel limit).

Ours: gravity, the 5 mm gap, the hard-landing bounce (asphalt restitution everywhere) and the lean
inputs are retail's. The road speed under a landing car is taken from the road's slope along its
path. The lean is drawn on the body only (it does not move weight or change grip; retail's lean
feeds back into its suspension) and its directions are physical: retail's signs are **not
resolved**, and the 1.5 m track width is a guess. The camera follows the road slope, not the lean.

## 8. Not in our model (yet)

- The four-point suspension against the road slices, tumbling and jumps with angular motion: we
  keep the rigid ground follower with retail's gravity and landings (section 7b).
- Damage (all wear factors are 0), engine misfire, slipstream, surface types other than asphalt,
  upgrade parts,
  setup sliders, RALLY mode and the TINYCAR cheat.
- Locked front wheels: retail does not turn their force back out of the wheel frame (it skips that
  step); ours does.

## 9. Our own rules (not retail)

| Rule | Why |
|---|---|
| Outside a race the automatic puts first gear in when the throttle is pressed | retail only does it at the race start (tick 448) |
| The wall swing-round (section 6) only for hits over 1 m/s | our walls are mesh faces; pressing into a corner of two of them locked the steering |
| Walls are checked every 64 Hz tick with two axle circles | we have wall triangles, not retail's road-edge polygons |
| The rigid ground follower carries vertical speed along the road, with sudden rises capped at 0.4 x speed + 1 m/s | stands in for retail's four-point suspension; the cap keeps seams between track pieces from launching the car |
| Wet road: the wet asphalt grip row (weather tables) unless a surface is 3.5 m or more above the car | retail's rule; we probe our ground mesh for the cover |
| Car against car: one impulse round, boxes separated by mass | retail re-checks walls up to four times |
| Speed is the true ground speed | retail uses max + quarter of min of the x/z components |

## 10. Checks against retail

`cargo run -p nfspu-game --example physics_check` (log `logs/physics_check.txt`): flat dry asphalt,
two seconds revving on the grid then full throttle (as in a race), setup sliders neutral. The
showroom numbers are the car's `.loc` specs, real-world figures that the game shows; they are **not**
measurements of the retail game.

| Car | 0-60 mph auto / manual (s) | showroom | top speed (mph) | showroom | 100-0 km/h ABS | 50 m circle |
|---|---|---|---|---|---|---|
| '50 356 1100 | 20.4 / 19.4 | 23.5 | 87.4 | 87 | 42.8 m | 80 km/h, 1.00 g |
| '89 964 Carrera 4 | 6.95 / 6.58 | 5.5 | 161.8 | 162 | 33.6 m | 100 km/h, 1.51 g |
| '93 964 Turbo | 5.20 / 5.05 | 4.7 | 174.8 | 174 | 31.2 m | 90 km/h, 1.21 g |
| '95 993 Carrera 4 | 6.36 / 5.89 | 5.4 | 167.7 | 168 | 33.8 m | 105 km/h, 1.64 g |
| '95 993 Turbo | 4.30 / 4.14 | 4.5 | 180.7 | 180 | 33.5 m | 95 km/h, 1.38 g |
| '87 959 | 3.64 / 3.95 | 3.9 | 194.7 | 195 | 35.0 m | 100 km/h, 1.61 g |
| '97 911 GT2 race | 4.02 / 4.02 | 4.4 | 183.7 | 183 | 31.3 m | 115 km/h, 2.42 g |
| '73 911 Carrera RS 2.7 | 5.92 / 5.77 | 5.8 | 149.8 | 152 | 36.6 m | 80 km/h, 0.94 g |
| '97 Boxster | 7.61 / 7.33 | 6.9 | 154.8 | 149 | 32.6 m | 100 km/h, 1.61 g |
| '56 550 A Spyder | 5.45 / 5.17 | 10.0 | 149.8 | 149 | 36.0 m | 80 km/h, 0.92 g |

- Top speed is built into the data (drag constant), so it matches within 1% (Boxster +4%: its
  automatic leaves out the top gear).
- 0-60 lands within about 10% of the showroom for most cars (manual: 7 of 10 within 10%). The 964s
  are 20-26% slow and the 550 is twice as fast as its showroom 10.0 s (a 550 kg race car with a 150
  mph top speed in the data). These follow from the data through the decoded formulas; whether the
  retail game shows the same is **not tested** (the reference video plays too small in the browser
  to read the speedometer, and the user's two recordings are menu captures).
- Retail drive force uses 0.1 where 2 pi / 60 x 1.356 (lb-ft to Nm) = 0.142 would be physical, so
  cars are about 30% weaker than their torque figures unless the per-car drive scale makes up for
  it (the 959's is 120%).
- `race_drive` (scripted autopilot, all 15 tracks): finishes everything with the made-up car, the
  '50 356, the 993 Turbo and the 959 (`RACE_DRIVE_CAR=<id>`).

### Handling (2026-10-19)

`cargo run -p nfspu-game --example handling_check` (log `logs/handling_check.txt`; before/after in
`logs/handling_before.txt`, `logs/handling_after.txt`). Keyboard inputs through `Car::step`:
step steer and lane changes at 100 and 150 km/h on flat ground, then a keyboard-style driver (full
lock taps toward the road ahead, no yaw damping, full throttle to a corner speed, braking while
still steering) for 90 s on alps, autobahn, canyon, coastal, forest and monaco1. Spin = slip angle
over 35 degrees above 10 m/s (or yaw rate over 2.5 rad/s on the flat runs).

| Car | track spins before | after | flat spins before | after |
|---|---|---|---|---|
| '50 356 1100 | 1 | 0 | 0 | 6 (five at 150 km/h, above its 140 km/h top speed so off throttle; one at 100 km/h with 2 s full lock then lift) |
| '94 993 Carrera | 1 | 15 | 0 | 2 (2 s of full lock, then lift) |
| '87 959 | 110 | 27 | 0 | 0 |
| total | 112 | 42 | 0 | 8 |

Before = the first port (physical yaw sign); after = retail's sign. Power-on spins (the 959's 110,
first and second gear, full throttle, full lock) are mostly gone; what remains is braking or lifting
with full lock, which retail's yaw sum rotates into the corner. The scripted driver brakes while
steering at full lock, which a player braking in a straight line avoids. Whether retail's cars feel
the same is **not tested** against the running game. race_drive still finishes all 15 tracks with
the made-up car, the '50 356, the 993 Turbo and the 959 (3, 1 and 0 spins), and with 7 AI opponents.

### Gravity (2026-10-20)

`handling_check`, '50 356 unless named, before and after section 7b's gravity, landing and lean
rules:

| Test | Before | After |
|---|---|---|
| Hill 100 km/h, time airborne | 0 s | 0.62 s |
| Hill 150 km/h, highest point | 1.65 m | 1.29 m |
| Hill 150 km/h, landing speed | 10.3 m/s | 6.2 m/s |
| Crest 100 km/h, highest point | 2.92 m | 2.43 m |
| Crest 150 km/h, highest point | 4.61 m | 4.50 m |
| Kicker, highest point (two speeds) | 2.67 / 4.02 m | 2.45 / 3.44 m |
| 993 crest 150 km/h | 6.67 m | 5.01 m |
| 959 crest 150 km/h | 6.69 m | 4.92 m |
| Flat spins (scripted) | 8 | 8 |
| Track spins (race_drive) | 42 | 46 (noise) |

Cars now fly lower and land softer, from downforce pressing harder with speed. Guesses: the
upright threshold (0.5), a 1.5 m track width for sideways lean, lean directions, wing damage
ignored, road speed at landing taken from the slope, asphalt bounce everywhere. Whether this matches
the retail game in flight is **not tested**. race_drive (generic, car 213, monaco1 with 7 AI) is
ALL OK after merging master.

## 10. Damage, roll-overs and the crash reset (decomp, 2026-10-22)

Verified in the decomp (read-only), our own words:
- Car keeps 10 damage floats (zones). Hit -> zone from hit direction in car frame (thresholds 0.1):
  ring 0..7 (front-right 0, front 1, front-left 2, left 3, rear-left 4, rear 5, rear-right 6, right 7;
  front/left signs are our guess), zone 8 for mostly vertical hits. Hits <= 10 ignored.
- Damage = strength x 0.5 (x1.5 in one other mode), max 100, never lowered; the hit zone's two
  ring neighbours rise to the average of the zones either side of them. Only with the damage option on.
- Effects: torque x (1 - engine/300) x (1 - gearbox x 0.005); brakes x (1 - b x 0.005);
  downforce x (1 - wing x 0.005); steering pull (z2 - z0)/4 units; front wheel angle x (1 - corner/512);
  suspension damage adds random steering wobble (x0.5, scaled below 15 m/s). Part zone masks come
  from parts data not read (our masks are guesses).
- Hit kicks body angular rates: k = min(s/32, 0.33) small, min(s/32, 2) x 1.2 for s > 80.
- s > 80: crash timer 192 ticks (-10/tick airborne); upright (up > 0.75) cancels it unless s > 120;
  at 0 the car is put back on the road at its current node (damage kept) unless upright, slow and grounded.
- Other resets: out of bounds (35 m past road edge), speed > 115, stuck 320 ticks.
- Visual damage: car `<name>D.fsh` holds stamps named by zone ("0000".."0008"); header gives page
  (0 body, 1 twin = other side), x, y on the 256 page. Stamp drawn alpha = zone x 3.5 (max 255)
  over the page (or opaque above a threshold). tpg `[damagezones]` gives zone box sizes.

Ours (`car_damage.rs`, `car.rs`): the zones, spreading, strength scale (walls: speed into the wall
in m/s; cars: the impulse strength), the mechanical effects, the hit kicks and the crash timer are
retail's. Guesses: which zone is front/left, the parts' zone masks, the tumble model (a spring to
the nearest resting angle: wheels, side or roof; balance points 1.35 / 1.06 rad), sliding friction
on the body (6 m/s^2), and that a car resting off its wheels starts the crash timer by itself.
Wall hits count only the speed into the wall, so a big hit (> 80) needs a very fast head-on crash.

### Suspension (ours, retail's four-point suspension not decoded)
Each corner follows the road under its wheel within the 0.1 m travel on a spring (90 /s^2, damper
8 /s, about 1.5 Hz). A landing pushes the corners down by half the landing speed. The body is drawn
on the corners (heave, pitch, roll); the wheels stay on the road. Driving is unchanged (rigid follower).

## 7c. Body lean against the 2026-10-23 footage (356 1100 Cabriolet, chase view)

Measured by eye on 1080p frames (rear chase view, about 1 s apart); angles are rough (+-3 degrees).
- Straights and gentle bends: 0-3 degrees of roll.
- Hard corners: the tail's number plate and bumper line tilt 6-10 degrees, with the body leaning
  outward over planted wheels. The old `LEAN_LOOK` 0.6 gave at most about 4.5, so it is now 1.0
  (the decoded lean, drawn as is). Per-car differences come from each `.sim`'s spring/damper.
- Not done: the GT1 lean (its chase frames show a stiff car with little visible roll, not measured),
  and a side-by-side of our 356 at the same corners.

### Still to measure from the footage (videos are local only: `C:\Users\CB\Videos\2026-10-23 11-00-02.mp4` race, `11-10-04.mp4` 356 lean)
- 356 (11-10-04, chase from ~26 s to ~42 s, then cockpit around 22 s): see 7c. Tightest corner at ~39-40 s shows ~10 degrees roll plus the car yawing to the camera.
- GT1 (race video, chase views from ~2:20 on): roll/pitch per corner and brake moment: **not measured**.
- Then compare our `lean_check` output (logs/lean_check.txt) and a capture at the same corners.

### 7c (measured 2026-10-23, GT1, chase, race video 222-238 s; angles by eye, +-2 degrees)
- Straight and gentle bends: wing line level (0-1 degree).
- Medium bends at about 200 km/h (wing tilt): 2-3 degrees of roll.
- Tightest bend seen (about 235 s, car yawed to the camera): about 4-5 degrees, outward lean.
- No visible pitch change on the chase view; brake-moment pitch **not measured** (no brake moments sampled).
- So the GT1 rolls about half as much as the 356 (6-10), as a stiffer car should; our per-car
  spring/damper values already give that ordering (compare logs/lean_check.txt: **not run**).

### 7c check of our per-car lean against both cars (2026-10-23, `lean_check`, `logs/lean_check.txt` GT1, `logs/lean_check_3cars.txt` the others)
Lean already differs per car because the decoded spring/damper come from each `.sim`; no extra
per-car scale is needed or known to exist in retail. Drawn roll in steady corners on flat asphalt:
- 356 1100 Coupe (spring 0.0050, damper 0.0035): 5.7-7.6 degrees (footage 6-10): matches.
- GT1 (spring 0.0010, damper 0.0003): 1.0-2.6 degrees (footage: 2-3 medium, 4-5 tightest): medium
  bends match, the tightest is about 2 degrees under (our flat-road lateral g of 2.7 is higher than
  a real bend, so a fitted scale would be a guess; `LEAN_LOOK` stays 1.0).
- GT1 braking pitch from 150 km/h: about -1.6 degrees (footage: not measurable). `lean_check` now
  takes car names (`lean_check gt1`).

## 11. Collision audit and stuck recovery (2026-10-24)

`collision_audit` sweeps every road slice across its width plus 3 m shoulders and uses the car's own support tiers. Result: no real holes on the road; leftovers are slivers at the nominal road edge beside void (4 spots), slice/mesh offsets on unused side stubs, and open edges (skidpad, forest, autobahn) where a car can drive off into void. Steps over 0.6 m near edges are kerbs and rails. Two industrial spots block the whole road width for the wall test (not looked into). Instead of patching per spot, the player now gets retail's reset rules (`stuck.rs`): stuck 320 ticks, 35 m past the edge, over 115 m/s, plus our own fell 30 m or airborne 4 s. The out-of-bounds test also checks the nearest slice anywhere, because the tracked slice can sit on a side stub. `race_drive` fails if any of these fire. Not tested in the game window.

`edge_walls.rs` (2026-10-25): where the audit's OPEN test fails at a slice, a 4.5 m tall wall strip is generated 0.4 m outside the road edge and added to the track's walls in `load_ground`. OPEN runs dropped from about 8000 to 430 over 15 tracks; the rest sit between slices or beside short road stubs. Not tested in the game.
