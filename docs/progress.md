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
| Race | Knockout / tournament modes | Not started | 0 |  |
| Race | Pursuit (police) mode | Not started | 0 |  |
| Physics | Engine, gearbox, tyres, ABS/TC from .sim data | Partial | 70 | Handling feel vs retail |
| Physics | Gravity, landings, body lean, wet grip | Done | 80 | Playtest |
| Physics | Walls and car-vs-car collisions, stuck recovery | Partial | 75 | Collision audit of all tracks; stuck/fell/out-of-bounds reset to the road; a few edge slivers left; not playtested |
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
| Career | Evolution career (eras, money, upgrades) | Partial | 50 | Slices 1-3 merged: profile, money, events, payout, buy/sell, tuning, test drive, era change; screens matched to retail footage |
| Career | Factory Driver mode | Not started | 0 |  |
| Career | Used cars, dealership, parts, paint shop | Partial | 65 | Dealership, used lot, parts, tuning, paint shop and test drive merged; showcase and car compare data open |
| Career | Save profiles (load/save .sav) | Partial | 35 | Career profile saved in our own format; retail .sav read for settings |
| Online | Multiplayer (new netcode, no EA services) | Not started | 0 | Long-term; ask first |
