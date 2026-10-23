# Playtest list

Things only the user can judge (by eye or ear). Agents add items as work merges; the user tests
when they can and notes results here or in a recording. Run the game with `.\run_menu.bat GameSetup`.

## Waiting for a playtest

| Since | What to check | What you should see or hear | Merged in |
|---|---|---|---|
| 2026-10-22 | Console during a race | No "Use-after-free" lines | `a415ff7` |
| 2026-10-22 | Sun flare on Côte d'Azur | Flare hidden or faded behind rocks and tunnels | `2ec0561` |
| 2026-10-22 | Handling and gravity | Retail-like weight; hard landings; body leans in corners; less grip in the wet | `f6ecc30` |
| 2026-10-22 | Menus | Controls slide and fade in; Race/Back/Options/Exit animate on hover; dropdown clicks hit only the list | `70cf5ea` |
| 2026-10-22 | Cars | Smooth shading; every paint for the model plus a custom colour | `cf2d8fe` |
| 2026-10-22 | Options > Audio | Five sliders drag; music changes live; settings kept after restart | `6259450` |
| 2026-10-22 | Race sound | Engine pitch with revs and gears, opponents passing, countdown voice, skids, wall hits, wind, road noise, track ambience; levels follow the sliders | `031dcef` |
| 2026-10-22 | Body lean and wheels | In a hard corner the body rolls but the wheels stay flat on the road, like retail; the lean should not look excessive | `opencode/lean-cockpit` |
| 2026-10-22 | Cockpit view (C) | The driver's arms/hands are visible on the steering wheel, and the wheel turns with the steering; the arms are the model's low-poly ones, judge how they look | `opencode/lean-cockpit` |
| 2026-10-22 | Cote d'Azur sea | The water/sea shows along the coast (sunset over rippled water); compare its look and extent with retail | `opencode/lean-cockpit` |
| 2026-10-22 | Water and cockpit wheel fixes | Côte d'Azur: the sea is a translucent sunset-tinted sheet over the coast (the water textures were being alpha-tested, discarding most of the sea; now blended). Cockpit (C): tap left/right and the steering wheel sweeps round smoothly (full lock in about 0.6 s, about 90 degrees) instead of snapping; the hands ride the rim. Retail's wheel angle and hand animation are not traced, so judge the look | `claude/water-cockpit-213623` |
| 2026-10-22 | Damage, roll-overs, suspension | Hitting walls/cars leaves dents and scratches on the side hit; heavy damage pulls the steering and loses power; landings make the body dip and bounce; a car on its side or roof is put back on the road after about 3 s (in the air much sooner) | `claude/damage-6135c5` |
| 2026-10-22 | Car reflections | A soft white sheen on the roof and wings that moves as the camera moves (stronger by day than at night); on lamp sections (e.g. Alps village around slices 414-433, Farmland, Forest) blue or yellow glows slide over the paint as you pass. Compare strength and colour with retail | `claude/car-reflections-9ce0ac` |

| 2026-10-22 | Effects slider in menus; all sliders in a race | Menu clicks get quieter with Effects; in a race Engine, Effects, Speech and Ambient each change their sounds | master |
| 2026-10-22 | Console during a race (again) | No "Use-after-free" line at all | master |

## Results
Add a line per playtest: date, recording path, what was wrong.

### 2026-10-22 (user notes)
- Console: one "Use-after-free" error left per race (was thousands). **Open.**
- Sun flare: hidden behind rocks and tunnels. **Pass.**
- Gravity: feels decent. Wet roads not tested yet.
- Menus: slide/fade, hover and dropdown all pass. Missing: a small bright light that travels around the button outlines (user can supply retail footage).
- Cars: shading looks good. Next: reflective surfaces (paint and chrome environment reflections).
- Options > Audio: sliders drag, music changes live, settings persist. **Bug:** every slider except music has no audible effect.
- Race sound: close, needs fine-tuning against retail.
- Body lean: better, not yet retail.
- Cockpit: hand position on the wheel needs work; the steering wheel turns in 90-degree steps instead of smoothly.
- Côte d'Azur: no water in the sea. **Fail.**

## Results screen (claude/results-screen-a9f5cc)
- Finish a quick race with opponents: about 2 s after the line a 500x325 "Race Summary" panel
  opens with eight places, NAME / CAR / BEST LAP / TIME, your row in amber. Compare its look,
  position and timing with retail (panel colours, position and delay are guesses).
- Opponents still racing show dashes and fill in as they finish. Space, Enter or Esc return to the menu.
- Retail takes control away at the checkered line and the car rolls on; ours does not yet. Compare the panel with your 2026-10-22 recording (gold tint is a guess).

- **Opponent routes at junctions** (castle, industrial, autobahn, canyon, farmland, alps): on beginner, about half the opponents take side roads; on expert they keep to the main road. Do they look like retail, and do they drive the side roads cleanly?
## Pause menu (branch `claude/pause-menu-22e199`)
- In a quick race press Esc (or P): the race stops and a "Pause Menu" panel shows continue /
  race again / quit…. Engine, ambience and the race clock should all stop; compare the panel with
  retail's (colours and button look are guesses).
