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
| Retail results screen | Mid | OpenCode (continuing) | `claude/results-screen-a9f5cc` | merged, built, tests pass, `race_drive` ALL OK; visual not checked by eye | Next: capture `logs/menu_results*.png` (finish a race) and compare the panel's look/position/timing with retail; add to PLAYTEST | Finishing shows the retail table, Space/Esc go to the menu |
| Route choice at AI junctions | Strong | OpenCode (continuing) | `claude/ai-routes-be7221` | merged, built, tests pass, `race_drive` ALL OK on all 15 tracks; the `junctions: N` log line is **not added yet**, so the `.jnc` weight check is still to do | Next: add a log line counting junctions and their route weights (`.jnc`) and compare on castle/industrial/autobahn; then playtest | Opponents take branches like retail (log); race_drive ALL OK; tests pass |
| Damage, suspension and roll-overs | Strong | Claude (damage session) | `claude/damage-6135c5` | Done on branch: damage zones + effects, tumble/roll-over, crash timer recovery, suspension, D.fsh stamps, Damage option; tests pass | Next: coordinator merge after race_drive; playtest (PLAYTEST.md). Open: zone side signs, parts zone masks, L.fsh purpose, [damagezones] unused | Cars sit on suspension, roll over and recover, show retail damage; race_drive ALL OK |
| Race sound effects polish | Mid | OpenCode | — | 2026-10-22: verified working from a race's `logs/race_sound.log` (engine layers, SND_coll skid/road/wind/wall, countdown voice, 75 coastal ambience including water lapping; levels follow the Options sliders). No specific fault found; the mix balance (MIX 0.7) and the missing Doppler/rain/opponent-skid channels are the open items | Next: wait for the user's ear (PLAYTEST), then adjust `MIX`/channel levels or add rain/opponent skids; `docs/provenance/sound.md` "Not done" lists the gaps | Sounds match retail |

## Up next (unclaimed)
Pick from here, highest first. See `docs/progress.md` for the full feature list.

| Task | Tier | Notes |
|---|---|---|
| Last slab-allocator "Use-after-free" error (one per race) | Mid | Likely fix on master (sky meshes start non-empty); not reproduced here, needs playtest |
| Audio sliders other than music have no effect | Mid | Menu sounds now follow the effects slider; in-race levels were already wired (031dcef) but not confirmed by ear |
| Côte d'Azur sea water not visible | Mid | **Claude `claude/water-cockpit-213623`, claimed 2026-10-22.** OpenCode's water planes (3efc9be) draw nothing in the user's playtest |
| Cockpit: hand position on the wheel and hand animation | Mid | **Claude `claude/cockpit-hands-afa0a8`, claimed 2026-10-22.** Wheel smoothing done (merged) |
| Car reflections (paint and chrome environment map) | Strong | User's next ask for cars |
| Menu button outline light that travels around the border | Mid | Needs retail footage (user can supply) |
| Race sound fine-tuning against retail | Strong | Needs a retail recording with game sound |
| Body lean closer to retail | Strong | After damage/suspension merges |
| Playtest round: 2026-10-20 fixes, Options > Audio, race sound | User | Flare on Côte d'Azur, slider dragging and sound levels by ear are not tested |
| Traffic | Strong | Needs decomp work (AIHigh_Traffic) |
| Roof up/down drawing | Mid | Choice already reaches the race |
| Location screen 3D track room | Mid | |
| Evolution career, Factory Driver | Strong | Big; split into pieces first |
| Keep `docs/progress.md` current and rerun the progress tool | Light | After each merge |

## Done (recent)

| Task | Merged as |
|---|---|
| Retail race summary panel after the finish | `a50c753` |
| Damage zones, roll-over reset, spring suspension, damage option | `42ab8d9` |
| Body lean over planted wheels + cockpit arms and turning steering wheel (2026-10-22 user ask) | `opencode/lean-cockpit` (branch; merged into master) |
| Cote d'Azur sea/water planes (`WATER_MID2` index-less triangles) | `opencode/lean-cockpit` (branch; merged into master) |
| Verified the three merged session branches: build, 336 tests, `race_drive` ALL OK | master `4c9109a` |
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
| **Strong** (Sonnet, high effort; coordinator only on Opus) | Reading the decomp and deciding what retail does; physics and AI behaviour; rendering design; anything new across several systems; merges with conflicts | Traffic, damage, in-race sound mixing, career modes, matching handling to retail |
| **Mid** (Sonnet) | Building a feature whose retail behaviour is already written down in `docs/provenance/`; front-end screens from known layouts; fixing a bug with a clear repro | Results screen, roof drawing, options pages, a crash with a log |
| **Light** (Haiku-class, or any cheap agent) | Mechanical work with a clear check: running the tests and `race_drive` and reporting; updating docs, tables and `docs/progress.md`; format dumps and audits with existing examples; tidying warnings; captures and log summaries | "Run `race_drive`, report failures", "rerun the progress tool", "add the new branch to Done" |

A light agent that finds the task needs judgement (a retail behaviour question, a design choice)
writes what it found into the task's **Next step** and stops rather than guessing.
