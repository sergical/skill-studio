# Agent benchmark

This benchmark measures whether coding agents answer questions about a user's
skills better with Skill Studio than without it. It runs each agent CLI
headless on a synthetic home folder with a known answer, three ways:

| Method | What the agent gets                                            |
| ------ | -------------------------------------------------------------- |
| `raw`  | Shell and file tools only. It reads the logs on disk.          |
| `cli`  | The same tools, plus `skill-studio` on `PATH`.                 |
| `mcp`  | The same tools, plus the Skill Studio MCP server (read tools). |

Read [METHODOLOGY.md](METHODOLOGY.md) for the fixture, the questions, the
scoring and the limits.

## Run it

You need:

- A debug build from this checkout:
  `cargo build --workspace --all-features`.
- The agent CLIs you want to test, signed in: `claude`, `codex`, `opencode`
  (with an OpenRouter key), `pi` (with an OpenRouter key).
- Python 3.11 or later. The scripts use only the standard library.

```bash
# Run the full matrix (3 runs per cell). Runs already in the results file are skipped.
pnpm run bench:agents

# Or pick cells
python3 tools/agent-bench/bench.py run --harness claude --method mcp --runs 1

# Show what would run
python3 tools/agent-bench/bench.py run --dry-run

# Aggregate results/runs.jsonl into results/results.json and the homepage data file
python3 tools/agent-bench/bench.py report
# Write it even when some cells have no scored run (the published data needs this)
python3 tools/agent-bench/bench.py report --partial
```

If a CLI is not on `PATH`, set `BENCH_<NAME>_BIN`, for example
`BENCH_CODEX_BIN=/path/to/native/codex`.

Each run writes its fixture, its runtime home and the agent's raw output to
`/tmp/skill-studio-bench/<run>/` (change with `--workdir`). Nothing goes to
your real home folder: the fixture is read-only and its hash is checked after
each run. A fresh home is not a sandbox, so a run whose output names a path in
your real home is marked as an error and left out of the scores.

## Files

| File                   | Purpose                                                                   |
| ---------------------- | ------------------------------------------------------------------------- |
| `fixture.py`           | Builds the synthetic home and `ground_truth.json`.                        |
| `prompts/`             | The task prompt and one note per method.                                  |
| `harnesses.py`         | How to start each agent CLI isolated, and how to read its output.         |
| `bench.py`             | Runs the matrix, scores answers, writes the results.                      |
| `matrix.json`          | The harness × model cells and the runs per cell.                          |
| `results/runs.jsonl`   | One record per run (answer, score, time, cost, tokens, tool calls).       |
| `results/results.json` | Median and spread per cell, and per method.                               |
| `results/prices.json`  | OpenRouter list prices used to estimate cost when a CLI does not give it. |

The homepage reads `packages/marketing/src/data/agent-benchmark.json`, which
`bench.py report` writes.

## Check the fixture by hand

```bash
python3 tools/agent-bench/fixture.py --out /tmp/skill-studio-fixture
cat /tmp/skill-studio-fixture/ground_truth.json
target/debug/skill-studio usage --home /tmp/skill-studio-fixture/home --json
```
