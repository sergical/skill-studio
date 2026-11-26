# Agent skill conventions

Reference for how each first-class agent discovers, invokes, and controls skills,
and what the agentskills.io spec requires. Verified against the linked docs on
2026-08-22. Re-verify a row before relying on it if the agent shipped a major
release since then.

## Adding a harness

Register the harness in [the Rust catalog](../crates/skill-studio-core/src/harness/registration.rs).
Give it its native skill folders, detection adapter, discovery facts and supported operations.
An install policy opts the harness into per-harness writes. A format-specific reader belongs
with the other readers. Run `pnpm --filter skill-studio run types:generate` to update the
desktop catalog. The generated picker lists require no startup IPC.

For listing views, `HarnessListing.evidence` identifies a discovery result or a rendered
preview. A saved listing has its own session and per-entry observations. Show the budget
control only when the report supplies the setting. The catalog's `verifiedVisibility`
describes a specific vendor revision and cites its sources. It is reference material
for that revision. Use the actual report to describe what Skill Studio saw.

## agentskills.io SKILL.md spec

Source: https://agentskills.io/specification

| Field           | Required | Constraint                                                                                             |
| --------------- | -------- | ------------------------------------------------------------------------------------------------------ |
| `name`          | yes      | 1–64 chars; `a-z`, `0-9`, `-` only; no leading/trailing/consecutive hyphens; equals the directory name |
| `description`   | yes      | 1–1024 chars; what the skill does and when to use it                                                   |
| `license`       | no       | License name or a bundled license file reference                                                       |
| `compatibility` | no       | 1–500 chars; environment requirements                                                                  |
| `metadata`      | no       | Arbitrary string-keyed map                                                                             |
| `allowed-tools` | no       | Space-separated pre-approved tools (experimental)                                                      |

Directory layout: `SKILL.md` (required), optional `scripts/`, `references/`, `assets/`.
The spec has **no** invocation-control fields; those are agent extensions (below).
Reference validator: `skills-ref validate ./my-skill`.

Skill Studio enforces these rules in `crates/skill-studio-core/src/frontmatter.rs`
(`validate_skill`) and reports failures as `spec_violations`.

## What agents do with a spec violation

Measured 2026-10-01: Claude Code 2.1.287, Codex 0.159.2, OpenCode 1.18.30, pi 0.99.1.
Cursor: blocked at login; Grok Build: not installed.

| Violation                          | Claude Code             | Codex                 | OpenCode              | pi                    |
| ---------------------------------- | ----------------------- | --------------------- | --------------------- | --------------------- |
| No description                     | Loads (first body line) | Skips, warns          | Skips silently        | Skips, warns          |
| No name                            | Uses folder name        | Uses folder name      | Skips silently        | Uses folder name      |
| Bad YAML                           | Repairs and loads       | Repairs and loads     | Repairs and loads     | Skips, warns          |
| Name differs from folder           | Uses folder name        | Uses frontmatter name | Uses frontmatter name | Uses frontmatter name |
| Bad name format                    | Loads                   | Loads                 | Loads                 | Loads, warns          |
| Description over 1024 characters   | Loads                   | Loads                 | Loads                 | Loads, warns          |
| Compatibility over 500, 500+ lines | Loads                   | Loads                 | Loads                 | Loads                 |

`specViolationSeverity` in `packages/lib/src/skill-health.ts` encodes this (error, warning, note).
Re-check it when an agent changes.

## Invocation control and per-agent off settings

