# Agent benchmark methodology

## Question

Do coding agents answer questions about a user's skills faster, cheaper and
more correctly when they have Skill Studio than when they read the files on
disk themselves?

We ask three tasks, each a job users do today:

| Task     | Job                                                   | Questions |
| -------- | ----------------------------------------------------- | --------- |
| `usage`  | Which skills ran, how often, and in which agent?      | 4         |
| `update` | A skill pack has a new version. Which copies are old? | 2         |
| `share`  | A teammate wants my setup. Which repos, which files?  | 2         |

## Fixture

`fixture.py` writes a synthetic home folder. Every skill name, project and
session is invented. Nothing comes from a real machine. The generator is
seeded, so every run gets the same content. Only the dates move: each run
builds a fresh fixture with "today" set to the run's start time, so the
45-day window always holds the same uses.

The home folder holds:

- A shared library `~/.agents/skills` with 36 skills. Nine are manual-only in
  Claude Code (`disable-model-invocation: true`). Five have an
  `agents/openai.yaml` with a Codex invocation policy.
- Entries in the library that are not skills: a README, a `.DS_Store`, folders
  with no `SKILL.md`, and `.trash` and `_drafts` folders that hold old copies
  of `SKILL.md` one level down.
- Two skills that only Claude Code has (outside the library), and
  `~/.claude/skills` links to each library skill (Claude Code does not read
  `~/.agents/skills`).
- 106 sessions from five harnesses, each in that harness's real log format:
  Claude Code JSONL, Codex rollout JSONL, the OpenCode SQLite database, pi
  session JSONL and Cursor transcripts.

The logs hold traps that a careful reader must not count as uses:

| Trap                                                                       | Harness            |
| -------------------------------------------------------------------------- | ------------------ |
| Resumed session that copies history                                        | Claude Code        |
| Forked sub-agent rollout that copies the parent history                    | Codex              |
| Skill list in a system or developer message                                | Claude Code, Codex |
| A `<skill>` block quoted in a user message                                 | Codex              |
| A `SKILL.md` that the agent edits, not loads                               | Claude Code        |
| A second read of the same `SKILL.md`, or a read right after the invocation | all                |
| A failed skill tool call                                                   | OpenCode           |
| A literal `/skill:x` in text                                               | pi                 |
| A use on day 44 (in the window) and uses on day 46 and later (out)         | all                |
| A use of a skill that is no longer in the library                          | Claude Code        |

The generator computes answers in memory from the uses it plans, not from
any parser. It never writes truth into the fixture.

## Task 1: skill usage (`usage`)

The prompt (`prompts/task.md`) asks four questions and gives the rules for
what counts as a use:

1. How many skills are in `~/.agents/skills`? (36)
2. How many library skills were used in the last 45 days, by any agent? (22)
3. Which 5 library skills had the most uses, with counts? (20, 15, 12, 10, 8)
4. How many library skills are manual-only in Claude Code but can load
   automatically in Codex? (6)

The agent ends with a JSON block. It also lists its confidence, what slowed it
down, and what tool feature it missed. We use those lists to find product
fixes; they are not scored.

## Tasks 2 and 3: pack update and sharing

`fixture.py --task update` and `--task share` add one overlay on top of the
same home folder. The overlay is the same for both tasks:

- `~/.agents/.skill-lock.json` in the format that `npx skills` writes. It
  names three invented packs: `acme-labs/review-skills` (8 skills),
  `orbit-dev/ops-skills` (10) and `northwind/frontend-skills` (10).
  `acme-labs/review-skills` was updated yesterday, so its library copies have
  a new section.
- Six real copies of skills in single-agent folders (Codex, OpenCode,
  Cursor), made before the update. Four of them are copies of pack skills that
  do not have the new version. One copy is current. One copy is of a skill
  outside the pack.
- One link from pi's folder to the library. A link always has the new
  version.

Traps:

| Trap                                                       | Task     |
| ---------------------------------------------------------- | -------- |
| A skill in the lock file whose folder is only in `.trash`  | both     |
| A current copy next to old copies                          | `update` |
| An old copy of a skill that is not in the updated pack     | `update` |
| A link that looks like a copy                              | `update` |
| Two hand-made skills whose text says "Adapted from" a pack | `share`  |

Questions (`prompts/task_update.md`, `prompts/task_share.md`):

- U1. How many skills from `acme-labs/review-skills` are in the library now?
  (8)
- U2. Which copies outside the library do not have the new version, and in
  which agent's folder? (4 skill and agent pairs)
- S1. Which repos were the library skills installed from, and how many from
  each? (3 repos: 8, 10, 10)
- S2. Which library skills were not installed from a repo? (8 names)

A list answer is correct only when it matches exactly.

## Methods

| Method | Note added to the prompt                                            |
| ------ | ------------------------------------------------------------------- |
| `raw`  | Skill Studio is not available. Find the answers from files on disk. |
| `cli`  | `skill-studio` is on `PATH` and already reads the user's home.      |
| `mcp`  | The Skill Studio MCP server is connected. Use its read-only tools.  |

