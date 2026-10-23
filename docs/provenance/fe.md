# Front end: screens, controls and strings

Notes on `FEData/`, the data behind the retail menus. Prose and tables only; the readers live in
`crates/nfspu-formats/src/fe.rs` and the audit is `crates/nfspu-formats/examples/fe_audit.rs`
(output in `logs/fe_audit.txt`).

## What is in `FEData`

| Folder | Files | What it is |
|--------|-------|------------|
| `Layouts` | 98 `.lay` | one screen per file: a list of controls with positions |
| `Controls` | 78 `.ctr` + 1 `.scr` | one control *type* per file: parts, states, defaults |
| `Locale` | `festrings.csv`, `festrings.h`, `hudstrings.h`, 99 `.loc` | every string, in seven languages |
| `Art` | 72 `.fsh` + 12 `.ffn` | menu art pages and fonts |
| `Models` | `garage`, `factory`, `dealer2a/b/c`, `track` (`.crp` + `.tpg`) | the 3D scenes the menus put behind themselves |
| `Showcase` / `Chron` / `Factory` | 64 / 182 / 35 `.fsh` | gallery, chronology and factory-driver art |
| `Movies` | 12 `.mad` | opening, ending and chapter movies |
| `Data` | `nfs5.car/.prt/.trn/.trk/.fac/.per/.loc`, `nfs5-1.hud`, `nfs5-2.hud` | catalogue tables and the default HUD layouts |

## `.lay` ÃƒÂ¢Ã¢â€šÂ¬Ã¢â‚¬Â one screen per file

Plain text, `[section.0]`, `[section.1]`, ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â¦ blocks of `key = value`. Comments start with `;`.
Section 0 is the screen itself (its `id` is a screen control such as `REAL.BLACK_640x480`); every
later block is one control placed on it. Keys seen across the 98 shipped files:

| Key | Meaning |
|-----|---------|
| `id` | control type; matches a `[control] id=` in `Controls/` |
| `name` | the control's own name symbol (`ID_...`), used by the control template, **not** a locale symbol |
| `pos` / `size` | `x,y` in the screen's pixels |
| `stringID` | numeric locale string id for the control's label |
| `destination` | screen to go to when the control is used (e.g. `garage`) |
| `callback` | named script hook, empty in the shipped files |
| `state`, `visible`, `enable`, `layer`, `maincolor`, `stringcolor`, `texture`, ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â¦ | appearance and behaviour |

All 3,380 blocks across the 98 files parse; 2,976 are control placements.

## `.ctr` / `.scr` ÃƒÂ¢Ã¢â€šÂ¬Ã¢â‚¬Â control types

Same block syntax. `[control]` gives `id`, `description`, `parts` (the drawable pieces) and
`states`; later blocks (`[state.hover]`, `[part.canvas]`, ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â¦) describe each part with
`value=`/`flags=` pairs. `.scr` files describe screen-sized controls (`Black Screen 640x480.scr`
defines `REAL.BLACK_640x480`), so both extensions are read the same way.

Two facts worth knowing:

* **73 distinct control ids** are defined across the 79 files (a couple of ids appear twice).
* **Six files are deliberately empty** ÃƒÂ¢Ã¢â€šÂ¬Ã¢â‚¬Â five are 0 bytes (`3DTrack.Ctr`, `ComboBoxSlider.ctr`,
  `GotoRect.ctr`, `GraphicsComboBoxSlider.ctr`, `TrackLarge2dSpline.ctr`) and
  `UserNameInputBox.ctr` is comment-only and says why: *"Left here, empty.. so that old copies
  aren't lying around on hard drives."* They are not parse failures.
* **Every id a layout uses exists**: 59 distinct ids used, 0 undefined. 14 templates ship unused.

## `festrings.csv` ÃƒÂ¢Ã¢â€šÂ¬Ã¢â‚¬Â the string table

`String ID` plus one column per language, then `DescID` (a symbol such as `IDS_QUICKRACE`),
`Conditionals`, `Comments`, `OrigFile` and trailing columns. 3,124 rows parse.

Two quirks, both handled:

* **The file is ISO-8859-1, not UTF-8** (it carries text such as `dÃƒÆ’Ã‚Â©fi Porsche`). Reading it as
  UTF-8 fails outright, so the reader takes bytes and decodes one-to-one (`decode_latin1`).
* **The column names are one off from the data.** The tags that name the screens a string belongs
  to (`!modeselectionbox`, `!donttouchme`, `!None`) sit in the column the header calls `OrigFile`,
  while the `Comments` column is mostly empty. The reader takes `Comments` and falls back to the
  next column, so both this file and a tidied export work.

