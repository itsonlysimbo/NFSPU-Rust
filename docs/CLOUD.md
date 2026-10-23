# Running sessions in the cloud

Cloud sessions run on Linux machines that start from a fresh clone of this GitHub repo. The repo
never holds game files (AGENTS.md rule 1), so each cloud machine downloads the user's own install
from private storage at start-up and points `NFSPU_GAME_DIR` at it. Nothing game-related is ever
committed, and when the project is finished the user deletes one zip.

The coordinator stays local: it runs `race_drive` before merges and starts OpenCode on this PC
when Claude usage reaches 90%.

## One-time setup (the user does these; secrets never go through an agent or the repo)

1. **Zip the install.** In File Explorer, right-click `C:\Users\CB\Downloads\NFS5 - Copy` and
   choose *Compress to ZIP*. Name it `nfs5-install.zip` (about 0.5 GB). Leave out `SaveData` if
   you don't want your profiles uploaded; the game code works without them.
2. **Make a separate private repo for it**, not this one: on github.com, *New repository*, name
   `nfs5-gamedata`, *Private*. Open its *Releases*, *Draft a new release*, tag `v1`, attach
   `nfs5-install.zip`, and publish. Release files may be up to 2 GB, so the 100 MB file limit
   doesn't apply.
3. **Make a read-only token.** Go to GitHub *Settings*, *Developer settings*, *Fine-grained
   tokens*, *Generate new token*. Repository access: only `nfs5-gamedata`. Permissions:
   *Contents: Read-only*. Copy the token.
4. **Create the cloud environment.** In claude.ai/code, add or edit the environment for
   `itsonlysimbo/NFSPU-Rust`:
   - Environment variable `GAMEDATA_TOKEN` set to the token from step 3. Store it as a secret,
     never in the repo.
   - Environment variable `NFSPU_GAME_DIR` set to `/opt/nfs5`.
   - Setup script: paste the script below.

## Setup script (paste into the cloud environment)

```bash
set -euo pipefail
# Bevy's Linux build needs these system libraries.
sudo apt-get update -qq
sudo apt-get install -y -qq pkg-config libasound2-dev libudev-dev libx11-dev libxkbcommon-dev \
  libwayland-dev libxcursor-dev libxrandr-dev libxi-dev unzip curl
# Rust (skip if the image already has it).
command -v cargo >/dev/null || { curl -sSf https://sh.rustup.rs | sh -s -- -y; . "$HOME/.cargo/env"; }
# The user's game install, from their private release (read-only token).
if [ ! -d /opt/nfs5/GameData ]; then
  url=$(curl -sSf -H "Authorization: Bearer $GAMEDATA_TOKEN" \
    https://api.github.com/repos/itsonlysimbo/nfs5-gamedata/releases/tags/v1 \
    | grep -o '"url": *"[^"]*/releases/assets/[0-9]*"' | head -1 | sed 's/.*"\(http[^"]*\)"/\1/')
  curl -sSfL -H "Authorization: Bearer $GAMEDATA_TOKEN" -H "Accept: application/octet-stream" "$url" -o /tmp/nfs5.zip
  sudo mkdir -p /opt/nfs5 && sudo unzip -q /tmp/nfs5.zip -d /opt/nfs5 && rm /tmp/nfs5.zip
  # The zip holds the "NFS5 - Copy" folder: move its contents up one level.
  [ -d "/opt/nfs5/NFS5 - Copy" ] && sudo sh -c 'mv "/opt/nfs5/NFS5 - Copy"/* /opt/nfs5/ && rmdir "/opt/nfs5/NFS5 - Copy"'
  sudo chmod -R a+rX /opt/nfs5
fi
```

## Known risks (not tested yet)

- **Letter case.** Windows ignores upper and lower case in file names; Linux doesn't. The code
  opens names like `GameData/Track/alps.crp` that may not match the install's exact case. The
  first cloud session should run the tests and `race_drive` with `NFSPU_GAME_DIR=/opt/nfs5`, and
  if files aren't found, add a case-insensitive file lookup in `nfspu-formats`. That task is on
  the work board.
- **No window or GPU.** Cloud machines are headless. `cargo test` and `race_drive` don't open a
  window, but live game runs and screen captures only work locally.
- **Build time.** A first cloud build compiles Bevy from scratch (tens of minutes).

## Which tasks may run in the cloud

Tasks marked **Cloud OK** on `docs/WORK.md`: anything checked by tests and `race_drive` alone
(physics, AI, formats, traffic logic, docs). Tasks that need a screen capture or the user's eyes
stay local.

## When the project is finished

Delete the `nfs5-gamedata` repo (or its release), the token, and the environment's
`GAMEDATA_TOKEN`. This repo needs no cleanup.

## OpenCode in the cloud

OpenCode is a command-line program, so it can run on any Linux machine with the same setup
script plus its own install and a DeepSeek / OpenCode Go key set as a secret. Two options:
- **GitHub Actions** on this private repo: a workflow that runs `opencode run ...` on a schedule
  or on demand, with the game zip downloaded the same way. This uses your Actions minutes
  (private repos get a monthly free allowance).
- **A small rented Linux server**, running OpenCode continuously.

Neither has been set up or tested. The local handoff (AGENTS.md, "Automatic handoff to OpenCode")
stays the default.