- Up/Down or the mouse pick a button; Enter, Space or a click press it. Esc goes back.
- continue: the race carries on from where it stopped, sounds come back.
- race again: the loading screen, then the same race from the grid.
- quit…: "Are you sure you want to exit?" with exit to game menu / exit to windows / cancel.
- Does retail's pause camera circle the car? Ours stays still.
# Playtest list

Things only the user can judge (by eye or ear). Agents add items as work merges; the user tests
when they can and notes results here or in a recording. Run the game with `.\run_menu.bat GameSetup`.

## Waiting for a playtest

| Since | What to check | What you should see or hear | Merged in |
|---|---|---|---|
| 2026-10-22 | Console during a race | No "Use-after-free" lines | `a415ff7` |
| 2026-10-22 | Sun flare on Côte d'Azur | Flare hidden or faded behind rocks and tunnels | `2ec0561` |
| 2026-10-22 | Handling and gravity | Retail-like weight; hard landings; body leans in corners; less grip in the wet | `f6ecc30` |
| 2026-10-22 | Menus | Controls slide and fade in; Race/Back/Options/Exit animate on hover; dropdown clicks hit only the list | `70cf5ea` |
| 2026-10-22 | Cars | Smooth shading; every paint for the model plus a custom colour | `cf2d8fe` |
| 2026-10-22 | Options > Audio | Five sliders drag; music changes live; settings kept after restart | `6259450` |
| 2026-10-22 | Race sound | Engine pitch with revs and gears, opponents passing, countdown voice, skids, wall hits, wind, road noise, track ambience; levels follow the sliders | `031dcef` |
| 2026-10-22 | Body lean and wheels | In a hard corner the body rolls but the wheels stay flat on the road, like retail; the lean should not look excessive | `opencode/lean-cockpit` |
| 2026-10-22 | Cockpit view (C) | The driver's arms/hands are visible on the steering wheel, and the wheel turns with the steering; the arms are the model's low-poly ones, judge how they look | `opencode/lean-cockpit` |
| 2026-10-22 | Cote d'Azur sea | The water/sea shows along the coast (sunset over rippled water); compare its look and extent with retail | `opencode/lean-cockpit` |
| 2026-10-22 | Water and cockpit wheel fixes | Côte d'Azur: the sea is a translucent sunset-tinted sheet over the coast (the water textures were being alpha-tested, discarding most of the sea; now blended). Cockpit (C): tap left/right and the steering wheel sweeps round smoothly (full lock in about 0.6 s, about 90 degrees) instead of snapping; the hands ride the rim. Retail's wheel angle and hand animation are not traced, so judge the look | `claude/water-cockpit-213623` |

