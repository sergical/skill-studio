# Performance measurements

The current baseline is provisional. Cargo was idle before each fixture. `baseline.json` records the check and a load snapshot taken during the large fixture. The final reference window follows wave 1. The CLI was built explicitly for this run.

Run `pnpm run perf:baseline` from the repository root. Set `CARGO_TARGET_DIR`
to the shared build folder first. The command builds release executables and
creates temporary homes with 20, 378, and 5,000 skills. It deletes these homes
after the run. It never selects the real home.

Each home uses `bench_estate::estate` and `FixtureBuilder`, seed 1. Two skills
are plugins in the Claude Code and Codex caches. Three shared skills have
invalid YAML. The estate generator places about 63% of skills globally and
places the rest in explicit projects. The JSON records the exact global count.
It includes shared skill links and variable name and description lengths.

The command records at least ten samples and reports the median, nearest-rank
p95, maximum, and raw samples. The JSON contains the commit, dirty state,
CPU, RAM, OS, compiler, build profile, seed, and cache state. The adjacent
Markdown file contains the same summary. Units are in metric names.

`core_scan_fresh_runtime_ms` creates a new runtime before each timed scan.
`core_scan_warm_runtime_ms` reuses one runtime. Neither clears the OS disk
cache. Fixture generation warms disk pages. These results do not claim cold
disk performance. CLI times include process launch and JSON serialization.
MCP times exclude initialize and include request and response serialization
on a persistent stdio connection. History uses an empty history folder.

Peak RSS is measured for each CLI command with the OS `time` tool. It is
process RSS, not combined desktop or webview memory. Desktop payload bytes
come from serializing `Some(SkillSnapshot)` from the real snapshot builder.
They do not measure paint time or UI responsiveness. Paths in generated homes
can change payload sizes by a few bytes between runs.

The CI job uploads JSON and Markdown. It has no duration assertion and cannot
block a PR. Existing required job names are unchanged.

## Verification plan

| Issue                 | User flow                                           | Expectation                                            | Proof                                          | Red before fix?                                          | Measured numbers                        |
| --------------------- | --------------------------------------------------- | ------------------------------------------------------ | ---------------------------------------------- | -------------------------------------------------------- | --------------------------------------- |
| Baseline prerequisite | Read skills through core, CLI, MCP, and desktop IPC | Repeatable release samples for all three fixture sizes | `pnpm run perf:baseline`                       | Baseline command and artifacts absent before this change | See baseline JSON and Markdown          |
| #224                  | Scan the owner's home                               | Record actual home scan against 100 ms                 | Owner must run the command against their home  | Not run: task safety rule forbids real-home access       | Fixture numbers do not close this issue |
| #687                  | Add 5 skills from one pack                          | One `npx skills add` call for the whole set            | `write_spawn_count.rs`; `tools/perf-writes.py` | Yes: 5 calls on `main` before #690                       | 10.9 s → 6.0 s median, 5 → 1 calls      |
| #687                  | Update 5 skills of one pack                         | One `npx skills update` call per scope                 | `write_spawn_count.rs`; `tools/perf-writes.py` | Yes: 5 calls on `main` before #690                       | 3.6 s → 0.84 s median, 5 → 1 calls      |
| Write baseline        | Park and unpark 5 skills                            | No `npx` call                                          | `tools/perf-writes.py`                         | Not applicable                                           | 0.46 s and 0.12 s median, 0 calls       |

Lifecycle transitions are in `docs/state-machines.md`; user flows are in
`docs/flows.md`.

CLI, MCP and desktop scans have no time budget by default, so they read every
root of a large fixture. `read_timeout_ms` limits only the wait for the shared
lease; `scan_budget_ms` (CLI `--scan-budget-ms`) is the only scan time limit,
and the runner does not set it. The JSON records CLI status,
MCP status and skill counts, and every desktop sample's skill count and `scan_partial`. The Markdown summary labels mixed desktop counts or completeness. Check these fields before comparing
results. A partial scan is not a complete inventory benchmark.

The runner copies each executable out of the shared target folder before it
measures it. Other worktrees can then build without replacing that executable
during the sample run. Before measurement, it checks each copy's embedded build
marker against this run's source SHA and unique build token. A mismatch stops
the run. The JSON records each verified marker and executable SHA-256.
Timing still includes contention from other processes.

The committed baseline predates these checks. Its executable identities and
per-sample desktop completeness were not recorded and cannot be recovered.
Its numbers remain historical, provisional measurements. New runs record these
fields and pass `--runs` to the desktop example. Run
`python3 tools/perf-baseline.py --check` to check the runner without a baseline.

The profile metadata records the copied executable hash, source commit, load average, Cargo process counts before and after, and `DEVELOPER_DIR`. The commands in `profile.md` generate the folded stacks and SVG from the same trace as the summary table. These derived files are not committed.

## Write jobs

`perf:baseline` times reads only. `tools/perf-writes.py` times write jobs:
`pack add`, `pack update --all`, `park` and `unpark` of 5 skills from one
pack, through a built CLI and the real `npx skills`. Run it by hand:

```bash
cargo build --workspace --all-features --bins
python3 tools/perf-writes.py --cli target/debug/skill-studio --label "$(git rev-parse --short HEAD)"
```

The pack is a local bare git repo served by `git http-backend` on loopback, so
no number depends on GitHub or the network. Each run uses a new temporary home
and one shared npm cache, warmed before the first sample. A wrapper on `PATH`
counts the `npx` calls of each job. The count is the deterministic part, and
`crates/skill-studio-core/tests/write_spawn_count.rs` checks it on every
commit. The times are for one machine and are not compared in CI.

`npx skills@1.8.1` writes no lock row for a loopback git URL, so the script
writes the rows it writes for a remote git source after `pack add`. Without
them, `pack update` finds no pack.

Measured on 2026-11-19, Apple M3 Max, debug builds, 5 runs, medians: with
`main` before #690, `pack add` 10.9 s (5 `npx` calls) and `pack update` 3.6 s
(5 calls); with #690, 6.0 s (1 call) and 0.84 s (1 call). `park` and `unpark`
take 0.46 s and 0.12 s and run no `npx`. The first `npx` call in a new home
takes about 5 s against the local repo, so most of `pack add` is the skills
CLI's own start, not the fetch.
