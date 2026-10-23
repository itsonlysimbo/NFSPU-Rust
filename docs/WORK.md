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
| Car lamp glow + 'covered'; location header flags | Mid | Claude (glow-flags) | claude/glow-flags-e70b14 | done, awaiting merge | Glow on screen not seen (car starts on a slice with no glow section; needs a slice-jump hook). Flag overlay only unit-tested (2D map box hides the wall face). | merge |

## Up next (unclaimed)
Pick from here, highest first. See `docs/progress.md` for the full feature list.

| Task | Tier | Notes |
|---|---|---|
| Career 2: new-career intro flow (`careerfirst`, `firstlicense`, missions) and `endofera` | Mid | Findings in `docs/provenance/career.md`; the first slice goes straight to `tournament` |
| Career 3: garage, dealership, used cars (buy and sell; prices from `nfs5.car`) | Strong | Needs the account (`career.rs` cash) and `OwnedCars`/`DealerCar` accessors |
| Career 4: parts, car setup, paint shop, repairs (upgrades) | Strong | After career 3 |
| Career 5: car-rule matching, tournament standings and points, prize cars, real payout timing | Strong | Rules are guesses in `career.rs`; check against the decomp / footage |
| Career 6: club events (Factory Driver, eras 524-526) | Mid | `nfs5.trn` records 20-34 |
| Career 7: read a retail `.sav` into a career (owned cars, year) | Light | Owned cars already readable in `quick_race.rs` |
| Car reflections: lamp glow and 'covered' glows | Mid | Sun highlight merged (845babe); lamp glow not seen by eye; trace the 'covered' flag (Monaco 3-5) and the sun direction |
| Location screen header overlay (flags) | Mid | Track room merged (673c0b3); the header overlay isn't drawn yet |
| Keep `docs/progress.md` current and rerun the progress tool | Light | After each merge |
| Menu button outline light that travels around the border | Mid | Waiting for retail footage from the user |
| Body lean scale (`LEAN_LOOK`) | Mid | Sim already matches the decoded numbers; pick the scale from retail footage (user) |
| Race sound fine-tuning against retail | Strong | Engine+road+wind level and pitch matched to GT1 footage (MIX 0.7 to 0.6; branch claude/footage-sound-a19d43). Next: skids, hits, opponents, ambience and countdown timing have no footage markers yet; a clip with a hard slide or wall hit would settle them |
| Cockpit hand placement | Mid | Arms follow the wheel (45d5bec); retail placement not found in the decomp; compare with footage |
| Playtest round (see `docs/PLAYTEST.md`) | User | Many merged items wait for eyes and ears |

## Done (recent)

| Task | Merged as |
|---|---|
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
