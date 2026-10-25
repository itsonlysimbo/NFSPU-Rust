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
- When Claude runs out of usage, the user's other agents (e.g. OpenCode) may keep working directly in the main checkout on `master`, without committing. Before merging, the coordinator runs `git status`. Any uncommitted work is the user's: commit it to an `other-agent/<date>` branch and merge it like a session branch. Never discard it.
- The user playtests once per milestone, with one recording (game sound on) of everything they noticed.

## Hand-offs (user, 2026-10-22)
Several different agents (Claude, OpenCode, others) and model sizes work on this project, and any of
them may stop mid-task (usage limits). Leave every task so the next agent can pick it up cold.
- **Start:** read this file, then `HANDOFF.md`, then `docs/WORK.md` (the work board). Run `git status`
  and check the worktrees for unsaved work (commands in `HANDOFF.md`) before anything else.
- **Claim:** put your agent name and branch in the task's row in `docs/WORK.md` before starting.
  Don't take a task someone else holds unless its last update is over a day old and its branch is
  committed.
- **Main checkout stays on `master`:** never run `git checkout`/`switch` in
  `C:\Users\CB\Desktop\NFSPU-Rust` itself (2026-10-23 a session switched it to its branch and a
  merge landed there). Work only in your own worktree.
- **Branch:** one task, one branch. Committing to your own task branch is always allowed; committing
  to `master` still needs the user's OK (rule 3) or a coordinator merge.
