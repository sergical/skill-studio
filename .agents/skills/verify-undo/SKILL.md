---
name: verify-undo
description: Model-check Undo (Activity Undo, `skill-studio restore`, MCP `restore_event`) with TLA+, so that a crash between two steps never leaves a change shown as undone when no undo finished. Use when a change touches `restore_event_body`, `recover_interrupted`, `EventStore::restore`, the `reverted_by` claim, or the order of the writes around them, and before you report such a change as done.
---

# Verify Undo

Undo is the safety net under every write Skill Studio makes. This skill checks
one promise of it in every order of steps, including a crash between any two
of them. A test checks the crash points someone thought of; the model checker
(TLC) tries them all, inside the bounds below.

## What is checked

**The rule (`NoStrandedUndo`).** Once no write is running and the next write
has done its recovery, a change is shown as undone only if an undo of it
finished.

**The harm when it breaks.** The change can never be undone. Activity shows it
as undone and offers no Undo, the CLI answers `this event was already
reverted` (also with `--force`), and the files were never put back.

Two more rules ride along: every finished undo has the files back as they
were (`UndoneMeansFilesBack`), and a finished undo keeps its claim, so a
change cannot be undone twice (`FinishedUndoKeepsClaim`).

The model is [model/UndoClaim.tla](model/UndoClaim.tla). It follows one change
through Undo by the two pieces of code that can undo it: the core
(`Owner = "core"`) and the desktop's own event store (`Owner = "desktop"`).
[CHART.md](CHART.md) shows both flows as state charts in the user's words.

## Run it

```bash
.agents/skills/verify-undo/check
```

It takes about 15 seconds. It needs Java 11 or later (`brew install openjdk`,
or set `JAVA`). The first run downloads TLC 1.7.4 to
`~/.cache/skill-studio/` and checks its SHA-256; set `TLA2TOOLS` to use a jar
you already have. It needs no Rust build and writes nothing in the repo.

Exit 0 means every result is the expected one. Exit 1 means something changed.

## Read the result

Each line starts with one of these words.

| Line      | Meaning                                                                                               | What to do                                                              |
| --------- | ----------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------- |
| `PASS`    | The rule holds in every state, or a deliberate break was caught.                                      | Nothing.                                                                |
| `KNOWN`   | A rule breaks, and only by the cause that [known-findings.json](known-findings.json) lists.           | Nothing, unless your change is the fix: then see "Known finding" below. |
| `FAIL`    | A rule breaks in a new way, or a deliberate break was not caught.                                     | Read the path. Decide: real bug or model error (below).                 |
| `CHANGED` | A covered function differs from the text the model was confirmed against, or a known finding is gone. | See "When the covered code changes".                                    |

A `FAIL` prints the rule in plain words and the shortest path to the break,
one step per line, with the model action in brackets. To see the path as a
chart, or a whole flow, read the result with the `/pstack-statechart` skill:
give it `model/UndoClaim.tla`, one of the two chart configs
(`model/UndoClaim.core.chart.cfg`, `model/UndoClaim.desktop.chart.cfg`) and
the view `model/view.mjs`, which holds the user's words for each state and
step. Use the chart configs for a chart: the check configs have more states
than a chart can show.

### A real bug or a model error?

1. For each step of the path, open the function that the action mirrors (the
   table below, or `mirrors` in [covered.json](covered.json)). Check that the
   code does that step, in that order, as its own durable write. One SQL
   statement or one file swap is one step.
2. If the code does not do a step, does it in another order, or does two
   steps inside one SQL transaction, the model is wrong. Fix the model and
   run the check again.
3. If every step matches the code, replay the path with the
   `verify-skill-studio` skill: the built CLI in a throwaway home. A step
   that only a crash can cause is one SQL statement; write it with `sqlite3`
   on `<home>/.local/share/skill-studio/history/events.sqlite3`, copied from
   the code. If the replay shows the harm, it is a real bug.
4. For a real bug: do not weaken the rule and do not shrink the model until
   it passes. Fix the code, or report the path. A bug you report and leave
   goes in `known-findings.json` with its issue, so the check stays useful
   for the next change.

## Does my change touch the verified code?

