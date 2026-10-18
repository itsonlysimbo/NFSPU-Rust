# Front-end 3D garage room

The turning garage with the chosen car seen behind `GameSetup`, `QuickRace`, `garage` and the
other pages with `room=ID_GARAGE`. Code: `nfspu_formats::fe_room` (file details),
`nfspu_game::garage` (staging, click zones, room loading), `menu_garage.rs` (the Bevy scene in
the front end), plus small hooks in `menu.rs` (frame lookup, `NFS5.ExtraCircle`), `menu_canvas.rs`
(see-through canvas) and `menu_mode.rs`. Sources: the decompiled executable (read-only
reference, described in own words), the room files in the player's install, and two retail
recordings by the user (`Videos/2026-10-16 22-27-22.mp4`, `22-40-11.mp4`); cropped 640x480
frames are in `logs/garage_retail_*.png`.

Labels: **verified** (checked against data, code and footage), **from code** (read in the
decompile, not checked on screen), **fitted** (number chosen to match footage), **guess**.

## Files

| File | What it is |
|---|---|
| `FEData/Models/garage.crp` | the room model: a RefPack-compressed car container (`" raC"`), 33 articles, read with the existing CRP reader (**verified**) |
| `FEData/Models/garage.tpg` | text: `file1..5` = `exterior.fsh` 256 (565), `floor.fsh` 64, `platform.fsh` 64, `spotlite.fsh` 128 (4444), `light.fsh` 64 (4444) |
| `factory`, `dealer2a/b/c`, `track`, `trackwall`, `victory` (`.crp` + `.tpg`) | the other rooms, same format (not drawn yet) |

The `.fsh` names in the `.tpg` are not files on disk: the archives are embedded in the `.crp` as
misc parts tagged `[n, 0, 'f', 's']`, archive n = `file(n+1)`. Garage archive 0 holds 11 tiles
(`door`, `wlbo`, `wlto`, `lamp`, `roof`, `tool`, `tire`, ...), 1 `flbi` (floor), 2 `tile`
(platform), 3 `spot`, 4 `ligh` (**verified** by dumping, `examples/fe_room_dump.rs`).

### Articles

All at level of detail 1 only. Names: `Room`, `Floor`, `Roof`, `Door` (the roller shutter),
`Beam01-06`, `Column01`, `Pole01/02`, `Logo01-07`, `Vent`, `FanBox01/02`, `Fan01/02`,
`GaragePit` (the pit and platform the car stands on), `GarageSpot` (light spot on the floor),
`LightRod` (ceiling light tubes), `Lightcast01-04` (light cones). The floor is at y = 0; the room
spans about x ±6, z -5.9..6.2, y 0..5.8 (game units = metres). 934 triangles in all.

Per article, besides the usual geometry parts:
- `mn` part: one normal per vertex, 16 bytes (x, y, z, pad); count = vertex count (**verified**:
  e.g. `Room` 96 vertices, 96 normals). `n$` holds one record per triangle (not used).
- `Base` byte 0x4B: animation slot. Garage: `Fan01` = 1, `Fan02` = 2, all others 0 (**verified**).
  Only the fans have a transform (`rt` part), giving their pivot; their geometry is centred on it
  (±0.24 across, flat in x-y, so they turn about z).
- `Base` byte 0x46: 0x81 for most, 0x86 for `GarageSpot`, 0x8B for the light cones; 0x47 a running
  number. Meaning not traced.

The same slot byte exists in car files (**verified** on `356_1.crp`): 1-4 the wheels, 5 the
steering wheel, 6 and 7 the two doors (all door parts: outer panel, window, handle, inner panel,
glass, decals), 8 the front lid (`Hood*`), 9 the rear lid (`Trunk*`), 11/12 the gauge needles,
18-21 the wheel shadows. Each door's outer panel carries its hinge as a transform: x ±0.733,
y 0.570, z -0.734 (the front edge; cars face -z). Some other door parts have a transform near the
centre line, which is not the hinge.

### Materials

