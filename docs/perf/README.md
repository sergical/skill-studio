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

| Issue                 | User flow                                           | Expectation                                            | Proof                                         | Red before fix?                                          | Measured numbers                        |
| --------------------- | --------------------------------------------------- | ------------------------------------------------------ | --------------------------------------------- | -------------------------------------------------------- | --------------------------------------- |
| Baseline prerequisite | Read skills through core, CLI, MCP, and desktop IPC | Repeatable release samples for all three fixture sizes | `pnpm run perf:baseline`                      | Baseline command and artifacts absent before this change | See baseline JSON and Markdown          |
| #224                  | Scan the owner's home                               | Record actual home scan against 100 ms                 | Owner must run the command against their home | Not run: task safety rule forbids real-home access       | Fixture numbers do not close this issue |

The requested `docs/state-machines.md` is absent at the base commit. The
available lifecycle guidance is `docs/action-map/lifecycle-states.md`.

CLI measurements use a 60-second read budget so large fixtures can complete.
MCP and desktop use their default read budget. The JSON records CLI status,
MCP status and skill counts, and the desktop skill count. Check these fields before comparing
results. A partial scan is not a complete inventory benchmark.

The runner copies each executable out of the shared target folder before it
measures it. Other worktrees can then build without replacing that executable
during the sample run. Timing still includes contention from other processes.

The profile metadata records the copied executable hash, source commit, load average, Cargo process counts before and after, and `DEVELOPER_DIR`. The SVG flame graph is generated from the same sampled stacks as the summary table.
