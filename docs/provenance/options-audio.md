# Options screen and the Audio page

Sources: the retail layouts in `FEData/Layouts` and controls in `FEData/Controls`, the two profiles in the user's
`SaveData`, and the executable's audio settings set-up (read through the Ghidra bridge, read-only). Our own words; no
code copied.

## How the screen is built

- `Options.lay` (frame `ID_OPTIONOVAL`, `musicfile=NONE`) holds five `NFS5.PANEL`s and one `NFS5.RADIOGROUP`. Each one
  names another layout in `filename=` and covers the whole screen at 0,0.
- Panels: `ID_controllers_panel` -> `controllers`, `ID_graphics_panel` -> `graphics`, `ID_display_panel` -> `display`,
  `ID_audio_panel` -> `audio`, `ID_cameras_panel` -> `cameras`. The tab group is `optiontabs`.
- `optiontabs.lay` has five `NFS5.GRAPHICTABBUTTON`s; each names the panel it shows (`panel=`), its label string and
  `tabs.fsh` frames for hover / selected (`hovershape`, `selectedshape`, `flipit` mirrors).

| Tab (left to right) | pos | string | panel |
|---|---|---|---|
| 1 | 59,0 | 14102 | audio |
| 2 | 154,0 | 14101 | controllers |
| 3 | 263,0 | 14100 | cameras |
| 4 | 354,0 | 14104 | graphics |
| 5 | 414,0 | 14103 | display |

- `audio.lay` holds two panels: `ID_MAINSCREENPANEL` -> `audioreal` (the levels) and `ID_PLAYLISTPANEL` -> `cdplayer`.
- The bottom bar's options button has `callback=OPTIONS` (no `destination`).

## The levels

`audioreal.lay` draws each level as an `NFS5.GouraudIndicatorBar` (198 x 9 at x 307) over an `NFS5.Rectangle` of the
same size, with an `NFS5.AccessorWatch` showing `<accessor>(PERCENT)` beside it. Bars have `changeable=1`,
`goingleft=0`, colours `c0=0xffe60000`, `c50=0xffffd76c`, `c100=0xfffdb306`.

| Accessor | bar y | Range | Start value |
|---|---|---|---|
| `Audio.MusicLevel` | 150 | 0-127 | 127 |
| `Audio.SfxLevel` | 169 | 0-127 | 127 |
| `Audio.EngineLevel` | 188 | 0-127 | 127 |
| `Audio.SpeechLevel` | 207 | 0-127 | 127 |
| `Audio.AmbientLevel` | 226 | 0-127 | 127 |

- **Verified (executable)**: the audio settings object makes all five as integer settings with minimum 0, maximum 127
  and start value 127. The small numbers next to them (0x37..0x3b) are not defaults; they rise by one per setting.
  The older note in `music.md` that the music default is 55 read one of those numbers.
- **Verified (saves)**: both profiles store all five as `name\0` + 32-bit value. A fresh-looking `8Driver.sav` has all
  at 127; the user's `3Greg.sav` has music 0 and the rest 127.
- Other audio settings on the page: `Audio.Mode` (3D audio dropdown) and `Audio.Quality` (dropdown), both 0 in the
  saves; `CDPlayer.*` drive the playlist. Not built.
- The saves also carry the other pages' values (`Graphics.*`, `Display.*`), listed here for later: e.g.
  `Graphics.ViewDistance 3`, `Graphics.GammaCorrection 100`, `Display.Speedometer 2`, `Display.SpeedUnits 0`.

## What we do

- Levels live in our own file `settings/audio.txt` in the project folder (gitignored; `NFSPU_SETTINGS_DIR` moves it),
  as `Audio.MusicLevel=127` lines with retail's names and range. Retail keeps them per player in the `.sav`; we keep one
  set, and the first run without our file takes them from the selected player's save. The install is never written.
- `nfspu_game::audio_settings::AudioSettings` (Bevy resource) holds them as 0..1 (level / 127).
- Clicking or dragging a bar sets it from the pointer's x; the file is written when the button comes up.
- **Guess**: the bar fill blends c0 -> c50 -> c100 along its length; percent is level * 100 / 127 rounded.
- **Guess**: loudness is linear in level / 127 (the real curve is not traced).
- The audio tab plays the archive's first song (as a retail recording showed, `music.md`); other tabs are silent
  (**guess**). The Graphics, Display, Controllers and Cameras tabs open their layouts, but their controls are not
  wired (**not done**).
