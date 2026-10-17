# Need for Speed 5: Porsche Unleashed — Reverse-Engineering Report

Scope analysed (all under `C:\Users\CB\Downloads\`):

| Tree | What it is |
|---|---|
| `NFS5\` | The game as downloaded (EA 2000 binaries + a 2009 "Direct Play" fan repack) |
| `NFS5-Combined-Patch-v1.1\` | The fan **compatibility patch** ("NFS5 Combined Patch v1.1") |
| `NFS5 - Copy\` | The game with the patch already merged over it |

Tooling: REA (`rea` 6.4.0) with Ghidra 12.1.4 + JDK 21 for native inspection, plus custom
static PE/disassembly analysis (pefile + capstone). All findings are **static**; nothing was
executed.

---

## 0. ⚠️ Security finding first (read this)

While handing `NFS5\RegSetup.exe` to Ghidra, **Windows Defender blocked the file**
(`ERROR_VIRUS_INFECTED`, Win32 225) and then **quarantined it — the file is gone from both
`NFS5\` and `NFS5 - Copy\` in the course of this analysis.** Defender's own verdicts,
straight from `Get-MpThreat` / `Get-MpThreatDetection`:

| File | Defender detection | Threat ID | State |
|---|---|---|---|
| `NFS5\RegSetup.exe` (and the `NFS5 - Copy` copy) | **Trojan:Win32/Alureon** | 100185 | **Active** |
| `NFS5\RegSetup.exe` | **Trojan:Win32/Alureon.BK** | 139065 | **Active** |
| `NFS5\RegSetup.exe` | Trojan:Win32/Alureon.gen!J | 126104 | Inactive |
| `NFS5 - Copy\RegSetup.exe` → `[aPLib_034_mem]` | Trojan:Win32/Bulta!rfn | 2147694403 | Inactive |
| `NFS5 - Copy\RegSetup.exe` → `[aPLib_034_mem]` | VirTool:Win32/Obfuscator.ER | 2147621044 | Inactive |
| `NFS5-Combined-Patch-v1.1\Copy\config.exe` | Trojan:Win32/Nfpatch | 2147873842 | Inactive |

Static corroboration that `RegSetup.exe` is packed/obfuscated (not a normal compiler output):

* PE TimeDateStamp is bogus: `112055885` → **1973-07-20**.
* `IMAGE_FILE_RELOCS_STRIPPED`, section table is `.text/.data/.rdata/.idata/.rsrc` only.
* `.idata` has **virtual size `0x89721` (562 KB)** but raw size only `0x25800` (153 KB) — the
  import "section" is a packed data blob, not a normal import directory.
* Imports are a decoy set: only 2 ADVAPI32 functions (incl. `ObjectPrivilegeAuditAlarmW`,
  `InitiateSystemShutdownA`) plus Toolhelp32 (`Heap32ListNext`, `Process32First`, `VirtualQuery`).
* Defender's unpacker labelled the payload `aPLib_034_mem`, i.e. **aPLib**-packed.

Assessment: **This file should be treated as malicious.** It is packed, obfuscated, flagged
"active" by Defender with two independent Alureon signatures, and it is the exact Windows-2000/
XP-era dropper profile. It is **not** required to play the game — the game's real compatibility
work is done by the Combined Patch's `nfs5.exe` + `nfs5.dll` (below). Recommend: leave it
quarantined, do not restore it, and run a full Defender/offline scan. (`NFS5.7z` still contains
it, so re-extracting the archive would re-introduce it.)

`NFS5-Combined-Patch-v1.1\Copy\config.exe` is a different story — see §4.4; its detection is
almost certainly a **generic "game patcher" false positive**, and its code is benign.

---

## 1. What each deliverable actually is

### 1.1 The base game `NFS5\`
The original 2000 EA build plus the **"Enhanced Version 3.5.20040310"** community build
(`_ReadMe.txt`, `version.txt` = `3.5`). Notable contents:

* `Porsche.exe` — the game (2,129,965 bytes). SHA-256
  `e4684948ca9bfb552a0fc0eeb7024a13bfe67f2e40ebb6ba121ae6a81dd166bb`.
* `gimme.dll` — EA's driver-avatar/image converter (see §3.1).
* `mrbupd.dll` — Marimba Castanet "UpdateNow" SDK (the EA Game Updater client) (see §3.2).
* `3DSetup\` — EA 3DSetup launcher (`3DSetup.exe`, `3DSetup.dll`, `3dsetup.ini`, `3DSetup.reg`).
* `Drivers\` — `dx7z.dll` (DirectX 7 "Thrash" renderer), `voodoo2z.dll` (Glide renderer), `dx7z.dlb`.
* `Support\` — `Ip.exe` (Eastern Digital "InfoPro" system-info tool), help files.
* `Language Selector.exe`, `RegSetup.exe` (**malicious, removed**), `RemoveReg.reg`.
* `GameData\`, `FEData\`, `SaveData\` — assets (formats in §5).

### 1.2 The patch `NFS5-Combined-Patch-v1.1\`
A repackage of several community patches. Per `Readme.txt` + `Docs\`:

* **Verok's NFSV Patch v1.0.6** (the core — new `nfs5.exe` + `nfs5.dll` + renderers),
* **NFS5 Essentials** (ND4SPD Racer) — 4 official add-on cars, >2.1 GHz CPU fix, save fixes,
* **Enhanced Patch 3.5.20040310** (bug fixes only),
* **Smooth Shines and Lights** (KrazyBoy) — `common.fsh`,
* plus Combined-Patch-specific data tweaks (`Docs\Combined Patch Changes.txt`).

The `Copy\` folder is what you overlay onto the game:
`nfs5.exe`, `nfs5.dll`, `config.exe`, `nfs5.ini`, `Drivers\{dx7,glide3x,nglide,opengl1,opengl3}\`,
and a handful of `GameData`/`FEData`/`SaveData` files.

### 1.3 The identity of the patched executable
`nfs5.exe`'s embedded version resource (`RT_VERSION`, resource type 16) reads:

```
FileDescription : Need For Speed: Porsche Unleashed Patch
ProductName     : Need For Speed: Porsche Unleashed Patch
FileVersion     : 2019.06.21.027
ProductVersion  : 1.0.6
InternalName    : NFS5
OriginalFilename: nfs5.exe
LegalCopyright  : 2000 Electronic Arts, 2019 Verok
Comments        : Based on the original Need For Speed Porsche Unleashed executable
```
and its `RT_MANIFEST` (type 24) requests **Common-Controls 6.0.0.0** (`name="nfs5"
version="1.6.1.0"`, `asInvoker`). Build/PE TimeDateStamp = 2019-06-21 ⇒ Verok's patch v1.0.6.

---

## 2. How the patch actually works (the important part)

### 2.1 `nfs5.exe` is *not* a recompiled game — it is byte-identical game code
A VA-aligned diff of `Porsche.exe` (base) vs `nfs5.exe` (patch) shows:

```
.text   0 changed ranges, 0 bytes      <-- game code is IDENTICAL
PACODE  0 changed ranges, 0 bytes
.data   0 changed ranges, 0 bytes
.rdata  4 changed ranges, 72 bytes     <-- import descriptor/thunks only
.rsrc   7 changed ranges, 2437 bytes   <-- resources replaced
```

What changed in `.rdata`/imports: `nfs5.exe` adds **one static import, `NFS5.dll!GetVersion`**
(import table grows 12 → 13 DLLs, IAT 1000 → 1008 bytes). `.rsrc` gains `RT_VERSION` and
`RT_MANIFEST` and drops the original icon set. The relocation directory is stripped
(`RELOCS_STRIPPED`, no `.reloc`), and the file is 45 bytes shorter.

**Conclusion: the compatibility patch does not edit the game's logic. It side-loads `nfs5.dll`,
which patches the running game in memory.** This is why `Readme.txt` insists you run
`nfs5.exe`, not `Porsche.exe`.

### 2.2 `nfs5.dll` — the patch engine (20,480 bytes)
Exports just `GetVersion`; imports KERNEL32/USER32/MSVCRT plus **runtime-resolved**
`CreateActCtxA/ReleaseActCtx/ActivateActCtx/DeactivateActCtx/SetProcessAffinityMask`
(kernel32) and `MonitorFromWindow/GetMonitorInfoA` (user32).

`DllMain` (`0x10001a90`) on `DLL_PROCESS_ATTACH`:
1. resolves the activation-context + affinity + monitor APIs,
2. calls its init (`0x10002020`),
3. creates an activation context from its own manifest ⇒ the game's dialogs/message boxes get
   the Common-Controls 6.0 visual styles ("implemented texture filtering for menu elements and
   fonts" in Verok's notes).

`0x10002020` (init) then:
* sets up **portable paths** using a hard-coded list of relative directories
  (`.\gamedata\simulation\ai\` … `.\savedata\`, 30 strings) — this is the "removed all registry
  linkage, so game is fully portable" feature; it also still knows the dead online host
  `porsche-entry.earacing.com`.
* **patches the game's code and data in memory** (see 2.3).

A separate loop (around `0x10003010`) rewrites the game's **directory-path pointer table** in
game `.data` at `0x0065b30c, 0x0065b310, 0x0065b314, …`: each 4-byte pointer is replaced with a
pointer to one of `nfs5.dll`'s own relative-path strings (`0x1000542c` = `.\gamedata\dashhud\`,
`0x10005440` = `.\gamedata\music\`, `0x10005454` = `.\gamedata\sounds\`, `0x10005468` =
`.\gamedata\speech\`, `0x1000547c` = `.\fedata\movies\`, `0x10005490` = `.\fedata\controls\`, …).
This is the concrete "no registry, fully portable" substitution: the engine stops asking the
registry where its data lives and reads `.\…` relative to the exe.

### 2.3 The in-memory detour table (proven)
`nfs5.dll` calls `VirtualProtect(addr, len, PAGE_EXECUTE_READWRITE)` → writes → restores. The
**complete** set is **132 patch sites**, applied by six passes
(`FUN_10001300`, `FUN_10001760`, `FUN_100029f0`, `FUN_10002e30`, `FUN_10003390`,
`FUN_10003c90`, `FUN_100042d0`) plus the inline block in `FUN_10002020`. The
highest-value subset, with the *original* instruction and why it is patched:

| Game address | Patch | Original bytes (Porsche.exe) | Meaning |
|---|---|---|---|
| `0x004dbe0a` | 7× `NOP` | `push edx; call [LoadLibraryA]` | **removes the `gimme.dll` `LoadLibrary`** |
| `0x004dbe18` | 12× `NOP` | `push "…"; push esi; call [GetProcAddress]` | removes `gimme.dll` export lookup |
| `0x004dbe43` | 7× `NOP` | `push esi; call [FreeLibrary]` | removes `gimme.dll` unload |
| `0x004dbe36` | `E9 → 0x10001fd0` | `call eax` | replaces the gimme image-decode call with the DLL's own decoder (§2.6) |
| `0x0044dfb0` | `E9 → 0x10001030` | `call 0x44e370` | **replaces the CPU-speed classifier** |
| `0x0044e370…0x0044e670` | 6× `E9` | CPU-class helpers | six CPU-class functions rewritten |
| `0x005492ae` | `E9 → 0x10001c00` | `call [0x6bd934]` | renderer-capability/Z handling hook |
| `0x005404a5` | 12× `NOP` | `test [dev+0x117e4],0x20000; je +0x11` | forces the patched **depth/Z** path |
| `0x0054051d` | 12× `NOP` | same | same, second site |
| `0x005752d5` | `E9 → 0x100036f0` | `mov eax,[0x6bd97c]; …` | installs the **Fog/Filtering** renderer filter (§2.6) |
| `0x005de6c4` | `= 0` | int `7` | resets a graphics table |
| `0x004a5496` + 25 sites | `NOP` + pointer writes | — | **portable paths**: game path table `0x65b2a8…0x65b360` → `.\gamedata\…` |
| `0x0041e1f0`,`0x4a5d31`,`0x4e0581`,`0x508c6a` | 5-byte constants | — | `push 0x0060a0a1` style patches |
| ~30 pointer sites | 4-byte redirect | — | widescreen/FOV/menu float pointers → DLL globals |

The full, machine-generated table (site, size, kind, computed DLL target, containing game
function) is saved as `nfs5.dll.patch_table.txt` in the reconstruction folder (§7).

Why these matter concretely:

* **gimme.dll removal.** The engine `LoadLibraryA("gimme.dll")` → `GetProcAddress` →
  `FreeLibrary` sequence is NOPped, and the decode call is diverted to `nfs5.dll:0x10001fd0`.
  This is Verok's "removed gimme.dll linkage (which has memory lag) and game decodes driver
  avatars by its own"; it is what removes the need for the `EmulateHeap` compatibility layer and
  fixes the `pic16.fsh` crash.
* **CPU classifier replacement.** `FUN_0044e370` returns a CPU class 0–3 from MHz
  (`DAT_005ce908/1e6`), a benchmark (`FUN_0053c3d0`) and `DAT_00619790`; thresholds top out at
  class 3. The patch redirects that call to `nfs5.dll:0x10001030`, which returns a class from
  the new calculation and honours the `CpuLimit` (fake MHz) option — this is the ">2.1 GHz fix"
  and "increased CPU limits to 4 GHz". The two NOP-outs at `0x5404a5/0x54051d` additionally
  force the high-detail texture/Z path that a slow-looking CPU would otherwise disable.
* **Renderer flags / ZBias.** `FUN_00549220` reads the renderer's device string (`0x6bd934`),
  and sets flag `0x20000` on the device when it is `"DX7"`. The hook at `0x5492ae` lets
  `nfs5.dll` (0x10001c00 → 0x10001bd0) drive that logic; the NOP-outs then make the
  `[dev+0x117e4] & 0x20000` test always take the float branch
  `*(float*)(dev+0x1180c) = z / 32768.0`, i.e. Verok's "proper ZBias calculation … car
  backlights are shadowed under trees".

### 2.4 Configuration: `nfs5.ini` + per-renderer `thrash.ini`
`nfs5.dll` reads its settings with `GetPrivateProfile*` from `nfs5.ini`, section `[NFS5]`:

```
[NFS5]
Language=english        ; → 0..6 = English/French/German/Italian/Spanish/Swedish/Brazilian-Portuguese
ThrashDriver=opengl1    ; renderer subfolder under .\drivers\
WindowedMode=0
SingleProcAffinity=1    ; → SetProcessAffinityMask(proc, 1)
CpuLimit=0              ; fake CPU MHz (0 = auto)
NoErrorReporting=0      ; → SetErrorMode(...|0x8003)
NoMovies=0
Fog=1
Filtering=1
CarControl=0
```

Renderer selection is `.\drivers\<ThrashDriver>\thrash.ini` with `[THRASH] File=… Type=…`
(`Type` ∈ `d3d`/`voodoo`/`opengl`). `nfs5.dll` exports every `[ENV]` key of the chosen
`thrash.ini` as a process **environment variable** (`SetEnvironmentVariableA`) so the renderer
DLL can read it (`OGL_*`, `NGLIDE_*`, …). Windowed mode writes a byte at game `0x004676ba` and
installs a 4-byte hook pointer `0x10003710` at game `0x0053ac98`; it uses
`MonitorFromWindow/GetMonitorInfoA` to centre the window.

### 2.5 On-disk `.fsh` palette patcher
`nfs5.dll` also contains a file rewriter (`0x10001c20`) that opens a file, matches the EA
container magics **`"SHPI"`** (FSH header) and **`"!pal"`** (palette chunk), then rewrites
palette entries (`WriteFile` of 0x18/0x20/0x08 bytes). This is the 16-bit→32-bit art/palette
fix applied to the game's `.fsh` textures, and it is exactly what `FUN_10001c20` does when
called from the gimme.dll hook below.

### 2.6 The two remaining hook handlers (fully reversed)
Ghidra's first pass recovered only 23 of the DLL's 58 functions and missed both of these. They
were recovered by forcing function creation at every detour target:

**`FUN_10001fd0` — neutralises `gimme.dll`'s `ConvertFile` (hook of game `0x004dbe36`).**
A naked detour: it `call`s `FUN_10001c20` (the `SHPI`/`!pal` `.fsh` rewriter, `__stdcall`,
3 args — epilogue `ret 0xc`, verified live), then `jmp dword ptr [0x10006048]` where
`0x10006048 = 0x004dbe4a` (right after the now-NOPped `FreeLibrary` block). The extra `call`
inserts one stack slot, so `FUN_10001c20` receives the game's return address as its *path*
argument → `CreateFileA` fails → it returns 0, and the shim's `mov bl,al` forwards "failed".
Runtime-verified net effect: **EA's VfW-codec image conversion is disabled** (always returns 0),
which is the "removed gimme.dll linkage" change — see §7 and `RECONSTRUCTION.md` §8.1.

**`FUN_100036f0` — installs the Fog/Filtering renderer filter (hook of game `0x005752d5`).**
```c
void FUN_100036f0(void) {                 /* game 0x5752d5 */
    DAT_1000612c = *(void**)0x006bd97c;   /* save the game's Thrash state dispatcher  */
    *(void**)0x006bd97c = FUN_100035f0;   /* install nfs5's filter                    */
    jmp dword ptr [0x10006060];           /* == 0x005752db : resume the game          */
}
```
`FUN_100035f0(op, value)` is an exact **jump-table filter** over that dispatcher (rebuilt from
the disassembly; the decompiler showed it degenerately):
```c
if ((unsigned)(op - 7) > 0x62) goto pass;          /* MSVC switch -> table at 0x10003678/0x10003688 */
switch (op) {
  case 0x07: case 0x49:  if (Filtering) goto pass; value = 0; goto pass;   /* texture-filter states */
  case 0x0e: case 0x0f: case 0x16: case 0x17:  if (!Fog) return; goto pass; /* fog states: skip call */
  case 0x14: case 0x15: case 0x69:  if (!Fog) value = 0; goto pass;         /* fog value: zero when off */
  default: goto pass;                                                       /* everything else */
}
pass: return DAT_1000612c(op, value);              /* saved original dispatcher */
```
`Fog` = `0x100060c8`, `Filtering` = `0x100060cc`. That is how the Fog and texture-Filtering
toggles are implemented without touching the renderer DLL. Two other previously-open items are
also closed: the four 5-byte writes (`0x41e1f0/0x4a5d31/0x4e0581/0x508c6a`) replace
`call 0x4b73f0` (the game's language getter) with `mov eax,[0x100060a0]` (the ini language id);
and `FUN_10004000` is not a function at all — it is the tail of `FUN_10003c90` (`880+671=1551`)
that a forced function creation split off.

Both handlers, and the other 56 functions, are reproduced C-by-C in the reconstruction folder —
see §7.

---

## 3. The base game's own binaries

### 3.1 `gimme.dll` (4,599,863 bytes)
A DLL whose **internal export name is `FaceInRace.dll`** (one export, `ConvertFile`). It imports
`MSVFW32` (`ICLocate/ICGetInfo/ICCompress/ICDecompress`) and clipboard APIs → it is EA's
VfW-codec-based **driver-face image converter** used by the front end. That is the component
Verok's patch excises (§2.3). (A tiny 5,120-byte "gimme.dll" stub is sometimes shipped by other
compat packs to neutralise it; here the file is the original large EA DLL `0200-02-01`.)

### 3.2 `mrbupd.dll` (102,400 bytes)
Marimba Castanet "UpdateNow" SDK 4.0.0.0 — the client for EA's **Game Updater** (`UGameUpdater`).
Exports 71 functions including **zlib** (`inflate`, `inflateInit_`, `adler32`) and `mrb*` API
(`mrbInitUpdate`, `mrbIsUpdateAvailable`, …). It talks to `http://trans.ea.com/games/Pioneer/Nfs5`
(wire `version.txt`, `properties.txt`, `fe.txt`). All of those services are long dead; the patch
makes the game portable/offline.

### 3.3 Other bundled tools
* **`3DSetup\3DSetup.exe`** (EA Canada, 2001-02-05) + **`3DSetup.dll`** (2003-09-09, exports the
  `TDSETUP_*` API: `TDSETUP_Init/GetRenderOptions/SetCurrentRenderer/SaveSettings/…`) — EA's
  renderer/resolution chooser. It writes the chosen driver+video mode to the registry.
  `3DSetup\3DSetup.reg` is the **EA "3D Data" card database** it installs at
  `HKLM\SOFTWARE\Electronic Arts\3D Data`: numbered cards with `Name`, `Module`
  (`softtri`, `verite`, `dx7z`, `voodoo2z`, …) and per-card `VendorID/DeviceID`. `3dsetup.ini`
  lists the supported cards/resolutions and the product registry key to use
  (`Electronic Arts\Need For Speed - Porsche 2000`). This is the *original* renderer-selection
  path that `nfs5.dll`'s `ThrashDriver` + `.\drivers\<name>\thrash.ini` mechanism supersedes.
* **`Language Selector.exe`** — a small **Delphi** utility (`"NFS5 Language Selector By LS"`) that
  writes the game's `Language` value under `\Software\Electronic Arts\Need For Speed - Porsche 2000`.
* **`Support\Ip.exe`** — Eastern Digital **"InfoPro 2.1"** system-information/benchmark tool
  (1997) with `Support\ip.cfg` (`/n="NFS: Porsche 2000"`), shipped to help diagnose hardware.
* **`Support\EAHELP.HLP`/`Eaukhelp.hlp`/`FRHELP.HLP`/`GRhelp.HLP`/`EAHELP.GID`** — WinHelp files.
* **`readme.txt`**, **`Support\readme.txt`**, **`_ReadMe.txt`** — EA readme + the repack's readme;
  **`RemoveReg.reg`** / **`properties.txt`** / **`version.txt`** belong to the Castanet
  update-channel repack.

### 3.4 `Porsche.exe` (base game) — quick facts from my own evidence
PE32 x86 GUI, MSVC (2000-06-05), image base `0x400000`, 6 sections. Imports 12 DLLs:
`mrbupd, KERNEL32, USER32, GDI32, ADVAPI32, SHELL32, DINPUT, DDRAW, WSOCK32, WINMM, VERSION,
DSOUND`. Strings confirm:

* Registry: `Software\Electronic Arts\Need for Speed - Porsche Unleashed` (and `- Porsche 2000`,
  `- Porsche`) plus a `RegUrl` value.
* Online: `http://trans.ea.com/games/Pioneer/Nfs5`, `http://login.earacing.com`.
* Screenshot: `%sUnleashed%02d.tga`; CD launcher `%sAutorun.exe`; debug `fe_out.txt`.
* The **Thrash API** was already the engine's renderer abstraction:
  `THRASH_opendll - LOADLIBRARY FAILED ON "%s"`, `THRASH_opendll - not a 3rash DLL`,
  `_THRASH_writerect@20`, `_THRASH_window@4`, …
* Assets: `%s%s.fsh`, `%s%s.viv`, `%s.crp`, `C%sSky\%s.fsh`, `%s\*.crp`, `gimme.dll`.

(Corroborates `C:\Users\CB\Desktop\NFS5 Decomp\SUMMARY.md`, which already decompiled this binary
in a prior session: `entry 0x5A2D1A`, `WinMain FUN_004B6710`, `main FUN_004B6A50`, 8,368
functions, ~95 % of `.text` inside functions, no packer, no copy-protection.)

### 3.5 Renderer modules ("Thrash"/3rash API)
`Drivers\dx7z.dll` and `Drivers\voodoo2z.dll` export the 3rash entry points the engine looks up
(`_THRASH_init`, `_THRASH_setvideomode`, `_THRASH_settexture`, `_THRASH_setstate`, `_THRASH_drawtri`,
`_THRASH_pageflip`, `_THRASH_talloc`, … 41–45 exports). `dx7z.dll` is the DirectX-7/DDraw backend;
`voodoo2z.dll` is the Glide backend; the engine loads the chosen one by name via `THRASH_opendll`
(details of the patched versions in §4.3). A third file, `Drivers\dx7z.dlb` (155,648 bytes), is
despite the extension **another PE** — an earlier 2000-03-02 build of the same `dx7z.dll` renderer
(same `dx7z.dll` export name, same 3rash exports); it is a spare/legacy copy.

### 3.6 Runtime settings files
`fe.txt` is the engine's text settings file (input keys, physics/HUD/`car[n]` blocks, audio,
rendering). On exit the game writes the live state back out as `fe_out.txt` — a **NUL-separated**
`key = value` mirror of every setting (quality/detail, `texturedetail`, `widescreen`, `window`,
`carchrome`, `cardetail`, controls, audio levels…). `NFS5 - Copy\fe_out.txt` is the merged
install's dump (16,243 bytes) and shows it was last run on `opengl3`, windowed, 32 bpp.

