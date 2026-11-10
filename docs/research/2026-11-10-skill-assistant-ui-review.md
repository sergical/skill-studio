# Skill Assistant workspace review — November 10, 2026

**Verdict: Ship it for the feature-flagged task workspace.** Implementation, code review, and the final native visual check are complete. No blockers remain in the reviewed flow. The previous modal review did not examine the entry hierarchy and page structure well enough. This review supersedes that approval.

The review used `ask-emil`, then `emil-design-foundations` for the layout and `emil-ui-review` for the finished flow. This is a review using those instructions, not a personal endorsement by Emil.

## Layout decisions

| Before                                                                        | After                                                                                       | Why                                                                           |
| ----------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| Test skill and Assistant competed in the skill header; one button was taller. | One Assistant entry, with the same visible height as Fork.                                  | One route to the feature and a consistent header.                             |
| A large modal held the whole workflow.                                        | Assistant occupies the main page, with a breadcrumb back to the skill.                      | The task, input, evidence, and review need a workspace.                       |
| Suite management, variants, checks, and judges preceded the task.             | Task and optional input files come first. Advanced options are collapsed.                   | The first run needs only the user's task and input.                           |
| Every new task implicitly expected report.md.                                 | A new task has a quality criterion for review.                                              | A generic skill must not fail because it did not create an arbitrary file.    |
| Comparisons and technical evidence dominated results.                         | Created/changed files come first; comparisons and run details are disclosed.                | Users should see what the skill produced before inspecting execution details. |
| The improvement view exposed Ask, Audit, and Test together.                   | It starts with a question; instruction review and project testing are under Advanced tools. | Preserve the existing tools while keeping the first choice clear.             |

## Fresh code review

The first pass was **Blocked**. These findings were fixed and checked:

- A missing or changed skill path could leave both the skill page and workspace hidden. Open state is now local, and workspace visibility requires the path that was opened.
- Editing an earlier run of the same suite could display stale files. Explicit suite replacement starts a new editor revision.
- Escape from Assistant could discard an underlying SKILL.md draft. Returning within the skill page now bypasses the departure guard; leaving the page still uses the guard.
- Unsaved review text could disappear across trials or views. Drafts are held by run and trial ID.
- Retry refreshed lists rather than retrying the failed action. It now says Refresh clients and history and explains the next step.

The final code-only review found no remaining findings. Its last pass caught a possible overlap between a question and project-test preparation. The question composer is now hidden during project testing, and the shared busy state blocks competing starts through preparation, execution, and judging. A regression test holds preparation pending and verifies those controls stay blocked.

## Rendered checks completed

- Native skill page: one Assistant button, matching the adjacent Fork control; no Test skill button.
- Native task page: dedicated workspace, task and input first, advanced options closed, one Run task action.
- Real execution started from that simple path without opening advanced options.
- Result: execution, skill loading, quality review, output files, and raw evidence remain distinct.
- Review text survived Task → Result navigation; the review was then saved.
- Saved runs reopened the completed result. Export used the native Save dialog and the JSON was read back.
- Dark and light task layouts were inspected. A clipped sticky footer was corrected. The final native check confirmed that the whole Run task button stays inside the viewport in both themes.
- The revised Improve skill view starts with the question composer and one Ask action. Advanced tools starts closed and reveals instruction review and project testing when opened.
- The skill breadcrumb returned to the skill page and restored keyboard focus to its single Assistant button. Pressing Return reopened the workspace, focused its region, and retained the task and input.
- The browser preview was inspected at 900 × 600 and at a 660-pixel content width, matching the space beside the desktop sidebar at the minimum window width. Task, advanced controls, and results had no document-level horizontal overflow.
- Compact results kept navigation visible and placed keyboard focus on the result region. Browser console error entries: none.
- Workspace buttons, inputs, select controls, and summaries use 44-pixel targets. The header keeps its compact visible size with expanded hit areas.
- Temporary browser viewport settings were reset and the preview tab was closed.

After the Mac was unlocked, the remaining native checks passed. Dark mode was restored, and the app was left on Task with the real example ready to edit and run. The feature remains off by default.

## Real task evidence

- Skill: installed `emil-unslop-writing`.
- Run: `87913b40-a799-4737-90ca-526fc8735063`.
- Client: Codex; execution took 17.907 seconds.
- Execution: completed. Skill loaded: yes.
- Automatic outcome: needs review, as expected for the default quality criterion with no separate judge.
- Source: `input.md` remained unchanged; `report.md` was created.
- A review labeled **Codex review** was accepted and saved separately from the check result.
- Exported evidence: [task workspace run](skill-assistant-evidence/2026-11-10-task-workspace-run.json).

Actual output:

> Tide launched CSV export on Tuesday. Users can now export filtered rows to CSV.

The earlier run with four explicit file checks is retained in [the earlier evidence](skill-assistant-evidence/2026-11-10-real-writing-skill.json). Neither case is a controlled comparison against a run without the skill.

## Broader installed-skill coverage

The [installed-skill acceptance report](2026-11-10-installed-skill-tests.md) adds six tasks across code cleanup, forms, and reduced motion. All 13 added checks passed. Those runs used the shared CLI engine; the desktop end-to-end path above used the writing skill.

## Checks and scope

- Full desktop test run after the state fixes: 570 tests passed in 57 files.
- Final scoped assistant, navigation, and flag checks: 24 passed in 5 files. The additional project-preparation regression test passed separately.
- Type checking, scoped lint, formatting, and whitespace checks passed.
- Post-green simplification removed the unused drawer and the dialog persistence option that no longer had callers. The related checks passed after that pass.
- No Rust code changed in this workspace redesign. The native run exercised the existing engine.
- The real task flow was exercised through execution, output review, saved history, and export. The legacy project-test path received a disclosure check and a busy-state regression test; it was not run end to end again during this review.
- The `skill-assistant` flag still defaults to off. The development opt-in is ignored in production. Both the entry and workspace are guarded; flag-off tests verify no lab requests.
- Task and review drafts survive workspace view changes and returning to the same skill page. Save a suite before leaving that skill page or quitting; backend runs continue independently.

## Pre-PR review fixes

A fresh code review found two further issues. Adding a file after deleting a middle row could reuse a filename; the editor now selects an unused default name and a regression test checks the submitted files. Codex loading evidence split quoted paths at spaces; shell arguments now retain quoted and escaped paths, with coverage for the desktop application data path and a shell-wrapped read.

After updating the PR to current main, 31 scoped tests passed in six files, including the file-name regression, Assistant navigation, project-test busy state, and feature-flag checks. Type checking passed with the current dependency lockfile. The last fresh code review found no remaining issues in the two pre-PR fixes.
