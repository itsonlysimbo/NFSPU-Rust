# Feature checklist

Hand-kept estimates of how much of each retail feature is recreated. `cargo run -p nfspu-game --example progress` reads this table for the "Features" panel of `logs/progress/progress.svg`. Update a row's percent when work lands; keep the five columns.

| Area | Feature | Status | Percent | Notes |
|---|---|---|---|---|
| Front end | Main menus, layouts, fonts, art, intro movie | Done | 95 |  |
| Front end | Menu transitions, hover animations, dropdowns, button outline light | Done | 90 | Outline light from retail footage (6024b73); not playtested |
| Front end | 3D garage and car pick (model, body, paint) | Done | 85 | Roof up/down drawn; not playtested |
| Front end | Options menu (audio, graphics, controls) | In progress | 25 | Audio sliders merged; graphics and controls not checked |
| Front end | Options > Audio sliders | Done | 70 | Saved to `settings/audio.txt`; not playtested |
| Front end | Location screen 3D track room | Partial | 65 | Track room and header flags merged; not playtested |
| Front end | Retail race results screen | Partial | 60 | Summary panel merged (gold panel, bars, times); not playtested |
| Front end | Loading screen | Done | 95 |  |
| Race | Quick race: grid, countdown, laps, finish | Done | 90 |  |
| Race | Mirrored and reversed tracks | Done | 90 |  |
| Race | In-race pause menu (resume, restart, quit) | Done | 80 | Restyled to match retail footage; not playtested |
| Race | Opponents: racing lines, retail drivers | Partial | 75 | Route choice at junctions verified on 7 tracks; not playtested |
| Race | Traffic | Partial | 50 | Retail spawn, cull and lane driving; opponents and autopilot avoid it; not playtested |
| Race | Knockout / tournament modes | Partial | 70 | Knockout and quick knockout merged (rounds, eliminations, run chaining; headless-verified); tournaments run through the career; not playtested |
| Race | Pursuit (police) mode | Partial | 40 | Pursuit window and cops merged and run in the game window; not playtested |
| Physics | Engine, gearbox, tyres, ABS/TC from .sim data | Partial | 72 | GT1 launch measured against footage (no engine change needed); feel still open; retail oracle traces show our stock Boxster launches ~11% slower (cause not found yet) |
| Physics | Gravity, landings, body lean, wet grip | Done | 80 | Playtest |
| Physics | Walls and car-vs-car collisions, stuck recovery | Partial | 75 | Collision audit of all tracks; stuck/fell/out-of-bounds reset to the road; road edges that drop into void now get wall strips (audit open edges ~8000 to 430); not playtested |
| Physics | Suspension, roll-overs | Partial | 60 | Spring suspension and roll-over reset with crash timer merged; not playtested |
| Physics | Damage (visual and mechanical) | Partial | 55 | Damage zones, painted damage stamps and damage option merged; not playtested |
| Graphics | Tracks: geometry, textures, objects | Done | 92 | Côte d'Azur water planes |
| Graphics | Cars: textures, smooth shading, cabin, shadows | Done | 88 | Roof up/down drawn; not playtested |
| Graphics | Car reflections: sun highlight, lamp glow | Partial | 70 | Sun highlight, lamp glow and covered glows merged; lamp glow not seen by eye |
| Graphics | Cockpit arms and steering wheel | Partial | 60 | Arms follow the wheel; retail hand placement not confirmed |
| Graphics | Cameras: chase and in-car | Done | 85 |  |
| Graphics | Sky, sun, lens flare, glows | Done | 85 | Flare occlusion untested on Cote d'Azur |
| Graphics | Weather, night, fog, rain/snow, headlights | Done | 80 | Feel vs retail |
| HUD | Retail HUD, minimap, positions | Done | 90 |  |
| Sound | Menu music | Done | 95 |  |
| Sound | In-race sound: engine, announcer, skids, ambience | Partial | 75 | All channels merged; mix tuned to a retail recording (sound.md); needs an ear |
| Career | Evolution career (eras, money, upgrades) | Partial | 58 | Slices 1-3 plus later eras: car rules enforced at race start, opponents drive the tournament car, era prize notes; not playtested |
| Career | Factory Driver mode | Partial | 35 | Screens and live missions merged (missions 0, 1, 12 with promotion, 16 verified headless); not playtested |
| Career | Used cars, dealership, parts, paint shop | Partial | 67 | Dealership, used lot, parts, tuning, paint shop and test drive merged; showcase texts and car compare bars use real data; car setup sliders and gear bars open; parts sell back at 80% (retail footage); sell-car offers, repair, paint trims and decals still open |
| Career | Save profiles (load/save .sav) | Partial | 50 | Our own career format; a retail .sav starts a career from its year, cash and cars (read-only); tournament progress still undecoded |
| Online | Multiplayer (new netcode, no EA services) | Not started | 0 | Long-term; ask first |
