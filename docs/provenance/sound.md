# Front-end sounds

## Sound banks (`GameData/Sounds/*.bnk`, `BNKl` version 4)
Worked out from the shipped `fesfx.bnk` and checked by decoding all of its sounds without clipping. Reader: `nfspu_formats::bnk`.

| Offset | Field |
|---|---|
| 0 | `BNKl` |
| 4 | u16 version (4) |
| 6 | u16 slot count (54 in `fesfx.bnk`) |
| 0x10 | one u32 per slot: offset of the slot's patch from the slot's own position; 0 or 0xFFFFFFFF = empty |

A patch starts `PT\0\0` and is a list of tagged fields: a tag byte, a length byte, then that many big-endian bytes. 0xFC, 0xFD and 0xFE are markers without a value, and 0xFF ends the list.

| Tag | Meaning | Default when absent |
|---|---|---|
| 0x82 | channels | 1 |
| 0x84 | sample rate | 22050 |
| 0x85 | sample count | |
| 0x88 | absolute data offset | |

- **Codec**: EA "XA" 4-bit ADPCM. Every sound uses exactly 15 bytes per 28 samples per channel (0.536 bytes per sample per channel).
- **Frame**: one header byte (high nibble = predictor 0-3, low nibble = scale), then 14 bytes of nibbles, high nibble first. A sample is `((nibble << 28 >> (scale + 8)) + c1*h1 + c2*h2 + 128) >> 8`, with c1 = {0, 240, 460, 392} and c2 = {0, 0, -208, -220}. The first version shifted only the prediction and clipped 57-83% of samples; fixed.
- **Stereo**: **planar**, all of the left channel's frames then all of the right's. Confirmed against a retail recording with sound (see below). An earlier header-pairing argument for alternating frames was wrong.
- `bnk_dump` example: `cargo run -p nfspu-formats --example bnk_dump --release -- <file.bnk>` writes WAVs to `extracted/bnk/<bank>/`.

