#!/usr/bin/env bash
# Runs check-release-complete.sh against a fake `gh` for each latest.json
# shape a release can carry. A wrong result means a good release stays a
# draft (npm publish never runs) or a release whose updater points at a dead
# or foreign download goes public.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/bin"

tag="v9.9.9"
repo="https://github.com/o/r"
api="https://api.github.com/repos/o/r/releases/assets"

asset() { printf '{"name":"%s","apiUrl":"%s/%s","url":"%s/releases/download/%s/%s"}' "$1" "$api" "$2" "$repo" "$tag" "$1"; }
names=(
  "Skill.Studio_9.9.9_aarch64.dmg:1" "Skill.Studio_9.9.9_x64.dmg:2" "Skill-Studio-arm64.dmg:3" "Skill-Studio-intel.dmg:4"
  "Skill.Studio_9.9.9_aarch64.app.tar.gz:11" "Skill.Studio_9.9.9_x64.app.tar.gz:12"
  "Skill.Studio_9.9.9_aarch64.app.tar.gz.sig:13" "Skill.Studio_9.9.9_x64.app.tar.gz.sig:14"
  "skill-studio-aarch64.dSYM.zip:15" "skill-studio-x86_64.dSYM.zip:16"
  "skill-studio-cli-aarch64-apple-darwin.tar.gz:17" "skill-studio-cli-x86_64-apple-darwin.tar.gz:18"
  "latest.json:19"
)
json='{"assets":['
for entry in "${names[@]}"; do json+="$(asset "${entry%%:*}" "${entry##*:}"),"; done
printf '%s' "${json%,}]}" > "$work/assets.json"

cat > "$work/bin/gh" <<GH
#!/usr/bin/env bash
set -euo pipefail
case "\$1 \$2" in
  "release view")
    if [[ " \$* " == *" --jq "* ]]; then jq -r '.assets[].name' "$work/assets.json"; else cat "$work/assets.json"; fi ;;
  "release download")
    dir=""; while [[ \$# -gt 0 ]]; do [[ "\$1" == --dir ]] && dir="\$2"; shift; done
    cp "$work/latest.json" "\$dir/latest.json" ;;
  *) echo "fake gh: unexpected \$*" >&2; exit 2 ;;
esac
GH
chmod +x "$work/bin/gh"

manifest() {
  printf '{"platforms":{"darwin-aarch64":{"url":"%s","signature":"s"},"darwin-x86_64":{"url":"%s","signature":"s"}}}' "$1" "$2" > "$work/latest.json"
}

failures=0
expect() {
  local want="$1" label="$2" got=0
  PATH="$work/bin:$PATH" "$here/check-release-complete.sh" "$tag" >"$work/out" 2>&1 || got=1
  if [[ "$want" == pass && "$got" -ne 0 ]] || [[ "$want" == fail && "$got" -eq 0 ]]; then
    echo "FAIL: $label (expected $want)"; sed 's/^/    /' "$work/out"; failures=$((failures + 1))
  else
    echo "ok: $label"
  fi
}

manifest "$api/11" "$api/12"
expect pass "tauri-action 1.x API asset URLs of this release's updater archives publish"
manifest "$repo/releases/download/$tag/Skill.Studio_9.9.9_aarch64.app.tar.gz" "$repo/releases/download/$tag/Skill.Studio_9.9.9_x64.app.tar.gz"
expect pass "tag download URLs of this release's updater archives publish"
manifest "$repo/releases/download/untagged-abc/Skill.Studio_9.9.9_aarch64.app.tar.gz" "$api/12"
expect fail "a draft's untagged download URL keeps the release a draft"
manifest "$api/999" "$api/12"
expect fail "an asset id from another release keeps the release a draft"
manifest "$api/13" "$api/12"
expect fail "a signature asset in place of the archive keeps the release a draft"
manifest "$api/11" "$api/11"
expect fail "one archive named for both architectures keeps the release a draft"

[[ "$failures" -eq 0 ]]