Seven `tm` records, 312 bytes each. Unlike cars (where the material kind picks the page), the
texture page is the u32 at **0x28** (**verified**: the floor's material 3 → archive 1 `flbi`,
the pit's material 4 → archive 2 `tile`, `GarageSpot` 5 → archive 3 `spot`, the cones 6 →
archive 4 `ligh`; all others → archive 0). The u32 at **0x10C** is 1 only on materials 5 and 6,
the two that use 4444 (alpha) pages: read as "translucent". Material 2 (the light rods) has
20.0 at 0x12C/0x130 where the others have 1.0; the rods glow in the recordings, so we draw a
material with more than 1.0 there at full brightness (**guess** at the meaning). Kind text at
0x10 is `CarInt` or `CarIntShd`.

## Staging (from code)

The garage room class is a "spinning room" (a subclass of a generic 3D-room menu). Its
constructor sets these fields; the room's debug overlay prints them with labels, which names
them:

| Label | Garage | Base default | Use |
|---|---|---|---|
| X, Y, Z | 0, -1.1, 4.8 | 0, -1.2, 4.2 | position of the room **and** the car relative to the camera |
| XRot, YRot, ZRot | 0 | | YRot = turn, in whole turns (the car shadow code multiplies it by -360) |
| Spin | 30 | 30 | seconds per turn: YRot = clock / (rate × Spin) |
| V_A | 37.5 | from a setting | view angle (see below) |
| AR AG AB | 55, 65, 55 | 64 each | ambient light |
| SR SG SB | 120 each | 159 each | light colour |
| SDir | -0.14, -0.98, 0.114 | | light direction |
| SPos, SStr, SAng | 0; 50; 0.8 | | spot position, strength, angle (not used here) |
| cardif | 3.5 | 1.0 | unknown |
| Tint RGB | (165, 210, 230) base | | unknown (environment tint?) |

- The camera never moves. Each frame the room model and the car are both placed at (X, Y, Z) and
  turned by YRot about the vertical axis, so the room orbits with the car (**verified** in the
  recording: the shutter door passes behind the car as it turns).
- The clock is the game's 128 Hz timer (the timer start-up takes the rate as a parameter; 128 is
  the value already established in `menu_movie.rs`). Spin 30 → one turn per 30 s, 12°/s.
  **Verified**: in `22-40-11.mp4` the car goes from rear-to-camera (31 s) to front-to-camera
  (46 s), 180° in 15 s.
- Direction: **verified** clockwise seen from above (front pointing left, then away, then right,
  then toward the camera). In Bevy (right-handed, x mirrored) that is a negative yaw.
- YRot = 0 has the car's front toward the camera (the car model faces -z, the camera looks +z).
- The room's two fans turn: slot 1 gets the clock / 128 and slot 2 the clock / 192 as their z
  angle (**from code**; read as whole turns, so 1 and 0.67 turns per second, **guess**). Another
  update in the shared base class drives slots 8 and 9 (clock / 256 and / 128); the garage has
  none (the factory room probably does).
- The room's "position blend" (0..1) moves X, Y, Z toward (0, -0.8, 0.3): the camera ends up in the
  car. The car control drives it over 64 ticks (0.5 s) when the player climbs in or out.
- A shadow texture is projected under the car: translate by the car's offset, turn by -360·YRot,
  scale by 1/(2·width) and 1/(2·length), shift by 0.5 (**from code**, not drawn yet).
- **V_A**: read as half the horizontal field of view, 37.5° (75° across at 4:3, vertical 59.8°).
  **Verified** against the recording: with it the 356, the shutter door, the pit and the far wall
  land within a few pixels of the retail frame (`logs/garage_compare_quickrace.png`). The setter
  was not traced.
- Lighting: the numbers above are fixed-function D3D lights (ambient + one light). We bake
  ambient + light × max(0, n·-dir) into vertex colours on unlit materials, scaled by 0.5, and draw
  the room without Bevy's tonemapping (**fitted**: the formula is retail's, the 0.5 is chosen to
  match the recording; retail's exact light model is not traced).

## Frame and blending