The CLI and the MCP server are the debug builds from this checkout. In the
`cli` method, a small wrapper on `PATH` adds `--home <fixture>` and pins
`CODEX_HOME`, `XDG_DATA_HOME` and `XDG_CONFIG_HOME` to the fixture. In the
`mcp` method, the server gets the same values as env vars. Only the read-only
MCP tools are allowed, plus `invocation`, which reads when it gets no policy.

Each harness needs one setting so that the agent can call the MCP tools
directly:

| Harness  | Setting                                                                                                                                                                    |
| -------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Codex    | `mcp_servers.skill-studio.tools.invocation.approval_mode="approve"`, because `invocation` has no read-only hint and `codex exec` cancels tool calls that ask for approval. |
| OpenCode | `"codemode": false`, so each tool is a direct tool call.                                                                                                                   |
| pi       | `-e builtin:mcp` and `"exposure": "direct"`. The default exposure puts all tools behind one script tool.                                                                   |

## Harnesses

Each harness runs headless with its own skills, rules, plugins and MCP servers
turned off, so the user's real setup cannot leak in.

| Harness     | How it runs                                                                                                                                                               | Cost                        |
| ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------- |
| Claude Code | `claude -p --output-format stream-json --setting-sources project --strict-mcp-config --disable-slash-commands`, edit and agent tools denied                               | Reported (`total_cost_usd`) |
| Codex       | `codex exec --json --ephemeral --ignore-user-config --ignore-rules --sandbox workspace-write` in an empty folder, with a fresh `CODEX_HOME` that holds only the auth file | Estimated from tokens       |
| OpenCode    | `opencode run --format json` with a fresh `HOME` and XDG folders that hold only the auth file; skills and edits denied                                                    | Estimated from tokens       |
| pi          | `pi -p --mode json --no-session --no-skills --no-context-files --no-extensions`, a fixed tool list                                                                        | Reported (`usage.cost`)     |

Every agent gets a fresh `HOME` inside its run folder, so a `~` path cannot
reach the user's real skills. Claude Code signs in from the macOS keychain: its
fresh `HOME` holds a link to `~/Library/Keychains` and a `.claude.json` that
keeps only the account fields. pi reads its sign-in through
`PI_CODING_AGENT_DIR`. The runner also sets `PWD` to the run folder: without
it, OpenCode used the inherited `PWD` (the repo checkout) as its shell folder.

Codex and OpenCode do not report cost. We estimate it from the token counts
they report and the OpenRouter list price of the same model
(`results/prices.json`, fetched once and kept with the results). Codex runs on
a ChatGPT plan, so this estimate is the API price of the same tokens, not
what the run cost the user. Results mark these costs as estimates.

