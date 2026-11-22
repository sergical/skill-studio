# skill-studio-mcp

The local MCP server uses stdio. Each call builds a fresh core runtime and
resolves its scope again. The server keeps no conversation state. It keeps
a shared skill usage index in memory and loads it on the first usage call.

## Tools

The [MCP contract snapshot](../../.gate/snapshots/mcp.snap) lists every tool
published by default and its input schema:

```text
add  apply_frontmatter_repair  diagnose  diagnose_conflict
doctor  extras  fix  invocation
invocation_mismatches  library  list_events  listing
outdated  pack_add  pack_export  pack_list
pack_remove  pack_show  pack_switch_source  pack_update
park  preview_frontmatter_repair  recovery_drop  recovery_list
remove  restore_event  review_clear  review_due
review_set  scan  skill_sessions  skill_usage
split  turn_off_check  turn_off_for_agent  unpark
update  update_all
```

Set `SKILL_STUDIO_MCP_DEV_TOOLS=1` to also publish `capabilities`,
`harnesses`, `sweep_quarantine` and `install_preferences`.
See the [tool naming rules](../../docs/spec-core-primitives.md#82-mcp-tools)
and the [flow map](../../docs/flows.md) for CLI equivalents. The server
returns a result envelope in `structuredContent`. It sets `isError`
when the envelope has `status: "error"`.

The text block carries the same JSON, as the MCP spec recommends for
clients that read only `content`. Each client gives its model a different
part of the result. Measured on 2026-11-16 with a probe server whose two
channels held different words, and for Gemini CLI read from its source
(`packages/core/src/tools/mcp-tool.ts`):

| Client              | Success: the model sees         | Error: the model sees |
| ------------------- | ------------------------------- | --------------------- |
| Claude Code 2.1.326 | `structured_content` only       | text block only       |
| Codex 0.169.0       | the whole result, both channels | the whole result      |
| OpenCode 2.3.0      | `structured_content` only       | not measured          |
| Gemini CLI (main)   | text block only                 | the whole result      |

So Codex reads each answer twice, and Gemini CLI loses the data if the text
block does not hold the JSON. Keep both copies. To make Codex read less,
make the envelope smaller; do not shorten the text block or branch on the
client name.

The server gives write calls, `list_events` and `outdated` access to
SQLite history. Core operations use the same lease rules as the CLI and
desktop app. A call that cannot acquire its lease in time returns
`scope_busy`.

## Bounded answers

A read tool may return a `page` with an opaque `next_cursor`. Copy the
cursor exactly and call the same tool with the same scope and filters.
The cursor continues its own list. If the cursor is invalid or that
list has changed, the tool returns `invalid_request` and asks you to start
again without a cursor.

## Progress

A call whose `_meta.progressToken` is set receives at least one
`notifications/progress` message (one at the start, one at completion) over
the call's `peer`. A call with no progress token receives none. The core
does not currently emit granular per-op progress; this is start/done
bracketing around the call, not a step counter.

## Environment variables

| Variable                     | Effect                                                                                                                                                    |
| ---------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `SKILL_STUDIO_FIXTURE`       | Read-only fixture. Looks for existing leases under `<dir>/.skill-studio/leases`. Takes precedence over `SKILL_STUDIO_HOME`.                               |
| `SKILL_STUDIO_HOME`          | Alternate live home. History and leases are under `<dir>/.skill-studio/`.                                                                                 |
| `SKILL_STUDIO_PROJECT`       | Explicit project directories. Use the platform path-list separator (`:` on Unix, `;` on Windows).                                                         |
| `XDG_DATA_HOME`              | With the default home, a nonempty value puts history and leases under `$XDG_DATA_HOME/skill-studio`. Otherwise the root is `~/.local/share/skill-studio`. |
| `SKILL_STUDIO_MCP_DEV_TOOLS` | Set to `1` to include the developer tools in `tools/list`.                                                                                                |
| `SKILL_STUDIO_TELEMETRY`     | Set to `0` or `false` to turn telemetry off. Other values leave the host's saved setting in effect.                                                       |

## Example client configuration

Users run the published server through the CLI package:
`npx -y skill-studio@latest mcp` (see `packages/npm/skill-studio/README.md` for
each agent's setup). For a local build, point the client at the binary:

```json
{
  "mcpServers": {
    "skill-studio": {
      "command": "/path/to/checkout/target/debug/skill-studio",
      "args": ["mcp"],
      "env": {
        "SKILL_STUDIO_HOME": "/Users/you"
      }
    }
  }
}
```

Use `SKILL_STUDIO_FIXTURE` to test against a fixture tree. The server
refuses write calls in this scope with `invalid_scope`.

## Testing

The [stdio integration suite](tests/mcp_server.rs) starts the real server.
It checks scope isolation, fixture write refusal, changes between calls,
progress notifications, tool schemas, history compatibility and agreement
with CLI results. It also checks the effects of park, unpark, remove and
restore.

## Read examples

The read-only `extras` tool takes `{}` and uses the server's existing home and
project scope. It returns the same declaration list as `skill-studio extras
--json`: hook events and counts, MCP server names, and instruction file paths.
It returns no config values or instruction contents.

The read-only `listing` tool returns, for each agent and scope, the state of
each skill in the model's skill list: `described`, `truncated`, `name_only`,
`omitted` or `hidden`. Optional `claude_budget_fraction`,
`claude_context_window_tokens` and `codex_max_context_tokens` predict another
budget. `claude_transcript` compares with one Claude Code session and
`codex_transcript` with one Codex rollout. Codex rows add `thresholds` (tier,
full and names-only costs, the distance to the next tier, the cap) and
`invocation_mismatch` on skills Claude Code keeps manual-only that Codex
lists. It writes no agent settings.

The read-only `skill_sessions` tool reads each agent's newest sessions (100 by
default, `sessions` up to 500) and returns three lists: `skills`, one row per
agent and skill with counts of sessions that listed it `described`,
`truncated` or `name_only`, used it, or possibly missed it, plus `flags` and
the reason it is not described now; `sessions`, each session's listing totals,
uses and possible misses; and `agents`, what each agent records. Flagged skills
come first, so the first page holds the findings. Possible misses are a guess.
