# skill-studio

The Skill Studio command line: a thin adapter over `skill-studio-core`. It
builds a `RuntimeScope` and a set of ports from flags and the environment,
calls one core operation, and prints the result. It holds no policy of its
own — issue derivation, capability facts, and exit statuses all live in the
core, so the CLI, the desktop app, and the MCP server agree.

## Commands

`skill-studio --help` lists these:

```
invocation  Check invocation across copies, or set every copy of a skill
scan        List every skill installed for your agents
diagnose    Find broken skills and say what is wrong with each
conflicts   Find skills that have different copies in different places
review      Set, clear, or list skill review dates
usage       Show which skills your agents used, and which they never used
park        Turn a skill off for all agents
turn-off    Split a shared skill and park one agent's copy
unpark      Turn a parked skill on again
enable      Same as `unpark`
disable     Same as `park`
remove      Remove a skill
fix         Repair a skill's problems and list the ones it cannot repair
undo        Reverse your last change
events      List your past changes, newest first (alias: history)
restore     Reverse one change from your history
add         Install a skill for one or more agents
outdated    List skills that have an update
update      Update installed skills
mcp         Start the MCP server
extras      List hook events, MCP server names, and instruction file paths
listing     Show which skills each agent lists with a description
doctor      Check skill state and model listing budgets
```

Run `skill-studio <command> --help` for each command's flags.

- `park`, `unpark` and `remove` take a skill name. When the name matches
  more than one copy, the command lists each copy's path and id and exits 2;
  run it again with `--id`. `scan` prints the id of each copy.
- `usage` lists skills not used in the last `--days` (default and maximum
  30, the history Skill Studio counts) first, then the used ones, then a
  summary line.
- `mcp` serves the Skill Studio MCP server over stdio.

`--fixture`, `--timings` and the commands below other than `scan` and
`diagnose` are hidden from `--help`. They stay for tests and
tooling.

```
skill-studio scan         [--home <dir>] [--project <dir>]... [--fixture <dir>] [--skill <name>]... [--timings] [--json]
skill-studio diagnose     (same flags)
skill-studio capabilities [--harness <id>]... [--observe] [--tool <name>]... [--json]
skill-studio watch        [--home <dir>] [--fixture <dir>] [--since <revision>] [--json]
skill-studio schema       [--out <dir>]
```

- `scan` inventories every installed skill.
- `diagnose` inventories and derives issues (broken links, spec violations,
  repairable frontmatter, drift, duplicates, parked and disabled skills).
- `capabilities` reports what each harness supports, per the built-in
  catalog; `--observe` also probes the machine (config presence, runner
  binary on `PATH`).
- `watch` polls the scope every 200ms and prints one JSON line per change,
  each carrying the new revision and the full inventory that revision names.
  The watcher already holds that inventory, so it sends it instead of making
  the reader run its own `scan`; a revision-only line would force that scan
  to race the watcher and label newer bytes with an older revision.
  `--since <revision>` suppresses the initial line when the revision already
  matches what the caller last saw. `Ctrl-C` exits 130, the same code a
  cancelled read or write command uses.
- `schema` writes one JSON Schema file per request/result DTO (via
  `schemars`) into `--out` (default `crates/skill-studio-core/schema`).

## Scope flags

- `--fixture <dir>` scans an in-memory-shaped fixture directory instead of
  the real machine (`RuntimeScope::fixture`); its lease root is
  `<dir>/.skill-studio/leases`.
- Without `--fixture`, the scope is `Live`, rooted at `--home` or the host's
  home directory. Its lease root is `<data_root>/leases`, where
  `data_root` is `$XDG_DATA_HOME/skill-studio` or
  `~/.local/share/skill-studio`. The core itself never reads `HOME` or
  calls `dirs`; this crate is where that happens.
- `--project <dir>` (repeatable) adds explicit projects. Without one,
  projects are discovered through `TranscriptProjectDiscovery`.

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
    "next_cursor": "61",
    "also": [
      {
        "list": "/inventory/skills",
        "total": 4002,
        "returned": 58,
        "next_cursor": "/inventory/skills@58"
      }
    ]
  }
}
```

A cursor continues only its own list: `<offset>` for the main list,
`<list>@<offset>` for a list in `also`. The other lists then return only
their totals, so no call repeats a row.

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

| Exit | Meaning                                                                                                                                                    |
| ---- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 0    | `status: "ok"`, no issues (`scan`, or `diagnose` with no issues).                                                                                          |
| 1    | `status: "ok"`, but the result has a problem: `diagnose` (`Warning` or above), `conflicts`, `doctor`, `fix` (unrepaired items) or `update` (errors).       |
| 2    | `status: "error"`, a request- or scope-shaped error (`invalid_request`, `invalid_scope`, `ambiguous_target`, `unsupported`, `execution_failed`, `io`).     |
| 3    | `status: "error"`, a concurrency conflict (`scope_busy`, `stale_proposal`, `drift_conflict`, `ownership_changed`, `already_reverted`, `recovery_pending`). |
| 4    | `status: "partial"` — some scope roots could not be read; see `data.observations`.                                                                         |
| 130  | `status: "error"`, `code: "cancelled"`.                                                                                                                    |

The mapping is exact: it is `ErrorCode::exit_status()` in the core, tested
there and exercised end-to-end in `apps/cli/tests/envelope.rs`.

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
compares with one recorded Claude Code session instead of the newest. The
command writes no agent settings; it prints the setting text and the file to
put it in.

This reads declarations only. It does not show config values or instruction
contents. Unreadable files have an `unreadable` row. Agent trust and enable
settings can prevent a declaration from loading.
