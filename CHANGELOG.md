# Changelog

All notable changes to Skill Studio are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## Unreleased

## v0.2.0

### Added

- Usage reads Codex `.jsonl.zst` logs in `sessions/` and `archived_sessions/`, counts sessions, shows how each skill was triggered (asked for, loaded by the agent, or read as a file), and filters by `--harness` and `--project`. A partial report names each root it did not read and why.
- `invocation` checks and sets manual-only ("only when I ask") for every harness, in the CLI and MCP: `disable-model-invocation` for Claude, pi and Cursor, and `allow_implicit_invocation` in `agents/openai.yaml` for Codex.
- `doctor` reports how much of each model's skill listing budget your skills use, and skips skills that a native switch turns off.
- Park and unpark several skills in one CLI or MCP call. Unpark checks every link destination before it moves any copy.
- `extras` lists hook events, MCP server names and instruction file paths, without their values.
- `review` sets, lists and clears skill review dates, and lists the skills that are due. Stored in `~/.agents/review-dates.json`.
- Scan honors `CLAUDE_CONFIG_DIR`, `PI_CODING_AGENT_DIR` and `GROK_HOME`.
- Activity can undo updates to split copies. The app warns about edited copies before an update.
- Claude Code plugin updates, fork-ahead notices, skill authors and install counts.
- Turn a shared skill off for one agent (confirm, split, park). Park works for any copy and is the only off switch.

### Changed

- `outdated` checks sources in a bounded pool of 4 and asks once per repo: about 2x faster on a 378-skill home.
- Scan reuses digests of identical folders, and writes scan only the skills they touch.
- YAML frontmatter uses `serde-saphyr` instead of the unmaintained `serde_yaml`.
- The telemetry switch in `~/.agents/skill-studio.json` is now `telemetry_enabled`; a file saved by an rc build under `error_reporting_enabled` is still read.
- Desktop: Enter submits Add skill, the first frame uses the saved theme, dialogs dim the window correctly, and tags and destructive buttons meet contrast targets.

### Fixed

- The skill list shows a loading state and the failure reason instead of an empty list, with Try again.
- Skill Store search ignores stale answers and keeps the search box on screen after an error.
- Settings shows why the version check or project folders failed to load.
- The welcome screen stays open and shows the reason when saving the choice fails.
- Interrupted writes are recovered before new commands; Undo restores every backup; history pages stay fast.
- Network requests to skills.sh, GitHub and the proxy time out instead of hanging.
- `usage` counts sessions and triggers only in the `--days` window; all-time uses stay in `total_uses`.
- `invocation` reports a mismatch only when one harness reads a skill as auto and another as manual; one copy or `unsupported` copies never mismatch.

### Removed

- The disabled assistant, unregistered skill packs, the unreleased TUI, unused helpers and tests that could not fail.

### Server

- skills.sh failures, 5xx or non-JSON answers, and unhandled route errors in the hosted proxy are reported to Sentry when a `SENTRY_DSN` secret is set; the user block, request headers, cookies, query strings, request body, breadcrumbs, and the timezone context are stripped first, and tracing stays off.

## v0.1.0

### Added

- Signed and notarized macOS release workflow, so the app installs without a
  Gatekeeper warning.
- Shared `skill-studio-core` crate: scan, ops, events, and DTOs used by the
  desktop app, the CLI, and the MCP server.
- Crash report scaffolding: a panic hook that sends only the code location
  where Skill Studio crashed. The welcome screen offers it on; Settings'
  "Telemetry" switch lets the user change that choice. Off in the registry
  until a choice is saved.

### Removed

- Eleven orphan IPC commands with no frontend caller.
