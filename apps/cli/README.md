# skill-studio

The Skill Studio command line: a thin adapter over `skill-studio-core`. It
builds a `RuntimeScope` and a set of ports from flags and the environment,
calls one core operation, and prints the result. It holds no policy of its
own — issue derivation, capability facts, and exit statuses all live in the
core, so the CLI, the desktop app, and the MCP server agree.

## Commands

`skill-studio --help` lists these:

```text
invocation  Check invocation across copies, or set every copy of a skill
scan        List every skill installed for your agents
diagnose    Find broken skills and say what is wrong with each
conflicts   Find skills that have different copies in different places
review      Set, clear, or list skill review dates
library     Show library skills, their sources, uses, invocation modes and agent copies
pack        List, add, update, remove or switch the source of packs: one source that ships skills
recovery    List or drop interrupted writes that recovery cannot undo
usage       Show which skills your agents used in the last 30 days, and which they did not
park        Turn a skill off for all agents. You can turn it on again later
turn-off    Split a shared skill and park one agent's copy. Stops source updates for the copies
unpark      Turn a parked skill on again
enable      Same as `unpark`: turn a parked skill on again
disable     Same as `park`: turn a skill off for all agents
remove      Remove a skill. Copies you made go to a recovery folder
fix         Repair a skill's problems and list the ones it cannot repair
undo        Reverse your last change
events      List your past changes, newest first [alias: history]
restore     Reverse one change from your history
add         Install a skill for one or more agents
outdated    List skills that have an update
update      Update installed skills
mcp         Start the MCP server so an agent can tidy your skills
extras      List hook events, MCP server names, and instruction file paths. Writes nothing
listing     Show which skills each agent lists for the model with a description, with a name only, or not at all. This writes nothing
sessions    Show, per recent session, which skills were listed with a description, by name only, or not at all, which were used, and which were possibly missed. This writes nothing
doctor      Check skill state and model listing budgets. This writes nothing
help        Print this message or the help of the given subcommand(s)
```

Run `skill-studio <command> --help` for each command's flags.

- `park`, `unpark` and `remove` take a skill name. When the name matches
  more than one copy, the command lists each copy's path and id and exits 2;
  run it again with `--id`. `scan` prints the id of each copy.
- `usage` lists skills not used in the last `--days` (default and maximum
  30, the history Skill Studio counts) first, then the used ones, then a
  summary line.
- `mcp` serves the Skill Studio MCP server over stdio.

The [CLI contract snapshot](../../.gate/snapshots/cli.snap) also records
help for internal commands and nested subcommands. For example,
`preview-repair` and `apply-repair` serve the frontmatter repair flow.
`watch` emits one JSON line per inventory change. Use `--since <revision>`
to skip the first line if the revision matches. `schema` writes the core
DTO schemas.

## Scope flags

- `--fixture <dir>` uses a fixture directory and opens existing leases at
  `<dir>/.skill-studio/leases`.
- `--home <dir>` uses that directory as a live home. It keeps history at
  `<dir>/.skill-studio/history` and leases at `<dir>/.skill-studio/leases`.
- With neither flag, the CLI uses the host home. It keeps history and leases
  under `$XDG_DATA_HOME/skill-studio` or, if unset or empty,
  `~/.local/share/skill-studio`.
- Repeat `--project <dir>` to name projects. Live scopes with no explicit
  projects use `HostProjectDiscovery`.

Set `SKILL_STUDIO_TELEMETRY` to `0` or `false` to turn telemetry off.
Other values leave the host's saved setting in effect.

## Output

`--json` prints the `ResultEnvelope` as one compact JSON document with a
trailing newline, and exits with `envelope.exit_status()`. Without
`--json`, it prints a short human table (skills, deployments, issues) and
uses the same exit status. `--json` never prompts and never colors.

### Bounded answers

`scan`, `diagnose`, `library`, `usage`, `outdated`, `invocation`,
`conflicts`, `doctor`, `extras` and `pack list|show` return their main list
in pages. The main list of `doctor` is `violations`; `listing_cost` gets the
space that is left. Without view flags, the
lists keep the rows that fit in a 32 KB answer, `page` included, main list
first. When a list loses rows, the envelope gets a top-level `page`:

