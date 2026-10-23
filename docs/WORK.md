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
| Race sound effects polish | Mid | Claude (cloud) | `claude/cloud-sound-channels` | 2026-10-23: rain, wet road, opponent skids/hits, Doppler, sideways balance swing, commentator lap/finish lines added (rules guessed where noted in sound.md); `RACE_DRIVE_SOUND=1` logs them to `logs/race_sound.log`; tests pass, race_drive ALL OK | Next: coordinator merge; check the guessed rules against the decomp (rain level, Doppler, swing values 5/6) and by ear (PLAYTEST). Not done: `c` table set, other surfaces, commentator position/record lines | Sounds match retail |
| In-race pause menu (Esc) | Mid | Claude (pause-menu session) | `claude/pause-menu-22e199` | Done on branch: retail `HPause` panel (continue / race again / quit… with exit confirmation), clock/AI/car/sound freeze, race again, quit; `NFSPU_PAUSE_TEST` checks pass, tests pass, race_drive ALL OK | Next: coordinator merge; playtest (PLAYTEST.md). Not done: orbiting pause camera, hide-menu key | Esc pauses with the retail menu; resume/restart/quit work; race_drive ALL OK |
| Retail traffic in races | Strong | Claude (traffic session) | `claude/traffic-4f929a` | done: race_drive ALL OK 15 tracks traffic 0 and 3, tests pass, game bin checks (not seen in a window) | Next: coordinator merge; playtest (PLAYTEST.md); then decode lane data and the speed ladder | Races with traffic on show retail-like traffic; race_drive ALL OK with traffic on and off |
| Opponents and autopilot avoid traffic | Strong | Claude (cloud session) | `claude/cloud-traffic-avoid` | done on branch: shared `ai::avoid` (pass slow cars on the left, keep right for oncoming, brake when boxed in); traffic keeps off the centre line and edges to its kerb for an oncoming racer; race_drive ALL OK 15 tracks traffic 0 and 3; traffic hits 182 -> 7 (seed 12345), 187 -> 11 (seed 777); tests pass. Cloud: game files loaded on Linux with no case problems, so no lookup change was needed | Next: coordinator merge; playtest (PLAYTEST.md). Retail's AI avoidance was not read (Ghidra was not reachable from the cloud), so the rules are guesses | Opponents pass and brake for traffic; race_drive ALL OK with traffic on and off |

## Up next (unclaimed)
Pick from here, highest first. See `docs/progress.md` for the full feature list.

| Task | Tier | Notes |
|---|---|---|
| Cloud: case-insensitive game file lookup on Linux | Mid | Cloud OK. First cloud session: run tests and race_drive with `NFSPU_GAME_DIR=/opt/nfs5` and fix missing files (`docs/CLOUD.md`) |
| Last slab-allocator "Use-after-free" error (one per race) | Mid | Likely fix on master (sky meshes start non-empty); not reproduced here, needs playtest |
| Audio sliders other than music have no effect | Mid | Menu sounds now follow the effects slider; in-race levels were already wired (031dcef) but not confirmed by ear |
| Côte d'Azur sea water not visible | Mid | Fixed on master (510f236): translucent water now blends; needs playtest |
| Cockpit hands on the wheel | Mid | Claude `cockpit-hands` session (wheel now turns smoothly, 510f236) |
| Car reflections: lamp glow and 'covered' glows | Mid | Sun highlight merged (845babe); lamp glow not seen by eye; trace the 'covered' flag (Monaco 3-5) and the sun direction |
| Côte d'Azur sea water not visible | Mid | **Claude `claude/water-cockpit-213623`, claimed 2026-10-22.** OpenCode's water planes (3efc9be) draw nothing in the user's playtest |
| Cockpit: hand position on the wheel and hand animation | Mid | **Claude `claude/cockpit-hands-afa0a8`, claimed 2026-10-22.** Wheel smoothing done (merged) |
| Car reflections (paint and chrome environment map) | Strong | User's next ask for cars |
| Menu button outline light that travels around the border | Mid | Needs retail footage (user can supply) |
| Race sound fine-tuning against retail | Strong | Needs a retail recording with game sound |
| Body lean closer to retail | Strong | **Claude `body-lean`, branch `claude/body-lean-2c2196`. Next step: `lean_check` shows the sim already hits the decoded numbers (roll 7.6 deg from 0.8 g up, pitch 3-5 deg braking); only `LEAN_LOOK` 0.6 (car.rs) and the guessed corner spring differ. Needs retail footage to pick a scale; no lean code changed, tests/race_drive not run** |
| Playtest round: 2026-10-20 fixes, Options > Audio, race sound | User | Flare on Côte d'Azur, slider dragging and sound levels by ear are not tested |
| Traffic | Strong | Needs decomp work (AIHigh_Traffic) |
| Roof up/down drawing | Mid | **Claude `roof-drawing`, branch `claude/exciting-heyrovsky-1fef36`, claimed 2026-10-23.** Code done (piece 41 type +1 for down); tests and race_drive ALL OK; garage live not tested (needs playtest) |
| Roof up/down drawing | Mid | Choice already reaches the race |
| Location screen 3D track room | Mid | |
| Roof up/down drawing | Mid | Choice already reaches the race |
| Location screen 3D track room | Mid | **Done on `claude/agitated-williamson-1794af` (ready to merge).** Room, walls, pictures, fly-in, panel and room turns built; 336+ tests and `race_drive` ALL OK. Next: playtest vs retail (see `docs/PLAYTEST.md`); header overlay (flags) not drawn |
| Evolution career, Factory Driver | Strong | Big; split into pieces first |
| Keep `docs/progress.md` current and rerun the progress tool | Light | After each merge |

## Done (recent)

| Task | Merged as |
|---|---|
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
