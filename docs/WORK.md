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
| Options menu: audio settings work and persist | Mid | Claude session `options-audio` | `claude/options-audio-a28663` | Building; 7 files uncommitted (2026-10-22) | Finish build, run tests, commit | Audio sliders move, persist, change music live; tests and `race_drive` pass |
| In-race sound: engine, announcer, skids, ambience | Strong | Claude session `race-sound` | `claude/race-sound-e8aa02` | Building; 12 files uncommitted (2026-10-22) | Finish build, run tests, commit | Race has engine, countdown voice, skids, ambience; tests and `race_drive` pass |

## Up next (unclaimed)
Pick from here, highest first. See `docs/progress.md` for the full feature list.

| Task | Tier | Notes |
|---|---|---|
| Playtest round for the 2026-10-20 fixes | User | Flare occlusion on Côte d'Azur is not tested |
| Retail results screen | Mid | Layouts are in `FEData`; placeholder exists |
| Route choice at AI junctions | Strong | `ai.rs`, `aidata.rs` |
| Traffic | Strong | Needs decomp work (AIHigh_Traffic) |
| Damage and roll-overs | Strong | Physics plus car model |
| Roof up/down drawing | Mid | Choice already reaches the race |
| Location screen 3D track room | Mid | |
| Evolution career, Factory Driver | Strong | Big; split into pieces first |
| Keep `docs/progress.md` current and rerun the progress tool | Light | After each merge |

## Done (recent)

| Task | Merged as |
|---|---|
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
