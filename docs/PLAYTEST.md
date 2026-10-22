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
