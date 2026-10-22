# Work board

The one place any agent (Claude, OpenCode, Codex, Cursor, ...) looks to see what is being worked
on, by whom, and exactly where it stopped. Read `AGENTS.md` first, then this file.

**Rules (short form; full rules in `AGENTS.md`, "Hand-offs"):**
- Claim a task by putting your agent name and branch in its row *before* you start.
- Every task has a **Next step** line: one concrete action the next agent can start on cold.
  Update it each time you commit, and always before you stop.
- Commit work in progress to the task's branch (`WIP: ...` messages are fine). Never end a
  session with uncommitted work.
- When a task is merged, move it to **Done** with the merge commit.
- **Tier** says how strong a model the task needs (see the bottom of this file).

## Active

| Task | Tier | Agent | Branch | State | Next step | Done when |
|---|---|---|---|---|---|---|
| Route choice at AI junctions | Strong | Claude (ai-routes) | `claude/ai-routes-be7221` | WIP, not built yet | Retail rules found and coded (`ai_route.rs`, `docs/provenance/ai.md`). Next: `cargo build -p nfspu-game --example race_drive --release -j 2`, fix compile errors, run race_drive on junction tracks (e.g. castle, industrial, autobahn with 7 opponents) and check `junctions: N (.jnc weights)` matches .jnc count; then all 15 tracks + `cargo test --workspace -j 2` | Opponents take branches like retail (log); race_drive ALL OK; tests pass |

## Up next (unclaimed)
Pick from here, highest first. See `docs/progress.md` for the full feature list.

| Task | Tier | Notes |
|---|---|---|
| Playtest round: 2026-10-20 fixes, Options > Audio, race sound | User | Flare on Côte d'Azur, slider dragging and sound levels by ear are not tested |
| Retail results screen | Mid | Layouts are in `FEData`; placeholder exists |
| Traffic | Strong | Needs decomp work (AIHigh_Traffic) |
| Damage and roll-overs | Strong | Physics plus car model |
| Roof up/down drawing | Mid | Choice already reaches the race |
| Location screen 3D track room | Mid | |
| Evolution career, Factory Driver | Strong | Big; split into pieces first |
| Keep `docs/progress.md` current and rerun the progress tool | Light | After each merge |

## Done (recent)

| Task | Merged as |
|---|---|
| In-race sound: engine, announcer, skids, ambience, wind | `fb5a9a5` (branch), wired to Options levels on master |
| Options > Audio sliders, saved to `settings/audio.txt` | `6259450` (branch) |
| Work board and hand-off rules | `52d4f1e` |
| Progress treemap tool and checklist | `5844a8d` |
| Empty effect meshes (slab allocator errors) | `a415ff7` |
| Sun flare hidden behind terrain | `2ec0561` |
| Retail gravity, landings, body lean, wet grip | `f6ecc30` |
| Car smooth shading, full paint list, custom colour | `cf2d8fe` |
| Menu transitions, hover animation, dropdown fix | `70cf5ea` |

## Tiers: which model for which work

| Tier | Use for | Examples here |
|---|---|---|
| **Strong** (Opus-class) | Reading the decomp and deciding what retail does; physics and AI behaviour; rendering design; anything new across several systems; merges with conflicts | Traffic, damage, in-race sound mixing, career modes, matching handling to retail |
| **Mid** (Sonnet-class) | Building a feature whose retail behaviour is already written down in `docs/provenance/`; front-end screens from known layouts; fixing a bug with a clear repro | Results screen, roof drawing, options pages, a crash with a log |
| **Light** (Haiku-class, or any cheap agent) | Mechanical work with a clear check: running the tests and `race_drive` and reporting; updating docs, tables and `docs/progress.md`; format dumps and audits with existing examples; tidying warnings; captures and log summaries | "Run `race_drive`, report failures", "rerun the progress tool", "add the new branch to Done" |

A light agent that finds the task needs judgement (a retail behaviour question, a design choice)
writes what it found into the task's **Next step** and stops rather than guessing.
