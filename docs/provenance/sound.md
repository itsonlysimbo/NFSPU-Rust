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