- Retail's frame lookup maps a page's `screen=ID_SCREEN` to `screenempty.lay` in its normal
  hardware-3D mode (**from code**: the lookup has two branches on a mode flag that is set when the
  card has more than about 4 MB; the low branch was not followed). `screenempty.lay` is 14
  `CnstLyr.fsh` pictures forming the dark lens ring around the room. **Verified**: the ring in the
  recordings matches it. Every non-popup page with `ID_SCREEN` has a room, so the bottom-bar rule
  is unchanged. `menu::frame_layout` now does this.
- `NFS5.ExtraCircle` (on `QuickRace`, `garage`, the victory and knockout pages): its template's
  `pic0..pic7` parts are `extracircle.fsh` frames 0-7 at fixed positions, the thin half circle
  seen in the recording. Now drawn.
- Garage pages compose the menu on a see-through canvas (the compositor keeps coverage, "over"
  with straight alpha; opaque pages are unchanged). The 3D room renders into a texture the size
  of the window's 640x480 area, and one UI node mixes the menu over it on the stored (gamma)
  values, as retail blends. Bevy's own UI blending works in linear light and made the half-clear
  ring glass too light (left ring sample 103,130,137 before, 97,120,123 after, retail 93,114,116).

## The car (`NFS5.3DCAR`, from code)

`QuickRace.lay` has `id=NFS5.3DCAR pos=110,230 size=420,180` (also on `garage`, `carcompare`,
`dealership`, `paintshop`, `repair`, the victory screens, ...). Its accessor (`zMergedList.Car`)
gives the car; the control builds the car object and hands it to the room, which draws it at the
room's offset and turn. `positiontype` other than 0 places the car itself (1: (-2.4, 0.2, 8.1),
2: (2.4, 0.2, 8.1), 3: (2.4, -1.2, 6.2)) with its own turn of one per 20 s (negative for 2): the
two-car compare screens (not done).

- Body style: we pick the `.tpg` style whose name appears in the catalogue name (`'50 356 1100
  Cabriolet` → `[style2] name=Cabrio`), **guess** at retail's rule; matches the recording (the
  cabriolet with its top up).
- Paint: the quick-race colour list's choice (list position → `.clr` section in file order; the
  356's file lists section 5 before 4). An owned car's own colour is kept in the save and is not
  read yet: the user's owned 356 is orange in retail (section 19 is the closest orange); our
  captures pick it with `NFSPU_MENU_PICK=GenericCar.Colour=18`.
- Retail draws the license plate with the player's name ("Greg" in the recording): not done.
- The car is built with the race viewer's pages and looks (`car_model::load_car` + the race
  viewer's material helpers), at level of detail 3, without a driver; doors are split off by slot.

### Inspection (from code)

A click inside the box picks a part with this rule (f = fractional part of YRot, the box split in
sixths across):

| f | left sixth | middle | right sixth |
|---|---|---|---|
| 0.9..0.1 | 4 | 4 | 4 |
| 0.1..0.4 | 4 | 2 | 3 |
| 0.4..0.6 | 3 | 3 | 3 |
| 0.6..0.9 | 3 | 1 | 4 |

and zone 0 for the top 5/16 of the box when the car has a roof animation. With f = 0 front to the
camera and the clockwise turn, 4 is the **front** and 3 the **rear**, and in the middle columns
the camera sees the car's left side (zone 2) or right side (zone 1).

- 0: toggles `Car.Roof` (the quick-race roof setting) and plays the roof animation (an animation
  channel of the car, frames scaled by a 0..255 level).
- 1, 2: opens or closes a door: zone 1 turns car slot 6, zone 2 slot 7, about y by up to 0.15 turn
  (54°), level 0..255. Clicking a door that is already open climbs in: the room blend runs to 1
  over 0.5 s; in the cabin a click climbs out (with a 0.5 s guard). Sounds 0x30 open, 0x31 closed.
- 3, 4: a timed fade on the car (sounds 0x32 / 0x34 on, 0x33 / 0x35 when off). Read as rear and
  front lights; the recording shows the headlights coming on after a click at the front
  (**verified** for 4 = headlights; 3 = rear lights is a **guess**).
- Pointer shapes by zone: roof 6, door 5 (open) or 8 (close), ends 3, in the cabin 7.