Language order is the game's own: `0 ENGLISH, 1 FRENCH, 2 GERMAN, 3 ITALIAN, 4 SPANISH, 5 SWEDISH,
6 PORTUGUESE`, exactly as `hudstrings.h` declares them (and the same ids `refbuild::Language`
uses). A missing translation falls back to English.

Linking a layout to its text is by number: **all 653 `stringID=` references in the 98 layouts
resolve** in this table. The `name=` symbols in `.lay`/`.ctr` are control names, not locale
symbols ÃƒÂ¢Ã¢â€šÂ¬Ã¢â‚¬Â 301 of the 462 locale symbols appear in the generated `festrings.h`/`hudstrings.h`
headers, and the rest are older names.

## The menu renderer (milestone M6)

`nfspu-game::menu` turns a `.lay` screen into things to draw, and `menu_mode.rs` puts them in the
viewer. It is driven entirely by the shipped data: geometry from the layout, colours and label
placement from the control templates, text from the locale table.

Drawn so far:

| Control | How |
|---------|-----|
| `NFS5.GOTOBUTTON`, `NFS5.BOTTOMBARBUTTON`, `NFS5.MEDIUMBUTTON` | the template's `bg` part is a `RECT` inset one unit, filled with `maincolor`, `hovercolor` on hover; the label is the control's `stringID` |
| `REAL.LABEL`, `NFS5.LABEL`, `NFS5.HEADER`, `NFS5.SUBHEADER` | text at the control's `pos` with `foreColor`; point size from the control's font name (`Univers17b.ffn` -> 17) |
| `NFS5.GLINTYLINE`, `NFS5.Rectangle`, `NFS5.OutlineRectangle` | a filled rectangle at `pos`/`size` |

Two traps worth remembering. Only the **first** block of a layout is the screen: `REAL.` means
"engine-built-in control", and `REAL.LABEL` / `REAL.HOVER_PICTURE` are ordinary controls that
appear after the header. And a text-only control carries **no size**, so a size test must not be
applied to it before its type is known.

Navigation follows the game: a control's `destination=` names `<destination>.lay` (the
`GoToButton.ctr` header documents exactly that), and a control with `callback=GOBACK` goes back.
Backspace is a back shortcut and Escape quits.

The layout's own 640x480 space is scaled uniformly to the window height and centred, so the
numbers land where the retail build puts them on a 4:3 window. The drawing goes through the UI
layer (`Node` + `BackgroundColor` + `Text`), which is what the rest of the program uses for 2D;
the sprite path was tried first and produced a blank frame. Two traps there: UI is still drawn by
a camera, so one has to exist, and a `cmp` on the capture is the only way to know it worked
(`NFSPU_MENU_CAPTURE=1` writes `logs/menu_<screen>.png`).

`logs/menu_audit.txt` (from `examples/menu_audit.rs`) reports coverage across all 98 screens:
1,500 draw commands today, with 1,936 controls not drawn yet. The biggest gaps are
`NFS5.GOURAUDBOX` (576 uses, a four-corner gradient box) and `NFS5.PICTURE` (484, a `SHAPE` part
that names an `Art/<name>.fsh` page and an entry index - the pieces are all in place, the
textured quad is not). Everything else is listed there by frequency. Passing a screen name
(`cargo run -p nfspu-game --example menu_audit -- singleplayer`) dumps that screen's draw list,
which is how the layout is checked without opening a window.

### Gradient box (`NFS5.GOURAUDBOX`)
Verified from the shipped `GouraudBox.ctr` template and the retail executable's control class (read-only reference; behaviour described here, no code taken).

| Field | Meaning |
|---|---|
| `pos`, `size` | Box rectangle in 640x480 layout units |
| `cUL` `cUR` `cBL` `cBR` | ARGB colour at each corner (template default `0xFF0000FF`) |
| `ULoffX/Y` `URoffX/Y` `BLoffX/BLoffy` `BRoffX/Y` | Per-corner shift in layout units, **added** to that corner (the lower-left Y key really is spelled `BLoffy`) |
| `state=hidden` | Not drawn |
| `transon`/`transoff` | Fade transitions, not implemented |

The retail draw call reads the four colours and eight offsets once, then emits one four-vertex polygon: each vertex is the box corner plus its offset, coloured by its corner, with smooth (Gouraud) shading between them. Layouts use offsets of +/-3 on thin bars to make 45-degree mitred frame corners.

Our renderer rasterises each box on the CPU (two triangles UL-UR-BL and UR-BR-BL; the retail split is not verified) into a small image drawn in the UI layer, so it stacks in layout order with everything else. **Not tested** against a retail screenshot.

### Screen layers and draw order
- **Frame**: a page header's `screen=ID_<NAME>` names the frame layout `screen<name>.lay` (`ID_MOVIEOVAL` -> `screenmovieoval.lay`). Verified in the retail executable's frame lookup. `ID_SCREEN`, `ID_NONE` and empty mean no frame. Retail also swaps the option oval for a `lite` variant in some states; not modelled.
- **Room**: `room=ID_GARAGE` is a 3D scene behind everything (the blurred garage in retail screenshots). Not drawn yet; we clear to black.
- **Popup**: retail opens `GameSetup` with `selectname` over it (seen in a retail screenshot; the front-end start-up references `selectname` and `bottombar.lay`). Our rule: a screen with `room=ID_NONE` opens over the current page. **Guess.**
- **Bottom bar**: `bottombar.lay` is drawn on framed pages. When exactly retail shows it is a **guess**.
- **Section order**: the first section is the topmost. Layouts list small details and buttons first and their large backing panels last (`selectname.lay` ends with three full-panel gradients and the popup circle), so retail paints from the last section to the first. Strong layout evidence; the draw loop itself is not traced.

### Pictures (`NFS5.PICTURE`, `NFS5.POPUPCIRCLE`)
| Field | Meaning |
|---|---|
| `shapePathName`, `shapeIndex` | Archive in `FEData/Art` and entry index in directory order |
| `pos` | Top-left in layout units |
| `size` | Ignored: the template sets `autosize`, so the image's own size is used (`50,150` is the template default) |
| `flip` | Bit 1 mirrors left-right, bit 2 top-bottom (3 = both) |

`POPUPCIRCLE` has no image fields of its own. Its template draws one quarter (`PopUp.fsh` #0, 207x175) four times at part offsets `0,0`, `207,0`, `0,175`, `207,175` with flips 0/1/2/3, anchored at the template position `113,100`. The quarter's decoded size equals the offsets, which confirms the layout.

Front-end art pixel formats: `0x6D` is 16-bit ARGB4444 (oval frame, popup circle) and `0x78` is RGB565 (bottom-bar buttons). Both were added to the SHPI reader and checked by eye on the rendered main menu. Still undecoded: `0x7B` with a non-32-bit palette, and the `0xED`/`0xF8`/`0xFB` codes (high bit set, possibly compressed entries). See `logs/fe_art_audit.txt`.

### User list and portrait (`selectname.lay`)
- **List** (`NFS5.ListBox`, `accessor=UserList.UserList`): every `<save dir>*.sav` name without extension, except the default template `XXDefXX`. The row whose full name matches the text of `<save dir>player.lst` (a file of at most 100 bytes) is selected. Verified in the executable. The portable build's save dir is `.\savedata\`.
- **Face digit in the file name**: a save's first character is the player's face and the rest is the name shown. `0Greg.sav` is "Greg" with face 0. Verified: the list's draw routine prints each row from its second character, and the portrait reads the first character minus `'0'`, clamped to 0..8. The face is not stored inside the `.sav`. Confirmed on a real install: a profile the author made in game was saved as `3Greg.sav` with `player.lst` = `3Greg`, and retail shows it as "Greg" with face 3 (`M04`).
- **Sort**: by the shown name, ignoring the digit. A retail screenshot lists "Driver" above "Greg". Case handling, the first-row fallback, the 14-unit row height and white selected text are **guesses** (the last two measured from that screenshot).
- **Portrait** (`NFS5.PLAYERPICTURE`): face `<digit>` from `FEData/Art/ALLdrivr.fsh`: 9 faces of 54x64 (`M01`-`M04`, `F01`-`F04`, helmet `driv` = 8). With an empty list retail shows a `leader` face; not modelled. `SaveData\pic16.fsh` holds one more 54x64 `driv` picture (RGB565). Its role is not traced.
- `ALLdrivr.fsh` faces are 8-bit paletted with palette type `0x29` = 256 RGB565 entries. Added to the reader and checked by eye (natural skin tones, blue backgrounds as in retail).

### Outline and list backing
- `NFS5.GlintyRectangle` is a 1-unit outline in `data.col` (four parts: top, bottom, left, right; from `GlintyRectangle.ctr`). Its `animate` glint is not modelled.
- The list box's `selectionbox` part (at 0,1, width x `selectionboxmaxheight`) has alpha `0x40` and an editor-green colour. A retail screenshot shows a white tint there instead: about `#ACBCBB` inside the list over `#869EA3` panel, roughly 26-31% white. We draw white at the template alpha (25%). **Guess**, measured.

### Bottom bar (`bottombar.lay`, `NFS5.BOTTOMBARBUTTON`)
From `BottomBarButton.ctr`:

| Field | Meaning |
|---|---|
| `posx`, `posy` | Button origin. With `align` bit 1 set, `posx` is the right edge (exit at 635) |
| `shapePathName`, `startframe` | Icon frame (`backbutton.fsh`: back 0, options 11, exit 32; `racebutton.fsh` 0) |
| `picoffy` | Icon drop below `posy` |
| `space` | Gap between icon and label |
| label | `stringID`, Univers 11, white, at icon width + `space`, y + 8. Hover label yellow `0xffffbb00`, disabled grey `0xff808080` |
| `callback` | `GOBACK`, `OPTIONS`, `PLAYGAME`, `QUIT` |
| hover/press frames | Icon animation ranges with tick counts. Not modelled |

