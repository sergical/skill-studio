#!/bin/sh
# Keeps Cargo build folders under a size cap and a free-disk floor.
#
# Expects `build.build-dir = "{cargo-cache-home}/build/{workspace-path-hash}"`,
# which gives each workspace (each git worktree) one folder:
#   $CARGO_HOME/build/<xx>/<hash>/
# Deletes whole folders, least recently built first. Deleting one only costs a
# rebuild. Never runs while cargo or rustc runs, so no build loses files.
#
#   CARGO_BUILD_GC_CAP_GB    total size cap (default 40)
#   CARGO_BUILD_GC_FLOOR_GB  free-disk floor (default 30)
#   CARGO_BUILD_GC_MAX_DAYS  delete folders not built for this long (default 3)
#   CARGO_BUILD_GC_DRY_RUN=1 print what would go, delete nothing
set -eu

root="${CARGO_HOME:-$HOME/.cargo}/build"
cap_kb=$((${CARGO_BUILD_GC_CAP_GB:-40} * 1048576))
floor_kb=$((${CARGO_BUILD_GC_FLOOR_GB:-30} * 1048576))
max_age_s=$((${CARGO_BUILD_GC_MAX_DAYS:-3} * 86400))
dry_run="${CARGO_BUILD_GC_DRY_RUN:-0}"
[ -d "$root" ] || exit 0

if pgrep -x cargo >/dev/null || pgrep -x rustc >/dev/null; then
  echo "$(date '+%F %T') skip: a build is running"
  exit 0
fi

mtime() { stat -f %m "$1" 2>/dev/null || stat -c %Y "$1"; }
free_kb() { df -k "$root" | awk 'NR == 2 { print $4 }'; }

# A build writes into deps/ and .fingerprint/, so their newest mtime is the
# last build time of the workspace.
last_built() {
  newest=0
  for p in "$1" "$1"/*/deps "$1"/*/.fingerprint; do
    [ -e "$p" ] || continue
    t="$(mtime "$p")"
    [ "$t" -gt "$newest" ] && newest="$t"
  done
  echo "$newest"
}

remove() {
  echo "$(date '+%F %T') delete $1 ($2, $(($3 / 1024)) MB)"
  [ "$dry_run" = 1 ] && return
  rm -rf "$1"
  rmdir "$(dirname "$1")" 2>/dev/null || true
}

now="$(date +%s)"
list="$(mktemp)"
trap 'rm -f "$list"' EXIT
for ws in "$root"/*/*/; do
  [ -d "$ws" ] || continue
  ws="${ws%/}"
  printf '%s %s %s\n' "$(last_built "$ws")" "$(du -sk "$ws" | cut -f1)" "$ws" >>"$list"
done

total=0
while read -r _ kb _; do total=$((total + kb)); done <"$list"

sort -n "$list" | while read -r built kb ws; do
  age=$((now - built))
  if [ "$age" -gt "$max_age_s" ]; then
    remove "$ws" "not built for $((age / 86400)) days" "$kb"
  elif [ "$total" -gt "$cap_kb" ]; then
    remove "$ws" "over cap" "$kb"
  elif [ "$(free_kb)" -lt "$floor_kb" ] && [ "$age" -gt 3600 ]; then
    remove "$ws" "below free-disk floor" "$kb"
  else
    continue
  fi
  total=$((total - kb))
done
