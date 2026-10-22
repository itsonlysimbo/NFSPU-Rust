# Feature checklist

Hand-kept estimates of how much of each retail feature is recreated. `cargo run -p nfspu-game --example progress` reads this table for the "Features" panel of `logs/progress/progress.svg`. Update a row's percent when work lands; keep the five columns.

| Area | Feature | Status | Percent | Notes |
|---|---|---|---|---|
| Front end | Main menus, layouts, fonts, art, intro movie | Done | 95 |  |
| Front end | Menu transitions, hover animations, dropdowns | Done | 90 | Playtest |
| Front end | 3D garage and car pick (model, body, paint) | Done | 85 | Roof up/down not drawn |
| Front end | Options menu (audio, graphics, controls) | In progress | 15 | Session running |
| Front end | Location screen 3D track room | Not started | 0 |  |
| Front end | Retail race results screen | Not started | 5 | Placeholder only |
| Front end | Loading screen | Done | 95 |  |
| Race | Quick race: grid, countdown, laps, finish | Done | 90 |  |
| Race | Mirrored and reversed tracks | Done | 90 |  |
| Race | Opponents: racing lines, retail drivers | Partial | 70 | No route choice at junctions |
| Race | Traffic | Not started | 0 |  |
| Race | Knockout / tournament modes | Not started | 0 |  |
| Race | Pursuit (police) mode | Not started | 0 |  |
| Physics | Engine, gearbox, tyres, ABS/TC from .sim data | Partial | 70 | Handling feel vs retail |
| Physics | Gravity, landings, body lean, wet grip | Done | 80 | Playtest |
| Physics | Walls and car-vs-car collisions | Partial | 60 |  |
| Physics | Suspension, roll-overs | Not started | 5 |  |
| Physics | Damage (visual and mechanical) | Not started | 0 |  |
| Graphics | Tracks: geometry, textures, objects | Done | 90 |  |
| Graphics | Cars: textures, smooth shading, cabin, shadows | Done | 85 | Roof drawing |
| Graphics | Cameras: chase and in-car | Done | 85 |  |
| Graphics | Sky, sun, lens flare, glows | Done | 85 | Flare occlusion untested on Cote d'Azur |
| Graphics | Weather, night, fog, rain/snow, headlights | Done | 80 | Feel vs retail |
| HUD | Retail HUD, minimap, positions | Done | 90 |  |
| Sound | Menu music | Done | 95 |  |
| Sound | In-race sound: engine, announcer, skids, ambience | In progress | 5 | Session running |
| Career | Evolution career (eras, money, upgrades) | Not started | 0 |  |
| Career | Factory Driver mode | Not started | 0 |  |
| Career | Used cars, dealership, parts, paint shop | Not started | 0 | Screens exist as layouts only |
| Career | Save profiles (load/save .sav) | Partial | 20 | Reads settings only |
| Online | Multiplayer (new netcode, no EA services) | Not started | 0 | Long-term; ask first |
