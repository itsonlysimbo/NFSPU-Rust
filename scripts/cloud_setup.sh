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
