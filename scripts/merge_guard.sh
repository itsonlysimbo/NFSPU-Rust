#!/bin/bash
# Usage: scripts/merge_guard.sh <branch> [--allow-oracle]
# Fails if <branch> changes a protected path (scripts/protected.txt, read from master so a
# branch can't shrink the list). --allow-oracle: the coordinator has reviewed the diff.
set -eu
branch=$1
allow=${2:-}
base=$(git merge-base master "$branch")
changed=$(git diff --name-only "$base" "$branch")
hits=""
while read -r p; do
  case "$p" in ""|\#*) continue;; esac
  for f in $changed; do
    case "$f" in "$p"*) hits="$hits $f";; esac
  done
done < <(git show master:scripts/protected.txt)
if [ -n "$hits" ]; then
  echo "PROTECTED CHANGED:$hits"
  [ "$allow" = "--allow-oracle" ] && { echo "GUARD PASS (reviewed)"; exit 0; }
  echo "GUARD FAIL"; exit 1
fi
echo "GUARD PASS"