---

## 4. The Combined Patch, component by component

### 4.1 `Copy\nfs5.exe` — see §1.3/§2.1. Verok's v1.0.6 game exe (no logic changes).
### 4.2 `Copy\nfs5.dll` — see §2.2–2.5. The whole patch mechanism.
### 4.3 Drivers added by the patch ("Thrash"/3rash renderers)
`Drivers\` gains a `thrash.ini` per backend plus new renderers. Every module exports the same
3rash API the engine looks up; two export families exist:

| Family | Exports | Extra entry points |
|---|---|---|
| DX7 (`dx7z.dll`) | 45 | `_THRASH_createwindow@16`, `_THRASH_destroywindow@4`, `_THRASH_getwindowtexture@4` |
| Glide/OpenGL (`voodoo2z`, `opengl1z`, `opengl3z`) | 41 | — |

Common core: `about, clearwindow, clip, drawline/…/drawtri/…/drawtristrip, flushwindow, getstate,
idle, init, is, lockwindow/pageflip, readrect/writerect, restore, selectdisplay, setstate,
settexture, setvideomode, sync, talloc/tfree/treset/tupdate, unlockwindow, window`.

* **`dx7\dx7z.dll`** — a **rebuild/port**, not a byte-patch: the 2001 EA `dx7z.dll`
  (`FileDescription="DX7 3rash Module"`) becomes VEG's *"D3D7 Thrash Driver Modern Patch"*
  v1.3.2 (`2016-11-06`), said to be based on the Motor City Online build. `PACODE` is merged
  into `.text`; 26 code ranges change (11,457 B). Representative fixes: video-mode/surface-format
  init rewritten, a `.rdata` float `0.0→0.5`, and a new 52-byte vertex helper appended at the end
  of `.text` that subtracts `0.5` from the x/y of `count` 0x20-byte vertices — a **half-texel
  vertex offset**. No new imports/strings. It reads `getenv("THRASH_DISPLAY")` and
  `getenv("THRASH_D3DDEVICETYPE")`.
* **`glide3x\voodoo2z.dll`** — EA's Glide renderer with a **tiny, surgical patch** (6 ranges,
  1,246 B; `FileDescription="Glide3x Thrash Driver (API v107)"`, Verok v1.0.0, 2018-01-04). The
  original code did `LoadLibraryA("glide3x.dll")`; the patch replaces that with a new routine
  (`0x60010398`) that does `GetModuleFileNameA` → strip to directory → `strcat("glide3x.dll")` →
  `LoadLibraryA(path)`, with a fallback to `LoadLibraryA("glide3x.dll")`. **It makes the renderer
  load Glide from its own folder**, so the bundled nGlide is used instead of a system Glide.
* **`nglide\glide3x.dll`** — **nGlide 2.10** ("3dfx Interactive, Inc.", Glide 3.10.00.0658),
  a Glide→D3D wrapper with 129 `_gr*/_gu*/_pci*/_pio*` exports, embedded splash (RCDATA), libjpeg/
  zlib/libpng, and one registry key `Software\Zeus Software\nGlide2`. Environment keys:
  `NGLIDE_SPLASH/GAMMA/VSYNC/REFRESH/ASPECT/RESOLUTION` (matching its `thrash.ini`).
* **`opengl1\opengl1z.dll`** and **`opengl3\opengl3z.dll`** — **Verok's own OpenGL renderers**
  (Oleksiy Ryabchun, 2019-12-05/2019-06-22), export-name `THRASH.dll`.
  `opengl1z` = OpenGL 1.4 fixed-function (+ARB/multitexture/edge-clamp); `opengl3z` = OpenGL 3.0
  VBO/VAO with **embedded GLSL `#version 130` shaders** (RCDATA 1001/1002) and gamma as
  `pow(rgb, 1/gamma)`.

  **How `[ENV]` options are consumed** (per-renderer, function ~RVA 0x9730): for each option name
  it builds `sprintf(key,"OGL_%s",name)`, tries `GetPrivateProfileStringA("THRASH", key, …)` from
  the module's own `*.ini`, exports it with `SetEnvironmentVariableA`, then falls back to
  `GetEnvironmentVariableA`. Option names: `WINDOWED, RESOLUTION (0..0x1c), COLORDEPTH (16/24/32),
  ZDEPTH (16/24/32), REFRESH, EXCLUSIVE, VSYNC, ASPECT, TEXFILTER (0..2), MOVIES16BIT, GAMMA,
  TEX_CONVERT_ARGB32, TEX_COLOR_INDEX_4/8, TEX_COLOR_ARGB_1555/RGB_565/RGB_888/ARGB_8888/ARGB_4444,
  TEX_INDEX_RGB/ARGB`.
  Gamma = `clamp(0.1*idx + 0.5, 0.5, 1.5)` (so `OGL_GAMMA=5` ⇒ 1.0). **Widescreen**: in
  `opengl3z` the code computes `ratio = (float)value / display_height` and, when `ASPECT != 0`,
  recomputes the horizontal projection term — the "proper widescreen FOV" behaviour.

