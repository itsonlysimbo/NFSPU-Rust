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
| Collision audit + stuck recovery | Mid | Claude (collision) | claude/collision-f04eb2 | done, awaiting merge (race_drive ALL OK 15 tracks, workspace tests pass) | `collision_audit` example (logs/collision_audit.log) and `stuck.rs` (player stuck 320 ticks / fell / 35 m out / airborne 4 s resets, wired in race_mode and race_drive). Audit residue: a few edge slivers, OPEN edges with void (skidpad etc., now recovered by the fall rule). Not tested in the game. | Run race_drive on all 15 tracks and `cargo test --workspace -j 2`; then look at OPEN runs (add edge walls?) |
| Car lamp glow + 'covered'; location header flags | Mid | Claude (glow-flags) | claude/glow-flags-e70b14 | done, awaiting merge | Glow on screen not seen (car starts on a slice with no glow section; needs a slice-jump hook). Flag overlay only unit-tested (2D map box hides the wall face). | merge |
| Cockpit hands colour + front wheel steer smoothing | Mid | Claude (hands-wheels) | claude/hands-wheels-39fd02 | done, awaiting merge | Hands/arm sleeve now navy like retail (page 9 is a white shirt texture; pale texels recoloured for arms+hands). Front wheels follow the sim steer angle at a capped rate (`WHEEL_STEER_RATE`). NOT DONE: the hand mesh includes a long forearm slab (z span 0.7) that swings with the wheel; retail keeps the arm pointing from the side; our cockpit eye sits lower/closer than retail (hub 0.86 vs 0.73 of height), so the slab looks bigger. Skin the hand forearm vertices like `skin_arms`. | merge |
| Career 3+4 slice 2: buy/sell cars, used lot, parts, paint shop, era advance | Strong | Claude (career-2) | claude/dazzling-bun-be66c4 | WIP, stopped for cloud move | DONE+tested: economy (career_shop.rs: retail value formula, used lot, parts, tuning), career.rs buy/sell/fit, career_ui.rs, ListBox renderer, dealership/usedcars/garage/paintshop/parts screens captured to logs/ (NFSPU_CAREER=1 NFSPU_CAREER_DO=...). Unit tests pass for career/career_shop/career_ui. NOT DONE: race with tuned car end-to-end (try NFSPU_MENU=tournament NFSPU_FLOW_TEST=1,12 NFSPU_CAREER_PLACE=2, look for 'career parts tune the car' in logs/m0.log); full `cargo test --workspace -j 2`; race_drive ALL OK; docs/PLAYTEST.md and docs/provenance/career.md notes (value formula tables, prt layout 0x168, part effects are guesses); era-advance uses slice-1 guess; repairs not built; click hit-testing of parts list only by eye | player can earn, buy, upgrade, race |
| Career slice 3: footage comparison, paint shop, test drive, era progression | Strong | Claude (career-footage) | claude/career-footage-0cff4a | done, awaiting merge | Tests (`cargo test --workspace`) and `race_drive` ALL OK; test drive run headless (NFSPU_MENU=carsetup NFSPU_CAREER=1 NFSPU_CAREER_TESTDRIVE=1 NFSPU_FLOW_TEST=1,12). Captures in logs/ours_*.png vs logs/footage_*.png. Next: feed showcase/car compare/car setup data (footage in career.md), retail test-drive HUD, stripe decal on garage cars, bonus prize-car race (VictoryRoom), real tournament routes (guess in `assign_routes`) | screens match footage, slice 3 works headless |

## Up next (unclaimed)
Highest first. The coordinator hands these to finished sessions automatically (AGENTS.md, "Auto-continue").

| Task | Tier | Notes |
|---|---|---|
| Oracle retail capture: record reference traces from retail in a copy of the install | Strong | *oracle* task. Stop and ask if it needs an unsigned tool or patch. `docs/ORACLE.md` |
| Career: showcase, car compare and car setup screens with real data | Mid | Left open by slice 3 (`career.md`) |
| Collision: close the remaining edge slivers and open edges from `logs/collision_audit.log` | Mid | Collision audit merged |
| Handling feel vs retail: tyres, engine and gearbox against the footage | Strong | Footage: `C:\Users\CB\Videos\2026-10-23 11-00-02.mp4` |
| Career: later eras' events and prize tables, full era progression | Mid | `career.md` plan |
| Save profiles: read retail .sav career progress (install read-only) | Mid | Our own format writes; never write to the install |
| Keep `docs/progress.md` current and rerun the progress tool | Light | After each merge |
| Menu button light, cockpit hands, pause menu: confirm by playtest | User | `docs/PLAYTEST.md` |

## Done (recent)

| Oracle runner | merged 38aef6a: ORACLE PASS on 10 cases (6 format, 4 self-reference sim, not retail); ORACLE PASS is now a merge condition |
| Task | Merged as |
|---|---|
| Knockout + quick knockout modes | merge 5e69472 |
| Cloud: race sound channels (rain, wet road, opponent skids/hits, Doppler, commentator) | `1695259` |
| Cloud: opponents and autopilot pass and brake for traffic | `8edda25` |
| AI route choice at junctions (verified against `.jnc` on 7 tracks) | `8d39abc` |
| Car paint sun highlight and lamp glow | `845babe` |
| Côte d'Azur water, smooth cockpit wheel | `510f236` |
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

## Active: BLOCKED: footage-car needs local video measurements (Claude, branch claude/footage-car-7859f5)
- Done: `LEAN_LOOK` 0.6 -> 1.0 (356 footage 6-10 deg roll in hard corners). Notes in physics.md 7c, car.md.
- BLOCKED: the GT1 lean, cockpit hand positions and camera position/FOV/lag were never measured (physics.md 7c and car.md say "not measured"), and the videos exist only on the user's PC. The cloud session (2026-10-23) changed no values because it had no numbers to work from. Rerun on this branch with NFSPU_GAME_DIR=/opt/nfs5: `cargo test --workspace` 364 passed, 0 failed; handling_check flat runs: 8 spins, 9 slides in 30 runs (tracks: 39 spins); `race_drive` ALL OK (P1, 02:08.35).
- **Next step (local, needs the video):** from `C:\Users\CB\Videos\2026-10-23 11-00-02.mp4` measure GT1 roll/pitch (chase from ~2:20), cockpit hand pixels and forearm angle, and per-view car width in frame plus steer-to-follow lag. Then change `chase_cam.rs` and the `main.rs` arm skinning. Note: hands come from the model's own LeftHand/RightHand meshes (main.rs ~1235), so retail placement may already match.
