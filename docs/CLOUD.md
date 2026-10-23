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

## Setup script

The script lives in `scripts/cloud_setup.sh` and runs by itself at the start of every session
through the `SessionStart` hook in `.claude/settings.json`, whenever `GAMEDATA_TOKEN` is set (so
it never runs on the local PC). The environment only needs the two variables; pasting the
script into the environment's setup box as well is optional. A copy for reference:

```bash
#!/bin/bash
# Each step logs; the game download never stops the session from starting.
set -u
SUDO=""; [ "$(id -u)" -ne 0 ] && command -v sudo >/dev/null && SUDO="sudo"
echo "[setup] system libraries"
$SUDO apt-get update -qq || echo "[setup] apt-get update failed"
$SUDO apt-get install -y -qq pkg-config libasound2-dev libudev-dev libx11-dev libxkbcommon-dev   libwayland-dev libxcursor-dev libxrandr-dev libxi-dev unzip curl python3 || echo "[setup] apt-get install failed"
command -v cargo >/dev/null || { curl -sSf https://sh.rustup.rs | sh -s -- -y && . "$HOME/.cargo/env"; } || echo "[setup] rustup failed"
echo "[setup] game data"
DEST=/opt/nfs5
if [ -d "$DEST/GameData" ]; then echo "[setup] game data already present"; exit 0; fi
if [ -z "${GAMEDATA_TOKEN:-}" ]; then echo "[setup] GAMEDATA_TOKEN is not set"; exit 0; fi
REPO=${GAMEDATA_REPO:-itsonlysimbo/nfs5-gamedata}
API=https://api.github.com/repos/$REPO/releases/tags/v1
ASSET=$(curl -sS -H "Authorization: Bearer $GAMEDATA_TOKEN" -H "Accept: application/vnd.github+json" "$API"   | python3 -c 'import sys,json; d=json.load(sys.stdin); a=d.get("assets"); print(a[0]["url"] if a else "ERROR: "+str(d.get("message","no attached file")))')   || { echo "[setup] could not read the release (token or network)"; exit 0; }
case "$ASSET" in ""|ERROR:*) echo "[setup] release v1 of $REPO: ${ASSET:-no attached file}"; exit 0;; esac
curl -sSL --fail -H "Authorization: Bearer $GAMEDATA_TOKEN" -H "Accept: application/octet-stream" "$ASSET" -o /tmp/nfs5.zip   || { echo "[setup] download failed (network allowlist?)"; exit 0; }
$SUDO mkdir -p "$DEST" && $SUDO unzip -q /tmp/nfs5.zip -d "$DEST" && rm -f /tmp/nfs5.zip
# The zip may hold the "NFS5 - Copy" folder: move its contents up.
inner=$(find "$DEST" -maxdepth 2 -type d -name GameData | head -1)
[ -n "$inner" ] && [ "$(dirname "$inner")" != "$DEST" ] && $SUDO sh -c "mv \"$(dirname "$inner")\"/* '$DEST'/"
$SUDO chmod -R a+rX "$DEST"
ls "$DEST" && echo "[setup] game data ready"
```

**Network access:** in the environment's settings, set network access so these hosts are
allowed: `api.github.com`, `github.com`, `objects.githubusercontent.com` and
`release-assets.githubusercontent.com` (the release download is redirected to the last two). If
there is a "Full" option, that works too.

## Repo access (verified 2026-10-23)
Cloud sessions reach only the GitHub repos attached to them, whatever the token or network
settings allow. A session needs **both** `itsonlysimbo/NFSPU-Rust` and
`itsonlysimbo/nfs5-gamedata`:
- Sessions the user starts on claude.ai/code: pick both in the repository picker.
- Sessions the coordinator starts (one repo only): the brief starts with *request access to
  `itsonlysimbo/nfs5-gamedata` with add_repo, then run `bash scripts/cloud_setup.sh`*, and the
  user approves the access prompt once.

Verified: with both repos attached and `GAMEDATA_TOKEN` and `NFSPU_GAME_DIR` set in the
`Default` environment, the start-up hook fills `/opt/nfs5`.

## Reporting back
Cloud sessions can't message the local coordinator. They report through GitHub: push the task
branch (`claude/cloud-*`) after every step, and keep the task's row in `docs/WORK.md` on that
branch starting with `WORKING`, `DONE` or `BLOCKED: <question>`. The local coordinator fetches
every 30 minutes while the PC is awake, merges `DONE` branches, and passes `BLOCKED` questions to
the user.

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