> Correction to a plausible reading: the `thrash.ini` `[ENV]` keys are exported to the process
> environment by **`nfs5.dll`** (`SetEnvironmentVariableA`, proven in §2.4), not by `nfs5.exe`.
> The renderers also independently re-read their own `*.ini` `[THRASH]` section as a fallback.
>
> The readme's "proper ZBias calculation for Glide" is **not** in `voodoo2z.dll` (its only change
> is the in-folder Glide loader + version resource); it is implemented in `nfs5.dll`'s detours
> (§2.3, the `0x5492ae`/`0x5404a5`/`0x54051d` patches).

### 4.4 `Copy\config.exe` (106,496 bytes)
A **data-driven INI editor** (build-time manifest template `Meegeez.WinApi.Test`, schema root
version 2.0.6.0). Its 67 KB resource section embeds an XML description of `nfs5.ini` and every
renderer's `thrash.ini`, from which it renders the GUI, then writes the INI files and optionally
launches the game (`ShellExecute`). It knows renderers `dgvoodoo, dx7, glide3x, nglide, opengl1,
opengl3`, the options listed in §2.4, and a resolution list up to 7680×4320. This is the tool the
patch's readme tells you to run as administrator.

**Defender's `Trojan:Win32/Nfpatch` on `config.exe` is almost certainly a generic "game patch
tool" heuristic false positive**: the file is unpacked (normal `.text/.data/.rsrc`), imports only
benign Win32/GDI/CRT APIs, and does nothing but read/write INI files and `ShellExecute` a game
exe. Treat with normal caution (unsigned), but it is not equivalent to the `RegSetup.exe` finding.