Cursor and Grok Build are not in this run. See [Limits](#limits).

## Safety

- The fixture is read-only (`chmod a-w`), except Skill Studio's own state
  folder `~/.skill-studio`, which it needs for its lease file.
- After each run, the runner hashes every path, mode, size, mtime and file in
  the fixture and compares it with the hash taken at build time. A run that
  changed the fixture is kept in the results file but left out of the scores.
- Each agent runs in an empty working folder.
- The answers stay in the runner's memory. No answer file is on disk while
  an agent runs.
- A fresh `HOME` is not a sandbox: an agent with a shell can still read the
  real home by its full path. A run whose output names any path in the real
  home, except the folders the agent itself is installed in, is marked
  `read the real home` and left out of the scores. Each run keeps its full
  output in `<workdir>/<run id>/stdout.jsonl`, so the check can run again.
- The fixture hash includes the target of each symlink.

## Scoring

- Q1, Q2 and Q4 must be exact.
- Q3 is correct only when all five skills and all five counts are right. We
  also keep two partial scores: names (how many of the five skills are in the
  answer) and counts (how many counts are exact). The question also asks
  which agents used each skill. The answer format does not collect them, so
  the score does not check them.
- Numbers that are not whole numbers are wrong. A part of an answer in the
  wrong shape is wrong; the other parts still count.
- "Correct" for a run is the number of questions that are fully right, out of 4. An answer with no JSON block scores 0.

## Metrics

Per run: correct answers, wall time, cost, tokens, tool calls and turns.

Per cell (harness × model × method), from 3 runs: the median, the minimum and
the maximum of each metric, the share of runs that got each question right,
and answer agreement (the share of runs that gave the most common answer).

Per method: the median of the cell medians.

Only scored runs (no error, fixture intact) go into the medians. If a run
ends in an error and `--retry-errors` runs it again, the report keeps the
last attempt only, and the total cost counts every attempt. `bench.py report`
refuses to write the homepage file while a matrix run has no scored result,
unless you pass `--partial`.

Each cell also reports `scored`, the number of scored runs.

The homepage shows bars. For each harness and model, it pairs the `raw` cell
with the `mcp` cell. A pair counts only when both cells have at least 3 scored
runs, so both sides cover the same agents and models. The `cli` cell is not
in the bars.

- Time and tool calls: the median over the pairs of each cell's median, for
  each side. The ratio above the bars uses the rounded numbers that the bars
  show. A ratio below 1.25× in either direction shows as "Same".
- Wrong answers: for each side, every question of every scored run in the
  pairs is one answer. The bar is the share of those answers that were wrong.
- The "Every run" table below the bars shows every cell, also `cli` and the
  cells the pairs leave out.

## Results (15 November 2026)

171 runs: 4 agents, 10 models, 3 runs per cell, on Skill Studio from #626.
A "pair" is one agent and model: its `raw` cell and its best Skill Studio
cell (see [Metrics](#metrics)). The ratios are medians over the pairs; the
homepage shows the best pair.

| Task     | Pairs | Time        | Tool calls | Cost       | Correct by hand | Correct with Skill Studio | Slower pairs |
| -------- | ----- | ----------- | ---------- | ---------- | --------------- | ------------------------- | ------------ |
| `usage`  | 10    | 5.8× faster | 4.5× fewer | 4.4× lower | 78%             | 100%                      | 0 of 10      |
| `update` | 4     | 1.5× faster | 1.3× fewer | 1.4× lower | 100%            | 100%                      | 1 of 4       |
| `share`  | 4     | 2.3× faster | 2.5× fewer | 2.4× lower | 100%            | 100%                      | 0 of 4       |

Best pairs on the usage task: pi with Sonnet 5.5 is 10.3× faster (69 s to
6.7 s), Codex with GPT-6.1 Sol needs 7× fewer tool calls (7 to 1), and pi with
Haiku 5.5 costs 17.9× less ($0.117 to $0.0065).

Findings:

- On the usage task, 9 of 30 runs by hand got all 4 answers right. With Skill
  Studio, 64 of 66 runs did (32 of 33 with the CLI, 32 of 33 with MCP). Q3,
  the top five skills with counts, was right in 10 of 30 runs by hand and 65
  of 66 with Skill Studio.
- The `library` answer (#626) gives the whole table in one call, and its
  counts agree with `usage`. Before #626, agents saw counts that did not agree
  and checked the logs by hand; the previous run was only 2.0× faster on this
  task, and less correct than by hand.
- The update and share tasks need only a few file reads, so the gain is
  smaller. On `update`, Claude Code with Sonnet 5.5 is 3% slower with Skill
  Studio, and OpenCode with Sonnet 5.5 costs more.
- OpenCode with Kimi K2.6 by hand hit the 900-second limit in 2 runs and
  changed the fixture in 1. That cell has no result. With MCP it got 4 of 4
  right in 37 s.
- The leak guard stopped 2 runs by hand that read the real home folder. They
  ran again; the report keeps the last attempt.

Spend: $21.98 of model use, every attempt counted (Codex and OpenCode
estimated from tokens).

## Limits

- One fixture. The traps are the ones we know of. A real home folder has more
  shapes. The fixture is a test of reading skill logs, not of general coding.
- Three runs per cell show the spread, not a confidence interval.
- The fixture was built before the Skill Studio fixes it measures (#606,
  #613, #626), and the same team wrote both. The traps come from real log
  shapes, but a fixture can still favour the tool that was tuned on it.
- Codex and OpenCode costs are estimates.
- pi and OpenCode reach Anthropic and Moonshot models through OpenRouter, not
  through the model maker's own API.
- Cursor is not in this run: its CLI on the test machine was signed out and
  out of date. Grok Build is turned off.
- A run that hits the 900-second limit counts as an error and is left out of
  the medians.

## Known Skill Studio issues on this fixture

| Issue                                                                                              | Effect                                                                                     |
| -------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| Read-only commands (`scan`, `usage`, `invocation`) take a write lease in `~/.skill-studio/leases`. | They fail when that folder is not writable, for example under `codex --sandbox read-only`. |
| `--home` does not override `CODEX_HOME`.                                                           | Under Codex, the CLI reads the agent's own Codex folder unless `CODEX_HOME` is pinned.     |
| `scan --json` and `invocation --json` print about 150 KB and 120 KB for 38 skills.                 | Agents spend tokens on long paths before they get to the counts.                           |

## Answer contracts

`bench.py contract` checks each question with one CLI call and one MCP tool
call, without a model. A code extractor reads only the returned output. The
existing scorer checks the answer against truth held in memory. A pass also
needs output of at most 2,048 bytes. The JSON report records the call, size,
seed, byte count and reason. Missing product fields fail the contract.

Sizes 40, 400 and 4,000 add skills and months of sessions with the same traps.
Size 36 keeps the published fixture and truth. CI reports sizes 40 and 400.
`contracts/baseline.json` stores passes by question, size, seed and transport.
`--check` rejects regressions among the entries tested in that run.