- String 0 in `FEStrings.csv` is a placeholder ("Hello, World!"). The RACE button names it, and retail shows no label, so string 0 means no text.
- Label widths come from the `.ffn` font advances (see Fonts below).
- `QUIT` quits at once in our build. Retail may confirm first (**not checked**).
- The cash backdrop and "ACCOUNT" label are career-only (see below).

### Fonts (`.ffn`)
Worked out from the shipped files. Checked by rendering sample lines and the atlas, and by the record count matching the bytes exactly.

| Offset | Field |
|---|---|
| 0 | `FNTF` |
| 4 | u32 file size |
| 8 | u16 (101 in `Univers11b`; meaning unknown) |
| 10 | u16 record count (224 = codes 32..=255) |
| 19 | u8 line height (18 for Univers 11; 0 in `Univers45b`) |
| 20 | u32 records offset (32) |
| 28 | u32 atlas offset |

Each record is 11 bytes: u16 code, u8 width, u8 height, u16 atlas x, u16 atlas y, u8 advance, i8 x offset, i8 y offset (glyph top below the line top). In Univers 11 the baseline is row 12: `A` top 1 + 11, `.` top 10 + 2, and `g`/`p` drop 3 below. The atlas is an image header (format `0x7A`, u16 width and height at +4/+6) followed by 4-bit coverage, two pixels per byte with the **high nibble first** (the other order garbles every glyph).

- `REAL.LABEL` positions its text at `(0 - width) / 2 * align`: 0 left, 1 centred, 2 right.
- `NFS5.GOTOBUTTON` has three label parts chosen by `medium`. 0 = `Universc11b` at 5,2. Bit 1 = `Universc11m` centred. Bit 2 = `univers8b` centred. Bit 4 = `univers8b` at 5,2.
- Label widths now come from the font advances, replacing the earlier 6.5-units-per-character estimate.
- Text is drawn at one pixel per layout unit, then scaled with the window using linear filtering. Retail draws at 640x480 natively, so ours is softer when scaled up.

### Page under a popup, career-only bottom-bar pieces
- Control state flags (from the executable): bit `0x20` is hidden and bit `0x10` is disabled. Each is set only if the control's template lists that state.
- **Account pieces**: the bottom-bar menu shows `ID_CASHBKGD` (the `NFS5.ACCOUNTTHING` cash backdrop) and `ID_ACCOUNTTHINGY` (the "ACCOUNT" label) only when `Game.GameType` is 4 (career). Otherwise it hides them. Verified. We have no career mode, so both stay hidden.
- **Page under a popup**: retail shows the main menu's ring buttons as faint see-through red with ghostly labels while `selectname` is open. The per-channel numbers don't fit a simple fade of the normal red. They do fit the button's **disabled** state: fill `disabledcolor` `0x30FF0000`, labels white at `0x30`, drawn over the 3D room. We draw the page in that state while a popup is open, with no hover or clicks. The disabled bit is set by per-button game logic in several places, and the code that switches the page this way under a popup is not traced (**guess**, matched to the screenshot).

### Observations from a retail recording (2026-10-16)
The author's 43-second screen recording of the retail front end (main menu, single player, quick race, options) was read as still frames. Nothing from it is stored in the repo.
- The main menu opened **without** the select-user popup. `GameSetup.lay` links "load game" to `selectname`, and the author reports the game shows the popup at first launch. We keep it at start-up (`GameSetup+none` starts without it).
- "singleplayer" opens `singleplayer.lay` (`room=ID_NONE`) **over** the main menu, with the ring buttons faint. This independently confirms the popup rule and the disabled-page look.
- The hovered ring button turns yellow (`hovercolor`).
- The centre oval plays a video (`NFS5.MOVIECIRCLE`), with the 3D garage room around it.
- Transition buttons have a vertical shading (light top, dark bottom) and a drop shadow (now drawn; see below).
- On the main menu the bottom bar's "back" and "RACE" are grey, which fits the disabled label colour `0xff808080`. Likely causes: no screen to go back to, and the page header's `canplaygame=0`. Not traced.
- "quick race" slides the buttons off sideways (`transon`/`transoff`), then fades through black into the 3D garage screen with a rotatable, openable car and a glove cursor.
- The single player popup's "difficulty" is a dropdown (`NFS5.ComboBoxSlider`); now drawn closed (see below).

### Transition button shading and drop shadow
From the retail button draw routine (described, not copied). Matched against a frame of the retail recording: a pale top line, shading from light to dark red, and a shadow about 4 pixels down and right.
- **Order**: fill and label (template parts), then the drop shadow, then the gloss. The gloss also tints the label.
- **Gloss**, when the button isn't hovered: over the inner rectangle (inset 1), the top half fades from `0x80FFFFFF` to clear and the bottom half from clear to `0x91000000`. Disabled buttons keep it.
- **`dropshadow=1`** (template default), when not hovered or disabled: offset 4,4. A right strip and a bottom strip run from `0xCC000000` at the button edge to `0x19000000` at the offset, with tapers at `0x0C000000` where each strip starts.
- `dropshadow=2` and the hovered state use different routines. Not drawn yet: our hover only recolours the fill.
- All of these colours are scaled by the control's fade alpha in retail. Transitions are not modelled, so we use full alpha.
- Our draw list is topmost first and each control orders its own pieces. Text is no longer lifted above the rest of its layer.

### Grey bottom-bar buttons and the hover look
- **RACE**: the bottom bar's update reads the top screen's `canplaygame` (header field, read through the same slot the update calls). 0 or empty disables RACE, 1 enables it, and other values defer to the game's own ready check. Verified. `GameSetup` has `canplaygame=0`.
- **back**: disabled when the front end is at its root screen: one menu on the stack with a single history entry. Verified, except for one start-up mode that is not modelled. We use "no history".
- **Disabled bottom-bar look**: label `disabledlabel` `0xff808080`, which matches retail `#858585`. Icon about half brightness in the recording (back arrow `#FF989D` to `#7F4C5E`), so we draw it at alpha 0.5.
- The "is disabled" check also returns true when the control has a disabled state and its menu reports itself inactive. That is very likely how the page under a popup becomes disabled.
- **Hover** (live transition buttons): fill `hovercolor`, no gloss, no drop shadow, and a pressed-in bevel inset 3. Top and left bands go from 80% black at the edge to 10% inside; right and bottom bands from 80% white to 20%. The retail bottom band has two irregular corner positions; we draw it symmetric. Matches the recording's hovered "singleplayer" by eye.
- **Blending space (fixed 2026-10-16)**: retail blends translucent colours in screen (gamma) space; Bevy UI blends in linear light. The front end is now composed in software at 640x480 (`menu_canvas`) with `src * a + dst * (1 - a)` on the stored 8-bit values, then shown as one scaled image. Measured against the recording: grey back arrow retail `#7F4C5E`, ours `#804A4A` (was `#BB6A6B`). Button top `#DF7B78` vs `#D27373`, button bottom `#6C0700` vs `#640000`, shadow below `#162322` vs `#161F23`.

