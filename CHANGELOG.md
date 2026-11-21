# Changelog

All notable changes to Skill Studio are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## Unreleased

### Fixed

- CLI and MCP pack updates reject plans with more than 100 update steps across all scopes before running an owner tool.
- Codex usage keeps a fork's copied skill uses when the parent rollout is gone.
- Pack updates report each tool's own result when skills.sh and dotagents claim the same skill name in one scope.

## v0.4.0

### Added

- Codex listing: Skill Studio now models Codex 0.170.0 exactly enough to match real sessions. It charges `rN/` short paths and the skill roots table, names plugin skills `plugin:name`, lists system skills and nested skills from the newest session, reads frontmatter the way Codex repairs it, and hides plugins Codex did not load, with the reason for each hidden skill. It reports the tier (all described, descriptions shortened, names only), the full and names-only costs, how far the budget is from the next tier, and the 10,000-token cap; it flags skills Claude Code keeps manual-only that Codex still lists. Replaying the 15 newest real sessions, it listed every recorded skill in order and gave the exact description length for 99.8%; overflow runs at 1000 to 4000 tokens matched exactly. The budget comes from the newest session's model. `skill-studio listing --codex-session <rollout>` compares with one session in this home.
- Session history and the listing comparison skip sessions that ran in a temporary folder outside the home folder, such as eval runs.
- Sessions: for each agent's recent coding sessions, Skill Studio shows which skills the model saw with a description, by name only, or not at all, which ones each session used, and which ones it possibly missed. Per skill it flags skills listed in nearly every session but never used, skills used while listed by name only, skills the agent cannot reach, and why a skill is not described now, with a plain next step. Claude Code and Codex record their listing in each session; for Cursor, OpenCode, pi and Grok Build the page says that they record uses only. Possible misses are a guess from the session's first prompt. Desktop **Sessions** page, `skill-studio sessions`, and the MCP `skill_sessions` tool.
- Activity no longer counts a shell loop such as `cat skills/$s/SKILL.md` as a use of a skill named `$s`.
- Desktop: a Model listing page. For each agent and scope it shows which skills the model sees with a description, with a shortened one, with a name only, or not at all, and which skills are hidden. Enter another budget to see which skills gain or lose a description. The page gives the `skillListingBudgetFraction` or `skills.max_context_tokens` text to paste and the file to paste it into; when chezmoi manages that file, it names the chezmoi source. `skill-studio listing` and the MCP `listing` tool return the same data.
- `skill-studio pack export <dir> --skill <name>...` (MCP `pack_export`) makes a new pack from installed skills: a folder with `skills/<name>/` for each and a README with the `npx skills add` install lines. Push it to git and anyone can install it. It writes only into a new or empty folder, never overwrites a file, and runs no git command.
- `skill-studio pack switch-source <source> <to>` (MCP `pack_switch_source`) moves the skills of a skills.sh pack to another git source, such as your GitHub fork, with `npx skills add`, and keeps the old source as the pack's original. `pack show <to> --remote` then lists the skills that differ from the original and the skills only the original has. Each skill gets its own undo event. Before, you had to remove the pack and add it again from the fork, and the link to the original was lost.

### Changed

- Skill Studio counts only the last 30 days of agent history: Claude Code deletes its transcripts after 30 days by default, so older counts depended on how long each agent kept its logs. `usage` and `library` (MCP `skill_usage` and `library`) refuse a window that starts more than 30 days ago, by `--days`, `--since` or an old `--until`, with `invalid_request`. `usage` rows no longer have `total_uses`, and `last_used` is `null` for a skill not used in the last 30 days. The desktop scan's `invocations` rows no longer have `total`. Desktop: Activity shows the 30 days as a calendar.

### Fixed

