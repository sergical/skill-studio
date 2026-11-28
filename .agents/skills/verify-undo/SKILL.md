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

Two more rules ride along. An undo is recorded as finished only after its own
write of the files ran to the end, and nothing moves the files afterwards
(`UndoneMeansFilesBack`). This is about the change's main path only; see "Not
covered". A finished undo keeps its claim, so a change cannot be undone twice
(`FinishedUndoKeepsClaim`).

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

Exit 0 means every rule holds, every deliberate break is caught, and the
covered code is the text the model was confirmed against. Exit 1 means one of
those is not true. There is no list of excused failures.

## Read the result

Each line starts with one of these words.

| Line      | Meaning                                                                   | What to do                                              |
| --------- | ------------------------------------------------------------------------- | ------------------------------------------------------- |
| `PASS`    | The rule holds in every state, or a deliberate break was caught.          | Nothing.                                                |
| `FAIL`    | A rule breaks, or a deliberate break was not caught.                      | Read the path. Decide: real bug or model error (below). |
| `CHANGED` | A covered function differs from the text the model was confirmed against. | See "When the covered code changes".                    |

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
4. For a real bug: fix the code. Do not weaken the rule, do not shrink the
   model until it passes, and do not add an exception so the check exits 0.
   If the fix cannot be in the same change, file the bug with the path and
   leave the check failing: a red check is the true state of the code.

## Does my change touch the verified code?

Run the check. A `CHANGED` line names each covered function your change
touched. The covered functions, and the model actions that mirror each:

| File                                                  | Function                                                                | Model actions                                                                    |
| ----------------------------------------------------- | ----------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| `crates/skill-studio-core/src/ops.rs`                 | `restore_event_body`                                                    | every `Core…` action: checks, record, claim, write, finish                       |
| `crates/skill-studio-core/src/ops.rs`                 | `apply_backup_content`, `restore_write_dir`                             | `CoreOpensSwapPlan`, `CoreWritesAtOnce`, `CoreClosesPlan`                        |
| `crates/skill-studio-core/src/fsops.rs`               | `swap`                                                                  | `CoreOpensSwapPlan`, `CoreWritesAtOnce`, `CoreWriteFails`                        |
| `crates/skill-studio-core/src/ops_remove.rs`          | `remove_tree_best_effort`                                               | `CoreStartsWrite`, `CoreEndsWrite`                                               |
| `crates/skill-studio-core/src/events.rs`              | `recover_interrupted`, `release_restore_claim`                          | `RecoveryReleasesClaim`, `RecoveryMarksRowInterrupted`                           |
| `crates/skill-studio-core/src/events.rs`              | `settle`                                                                | every action that sets a row's status                                            |
| `crates/skill-studio-core/src/ports.rs`               | `begin_for`                                                             | recovery runs first, in this order: rows, then the journal                       |
| `crates/skill-studio-core/src/ports.rs`               | `take`, `acquire_exclusive`                                             | one writer at a time (the home lease)                                            |
| `crates/skill-studio-core/src/journal.rs`             | `reconcile_scope_journal`, `reconcile`, `reverse_steps`                 | the three `Recovery…` actions that put a swapped folder back and close the plan  |
| `crates/skill-studio-host/src/history.rs`             | `record`, `claim_revert`, `release_revert`                              | `CoreRecordsRestoreRow`, `CoreClaims`, the two release actions                   |
| `crates/skill-studio-host/src/history.rs`             | `held_restore_claims`, `pending`, `finish`                              | which claims recovery releases, which rows it marks                              |
| `apps/desktop/src-tauri/src/skills/event_store.rs`    | `restore_with_skill_md_transaction_observer`                            | `DesktopChecksClaim`, `DesktopReleasesOnError`                                   |
| `apps/desktop/src-tauri/src/skills/event_store.rs`    | `apply_restore`, `apply_restore_distribute`                             | drift check, record, claim, and what follows a failed or finished write          |
| `apps/desktop/src-tauri/src/skills/event_store.rs`    | `backup_for_restore`, `record_with_bound_paths`                         | `DesktopRecordsRestoreRow`                                                       |
| `apps/desktop/src-tauri/src/skills/event_store.rs`    | `claim_recorded_restore`, `claim_event_restore`                         | `DesktopClaims`                                                                  |
| `apps/desktop/src-tauri/src/skills/event_store.rs`    | `fail_claimed_restore`, `unclaim_event_restore`                         | `DesktopReleasesAfterFailure`, `DesktopFinishesFailed`, `DesktopReleasesOnError` |
| `apps/desktop/src-tauri/src/skills/event_store.rs`    | `finish`, `patch_inverse_post_fingerprint`                              | `DesktopFinishesFailed`, `DesktopFinishesDone`                                   |
| `apps/desktop/src-tauri/src/skills/event_store.rs`    | `apply_inverse_op`                                                      | `DesktopWritesAtOnce`, `DesktopWriteFails`                                       |
| `apps/desktop/src-tauri/src/skills/undo_files.rs`     | `restore_from`, `symlink`                                               | `DesktopRemovesLiveFiles`, `DesktopPutsOldFilesIn`                               |
| `apps/desktop/src-tauri/src/skills/event_store.rs`    | `reconcile_at_startup`                                                  | `DesktopAppStarts`, then `RecoveryMarksRowInterrupted`                           |
| `apps/desktop/src-tauri/src/skills/event_commands.rs` | `restore_desktop_event`, `restore_event_with_state`                     | which code undoes a change, and that it holds the lease                          |
| `apps/desktop/src-tauri/src/lib.rs`                   | `reconcile_event_store_at_startup`, `reconcile_core_journal_at_startup` | `DesktopAppStarts` and the recovery steps it runs                                |