```json
{
  "page": {
    "list": "/skills",
    "total": 4002,
    "returned": 61,
    "next_cursor": "<opaque token>",
    "also": [
      {
        "list": "/inventory/skills",
        "total": 4002,
        "returned": 58,
        "next_cursor": "<opaque token>"
      }
    ]
  }
}
```

Copy a `next_cursor` exactly and repeat the same scope and filters. It
continues its own list. The other lists return only totals. If the cursor
is from an older version or the list has changed, the CLI returns
`invalid_request` and asks you to start again without a cursor.

These flags need `--json`. The MCP read tools take the same names
(`limit`, `cursor`, `fields`, `count_by`) as arguments.

| Flag                 | Effect                                                                |
| -------------------- | --------------------------------------------------------------------- |
| `--limit <n>`        | Return at most `n` rows. `--limit 0` returns only the totals.         |
| `--cursor <c>`       | Continue the list of a `next_cursor` from an earlier call.            |
| `--fields a,b`       | Keep only these keys in each row.                                     |
| `--count-by <field>` | Put row counts per value of `field` in `page.counts`, over every row. |

A field that is not a row key, or a cursor that is not a `next_cursor`,
gives `invalid_request` and exit 2. `timings` is in the envelope only with
`--time`; MCP answers do not include it.

`library --mode <agent>=<manual|auto>` (repeatable; MCP `modes`) keeps the
rows that each named agent reads in that mode.

An error — including a scope that fails to normalize, such as
`--fixture /nonexistent` — still prints a full envelope with
`status: "error"` and `data: null` in JSON mode; the CLI never panics
bare. Every invocation carries a fresh ULID `correlation_id`.

### Exit statuses

| Exit | Meaning                                                                                                                                                                                                                                            |
| ---- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 0    | `status: "ok"`, no issues (`scan`, or `diagnose` with no issues).                                                                                                                                                                                  |
| 1    | `status: "ok"`, with findings. Examples include `diagnose` (warning or higher), `conflicts`, `doctor`, and `fix` (unrepaired items or conflicts). Also used when every item in a batch update fails (`status: "error"`, details in `data.errors`). |
| 2    | `status: "error"`, a request- or scope-shaped error (`invalid_request`, `invalid_scope`, `ambiguous_target`, `unsupported`, `backup_expired`, `execution_failed`, `io`).                                                                           |
| 3    | `status: "error"`, a concurrency conflict (`scope_busy`, `stale_proposal`, `drift_conflict`, `ownership_changed`, `already_reverted`, `recovery_pending`).                                                                                         |
| 4    | `status: "partial"` — the result is incomplete, for example an unreadable scope root or a batch that partly succeeded.                                                                                                                             |
| 130  | `status: "error"`, `code: "cancelled"`.                                                                                                                                                                                                            |

See [result status](../../crates/skill-studio-core/src/ops.rs) and
[error codes](../../crates/skill-studio-core/src/error.rs) for the full rules.

List hook events and counts, MCP server names, and instruction file paths:

```sh
skill-studio extras --project /path/to/repo --json
```

Show which skills each agent lists for the model with a description, with a
name only, or not at all, and what changes at a 2% Claude Code budget:

```sh
skill-studio listing --claude-fraction 0.02
```

`--codex-max-tokens N` does the same for Codex. `--claude-session <file.jsonl>`
compares with one recorded Claude Code session instead of the newest, and
`--codex-session <rollout-*.jsonl>` with one Codex session. For Codex the
output also gives the tier, the full and names-only costs, how far the budget
is from the next tier, and skills that Claude Code keeps manual-only but Codex
lists. The
command writes no agent settings; it prints the setting text and the file to
put it in.

Show, for the newest 100 sessions of each agent, which skills the model saw
with a description, by name only, or not at all, which ones each session
used, and which ones it possibly missed:

```sh
skill-studio sessions
```

The text output groups skills by finding: listed in nearly every session but
never used, used while listed by name only, possibly missed (a guess from the
session's first prompt), not described now, and installed but out of the
agent's reach. `--sessions N` reads up to 500 sessions per agent;
`--for-project <path>` reads one project's sessions. Claude Code and Codex
record their listing; the other agents record uses only, and the report says
so.

This reads declarations only. It does not show config values or instruction
contents. Unreadable files have an `unreadable` row. Agent trust and enable
settings can prevent a declaration from loading.
