#!/usr/bin/env bash
# Fails when a release is missing any artifact a complete two-architecture
# macOS release must carry.
#
# release.yml builds aarch64 and x86_64 in two matrix legs, and both legs
# upload to the same release with tauri-action. The release is created as a
# draft and is published only from a final job that runs this script first, so
# a leg that failed (or any step that failed after an upload) can never leave a
# public release that only carries one architecture. This asserts, from the
# release's own uploaded assets, that both legs landed: four DMGs (versioned and fixed-name), two updater
# archives with their signatures, the two dSYM archives, the two signed CLI
# archives that the npm-publish job repackages, and a latest.json
# that names both darwin-aarch64 and darwin-x86_64. A single-target
# latest.json is exactly the dead end the updater hits when the other
# architecture is missing.
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <tag>" >&2
  exit 1
fi

tag="$1"

assets="$(gh release view "$tag" --json assets --jq '.assets[].name')"

count() { printf '%s\n' "$assets" | grep -cE "$1" || true; }

problems=()

# Each architecture ships its versioned DMG and a fixed-name copy that the
# website links to.
if [[ "$(count '\.dmg$')" -ne 4 ]]; then
  problems+=("expected 4 .dmg assets (versioned and fixed-name, per architecture), found $(count '\.dmg$')")
fi
for dmg in Skill-Studio-arm64.dmg Skill-Studio-intel.dmg; do
  if ! printf '%s\n' "$assets" | grep -qx "$dmg"; then
    problems+=("missing fixed-name DMG $dmg")
  fi
done
if [[ "$(count '\.app\.tar\.gz$')" -ne 2 ]]; then
  problems+=("expected 2 .app.tar.gz updater archives, found $(count '\.app\.tar\.gz$')")
fi
if [[ "$(count '\.app\.tar\.gz\.sig$')" -ne 2 ]]; then
  problems+=("expected 2 .app.tar.gz.sig signatures, found $(count '\.app\.tar\.gz\.sig$')")
fi
if [[ "$(count '\.dSYM\.zip$')" -ne 2 ]]; then
  problems+=("expected 2 .dSYM.zip debug-symbol archives, found $(count '\.dSYM\.zip$')")
fi
for target in aarch64-apple-darwin x86_64-apple-darwin; do
  if ! printf '%s\n' "$assets" | grep -qx "skill-studio-cli-$target\.tar\.gz"; then
    problems+=("missing CLI archive skill-studio-cli-$target.tar.gz")
  fi
done

if printf '%s\n' "$assets" | grep -qx 'latest.json'; then
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  gh release download "$tag" --pattern 'latest.json' --dir "$tmp" --clobber
  # tauri-action 1.x writes each asset's API URL (.../releases/assets/<id>),
  # which stays valid when the draft is published; the updater sends
  # Accept: application/octet-stream, so GitHub serves the file. A tag
  # download URL also works. A draft's untagged-... download URL does not.
  updater_urls="$(gh release view "$tag" --json assets | jq -r --arg t "/releases/download/$tag/" \
    '.assets[] | select(.name | endswith(".app.tar.gz")) | .apiUrl, (.url | select(contains($t)))')"
  first_url=""
  for platform in darwin-aarch64 darwin-x86_64; do
    if ! jq -e --arg p "$platform" '.platforms[$p].url and .platforms[$p].signature' "$tmp/latest.json" >/dev/null; then
      problems+=("latest.json has no url+signature entry for $platform")
      continue
    fi
    url="$(jq -r --arg p "$platform" '.platforms[$p].url' "$tmp/latest.json")"
    if ! printf '%s\n' "$updater_urls" | grep -qxF "$url"; then
      problems+=("latest.json url for $platform is not an updater archive of release $tag: $url")
    elif [[ "$url" == "$first_url" ]]; then
      problems+=("latest.json names the same archive for darwin-aarch64 and $platform")
    fi
    first_url="$url"
  done
else
  problems+=("missing latest.json updater manifest")
fi

if [[ ${#problems[@]} -ne 0 ]]; then
  echo "Release $tag is incomplete; refusing to publish it." >&2
  printf '  - %s\n' "${problems[@]}" >&2
  echo "Uploaded assets:" >&2
  printf '  %s\n' "$assets" >&2
  exit 1
fi

echo "Release $tag is complete: 4 DMGs, 2 updater archives with signatures, 2 dSYM archives, 2 CLI archives, and a latest.json covering darwin-aarch64 and darwin-x86_64."
