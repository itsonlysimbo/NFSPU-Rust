# The reference build (Enhanced 3.5 + Combined Patch v1.0.6)

Notes on the build this rewrite compares against, and on the runtime layer it adds over the 2000
engine. Prose and tables only; the code lives in `crates/nfspu-game/src/refbuild.rs`.

## What it is

* Base: the community **"Enhanced 3.5"** build (`Porsche.exe`, 2000-06-05, file version 3.5,
  already carrying the 4 add-on cars).
* On top: the **Combined Patch v1.1** (2019). Its executable is `nfs5.exe`
  (product version **1.0.6**, build date **2019-06-21**, SHA-256
  `c318af39…c94906`), and it ships one small helper DLL plus replacement renderer modules under
  `drivers/`.
* The patched executable's **code section is byte-identical** to `Porsche.exe`. The only
  differences are one extra imported DLL, a version resource and a manifest. **All behavioural
  changes live in the helper DLL**, which is loaded before the game starts and adjusts the running
  process. That is why the settings files, not the executable, define the reference behaviour.

Read from the patch's own files and constants, plus a **read-only** sample of a live patched
session (process memory only; no breakpoints, no writes). The helper DLL was loaded at a raised
base address and every adjustment predicted from its data was present in memory.

## Settings (`nfs5.ini`, block `[NFS5]`)

| Key | Type | Default | Effect |
|-----|------|---------|--------|
| `Language` | token | `English` | interface language (see table below) |
| `ThrashDriver` | folder | `dx7z` | renderer folder under `drivers/` |
| `WindowedMode` | 0/1 | 0 | run in a window |
| `SingleProcAffinity` | 0/1 | 0 | pin the game to one CPU |
| `CpuLimit` | MHz | 0 | report a fixed clock instead of measuring; 0 = measure |
| `NoErrorReporting` | 0/1 | 0 | suppress the Windows error dialog on a crash |
| `NoMovies` | 0/1 | 0 | skip the intro and idle movies |
| `Fog` | 0/1 | 1 | fog effect |
| `Filtering` | 0/1 | 1 | linear texture filtering |
| `CarControl` | 0/1 | 0 | keep control of the car after the finish line |

Verified against the author's patched copy: `Language=english`, `ThrashDriver=opengl3`,
`WindowedMode=1`, `NoMovies=1`, `Fog=1`, `Filtering=1`, everything else 0.

Language ids, in the order the build compares them: `0 ENGLISH`, `1 FRENCH`, `2 GERMAN`,
`3 ITALIAN`, `4 SPANISH`, `5 SWEDISH`, `6 PORTBRZL` (Brazilian Portuguese).

## Renderer selection

`drivers/<ThrashDriver>/thrash.ini` names the module and its back end:

| `Type=` | Back end |
|---------|----------|
| `d3d` | Direct3D 7 |
| `voodoo` | Glide |
| `opengl` | OpenGL |
| `software` | software |

Every key in that file's `[ENV]` block is handed to the module as a process environment variable,
which is how per-renderer options (resolution, colour depth, vsync, gamma, aspect, texture
formats …) reach it. The author's `opengl3` profile loads `opengl3z.dll` and carries **18**
options; the audit prints the count, which is a cheap cross-check that the profile still parses.

## View and field of view

The build keeps a **virtual framebuffer** and a camera field of view that depend on the client
aspect ratio. The comparison is against exactly 4:3, and equal is *not* widescreen.

| Condition | Virtual width | Virtual height | Field of view |
|-----------|---------------|----------------|---------------|
| aspect ≤ 4:3 | 640 | 640 / aspect | 37.5° (the engine's own value) |
| aspect > 4:3 | 480 · aspect | 480 | `atan(360 · aspect)`, degrees |

`360` is `480 · 3/4`. Sampled live:

| Client | Virtual | Field of view | Widescreen |
|--------|---------|---------------|------------|
| 640×480 (live race) | 640 × 480 | 37.5° | no |
| 1280×960 (exactly 4:3) | 640 × 480 | 37.5° | no |
| 1920×1080 / 1280×720 | 853.33 × 480 | 89.9105° | yes |
| 1920×1200 | 768 × 480 | 89.9005° | yes |
| 2560×1080 | 1137.78 × 480 | 89.9329° | yes |
| 3440×1440 | 1146.67 × 480 | 89.9334° | yes |

The constants were read from the build's own data (the 4:3 threshold is stored as a double
`1.33333337306976`; the degrees conversion divides by π/180; the 3 and 4 are stored as doubles).

Two notes for later work:

* The field-of-view value is the engine's own camera constant, which the build simply replaces.
  It converts its own result from radians to degrees, so the value is in **degrees**. The
  *consumer* (the camera at about `+0x9c`/`+0x290` of the view object) is not yet traced, so if
  the rewrite ever draws widescreen, that is where to check the interpretation.
* A 640×480 internal mode is what a "by desktop" renderer setting still produces, so the
  widescreen branch only engages when the game's own video mode is widescreen.

## Renderer state gate

The build wraps the renderer's state function and gates a few state codes on the two graphics
options. Everything else passes through untouched.

| States | Option | When the option is **off** |
|--------|--------|----------------------------|
| `0x07`, `0x49` | `Filtering` | call with value 0 |
| `0x0E`, `0x0F`, `0x16`, `0x17` | `Fog` | skip the call |
| `0x14`, `0x15`, `0x69` | `Fog` | call with value 0 |

State codes are the engine's own; the sets come from the build's dispatch table and were confirmed
present in the live process.

## CPU detail class

The build replaces the engine's clock measurement and then clamps the resulting detail class:

```
class = measured;  if measured > 2 && secondary < 32 { class = 2 }
```

The live machine reported class 2 with the secondary value at 16, so the clamp did nothing and
detail stayed high. Classes run 0 (slowest) to 3. The engine's own thresholds for producing the
class (clock bands plus a short benchmark) are in `Porsche.exe` and are not reproduced yet.

## Portable data folders

The build hands the engine 25 paths relative to its own folder instead of a registry install
location, which is what makes a patched copy portable. They are listed as
`refbuild::PORTABLE_DATA_DIRS` (`.\gamedata\…`, `.\fedata\…`, `.\drivers\`, `.\savedata\`).

## Verified negatives

* **`!pal` does not occur in shipped art.** The build's texture path looks for a palette chunk
  named `!pal` inside `SHPI` archives (its other needle is the `SHPI` magic). A scan of every
  `.fsh`/`.env` under `GameData` — 168 archives, 153 plain and **15 RefPack, decompressed first**,
  112.8 MiB on disk — found the name in **zero** of them. Whatever that branch was written for, it
  is dormant for the retail data. (Reported by `examples/refbuild_audit.rs`.)
* The patched executable's code section is unchanged, so no game logic differs between
  `Porsche.exe` and `nfs5.exe`.

## Known rough edges in the build (for parity work)

* Its avatar/image conversion path is effectively disabled: the replacement call receives its
  arguments shifted by one stack slot, so the file it is asked to open is not a filename and the
  conversion reports failure. The observable result is "no conversion", which is what the patch
  advertises, but it is a bug rather than a design.
* Because the gimme-era image converter is gone, driver avatars come from the game's own path.

## How to reproduce the audit

```powershell
$env:NFSPU_GAME_DIR="C:\Users\CB\Downloads\NFS5 - Copy"   # the patched copy, to read its nfs5.ini
cargo run -p nfspu-game --example refbuild_audit -j 2
```

Prints the tables above and writes `logs/refbuild_audit.txt`. With an unpatched folder it simply
reports that no `nfs5.ini` is present.
