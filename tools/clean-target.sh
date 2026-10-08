#!/bin/sh
# Deletes build outputs not touched in the last N days (default 2) from the
# main checkout's target/, which every worktree shares. Cargo rebuilds
# anything it still needs.
set -eu
days="${1:-2}"
target="$(dirname "$(git rev-parse --path-format=absolute --git-common-dir)")/target"
[ -d "$target" ] || exit 0
du -sh "$target"
for profile in debug release; do
  find "$target/$profile/deps" -mindepth 1 -maxdepth 1 -mtime +"$days" -exec rm -rf {} + 2>/dev/null || true
  find "$target/$profile/incremental" -mindepth 1 -maxdepth 1 -mtime +"$days" -exec rm -rf {} + 2>/dev/null || true
done
du -sh "$target"