Run the check. A `CHANGED` line names each covered function your change
touched. The covered functions, and the model actions that mirror each:

| File                                                  | Function                                                                | Model actions                                                        |
| ----------------------------------------------------- | ----------------------------------------------------------------------- | -------------------------------------------------------------------- |
| `crates/skill-studio-core/src/ops.rs`                 | `restore_event_body`                                                    | every `Core…` action: checks, record, claim, write, finish           |
| `crates/skill-studio-core/src/ops.rs`                 | `apply_backup_content`, `restore_write_dir`                             | `CoreWritesInOneStep`, `CoreSwapsFolderIn`, `CoreClosesPlan`         |
| `crates/skill-studio-core/src/ops_remove.rs`          | `remove_tree_best_effort`                                               | `CoreStartsRemovingFolder`, `CoreEndsRemovingFolder`                 |
| `crates/skill-studio-core/src/events.rs`              | `recover_interrupted`, `release_restore_claim`                          | `RecoveryReleasesClaim`, `RecoveryMarksRowInterrupted`               |
| `crates/skill-studio-core/src/events.rs`              | `settle`                                                                | every action that sets a row's status                                |
| `crates/skill-studio-core/src/ports.rs`               | `begin_for`                                                             | recovery runs first, in this order: rows, then the journal           |
| `crates/skill-studio-core/src/ports.rs`               | `take`, `acquire_exclusive`                                             | one writer at a time (the home lease)                                |
| `crates/skill-studio-core/src/journal.rs`             | `reconcile_scope_journal`                                               | `RecoveryReversesPlan`, `DesktopStartReversesPlan`                   |
| `crates/skill-studio-host/src/history.rs`             | `record`, `claim_revert`, `release_revert`                              | `CoreRecordsRestoreRow`, `CoreClaims`, the two release actions       |
| `crates/skill-studio-host/src/history.rs`             | `held_restore_claims`, `pending`, `finish`                              | which claims recovery releases, which rows it marks                  |
| `apps/desktop/src-tauri/src/skills/event_store.rs`    | `restore_with_skill_md_transaction_observer`                            | `DesktopChecksClaim`, `DesktopClaims`, `DesktopReleasesAfterFailure` |
| `apps/desktop/src-tauri/src/skills/event_store.rs`    | `apply_restore`, `record_with_bound_paths`                              | drift check, `DesktopRecordsRestoreRow`, the two finish actions      |
| `apps/desktop/src-tauri/src/skills/event_store.rs`    | `apply_inverse_op`                                                      | `DesktopWritesInOneStep`, `DesktopWriteFails`                        |
| `apps/desktop/src-tauri/src/skills/undo_files.rs`     | `restore_from`                                                          | `DesktopRemovesLiveFiles`, `DesktopCopiesBackupIn`                   |
| `apps/desktop/src-tauri/src/skills/event_store.rs`    | `reconcile_at_startup`                                                  | `DesktopStartMarksRowInterrupted`                                    |
| `apps/desktop/src-tauri/src/skills/event_commands.rs` | `restore_desktop_event`, `restore_event_with_state`                     | which code undoes a change, and that it holds the lease              |
| `apps/desktop/src-tauri/src/lib.rs`                   | `reconcile_event_store_at_startup`, `reconcile_core_journal_at_startup` | the two `DesktopStart…` actions                                      |

One piece of covered code has no hash: the recovery closure in `run`
(`apps/desktop/src-tauri/src/lib.rs`), which calls
`reconcile_event_store_at_startup` and then `reconcile_scope_journal` under
the startup lease. `run` is too long to hash without noise. If you change
what that closure calls, or its order, change the `DesktopStart…` actions.

Also run the check when you add a new writer of `reverted_by`, a new status
for a restore row, or a new recovery pass, even when no line above changes:
the model then misses a step, and only you know it.

## When the covered code changes

`covered.json` is the record of what the model was confirmed against: each
covered function with the SHA-256 of its text. The hashes are the sync
record, because PRs are squash-merged and a branch commit would not survive.
`accepted_on_top_of` is the commit on `main` that the accepted text was based
on. When a branch changes a covered function and accepts it, the accepted text
is that commit plus the branch's change, not the commit alone.