One piece of covered code has no hash: the recovery closure in `run`
(`apps/desktop/src-tauri/src/lib.rs`), which calls
`reconcile_event_store_at_startup` and then `reconcile_scope_journal` under
the startup lease. `run` is too long to hash without noise. If you change
what that closure calls, or its order, change `DesktopAppStarts` and
`RecoveryEnds`.

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

A check that was never seen failing proves nothing. The spec has seven
switches (`Mutations`), each one edit to the real code: recovery skips the
release, recovery releases a finished undo's claim, the core claims before it
records, the core records a failure before it releases, the core records
success before it writes the files, the desktop claims before it records, and
the desktop records a failure before it releases. The last two are the order
the desktop had until [issue 787](https://github.com/sergical/skill-studio/issues/787)
was fixed. Every run turns each on alone and requires the rule it guards to
break. A `FAIL mutation …` line means a rule stopped being able to fail: the
model lost the step that rule depends on.

## Bounds and assumptions

- One change, 3 Undo requests, 2 crashes, and each way of writing the files
  below. 28,595 states for the core and 7,666 for the desktop, and all of
  them are checked. A pass covers these bounds, not every history.
- A crash is the only fault: the process dies between two durable steps and
  what it wrote stays. A single SQL statement that fails while the process
  lives is not modeled, apart from the file write itself. One consequence is
  known and open: the core ignores a failed release and can then record the
  undo as failed with the claim still held
  ([issue 796](https://github.com/sergical/skill-studio/issues/796)).
- One writer at a time. Every writer takes the exclusive home lease, an OS
  file lock (`crates/skill-studio-host/src/lease.rs`). Desktop startup holds
  it across its whole recovery. The model assumes the lock works.
- The files are in one of three states: as the change left them, as they
  were before it, or neither (`partial`). Every write may land at once or be
  caught half way, by a crash or by an error. Where the code writes in one
  atomic operation, the half-way state cannot happen, so the model allows
  more than the code does, never less: a rule that holds in the model holds
  for that code. `partial` stands for every half-written state; the model
  does not tell them apart.
- The core's folder swap runs under a journal plan (`WriteShapes`). Recovery,
  and desktop startup, put back what an unfinished swap replaced, which can
  also be caught half way, and close the plan in a second write.

## Not covered

- What the files hold. The model knows that an undo's write ran to the end,
  not that it reproduced the old tree byte for byte. One gap seen while
  reading the code, not checked here: `read_backup_files` in `history.rs`
  returns regular files only, so an empty folder inside a backed-up skill
  folder is not put back by an undo.
- Redo (an undo of a restore row). It runs the same code with the restore row
  as the change, but the model has one change only.
- Force retries: `restore_retry`, `displaced_data`, and which snapshot a Redo
  puts back after a forced retry.
- Events with several paths, links, lock rows or registry rows. The model has
  one path, the change's main one. For the other paths the core works on a
  best effort: it records an undo as finished even when a link or a copy
  could not be put back, and notes the error on the restore row.
- `make_independent_copy`: its restore keeps its claim on purpose and desktop
  startup resumes it (`skill_independent_copy.rs`).
- Backup expiry, `recovery drop`, and a journal plan that recovery cannot
  reverse.
- Unpark, which also writes `reverted_by` on a park row (`ops.rs`,
  `unpark_body`).

## What it has found

[Issue 787](https://github.com/sergical/skill-studio/issues/787): the desktop
claimed a change before it recorded the restore row, and recorded a failed
undo before it released the claim. An app that died in either gap left the
change shown as undone for good. The desktop check broke `NoStrandedUndo` in
8 steps. The fix made the desktop use the core's order, and the test
`a_desktop_undo_cut_off_at_any_history_write_can_run_again_or_the_change_stays_marked_undone`
in `apps/desktop/src-tauri/src/skills/event_commands.rs` kills the real Undo
before each of its history writes. That test runs on the code; this check
runs on the model. Keep both.

## Files

- `check`: the one command. Python 3, standard library only.
- `model/UndoClaim.tla`: the model. `model/UndoClaim.core.cfg` and
  `model/UndoClaim.desktop.cfg`: the two checks.
- `model/*.chart.cfg`, `model/view.mjs`: smaller bounds and the user's words,
  for charts.
- `covered.json`, `words.json`: the coverage record and the plain words the
  check prints.
- `CHART.md`: the state charts.