- **Checkpoint:** commit at each working step (`WIP: ...` is fine) and update the task's **Next step**
  line: the one concrete thing to do next, plus anything you learned that isn't in the code
  (dead ends, what was tested, what's not tested).
- **Stop:** never end with uncommitted work. If you run out mid-change, commit it as `WIP:` with a
  Next step saying what is half-done.
- **Tier:** each task on the board has a tier (Strong / Mid / Light; table at the bottom of
  `docs/WORK.md`). A lighter model that hits a judgement call writes it into Next step and stops.
- **Finish:** the coordinator (or whoever merges) moves the task to Done with the merge commit and
  updates `docs/progress.md`.

## Continuous running (user standing approval, 2026-10-22, until the user says stop)
These override "ask first" items above where they overlap. Hard rules 1, 2, 4-8 still hold.
- Coordinator commits and merges to `master` without asking once tests and `race_drive` pass, and
  pushes `master` to GitHub after each merge.
- Keep 3 sessions running. When one finishes, merge it and start the next task from `docs/WORK.md`,
  highest first, without asking.
- Sessions commit to their own branch freely and run in auto permission mode.
- **Auto-continue (user, 2026-10-24):** the app refuses new sessions from automated turns, so when
  a session's branch is merged, the coordinator sends that same session the next unclaimed task
  from `docs/WORK.md` "Up next". The session then, in its own worktree only:
  `git fetch` (worktrees share the repo), `git checkout -b claude/<task> master`, claims the row
  and works as usual.
- New sessions use Sonnet or lower (user, 2026-10-22): Sonnet for Strong and Mid tasks, Haiku for
  Light ones. Only the coordinator runs on Opus.
- Delete `target/`, finished worktrees and merged branches after merging, without asking. Only
  worktrees under `.claude/worktrees/` and `claude/*` branches: never remove Orca's workspaces
  (`C:\Users\CB\orca\...`, `itsonlysimbo/*` branches) or `other-agent/*` branches.
- Live game tests, captures and launches need no announcement.
- Don't wait for playtests: add what needs the user's eyes and ears to `docs/PLAYTEST.md`.
- Stop and ask only for: networking, unsigned programs, installing software, anything outside the
  project folder, or a real design choice the evidence can't settle. Otherwise decide, write it
  down as a guess, and continue.
- Keep the computer awake while work runs.
- After each major milestone (user, 2026-10-23): update `docs/progress.md`, rerun the progress tool,
  and refresh the "NFSPU rewrite progress" dashboard
  (https://claude.ai/artifact/84fMsfSDpfdvNn4PCQapD1) from that table.
- **Oracle (user, 2026-10-24):** `docs/ORACLE.md`. Before every merge the coordinator runs
  `bash scripts/merge_guard.sh <branch>`; a branch changing protected checker files (list in
  `scripts/protected.txt`) is merged only after the coordinator reads that diff
  (`--allow-oracle`). Once `oracle.rs` exists, `ORACLE PASS` joins tests and `race_drive` as a
  merge condition. Never loosen a tolerance or edit a reference trace to pass.
- **Context upkeep (user, 2026-10-24):** sessions compact at 40% of their context
  (`CLAUDE_AUTOCOMPACT_PCT_OVERRIDE` in `.claude/settings.json`), and the coordinator's hourly
  check-in tells each working session to re-read AGENTS.md and its board row.
- **Discord (user, 2026-10-24):** every agent (Claude, OpenCode, Codex) posts to the project channel when it starts, finishes or gets blocked on a task: `python scripts/discord_bridge.py post "<agent> (<branch>): <one line>"`. The user's replies land in `logs/discord_inbox.md`; the coordinator reads and forwards them. WORK.md stays the durable record.
- When Claude usage runs out, every task is left as a committed WIP with a Next step on the board
  so OpenCode can continue.

### Automatic handoff to OpenCode (user, 2026-10-22)
The coordinator checks the plan usage (`get_usage`) each time a session reports. At **90%** of the
5-hour or weekly limit:
1. Tell every running Claude session to commit its work as `WIP:` and write its Next step in
   `docs/WORK.md`, then stop. Do the same for the coordinator's own work.
2. Commit the board on `master` and push.
3. Start OpenCode in the background from the project folder (the user approved running this):
   `C:\Users\CB\bin\opencode.cmd run --auto -m opencode-go/deepseek-v4.1-flash --title "NFSPU handoff <date>" "<prompt>"`
   with the prompt: *Read AGENTS.md, HANDOFF.md and docs/WORK.md. Continue each Active task whose
   last commit is WIP, one at a time, in its worktree under .claude/worktrees on its own branch.
   Follow its Next step, build with -j 2, run the tests and race_drive (NFSPU_GAME_DIR set), commit
   to that branch and update its row. Do not merge to master, push, or start new tasks.*
4. When Claude usage resets, the coordinator reads what OpenCode committed, checks it, and merges.

## Builds
- `.cargo/config.toml` links with `rust-lld` (ships with Rust; much faster than link.exe).
- The dev profile keeps line tables only (`debug = "line-tables-only"`, none for dependencies).
- sccache is installed (winget) and set as the rustc wrapper in the user's `~/.cargo/config.toml`, so all worktrees share one compile cache.
- For quick edit-build-run loops use `cargo run -p nfspu-game --features dev -j 2`, which links Bevy as a DLL. Leave `dev` off for builds the `run_*.bat` launchers start (they need no DLL).
- New worktrees seed `target/` from the main checkout's (`.claude/settings.json` SessionStart hook,
  robocopy without `incremental/`), so a first build only recompiles our own crates. If the hook
  didn't run, do it by hand before the first build:
  `robocopy ..\..\..\target target /E /MT:8 /XD incremental`.
- Disk: each worktree's `target/` grows to 5-40 GB (that was before the lighter debug info). Build inside your own worktree's `target/`, not in extra `target-*` folders. After merging a branch, the coordinator deletes that worktree's `target/` (build output only, regenerable). sccache caps its cache at 10 GB by default.
- Long builds: run them in the background rather than under a short timeout. A killed link shows as `link.exe failed: exit code 143`.

## Decomp lookups (Ghidra bridge)
- `.mcp.json` registers the GhidraMCP bridge as the `ghidra` tool server: decompile, list and search functions, xrefs, strings. It works while the user has Ghidra open on the NFS5 project with the GhidraMCP plugin on (port 8080).
- Its write tools (rename_*, set_*) are denied in `.claude/settings.json`, because the decomp project is read-only. Never work around that.
- A session that started before Ghidra was opened can query the plugin directly, GET only (read-only), e.g. `curl -s "http://127.0.0.1:8080/decompile_function?address=004ae8c0"`. Other paths: `searchFunctions?query=`, `xrefs_to?address=`, `xrefs_from?address=`, `function_xrefs?name=`, `disassemble_function?address=`, `strings?filter=`. Never POST (that renames or comments).
- Without the bridge, fall back to searching `decomp/classes` / `decomp/src` (not the whole folder; that times out).

## Environment
- OS: Windows 10 Pro (build 26200), 16 GB RAM, Intel UHD + NVIDIA RTX 4050 laptop GPU (can be selected per app)
- Game: NFS Porsche Unleashed community "Multi6 Enhanced 3.5" build, `Porsche.exe` (v3.5), at `C:\Users\CB\Downloads\NFS5`
- Runtime comparison copy: patched `nfs5.exe` from Combined Patch v1.1 (to be made in a separate copy, not yet made)
- Language: Rust stable 1.99 (MSVC), Bevy 0.19.1
- PATH note: in agent shells run `$env:PATH="$env:USERPROFILE\.cargo\bin;C:\Program Files\Git\cmd;$env:PATH"` first
- Real install (patched, played, holds saves): `C:\Users\CB\Downloads\NFS5 - Copy`. Its `SaveData` has the user's test profile `3Greg.sav` (made by the user, not shipped) and `8Driver.sav`.
- Game dir env var for tools: `NFSPU_GAME_DIR=C:\Users\CB\Downloads\NFS5 - Copy` (the `run_*.bat` launchers use it)
- Agents: Claude Code (coordinator and sessions), OpenCode, others; see "Hand-offs"

