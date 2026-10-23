# Road traffic

Evidence: the retail executable (Ghidra, read-only: `AIHigh_Traffic`, `AIState_NonActive/Normal/Purgatory/Idle`, the
car-list builder `0x40f160`) and the install's car catalogue. Our code: `crates/nfspu-game/src/traffic.rs`, wired in
`race_mode.rs` and `examples/race_drive.rs` (`RACE_DRIVE_TRAFFIC=0..3`).

## What retail does (decomp)
- Cars are sorted into lists when created. Traffic cars (flag 4/0x10) are separate from racers and police.
- Each traffic car is a small state machine: waiting (hidden, "purgatory"), then driving. A car that is switched off
  waits a random 1..N ticks, N from a table `25, 50, 75, 100, 200, 400` indexed by the number of traffic cars (x5 on
  one named track).
- **Appear**: it picks a random racer as reference and a start slice 60 to 65 slices ahead of or behind it (even odds).
  If that racer is faster than 30 m/s it uses the racer's own direction. The car's own direction along the road is
  chosen evenly (so half the traffic is oncoming).
- **Remove**: when the flat distance (larger axis + 0.25 x smaller) to its reference is over 400 m it is taken off; it
  first tries to attach to a nearer racer.
- **Speed**: base 16.5 m/s times a per-car factor (1.0, 0.9, 0.8, 0.7), at least 8.9 m/s and at most 13.4 below the base.
  Retail also has a ladder that varies the base with the reference's speed (values 22.2, 13.3, 8.9, 4.4, 2.2, 2.8 m/s were
  seen) whose branches the decompiler dropped; not used.
- **Lane**: one of four lane values from the vehicle's data; direction sign decides the side. Values not decoded.
- The track file holds a list of traffic vehicle definitions (collision mesh, mass, a type 0..12); the count of traffic
  cars comes from that list, not from a number I could find. The option `Track1.Traffic` is none/light/medium/heavy.
- Models: catalogue records of kind 1: `eas1`-`eas11`, `eden1`-`eden5` (16 models). They have no `.sim`, so physics
  falls back to `default.sim`.

## Ours
Same appear/remove rules and speeds; car counts per density are **guessed** (4, 8, 12); lanes are 25-70 % of the
narrower half-road (guess); braking for a car ahead in the lane, and a "honk" event (logged only, no sound) when
closing under 12 m ahead, are guesses. Traffic is full `Car` physics, so hits move both cars (`collide_cars`).
A traffic car that is far, stuck for 6 s, fallen or crashed is taken off and reappears after the wait.

## Numbers against retail (checked 2026-10-23)
`traffic.rs` matches every number above that the decomp gave: removal at 400 m flat distance, appear 60 + 0..5 slices
from the reference racer, wait table 25/50/75/100/200/400 ticks by car count, base 16.5 m/s, factors 1.0/0.9/0.8/0.7,
floor 8.9 m/s and at most 13.4 below base (unit tests). Not matched: the x5 wait on one named track (which track is not
noted), the speed ladder, and the car counts per density (no retail number found; ours 4/8/12, guess).

## Avoiding traffic (ours, guesses; 2026-10-23)
Retail's opponent avoidance was not read (no Ghidra in the cloud session), so this is our own rule, shared by the
opponents and the `race_drive` autopilot (`ai::avoid`):
- Each car ahead (on this stretch of road, within about 2.5 s of meeting) gets a place across the road, measured at its
  own slice so bends do not skew it.
- Places across the road (half-metre steps, the car's centre kept 2 m from each edge) are scored by how long they stay
  clear, with a 3.2 m passing gap; small moves are preferred, and the right on a tie. So slower cars are overtaken on the
  left only when that is clear, and oncoming cars are passed on the right.
- A car overlapping (2.4 m) the chosen place close ahead: brake to its speed, or stop for an oncoming one; one that only
  overlaps where the car is now: slow to 6 m/s while steering round it.
- Traffic lanes never come nearer than 1.4 m to the centre line; a traffic car with a racer coming at it in its half
  moves to the outer lane, and it brakes using the racer's speed along its own way (so it stops for a head-on racer).

Effect (`race_drive`, all 15 tracks, 2 laps, heavy traffic, traffic hits of impact 20+): seed 12345: 182 before,
7 after; seed 777: 187 before, 11 after. Before, autobahn gave up at seed 12345; after, ALL OK both seeds and with
traffic off. Alps, 7 expert opponents, heavy traffic: 45 traffic hits before, 7 after. Racer-to-racer hits with traffic
off (alps, monaco1, castle, seeds 1 and 2): 37 before, 17 after. Most of the old hits were nose-to-nose pushes with
oncoming traffic, both cars creeping forward.

## Not done
Horn sound, the speed ladder, decoded lane data, retail's own avoidance rules, police, traffic on junction
route choice (cars take the first route).
