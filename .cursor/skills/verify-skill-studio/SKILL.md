---
name: verify-skill-studio
description: Use the real Skill Studio app and record evidence before handing back a change.
---

Build the CLI and native test binaries, then run `scripts/verify/run.py` with
`--rust-artifacts` and `--ui-url`. See `scripts/verify/README.md`. A CLI-only
release smoke run does not replace full verification.

Read the intent from `gate start` when present, otherwise from the PR or issue.
Write and run 1 to 3 extra scenarios from that intent. State the user action,
expected result, and one question before each browser check. Use a temporary
HOME and the existing app tools. Do not use the diff as the scenario source.

Inspect transcripts and screenshots. Record what you actually saw, including
failures and blocked checks. Append your scenarios to `.gate/out/verify.json`
using the same fields and evidence kinds. Evidence paths are relative to that
manifest. Never mark an unrun check as passed. Paste the manifest summary into
the PR body. Keep evidence out of Git. Do not merge by hand.
