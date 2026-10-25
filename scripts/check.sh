#!/usr/bin/env bash
# Full local check, the merge condition: workspace tests, ORACLE PASS, race_drive ALL OK.
# Needs the game install (NFSPU_GAME_DIR). On a failure it posts to the Discord channel.
#   bash scripts/check.sh [label]     label names what is being checked, e.g. the branch
cd "$(dirname "$0")/.."
export PATH="$USERPROFILE/.cargo/bin:$PATH"
export NFSPU_GAME_DIR="${NFSPU_GAME_DIR:-C:\Users\CB\Downloads\NFS5 - Copy}"
label="${1:-$(git branch --show-current)}"
fail() {
    echo "CHECK FAIL: $1"
    python scripts/discord_bridge.py post "CI (local, $label): FAILED at $1 on $(git rev-parse --short HEAD)" || true
    exit 1
}
cargo test --workspace -j 2 2>&1 | tee logs/check_tests.log | grep -E "test result|FAILED|^error" ; [ "${PIPESTATUS[0]}" = 0 ] || fail "cargo test"
cargo run -p nfspu-game --example oracle --release -j 2 2>&1 | tail -1 | grep -q "ORACLE PASS" || fail "oracle"
cargo run -p nfspu-game --example race_drive --release -j 2 2>&1 | tail -1 | grep -q "ALL OK" || fail "race_drive"
echo "CHECK PASS"