### 4.5 Data files shipped by the patch
See §5.7 (byte-diff of base vs patch data).

---

## 5. Data formats and asset changes

### 5.1 Format catalogue (magic → meaning, all observed from the shipped bytes)

| Ext | Magic / header | Structure |
|---|---|---|
| `.fsh` | `SHPI` | EA "shape/image package": `SHPI` + dword total size + dword chunk count, then named chunks — image chunks **`GIMX`** (with a 4-char subtype: `0000`, `spik`, `Nkhm`, `st1a`, `driv`, …), `SHAD`, `FLR0/FLR1`, `BT1 `, `550N`, `N250`, and palette chunks **`!pal`**. (`pic16.fsh` contains a `GIMX driv` = the driver avatars.) |
| `.crp` | `10 FB 5E 20…` (tracks) / `" raC"` (cars) | Chunked model archive; chunk tags are stored byte-reversed (`" raC"`=**Car **, `"itrA"`=**Arti**, `"karT"`=**Trak**), little-endian. |
| `.viv` | `BIGF` | EA "Big File" archive of audio banks: entries like `sd1a.bnk`, `sd1a.cc`, `ahead-yourejust.bnk`. |
| `.bnk` | (inside `.viv`) | EA audio bank (the `.spl` files are the per-car sound *programs*, 52–60 bytes). |
| `.sim` | ASCII name | **Fixed 328-byte car-simulation record.** Offsets: `0x00` name (`"1998 996 Carrera 3.4L"`), `0x40` two floats (1320.0 / 2350.0 — mass & power), `0x50` ints, `0x6c`–`0x7f` **5 floats of gear ratios**, `0x80`–`0x8f` more, `0x90`+ a power/torque curve (175, 7300, 25, 161, 181, 200, 205, 224, 236, 232, 250, 258, 253, 248, 240, 234, 228, 189 …), `0xf0`+ suspension/tire values. |
| `FEData\Data\nfs5.car` | `"L\0"` + name | Array of fixed car-definition records beginning with a 2-byte prefix + name (`"'50 356 1100 Cabriolet"`) — the showroom/car-catalog. |
| `FEData\Data\nfs5.prt` | `0D 00 00 00 50 14 00 00` | Parts catalogue: count + offset, then per-car part lists by name (`"356"`, `"Engine"`, …). |
| `FEData\Data\nfs5.trn` | `01 00 00 00 09 02 00 00 …` | Transmission/gearbox tables (index + ratio arrays). |
| `FEData\Locale\festrings.loc` | `LOCH` (then `LOCI`) | Compiled locale string tables (front-end text). Its source `festrings.csv` is shipped too. |
| `FEData\Layouts\*.lay` | `[section.0]` | **Plain text** UI layout descriptions (ids, names, coordinates). |
| `SaveData\XXDefXX.sav` | `"Missions\0"` | Save game (missions/progress + money/cars). |
| `SaveData\nfs5.trk` | `08 00 00 00 2F 01 00 00 "coastal"` | Track records / best times per track. |
| `SaveData\*.hud` | binary | HUD element configuration. |
| `.key` | `01 00 00 00 <count>` + floats | Animation keyframes. |
| `.map` | `[header]\r\ncount=…` | **Plain text** track map metadata. |
| `.scn` | binary | Scene/animation definitions. |
| `.edg` | binary | Track edge/geometry collision data. |
| `.env` | `SHPI` | Environment-map texture package (same format as `.fsh`). |
| `.jnc` | floats | Junction/node data. |
| `.dtx` | 11 bytes | Tiny track descriptor/flags blob. |
| `.cop` | int array | Police/cop configuration per track (count, timings, flags). |
| `.lsp` | `01 00 00 00` + floats | Racing-line / "line start point" data for the AI. |
| `Sky\*.bin` | `07 00 00 00` + floats/bytes | Sky-dome vertex/colour data. |
| `.scc` | `34 12 01 00 …` | **Microsoft Visual SourceSafe** metadata (`vssver.scc`) — a leftover from EA's build system, not game data. |
| `.ini` / `.csv` / `.txt` | text | Config/data: `particle.ini`, `carpart.ini`, `packages.ini`, `tuning.csv`, `SCRIPTS.CSV`, `animdefs.txt`, `Sky\*.txt`. |

