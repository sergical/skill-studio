# Skill Assistant: research and delivery contract

Research date: November 9, 2026. Native client help inspected: Claude Code 2.1.319, Codex 0.167.0, OpenCode 2.2.6, pi 1.5.0, Cursor Agent 2025.09.12-4852336. Grok was not on PATH.

## Decision

Use installed clients behind a shared evaluation engine. Save a suite, freeze its inputs, run each case in a fresh workspace and session, retain the evidence, then evaluate the output. Desktop and CLI use the same engine. A completed process is not a passing test.

The existing assistant provides useful streaming parsers and native runners. Its React panel owns run, judge, diff, and record. A closed screen can lose the result. Stored records lack the complete input, skill snapshot, model, and judge evidence. Native flags have drifted. The new engine owns runs independently of the screen.

## Alternatives considered

| Direction                            | Benefit                                                    | Reason for the decision                                                                                      |
| ------------------------------------ | ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| Expand the existing chat panel       | Small initial change                                       | Screen-owned execution cannot support reliable recovery or CLI reuse. Keep Ask/Audit as supporting tools.    |
| Shared engine over installed clients | Tests actual tools, login, permissions, and skill behavior | Selected. A dedicated Rust crate avoids adding asynchronous policy to the synchronous host adapter crate.    |
| Replace clients with a model SDK     | Uniform tools and metrics                                  | Changes the environment under test and may use separate billing. Does not satisfy the request.               |
| Adopt a benchmark framework          | Mature evaluation features                                 | Useful import/export targets. Do not require a proxy, container, or separate runtime for the local workflow. |

## Primary sources

- [Agent Skills evaluation guidance](https://agentskills.io/skill-creation/evaluating-skills): fresh sessions, matched inputs, objective checks before subjective review. Start with a few cases, then add failures observed in use.
- [Anthropic skill-creator](https://github.com/anthropics/skills/blob/main/skills/skill-creator/SKILL.md): candidate/control runs, repeated benchmarks, artifact grading and blind comparison. Its trigger script uses a temporary command shim and early tool calls; this is not a complete task test. Its aggregator can substitute output characters for absent token counts. We keep missing metrics unknown.
- [OpenAI skill evaluation article](https://developers.openai.com/blog/eval-skills): evaluate traces, artifacts and explicit criteria. Some example flags are older than the installed client.
- [OpenAI plugin-eval](https://github.com/openai/plugins/tree/main/plugins/plugin-eval): native Codex, workspaces, verifiers, changed files and usage. Inspected v0.1.0 copies config with authentication and does not provide the required cancellation/deadline contract.
- [Codex noninteractive mode](https://learn.chatgpt.com/docs/non-interactive-mode) and [configuration](https://learn.chatgpt.com/docs/config-file/config-reference): JSONL, output schemas, ephemeral sessions, sandbox and configuration controls. `--ignore-user-config` does not prove all skills are disabled; `--ignore-rules` does not disable AGENTS.md.
- [Claude Code CLI](https://code.claude.com/docs/en/cli-reference): streaming JSON, tool restrictions, supported permission modes. `--bare` disables OAuth/keychain and is unsuitable as a default. Skill disable exists; exclusive candidate discovery needs proof.
- [OpenCode skill permissions](https://opencode.ai/docs/skills/#permissions) and [configuration](https://opencode.ai/docs/config/): v2 permission arrays, wildcard deny then exact allow. Empty skill paths do not disable global discovery.
- Installed pi help/source: `--no-skills --skill <path>` retains the explicit skill and disables discovery; use `--no-extensions --no-mcp --no-prompt-templates`. Tool restrictions alone do not remove MCP tools.
- [Promptfoo Codex provider](https://www.promptfoo.dev/docs/providers/openai-codex-app-server/): possible future adapter; uses a separate app server and experimental protocol.
- [Harbor native agents](https://docs.harborframework.com/agents/pre-integrated-agents), [skills](https://docs.harborframework.com/tasks/skills), [verifiers](https://docs.harborframework.com/tasks/verifier): a useful future container/CI mode, not the same installed local environment.
- [Inspect agent bridge](https://inspect.aisi.org.uk/agent-bridge.html): changes model traffic through a proxy, so it is not the default local-client path.

## Product contract

1. A quick test and a reusable suite use the same flow. Cases store the request, files, expected results, and discovery or explicit loading mode.
2. Variants select a client, model, candidate or previous skill snapshot. No-skill comparison is restricted to supported controls; every result states the isolation limit.
3. Freeze skill and fixture content with hashes. Use a fresh temporary workspace/session per trial. Never execute inside the source project.
4. Save intent before launch, raw streams and normalized events during execution, and results atomically. Navigation cannot cancel a run. Restart marks unfinished runs interrupted, never silently repeats paid work.
5. Separate execution state, skill loading evidence, and check outcome. Missing evidence, malformed judgments, authentication failure, timeout and cancellation never pass.
6. Check output text/JSON, artifacts, tool use, selection, and a configured acceptance command. Commands are explicit suite content and run only after Start. A separate judge can grade semantic criteria with structured evidence. Human review is separate and preserves the machine result.
7. Compare matching immutable inputs. Report counts and the evaluated denominator, infrastructure failures, repetitions and missing metrics. Do not infer causality or significance from a single win.
8. Export portable suite JSON separately from evidence. Imports validate without launching clients or commands.
9. Provide a small example with a successful artifact check and a deliberately failed check. Document one command and a desktop flow that reproduce the evidence.

## Acceptance evidence to collect

- Meaningful engine tests: success text with a wrong file fails; missing client cannot pass; malformed judge needs review; cancel/timeout stop children; reload retains evidence; unfinished jobs recover as interrupted; fixture traversal/symlinks and unknown schema are refused.
- Native client probes with actual installed versions, distinct reported capability and authentication failures.
- A passing and failing end-to-end example, repeated candidate/baseline comparison, cancellation and reopened history.
- Desktop input, progress, result and evidence flow; no fabricated production metrics.
- Type checks, formatting, lint, scoped Rust and React tests, then simplification and an independent diff review.

## Recorded verification

On November 9, 2026, all four installed clients completed the same report-writing case and passed its artifact checks. Pi candidate/control trials repeated twice: candidate passed twice; control preserved the source facts but failed the skill-specific verification-line check twice. A separate pi judge accepted the factual accuracy criterion while the deliberately missing-file check still failed the overall trial. These examples establish the test mechanism, not general skill quality or statistical significance.

The sanitized four-client result is in `apps/desktop/src/dev/harness/skill-lab-recording.ts`; sanitized native stream fixtures are in `crates/skill-studio-eval/tests/fixtures`. Full local run evidence was retained in `/tmp/skill-studio-lab-verification` for this implementation session. See [the verification guide](../../examples/skill-tests/README.md) for repeatable commands and the desktop flow.

These historical screenshots show the initial modal, which was replaced by the [dedicated workspace](2026-11-10-skill-assistant-ui-review.md). The recorded [setup](skill-assistant-evidence/setup.png) and [comparison](skill-assistant-evidence/results.png) screens were checked at a 1280 × 900 viewport with no browser console errors. The UI flow test opens the saved native run and expands the report artifact. Browser preview data is read-only and is separate from native execution.