Clients use the skill description to decide when to load a skill. The table lists
their invocation controls. Skill Studio implements a subset of these controls.
[Skill uses](#skill-uses) covers recorded invocations. [Listing evidence](research/harness-listing-evidence.md)
covers discovery, prompt rendering and session records by client version.

| Agent       | Explicit invocation | Restrict model auto-invoke                                                                                       | Agent setting that hides a skill (Skill Studio reads it)                         |
| ----------- | ------------------- | ---------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| Claude Code | `/name [args]`      | frontmatter `disable-model-invocation: true` (user only); `user-invocable: false` (model only)                   | `~/.claude/settings.json` → `skillOverrides: {"<name>": "off"}`                  |
| Codex       | `$name`, `/skills`  | sidecar `agents/openai.yaml` → `policy.allow_implicit_invocation: false`                                         | `~/.codex/config.toml` → `[[skills.config]] path = "…/SKILL.md" enabled = false` |
| OpenCode    | `/name`             | `disable-model-invocation: true`; `metadata["opencode/autoinvoke"]` overrides it (boolean or number 1/0, v2.2.6) | `permission.skill` in `opencode.json`: `"name": "deny"` (wildcards allowed)      |
| pi          | `/skill:name`       | frontmatter `disable-model-invocation: true`                                                                     | none                                                                             |
| Cursor      | `/name`             | frontmatter `disable-model-invocation: true`                                                                     | none                                                                             |
| Grok Build  | `/name`             | frontmatter `disable-model-invocation: true` in xAI’s published implementation                                   | native `[skills].disabled` exists; Skill Studio does not read it yet             |

Sources: https://code.claude.com/docs/en/skills · https://developers.openai.com/codex/skills ·
https://opencode.ai/docs/skills/ · https://opencode.ai/v2/docs/skills/ · https://pi.dev/docs/latest/skills ·
https://cursor.com/docs/skills · https://docs.x.ai/build/features/skills-plugins-marketplaces

Frontmatter-based controls (`disable-model-invocation`, `user-invocable`) edit the
Universal SKILL.md, so they apply to every agent that reads that folder or a symlink
to it. Config-based controls for Codex and OpenCode apply to one harness.

`skill-studio invocation [skill]` checks both skill-owned controls and reports
manual, auto, or unsupported for each harness and copy. `--manual` or `--auto`
writes the controls to all editable copies of the named skill. OpenCode v2.2.6
uses `metadata["opencode/autoinvoke"]` when set, otherwise it uses
`disable-model-invocation`, otherwise model invocation is allowed. Manual mode
sets the frontmatter flag and changes an existing auto-invoke override to false.
Grok Build reports unsupported. The setter does not change harness config files.

### Other Claude Code frontmatter fields

`allowed-tools`, `disallowed-tools`, `context: fork`, `agent`, `background`, `paths`
(globs that gate auto-loading), `model`, `effort`, `argument-hint`, `arguments`,
`hooks`, `shell`, `when_to_use`, `metadata`, `license`, `compatibility`.

## Scope and destination

Choose a scope and a destination separately:

| Choice      | Values                 | Meaning                                                                                                                                   |
| ----------- | ---------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| Scope       | Global, Project        | Global writes below the user's home directory. Project writes below one selected project.                                                 |
| Destination | Universal, Per harness | Universal writes one canonical deployment to `.agents/skills/<name>`. Per harness writes a separate copy to each selected harness folder. |

Global Universal deployments go in `~/.agents/skills`. Project Universal deployments
go in `.agents/skills`. Codex, OpenCode, pi, Cursor, and Grok Build read these paths
directly. Claude Code can use a per-skill link to the same deployment. Selecting these
readers changes visibility. It does not create more deployments.

Per harness never writes `.agents/skills` and never creates links back to Universal.
Each selected harness gets its own directory copy. Each copy has an independent
lifecycle and can differ from the others. Only the Copy method supports Per harness.
dotagents and skills.sh, including Store installs, support Universal only.

### Parking (disable globally, for every harness)

Parking is Skill Studio's own global disable: it moves a skill's Universal
deployment from `~/.agents/skills/<name>` to `~/.agents/skills-parked/<name>`
(a rename, not a copy), removing a per-skill Claude Code symlink first if one
exists. Every harness that reads the Universal folder loses the skill at once;
unparking reverses both steps. The registry (`~/.agents/skill-studio.json`)
records `parked: {name: {parked_at, source_kind, claude_link}}` so unparking
knows whether to recreate a Claude Code link.

Because parking only moves the Universal folder, a `sync`/`update`/reinstall run
while a skill is parked can recreate `~/.agents/skills/<name>` on its own -
Skill Studio still shows the skill as parked (from the registry record), but
flags it as `parked-but-reinstalled` until it's unparked. Unparking then
reconciles the two copies: if the reinstalled copy is byte-identical to the
parked one, the parked copy is simply discarded; otherwise it's moved to
`~/.agents/skills-trash/<name>-<timestamp>` rather than silently overwriting
either copy.

### Agent settings that hide a skill (read only)

Park is the only way Skill Studio turns a skill off. It never writes an
agent's own config file. It does read these settings and shows them on the
skill page as "Hidden by <agent> setting", with an "Open" action for the file:

- **Claude Code**: `~/.claude/settings.json` → `skillOverrides.<name> = "off"`.
- **Codex**: `~/.codex/config.toml` → `[[skills.config]] path = "…/SKILL.md" enabled = false`,
  matched by the skill's canonical SKILL.md path. Park and unpark leave this
  file byte-identical.
- **OpenCode**: `~/.config/opencode/opencode.json` → `permission.skill.<name-or-glob> = "deny"`.
- **pi and Cursor**: this flow offers Park.
- **Grok Build**: its native `[skills]` settings include `disabled` names and `ignore` paths.
  Skill Studio currently offers Park. [xAI’s skill configuration](https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-pager/docs/user-guide/08-skills.md)

A skill a setting hides stays hidden after unpark: edit the file to change it.
Older builds also wrote these files and, for Claude Code, removed the per-skill
link. Scans still report those leftovers (`disabled_by`), and the journal still
loads their events.

## Discovery paths

| Agent       | Project                               | Global                                                  | Notes                                                                                                                                                                              |
| ----------- | ------------------------------------- | ------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Claude Code | `.claude/skills/`                     | `$CLAUDE_CONFIG_DIR/skills/` or `~/.claude/skills/`     | also plugin cache `~/.claude/plugins/cache`; nested `.claude/skills/` in subdirs                                                                                                   |
| Codex       | `.codex/skills/`                      | `~/.codex/skills/`                                      | also plugin cache `~/.codex/plugins/cache`                                                                                                                                         |
| OpenCode    | `.opencode/skills/` (legacy `skill/`) | `~/.config/opencode/skills/` (legacy `skill/`)          | walks up to the git worktree root                                                                                                                                                  |
| pi          | `.pi/skills/`                         | `$PI_CODING_AGENT_DIR/skills/` or `~/.pi/agent/skills/` | root `.md` files with valid frontmatter count too                                                                                                                                  |
| Cursor      | `.cursor/skills/`                     | `~/.cursor/skills/`                                     | also reads .agents, .claude and .codex skill dirs; plugins in ~/.cursor/plugins/{cache,local}                                                                                      |
| Grok Build  | `.grok/skills/`                       | `$GROK_HOME/skills/` or `~/.grok/skills/`               | also reads .agents and .claude skill dirs; project .grok/skills walks up to the repo root; plugins in ~/.grok/plugins, marketplaces in ~/.grok/config.toml [[marketplace.sources]] |
| Universal   | `.agents/skills/`                     | `~/.agents/skills/`                                     | Canonical deployment. Codex, OpenCode, pi, Cursor and Grok Build read it directly. Claude Code needs a symlink.                                                                    |
| parked      | n/a (global only)                     | `~/.agents/skills-parked/`                              | Skill Studio's own root for parked (disabled globally) skills - see "Parking" above; excluded from coverage                                                                        |

Plugin caches also hold the harness's bookkeeping, which is never a plugin.
Claude Code writes `<plugin>/<version>/.in_use/<pid>` each time one of its
processes starts or stops, and clones a marketplace into `temp_git_<ms>_<id>`
before it moves a plugin into place. Codex writes
`.codex-remote-plugin-install.json` through a `.tmp*` file. The plugin walk
skips dot-named entries (except the `.claude-plugin`, `.codex-plugin` and
`.cursor-plugin` manifest folders) and `temp_git_*`, and the desktop watcher
does not rescan when they change (`harness::is_plugin_cache_bookkeeping`).

## Local data sources Skill Studio reads

| Purpose                            | Location                                                                                           | Shape                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| ---------------------------------- | -------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Installed-skill lock               | `~/.agents/.skill-lock.json`                                                                       | `{version, skills: {name: {source, sourceType, sourceUrl, skillFolderHash, installedAt, updatedAt}}}`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| dotagents declared skills          | `~/.agents/agents.toml`                                                                            | `[[skills]]` rows: `{name, source, path, ref?}` - `ref` absent means unpinned; no `[[skills]]` row for a lock entry means a wildcard (`--all`) install                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| dotagents resolved skills          | `~/.agents/agents.lock`                                                                            | `[skills.<name>]` tables: `{source, resolved_path, resolved_commit}` - the commit actually on disk                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| Fork registry (Skill Studio-owned) | `~/.agents/skill-studio.json`                                                                      | `{version, forks: {name: {forked_at, origin_tool, origin_source, repo, path, declared_ref, base_commit}}, pack_originals: {scope: {skill: [{source, original, linked_at}]}}, parked: {name: {parked_at, source_kind, claude_link?}}, harness_disabled: {name: {"claude-code": {link_target}}}, projects: {added: [path], excluded: [path]}, discovery: {harness: false}}` - a harness id mapped to `false` switches its project history off for discovery in the desktop, CLI, and MCP server; a missing key means on. Trials were removed in #285; a leftover `trials` map from before that round-trips unchanged and is otherwise ignored |
| Skill uses                         | see [Skill uses](#skill-uses)                                                                      | one reader per harness                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| Projects list (Codex)              | `~/.codex/config.toml`                                                                             | `[projects."/abs/path"]` sections                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| Projects list (Claude Code)        | `~/.claude/projects/<encoded-path>/`                                                               | `cwd` field inside the transcripts (dir name encoding is lossy)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| Projects list (pi)                 | `~/.pi/agent/sessions/--<encoded-cwd>--/*.jsonl`                                                   | `cwd` in the first (`"type":"session"`) record (dir name encoding is lossy: `/ \ :` all become `-`)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| Projects list (Cursor)             | `<user data dir>/Cursor/User/workspaceStorage/<hash>/workspace.json`                               | `{folder: "file:///abs/path"}`; `workspace` (multi-root) and `vscode-remote://` entries name no local folder. User data dir: `~/Library/Application Support` (macOS), `~/.config` (Linux), `%APPDATA%` (Windows). Not documented by Cursor                                                                                                                                                                                                                                                                                                                                                                                                  |
| Projects list (OpenCode)           | `~/.local/share/opencode/opencode.db`, `opencode-<channel>.db`; legacy `storage/project/<id>.json` | `project.worktree` (SQLite, WAL mode) and the legacy `worktree` key. Channels `latest`, `beta` and `prod` use `opencode.db`; any other channel (`next` = v2 beta, `local` = source build) uses `opencode-<channel>.db`. Sessions outside a project have worktree `/`. Open with `mode=ro` only while both `-wal` and `-shm` exist, else `mode=ro&immutable=1`: a plain read-only open creates the missing files. `$XDG_DATA_HOME` and `OPENCODE_DB` move the files; Skill Studio reads only the default location                                                                                                                            |
| Projects list (Grok Build)         | `~/.grok/sessions/<encoded-cwd>/<session-id>/`                                                     | Folder name is the cwd percent-encoded (only `A-Z a-z 0-9 - _ . ~` kept, so `/` is `%2F`) while that fits in 255 bytes. A longer cwd gets `<slug>-<blake3 hex16>` and a `.cwd` file with the path; a slug never decodes to an absolute path. Source: `encode_cwd_dirname` / `decode_cwd_from_dirname` in xai-org/grok-build `crates/codegen/xai-grok-config/src/paths.rs`. `$GROK_HOME` moves the store; Skill Studio uses the resolved root                                                                                                                                                                                                |
| skills.sh search                   | `https://skills.sh/api/v1/skills/search`                                                           | `{data: [{id, name, installs, source}]}` (`source`, not `topSource`)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |

### Skill uses

Skill Studio counts only the last 30 days of history (`HISTORY_DAYS` in
`crates/skill-studio-core/src/skill_uses.rs`, mirrored in
`packages/lib/src/skill-stats.ts`). Claude Code deletes its transcripts after
30 days by default (`cleanupPeriodDays`), so older history is missing for most
people, and agents keep their logs for different times. Every count, last
use, heatmap day and window uses this one horizon. A use older than 30 days
counts nowhere. A resumed session still counts once, at its first load: the
session dedupe runs before the history cut. CLI `usage` and `library` and MCP
`skill_usage` and `library` refuse `days` above 30 and a `since` before the
first day of the history with `invalid_request`.

Each skill use has one of three triggers. The names follow Grok Build's own
telemetry (`SkillTrigger` in xai-org/grok-build
`crates/codegen/xai-grok-telemetry/src/events/skills.rs`):

- **user**: the person typed the skill command (`/name`, `$name`, `/skill:name`).
- **agent**: the model called a skill tool.
- **file read**: the model read a `SKILL.md` with a file or shell tool. Count it
  only when the path is inside a known skill root. The skill list in the
  instructions names every `SKILL.md` path on every turn, so text that only
  mentions a path is not a use.

Checked on 2026-09-16 against harness source, local history, and one live CLI
run per harness. `docs/research/harness-primitives.md` section 7 has the
evidence.

| Harness       | Store and folder                                                                                                                                                                                                                                                                                                        | user                                                                                                                                                                                                                                                 | agent                                                                                                                                                                                                                                                                                                                                                                                                | file read                                                                                                                                                                                                                 |
| ------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Claude Code   | `~/.claude/projects/*/*.jsonl` and `~/.claude/projects/*/<session>/subagents/*.jsonl`; `cwd` on each record                                                                                                                                                                                                             | user text `<command-message>X</command-message>` + `<command-name>/X</command-name>`, with no tool call                                                                                                                                              | assistant `tool_use` `{"name":"Skill","input":{"skill":"X","args"?}}`                                                                                                                                                                                                                                                                                                                                | not seen: the model uses `Skill`                                                                                                                                                                                          |
| Codex         | `$CODEX_HOME/{sessions,archived_sessions}/**/*.jsonl` and `*.jsonl.zst` (default `~/.codex`); `session_meta.payload.cwd`. `originator` names the surface (`Codex Desktop`, `codex_exec`, ...)                                                                                                                           | `response_item` user `message` with a content item whose text starts with `<skill>\n<name>X</name>\n<path>P</path>`. `$X` makes it in the CLI, IDE, desktop app, and `codex exec`. Other items quote `<skill>` mid-text; do not count them           | none in use. The source has a `skills` namespace tool (`read`, `list`; openai/codex `codex-rs/ext/skills/src/tools/`) that no local session calls and the docs do not name. Read a `function_call` with `namespace: "skills"`. `package` is either the host provider's `SKILL.md` path or the executor provider's `skill://<root-id>/<path>` URI; the skill name is the folder that holds `SKILL.md` | `custom_tool_call` `exec` whose command reads a `SKILL.md` (`cat`)                                                                                                                                                        |
| OpenCode (v2) | `~/.local/share/opencode/opencode.db` (current v2 beta, `opencode2`) and `opencode-next.db` (older v2 builds), table `session_message`; `session_v2.directory` or `session.directory`. Every `opencode-next.db` session is also in `opencode.db`: dedupe by id                                                          | `type='skill'` row, `data` `{skill, name, text, time.created}`, from the client `session.skill` action (event `session.skill.activated`). `opencode2 run "/X ..."` stores plain user text instead, and the model then calls the tool                 | `type='assistant'` `data.content[]` item `{type:"tool", name:"skill", state.input:{id:"X"}}`. The dev source names the key `name` (`packages/core/src/tool/skill.ts`); read both                                                                                                                                                                                                                     | `read` or `shell` tool on a `SKILL.md`                                                                                                                                                                                    |
| pi            | `~/.pi/agent/sessions/**/*.jsonl`; `cwd` in the `session` header                                                                                                                                                                                                                                                        | user text starts with `<skill name="X" location="P">` (`/skill:X`, interactive and `pi -p`). The text stays a literal `/skill:X` when pi did not load the skill, for example project `.agents/skills` in an untrusted folder (`--approve` trusts it) | none: pi has no skill tool                                                                                                                                                                                                                                                                                                                                                                           | `toolCall` `read` of a `SKILL.md` (`bash` `cat` also seen)                                                                                                                                                                |
| Cursor        | `~/.cursor/projects/<name>/agent-transcripts/<session>/*.jsonl`; `<name>` is the Cursor workspace folder's path, without the leading `/` and with every non-letter/digit changed to `-`; `<session>` is the `agent-transcripts/<session>` dir                                                                           | docs name `/skill-name` (https://cursor.com/docs/skills); not seen in local data                                                                                                                                                                     | none: no skill tool in transcripts or in `state.vscdb` tool names                                                                                                                                                                                                                                                                                                                                    | `Read`, `ReadFile`, or `Shell` call on a `SKILL.md`                                                                                                                                                                       |
| Grok Build    | `~/.grok/sessions/<encoded-cwd>/<session-id>/updates.jsonl` plus `summary.json`; a long cwd falls back to a `.cwd` file. A forked session copies its parent's lines with fresh timestamps but keeps `toolCallId`; its `summary.json` gets `forked_at`, so copied lines (`timestamp <= forked_at`) are skipped on replay | ACP `user_message_chunk` whose text starts with `/name` (skip `_meta.hostTurn: true` echoes)                                                                                                                                                         | `tool_call`/`tool_call_update` with `_meta["x.ai/tool"].kind == "skill"`, title `Skill: <name>`                                                                                                                                                                                                                                                                                                      | `tool_call`/`tool_call_update` `read` of a known skill's `SKILL.md` (path from `_meta["x.ai/tool"].input.path`, `locations[0].path`, or `rawInput` with `variant: "ReadFile"`), or an `execute`/shell call that reads one |

Claude Code adds an `isMeta` user message that starts with
`Base directory for this skill:` after both the typed command and the `Skill`
call. It is part of the same use; do not count it again.

CLI `usage` and MCP `skill_usage` report `recent_uses` for the requested window and windowed `triggers`, plus `last_used` across the 30-day history. These reports count each installed skill once per session, after applying the window; the desktop keeps its separate event and file-read counting rules. Usage rows do not expose a separate session count. The review-date report keeps its standalone distinct `(harness, session identity)` count for the last 30 days. Transcript logs without a native session id use their file path as the identity; sub-agent logs have their own identity. The normalized `use_session` identity is shared with the review-date count. The native `session` remains unchanged for desktop counts. Database records without a session identity do not add a review-date session. Trigger keys are `user`, `agent`, `file_read`, and `unknown`; `unknown` is zero because readers require direct evidence. A mention of a skill is not evidence of use. The table above states which signals each reader accepts. In particular, pi has no agent skill tool, and Cursor supplies only file reads, with file mtime as the use time.

Codex history includes streaming reads of `.jsonl.zst` files in both `sessions`
and `archived_sessions`. `$CODEX_HOME` moves both the reader and its live watch.
Claude Code also writes a typed command wrapper for built-ins such as
`/context`. A following system record with subtype `local_command` links to
that wrapper by its parent UUID. Skill Studio excludes that command from skill
use counts.

A changed compressed file is decoded again; an unchanged file uses the cache.
Large files resume at a decoded byte offset. The decoder replays the prefix
without buffering it because zstd does not provide a byte seek.

CLI `usage --harness codex --project /absolute/project` and MCP
`skill_usage({"harness":"codex","project":"/absolute/project"})` select exact
recorded values before counting. CLI `--project` can repeat to include several
paths. An empty selection includes all history. Records with no project path
do not match a project filter. Existing file-read deduplication still applies.

`observations` names disabled, unreadable, corrupt, or skipped history. Missing
roots mean no history and do not make the report partial. Existing history
that could not be read makes the report partial; CLI exits with status 4.
CLI and MCP read all changed history in one pass without byte deferral.
They read the desktop cache but never write it. CLI saves its own
`cli-skill-uses.json` beside that cache with an atomic rename. MCP keeps its
index in memory.

## Update check

`src-tauri/src/skills/skill_update_check.rs` compares each global-scope,
GitHub-backed skill's installed commit against the newest commit `gh api`
reports for its path, on a 6-hour timer plus a manual "Check now" (Issues
view). For a dotagents skill the installed commit comes straight from
`agents.lock`; for a skills.sh skill it's the newest commit at or before the
lock entry's `updatedAt` (cached until `updatedAt` changes, so a lock entry
that hasn't moved never re-queries its baseline). Results persist at
`<app data dir>/skill-studio/update-check.json` so a full snapshot rebuild can
read them without shelling out. Access is read-only (`gh api repos/.../commits`)
through the user's own `gh` CLI login; the app stores no tokens itself, and
"Update" runs the tool that owns the skill (`npx @sentry/dotagents add|install`
or `npx skills update`) rather than writing to `agents.toml`, `agents.lock`, or
`.skill-lock.json` directly.

## Fork / Pull upstream / Un-fork

A dotagents or skills.sh skill's local edits don't survive that CLI's own
lifecycle: dotagents `install` overwrites the folder outright (`sync`
preserves edits, but `install`/re-adding does not), and a folder `sync`
finds with neither a `[[skills]]` row nor a lock table gets silently adopted
as `source = "path:.agents/skills/<name>"` rather than kept as the skill the
user meant to edit. "Fork" (`src-tauri/src/skills/skill_fork.rs`) detaches a
skill from its owning ledger so edits stick: it snapshots the current folder,
removes the skill from its ledger (`npx @sentry/dotagents remove` or
`npx skills remove`), restores the folder if the removal deleted it, and
records a `ForkRecord` in `~/.agents/skill-studio.json`. Wildcard dotagents
entries (`name = "*"`, no per-skill manifest row) are refused in v1 - forking
one by name first requires adding a named row for it, which dotagents
doesn't offer without a fresh `add`.

"Pull upstream" never merges automatically: it compares the skill's
last-synced snapshot (`base`), its current on-disk copy (`mine`), and a
freshly fetched upstream copy at the latest commit (`theirs`), file by file:
unchanged-in-mine takes theirs; unchanged-in-theirs keeps mine; a text file
that differs on all three sides gets git-style conflict markers written into
it, and the file is opened in the user's editor for manual resolution; a
binary file that differs on all three sides keeps mine and is flagged. Files
added or removed upstream are added or removed locally when the local copy
hadn't diverged.
The snapshot always advances to the new upstream commit afterward, even when
there were conflicts, so the fork's `base_commit` stays a true "last pulled"
marker. The upstream copy is fetched read-only via
`gh api repos/{owner}/{repo}/tarball/<sha>`, extracted with `tar`, and never
writes back to GitHub or to the owning CLI's own files.

"Un-fork" discards local edits and reinstalls the skill from its recorded
origin (`declared_ref`, if any, for a dotagents fork), then drops the fork
record and snapshot - the frontend confirms this destructively first.

## Lifecycle targets and safety

Each discovered deployment has a stable `deployment_id`, destination, backing
relationship, owner kind, and mutability. A mutating command receives one exact
`deployment_id` or one explicit `owner_id`. If a scope has more than one mutable
owner, the command does not select one by skill name. Before writing, the backend
reloads the snapshot and confirms that the id still matches the path, scope, and
destination.

An owner-wide update or removal affects only deployments with that owner id. Global
and Project ledgers have different owner ids. A Per harness Copy has its own lifecycle
and does not inherit a Universal ledger. Park and unpark accept only the canonical
Global Universal deployment. They do not move Project deployments or Per harness
copies.

Skill Studio can change deployments owned by skills.sh, dotagents, Copy, or a fork.
Plugin, in-repo, manual, wildcard-dotagents, and ambiguous deployments are read-only.
Lifecycle commands reject read-only deployments. They do not guess an owner or
delete a path found during discovery. Source reads, update checks, plugin-cache
enumeration, and GitHub fetches do not write to those sources. Managed updates and
removals run the owning CLI instead of editing its lock or manifest files.

### Compatibility identifiers

The scanner and existing DTOs can still use the machine value `shared` for the
Universal `.agents/skills` root. Existing fields such as
`claude_reads_shared_folder`, `shared_via_whole_dir_link`, event kinds such as
`explode_shared_dir`, and internal `shared-root` relationship names remain on the
wire for compatibility. UI copy must call this destination Universal. These
compatibility values do not define another destination.

## Add skill

The "Add skill" sheet (`src/components/AddSkill/AddSkillSheet.tsx`, backend
`skills/skill_add.rs`) accepts a free-text source - `owner/repo`,
`owner/repo/<path>`, a `github.com` URL (bare, `/tree/<ref>/<path>`, or
`/blob/<ref>/<path>/SKILL.md`), a `skills.sh/<owner>/<repo>/<skill>` URL,
`git:<url>` or a bare `*.git` URL, or an absolute/`~/` local path - parsed by
`src/lib/skill-source-parse.ts` into a `ParsedSkillSource`. One of three
methods installs it: **dotagents** (`npx -y @sentry/dotagents add`, tracked in
`agents.toml`/`agents.lock`), **skills.sh** (tracked in `.skill-lock.json`;
GitHub sources only), or **Copy** (untracked, with no update ledger).

dotagents and skills.sh always install one Universal deployment. A skills.sh install
uses its Universal target. It does not use a direct reader such as Codex as a proxy
target. The install can also request a Claude Code link. Copy can install one
Universal deployment or separate Per harness copies. For Per harness, the backend
copies the fetched source into each selected harness path and rejects an empty
selection. Project scope uses the same layout below the selected project.

The sheet's **Paste** tab makes a new skill from pasted text (a whole SKILL.md,
or the instructions plus a name and description). The desktop builds the file,
shows `validate_skill`'s problems before saving, and writes it as a Global
Universal Copy through the same `ops::install`, so it is a Copy Skill Studio
owns. It refuses a folder that already exists.

## Trials (removed)

The 24-hour trial install was removed in #285. There is no time-boxed install
mode anymore. To switch a skill off without deleting it, use park
(`park_skill`/`unpark_skill`, `src-tauri/src/skills/skill_park.rs`) - see
[Flows](flows.md). A `trials` map left over
in `~/.agents/skill-studio.json` from before #285 round-trips unchanged and is
otherwise ignored.

## Navigation

The app shell keeps two different jobs apart: the sidebar (`Sidebar.tsx`) holds
_places_ - Home, Skills, Activity, and Parked - and the Skills view's
filter bar (`SkillListFilterBar.tsx`) holds _filters_ over that one list:
scope (all/global/project), harness, source, a coverage toggle, and a
free-text query. There is no separate page per scope, per harness, or per
issue kind; every one of those is a value of `SkillListFilter`
(`src/lib/skill-list-filter.ts`), applied to the same `SkillsView`. Deep
links (Home's "N missing" card, the sidebar's Parked row) work by handing
`ActiveView`'s `{ kind: "skills", filter }` a partial filter, not by routing
to a different view.

Plugin-shipped skills are a _source_ of skills, not a managed primitive of
their own: `source_kind: "plugin"` is one value of the Skills view's Source
filter, on the same list and the same `SkillListTable`/`SkillCoverageMatrix`
as every other skill - see `ownSkillsView`/`pluginSkillsView` in
`skill-plugin-partition.ts`. There is no standalone "Plugins" page.

Health issues (`collectDashboardIssues`, `src/lib/skill-health.ts`) are
surfaced on Home's "Needs attention" card and reachable from the Skills view
via `filter.issue`. The kinds are: `parked-but-reinstalled`, `duplicate`,
`broken-symlink`, `spec-violation` (error-severity agentskills.io violations, see
`specViolationSeverity`), `spec-warning`, and `lock-only`. Two things that look like
issues deliberately are not: a skill that has never been invoked (noise, not
something worth fixing for every skill) and a skill with an update available
(that's the "Updates" section on Home, a routine action, not a health
problem).

## Model listing budgets

Verified on 2026-11-11 against Claude Code 2.1.319 and Codex CLI
0.167.0, and on 2026-11-19 against Claude Code 2.1.327 and Codex CLI 0.170.0. `skill-studio doctor --json` and the MCP `doctor` tool return
`listing_cost` for each harness and session scope. Project rows include global
skills. Codex and Claude keep same-name skills from different files and remove
duplicate file paths. Parked, hidden, disabled-plugin, and manual-only skills
do not enter the count. Codex also rejects missing or blank descriptions.
The five largest entries help the owner choose skills to park or make manual-only.

Claude's installed executable at `~/.local/share/claude/versions/2.1.327`
contains its JavaScript (checked on 2026-11-19).
`hVt` computes the character budget as context tokens times characters per
token times `skillListingBudgetFraction`, default 0.01, at least 1.
`SLASH_COMMAND_TOOL_CHAR_BUDGET` replaces it. Only when the fraction is not
set, an account experiment (`tengu_typed_pebble`, cached in `~/.claude.json`
`cachedGrowthBookFeatures`, capped at 100000) can raise it; this path is
inferred from the code, not observed. The context size is the active model's
window (`$h`); characters per token (`Ky`) are 4 for Claude 3.x and the 4.0 to
4.6 models and 3 for newer models. Skill Studio resolves the model from the
`model` setting, else the last session's model, through Claude Code's cache
`~/.claude/cache/model-catalog/*.json`: `provider_alias_targets.<alias>.default`
gives the id, and `runtime.max_input_tokens` the window. A `[1m]` suffix means 1000000. On this machine `opus` is `claude-opus-5-5` with 1000000 tokens, so
0.01 is 30000 characters; a `--debug-file` log of `claude -p` printed
"9000 budget" at 0.003, which matches.

`oVn` builds the listing. An entry is `- name: description`, or `- name`
without one. The description is the frontmatter `description` plus
` - when_to_use` when set, cut to `skillListingMaxDescChars` (default 1536)
with an ellipsis. Width is display width (`Bun.stringWidth`); entries are
joined by newlines. When the full listing fits, every entry keeps its
description. Otherwise bundled skills and `skillOverrides` `name-only`
entries keep their form, every other entry starts as a name, and descriptions
come back in usage-score order while each still fits in what is left (a
greedy pass: a smaller description later in the order can still fit). The
score (`jBr`) is `usageCount × max(0.5^(days since lastUsedAt / 7), 0.1)` from
`skillUsage` in `~/.claude.json`. The sort is stable, so equal scores keep
listing order: user and plugin skill folders, bundled skills, then built-in
commands and synced plugins. `skillOverrides` `off` and `user-invocable-only`
remove an entry. Plugin skills are listed as `plugin:name`, and only for user
installs or installs for the session's project (`plugins/installed_plugins.json`).
The listing keeps one entry per name.

Claude Code records each session's listing in its transcript
(`~/.claude/projects/<path with non-alphanumerics as ->/<session>.jsonl`) as an
`attachment` of type `skill_listing` with `isInitial`, `names` and `content`.
Skill Studio reads the newest 20 transcripts for the scope: the newest gives
the observed state and the listing order, and all of them size the entries
Skill Studio does not scan (bundled skills, built-in commands, synced plugins).
An entry never seen with a description stays a name; one seen both ways
competes for the budget. The settings loader is the same as before: `yt`
merges user `~/.claude/settings.json`, project `.claude/settings.json`, and
project `.claude/settings.local.json`; managed settings and session flags can
also apply. The skill directory loader removes duplicate real file paths.
Disabled plugins do not supply skills. When an estimate uses a saved listing
from a known project to account for bundled entries, it scans that project's
skills as well. Entries supplied by the project stay in the project count.

Codex's installed package identifies version 0.167.0. Its binary contains
`SkillsConfig.max_context_tokens`, `SkillMetadataBudget`, and the bounded-list
warnings from `ext/skills/src/render.rs`. The matching
[tagged renderer source](https://github.com/openai/codex/blob/rust-v0.167.0/codex-rs/ext/skills/src/render.rs)
sets the budget to 2% of the supplied context window. Without one, it uses 8000
characters. An explicit `skills.max_context_tokens` overrides this and is capped
at 10000 tokens. The renderer caps descriptions at 1024 characters. It counts
names, descriptions, locators, line syntax, and any root-alias overhead. It uses
UTF-8 bytes divided by four for token estimates. When the full list does not fit, it gives
each description one more character per pass, in list order, while the line
still fits (so all descriptions shrink about evenly from the end); if even
names and locators do not fit, it keeps names in list order while they fit
and omits the rest. The core-compatible order is repository skills, then
user skills, by name, then path. Codex 0.170.0 `render.rs` is unchanged. The
context window is the model's `context_window` (else `max_context_window`)
in `$CODEX_HOME/models_cache.json` for the `model` setting; on this machine
`gpt-6-astra` has 272000, so the budget is 5440 tokens. Extension listings
can use `interface.short_description` from `agents/openai.yaml`. The
[config type](https://github.com/openai/codex/blob/rust-v0.167.0/codex-rs/config/src/skills_config.rs)
loads through Codex's config layers, which can include user, project, and session
settings. The [host merge](https://github.com/openai/codex/blob/rust-v0.167.0/codex-rs/ext/skills/src/loader/host_merge.rs)
uses document paths as identity. Each distinct file keeps its own locator.
The [parser](https://github.com/openai/codex/blob/rust-v0.167.0/codex-rs/skills/src/parser.rs)
rejects descriptions that are empty after whitespace normalization.
`[[skills.config]] enabled = false` selects a document by canonical path or
frontmatter name in any root, including Universal. Rules apply in order; later
matching rules override earlier rules. Other readers of that file stay enabled.
The [config loader](https://github.com/openai/codex/blob/rust-v0.167.0/codex-rs/config/src/loader/mod.rs)
loads project layers from the project root to the working directory. It checks
user `projects.<path>.trust_level` records for the directory, then the project
root, then the repository trust root. Untrusted project layers do not apply.
The active model supplies the context size; `model_context_window`
can override it.

Checked again on 2026-11-20 against the Codex 0.170.0 source (`ext/skills/src/render.rs`,
`host_aliases.rs`, `aliases.rs`, `catalog_prompt.rs`, `skills/src/parser.rs`) and
the `### Available skills` block of real sessions:

- **Reading a skill.** Codex parses the frontmatter as YAML. On an error it
  quotes each plain `key: value` line whose value holds `: ` and parses again
  (`repair_frontmatter_scalar_fields`). Any other error, such as a repeated
  `metadata:` key, drops the skill. Whitespace runs in `name` and
  `description` become one space. Codex ignores `disable-model-invocation`:
  only `policy.allow_implicit_invocation: false` in `agents/openai.yaml` hides
  a skill, so a skill that Claude Code keeps manual-only can still be listed
  by Codex. `listing` flags this as `invocation_mismatch`.
- **Names and order.** Plugin skills are `plugin:name`. Names over 64 characters, and a `metadata`
  that is not a mapping, drop the skill. The list is in catalog
  order: system skills (`$CODEX_HOME/skills/.system`), then repository skills,
  then user skills, each by name, then path. The `description` is used, not
  `metadata.short-description`.
- **Short paths.** Each line ends in `(file: <path>)`. Codex renders the list
  once with absolute paths and once with `rN/...` aliases plus a
  `### Skill roots` table, and keeps the one that lists more skills, then cuts
  fewer description characters, then costs less. The table costs the token
  difference between the aliased and the absolute `## Skills` header. A plugin
  version with one skill shares its `plugins/cache/<marketplace>` folder as
  its root.
- **Tiers.** All lines fit: every description is whole. Otherwise, if every
  name-and-path line fits, every skill stays and the rest of the budget goes
  round robin, one character at a time, so descriptions end up about the same
  length. Otherwise there are no descriptions and lines are kept in order
  while they fit. Host skills render with the core-compatible policy, so
  there is no `- N additional skills omitted` line (none appears in real
  sessions, also at budgets that leave skills out). A line costs
  `ceil(bytes / 4)` tokens with its newline.
- **Budget.** The model of the newest session decides the budget, through
  `context_window` in `$CODEX_HOME/models_cache.json`: T3 Code sessions on
  `gpt-6.1-sol` (500,000) get 10,000 tokens, while the configured
  `gpt-6-astra` (272,000) gets 5,440. The `model_context_window` a session
  logs in `task_started` (475,000 for `gpt-6.1-sol`) is the usable part and is
  used only when the model is not in the cache; 2% of 475,000 (9,500) does not
  match the real lists.
- **What Skill Studio adds from a session.** The newest rollout outside a
  temporary folder (for the global listing, the newest without repository
  skills, among the 20 newest) gives the entries Skill Studio does not scan
  (system skills, skills in nested folders such as
  `~/.agents/skills/synced/<id>/`), with their descriptions read from their
  `SKILL.md` inside the home. It also gives the alias order, the observed
  state of each skill, and the model. `--codex-session <rollout>` compares
  with one chosen session instead. Codex prints paths with links resolved.
- **Plugin load state.** A listing that kept some description left no skill
  out, so an installed plugin with no skill in it was not loaded ("plugin not
  loaded"; `codex exec` does not load the `openai-primary-runtime` plugins,
  the T3 Code app does, and only the curated-remote `visualize` of two copies
  is loaded). A names-only listing is not evidence: the newest complete
  listing in any session supplies the load state and the unscanned skills.
- **Match rate.** Replaying the 15 newest real sessions with `--codex-session`
  listed 2,774 of 2,774 recorded entries in the same order and tier, and gave
  the exact description length for 2,769 (99.8%); 14 sessions matched
  exactly. The ground-truth session `rollout-2026-11-20T13-41-23-...` (185
  lines, 182 names, every copy counted) matched 185 of 185. The overflow runs
  of the `codex exec` eval home (`skills.max_context_tokens` 1000, 2000, 2800,
  3200, 4000), replayed with `--home` on that home, matched exactly: 52
  listed and 126 omitted at 1000, 111 and 67 at 2000, 163 and 15 at 2800,
  and 178 of 178 description lengths at 3200 and 4000. `--codex-session`
  reads only rollouts inside the home's `sessions` or `archived_sessions`
  folder, after resolving links.

Skill Studio estimates name, capped description, punctuation, and Codex's file
locator in UTF-16 characters, or UTF-8 bytes for Codex, divided by four,
rounded up. This is a planning
estimate before budget reduction, not a copy of a running session's prompt.
It does not reproduce plugin namespaces, cloud skills, alias tables, short
descriptions, runtime flags, environment overrides, managed policy, or model
catalog selection. The scan's completeness still applies. It reads Claude user
and selected project settings. It reads Codex's user config,
`model_context_window`, and trusted project
`.codex/config.toml` layers. Each budget reports the file that supplied it.
Project trust uses exact and canonical path records and the nearest configured
project root marker, `.git` by default. Separate worktree repository trust
roots, system config, and session overrides are not resolved. Without a known
model, it reports Claude's 200000-token default at 4 characters per token,
or Codex's 8000-character fallback, as 2000 estimated tokens. The
`limit_source` and `uses_default_context_window` fields state the fallback.
An unknown limit is null with source `no known limit`.

`skill-studio listing`, the MCP `listing` tool and the desktop Agents page (What it sees)
page report each skill's state per agent and scope: `described`, `truncated`,
`name_only`, `omitted` or `hidden` (manual-only, off in settings, parked,
plugin off or installed for another project, or rejected by Codex). Inputs
predict another Claude fraction, context window or Codex
`skills.max_context_tokens`. The desktop saves supported numeric settings in regular JSON and
TOML config files. For chezmoi-managed files it opens the owning source.
Previewing a budget only estimates the effect. Measured on 2026-11-19 with five `claude -p` sessions at
fractions 0.003 to 0.02: 418 of 420 predicted skill states matched the
recorded listing (99.5%), and the model named the described skills of its
own listing correctly in 755 of 755 cases. Skill Studio currently computes budgets for
Claude Code and Codex. Grok Build also publishes a budgeted renderer. OpenCode
and pi publish listing formats that can be rendered without a budget setting.
See [the six-client evidence](research/harness-listing-evidence.md) for version and session coverage. `near_limit` starts at 80%;
`over_limit` means the estimate exceeds the budget. These are warning findings.

Manual-only checks are per harness. Claude, pi, and Cursor read
`disable-model-invocation: true`; Codex reads
`policy.allow_implicit_invocation: false` in `agents/openai.yaml`; OpenCode reads
`metadata["opencode/autoinvoke"]: false`. A restriction for one harness does not
hide a skill from another harness that does not support that field.

## Skill sessions

Checked on 2026-11-20 against the session files on one machine (500 Claude
Code transcripts, 301 Codex rollouts). `skill-studio sessions`, the MCP
`skill_sessions` tool and the desktop **Sessions** page join each session's
recorded listing with its skill uses.

- **Claude Code** writes the listing into the transcript as `attachment`
  rows of type `skill_listing` (`names`, `content`). The first has
  `isInitial: true`; later ones add skills that appear during the session.
  An entry `- name: description` is described; `- name` is name only. Claude
  Code can save the actual description text. Sessions preserves that text, including line breaks.
  Subagent transcripts (`<session>/subagents/*.jsonl`) carry no listing and
  are not read.
- **Codex** writes the listing into the developer message inside
  `<skills_instructions>`, under `### Available skills`, as
  `- name: description (file: rN/...)`. Plugin skills are `plugin:name`. A
  skill that is not in the block was omitted. A description shorter than the
  scanned one (whitespace collapsed) is reported as shortened.
- **OpenCode 2.3.2** saves the current skill IDs, names and descriptions for each session in the instruction blob referenced by `current_values["core/skill-guidance"]`. The saved set reflects that agent’s permissions and auto-invocation settings. What it sees shows current folder discovery. What it used shows the saved guidance.
- **pi 1.6.0** formats the base prompt from eligible skills in loader order. What it sees previews the default read tool and scanned roots in a trusted project, including XML-escaped locations. What it used reads the most recent structured system skills section on the saved JSONL branch, including replacements and explicit removal. An extension can also force a different outgoing prompt without recording it.
- **Cursor and Grok Build** have usage readers and folder discovery. The version-specific [listing research](research/harness-listing-evidence.md) describes additional sources.

Uses come from the same parsers as Activity ("Skill uses"): a skill tool
call, a typed command, or a `SKILL.md` read. A use names a listing entry by
name, or by the part after `plugin:`. Skill Studio reads the newest 100
sessions per agent from the last 30 days (`--sessions` up to 500), streams
each file line by line, and skips lines above 8 MB.

Flags per skill: **listed, never used** (listed in at least 3 sessions and in
at least 80% of the sessions with a listing, no use); **used while name
only**; **possibly missed**; **not described now** (the `listing` prediction
is name only, shortened, omitted or hidden, with the reason); and **not
reachable** (installed in scope, but in no folder the agent reads). Each agent
row links its skills doc: [Claude Code](https://code.claude.com/docs/en/skills),
[Codex](https://developers.openai.com/codex/skills),
[Cursor](https://cursor.com/docs/skills),
[OpenCode](https://opencode.ai/docs/skills),
[pi](https://docs.pi.dev/latest/skills).

**Possibly missed is a guess.** No agent has a skill search, and there is no
record of what the model should have used. Skill Studio scores the session's
first prompt (400 characters or less) against every skill description with
BM25. When the best score is 10 or more and the session used no skill, that
skill is a possible miss. On 282 eval sessions with known expected skills
(`~/.agents/evals/ui-skills`), this flagged 62 sessions with precision 0.58,
and found 36 of the 75 sessions that really missed. Phrase lists taken from
"Triggers on:" lines reached about 0.15 precision in a prototype. To know for
sure, run an eval like `compare.py` for the chosen skills.

Parse checks on the same machine: every recorded name appeared in 500 of 500
Claude Code listings, and 301 of 301 Codex listings matched their block.
Model-started uses agreed with `compare.py`'s parser (Skill calls and `Read` of
`SKILL.md`) in 442 of 500 sessions. Every difference was a use that Skill
Studio also counts and `compare.py` does not, such as `cat SKILL.md` through
the shell.

## Non-skill configuration inventory

`skill-studio extras [--project <path>] [--json]` and the MCP `extras` tool
list declaration names and file paths. They do not run hooks, connect to MCP
servers, or read instruction bodies. Hook counts count declared handlers,
including handlers with the same command. A file that cannot be read or parsed
gets one `unreadable` row. Parser messages never enter the output.

These are declarations, not proof that an agent loaded them. Trust, enable
settings, compatibility switches, and precedence can change what loads. The
inventory covers the global root and each selected project root. It does not
walk project children, parent directories, plugin caches, managed policy files,
or config paths named by a value in another config file. It does not expand
instruction imports, globs, or environment variables. Codex and OpenCode use the
config roots already resolved by the runtime scope. Live scope uses `CODEX_HOME`
for Codex and `OPENCODE_CONFIG_DIR` or `XDG_CONFIG_HOME/opencode` for OpenCode,
with the default paths below when unset. Explicit `--home` and fixture scopes
keep these roots under the selected home. Other roots use the default paths below.

Checked against upstream docs on 2026-11-11:

| Agent       | Hooks                                                                                                                        | MCP servers                                                                                                                                                                                                                                              | Instruction files                                                                                                                                                           |
| ----------- | ---------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Claude Code | `~/.claude/settings.json`; project `.claude/settings.json`, `settings.local.json`; `hooks.<event>[]` contains handler groups | `~/.claude.json` has `mcpServers` and `projects.<absolute-project-path>.mcpServers`; project `.mcp.json` has `mcpServers`                                                                                                                                | Global `~/.claude/CLAUDE.md`; project `CLAUDE.md`, `CLAUDE.local.md`, `.claude/CLAUDE.md`                                                                                   |
| Codex       | Global and project `.codex/hooks.json` or inline `[hooks]` in `.codex/config.toml`; event arrays contain handler groups      | Global and project `.codex/config.toml`, `[mcp_servers.<name>]`                                                                                                                                                                                          | Global `.codex/AGENTS.md`, `AGENTS.override.md`; project `AGENTS.md`, `AGENTS.override.md`                                                                                  |
| OpenCode    | No declarative hook file in this inventory; plugins can register events in code                                              | Global `~/.config/opencode/opencode.json` or `.jsonc`; project `opencode.json` or `.jsonc` and `.opencode/opencode.json` or `.jsonc`; `mcp.<name>` (V1) and `mcp.servers.<name>` (V2), both in one file; `opencode mcp add` writes V2 next to V1 entries | Global config directory `AGENTS.md`, fallback `~/.claude/CLAUDE.md`; project `AGENTS.md`, `CLAUDE.md`                                                                       |
| Cursor      | Global and project `.cursor/hooks.json`; `hooks.<event>[]` contains handlers                                                 | Global and project `.cursor/mcp.json`; `mcpServers`                                                                                                                                                                                                      | Project `AGENTS.md`                                                                                                                                                         |
| pi          | Events are registered by extensions in code, not a declarative hooks file                                                    | `~/.pi/agent/mcp.json`, project `.pi/mcp.json`; `mcpServers` in current pi                                                                                                                                                                               | Global `~/.pi/agent/AGENTS.override.md`, `AGENTS.md`, `CLAUDE.md`; project equivalents; global agent directory and project `.pi/` also have `SYSTEM.md`, `APPEND_SYSTEM.md` |
| Grok Build  | `~/.grok/hooks/*.json`, project `.grok/hooks/*.json`, inline `[hooks]` in `~/.grok/config.toml`                              | Global and project `.grok/config.toml`, `[mcp_servers.<name>]`                                                                                                                                                                                           | Global `~/.grok/` and project root: `AGENTS.md`, `AGENT.md`, `CLAUDE.md`, `CLAUDE.local.md`                                                                                 |

Grok also reads Claude and Cursor rule directories for compatibility. Rule
directories are outside this inventory. Grok instruction filenames in the table
are listed under Grok at both scopes. Older pi versions did
not include native MCP support. Case-only filename variants use the same lookup
on the default macOS filesystem. The inventory uses the usual `.md` spelling.

Sources: [Claude settings](https://code.claude.com/docs/en/settings),
[Claude MCP](https://code.claude.com/docs/en/mcp),
[Claude memory](https://code.claude.com/docs/en/memory),
[Codex hooks](https://developers.openai.com/codex/hooks),
[Codex MCP](https://developers.openai.com/codex/mcp),
[Codex instructions](https://developers.openai.com/codex/guides/agents-md),
[OpenCode config](https://opencode.ai/docs/config/),
[OpenCode rules](https://opencode.ai/v1/docs/rules/),
[Cursor hooks](https://cursor.com/docs/hooks),
[Cursor MCP](https://cursor.com/docs/mcp),
[Cursor rules](https://cursor.com/docs/rules),
[pi MCP](https://pi.dev/docs/latest/mcp),
[pi configuration](https://pi.dev/docs/latest/configuration),
[Grok settings](https://docs.x.ai/build/settings),
[Grok project rules](https://docs.x.ai/build/features/project-rules#discovery),
[Grok MCP](https://docs.x.ai/build/features/mcp-servers),
[Grok hook source](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/docs/user-guide/10-hooks.md).

### Agent home overrides

The host resolves these roots for scan, extras, and skill operations; project paths do not move. Explicit `--home` and fixture scopes use defaults under the selected home and ignore these variables.

| Agent       | Variable and meaning                                                                                                                                                                           | Source checked on 2026-11-11                                                                                                                                                                     |
| ----------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Claude Code | Non-empty `CLAUDE_CONFIG_DIR` replaces `~/.claude`. Skills, settings, plugins and sessions move with it. The global MCP file moves from `~/.claude.json` to `$CLAUDE_CONFIG_DIR/.claude.json`. | Installed `~/.local/share/claude/versions/2.1.319`, embedded `Ue`, `G` and `PFr` path functions.                                                                                                 |
| pi          | Non-empty `PI_CODING_AGENT_DIR` replaces `~/.pi/agent`, including skills, settings and sessions. A leading `~/` expands to the home directory.                                                 | Installed `~/.volta/tools/image/packages/@earendil-works/pi-coding-agent/lib/node_modules/@earendil-works/pi-coding-agent/dist/config.js`, `getAgentDir` and `expandTildePath`.                  |
| Grok Build  | Non-empty `GROK_HOME` replaces `~/.grok`, including skills, hooks, config, plugins and sessions. The value is a directory path used as given.                                                  | Grok is not installed on this machine. Checked upstream `xai-org/grok-build`, `crates/codegen/xai-dirs/src/lib.rs`, `resolve_grok_home_from`, and `crates/codegen/xai-grok-config/src/paths.rs`. |

On macOS, the desktop app started from Finder does not receive shell environment variables.

## skills CLI pack contract

`crates/skill-studio-core/tests/skills_cli_contract.rs` records the pinned
`npx skills` pack behavior for global installs, updates, removal, project
restore at a tag, and npm sync. It uses the offline smart-HTTP git fixture in
`tests/support/git_pack.rs`. Upstream changes overwrite local edits; unchanged
skills keep edits, and update does not add new upstream skills. Adding with one
selected agent defaults to copies in 1.8.1; restore and update can create links.
The tests set the host-agent identity to Claude so updates target the same agent
when run from Codex or another agent.

The tests are ignored by default because fetching the CLI needs npm registry
access. Run `cargo test --workspace --all-features -- --ignored skills_cli_contract`.
The non-required `skills-cli-contract` workflow runs on pull requests and weekly.
It also checks the app's universal/pi flags and verifies that every folder
created by an all-agent install is covered by the removal backup table.
skills 1.8.1 places pi, Codex, and fx in the shared folder; legacy per-agent
folders remain readable by Skill Studio but are not cleared by the CLI.
