# Damage, roll-overs, suspension: findings (WIP, to merge into physics.md / car.md)

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