This independently **confirms the `nfs5.dll` `.fsh` patcher** in §2.5: the files it rewrites are
exactly the `SHPI` containers with `!pal` palette chunks.

### 5.2 What the Combined Patch changes in the data
Base ⇄ patch byte comparison (`Copy\…` vs `NFS5\…`):

**Added (new files):**
`GameData\CarModel\928.clr`, `GameData\Track\Cops\monaco1..5.cop` (the Monaco cop files,
matching change-list item 37), `SaveData\8Driver.sav` + `SaveData\player.lst` (the bundled
clean driver/save).

**Replaced with edits:**
* `FEData\Data\nfs5.car` — 2,306 bytes from `0x204` (car-catalog fields: rims, tachometers,
  prices, availability years, add-on-car entries).
* `FEData\Data\nfs5.prt` — size grows 245,880 → 267,840 (+21,960 B) with 127 bytes differing —
  the added **parts for the 4 add-on cars** (GT2/GT3/928 GTS/959).
* `FEData\Layouts\graphicsettings.lay` — 4,205 bytes differ, size −6: this is where the
  **widescreen option is removed from the Graphics menu** (change-list item 27).
* `FEData\Locale\festrings.loc` — 99 bytes (new localised strings for the add-on cars).
* `GameData\Render\common.fsh` — 47,878 bytes differ: **Smooth Shines and Lights**.
* `GameData\DashHud\speedKN.fsh` — 11,580 bytes (HUD/speedometer art fix).
* ~90 car `.fsh` files (`*D`/`*L` = detail/lo, `cop*`, `eas*`, `eden*`, `f*`, `gt*`) — typically
  9–13 KB each changed, i.e. **new/edited car textures (rims, tachos, add-on cars)**.