1. Read your diff of each function a `CHANGED` line names.
2. If it changes what is written, or the order, change the matching action
   in `model/UndoClaim.tla` the same way. Keep one action per durable step.
   If it changes neither (a rename, a log line, a new read), the model stays.
3. Run the check. A `FAIL` here is your change breaking the rule: go to
   "A real bug or a model error?".
4. Only when the model describes the code as it is now, run
   `.agents/skills/verify-undo/check --accept`. It records the new hashes and
   sets `accepted_on_top_of` to the branch's merge base with `origin/main`.
   Never run it to make a `CHANGED` line go away
   without step 1 to 3.
5. If you added or removed a durable step, update the table above,
   `mirrors` in `covered.json`, the words in [words.json](words.json) and
   `model/view.mjs`, and draw [CHART.md](CHART.md) again with
   `/pstack-statechart`.

## The check can fail

A check that was never seen failing proves nothing. The spec has five
switches (`Mutations`), each one edit to the real code: recovery skips the
release, recovery releases a finished undo's claim, the core claims before it
records, the core records a failure before it releases, the core records
success before it writes the files. Every run turns each on alone and
requires the rule it guards to break. A `FAIL mutation …` line means a rule
stopped being able to fail: the model lost the step that rule depends on.

## Bounds and assumptions

- One change, 3 Undo requests, 2 crashes, and each way of writing the files
  below. 11,017 states for the core, and all of them are checked. A pass
  covers these bounds, not every history.
- A crash is the only fault: the process dies between two durable steps and
  what it wrote stays. A single SQL statement that fails while the process
  lives is not modeled, apart from the file write itself.
- One writer at a time. Every writer takes the exclusive home lease, an OS
  file lock (`crates/skill-studio-host/src/lease.rs`). The model assumes the
  lock works.
- The files are in one of three states: as the change left them, as they
  were before it, or neither (`partial`). An undo writes them in one of
  three ways, fixed per run (`WriteShapes`): one atomic step; two steps with
  `partial` in between and no journal (the core removing an installed folder
  entry by entry, the desktop removing the live path and then copying the
  backup); or the core's folder swap under a journal plan, which recovery and
  desktop startup reverse if it was never finished. `partial` stands for
  every half-written state; the model does not tell them apart.

## Not covered

- Redo (an undo of a restore row). It runs the same code with the restore row
  as the change, but the model has one change only.
- Force retries: `restore_retry`, `displaced_data`, and which snapshot a Redo
  puts back after a forced retry.
- Events with several paths, links, lock rows or registry rows. The model has
  one path.
- `make_independent_copy`: its restore keeps its claim on purpose and desktop
  startup resumes it (`skill_independent_copy.rs`).
- Backup expiry, `recovery drop`, and a journal plan that recovery cannot
  reverse.
- Unpark, which also writes `reverted_by` on a park row (`ops.rs`,
  `unpark_body`).

## Known finding

`known-findings.json` lists each rule that breaks today, with its issue and
its `except_rule`: the same rule with that one cause left out. Today it has
one entry, [issue 787](https://github.com/sergical/skill-studio/issues/787):
the desktop claims a change before it records the restore row, and records a
failed undo before it releases the claim. If the app dies in either gap, the
change is stranded.

For a config with a known finding the check runs TLC twice: once with the
rule, which must break, and once with the `except_rule`
(`NoStrandedUndoBeyondIssue787` in the spec), which must hold. So a second,
different way to strand a change still prints `FAIL`. When a fix lands, the
first run passes, the check prints `CHANGED`, and you remove the entry, the
`except_rule` and the `bareClaim` variable in the same PR.

## Files

- `check`: the one command. Python 3, standard library only.
- `model/UndoClaim.tla`: the model. `model/UndoClaim.core.cfg` and
  `model/UndoClaim.desktop.cfg`: the two checks.
- `model/*.chart.cfg`, `model/view.mjs`: smaller bounds and the user's words,
  for charts.
- `covered.json`, `known-findings.json`, `words.json`: the coverage record,
  the open findings, and the plain words the check prints.
- `CHART.md`: the state charts.