### Dropdown (`NFS5.ComboBoxSlider`) and button outlines
- **Closed dropdown**, from `ComboBox.ctr`: red back rectangle at full size; the control's label (`stringID`) at 5,2 in universC11b, white. The current value is right-aligned to end 13 units before the right edge, in universC11m, dark (template alpha 0, shown solid in retail). The arrow is `weeicons.fsh` #14 (`BarD`, 10x7) at 4 from the right, 4 down.
- **Gloss, shadow and outline** match transition buttons in the retail recording, although the combo class doesn't call those routines itself (**source not traced**).
- **Difficulty values**: strings 450-452 ("beginner", "advanced", "expert") are consecutive in the locale table and fit the executable's localized-text list for `Game.Difficutly` (the game's own spelling). The list's construction is not traced. The selected value is stored per player and not read yet, so we show "advanced" as in the recording (**guess**). Opening the list is not done yet.
- **Transition button outline**: unless `glintybox` is -1 (template default 1; an empty value reads as 0), the retail button makes a black (`0xFF000000`) 1-unit outline box its own size and draws it last. It also draws two black 1-unit stems whenever the outline exists, even at height 0: at the left edge (button height + `stemleftheight`) and just right of the right edge (x + w, button height + `stemrightheight`). Black measured in the recording (`load game` stem `#000607`). The outline's `animate` glint is not drawn.