## Menu sound calls
- The retail front-end sound call plays sound *n* of the front-end bank (`fesfx`). It uses a volume derived from the effects level (not modelled; we play at full volume) and centre pan (0x40). The executable checks *n* against the bank's slot count, and sound *n* is **slot n + 1** (slot 0 is empty).
- Calls seen in the code: transition button click 5; dropdown open 2; dropdown closed by a click elsewhere 3; dropdown choice 5 (unless `disablesound`). Sounds for hover, back and other controls are not traced yet.
- **Recording check** (author's retail capture with sound, 2026-10-16). Every bank sound was decoded and cross-correlated with the recording (`bnk_variants` example), trying nibble order, scale rule and stereo layout. With the scale rule as written and planar stereo, **slot 6** matches at 5.6, 9.7, 12.5, 14.4, 17.0, 21.0, 25.8 and 29.9 s (scores 0.42-0.54). Those are all eight button clicks and dropdown choices in the video (sound call 5), in both `fesfx` and `lilfesfx`. Alternating-frame stereo gave no repeated matches.
- **Open sound confirmed** (author's second retail capture, music at 0, 2026-10-16, ~48 s). Same correlation, planar stereo:

| Slot | Call | Where it matched | Meaning |
|---|---|---|---|
| 3 | 2 | 22.22 s (0.54) and 30.61 s (0.41), the two frames where a dropdown opens | dropdown opens |
| 6 | 5 | 9.80, 18.41, 23.66, 25.46, 35.56, 40.95, 45.51 s (0.54-0.84) | every click |
| 7 | 6 | ~41 times (0.5-0.65), whenever the pointer moves onto another button | hover |
| 1 | 0 | 18.41, 25.77, 40.95 s, together with clicks that change screen or open a popup | screen change |
| 9/10 | ? | 0.3 s after each dropdown open | not identified; not played |

- **Close sound** (call 3 -> slot 4): never matched. In the capture every dropdown closed by picking a row (call 5), so a close-by-clicking-elsewhere never happened. Kept from the code reading. **Confirmed by ear** in the author's playtest of our menu (2026-10-16): open and close both sound like retail.
- Our mapping: screen change 0, open 2, close-elsewhere 3, click 5, hover 6. Volume still not modelled.

# In-race sounds

Worked out from the shipped files and the executable (Ghidra, read-only), 2026-10-22. Code: `nfspu_formats::engine_snd`, `nfspu_formats::bnk::sound`, `nfspu_game::race_sound` (rules) and `race_audio.rs` (playback). Numbers from a run go to `logs/race_sound.log`.

## Where the sounds are

| File | Holds |
|---|---|
| `GameData/Sounds/sd*.viv` (25 archives, plus `*16.viv` variants) | one engine sound set each: `<name>.bnk` and four `CRDl` tables (`.ect`, `.elt`, `.cct`, `.clt`), plus an 8-byte `default.cfg` (not read) |
| `GameData/Sounds/<spec>.spl` (86 files, 52 bytes) | per-car engine mixer settings, named by the car's `.sim` stem |
| `GameData/Sounds/zzzwzzz.viv` | opponent engines: `<name>lden.bnk` and `<name>ldex.bnk` (one looping sound each) for each engine set and for traffic, and `rain.bnk` |
| `GameData/Sounds/SND_coll.bnk` (`LilSND_coll.bnk` is the small version) | tyres, road, wind, hits, scrapes |
| `GameData/Sounds/track.viv` | 593 ambient sounds, named `<description>_<number>.bnk` |
| `GameData/Track/<track>_audio.scn` | where each track plays its ambient sounds |
| `GameData/Speech/zzzx<lang>.viv` (`zzzxeng.viv`) | the commentator: countdown `sor-a-3/2/1/go`, laps, places, records; `oneshots.txt` lists them |

- **Which engine set a car uses**: the catalogue record (`FEData/Data/nfs5.car`) holds it at 0x179 (`sd1a`, `sd1b`, ...). The executable opens `<name>.viv` (or `<name>16.viv` in its low-memory mode) and falls back to `sd7a` when the archive is missing. Opponents fall back to `sd6b`.
- **Sound numbers**: the game's sound number *n* is entry *n + 1* of a bank's slot table (entry 0 is never used); the u16 at 6 counts the sounds. This is the same rule as the front-end bank. Confirmed on the one-sound opponent banks (their only patch is in entry 1) and on the engine tables (highest number = count - 1).

## More patch fields

| Tag | Meaning | Evidence |
|---|---|---|
| 0x07 | the sound's key (MIDI note) | engine layers carry 29, 41, 51, 55-59: the firing pitch they were recorded at (key 29 = 44 Hz, a flat six at about 900 rpm). The sound library plays every sound at note 60 by default and offsets the pitch by (60 - key) semitones |
| 0x10 | fine tune, signed cents | values -70, -60, -30, +40 on engine layers |
| 0x86 / 0x87 | loop start / end sample | both 0 (or absent) on one-shots |
| 0x13 | volume (not used yet) | 127, 100, 80, 40 |

## Engine tables (`CRDl`)

| Offset | Field |
|---|---|
| 0 | `CRDl` |
| 4 | version byte, 2 |
| 8..0x20 | eight values, the same in every table seen (20, 15, 500, 200, 100, 40, 300, 100); not read by the mixer |
| 0x20 | eight signed sound numbers, -1 = unused layer |
| 0x128 + 4i | where layer i's volume curve starts, counted from this field |
| 0x148 + 4i | a second curve per layer, never read by the mixer |

A volume curve is 512 bytes: the layer's volume (0..127) at each engine-speed step. `sd1a.ect` has 8 layers (sounds 0, 34, 35, 36 and 64, 82, 83, 84) whose curves cover steps 0-168, 7-258, 180-307 and 271-511: a crossfade from idle to the redline.

## How retail mixes the player's engine

32 layers are kept: 0-7 `.ect`, 8-15 `.elt`, 16-23 `.cct`, 24-31 `.clt`. The view picks the set: the usual views use `.ect`/`.elt`; the `c` set is used only in one camera mode we have not identified. Every few ticks, each layer's volume is:

`curve[step] x fade(throttle) x fade(balance) / 8 x master / 2^18`, capped at 127, moving at most 8 per update.

- **step** = `scale x (rpm / max rpm x 65536) / 16384`, capped at 511. The scale comes from the `.spl` (value 9, 104 = 0x68 for most cars), so the redline is step 416. The "max rpm" is a car-spec value at +0x94 that we take to be the redline (**not verified**).
- **throttle fade**: `.ect`/`.cct` layers (coast) use fade(128 - load), `.elt`/`.clt` (load) use fade(load). Load is the throttle smoothed (up by half the gap per tick, down by an eighth), halved to 0..127, then raised by 1/64 + 1/32 and capped at 128. The fade curve is a 129-entry table (0, 3, 7, 10 ... 127, 127); we use `127 x ln(1 + 8t) / ln 9`, within 2 of it everywhere.
- **balance** fades sounds numbered below 64 against those from 64 up: fade(balance) and fade(128 - balance). The balance comes from the `.spl`: value 1 for the first-person views, value 4 (64) with a sideways adjustment for the others.
- **master** = `.spl` level (value 0 first-person, value 7 outside) x the engine channel level x the distance volume `2048 / (metres + 2)`, which grows a little with speed. A nearby engine saturates at 127.
- **pitch**: one pitch for every layer, `.spl` value 8 (80) x (rpm fraction x 65536 + 13107) / 1024, where 4096 is normal speed: 0.25 at standstill, 1.5 at the redline. The library then adds each sound's key offset, (60 - key) semitones + fine. Because every layer's key is its recorded firing pitch, all layers end up at the same pitch and cross-fade in tune.
- **gear changes**: during a change the smoothed throttle is cut (a quarter, a half or all of it by stage) and the volume dips; engine-bank sound 2 (a clunk) plays at the end of a change in some views. Ours: throttle target 0 and volume x 0.6 while changing, the clunk at half volume in every view.
- **opponents**: each opponent plays its `lden` and `ldex` loop at the same pitch rule (scale 80). The volume is `2048 / (metres + 2)` capped at 127. `lden` gets fade(64 - p) and `ldex` fade(64 + p), where p grows with how fast the car moves away from the listener: `lden` is heard coming, `ldex` going. Opponent channels mix at 70/127 x 85/128.

### `.spl` (13 x u32)

| Value | Use | Default | `959coupe29` |
|---|---|---|---|
| 0 | level, first-person views | 0x32 | 0x83 |
| 1 | balance, first-person views | 0 | 0x33 |
| 2, 3 | level and balance for the `c` table set | 0x46, 0x14 | 0x99, 0x14 |
| 4 | balance, outside views | 0x40 | 0x40 |
| 5, 6 | sideways balance swing | 0x80, 0x80 | 0x80, 0x80 |
| 7 | level, outside views | 0xaa | 0xff |
| 8 | pitch scale | 0x50 | 0x50 |
| 9 | step scale | 0x68 | 0x68 |
| 10-12 | not traced | 0x19, 0xdc, 0x1e | 0x0b, 2, 0x0b |

## Other race sounds (`SND_coll.bnk`, game numbering)

| Sound | Use | Evidence |
|---|---|---|
| 0x11 + surface | tyres rolling (tarmac 0x11); volume grows with speed above a threshold, capped at 122 | engine-update code; 0x11-0x14 share one low 390 Hz loop |
| 0x28, 0x2a, 0x2e, 0x2f, ... | skid sounds picked by surface and kind: on tarmac 0x2a by default (a tonal 790 Hz loop), 0x28 (noisy) joins on hard slides; 0x2c is the table's default | skid event code; tone measured on the decoded loops |
| 0x1f | wind: a loop whose volume (40-127) and pitch (+-2.3%) wander at random | engine-update code |
| 0x40-0x42 | wall hits, one of three at random, never the same twice in a row; louder with impact (64 + up to 63 at 10 m/s) | hit code |
| 0x20 | the hit sound for another collision kind; we use it for car against car (**guess**) | hit code |
| 0x0f | turbo whine | its channel level in the executable is 0, so it is silent in retail; not played |

What each skid kind and surface code means, and the input that drives the wind channel, are **not traced**: ours use the strongest wheel slide for the skids, and speed for the wind.

## Countdown voice

`sor-a-3`, `sor-a-2`, `sor-a-1` and `sor-a-go` (11025 Hz, mono) in the speech archive. We play them as the countdown numbers appear and "go" when the cars are released. The retail timing is **not verified** against a recording.

## Track ambience (`<track>_audio.scn`)

`AUDIO_ELEMENT 4` entries with no orientation line: a position, then twelve numbers, then a name, e.g. `4 94 300.0 80.0 0 0 0 0 0 0 100 0` "Water Lapping". Read so far: mode, sound number (the `track.viv` entry ending `_94.bnk`), outer and inner radius, shortest and longest wait between plays (0 0 = a steady loop), and volume 0..100 (11th value). The executable preloads every sound whose outer radius reaches the start. Our fade (full inside the inner radius, nothing beyond the outer) and the meaning of the other numbers are **guesses**.

## Channels added 2026-10-23 (cloud session, `claude/cloud-sound-channels`)

No decomp access in that session (the Ghidra bridge was not running), so only the install's banks
were read; every rule below that is not from the earlier notes is a **guess** and says so in
`race_sound.rs`. Levels all go through the Options sliders (`AudioSettings`: rain, skids, hits and
road on effects, opponent engines on engine, commentator on speech).

| Channel | Sound | Rule (ours) | Evidence |
|---|---|---|---|
| Rain on the car | `rain.bnk` in `zzzwzzz.viv`: one mono sound, 8000 Hz, 1.84 s, **not looped**, key 57 | rain amount at the camera x effects level; x 0.7 from outside views; silent under cover (wet track, dry road under the car). We loop it and play it at its key ratio | bank listing (`SOUND_CHECK_LIST=1 sound_check`); rule a guess |
| Wet road | `SND_coll` 0x12 (0x11-0x14 and 0x19 are the same 0.77 s loop, key 60 fine +50) | surface type moves up one when wet (weather.md: type 5 to 6), so 0x11 dry, 0x12 wet; same volume rule | weather.md + bank listing |
| Skids on a wet road | 0x2a / 0x28 | squeal at a third, scrub unchanged | guess |
| Opponent skids | 0x2a / 0x28, spatial | player's skid rule x distance volume (`2048/(m+2)` capped 127) | guess (same rule as their engines) |
| Opponent wall hits | 0x40-0x42, spatial | player's hit rule x distance volume | guess |
| Doppler | all opponent voices | pitch x `343 / (343 + v)`, v = how fast the distance grows, kept 0.5-2 | textbook rule, retail not traced |
| Sideways balance swing | engine layers, outside views | balance x (1 - 0.5 x \|side\| x swing/128), swing = `.spl` value 5 (camera left) or 6 (right) | guess on the meaning of values 5/6 |
| Commentator | speech archive | new lap: `lap2`..`lap8`, last lap `lap-final`; finish: `place-youwonrace`, `place-youplace<N>`, `place-youplacelast` (last of 3+) | phrase groups in `oneshots.txt`; which variant and when are guesses |

`RACE_DRIVE_SOUND=1` makes `race_drive` run these rules with the listener on the player's car and
write each channel's start/stop (with loudest volume), hits, passes with their Doppler factor and
the voice lines to `logs/race_sound.log`. Autobahn, 3 opponents (2026-10-23): rain stops under the
bridges and dry road takes over from wet road there; opponents' launch wheelspin triggers their
skids; passes log Doppler x1.02 closing, x0.98 leaving at ~8 m/s; finish line `place-youplacelast`.
Monaco1, 3 laps: `lap2` and `lap-final` at the line.

## Retail recording, GT1 on Cote d'Azur (2026-10-23, `2026-10-23 11-00-02.mp4`)

Audio pulled with ShareX's ffmpeg to `extracted/footage/race.wav` (48 kHz, stereo; not in the repo). The menu is
0:00-2:43 at about -20 dBFS RMS, silence 2:43-2:51, and the race runs from 2:51.5. Numbers below are from the
mono mix of that recording.

| Measure | Retail |
|---|---|
| Race loudness, whole mix | -10 to -14 dBFS RMS in 2 s windows (steady; the loudest stretches are -8) |
| Band energy (relative dB, summed per band) | under 150 Hz -25, 150-600 Hz -23, 600-2k -27, 2-6k -33, over 6k -47 |
| Engine at idle in the countdown | fundamental 62 Hz with a strong 2nd harmonic at 123 Hz |
| Engine in gear | partials at 155, 311, 618 Hz (octaves of about 78 Hz); the highest fundamental seen is 173 Hz |
| Idle to top ratio | 173 / 62 = 2.8 (the in-game rule gives 3.1 at the redline from 19% of it) |

No telemetry sits in the recording, so the engine pitch is compared as ratios and absolute frequency, not per gear.

## Not done / not verified

- None of the levels has been compared with a retail recording. The overall balance between engine, effects and speech is a guess (`MIX` = 0.7 on everything).
- The `c` table set (its camera mode is not identified), road surfaces other than tarmac (dry/wet), opponents' car-against-car hits, and the commentator's position, record and gap lines are not modelled.
- The rain, Doppler, opponent skid/hit, wet skid, sideways swing and commentator-timing rules above are guesses, to check against the decomp and a retail recording.
