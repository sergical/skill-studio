# skill-studio-mcp

A stateless local MCP server over `skill-studio-core`, stdio transport. Each
tool call builds a fresh `RuntimeScope` and `Ports`, re-reads disk, runs one
core operation, and drops everything — restarting the process between two
calls gives identical results (see `apps/mcp/tests/mcp_server.rs`,
`restart_gives_the_same_envelope_as_the_first_run`). No sessions, no
subscriptions, no cache between calls (MCP revision 2026-07-28, decision
D13).

## Tools

One tool per core operation. The `#[tool]` methods in `src/lib.rs` are the
tool list. By default, `tools/list` leaves out the four tools in
`DEV_TOOLS` (`capabilities`, `harnesses`, `sweep_quarantine`,
`install_preferences`); set `SKILL_STUDIO_MCP_DEV_TOOLS=1` to publish them.
`docs/flows.md` maps each tool to its user flow, its CLI command and its
parity test. Every tool's input schema is the core's own
request DTO's `schemars` output, unchanged; every tool's output is the same
`ResultEnvelope` JSON `skill-studio` (the CLI) prints, as
`structured_content`, with `is_error` set to match
`envelope.status == "error"`.

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

Tools that write, or that open the history store (`list_events`,
`restore_event`), take the same file lease as the CLI and the desktop app.
If another process holds it, the call returns the ordinary `scope_busy`
error envelope (bounded wait, never an indefinite hang).

A core error is never a panic and never a bare string: it comes back as
`structured_content` carrying the envelope's `errors` array, with
`is_error: true`.

## Progress

A call whose `_meta.progressToken` is set receives at least one
`notifications/progress` message (one at the start, one at completion) over
the call's `peer`. A call with no progress token receives none. The core
does not currently emit granular per-op progress; this is start/done
bracketing around the call, not a step counter.

## Environment variables

Scope resolution follows the scope, never the process: an explicit home or
fixture derives its own history root and lease root from itself; only the
true default (neither variable set) resolves against the real machine. The
core itself never reads the environment — `apps/mcp/src/scope.rs` is where
that happens, mirroring `apps/cli/src/scope.rs`'s flag-driven version.

| Variable               | Effect                                                                                                               |
| ---------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `SKILL_STUDIO_FIXTURE` | Fixture scope rooted at this directory. Lease root: `<dir>/.skill-studio/leases`.                                    |
| `SKILL_STUDIO_HOME`    | Live scope rooted at this directory instead of the real home. Lease root: `<dir>/.skill-studio/leases`.              |
| `SKILL_STUDIO_PROJECT` | A `PATH`-style list of explicit project directories (`:` on Unix, `;` on Windows). Optional with either scope above. |

Neither variable set: the real host home (`dirs::home_dir()`), with the
ambient `$XDG_DATA_HOME/skill-studio` (or `~/.local/share/skill-studio`)
data root — the same rule `apps/cli/src/scope.rs::data_root` uses.

## Example client configuration

Users run the published server through the CLI package:
`npx -y skill-studio@latest mcp` (see `packages/npm/skill-studio/README.md` for
each agent's setup). For a local build, point the client at the binary:

```json
{
  "mcpServers": {
    "skill-studio": {
      "command": "/path/to/skill-studio-mcp",
      "env": {
        "SKILL_STUDIO_HOME": "/Users/you"
      }
    }
  }
}
```

Point `SKILL_STUDIO_FIXTURE` at a materialized fixture directory instead,
for a read-only sandboxed session against a known tree rather than the real
machine.

## Testing

`apps/mcp/tests/mcp_server.rs` spawns the built binary as a real child
process over stdio (via `rmcp`'s `TokioChildProcess` client, and, for the
one case that client cannot exercise — a call with no progress token at
all — a raw JSON-RPC exchange over the same stdio pipes) and covers:
restart equivalence, statelessness within one process (a file added between
two calls on the same server), error mapping for an invalid scope, progress
notifications with and without a token, that `SKILL_STUDIO_HOME` never
writes outside itself, and that `skill-studio watch` and a fresh MCP `scan`
agree on state after a mutation applied through the CLI.

The read-only `extras` tool takes `{}` and uses the server's existing home and
project scope. It returns the same declaration list as `skill-studio extras
--json`: hook events and counts, MCP server names, and instruction file paths.
It returns no config values or instruction contents.

The read-only `listing` tool returns, for each agent and scope, the state of
each skill in the model's skill list: `described`, `truncated`, `name_only`,
`omitted` or `hidden`. Optional `claude_budget_fraction`,
`claude_context_window_tokens` and `codex_max_context_tokens` predict another
budget. `claude_transcript` compares with one Claude Code session. It writes
no agent settings.
