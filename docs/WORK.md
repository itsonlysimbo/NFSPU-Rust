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
| Route choice at AI junctions | Strong | Claude (ai-routes) | `claude/ai-routes-be7221` | Verified 2026-10-22: `.jnc` counts match on 7 tracks, choices follow weights/difficulty (`docs/provenance/ai.md`) | Merge the branch; playtest item added | Opponents take branches like retail (log); race_drive ALL OK; tests pass |
| Damage, suspension and roll-overs | Strong | OpenCode (continuing) | `claude/damage-6135c5` | merged, builds, tests pass; `car_damage.rs`, `take_hit`/`recover` and the `susp`/`tumble` fields are in, but the `car.rs` tick does **not** integrate them yet (suspension, tumble spring, crash timer, landing zone 8 are still to wire) | See `docs/provenance/damage-notes.md`. Next: in `car.rs` tick integrate tumble (spring to rest 0 / side / roof via TIP, SIDE_TO_ROOF), crash timer, suspension (susp/susp_vel/susp_road), landing hit zone 8; pass aero to fall_accel; take_hit in collide_cars; handle wants_recover (draw_pos, wheel_drop); D.fsh stamps in car_model; build + tests + race_drive | Cars sit on suspension, roll over and recover, show retail damage; race_drive ALL OK |
| Race sound effects polish | Mid | OpenCode | — | 2026-10-22: verified working from a race's `logs/race_sound.log` (engine layers, SND_coll skid/road/wind/wall, countdown voice, 75 coastal ambience including water lapping; levels follow the Options sliders). No specific fault found; the mix balance (MIX 0.7) and the missing Doppler/rain/opponent-skid channels are the open items | Next: wait for the user's ear (PLAYTEST), then adjust `MIX`/channel levels or add rain/opponent skids; `docs/provenance/sound.md` "Not done" lists the gaps | Sounds match retail |

## Up next (unclaimed)
Pick from here, highest first. See `docs/progress.md` for the full feature list.

| Task | Tier | Notes |
|---|---|---|
| Playtest round: 2026-10-20 fixes, Options > Audio, race sound | User | Flare on Côte d'Azur, slider dragging and sound levels by ear are not tested |
| Traffic | Strong | Needs decomp work (AIHigh_Traffic) |
| Roof up/down drawing | Mid | Choice already reaches the race |
| Location screen 3D track room | Mid | |
| Evolution career, Factory Driver | Strong | Big; split into pieces first |
| Keep `docs/progress.md` current and rerun the progress tool | Light | After each merge |

## Done (recent)

| Task | Merged as |
|---|---|
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
| **Strong** (Opus-class) | Reading the decomp and deciding what retail does; physics and AI behaviour; rendering design; anything new across several systems; merges with conflicts | Traffic, damage, in-race sound mixing, career modes, matching handling to retail |
| **Mid** (Sonnet-class) | Building a feature whose retail behaviour is already written down in `docs/provenance/`; front-end screens from known layouts; fixing a bug with a clear repro | Results screen, roof drawing, options pages, a crash with a log |
| **Light** (Haiku-class, or any cheap agent) | Mechanical work with a clear check: running the tests and `race_drive` and reporting; updating docs, tables and `docs/progress.md`; format dumps and audits with existing examples; tidying warnings; captures and log summaries | "Run `race_drive`, report failures", "rerun the progress tool", "add the new branch to Done" |

A light agent that finds the task needs judgement (a retail behaviour question, a design choice)
writes what it found into the task's **Next step** and stops rather than guessing.