| 2026-10-22 | Cockpit arms and hands | In the cockpit (C), turn the wheel left and right: the forearms now stretch from the fixed shoulders to the hands, so the arms stay attached to the hands as the wheel turns (before, they stayed still and the hands left them). Check where the hands sit on the rim (retail's grip and any hand-over-hand motion are not traced, so the hands stay where the model authored them), and whether the arm at full right lock looks too stretched | `claude/cockpit-hands-afa0a8` |
| 2026-10-22 | Damage, roll-overs, suspension | Hitting walls/cars leaves dents and scratches on the side hit; heavy damage pulls the steering and loses power; landings make the body dip and bounce; a car on its side or roof is put back on the road after about 3 s (in the air much sooner) | `claude/damage-6135c5` |

| 2026-10-22 | Effects slider in menus; all sliders in a race | Menu clicks get quieter with Effects; in a race Engine, Effects, Speech and Ambient each change their sounds | master |
| 2026-10-22 | Console during a race (again) | No "Use-after-free" line at all | master |

## Results
Add a line per playtest: date, recording path, what was wrong.

### 2026-10-22 (user notes)
- Console: one "Use-after-free" error left per race (was thousands). **Open.**
- Sun flare: hidden behind rocks and tunnels. **Pass.**
- Gravity: feels decent. Wet roads not tested yet.
- Menus: slide/fade, hover and dropdown all pass. Missing: a small bright light that travels around the button outlines (user can supply retail footage).
- Cars: shading looks good. Next: reflective surfaces (paint and chrome environment reflections).
- Options > Audio: sliders drag, music changes live, settings persist. **Bug:** every slider except music has no audible effect.
- Race sound: close, needs fine-tuning against retail.
- Body lean: better, not yet retail.
- Cockpit: hand position on the wheel needs work; the steering wheel turns in 90-degree steps instead of smoothly.
- Côte d'Azur: no water in the sea. **Fail.**

## Results screen (claude/results-screen-a9f5cc)
- Finish a quick race with opponents: about 2 s after the line a 500x325 "Race Summary" panel
  opens with eight places, NAME / CAR / BEST LAP / TIME, your row in amber. Compare its look,
  position and timing with retail (panel colours, position and delay are guesses).
- Opponents still racing show dashes and fill in as they finish. Space, Enter or Esc return to the menu.
- Retail takes control away at the checkered line and the car rolls on; ours does not yet. Compare the panel with your 2026-10-22 recording (gold tint is a guess).

## Body lean (claude/body-lean-2c2196)
- Logged numbers (`logs/lean_check.txt`, `lean_check` example): retail's decoded lean model gives
  about 7.6 degrees of roll from roughly 0.8 g up (travel limit) and 3-5 degrees of nose-down
  under hard braking, before our 0.6 look scale (`LEAN_LOOK` in `car.rs`; drawn 4.6 / 3 degrees).
- Look for: how far the body rolls in a hard corner and dips under braking compared with retail
  footage. If retail looks bigger, raise `LEAN_LOOK` toward 1.0; if smaller, lower it. Not tested by eye.

## Roof up/down (roof-drawing)
- Garage: pick a convertible (993 Cabriolet, Boxster, 356 Cabriolet), set Roof to Up then Down on the
  car page: the car in the room should be rebuilt with the top up or folded (no animation, not tested live).
- Race and cockpit: roof down shows the folded top behind the seats; roof up the raised soft top.
- Coupes should look the same either way. Captures in `logs/car_993_s6_*`.
## Location screen 3D track room (2026-10-23)
- Open quick race > location: the camera should fly in from outside the room to a framed picture
  of the chosen track, on a hoarding in a round room.
- Press the location arrows: the room should turn a quarter and the picture change with a
  turn of the panels (0.5 s). Compare with retail: direction of the turn, how the panel flips.
- Wait 4 s without touching: the picture should change by itself to another view of the track.
- Check the seams between the 12 panels (thin lines are visible in our captures) and the room's
  brightness against retail.

## Traffic (2026-10-23, `claude/traffic-4f929a`)
Set Traffic to light/medium/heavy in the quick-race menu (`NFSPU_TRAFFIC=1..3` from the command line). Check: cars appear
ahead without popping in view, both directions, lanes sensible, they slow for you, hits shove both cars and cause damage,
nothing floats or sits across the road, the frame rate holds on heavy. Counts, lanes and speeds are guesses.

## Race sound channels (2026-10-23, `claude/cloud-sound-channels`)
Not tested by ear (made in a cloud session without speakers). Quick race on Autobahn with 3+ opponents:
- Rain: a steady patter from the start; it should go quiet under the bridges and return after. Quieter from the chase view than the cockpit.
- Wet road: tyre roll sounds the same loop as dry (the banks share it); skids should squeal less on the wet road than on Monaco.
- Opponents: their tyres squeal when they slide near you (fainter far away); their wall hits sound from their direction.
- Doppler: a car passing you should drop slightly in pitch as it goes by (about 2-5%). Compare with retail: stronger, weaker or absent?
- Chase view: swinging the camera to the side of the car should bring out the engine-side layers a little.
- Commentator: on a circuit, "lap 2" (etc.) at the line and "final lap" before the last; at the finish "you won the race" / "you placed Nth".

## Opponents avoiding traffic (2026-10-23, `claude/cloud-traffic-avoid`)
Quick race, 7 opponents, Traffic heavy. Check: opponents pull out to pass slow traffic and slot back in, they stay right
for oncoming cars, they brake behind traffic when both halves are blocked instead of ramming it; oncoming traffic edges
to its kerb as you come at it; traffic keeps clear of the centre line. Watch for opponents weaving or scraping walls on
narrow Monaco streets while passing. Measured offline only (not seen in a window).

## Lamp glow "covered" and location flags (glow-flags branch)
- Night race on Monaco 3-5 (tunnels): the car paint should show lamp glows sliding over it under a roof; in the open, the usual sun highlight. Log lines `car glow: ... (covered|open)` and `covered slices on <track>`.
- Location screen: watch the wall panels turn; the first picture of each track should carry a small flag. Backward/mirrored directions should move or swap it. Not seen on screen yet.
## Evolution career, first slice (branch `claude/career-1-9d081d`)
- Main menu -> "evolution...": the tournament list opens, account shows `$0` bottom left, car is the
  `'50 356` (orange). Event dropdown lists the six classic tournaments.
- "race info..." shows the next track; RACE runs it with the 356 and seven rivals in 356s (rival
  cars are a guess). Finish and close results: you should be paid `prize / races` for your place
  (e.g. 1st in 356 Challenge = $1,250). Quit and come back: cash and race count are kept in
  `settings/career/<name>.txt`.
- Check: does the payout size feel right? Does retail pay per race or at the end? Does the
  description text under the event name appear in retail (ours is missing)? Entry fee and start
  cash are guesses.

## Button outline light and pause panel (footage-menus)
- Main menu: hover a red button (singleplayer, load game...). A white streak should run along its
  top edge left to right as it turns yellow. Compare with retail: does the light continue round the
  right, bottom and left sides? How long does it last, does it repeat while you stay on the button?
- Race, press Esc/P: the panel should be pale yellow-to-teal with the world showing through, title
  at the top right, red buttons. Retail also squashes the panel up on continue (ours just closes).