### Centre movie (`NFS5.MOVIECIRCLE`)
- **Placement**: `GameSetup.lay` puts it at 112,100, 416x348, in state `test1`, with `othersource=demoloop`, `audio=0`, `tint=0xFFFFFFFF`. In `test1` the template draws a black rectangle and eight `movieoval.fsh` pieces (#0/#1 in four flips) as a stencil. That is how the video is cut to the oval.
- **Source selection** (executable, movie-box class): `othersource=demoloop` plays a looping playlist built from `<movies dir><name>.mad`.
  - Slot 0 is `ch6` (read from the executable's data section, `0x5d8b74`).
  - Slots 1-4 are `er1pc-av` .. `er4pc-av`, included as the profile's progress reaches each era (three progress thresholds, `0x209`-`0x20b`, plus a separate check for slot 1).
  - `careerfirst` uses `er1pc-av`. The chronicle screens choose their own clip.
- **Files**: `FEData\Movies\*.mad`: `ch1`-`ch6`, `er1pc-av`-`er4pc-av`, `inPC-av` (intro), `EArts-av` (logo). Headers start with the `MADk` chunk tag, EA's "Madcow" video. FFmpeg's EA demuxer and `eamad` decoder support this format, which is the basis for our decoding plan. `-av` files carry interleaved audio. Not decoded by us yet.
- **Decode check** (2026-10-16, ShareX's FFmpeg run read-only on the author's copy; frames kept outside the repo): every `.mad` tested decodes as `eamad` video, 192x144 at 15.15 fps.
  - `ch6` has no audio: 1,449 frames, about 96 s. It is a black-and-white Porsche history montage that ends in colour (996 Turbo, Porsche crest).
  - `er1pc-av`..`er4pc-av` add EA ADPCM (R1) stereo audio at 22,050 Hz, which the menu doesn't use (`audio=0`).
- Both retail recordings play `ch6` (frame matching with `mad_match`, scores 0.95-1.00). An earlier by-eye match to `er2pc-av` was wrong: `ch6`'s colour section has a similar red 914 shot.
- The "NEED FOR SPEED" lettering and grid lines over the video are not in the clips. Something in the front end draws them on top (**not traced**).

### `.mad` decoder (ours) and movie playback
Our decoder (`nfspu_formats::mad`) is written from the format's structure. FFmpeg served only as a black-box reference: its raw YUV output was compared frame by frame.
- **Chunks**: `tag` + u32 LE size (header included). `MADk` is a key frame; `MADm`/`MADe` may copy from the previous frame; other tags (audio) are skipped. `ch6.mad`: 290 `MADk` + 1,159 `MADm` = 1,449 frames, a key frame every 5.
- **Chunk header**: u16 frame time in ms at 14 (66 = 15.15 fps), u16 width at 16 and height at 18 (192x144), u8 quantiser scale at 21. Bits start at 24, as 16-bit little-endian words read from the top bit down.
- **Macroblocks**: 16x16 in raster order, six 8x8 blocks each (4 luma, Cb, Cr; 4:2:0). In inter frames a prefix chooses: `1` = all six blocks copied, `01` = a 6-bit copy mask, `00` = none. Then one motion vector, each component `0` = 0, else `1`, a sign-ish bit and 4 bits (value + 1, or - 16 + value when negative). A copied block is full-pixel (chroma vector halved toward zero) plus 2 x a per-block motion-coded constant. A source outside the picture copies nothing.
- **Intra blocks**: DC is 8-bit signed around 128. AC uses the MPEG-1 coefficient codes (ISO/IEC 11172-2 Table B.5c; escape = signed 10-bit level, then 6-bit run - 1). Dequantisation follows MPEG-1 intra with the default intra matrix. It is done in the scaled-DCT domain (steps = matrix x scale x round(4096 / (s(u)s(v))) / 1024, then `>> 4`, made odd), because the reference rounds there.
- **Result vs FFmpeg** on `ch6.mad`: all 1,449 frames decode; worst sample difference 2 of 255; mean PSNR 55.5 dB; frame 0 exact. The rest is inverse-DCT rounding (ours is exact floating point; FFmpeg uses EA's integer transform). Run: `cargo run -p nfspu-formats --example mad_check --release -- <file.mad> <ref.yuv>` writes `logs/mad_check.txt`.
- **Playback**: the movie circle draws the current frame scaled (bilinear) into its 416x348 rectangle, only inside its stencil (the centre rectangle plus the opaque pixels of the eight `movieoval.fsh` pieces). The front end advances movies on real time and recomposes per frame: about 20 ms per recompose in a debug build (worst 41 ms), so 15 fps holds. `demoloop` plays `ch6` only, because era progress is not read (**guess** for unlocked profiles). BT.601 full-range YUV to RGB (matched to retail).
- Era clips follow the selected profile's career progress (see the demo loop notes below).

### Movie circle overlay and brightness (2026-10-16)
- **Brightness** (corrected 2026-10-16): the movie is not dimmed. Retail shows luma as stored (**full-range** BT.601, no 16-235 stretch): versus `ch6` frame 848 (the right pairing), luma average 88.7 retail vs 87.8 ours, percentiles within 3 levels. The first comparison below used a wrong pairing (`er2` frame 162). The movie box draws it with the control's `tint` (field `+0x120`, loaded from `data.tint`; the layout sets `0xFFFFFFFF`), not with `color`. Retail frame 4 of the recording vs our `er2pc-av` frame 162 (same shot): luma average 89 vs 79, 98th percentile 189 vs 163. That is close, with retail if anything slightly more contrasty (recording compression and frame timing allow for that). The "dim" look came from the translucent overlay and the fades between clips.
- **Draw order** (movie circle class): write the stencil (rectangle + eight `movieoval.fsh` pieces), restrict drawing to it, draw the movie, then draw the overlay, all inside the oval.
- **Clock**: the front end's timer (`FUN_0053c2e0`) returns a tick counter advanced by a timer thread that the game starts at 0x80 = **128 Hz**, so all overlay timings are in 1/128 s ticks. The first version read them as milliseconds and ran about 7.8x too fast (reported by the author).
- **Overlay** (verified structure; we use our own random source; timings in ticks):
  - **Lines**: 3 horizontal and 3 vertical, full width/height of the movie rectangle, colour `0x4CFFFFFF`. Each moves 1 pixel every 5-9 ticks (14-26 px/s) in a random direction and bounces. Retail also forces at least 1 pixel per drawn frame, which ties speed to frame rate; we move by the tick rate only. After a 0-127 tick wait a streak a fifth of the span long runs along it at 3-5 px/tick (380-640 px/s), then the wait repeats.
  - **Markers**: 3. After 0-511 ticks (up to 4 s) each plays `circles.fsh` frames 0-4 (8 to 128 px, 16 ticks = 125 ms each), centred where a random horizontal line crosses a random vertical one, colour `0x30FFFFFF`.
  - **Lettering**: 4. After 0-383 ticks (up to 3 s) each shows "NEED FOR SPEED" for 256-511 ticks (2-4 s). Font random (Univers 7 or 11 bold), direction random (across, or letters stacked down), anchor random (start/centre/end), point random in the rectangle, colour `0xA0FFFFFF`. Letters are placed one by one with a spacing factor that is 1 for the first 20% of the time and then changes.
  - In a game mode not identified yet, the colours switch to amber (`0x26/0x50/0x18 FAB400`). Not modelled.
- **Approximations** (the float maths did not decompile): spacing grows from 1 to 3 over the last 80%, alpha fades to 0 over the same span, and the streak's bright end is 50% white.
- A clip-timing oddity: our decoder puts the 914 head-on shot at `er2pc-av` frames 159-165 (about 10.7 s). An earlier FFmpeg contact sheet suggested about 10 s, but that sheet's frame selection was not checked. Not investigated.

### Demo loop and career progress (2026-10-16)
- **Rule** (executable): the highest playlist slot is 1 if the flag `DAT_00691dd5` is set, raised to 2 / 3 / 4 when the career year reaches 1970 / 1989 / 2000. The thresholds are literal in the threshold helper. Slots: `ch6`, `er1pc-av`, `er2pc-av`, `er3pc-av`, `er4pc-av`. When a clip ends the loop advances, and it wraps to slot 0 past the highest slot.
- **Save layout**: a run of named sections (`Missions`, `PartsMarket`, `Tournament`, `PlayerInfo`, `TrackRecords`, ...) whose layouts differ. The career-section loader reads the year as the first 32-bit value of `Tournament`; the executable's own starting value is 1950. The player-info loader reads the flag as a byte. By comparing three saves, the byte straight after `PlayerInfo\0` is the likely flag (**likely**).
- The year rises through a year table when race results are processed (the race-results code calls the year step).
- The apparent contradiction with a retail recording was a wrong clip match; resolved, see `STATUS.md`.

### Difficulty dropdown, working (2026-10-16)
- **Save values**: settings are stored as `accessor name` + zero byte + u32 LE (for example `Game.Difficutly`, `Game.GameType`). Found by comparing three saves. `Game.Difficutly`: Greg 0, Driver 1, default profile 1. The index order 0 beginner / 1 advanced / 2 expert is confirmed by a retail recording (see below).
- **Behaviour** (combo box class): a click on the closed control (right of its label) opens it with a sound. The list slides open over 16 ticks (125 ms) to `selectionboxmaxheight`, in the direction of `selectionboxslidedirection`. A click on an item sets the value (sound) and closes; a click elsewhere closes (sound) and is passed on.
- **Ours**: click toggles, option rows (Univers8b) with the current one red, hover yellow, a click picks and closes, any other click closes then acts. The choice is session-only (not written to the save). **Provisional look**: list below the control, rows like the user list, no slide animation, no sounds. Which way `-1` slides and the real row styling are **not verified**; a retail recording of the open dropdown would settle both.
- Capture helper: `NFSPU_MENU_COMBO=Game.Difficutly` starts with the dropdown open.

### Difficulty dropdown matched to retail (2026-10-16)
Retail recording of the dropdown with Greg selected:
- **Value order confirmed**: Greg showed "beginner" with `Game.Difficutly` = 0. After the author picked "advanced", retail rewrote `3Greg.sav` (21:14:53, after the 21:13:35 recording) with value 1. So 0 beginner, 1 advanced, 2 expert, and retail saves the choice at once. Ours does not write the save (read-only install).
- **Hover**: the whole control turns yellow (0xFFBB00), and stays yellow while open. A black divider follows the label in every state (position measured; source not traced). A tooltip with the control's `helptext` appears after resting on it (not done).
- **Open list**: opens **downwards** from directly under the control. Rows are 14 units, flat yellow (~#E9B819), with a black outline at the control's left/right edges and a drop shadow. The hovered row is red (0xBC0000) with white text; other rows are black text in Univers8b. The current value is not marked.
- **Slide**: done. The list is revealed downwards over 16 ticks (125 ms) on open and hidden the same way on close; rows take clicks only when fully open (`Draw::Clipped`).
- **Sounds**: open 2, close 3, choice 5 from `fesfx.bnk` (see `docs/provenance/sound.md`).
- **Tooltips**: done, see Help tips below.

### Help tips (`helptext`) (2026-10-17)
15 control templates declare a `helptext` field (INT, default 0), and the shared control code reads it for every control; 269 layout controls set a non-zero id. All 24 ids on the main menu, `selectname`, `singleplayer` and `QuickRace` are `!helptext` rows in `festrings.csv` (1000-1011, 1155-1167, 1258, 1283, 1309). The bottom bar sets none.

**Behaviour** (retail executable: the shared control load stores the id, and the shared control draw step shows the tip; described, not copied):

| What | Value |
|---|---|
| When | the control is hovered, not disabled, not hidden, and no screen transition is running. Otherwise the hover clock restarts every frame |
| Delay | 128 ticks of the 128 Hz front-end clock = **1 s** of continuous hover |
| Fade in | alpha = `min(255, (ticks - 128) * 4)`: full after another 64 ticks (0.5 s) |
| Font | `Univers8b.ffn` (loaded once by the front end for this) |
| Wrap | greedy at spaces; a line must measure **less than 310** pixels. Line step = measured text height + 2 |
| Box | text width + 8 by text height + 4 |
| Place | bottom-left corner at the pointer when the tip first appears (fixed while shown). If `x + width + 8 > 640` then `x = 636 - width`; the top is clamped to 0 |
| Fill | white, alpha `alpha * 0xC0 / 255` (75% when fully shown) |
| Outline | 1-pixel black at `alpha`, drawn as four strips (they overlap at the corners) |
| Text | black at `alpha`, 4 right and 2 down from the box corner |

**Checked against retail footage** (the let's play linked from the coordinating notes, 854x480, in which the 640x480 front end is 1:1 vertically; frames read in the browser, nothing saved):
- At 2:30, "continue" on `selectname` shows "Select player profile and continue playing" in a box at about 337,387, 242x17: black text on a pale box with a dark 1-pixel edge, just above-right of the glove pointer. 17 = 13 + 4 fits one Univers8b line (11 + 2).
- The fill is see-through: over the yellow button it is `#FFF2C9`, over grey-green `#CFD1D0`. Fitted alpha 0.77; the code says 0.75.
- Timing, sampled every 0.1 s: the button turns yellow at 148.15 s, the tip starts to show at about 149.25 s and is complete by 149.7 s. That fits 1 s + 0.5 s.
- Size check with our Univers8b metrics: the width comes out at exactly **242** (text 234 + 8). The height needs a text height of 11, but the font's line height field is 13 (box 19). 11 is the glyph extent of that line: capitals start at row 1, descenders end at row 12. We measure each line's glyph extent (**guess**: the retail measure routine is not traced; "line height - 2" would also fit this one frame). Retail does keep a separate height per line while wrapping and steps lines by the last one's, which suggests a per-string measure.
- The author's two recordings do not show a tip: the bottom of the game window is covered by the recorder's own panel.

**Ours** (`crates/nfspu-game/src/menu_help.rs`):
- `menu_help::target` reads a control's `helptext` while the screen is built. Its hover area is the largest rectangle or click target the control drew (a button also draws 1-unit outline strips). A page drawn disabled (under a popup) gets no tips.
- `HelpTimer` keeps the hover clock: same layer and same help, or it restarts. `tooltip` turns text, ticks and anchor into draw commands.
- The front end draws the tip as an extra non-interactive layer on top of everything and recomposes while it fades.
- `cargo run -p nfspu-game --example help_audit -j 2` lists every tip and its box (`logs/help_audit.txt`).
- Capture: `NFSPU_MENU_CAPTURE_SECS=2` waits long enough for a tip to fade in under `NFSPU_MENU_POINTER`.
- Capture checks (2026-10-17, `NFSPU_MENU_CAPTURE_SECS=3`): `GameSetup+selectname` with the pointer at 337,403 on "continue" matches the retail frame by eye (same text, place, see-through fill over the yellow button, dark edge). Also checked: main menu "singleplayer", the `singleplayer` popup's "quick knockout" (two lines), and `QuickRace` "settings" (two lines).
- Data quirk, not ours: the main menu's "singleplayer" button carries string 1007, "Play a quick race" in English. The German text for 1007 describes all three single-player modes.
- **Not modelled**: suppression during screen transitions (we have no transitions yet). The hover test uses our drawn rectangle, which may differ slightly from the retail control's hit area (**guess**).

## Quick race (2026-10-17)

Sources: the shipped layouts and catalogue files, the retail executable's accessor and list
classes (read-only reference, described here in our own words), and the author's retail
recording of 2026-10-16 22:40 (quick race with Greg, read as still frames; nothing stored).

### Flow
| Screen | Layout | Header | Reached from |
|---|---|---|---|
| main menu | `GameSetup.lay` | `ID_MOVIEOVAL` frame, `ID_GARAGE` room, `canplaygame=0` | start |
| single player | `singleplayer.lay` | popup (`room=ID_NONE`) | "single player" |
| quick race | `QuickRace.lay` | no frame, `ID_GARAGE` room, `canplaygame=1` | "quick race..." (`callback=SETTYPEQUICKRACE`) |
| location setup | `location.lay` | no frame, `ID_TRACK` room, `canplaygame=1` | "location setup..." |
| settings | `settings.lay` | popup, `canplaygame` empty (RACE grey) | "settings" |
| car compare | `carcompare.lay` | popup (`ID_FULLOVAL`) | "car compare..." |
| garage | `garage.lay` | `ID_GARAGE` room | "garage..." (owned car only) |

- RACE is the bottom bar's `PLAYGAME` button, live while the top screen's `canplaygame` is 1. The
  location screen also has `canplaygame=1`, so RACE works there too.
- The recording shows the bottom bar on the quick-race page although it has no frame. We draw
  the bar on every page that has a room (`ID_GARAGE`, `ID_TRACK`); beyond quick race this is a
  **guess**.
- `SETTYPEQUICKRACE` sets `Game.GameType` to 0, rebuilds the car and track lists and resets
  `zMergedList.Car` to the first row. The recording agrees: Greg's save holds car 72, yet quick
  race opened on his owned 356.

### Dropdowns and what they list
| Control (accessor) | Rows | Closed shows | Save value (Greg) |
|---|---|---|---|
| car (`zMergedList.Car`) | "OWNED" cars, then "FACTORY STOCK", each grouped by era and class titles | the car's short name (`356`, `356 B`) | 72 (not used, see above) |
| location (`Track1.Track`) | the open tracks' full names | the short name (string + 40) | 0 |
| opponents (`OpponentCar.Car`) | same car, same era, same class, same era/class, random car, then every installed road car | "era/class" for the fourth rule; a car's short name | 0 |
| opponents (`Game.NumOpponents`) | 0..7 (number accessor, default 4) | the number | 7 |
| direction (`Track1.Direction`) | forward, backward, forward/mirrored, backward/mirrored (410-413) | forward, bkw, fwd/mir, bkw/mir (410, 416-418) | 2 |
| traffic (`Track1.Traffic`) | none, light, medium, heavy (403-400) | same | 2 |
| laps (`Track1.Laps`) | 1..8 (420-427) | same | 1 (two laps) |
| transmission (`Car.Transmission`) | manual, automatic (560-561) | same | 1 |
| ABS, traction control (`Car.ABS`, `Car.Traction`) | off, on (1500-1501) | same | 1, 0 |
| roof (`Car.Roof`) | up, down (1509-1510) | same | 0 |
| damage (`Game.Damage`) | off, on (705, 704) | same | 0 |
| color (`GenericCar.Colour`, `NFS5.ColorBox`) | the model's paints (`.clr` sections) | a swatch | 20 |

- Direction items 2 and 3 are "forward/mirrored" and "backward/mirrored" (410/412, strings 410-413).
  The mirror flag reaches the race as `NFSPU_RACE=forward_mirrored` (or `backward_mirrored`) and as
  `,mirror` on `NFSPU_LOADING`; `docs/provenance/race.md` describes what it mirrors.
- The `(SHORT)` suffix in a layout's accessor picks the short text for the closed control.
- Save values are the accessor name, a zero byte and a `u32`, as for the difficulty. Read-only;
  choices made in our menu are not written back.
- **Garage button or colour box**: `QuickRace.lay` places "garage..." (`ID_MERGEDLISTCARSETUP`) and
  the colour box (`ID_MERGEDLISTCOLOR`) at the same spot. The recording shows "garage..." with
  the owned car and "color" with a factory car. The switching code is not traced.

### `nfs5.car` (0x670-byte records, 109 in this install)
| Offset | Field |
|---|---|
| 0 | `u16` id |
| 2 | full name, `'50 356 1100 Cabriolet` |
| 0x34 | short name (`356`, `356 B`), shown in the closed car control |
| 0x4d | simulation file stem |
| 0x7f | model stem in `GameData/CarModel` (`356_1`); also the `.clr` paint file |
| 0x147 | logo archive (`finallogos1`/`finallogos2`) |
| 0x179 | engine sound bank |
| 0x1ac | `u32` class string (501 CLASS 3, 502 CLASS 2, 503 CLASS 1) |
| 0x1b0 | `u32` era string (521 CLASSIC ERA, 522 GOLDEN ERA, 523 MODERN ERA) |
| 0x1bf | `u8` 1 = open car (cabriolet, roadster, speedster, convertible): the roof setting applies |
| 0x1c0 | `u8` body style, 0-based: style `n + 1` of the model's `.tpg` (`[styleN] name=`). Matches the style name for all 84 quick-race cars (356 Cabriolet 1 = "Cabrio", 901 RS 6 = "carrera 2.7", 993 Cabriolet 5 = "Carreracabrio") |
| 0x1f4 | `u16` kind: 0 road car, 1 traffic, 2 police |
| 0x1f6 | `u16` logo number n: archive entries 2n and 2n+1 are its left and right halves ("PORS" + "CHE" for 4; the recording shows "1600 SUPER" for 19) |
| 0x1f8, 0x202 | `u16` first and last model year |
| 0x204 | `u16` availability: 0/2 listed, 1 hidden, 3 never (the three factory-driver prize cars) |
| 0x208 | price (11000 for the first 356) |

- **Factory stock** outside career: kind 0 with availability 0 or 2 (84 cars here). The list is
  sorted by era, then class, then first year, with a simple exchange sort (each slot swapped
  with any later car that sorts before it). We do the same. The comparison's argument order
  comes from a decompile that lost the `this` register, so "ascending" is inferred; the
  recording's first rows ('50 356 1100 Cabriolet, '50 356 1100 Coupé Ferdinand, '51 356 1300
  Cabriolet, '51 356 1300 Coupé) agree.
- **Owned cars**: the save's `PlayerInfo` section holds a copy of each owned car's record (id,
  then name). We find them by that pattern; Greg's one 356 is found. The record starts at the id
  and is 0x670 bytes, the shape `nfs5.car` uses (checked byte for byte on Greg's car).
  `u16` at **0x3e6** is the car's paint: the value of the engine's `OwnedCar.ExtColour` accessor
  (getter `0x4f0e20`, a `short`). Greg's 356 stores 19, and section 19 is the orange retail shows,
  so we take it as the `[sectionN]` number (see below). Rest of the section layout is read only
  as far as the pattern needs.
- **Opponent cars**: every kind-0 car whose model file is installed, same sort (87 here).
- **List rows** (retail list drawing): a title row (13 units) whenever the era changes and
  whenever the class changes, plus the "OWNED"/"FACTORY STOCK" title before each part. Item rows
  are 14 units: the first three characters of the name (the year) in a white column, the name
  from the fifth character, 22 units in. The owned row adds a paint swatch and a divider.

### `nfs5.trk` (0x4a8-byte records, 15 here) and `nfs5.trn` (0xd40-byte records, 35)
| Offset | Track field |
|---|---|
| 0 | `i16` id |
| 4 | `u32` name string (full); short = +40, capitals = +60 |
| 8 | file stem in `GameData/Track` |
| 0x30, 0x44, 0x58 | `FEData/TrackArt` pictures (main, `Sp`, `SpSm`) |
| 0x6c | `i16` 3 = locked until a tournament that uses it opens |
| 0x6e | `i16` 1 = in the looped (test-drive) list |
| 0x78 | `i16` difficulty (0-2, strings 450-452) |
| 0x1ac.. | best-time table: 0x4c-byte entries of driver, car, `u32` time |

- **Track list rule**: every record whose `<file>.crp` exists in `GameData/Track`, except the skid
  pad. A record with 0x6c = 3 is included only when a tournament that lists it is open:
  tournament era code (at 4) 521/522/523 opens in career year 1970/1989/2000; other codes are
  open from the start. Track ids sit at 12 (count at 10). In game type 3 every track is open
  (not used here). List order is file order.
- With a new profile (career year 1950) quick race offers Côte d'Azur, Pyrénées, Normandie and
  Monte Carlo 3. 1970 adds Schwarzwald, Zone Industrielle, Monte Carlo 1 and 2; 1989 adds Alps,
  Autobahn, Auvergne, Monte Carlo 4 and 5, Corsica.
- The save keeps its own tournament table, which we do not read; we use `nfs5.trn` with the
  save's career year. **Not checked** against a later career.
- **Record time** (location screen): the first entry's time, a `u32` in 1/128 s ticks (the front
  end clock), shown as `mm:ss.hh`. Which entry and the exact format are **guesses**.

### `.clr` paints
#### Menu-to-race correction (2026-10-19)
The dropdown index is a file-order position, while the renderer takes the actual number in
`[sectionN]`. Verified in the installed `356_1.clr`: sections start 1, 2, 3, 5, 4, so adding
one to the index is insufficient. The catalogue now keeps the section with each swatch;
garage preview and `RaceSetup.paint` use that same mapping. The race launcher passes the
section as the existing fifth argument. Previously it omitted paint entirely, leaving the
player in default section 4. Opponents now choose an existing section rather than treating
a random list position as a section number.

This fixes an internal hand-off discrepancy, not the unresolved owned-save paint format.
Old `logs/race_setup.txt` files should be regenerated by pressing RACE in the menu.
Full live menu-to-race colour agreement is **not tested**.

#### The owned car's paint in the save (2026-10-19)
An owned car's paint is kept in its saved catalogue copy, `u16` at **0x3e6** (the engine's
`OwnedCar.ExtColour`, accessor 0x4f1ea0; the getter 0x4f0e20 returns a `short`). We verified it
against Greg's save: the owned 356 stores **19**, section 19 is orange (255, 114, 0) in
`356_1.clr`, and retail shows the car orange. The same value is used for the owned row's swatch
and as the quick-race/garage paint, and so reaches `RaceSetup.paint`.

- `logs/race_setup.txt` now reads `paint = 19, owned_car = true` for a menu RACE with the owned
  car selected; the capture `logs/menu_quickrace.png` shows the orange car in the garage room.
- The save may also hold the value as a 1-based list position instead: 19 and 1 are the only
  values in this install's records, and both readings agree for section 19 (the `.clr` file's 5/4
  swap is behind it). We prefer `[sectionN]`, then fall back to a 1-based position.
- **Not resolved**: the colour table next to the field (dwords at 0x3e8..0x3e8+0x140 and an
  interior field at 0x528). Its entries hold per-paint colours, but their order is not the file's
  and two entries' body colours differ from the `.clr`'s, so what fills it is not traced.

Plain text, one `[sectionN]` per paint scheme, with `color1..4` as both HSV (`.h .s .v`) and RGB
(`.r .g .b`). `color1` is the body colour. How retail lays out its colour picker
(`selectionboxwidth=0`, `allowcustom=1`) is not traced; we use a plain list of swatches.

### Drawing (ours)
- Dropdowns list any rows (titles and items), scroll when taller than `selectionboxmaxheight`
  (arrows, plus the mouse wheel, which is our addition), and open on top of every other
  control. The open car list in the recording is 262 wide (`selectionboxwidth`) with a scroll bar
  of about 12 units.
- `NFS5.CarLogo` draws the car's logo (both halves) at the control's position: `align=1` centres it
  there (settings popup), `align=0` starts it there (quick race page). Both match the recording.
- Under a popup the page's dropdowns fade like its buttons (fill `0x30FF0000`, faint text), as in
  the recording.
- **Not done**: retail shows "N/A" (and a faded control) for ABS, traction control and roof on cars
  without them (a 1600 Super in the recording); where that comes from is not traced. `NFS5.AccessorWatch` draws
  its accessor's text (track name, difficulty, record time) in the layout's font and colour.
- The location arrows are bottom-bar buttons (`SPINArrw.fsh`, `flip=1` on the right one) with
  callbacks `TRACK1INC`/`TRACK1DEC`; they step through the tracks and wrap.
- **Gaps**: the 3D garage car (`NFS5.3DCAR`) and the track room (`ID_TRACK`) are not drawn. As a
  stand-in the location screen shows the chosen track's `TrackArt` picture. The garage, car
  compare and track records screens open, but their special controls are not drawn.

### The picked car in the race (2026-10-19)
- Playtest of 2026-10-19: RACE wrote `car_model = 356_1, paint = 20`, but the race showed the
  356 as a coupé in the default paint. The race read neither style nor paint from the setup.
- Now one value, `QuickRace::car_look()` (model, `.clr` section, `.tpg` style), feeds both the
  garage car and `RaceSetup` (`paint_section`, `car_style`). The race builds the player car from
  `RaceSetup::car_options()` and the physics from `car_id` (its `.sim`).
- Body style comes from the record (byte 0x1c0), replacing the earlier name matching. The
  retail car builder takes a 0-based style number and opens `[style<n+1>]` of the `.tpg`
  (Ghidra); that the number is this byte is shown by the data (all 84 cars match by name).
- Opponents now also use their record's style, and a random paint by `.clr` section number (the
  list index used before was off by one, and 0 means unpainted).
- Roof up/down: drawn (see `car.md`, "Roof up and down").

### RACE hand-off
Pressing RACE builds a `RaceSetup` (`crates/nfspu-game/src/race_setup.rs`), logs it to
`logs/m0.log`, writes it to `logs/race_setup.txt` as `key = value` lines (a log), and hands it to
the race in memory: the same program moves on to the loading screen (`docs/DESIGN.md`, "One
program for menu and race"). `NFSPU_MENU_PLAY=1` with `NFSPU_MENU_CAPTURE=1` presses RACE after
the capture, for checks without a human at the window. `NFSPU_MENU_PICK=zMergedList.Car=5;Track1.Track=2` makes choices before the first frame.

### Screen transitions (`transon` / `transoff`, 2026-10-20)
From the retail executable (Ghidra, read-only; described, not copied):
- **Words**: each control reads `data.transon` and `data.transoff` into a bit set: `same` 1,
  `left` 2, `right` 4, `top` 8, `bottom` 16, `fade` 32 (case ignored; anything else 0). `SAME` on
  one side takes the other side's value; both `SAME` means no transition.
- **Per menu**: every menu (page, popup, bottom bar) has a phase (0 gone, 1 coming on, 2 shown,
  3 going off) and a progress 0..512. Starting either way first waits 3 frames, then the
  progress runs from the start time on the 128 Hz front-end clock: `p = elapsed * 512 / (rate / 2)`,
  so a full run is 64 ticks = **0.5 s**. Coming on shows `512 - p` (down to 0, then "shown");
  going off shows `p` (up to 512, then "gone").
- **Per control**: while its menu is coming on it uses `transon`, while going off `transoff`
  (a control in a sub-menu follows its parent menu's phase). Left/right move it by
  `640 * progress / 512`, top/bottom by `480 * progress / 512`; `fade` sets its alpha to
  `255 - 255 * progress / 512`. A global switch (`FEFADESENABLED` area) can turn all of this off.
- **Order** (retail recording 2026-10-16 and the code): the old screen's controls go off, then
  the new screen's come on (singleplayer → quick race: the ring buttons slide off sideways, then
  the quick-race buttons slide in).

Ours (`menu.rs` `transitions`/`trans_offset`, `menu_canvas::compose`, `menu_mode` `Transition`):
- Each control's draw range keeps its (on, off) bits. On a screen change, layers whose screen is
  gone go off (drawn from the picture before the change), then new layers come on; a page that
  stays under a new popup, and the bottom bar, do not move. The front end also comes on at start.
- Input waits while a change runs (no clicks, no hover). `NFSPU_MENU_NOTRANS=1` turns it off.
- **Not modelled**: the room behind (the 3D garage) does not fade; the fade through black seen
  between single player and quick race in the recording is not traced; per-menu timing
  differences beyond the shared 0.5 s.

### Bottom-bar hover animation (`NFS5.BOTTOMBARBUTTON`, 2026-10-20)
The animating-button class reads three frame sets from the layout: idle (`startframe`,
`endframe`, `totalticks`), hover (`hover...`) and mouse-down (`hovermousedown...`, used when
`seperatehovermousedown=1`), each with `mindelay`/`maxdelay`. It shows the mouse-down set while
pressed over the button, the hover set while over it, else the idle set: frame
`start + elapsed * (end - start) / ticks`, restarting after `ticks` (all bottom-bar delays are 0,
so the runs loop without a pause). The run timer is shared by the three sets, so a hover run does
not start from its first frame.

| Button | Idle | Hover | Mouse down |
|---|---|---|---|
| back (`backbutton.fsh`) | 0 | 0..9 in 64 ticks | 10 |
| options | 11 | 11..30 in 128 ticks | 31 |
| RACE (`racebutton.fsh`) | 0..29 in 128 ticks (always moving) | 30..59 in 64 ticks | 30..59 in 64 ticks |
| exit | 32 | 32..51 in 128 ticks | 52 |

- The label turns `hoverlabel` yellow (0xffffbb00) under the pointer.
- Ours runs every icon on one front-end clock (so runs are in step); the random pause between
  runs (`mindelay`/`maxdelay`) is not modelled, as the bottom bar does not use it. A grey button
  holds its first frame. **Not checked** frame by frame against a retail recording.

### Open dropdown lists catch the pointer (2026-10-20)
- Playtest: clicks on an open list's section titles, scroll-bar track or edge went through to
  the buttons underneath (only item rows took clicks). Now the whole list is a click target
  (`Hit::Inert` where nothing is choosable), and nothing under it lights up, plays its hover
  sound or shows its tip while the pointer is over the list. Matches retail, where the list is a
  window over the page (by eye in the recordings).

## What is still open

* What each `[part.*]` and `flags=` token renders as, and how `callback`/`callbacklist` hook into
  the game's script (all shipped layouts leave them empty).
* The `.ffn` font format, and how `Art/*.fsh` pages are addressed by a control (`texture=`).
* How a `.lay` screen is *navigated*: `destination=` names a screen, but the transition, the
  modal/oval screen variants and the `Screen*Oval` family need the retail build to compare
  against.

### Dropdown input interception (2026-10-20 resume)
The user reported clicks reaching controls behind an open list. Claude's preserved menu
worktree patch adds an inert target after the list's rows and arrows, so headers, borders
and scrollbar gaps consume clicks without choosing a value. While the pointer is over a
list, underlying hover sounds, help tips and hover artwork are suppressed. This follows
the reported interaction requirement; exact retail transition animation is not established.

Review found that filtering hidden draws before numbering the hover cache could reuse a
key for a different control. The resumed patch numbers all flattened draws first; a new
regression covers moving from a button into a list row. Runtime behaviour is **not tested**
until the restricted-environment build succeeds and the menu is playtested.

During the opening/closing slide, the inert target now follows only the revealed list
body. A regression places a clickable control behind it and checks that the visible
portion absorbs the click while the unrevealed portion still reaches that control.
Build environment: the configured external sccache executable was denied; validation
uses `cargo --config logs/build_without_cache.toml` with an empty rustc wrapper.
This configuration lives only in ignored logs; system configuration is unchanged.
