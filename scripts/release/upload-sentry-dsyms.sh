#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${SENTRY_AUTH_TOKEN:-}" ]]; then
  echo "::warning::SENTRY_AUTH_TOKEN is missing. Skipping native Sentry debug file upload."
  exit 0
fi

symbols=()
debug_ids=()
for binary in "$@"; do
  dsym="$binary.dSYM"
  binary_id=$(dwarfdump --uuid "$binary" | awk '{print $2}')
  dsym_id=$(dwarfdump --uuid "$dsym" | awk '{print $2}')
  if [[ -z "$binary_id" || "$binary_id" != "$dsym_id" ]]; then
    echo "::error::Debug UUID does not match the release binary: $binary"
    exit 1
  fi
  symbols+=("$dsym")
  debug_ids+=(--id "$binary_id")
done

# The release job also has Apple signing credentials. Give the uploader only its own token.
# The local UUID check above also covers retries when Sentry already has the files.
env -i HOME="$HOME" PATH="$PATH" SENTRY_AUTH_TOKEN="$SENTRY_AUTH_TOKEN" \
  pnpm exec sentry-cli debug-files upload --org sergtech \
  --project skill-studio-desktop-rust --type dsym --wait-for 120 \
  "${debug_ids[@]}" "${symbols[@]}"