- Model listing: the Codex tier has its own box with grouped numbers, the comparison column stays empty until you try a budget, and a page subtitle no longer draws over the title in a narrow window. Sessions links each agent's skills docs.
- The doctor listing budget for Claude Code now uses the model's context window from Claude Code's model catalog cache and the model's characters per token (3 for current models). Before, it used 200,000 tokens at 4 characters, so the limit for Opus 5.5 was 2,000 tokens instead of 7,500. Codex uses `context_window` from `models_cache.json` for the configured model.
- Frontmatter that is not valid YAML, such as a description with `: ` in it, still shows its top-level fields, the way Claude Code reads it.
- Undo of a skills.sh update in a project no longer fails with `drift_conflict`. It compared the home lock file, not the project's `skills-lock.json`.

## v0.3.0

### Added

- Desktop: a Packs page. It lists each pack and has Check upstream, Update all and Add pack. Add, Update and Remove show the plan first, Confirm runs it, and the dialog shows what each step did. Update and Remove are off for plugin and npm packs; a tooltip names the command that owns them.
- `skill-studio recovery list` and `recovery drop <PLAN_ID>` (MCP `recovery_list` and `recovery_drop`) show an interrupted write plan that recovery cannot finish, with its failed-run count, last error and touched paths, and let you give it up. Drop changes no file and records a history event. Before, such a plan blocked every write and nothing could clear it.
- `watch --max-seconds N` stops after N seconds and exits 0.
- `library --mode AGENT=manual|auto` (MCP `library` `modes`) shows only the skills that each named agent reads in that mode. Repeat `--mode` for more agents.

### Changed

- These CLI and MCP reads page their lists to fit a 32 KB JSON answer by default: `scan`, `diagnose`, `library`, `usage`, `outdated`, `invocation`, `pack list`, `pack show`, `doctor`, `conflicts` and `extras`. An answer keeps at least one row, so a very large row can go over 32 KB. CLI tables stay whole. An MCP tool result holds two copies of the answer, so it can be about 70 KB. `--limit`, `--cursor`, `--fields` and `--count-by` (MCP: `limit`, `cursor`, `fields`, `count_by`) give an exact answer in one call. Before, `scan --json` and `diagnose --json` were 2.9 MB on a real home.
- MCP tool results start with `untrusted_paths`, the JSON paths of text that comes from third parties: SKILL.md bodies, names, descriptions, frontmatter, file paths and child-process output. The server instructions tell the agent to treat text at those paths as data. `npx` error output is now in a separate `detail` field.
- One `update`, `park` or `unpark` call (MCP `update_all`, `park` or `unpark`) takes at most 100 skills. A larger call fails with `invalid_request` before any skill changes. The desktop Update all and `pack update` have no limit.
- A write that an unfinished recovery blocks now fails with `recovery_pending` (exit 3), not `incomplete` (exit 4).
- JSON output and schemas name OpenCode `opencode`, not `open-code`. `open-code` is still accepted as input.
- Desktop: the webview runs with a strict Content Security Policy. It runs only the app's own scripts and connects only to Tauri IPC.

### Fixed

