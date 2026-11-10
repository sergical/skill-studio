# Installed-skill acceptance results — November 10, 2026

Four distinct installed skills have now been exercised. The writing skill was run twice through the desktop flow. Three additional skills were run through the same evaluation engine's CLI, with two tasks each. The earlier report-writing fixture is counted separately because it was made for testing the runner.

## Results

| Installed skill                | Tasks                                                                  | Result                                                                                                                                                                                                                  | Evidence                                                                                                                                                 |
| ------------------------------ | ---------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `emil-unslop-writing`          | Rewrite a product update; repeat through the new workspace             | Both executions completed and loaded the skill. The first passed four file checks. The second used the default review criterion, so its machine outcome remains needs review; a separate review was accepted and saved. | [File checks](skill-assistant-evidence/2026-11-10-real-writing-skill.json), [workspace run](skill-assistant-evidence/2026-11-10-task-workspace-run.json) |
| `emil-unslop-code`             | Clean invoice totals; stop hiding fetch failures                       | 2/2 tasks passed, including loading evidence, unchanged valid results, input preservation, and error propagation.                                                                                                       | [Full result](skill-assistant-evidence/2026-11-10-emil-unslop-code-result.json)                                                                          |
| `emil-forms-and-inputs`        | Repair signup markup; prevent duplicate submissions                    | 2/2 tasks passed, including loading evidence, associated label, email type, submit semantics, focus styling, one concurrent save, failure recovery, and retry.                                                          | [Full result](skill-assistant-evidence/2026-11-10-emil-forms-and-inputs-result.json)                                                                     |
| `emil-animation-accessibility` | Add a reduced-motion modal variant; respect live scrolling preferences | 2/2 tasks passed, including loading evidence, fade-only CSS, stopped decorative motion, and JavaScript behavior when preferences change.                                                                                | [Full result](skill-assistant-evidence/2026-11-10-emil-animation-accessibility-result.json)                                                              |

The six added tasks passed all 13 checks. Before execution, each unchanged fixture failed at least one acceptance check. Each client finished with exit zero and positive native evidence that it read the target skill. Generated files were also read back and inspected. The outputs retain the expected public APIs and contain the requested changes.

Run IDs:

- Code cleanup: `08c5cd88-ebd1-4c50-94c8-2c2769bc9702`.
- Forms: `9a4c82b5-23f3-4c8f-9fa8-c96ff1aaa049`.
- Motion: `9ccb1c02-6a36-4160-a262-c3d7808874f7`.

## Scope and reproduction

Use the [installed-skill suites and instructions](../../examples/skill-tests/installed/README.md) to repeat the added cases. The evidence records the exact client version, input, command, loading events, checks, changed files, and hashes. Home-directory prefixes are replaced with `<home>` in the committed exports; original local records remain intact.

These are acceptance cases, not a general skill-quality benchmark. They use explicit selection, one client, and one attempt per added task. They do not measure discovery, repeated-run reliability, or improvement against a no-skill baseline. Installed Codex settings and other skills can still affect the result.

The new HTML and CSS checks inspect source contracts. They do not establish rendered accessibility or motion quality. The JavaScript cases execute the output. Only the writing task was exercised through every desktop step, including saved review and native export. The [Assistant UI review](2026-11-10-skill-assistant-ui-review.md) describes the rendered application checks separately.

The existing synthetic report fixture also passed across Codex, Claude Code, OpenCode, and pi. Its pi no-skill control and deliberately missing-file case demonstrated that the engine can retain a failure when execution or a separate judge succeeds. See the [engine verification](2026-11-09-skill-assistant-verification.md).
