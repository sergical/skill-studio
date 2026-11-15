#!/usr/bin/env bash
# Runs check-versions.mjs and set-version.mjs on a small fake repo. A wrong
# result means a release ships an app, CLI and npm package that report
# different versions, or a good release is refused.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
export VERSIONS_ROOT="$work"

reset() {
  rm -rf "${work:?}"/*
  mkdir -p "$work/crates/a" "$work/apps/desktop/src-tauri" "$work/packages/npm/main" "$work/node_modules/dep"
  printf '[workspace]\nmembers = ["crates/*"]\n\n[workspace.package]\nversion = "1.2.3"\n' > "$work/Cargo.toml"
  printf '[package]\nname = "a"\nversion.workspace = true\n' > "$work/crates/a/Cargo.toml"
  printf '{\n  "name": "root",\n  "version": "1.2.3"\n}\n' > "$work/package.json"
  printf '{\n  "productName": "App",\n  "version": "1.2.3"\n}\n' > "$work/apps/desktop/src-tauri/tauri.conf.json"
  printf '{\n  "name": "main",\n  "version": "1.2.3",\n  "optionalDependencies": {\n    "@skill-studio/cli-darwin-arm64": "1.2.3"\n  }\n}\n' > "$work/packages/npm/main/package.json"
  printf '{ "name": "dep", "version": "9.9.9" }\n' > "$work/node_modules/dep/package.json"
}

failures=0
expect() {
  local want="$1" name="$2"
  shift 2
  local got=0
  "$@" > "$work.out" 2>&1 || got=1
  if [[ "$got" != "$want" ]]; then
    echo "FAIL: $name (exit $got, want $want)"
    cat "$work.out"
    failures=$((failures + 1))
  fi
}
check() { node "$here/check-versions.mjs" "$@"; }

reset
expect 0 "matching versions pass, node_modules is skipped" check
expect 0 "an rc tag matches its base version" check v1.2.3-rc.1
expect 1 "a tag for another version fails" check v1.2.4

reset
sed -i.bak 's/1.2.3/1.2.4/' "$work/apps/desktop/src-tauri/tauri.conf.json"
expect 1 "a drifted app version fails" check

reset
sed -i.bak 's/"@skill-studio\/cli-darwin-arm64": "1.2.3"/"@skill-studio\/cli-darwin-arm64": "1.2.2"/' "$work/packages/npm/main/package.json"
expect 1 "a stale CLI pin fails" check

reset
printf '[package]\nname = "a"\nversion = "1.2.3"\n' > "$work/crates/a/Cargo.toml"
expect 1 "a crate with its own version fails" check

reset
expect 0 "version:set writes every manifest" node "$here/set-version.mjs" 2.0.0
expect 0 "after version:set the new tag matches" check v2.0.0
expect 1 "version:set refuses a pre-release version" node "$here/set-version.mjs" 2.0.0-rc.1

if [[ "$failures" -gt 0 ]]; then
  echo "$failures check-versions case(s) failed"
  exit 1
fi
echo "check-versions: all cases passed"