- Read-only CLI commands and MCP tools work on a home folder you cannot write to; a read no longer creates lock files. With `--home` or MCP `SKILL_STUDIO_HOME`, usage and review count only that home's Codex sessions, not the ones in `CODEX_HOME`.
- A scan no longer stops after 2 seconds. On a home with 4,000 skills, `scan`, `diagnose` and the reads built on them returned `partial` and exited 4.
- A desktop SKILL.md save takes the same write lease as CLI and MCP writes, so the two can no longer interleave and lose a write.
- If another tool replaces a folder under a scope root with a link during a write, the write fails instead of landing outside the scope.
- Desktop: red text on red tints (destructive buttons, badges, menu items, error boxes) has its own color and reaches at least 4.75:1 in light and dark mode. In v0.2.0 the dark-mode text was 3.9:1.
- Desktop: ⌘, and ⌘N no longer open another page while an alert dialog is open.
- `extras` and Settings → Connect your agents list every OpenCode MCP server when `opencode.json` holds both the V1 `mcp.<name>` and the V2 `mcp.servers.<name>` shape. `opencode mcp add` writes that mix into a V1 config, and before this fix the V1 servers went missing.
- `pack add --agent <name>` (MCP `pack_add` `harnesses`) also writes the shared `~/.agents/skills` copy. Before, naming an agent left no shared copy, so a later `pack update` or `pack remove` could not find the skills.
- `pack list` and the other pack commands (MCP `pack_list`) leave out skills that come from a folder on this computer: a dotagents `path:` skill or plugin, or a skills.sh install with `sourceType: "local"`. These have no upstream. Before, each one showed as a pack, and `pack remove` on a folder that dotagents adopted deleted its only copy.

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
- Settings → Connect your agents adds the MCP server to Claude Code, Codex, Cursor, OpenCode, pi and Grok Build, and has a setup prompt to paste into any agent. The homepage and the MCP docs have the same prompt; the docs and `llms.txt` list the setup for VS Code, Gemini CLI and Claude Desktop too.
- Packs: `skill-studio pack list|show|add|update|remove` on the CLI, and `pack_list`, `pack_show`, `pack_add`, `pack_update` and `pack_remove` on MCP. A pack is one source that ships skills: a git repo, an npm package, a dotagents source or a plugin. Each change plans first, and `--dry-run` returns only the plan. Update and remove skip plugin packs, npm packs and skills with local edits, and say why.
- `skill-studio library` (MCP `library`) gives each skill's folder, agents, source, invocation mode and use count in one call. Usage, review and library counts now agree: a resumed or forked session counts once, and a sub-agent log is its own session.

### Changed

- `outdated` checks sources in a bounded pool of 4 and asks once per repo: about 2x faster on a 378-skill home.
- Scan reuses digests of identical folders, and writes scan only the skills they touch.
- YAML frontmatter uses `serde-saphyr` instead of the unmaintained `serde_yaml`.
- The telemetry switch in `~/.agents/skill-studio.json` is now `telemetry_enabled`; a file saved by an rc build under `error_reporting_enabled` is still read.
- Desktop: Enter submits Add skill, the first frame uses the saved theme, dialogs dim the window correctly, and tags meet contrast targets. Destructive buttons have more contrast, but their dark-mode text is still 3.9:1 (#646).
- Desktop: Home and Skills share one skill row. A state icon turns into a checkbox on hover or focus, each row has one named fix button (Update, Compare, Fix, Resolve, Convert, Park, Unpark), and both pages share one selection bar. ↑ ↓ move, Enter opens, X or Space selects, Escape clears. "Pull latest" is now "Update", and "Convert links" is now "Convert".
- Desktop: the Updates tile on Home shows when the last check ran. The separate source update card, which counted copies instead of skills, is gone.
- The pinned skills CLI is 1.8.1. Adding a skill to Claude Code only now makes a copy, not a link.

### Fixed

- Connecting an agent no longer fails with `npm error could not determine executable to run`: every MCP and CLI command uses `skill-studio@latest`.
- The Settings cards share one layout for rows, buttons and errors; a long error shows one sentence with the command output under Show details.
- The skill list shows a loading state and the failure reason instead of an empty list, with Try again.
- Skill Store search ignores stale answers and keeps the search box on screen after an error.
- Settings shows why the version check or project folders failed to load.
- The welcome screen stays open and shows the reason when saving the choice fails.
- Interrupted writes are recovered before new commands; Undo restores every backup; history pages stay fast.
- Network requests to skills.sh, GitHub and the proxy time out instead of hanging.
- `usage` counts sessions and triggers only in the `--days` window; all-time uses stay in `total_uses`.
- `invocation` reports a mismatch only when one harness reads a skill as auto and another as manual; one copy or `unsupported` copies never mismatch.
- Desktop counts, labels and dead ends: the Skills count leaves out parked skills, a spec violation with no quick fix has an Edit SKILL.md button, Compare says when SKILL.md is the same but another file differs, and error and warning rows show a word, not only a color.
- Command palette (⌘K): arrow keys and focus work in WebKit, and ⌘K no longer moves the list cursor.

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
