# AGENTS.md

## Project
A from-scratch Rust/Bevy/wgpu rewrite of Need for Speed: Porsche Unleashed (NFS5, 2000). It reads track, car and texture data from the player's own install at runtime. First goal: load one real track and drive around on it.

## Hard rules, never break these

1. **Never write game assets, decompiled code, or extracted game files into this repository.** They stay on this machine, untracked. Read game data from the install path at runtime, or extract to the gitignored `extracted/` folder.
2. **`.gitignore` is a whitelist.** It ignores everything and includes only source files. Do not switch it to a normal ignore list.
3. **Do not run `git commit` unless I asked.**
4. **Do not touch anything outside this project folder** unless I explicitly name the path. The decomp project at `C:\Users\CB\Desktop\NFS5 Decomp` is **read-only reference**: never modify, move or delete anything in it. My game installs at `C:\Users\CB\Downloads\NFS5 - Copy` (the real, played install) and `C:\Users\CB\Downloads\NFS5` are also read-only. Any patching or testing happens in a separate copy, with a backup first.
5. **Single-player and offline only for now.** Online play is a long-term goal, to be built as new netcode with no EA services. Stop and ask before any networking work. Never bypass DRM or anti-cheat.
6. **Never put credentials in the repo.**
7. **Own structure, not copied code.** When using the decompile as reference, write original code and notes. Do not copy decompiled code, its names, quirks or dead code into the repo. Format notes go in `docs/provenance/` as prose and tables.
8. **Ask before** installing system software, running unsigned binaries (the patch's `config.exe`, `nfs5.dll` are unsigned), changing registry or graphics settings, or deleting anything.

## How to work
- Plan in `docs/DESIGN.md`, build one small step at a time, run the build before saying it's done.
- Log numbers (sizes, counts, positions) to `logs/`; I can't see the game, the user playtests.
- The user authorizes opening and closing the game as needed for testing (2026-10-15). Announce live tests briefly so intentional test exits are clear. Game install/decomp remain read-only; retail tests that may write settings or saves must use a separate copy with a backup first.
- Target: recreate the retail game as closely as possible (1:1 where practical). Work continuously through small, evidence-based steps; keep guesses clearly separate from verified behavior.
- After every change, give the exact command to run and what the user should see.
- User is a beginner who wants things explained ("teach me as we go"), but updates stay short (1-3 lines) unless asked.
- Write **"not tested"** for anything unverified. After two failed attempts at one problem, stop and write `STATUS.md`.
- Update `MODLOG.md`, the README works/doesn't-work list and `STATUS.md` only when a big milestone is complete (e.g. "quick race works end to end"), not after every step (user, 2026-10-16). Provenance notes in `docs/provenance/` are still written as findings are made.

## Parallel sessions (user, 2026-10-17)
- Big goals are split into parallel sessions, each in its own git worktree and branch. Keep to about 3-4 at once on this 16 GB machine.
- Each session keeps to its own files, commits to its branch and never pushes. The coordinating session merges and tests each branch as soon as it lands, so later work starts from current code.
- The user playtests once per milestone, with one recording (game sound on) of everything they noticed.

## Builds
- `.cargo/config.toml` links with `rust-lld` (ships with Rust; much faster than link.exe).
- The dev profile keeps line tables only (`debug = "line-tables-only"`, none for dependencies).
- sccache is installed (winget) and set as the rustc wrapper in the user's `~/.cargo/config.toml`, so all worktrees share one compile cache.
- For quick edit-build-run loops use `cargo run -p nfspu-game --features dev -j 2`, which links Bevy as a DLL. Leave `dev` off for builds the `run_*.bat` launchers start (they need no DLL).
- Disk: each worktree's `target/` grows to 5-40 GB (that was before the lighter debug info). Build inside your own worktree's `target/`, not in extra `target-*` folders. After merging a branch, the coordinator deletes that worktree's `target/` (build output only, regenerable). sccache caps its cache at 10 GB by default.
- Long builds: run them in the background rather than under a short timeout. A killed link shows as `link.exe failed: exit code 143`.

## Decomp lookups (Ghidra bridge)
- `.mcp.json` registers the GhidraMCP bridge as the `ghidra` tool server: decompile, list and search functions, xrefs, strings. It works while the user has Ghidra open on the NFS5 project with the GhidraMCP plugin on (port 8080).
- Its write tools (rename_*, set_*) are denied in `.claude/settings.json`, because the decomp project is read-only. Never work around that.
- Without the bridge, fall back to searching `decomp/classes` / `decomp/src` (not the whole folder; that times out).

## Environment
- OS: Windows 10 Pro (build 26200), 16 GB RAM, Intel UHD + NVIDIA RTX 4050 laptop GPU (can be selected per app)
- Game: NFS Porsche Unleashed community "Multi6 Enhanced 3.5" build, `Porsche.exe` (v3.5), at `C:\Users\CB\Downloads\NFS5`
- Runtime comparison copy: patched `nfs5.exe` from Combined Patch v1.1 (to be made in a separate copy, not yet made)
- Language: Rust stable 1.99 (MSVC), Bevy 0.19.1
- PATH note: in agent shells run `$env:PATH="$env:USERPROFILE\.cargo\bin;C:\Program Files\Git\cmd;$env:PATH"` first
- Real install (patched, played, holds saves): `C:\Users\CB\Downloads\NFS5 - Copy`. Its `SaveData` has the user's test profile `3Greg.sav` (made by the user, not shipped) and `8Driver.sav`.
- Game dir env var for tools: `NFSPU_GAME_DIR=C:\Users\CB\Downloads\NFS5 - Copy` (the `run_*.bat` launchers use it)
- Agent: OpenCode

