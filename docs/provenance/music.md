# Music (`GameData/Music/zzzymus.viv`)

Sources: the shipped archive (read-only), FFmpeg's decode of one song for comparison, the
executable's music code (read as reference only), and the author's retail recording with music
(2026-10-16 22-27-22). Verified items are marked; the rest is a guess and says so.

## Archive: `BIGF`
| Offset | Meaning |
|---|---|
| 0 | `BIGF` |
| 4 | total size (not used) |
| 8 | u32 big-endian entry count (28) |
| 12 | u32 big-endian header size (1049) |
| 16 | per entry: u32 BE offset from the archive start, u32 BE size, NUL-terminated name |

Names are `<group><nn>-<title>-<artist>`. The groups are `game01`-`game14` (races), `menu01`-`menu05`
(front end), `win01`-`04`, `lose01`-`04` and `vic01`.

## Song stream: `SCHl` blocks
Every song is a chain of blocks: a 4-byte id and a u32 little-endian size that includes the 8-byte block header.

| Block | Contents |
|---|---|
| `SCHl` | `PT\0\0` and tagged fields, the same scheme as a sound-bank patch (`sound.md`): 0x82 channels (2), 0x84 rate (32000), 0x85 total samples, 0x80 version (1) |
| `SCCl` | u32 number of data blocks |
| `SCDl` | u32 samples in this block (2156 = 77 x 28 in full blocks), then one u32 per channel: where that channel's data starts, counted from just after this table |
| `SCEl` | end |

- Each channel's data in a block starts with two i16 values: the newest sample, then the one before it. They seed the predictor, so each block decodes on its own.
- Then come 15-byte frames of 28 samples. The header byte holds the predictor (high nibble, 0-3) and the scale (low nibble). It is followed by 14 bytes of nibbles, high nibble first.
- Sample = `((nibble sign-extended) << (20 - scale)) + c1*h1 + c2*h2) >> 8`, clamped to 16 bits. The coefficients are c1 = {0, 240, 460, 392} and c2 = {0, 0, -208, -220}. Unlike the sound banks, there is no rounding term.
- **Verified**: our decode of `menu01` equals FFmpeg's (which calls the codec "adpcm_ea_r1") in all 9,855,552 samples (`asf_check` example).

## Which song plays
- Every front-end layout's header names the music it wants: `musicfile=menu*` on 93 screens. `NONE` is on 5: `Options`, `careerfirst`, `chroniclemovie`, `chroniclemoviemain`, `endofera` is `dogface` (no song has that name).
- **Screen change**: when the active screen's pattern differs from the current one, the music fades over 1.5 s. Unless the pattern is `NONE`, a random song whose name matches it then starts. An empty pattern counts as `menu*`. The pattern match is a wildcard match, and we treat it as case-insensitive.
- **Song ends**: the next matching song plays, in archive order. A `RANDOMPLAYLIST` option makes this a random different song instead; it is off unless set.
- **Main menu**: on arrival, if the main menu has not picked a song for 7680 ticks (60 s), it starts a random `Menu*` song. A song change from a pick fades the old one over 0.5 s.
- **Level**: `Audio.MusicLevel` in the player's save (0-127; the executable's default is 55). At 0 the game plays no music. How the level maps to loudness is **not known**; we use a linear level/127.
- **Recording check** (`music_match` example, which cross-correlates recording clips against every song):
  - From 0 to about 24 s (quick race, then the main menu), `menu05` plays without a break.
  - At about 26 s the author opens Options (`NONE`), and its audio tab plays `game13`, the archive's first song (the tab previews the song selected in its list; not modelled).
  - Back on the main menu, at about 31 s, `menu02` plays. That is a new random menu song, as the rules say. It was 94 s into the song rather than at the start. This is **not explained**: either retail does not always start at 0, or `menu02` repeats an earlier section there.
- Not modelled: the start fade-in (the code passes 1000, possibly ms), the audio-tab preview, the shuffle option, and race music.