* `GameData\CarModel\356_1.crp` / `356b.crp` — rebuilt car geometry (high-detail car models and
  the 2 hidden LOD levels; `356_1.crp` grows 1,309,185 → 1,315,342).
* `GameData\CarModel\cop356_german.tpg`, `eas11D/L`, `eden1D/L` — trim/colour textures
  (note `eas11D/L` and `eden1D/L` effectively **swap sizes**, i.e. assets were rebalanced).
* `GameData\Simulation\CarData\993coupeS36.sim` — **the only simulation file edited**: five
  floats at `0x6c–0x7f` (the gear ratios) change
  `2.048/1.407/1.118/0.928/0.775 → 2.150/1.560/1.242/1.024/0.820` — the gearbox-final-drive fix.
* `SaveData\XXhud1XX.hud` / `XXhud2XX.hud` — a few bytes (new default HUD layout).

**Unchanged (verbatim copies):** `nfs5.trn`, `garage/loadreplay/usedcars.lay`,
`festrings.csv`, `hudstrings.h`, and the `928/959/gt2/gt3` `.crp`/`.clr`/`.tpg` and
`928coupeGTS54.sim`/`959coupe29.sim` (the add-on cars were already present in the base build).

> Cross-check: the base exe is already the "Enhanced 3.5.20040310" build (add-on cars present),
> so most of the patch's *art/parts* work is the Combined Patch's own changes, not the add-on
> car install.

