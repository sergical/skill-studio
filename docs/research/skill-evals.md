# Skill evals: research and recommendation

Research date: 2026-11-11. Status: recommendation, not a plan. Nothing here is
built.

## Question

Should Skill Studio help users write and run evals for their skills? If yes,
what should it measure, and should it build on getsentry/skillet or on
something else?

## Short answer

- **Yes, build it.** The evidence says that most skills do not help, and the
  most common failure is that the agent never loads the skill. Users cannot
  see either problem today.
- **Measure triggering first, then lift.** Triggering asks: does the agent load
  the skill when it should, and stay off when it should not? Lift asks: is the
  result better with the skill than without it?
- **Skillet is not the right core engine.** It measures outcome only, not
  triggering. It drives two of our six agents, and it runs agents with
  permissions bypassed by default. It can be one backend for outcome evals
  later.
- **Our advantage is data that no eval tool has.** We know every skill the user
  has installed, each skill's invocation mode, and real skill use from six
  agents' session logs. The gaps that no existing tool covers are exactly
  these.

## What we did before this

None of this was researched before. We had our own Test run
(`skill_agent_runner.rs`, 2,280 lines, deleted in #544 on 2026-11-11). It ran
one agent once against a skill in a scratch folder, showed the transcript, and
guessed whether the skill loaded. It had no cases, no repeats, no baseline,
and no isolation from the user's global skills. Test was deferred on
2026-09-17 to cut scope. Nobody compared it with other approaches.

## Evidence: do skills help?

| Source                                                                                                                                         | Result                                                                                                                                                                                                                  |
| ---------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Vercel, [AGENTS.md outperforms skills in our agent evals](https://vercel.com/blog/agents-md-outperforms-skills-in-our-agent-evals), 2026-01-27 | Baseline 53%, skill 53%, skill plus an explicit "use it" line 79%, AGENTS.md docs index 100%. "In 56% of eval cases, the skill was never invoked." The post does not name the agent, the model, or the number of evals. |
| [SWE-Skills-Bench](https://arxiv.org/abs/2603.15401), v2 2026-10-27                                                                            | 98 public software skills, 3,920 paired runs. "92 of 98 skills have no statistically significant effect." The average moved from 76.6% to 76.9%.                                                                        |
| [SkillsBench](https://arxiv.org/abs/2602.12670)                                                                                                | Curated skills: 33.9% → 50.5% (+16.6 pp) over 87 tasks. Self-generated skills: −1.3 pp. Large, broad skills scored worse than small, focused ones. Numbers changed between paper versions.                              |
| [Lost in Implicit Routing](https://arxiv.org/abs/2610.18522), 2026-10-13                                                                       | With 1,000 irrelevant skills installed, recall of the right skill fell from 73.6% to 27.4%, and runs that loaded no skill rose from 9.3% to 51.6%.                                                                      |
| [Evaluating AGENTS.md](https://arxiv.org/abs/2602.11988)                                                                                       | Context files do not generally improve success and add over 20% inference cost.                                                                                                                                         |

What this means for us:

1. A skill can be useless or harmful, and its author cannot tell without a
   with/without comparison.
2. Triggering is the main failure, and it gets worse as more skills are
   installed. Our users have many skills: one measured home had 378 (see the
   2026-09-11 estate count).
3. Cost matters. Every skill description takes context on every turn. In
   Claude Code the skill listing gets about 1% of the context window, and the
   descriptions of the least-used skills are dropped when the list is too long
   ([Claude Code skills docs](https://code.claude.com/docs/en/skills)).

## What a skill eval should measure

From the official guidance (Anthropic best practices and skills docs,
agentskills.io, OpenAI's
[eval-skills post](https://developers.openai.com/blog/eval-skills),
Anthropic's
[Demystifying evals for AI agents](https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents)):

| Measure      | How                                                                                                                               | Minimum useful set                                                                                                                                           |
| ------------ | --------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Triggering   | About 20 realistic queries, half should-trigger and half near-miss should-not-trigger. Run each 3 times; a query passes at ≥ 0.5. | 20 queries; 5–10 more held back for a check after tuning ([agentskills.io](https://agentskills.io/skill-creation/optimizing-descriptions))                   |
| Lift         | The same prompt with the skill and without it (or old version vs new). Report the difference, not the with-skill score.           | Start with 2–3 cases, add assertions after the first run ([agentskills.io](https://agentskills.io/skill-creation/evaluating-skills)); Anthropic's floor is 3 |
| Cost         | Tokens, time, and tool calls per run; the description's share of the listing budget                                               | Recorded on every run                                                                                                                                        |
| Regression   | Run the same suite again when the skill, the model, or the agent version changes                                                  | The last results kept as the baseline                                                                                                                        |
| Interference | Triggering measured with the user's real skill set installed, not the skill alone                                                 | Same trigger set                                                                                                                                             |

Grading rules:

- Use deterministic checks first: files, regex, exit codes, the tool trace.
  Use an LLM judge only for what code cannot check. Use a 2-of-3 vote, and
  always link the transcript.
- Remove assertions that pass in both arms: they make the with-skill score
  look better than it is.
- Show uncertainty. With 20 queries × 3 runs, the 95% interval is still about
  ±12 pp, so only large changes are real. Label small samples "inconclusive"
  ([Miller, Adding error bars to evals](https://arxiv.org/abs/2411.00640)).
- Run each trial from a clean state. Trials that share a folder leak into
  each other (Demystifying evals).

## Existing tools

| Tool                                                                                 | Measures                                       | Agents of our six                                      | Baseline                                 | Isolation                                                  | Notes                                                                                                                                                                                        |
| ------------------------------------------------------------------------------------ | ---------------------------------------------- | ------------------------------------------------------ | ---------------------------------------- | ---------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [getsentry/skillet](https://github.com/getsentry/skillet) 1.8.0                      | Outcome                                        | Claude Code, Codex (others through a command template) | `--baseline` (lift)                      | None by default (permissions bypassed); `--sandbox docker` | Own format: `spec.md` + `evals/cases/*.yaml`. `--json`. Uses the user's CLI login. No trigger checks (only `file_exists`, `shell`, `judge`).                                                 |
| [skill-creator](https://github.com/anthropics/skills/tree/main/skills/skill-creator) | Triggering, outcome, cost                      | Claude Code                                            | with / without / old                     | None                                                       | A skill, not a program: the loop is prose the model follows. Its trigger test writes a temporary `.claude/commands/<name>.md` into the real project and tests that, not the installed skill. |
| [`claude plugin eval`](https://code.claude.com/docs/en/plugin-evals)                 | Triggering (`tool_used: Skill`), outcome, cost | Claude Code                                            | `--ablation with-without`, on by default | Temporary HOME and workspace, OS sandbox for Bash          | Best Claude-only runner. Targets a plugin or a skills-directory plugin. `--json`, `--max-cost-usd`, 3 runs by default.                                                                       |
| [promptfoo](https://github.com/promptfoo/promptfoo)                                  | Triggering (`skill-used`), outcome             | Claude Code, Codex, OpenCode (SDKs)                    | Two providers                            | Working folder only                                        | General tool with a skill guide.                                                                                                                                                             |
| [Harbor](https://github.com/harbor-framework/harbor)                                 | Outcome                                        | All six                                                | Run twice                                | Docker                                                     | Benchmark harness; needs Docker and API keys (Cursor, Grok).                                                                                                                                 |
| [Inspect](https://github.com/UKGovernmentBEIS/inspect_ai) + inspect_swe              | Outcome                                        | Claude Code, Codex, OpenCode                           | Do it yourself                           | Docker                                                     | Best statistics (epochs, pass@k). Model API keys.                                                                                                                                            |
| [alibaba/skill-up](https://github.com/alibaba/skill-up) 0.15.0                       | Outcome                                        | Claude Code, Codex (+custom)                           | Yes                                      | Local, Docker                                              | Reads `evals.json`; writes Anthropic-style `grading.json` / `benchmark.json`.                                                                                                                |
| OpenAI platform Evals                                                                | —                                              | —                                                      | —                                        | —                                                          | [Shuts down 2026-11-30](https://developers.openai.com/api/docs/guides/evals). Do not use.                                                                                                    |

Braintrust and LangSmith are scoring and tracking services. They do not run
agent CLIs.

### Case formats

There is no single standard. The closest is the agentskills.io
`evals/evals.json` (`skill_name`, `evals[]` of `id`, `prompt`,
`expected_output`, `files`, `assertions`). skill-creator uses the same shape
but names the field `expectations`. skillet uses YAML cases tied to
`spec.md` behaviors. `claude plugin eval` uses `prompt.md` plus
`graders/*.md`. Harbor uses a Dockerfile plus a test script. None of these
converts into another one.

### Gaps that no tool covers

1. All six agents with the user's own login and no Docker.
2. Triggering measured on the real installed skill for each agent. Reliable
   detection exists only for Claude Code.
3. Triggering measured against the user's other installed skills. This is
   interference, the cause of the 56% and the 27.4% results.
4. Eval cases taken from real sessions.

Skill Studio is positioned for gaps 2–4.

## What we already have

- **Skill-use parsers for all six agents** (`crates/skill-studio-core/src/skill_uses/`).
  Each record has `skill`, `harness`, `trigger` (`user`, `agent`,
  `file_read`), `at`, `project_path`, and `session`. This is the detection
  that a trigger eval needs. The headless probes in
  `docs/research/harness-primitives.md` show what each agent's CLI writes for
  a model-chosen load and for a typed one.
- **Invocation modes** (`ops_invocation.rs`). A "should auto-trigger" case
  means nothing for a skill that is set to manual. The mode decides which
  cases apply.
- **The installed set** for each agent, which is the interference setup.
- **The deleted runner** (parent of `4e344655`). It has command builders and
  stream parsers for `claude -p`, `codex exec --json`, `opencode2 run`, and
  `pi -p`, cancel by process group, cost capture, a scratch-folder layout,
  and a run-history schema.
- **Headless flags for a clean run**, checked on this machine:
  - pi: `--no-skills --skill <path>`, `--no-context-files` (the best
    isolation of the six).
  - claude 2.1.322: `--disable-slash-commands` (no skills, for the baseline
    arm), `--max-budget-usd`, `CLAUDE_CONFIG_DIR`.
  - codex 0.167.1: `--ignore-user-config`, `--ephemeral`, `CODEX_HOME`.
  - opencode 2.2.7: `--standalone`.

## What is missing

- **Prompt and turn link.** `SkillInvocation` has no message or tool-call id.
  The parsers skip the user prompt, so a use cannot become a case.
- **Negatives from real data.** We index only sessions that have a use, so
  "should have triggered but did not" cannot be found.
- **Outcome.** No tool results or final message are read.
- **Clean baselines for OpenCode, Cursor, and Grok.** OpenCode isolation is
  not proven: the old runner found `OPENCODE_CONFIG` ignored. cursor-agent on
  this machine is an old build that needed sign-in. Grok is not installed.
- **Codex trigger detection.** A model-chosen load shows up only as a
  `SKILL.md` file read, not a skill call.

## Recommendation

1. **Phase 1: trigger evals in the core, for every agent that supports it.**
   - Generate about 20 labelled queries (half near-miss) from the skill's
     description and body. The user reviews and edits them.
   - Run each query 3 times through each agent's headless CLI in a temporary
     workspace with a temporary config home.
   - Install the user's real skill set next to the skill under test.
   - Detect loads with the existing skill-use parsers.
   - Stop each run as soon as the skill loads or the agent starts work
     without it, which keeps cost low.
   - Show the trigger rate per query and per agent, with the interval.
   - Show the skill's share of the description budget.
   - Start with Claude Code, Codex, and pi, where detection and isolation are
     proven. Add the others when they are.
2. **Phase 2: lift evals.** Store cases in agentskills.io `evals/evals.json`
   format. Run paired with/without arms. Grade with deterministic checks
   first, then a 2-of-3 judge. Report the difference, the cost difference,
   and "inconclusive" for small samples. For Claude Code, call
   `claude plugin eval` instead of our own runner if it can target one skill
   folder. If a skill already has a skillet `spec.md`, offer `skillet eval`.
3. **Phase 3: cases from real use.** This needs opt-in per case. Add a turn
   link and prompt capture to the parsers, and a session index for
   negatives.
4. **Safety for every phase.**
   - Never run with permissions bypassed against the real HOME.
   - Use a temporary HOME and workspace and pass no host credentials beyond
     the agent's own login.
   - Show the estimated cost (runs × arms × cases) and a hard budget before
     each run.
5. **Keep results.** Save the last results as a regression baseline, and
   offer a rerun when the skill hash, the model, or the agent version
   changes.

## Open questions

- Where do eval cases live? A skill installed through `npx skills` or
  dotagents is overwritten on update. Writing `evals/` into it breaks the
  rule that the tool that owns a file writes it. Choices: the app's data
  folder, the user's fork, or the skill's source repo.
- Can `claude plugin eval` target one standalone skill folder without a
  wrapper plugin?
- Can OpenCode, Cursor, and Grok run with only a chosen skill set? This needs
  a probe for each agent, recorded in `docs/research/harness-primitives.md`.
- What does a default trigger run cost per agent? This needs a measured run
  before we pick defaults.
- Is there user demand? We have no user evidence yet.

## Sources not read in full

The SkillRouter figure (74.0% Hit@1 over about 80K skills,
[arXiv 2603.22455](https://arxiv.org/abs/2603.22455)), the smaller GitHub
tools (`tardigrde/agent-skill-eval`, `adewale/skill-eval-harness`,
`vercel-labs/agent-eval`), and the Harbor and Inspect auth details came from
search summaries or helper reads. They do not change the recommendation.
