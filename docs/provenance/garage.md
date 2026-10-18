# Front-end 3D garage room

The turning garage with the chosen car seen behind `GameSetup`, `QuickRace`, `garage` and other
pages with `room=ID_GARAGE`. Code: `nfspu_formats::fe_room` (file details),
`nfspu_game::garage` (staging, zones, room loading), `menu_garage.rs` (the Bevy scene in the
front end). Sources: the decompiled executable (read-only reference, own words only), the room
files in the player's install, and two retail recordings by the user
(`Videos/2026-10-16 22-27-22.mp4`, `22-40-11.mp4`), cropped frames in `logs/garage_retail_*.png`.

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
`LightRod`, `Lightcast01-04` (light cones under the ceiling lamps). The floor is at y = 0, the
room spans about x ±6, z -5.9..6.2, y 0..5.8 (game units = metres).

Per article, besides the usual geometry parts:
- `mn` part: one normal per vertex, 16 bytes (x, y, z, pad); count = vertex count (**verified**:
  e.g. `Room` 96 vertices, 96 normals). `n$` holds one record per triangle (face normals, not used).
- `Base` byte 0x4B: animation slot. `Fan01` = 1, `Fan02` = 2, all others 0 (**verified** in the
  data). Only the two fans have a transform (`rt` part), giving their pivot; their geometry is
  centred on it (±0.24 across, flat in x-y, so they turn about z).
- `Base` byte 0x46: 0x81 for most, 0x86 for `GarageSpot`, 0x8B for the light cones; 0x47 a running
  number. Meaning not traced.

### Materials

Seven `tm` records, 312 bytes each. Unlike cars (where the material kind picks the page), the
texture page is the u32 at **0x28** (**verified**: the floor's material 3 → archive 1 `flbi`,
the pit's material 4 → archive 2 `tile`, `GarageSpot` 5 → archive 3 `spot`, the cones 6 →
archive 4 `ligh`; all others → archive 0). The u32 at **0x10C** is 1 only on materials 5 and 6,
the two that use 4444 (alpha) pages: read as "translucent". Material 2 (the light rods) has
20.0, 20.0 at 0x12C/0x130 where the others have 1.0 (meaning unknown). Kind text at 0x10 is
`CarInt` or `CarIntShd`.

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
  turned by YRot about the vertical axis. So the room orbits with the car (**verified** in the
  recording: the shutter door passes behind the car as it turns).
- The clock is the game's 128 Hz timer (the timer start-up takes the rate as a parameter; 128 is
  the value already established in `menu_movie.rs`). Spin 30 → one turn per 30 s, 12°/s.
  **Verified**: in `22-40-11.mp4` the car goes from rear-to-camera (31 s) to front-to-camera
  (46 s), 180° in 15 s.
- Direction: **verified** clockwise seen from above (front pointing left, then away, then right,
  then toward the camera). In Bevy (right-handed, x mirrored) that is a negative yaw.
- YRot = 0 has the car's front toward the camera (the car model faces -z, the camera looks +z);
  consistent with the click zones below.
- The room's two fans turn: slot 1 gets the clock / 128 and slot 2 the clock / 192 as their z
  angle (**from code**; read as whole turns, so 1 and 0.67 turns per second, **guess**). Another
  update in the shared base class drives slots 8 and 9 (clock / 256 and / 128); the garage has
  none (the factory room probably does).
- The room's "position blend" (0..1) moves X, Y, Z toward (0, -0.8, 0.3): the camera ends up in the
  car. The car control drives it over 64 ticks (0.5 s) when the player climbs in or out (below).
- A shadow texture is projected under the car: translate by the car's offset, turn by -360·YRot,
  scale by 1/(2·width) and 1/(2·length), shift by 0.5 (**from code**, not drawn yet).
- **V_A**: read as half the horizontal field of view, 37.5° (75° across at 4:3, vertical 59.8°).
  **Fitted**: with it the 356 at the first retail frame's angle spans about half the screen width,
  as in the recording; read as a full angle the car would overflow the screen. The setter was not
  traced.
- Lighting: the numbers above are fixed-function D3D lights (ambient + one light). We bake
  ambient + light × max(0, n·-dir) into vertex colours on unlit materials with a gain of 1.6
  (**fitted** to the recording's wall brightness); retail's exact light model is not traced.

## The car (`NFS5.3DCAR`, from code)

`QuickRace.lay` has `id=NFS5.3DCAR pos=110,230 size=420,180` (also on `garage`, `carcompare`,
`dealership`, `paintshop`, `repair`, the victory screens, ...). Its accessor (`zMergedList.Car`)
gives the car; the control builds the car object and hands it to the room, which draws it at the
room's offset and turn. `positiontype` other than 0 places the car itself (1: (-2.4, 0.2, 8.1),
2: (2.4, 0.2, 8.1), 3: (2.4, -1.2, 6.2)) with its own turn of one per 20 s (negative for 2): the
two-car compare screens.

Retail draws the license plate with the player's name ("Greg" in the recording) — not done.

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
camera and the clockwise turn, 4 is the **front**, 3 the **rear**, 2 the **left** door, 1 the
**right** door (derived; both readings of the table agree only with the observed direction).

- 0: toggles `Car.Roof` (the quick-race roof setting) and plays the roof animation (an animation
  channel of the car, frames scaled by a 0..255 level).
- 1, 2: opens or closes that door: car parts 6 and 7 turn about y by up to 0.15 turn (54°), level
  0..255. Clicking a door that is already open climbs in: the room blend runs to 1 over 0.5 s; in
  the cabin a click climbs out (with a 0.5 s guard). Sounds 0x30 open, 0x31 closed.
- 3, 4: a timed fade on the car (sounds 0x32 / 0x34 on, 0x33 / 0x35 when off). Read as rear and
  front lights; the recording shows the headlights coming on after a click at the front
  (**verified** for 4 = headlights; 3 = rear lights is a **guess**).
- Pointer shapes by zone: roof 6, door 5 (open) or 8 (close), ends 3, in the cabin 7.

What we do: zones, headlights (glow by brightening the lamp texture, as the race viewer does),
rear lights, climbing in and out (the detailed cabin is shown from inside). Doors are logged but
not drawn: the car is built as a few merged meshes (`car_model.rs`, owned by another session),
so a door cannot turn on its own yet. Roof animation: not built (the roof band is not a zone).

The turn pauses while the pointer is on the car: in `22-27-22.mp4` the car holds still for about
4 s under the glove pointer, while in `22-40-11.mp4` (pointer elsewhere) it turns steadily. The
code for this was not found (**guess** that hovering is the cause).

## Pages

- `ID_GARAGE`: `GameSetup` (main menu: the room around the movie oval, no car), `QuickRace`,
  `garage` and more. A popup keeps the page's room behind it.
- `ID_TRACK` (`location.lay`): its own room class (track select), not drawn yet.
- The menu image is composed with a see-through backdrop on garage pages; the compositor keeps
  coverage ("over" with straight alpha) so the room shows where nothing is drawn. Retail blends in
  screen space; Bevy blends the menu image over the 3D view in linear light, so half-transparent
  edges over the room differ slightly (not measured).
