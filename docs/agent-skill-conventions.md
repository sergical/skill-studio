# Agent skill conventions

Reference for how each first-class agent discovers, invokes, and controls skills,
and what the agentskills.io spec requires. Verified against the linked docs on
2026-08-22. Re-verify a row before relying on it if the agent shipped a major
release since then.

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

Claude Code, Codex, OpenCode and pi auto-invoke a skill by default when its
description matches the task. Grok Build's model-side control is not verified
yet (see the "unknown" cell below). [Skill uses](#skill-uses) lists
how each harness records a typed command and a model call.

| Agent       | Explicit invocation | Restrict model auto-invoke                                                                                       | Agent setting that hides a skill (Skill Studio reads it)                         |
| ----------- | ------------------- | ---------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| Claude Code | `/name [args]`      | frontmatter `disable-model-invocation: true` (user only); `user-invocable: false` (model only)                   | `~/.claude/settings.json` → `skillOverrides: {"<name>": "off"}`                  |
| Codex       | `$name`, `/skills`  | sidecar `agents/openai.yaml` → `policy.allow_implicit_invocation: false`                                         | `~/.codex/config.toml` → `[[skills.config]] path = "…/SKILL.md" enabled = false` |
| OpenCode    | `/name`             | `disable-model-invocation: true`; `metadata["opencode/autoinvoke"]` overrides it (boolean or number 1/0, v2.2.6) | `permission.skill` in `opencode.json`: `"name": "deny"` (wildcards allowed)      |
| pi          | `/skill:name`       | frontmatter `disable-model-invocation: true`                                                                     | none                                                                             |
| Cursor      | `/name`             | frontmatter `disable-model-invocation: true`                                                                     | none                                                                             |
| Grok Build  | `/name`             | unknown                                                                                                          | none                                                                             |

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
- **pi, Cursor, Grok Build**: no per-skill setting. Park is the only off.

A skill a setting hides stays hidden after unpark: edit the file to change it.
Older builds also wrote these files and, for Claude Code, removed the per-skill
link. Scans still report those leftovers (`disabled_by`), and the journal still
loads their events.

## Discovery paths

| Agent       | Project                               | Global                                         | Notes                                                                                                                                                                              |
| ----------- | ------------------------------------- | ---------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Claude Code | `.claude/skills/`                     | `~/.claude/skills/`                            | also plugin cache `~/.claude/plugins/cache`; nested `.claude/skills/` in subdirs                                                                                                   |
| Codex       | `.codex/skills/`                      | `~/.codex/skills/`                             | also plugin cache `~/.codex/plugins/cache`                                                                                                                                         |
| OpenCode    | `.opencode/skills/` (legacy `skill/`) | `~/.config/opencode/skills/` (legacy `skill/`) | walks up to the git worktree root                                                                                                                                                  |
| pi          | `.pi/skills/`                         | `~/.pi/agent/skills/`                          | root `.md` files with valid frontmatter count too                                                                                                                                  |
| Cursor      | `.cursor/skills/`                     | `~/.cursor/skills/`                            | also reads .agents, .claude and .codex skill dirs; plugins in ~/.cursor/plugins/{cache,local}                                                                                      |
| Grok Build  | `.grok/skills/`                       | `~/.grok/skills/`                              | also reads .agents and .claude skill dirs; project .grok/skills walks up to the repo root; plugins in ~/.grok/plugins, marketplaces in ~/.grok/config.toml [[marketplace.sources]] |
| Universal   | `.agents/skills/`                     | `~/.agents/skills/`                            | Canonical deployment. Codex, OpenCode, pi, Cursor and Grok Build read it directly. Claude Code needs a symlink.                                                                    |
| parked      | n/a (global only)                     | `~/.agents/skills-parked/`                     | Skill Studio's own root for parked (disabled globally) skills - see "Parking" above; excluded from coverage                                                                        |

## Local data sources Skill Studio reads

| Purpose                            | Location                                                                                           | Shape                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| ---------------------------------- | -------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Installed-skill lock               | `~/.agents/.skill-lock.json`                                                                       | `{version, skills: {name: {source, sourceType, sourceUrl, skillFolderHash, installedAt, updatedAt}}}`                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| dotagents declared skills          | `~/.agents/agents.toml`                                                                            | `[[skills]]` rows: `{name, source, path, ref?}` - `ref` absent means unpinned; no `[[skills]]` row for a lock entry means a wildcard (`--all`) install                                                                                                                                                                                                                                                                                                                                                                                                                   |
| dotagents resolved skills          | `~/.agents/agents.lock`                                                                            | `[skills.<name>]` tables: `{source, resolved_path, resolved_commit}` - the commit actually on disk                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| Fork registry (Skill Studio-owned) | `~/.agents/skill-studio.json`                                                                      | `{version, forks: {name: {forked_at, origin_tool, origin_source, repo, path, declared_ref, base_commit}}, parked: {name: {parked_at, source_kind, claude_link?}}, harness_disabled: {name: {"claude-code": {link_target}}}, projects: {added: [path], excluded: [path]}, discovery: {harness: false}}` - a harness id mapped to `false` switches its project history off for discovery in the desktop, CLI, and MCP server; a missing key means on. Trials were removed in #285; a leftover `trials` map from before that round-trips unchanged and is otherwise ignored |
| Skill uses                         | see [Skill uses](#skill-uses)                                                                      | one reader per harness                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| Projects list (Codex)              | `~/.codex/config.toml`                                                                             | `[projects."/abs/path"]` sections                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| Projects list (Claude Code)        | `~/.claude/projects/<encoded-path>/`                                                               | `cwd` field inside the transcripts (dir name encoding is lossy)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| Projects list (pi)                 | `~/.pi/agent/sessions/--<encoded-cwd>--/*.jsonl`                                                   | `cwd` in the first (`"type":"session"`) record (dir name encoding is lossy: `/ \ :` all become `-`)                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| Projects list (Cursor)             | `<user data dir>/Cursor/User/workspaceStorage/<hash>/workspace.json`                               | `{folder: "file:///abs/path"}`; `workspace` (multi-root) and `vscode-remote://` entries name no local folder. User data dir: `~/Library/Application Support` (macOS), `~/.config` (Linux), `%APPDATA%` (Windows). Not documented by Cursor                                                                                                                                                                                                                                                                                                                               |
| Projects list (OpenCode)           | `~/.local/share/opencode/opencode.db`, `opencode-<channel>.db`; legacy `storage/project/<id>.json` | `project.worktree` (SQLite, WAL mode) and the legacy `worktree` key. Channels `latest`, `beta` and `prod` use `opencode.db`; any other channel (`next` = v2 beta, `local` = source build) uses `opencode-<channel>.db`. Sessions outside a project have worktree `/`. Open with `mode=ro` only while both `-wal` and `-shm` exist, else `mode=ro&immutable=1`: a plain read-only open creates the missing files. `$XDG_DATA_HOME` and `OPENCODE_DB` move the files; Skill Studio reads only the default location                                                         |
| Projects list (Grok Build)         | `~/.grok/sessions/<encoded-cwd>/<session-id>/`                                                     | Folder name is the cwd percent-encoded (only `A-Z a-z 0-9 - _ . ~` kept, so `/` is `%2F`) while that fits in 255 bytes. A longer cwd gets `<slug>-<blake3 hex16>` and a `.cwd` file with the path; a slug never decodes to an absolute path. Source: `encode_cwd_dirname` / `decode_cwd_from_dirname` in xai-org/grok-build `crates/codegen/xai-grok-config/src/paths.rs`. `$GROK_HOME` moves the store; Skill Studio reads only `~/.grok`                                                                                                                               |
| skills.sh search                   | `https://skills.sh/api/v1/skills/search`                                                           | `{data: [{id, name, installs, source}]}` (`source`, not `topSource`)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |

### Skill uses

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

CLI `usage` and MCP `skill_usage` report `recent_uses` for the requested day
window, plus `total_uses`, `sessions`, `last_used`, and `triggers` across all
selected history. Sessions are distinct `(harness, session id)` pairs. Records
without a session id do not add a session. Trigger keys are `user`, `agent`,
`file_read`, and `unknown`. The current readers emit only triggers with direct
evidence, so `unknown` is zero. A mention of a skill is not evidence of use.
The table above states which signals each reader accepts. In particular, pi
has no agent skill tool, and Cursor supplies only file reads, with file mtime
as the use time.

Codex history includes streaming reads of `.jsonl.zst` files in both `sessions`
and `archived_sessions`. `$CODEX_HOME` moves both the reader and its live watch.
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

Verified on 2026-11-11 against installed Claude Code 2.1.319 and Codex CLI
0.167.0. `skill-studio doctor --json` and the MCP `doctor` tool return
`listing_cost` for each harness and session scope. Project rows include global
skills. Codex and Claude keep same-name skills from different files and remove
duplicate file paths. Parked, hidden, disabled-plugin, and manual-only skills
do not enter the count. Codex also rejects missing or blank descriptions.
The five largest entries help the owner choose skills to park or make manual-only.

Claude's installed executable at `~/.local/share/claude/versions/2.1.319`
contains its JavaScript. `dOt` computes the character budget as context tokens
multiplied by four, then by `skillListingBudgetFraction`, default 0.01.
`Z9t` reads `skillListingMaxDescChars`, default 1536. `Y_n` counts the name,
capped description, list punctuation, and newlines. `X_n` keeps names and removes
lower-priority descriptions when the list is over budget. `cOt` adds
`when_to_use` text. `yt` reads merged settings, including user
`~/.claude/settings.json`, project `.claude/settings.json`, and project
`.claude/settings.local.json`. Managed settings and session flags can also apply.
The skill directory loader removes duplicate real file paths. It keeps name
collisions and calls `t0t` with `resolves: false`. Plugin skill loading also
removes duplicate real paths. Disabled plugins do not supply skills.
The listing call uses `Xh(mainLoopModel, jp())` for the active context size and
`E_(mainLoopModel)` for the character ratio. If no context is supplied, `dOt`
uses 200000 tokens. An environment character-budget override and experiments
can change the live budget.

Codex's installed package identifies version 0.167.0. Its binary contains
`SkillsConfig.max_context_tokens`, `SkillMetadataBudget`, and the bounded-list
warnings from `ext/skills/src/render.rs`. The matching
[tagged renderer source](https://github.com/openai/codex/blob/rust-v0.167.0/codex-rs/ext/skills/src/render.rs)
sets the budget to 2% of the supplied context window. Without one, it uses 8000
characters. An explicit `skills.max_context_tokens` overrides this and is capped
at 10000 tokens. The renderer caps descriptions at 1024 characters. It counts
names, descriptions, locators, line syntax, and any root-alias overhead. It uses
UTF-8 bytes divided by four for token estimates. It shortens descriptions, then
omits entries if even their names and locators do not fit. Extension listings
can use `interface.short_description` from `agents/openai.yaml`. The
[config type](https://github.com/openai/codex/blob/rust-v0.167.0/codex-rs/config/src/skills_config.rs)
loads through Codex's config layers, which can include user, project, and session
settings. The [host merge](https://github.com/openai/codex/blob/rust-v0.167.0/codex-rs/ext/skills/src/loader/host_merge.rs)
uses document paths as identity. Each distinct file keeps its own locator.
The [parser](https://github.com/openai/codex/blob/rust-v0.167.0/codex-rs/skills/src/parser.rs)
rejects descriptions that are empty after whitespace normalization.
`[[skills.config]] enabled = false` selects a document by canonical path in
any root, including Universal. Other readers of that file stay enabled.
The [config loader](https://github.com/openai/codex/blob/rust-v0.167.0/codex-rs/config/src/loader/mod.rs)
loads project layers from the project root to the working directory. It checks
user `projects.<path>.trust_level` records for the directory, then the project
root, then the repository trust root. Untrusted project layers do not apply.
The active model supplies the context size; `model_context_window`
can override it.

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
roots, system config, and session overrides are not resolved. Without a session
context, it reports Claude's 200000
token default or Codex's 8000-character fallback as 2000 estimated tokens. The
`limit_source` and `uses_default_context_window` fields state the fallback.
An unknown limit is null with source `no known limit`. OpenCode, pi, Cursor, and
Grok Build currently have no known listing limit. `near_limit` starts at 80%;
`over_limit` means the estimate exceeds the budget. These are warning findings.

Manual-only checks are per harness. Claude, pi, and Cursor read
`disable-model-invocation: true`; Codex reads
`policy.allow_implicit_invocation: false` in `agents/openai.yaml`; OpenCode reads
`metadata["opencode/autoinvoke"]: false`. A restriction for one harness does not
hide a skill from another harness that does not support that field.

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

| Agent       | Hooks                                                                                                                        | MCP servers                                                                                                                                                                          | Instruction files                                                                                                                                                           |
| ----------- | ---------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Claude Code | `~/.claude/settings.json`; project `.claude/settings.json`, `settings.local.json`; `hooks.<event>[]` contains handler groups | `~/.claude.json` has `mcpServers` and `projects.<absolute-project-path>.mcpServers`; project `.mcp.json` has `mcpServers`                                                            | Global `~/.claude/CLAUDE.md`; project `CLAUDE.md`, `CLAUDE.local.md`, `.claude/CLAUDE.md`                                                                                   |
| Codex       | Global and project `.codex/hooks.json` or inline `[hooks]` in `.codex/config.toml`; event arrays contain handler groups      | Global and project `.codex/config.toml`, `[mcp_servers.<name>]`                                                                                                                      | Global `.codex/AGENTS.md`, `AGENTS.override.md`; project `AGENTS.md`, `AGENTS.override.md`                                                                                  |
| OpenCode    | No declarative hook file in this inventory; plugins can register events in code                                              | Global `~/.config/opencode/opencode.json` or `.jsonc`; project `opencode.json` or `.jsonc` and `.opencode/opencode.json` or `.jsonc`; `mcp.<name>` (V1) or `mcp.servers.<name>` (V2) | Global config directory `AGENTS.md`, fallback `~/.claude/CLAUDE.md`; project `AGENTS.md`, `CLAUDE.md`                                                                       |
| Cursor      | Global and project `.cursor/hooks.json`; `hooks.<event>[]` contains handlers                                                 | Global and project `.cursor/mcp.json`; `mcpServers`                                                                                                                                  | Project `AGENTS.md`                                                                                                                                                         |
| pi          | Events are registered by extensions in code, not a declarative hooks file                                                    | `~/.pi/agent/mcp.json`, project `.pi/mcp.json`; `mcpServers` in current pi                                                                                                           | Global `~/.pi/agent/AGENTS.override.md`, `AGENTS.md`, `CLAUDE.md`; project equivalents; global agent directory and project `.pi/` also have `SYSTEM.md`, `APPEND_SYSTEM.md` |
| Grok Build  | `~/.grok/hooks/*.json`, project `.grok/hooks/*.json`, inline `[hooks]` in `~/.grok/config.toml`                              | Global and project `.grok/config.toml`, `[mcp_servers.<name>]`                                                                                                                       | Global `~/.grok/` and project root: `AGENTS.md`, `AGENT.md`, `CLAUDE.md`, `CLAUDE.local.md`                                                                                 |

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
