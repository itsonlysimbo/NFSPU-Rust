# NFS5: Porsche Unleashed — reverse engineering + Rust rewrite: project summary

Written 2026-10-15. Covers everything done in this session, where each artefact lives, and what is
still open. Three separate things were produced, and they live in three places:

| # | What | Where |
|---|------|-------|
| 1 | Reverse-engineering report on the game and its compatibility patch | `C:\Users\CB\Downloads\NFS5-REVERSE-ENGINEERING.md` (38 KB) |
| 2 | Full deconstruction of the patch + live-verification dumps | `C:\Users\CB\Desktop\NFS5 Decomp\patch\` |
| 3 | The from-scratch Rust/Bevy rewrite | `C:\Users\CB\Desktop\NFSPU-Rust\` |

Nothing was written into the game install or the decomp folder; they stayed read-only throughout.
No git commits were made.

---

## 1. What was asked

1. Reverse engineer Need for Speed 5: Porsche Unleashed at `C:\Users\CB\Downloads\NFS5`.
2. Deconstruct the compatibility patch bundled with it.
3. Assume the quarantined `RegSetup.exe` is malicious.
4. Reverse the two remaining hook handlers so the game could be rewritten.
5. Use a running game to resolve what static analysis could not.
6. Implement the findings in `C:\Users\CB\Desktop\NFSPU-Rust`, milestone by milestone, starting
   with menus (M6), with retail accuracy as the goal rather than a tech demo.

---

## 2. The game and the patch (findings)

### Identities
* Base game `Porsche.exe`: SHA-256 `e4684948ca9bfb552a0fc0eeb7024a13bfe67f2e40ebb6ba121ae6a81dd166bb`,
  built 2000-06-05, already the community "Enhanced 3.5" build (the 4 add-on cars are present).
* Patch "NFS5 Combined Patch v1.1": `nfs5.exe` (product version **1.0.6**, built **2019-06-21**,
  SHA-256 `c318af39…c94906`) plus **one** helper DLL `nfs5.dll` (20,480 bytes).
* Bundle: Verok's NFSV Patch 1.0.6 + NFS5 Essentials + Enhanced Patch 3.5.20040310 + Smooth Shines
  and Lights + data tweaks.

### The key structural finding
`nfs5.exe`'s `.text`, `PACODE` and `.data` are **byte-identical** to `Porsche.exe`. The only PE
differences are one added import (`NFS5.dll!GetVersion`), a version resource and a manifest.
**All behaviour lives in the DLL**, which loads first and patches the running game.

### What the patch does (132 edits, 58 functions)
Every change is an in-memory edit applied with `VirtualProtect` in seven passes:
* removes the `gimme.dll` load/getproc/free and replaces its `ConvertFile` call (its own `.fsh`
  `SHPI`/`!pal` rewriter is invoked, which effectively disables the conversion);
* replaces the CPU-speed classifier and clamps the detail class (`>2` with a secondary value
  `<32` becomes 2);
* forces the depth/Z path (two 12-byte NOPs) and hooks the renderer-capability check;
* rewrites the engine's 25-entry data-path table to `.\gamedata\…` (this is what makes the install
  portable and registry-free);
* replaces the language getter with a direct read of the ini language (4 sites);
* installs a **Fog/Filtering filter** over the renderer's state dispatcher (opcodes `0x07/0x49`
  filtered, `0x0E/0x0F/0x16/0x17` skipped, `0x14/0x15/0x69` zeroed);
* widens the camera field of view on widescreen.

### The two hook handlers (asked for explicitly)
* `FUN_10001fd0` — neutralises gimme.dll's `ConvertFile`: it calls the DLL's own `SHPI` rewriter and
  tail-jumps back. Its `ret 0xc` (verified in live memory) proves it is stdcall with 3 arguments;
  the shim's extra `call` shifts them, so the "path" it receives is the game's return address and
  the conversion always reports failure. That *is* "gimme.dll removed", bug and all.
* `FUN_100036f0` — installs the Fog/Filtering filter over game `0x006bd97c`, saving the original and
  resuming. Confirmed live: `0x006bd97c == nfs5.dll + 0x35f0`.

### Runtime verification (read-only, against the running patched game)
`nfs5.dll` loaded at `0x6f5a0000`; every one of the 132 edits was present exactly as modelled; the
language override and the path table resolved to the DLL's own strings; the filter was installed.
Recovered numbers:
* view model: aspect ≤ 4:3 → 640 × (640/aspect), FOV **37.5°**; aspect > 4:3 → (480·aspect) × 480,
  FOV `atan(360·aspect)` degrees (16:9 ≈ 89.91°). The FOV variable is a drop-in for the engine's own
  `37.5` constant and is in **degrees**.
* in-race sample: internal mode 640×480 → FOV 37.5°, matching the shipped constants.

### Data and formats
`.fsh` = `SHPI` containers (`GIMX` images, `!pal` palette chunks — the name appears in **0 of 168**
shipped archives, so the patch's palette branch is dormant); `.crp` = chunked models with reversed
tags; `.viv` = `BIGF` audio archives; `.sim` = a 328-byte car record whose gear-ratio floats at
`0x6c–0x7f` are what the patch edits; `nfs5.car/.prt/.trn` = catalogue tables; the front end is
`.lay` screens + `.ctr` controls + `festrings.csv`; `vssver.scc` files are leftover SourceSafe
metadata. The patch's data changes were diffed file by file (~90 car `.fsh`, `common.fsh`,
`356_1.crp`, `graphicsettings.lay`, one `.sim`, plus new Monaco cop files and a clean savegame).

### Security finding (important)
`NFS5\RegSetup.exe` is detected by Windows Defender as **active `Trojan:Win32/Alureon`** (also
`Alureon.BK`, plus `Bulta!rfn` and `VirTool:Win32/Obfuscator.ER` on the copy) — aPLib-packed, bogus
1973 timestamp, decoy imports. It was **quarantined by Defender during this session** and is gone
from both game folders. Treat it as malicious; the patch's `config.exe` is separately flagged
`Trojan:Win32/Nfpatch` but is a benign, unpacked INI editor (a generic game-patcher false positive).

### Tooling notes
REA 6.4.0 with Ghidra 12.1.4 + JDK 21 (its Ghidra bridge timed out on 3 of 4 attempts; one run
produced a `binary_overview` bundle for `nfs5.dll`). Where Ghidra failed, capstone/pefile
disassembly and a purpose-built differ were used. All 58 DLL functions were eventually decompiled.

---

## 3. The Rust rewrite (`NFSPU-Rust`)

A from-scratch Bevy 0.19/wgpu rewrite that reads the player's own install at runtime. Two crates:
`nfspu-formats` (plain readers: RefPack, SHPI, CRP, and the new front-end data) and `nfspu-game`
(engine: track loading, ground, collision, car model, sky, and now the front end).

### Milestones reached
| # | Goal | State |
|---|------|-------|
| M0 | window + find the game folder | works |
| M1 | RefPack, SHPI, CRP readers | all 15 tracks / 44 car containers parse |
| M2 | render a real track | works (user-confirmed) |
| M3 | drive on it | works (placeholder car, walls) |
| M4 | real car model (993 + others) | works |
| M5 | cars/sound/menus/AI | in progress |
| **M6** | **retail front end** | **first pass working** (see below) |

### M6 as delivered
* `nfspu-formats::fe` reads the front end's own data: `.lay` screens, `.ctr`/`.scr` control
  templates and `festrings.csv` (ISO-8859-1, with the shipped column quirk handled).
* `nfspu-game::menu` turns a screen into draw commands from that data alone: geometry from the
  layout, colours and label placement from the templates, text from the locale. Navigation follows
  the game: `destination=` → `<destination>.lay`, `GOBACK`/Backspace back, Escape quits.
* `menu_mode.rs` draws it through the UI layer, scaled 640×480 → window height and centred.
* Coverage over all 98 screens: **1,500 draw commands**, 1,936 controls not drawn yet, led by
  `NFS5.GOURAUDBOX` (576, a four-corner gradient) and `NFS5.PICTURE` (484, `Art/<name>.fsh` + index).
* Verified by capture rather than by eye: `logs/menu_singleplayer.png` contains 27,267 pixels of
  `rgb(188,0,0)` — exactly the retail button `maincolor` `0xBC0000` — and text on black.
  A live session also logged `quickrace -> location -> trackrecords` navigation.
* Two real bugs the shipped data exposed: `REAL.`-prefixed controls after the header were being
  dropped (only the *first* block is the screen), and text-only controls were skipped for having no
  size (point size now comes from the font name, `Univers17b.ffn` → 17).

### Test status
40 `nfspu-formats` + 77 `nfspu-game` lib tests pass; `cargo check --all-targets` clean; 5 bin tests
(2 ignored). The repository has no commits yet; the `.gitignore` is a whitelist and new `.rs`/`.md`
files are visible to git (the `.bat` launchers are not — `!*.bat` would fix that if wanted).

### How to run
```powershell
$env:PATH="$env:USERPROFILE\.cargo\bin;C:\Program Files\Git\cmd;$env:PATH"
cd C:\Users\CB\Desktop\NFSPU-Rust
cargo build -p nfspu-game --bin nfspu-game -j 2      # close any running window first
.\run_menu.bat singleplayer                          # front end
.\run_m2.bat alps                                    # track viewer
$env:NFSPU_RETAIL_FOV="1"                            # opt-in retail field of view
$env:NFSPU_MENU_CAPTURE="1"                          # save logs/menu_<screen>.png and exit
```

---

## 4. Still open

**Front end (M6):** the shipped `.ffn` fonts are not used yet, so typography is not retail;
`NFS5.GOURAUDBOX` and `NFS5.PICTURE` are undrawn (they are ~1,060 of the 1,936); the 3D menu rooms
(`FEData/Models/garage.crp` etc.) are not shown behind the screens; `[part.*]` rendering rules and
`callback`/`callbacklist` scripting are unmapped.

**Then, in the planned order:** M7 HUD (`DashHud/*.fsh` + `nfs5-1.hud`), M8 lighting (per-track
`Sky/*.txt` weather regions and time of day, replacing today's guessed tint), M9 AI and traffic
(`Simulation/AI/*.csv`+`.bin`, the 30 racing-line `.lsp` files, `TRAFCFG.DAT`).

**Unresolved from the RE side:** whether the one-slot argument shift in the gimme hook was
deliberate (behaviour is known either way).

**Housekeeping:** `target\` holds a large build cache that is safe to delete; the on-disk
`nfspu-game.exe` is one warning-only edit behind and needs a rebuild with the window closed.