**Unresolved: which door.** Slot 6 sits at data x = +0.73. Under the project's left-handed reading
of the data (see `MIRROR_X`) that is the car's left, but the zone table, the observed turn and
the camera place the car's left side in front of the camera for zone 2, which turns slot 7. The
two cannot both hold; one of the handedness assumptions is off somewhere. We open the door that
faces the camera (zone 2 → slot 6, the car's left as we draw it), so the result looks right.

What we do: the zones; doors open and close (54°, one second end to end, rate **guess**);
headlights and rear lights fade on and off (0.4 s, **guess**): the lamp texture brightens as in
the race viewer, and a soft round glare sprite (0.5 m, our own art, **guess** at retail's look)
appears in front of each headlight; climbing in and out (the detailed cabin shows from inside).
Not done: the roof animation (the roof band is not a zone), click sounds, pointer shapes, the
hood and trunk.

**Level of detail.** Retail's showroom car is the garage model (level 1). Only levels 1 and 2 have
the doors as separate parts; level 3 bakes them into the body. Our car loader draws levels 1-2
with wrong artwork on the rear quarters (grey panels; not the `Cabrio` article, tested), so the
show car uses level 3 and the doors, though clicked and animated, do not visibly open.
`NFSPU_GARAGE_LOD=2` shows them opening (`logs/garage_inspect_door_lod2.png`). Fixing levels 1-2
in `car_model.rs` would give retail's model and working doors.

The turn pauses while the pointer is on the car: in `22-27-22.mp4` the car holds still for about
4 s under the glove pointer, while in `22-40-11.mp4` (pointer elsewhere) it turns steadily. The
code for this was not found (**guess** that hovering is the cause). It also pauses in the cabin.

## Pages

- `ID_GARAGE`: `GameSetup` (main menu: the room around the movie oval, no car), `QuickRace`,
  `garage`, `knockout`, `paintshop`, `settingsMP`, ... A popup keeps the page's room behind it.
- `ID_TRACK` (`location.lay` and friends): its own room class, not drawn yet. From its
  constructor: two models, `track` and `trackwall`; position (0, -4.8, 6.2) (a second set, (0, -3.2, 1.9), is kept beside
it), XRot -0.04, Spin 20,
  V_A 25, ambient (55, 65, 76), light 115 each, light direction (-0.29, -1.5, 0.29).
- `ID_VICTORY`, `ID_DEALER`, `ID_FACTORY`: other room classes, not drawn.

## Checks

Captures (`NFSPU_MENU_CAPTURE=1`, waits 3 s on garage pages while Bevy compiles the 3D shaders):
`logs/menu_quickrace.png`, `menu_gamesetup.png`, `menu_garage.png`; side by side with retail:
`logs/garage_compare_quickrace.png`, `garage_compare_gamesetup.png`; inspection with scripted
clicks (`NFSPU_GARAGE_CLICKS=x,y@secs;...`, garage clock): `garage_inspect_lights.png` (front
click at turn 0.95: headlights on), `garage_inspect_door_lod2.png` (door at level 2),
`garage_inspect_cabin.png` (door, then a click on the open door: inside the car).
`NFSPU_GARAGE_TURN=0..1` sets the starting turn; `NFSPU_MENU_POINTER=x,y` keeps the real mouse
from pausing the turn.

Brightness of sampled 640x480 patches in `garage_compare_quickrace.png`, ours vs retail (sRGB
averages, gain 0.5, tonemapping off, screen-space blend; `logs/garage_brightness.txt`):

| Patch | Ours | Retail |
|---|---|---|
| far wall | 101,107,99 | 97,112,91 |
| floor, left | 58,63,61 | 59,65,63 |
| ring glass, left | 93,115,118 | 93,114,116 |
| wall right of the ring | 95,110,110 | 89,93,96 |
| pit stripes | 85,86,70 | 74,76,61 |
| shutter door (under the ring edge) | 76,85,79 | 56,65,57 |

Not tested: window resizing while a garage page is open (the room texture is resized; only the
start-up size was seen), other cars than the 356, the `garage` page with an owned car's own
colour, pages other than `QuickRace`, `garage` and `GameSetup`.
