#!/bin/sh
# Copies the Markdown of four open-source skill packs, each at a pinned
# commit and with its own license, into the core crate's test fixtures.
# The skill links corpus test reads them. To move a pack to a new commit,
# change its line below, run this script, and rerun
# `cargo test --workspace --all-features --test skill_links_corpus`.
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
DEST="$ROOT/crates/skill-studio-core/tests/fixtures/real-packs"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

# repository  commit  skills folder  license file
PACKS="
mattpocock/skills bbadaa631c37af594e5a05c9cefacd9956136b18 skills LICENSE
cursor/plugins b7b93c36be9bcc36a74db438e0fd385d660bc559 pstack/skills pstack/LICENSE
obra/superpowers 22f0b463e7a5e579904dd6211a520321035526f7 skills LICENSE
emilkowalski/skills 64eb3deb9a5fbfa041faf783e0bc3fd635872e9c skills LICENSE
"

rm -rf "$DEST"
mkdir -p "$DEST"
{
  echo "# Real packs"
  echo
  echo "Markdown files of open-source skill packs, copied by \`tools/refresh-real-packs.sh\`."
  echo "Each folder keeps the pack's own license. Do not edit by hand."
  echo
  echo "| Folder | Source | Commit | License |"
  echo "| --- | --- | --- | --- |"
} >"$DEST/README.md"

echo "$PACKS" | while read -r repo sha skills license; do
  [ -n "$repo" ] || continue
  name=$(echo "$repo" | tr '/' '_')
  if [ "$skills" != "skills" ]; then
    name="${name}_$(dirname "$skills")"
  fi
  clone="$WORK/$name"
  git init -q "$clone"
  git -C "$clone" fetch -q --depth 1 "https://github.com/$repo.git" "$sha"
  git -C "$clone" checkout -q FETCH_HEAD
  out="$DEST/$name"
  mkdir -p "$out"
  cp "$clone/$license" "$out/LICENSE"
  (cd "$clone/$skills" && find . -type f -name '*.md' -print) | while read -r f; do
    mkdir -p "$out/skills/$(dirname "$f")"
    cp "$clone/$skills/$f" "$out/skills/$f"
  done
  holder=$(grep -m1 -i 'copyright' "$out/LICENSE" | sed 's/^ *//')
  echo "| \`$name\` | [$repo](https://github.com/$repo/tree/$sha/$skills) | \`$(echo "$sha" | cut -c1-7)\` | MIT, $holder |" >>"$DEST/README.md"
done
