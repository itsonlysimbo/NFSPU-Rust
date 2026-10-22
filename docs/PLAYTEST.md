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
| 2026-10-22 | Damage, roll-overs, suspension | Hitting walls/cars leaves dents and scratches on the side hit; heavy damage pulls the steering and loses power; landings make the body dip and bounce; a car on its side or roof is put back on the road after about 3 s (in the air much sooner) | `claude/damage-6135c5` |

## Results
Add a line per playtest: date, recording path, what was wrong.

## Results screen (claude/results-screen-a9f5cc)
- Finish a quick race with opponents: about 2 s after the line a 500x325 "Race Summary" panel
  opens with eight places, NAME / CAR / BEST LAP / TIME, your row in amber. Compare its look,
  position and timing with retail (panel colours, position and delay are guesses).
- Opponents still racing show dashes and fill in as they finish. Space, Enter or Esc return to the menu.
