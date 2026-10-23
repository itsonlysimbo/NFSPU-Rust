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

## Avoiding traffic (ours)
Retail opponents pass traffic, but the avoidance code was not read. Ours (`ai::avoid`, guesses): a car within 11 m plus 2.2 s of closing speed ahead and within 2.4 m sideways is passed on the side with more road (3.2 m needed beside it); with no gap the driver brakes to arrive behind it at its speed. Oncoming cars close at both speeds, so they are seen further out.

## Not done
Horn sound, the speed ladder, decoded lane data, reaction to opponents beyond braking, police, traffic on junction
route choice (cars take the first route).