---

*(Appendix A — file hashes — is at the end of this document.)*

## 6. Evidence, method and limits

* Executable identity is established by SHA-256 of the on-disk files and by REA Evidence:
  * `nfs5.exe` PE resources ⇒ `ev_8a0c1b7ea99cc3783ad4e54ccfb3038d3b9e4703f8cb30215ddc554698f4a390`;
  * `nfs5.dll` Ghidra `binary_overview` ⇒
    `ev_74bb1145831b76621a50fe93963b3067a3d200e2f53445c8054ecdcd7a82dc18`
    (provider `rea-workflow`/Ghidra 12.1.4; 7 segments, **55 procedures**, 114 strings,
    confidence `derived`, authority `shipped-artifact`).
* The code-level claims in §2 come from my own disassembly (capstone) of `nfs5.dll` and of
  `Porsche.exe`, cross-checked against the pre-existing Ghidra decompilation in
  `C:\Users\CB\Desktop\NFS5 Decomp\`.
* **REA's Ghidra bridge is unreliable on this host**: it returned `provider_timeout` after 330 s
  (`list_documents`) on three of four attempts and, for `RegSetup.exe`, failed with Defender's
  `ERROR_VIRUS_INFECTED`. One `nfs5.dll` run did succeed (cited above). The non-Ghidra REA tool
  `inspect-pe-resources` worked every time and is cited. Where Ghidra did not return, the
  capstone disassembly above is the substitute and is labelled as such.
* No game or patch binary was executed. No runtime behaviour was observed.
* Unknowns: the exact semantics of hook handlers `0x10001fd0` (which calls the `.fsh` patcher
  `0x10001c20` then tail-jumps through `[0x10006048]`) and `0x100036f0` (the DDraw-init hook);
  the meaning of the `0x5de6c4` table it zeroes; and whether the quarantined `RegSetup.exe`
  payload is *genuinely* Alureon or an aggressively packed fan installer that happens to trip the
  same signatures (Defender's several independent, currently-**active** detections plus
  aPLib + `Obfuscator.ER` point to the former; the file could not be re-analysed because Defender
  removed it).

---

## 7. Full reversal of the patch — how to rewrite the game

`nfs5.dll` is now fully reversed. All 58 functions were decompiled (Ghidra 12.1.4) and every
in-place edit has been enumerated. Artifacts live in **`C:\Users\CB\Desktop\NFS5 Decomp\patch\`**
(alongside the game's existing decompilation in `..\decomp\`):

| file | contents |
|---|---|
| `RECONSTRUCTION.md` | the complete write-up: load/patch model, the two hook handlers, the orchestrator, the config loader, a 58-row function inventory, the global-variable map, and folding instructions |
| `nfs5.dll.decompiled.c` | C for **all 58 functions** (23 auto + 35 force-recovered) |
| `nfs5.dll.patch_table.txt` | all **132** `VirtualProtect` sites: address, size, kind (`NOP` / `E9` detour / `E8` call / `EB` force-branch / pointer write), computed DLL target, and containing game function |
| `nfs5.dll.full_disasm.txt` | IAT-annotated x86 listing |
| `nfs5.dll.functions.txt`, `nfs5.dll.strings.txt` | inventories |
| `config.exe.strings.txt` | the settings schema embedded in `config.exe` |

**A complete rewrite now only needs two inputs, both present:** the game's decompiled source
(`..\decomp\`, 8,368 functions / 360 RTTI classes) and this patch. Apply the 132-row table
mechanically — replace each `E9` site's original statement block with a call to the named
handler (preserving the naked-detour contract and its saved continuation), delete each `NOP`
run, make each `EB` branch unconditional, and substitute each pointer/constant write
(`.\gamedata\…` paths, `250000.0f`/`125000.0f` CPU clamps, FOV/aspect globals) — and the
result is a single self-contained `Porsche.exe` with no DLL injection.

**Runtime verification (done).** The patch was confirmed against the **live game process** by
read-only memory inspection (no breakpoints, no writes): `nfs5.dll` loaded at `0x6f5a0000` with
every detour/NOP/pointer redirected exactly as modelled, the language override proven
(`mov eax,[0x6f5a60a0]`), the portable path table pointing at the DLL's strings, the
Fog/Filtering filter installed at game `0x006bd97c`, and the config globals matching
`nfs5.ini`. `FUN_10001c20`'s arity was settled from `ret 0xc` plus the verified calling
convention, and the widescreen FOV formula was recovered from the live constants
(`atan2(4.0, 480·aspect·3)` for aspect > `4/3`, else `37.5`). `0x10006080` was also proven to be
a drop-in for the engine's own `37.5` FOV constant (the patched operands were literally
`[0x005b3c9c]`) and to be in **degrees** (the code converts rad→deg via `π/180`). Raw dumps:
`runtime_verify_1.txt` … `runtime_verify_4_inrace.txt` in the same folder. In-race sampling
confirmed the FOV variable stays at the original `37.5` because the game's *internal* mode is
640×480 (4:3) — the widescreen branch needs an internal widescreen mode, not just a renderer
setting.

---

## Appendix A — file identity (SHA-256)

| File | Bytes | SHA-256 |
|---|---|---|
| `NFS5\Porsche.exe` | 2,129,965 | `e4684948ca9bfb552a0fc0eeb7024a13bfe67f2e40ebb6ba121ae6a81dd166bb` |
| patch `Copy\nfs5.exe` | 2,002,989 | `c318af393d4c2b7071a820ca3d69e956bbd94769b6e5c2e3e49f498ecbc94906` |
| patch `Copy\nfs5.dll` | 20,480 | `f24b5c82404f403645e5bc3e52d47e4dd6d19bd92613c243ac31bea285fce758` |
| patch `Copy\config.exe` | 106,496 | `3390f8d9badb157608a2161fe348178237842ad4fd0f0971d1c4f9a8541e96f0` |
| `NFS5\gimme.dll` (FaceInRace.dll) | 4,599,863 | `bbe31b8e8102e7d67e0e748287b503378df9798996e2abb8a96b0bc4ee1837b0` |
| `NFS5\mrbupd.dll` | 102,400 | `53caa8293f2bcac23f1f80b6f2a605f6b9cf801554fd92c09a068d2220235722` |
| `NFS5\Language Selector.exe` | 192,000 | `a0571d4c176fe063cefb463a804dbad5d4caa504a0f2ec4dee13881a3b78b01c` |
| `NFS5\Drivers\dx7z.dll` (EA 2001) | 110,592 | `1ddc82718e65cdc3bc8efb09bc77c3ac90070952e777275e6f594951c66703c7` |
| patch `Drivers\opengl3\opengl3z.dll` | 53,248 | `4fc2fefdd45e912cefa2034ee73254f86e027106cc86a878028450c4abf6d6ea` |
| `NFS5\RegSetup.exe` (**quarantined by Defender**) | 222,209 | `d493238e0b46bcd5e0b4ae09b3197498b8fb2423d83692c36b99f6d98d34e649` |
