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
- **Not done**: tooltips.

## What is still open

* What each `[part.*]` and `flags=` token renders as, and how `callback`/`callbacklist` hook into
  the game's script (all shipped layouts leave them empty).
* The `.ffn` font format, and how `Art/*.fsh` pages are addressed by a control (`texture=`).
* How a `.lay` screen is *navigated*: `destination=` names a screen, but the transition, the
  modal/oval screen variants and the `Screen*Oval` family need the retail build to compare
  against.
