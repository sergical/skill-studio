# Dotagents Park race verification plan

| Issue | User flow                                                                       | Expectation                                                                     | Proof (file::test or command)                                                                                    | Red before fix?                    | Measured numbers |
| ----- | ------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- | ---------------------------------- | ---------------- |
| #447  | Unlist a parked skill while dotagents reinstalls it after the live-folder check | Preserve all new live bytes                                                     | park_install_aware::unlist_keeps_a_copy_reinstalled_after_the_live_folder_check; CLI/MCP no external remove flow | Yes: live file was deleted         | No timing claim  |
| #448  | Config write fails after another writer changes an unrelated row                | Rollback preserves the concurrent row, or refuses on drift with backup retained | park_install_aware::failed_park_keeps_concurrent_config_edits_during_rollback; fs callbacks at move and write    | Yes: concurrent extra row was lost | No timing claim  |

No sleep controls a race. Process and filesystem fakes inject writes at named operations.

The old implementation failed both tests using deterministic port callbacks.
No sleep controls a concurrent edit. The failed-Park flow also covers a change
made after Park's config write: rollback detects drift, keeps the current file,
and leaves its original backup available.

CLI and MCP run against temporary HOME folders. Their fake dotagents executable
fails if called. Each surface verifies Park/Turn on without calling that command
and a denied lock-file write that restores the config and live copy.

Local Cargo uses a temporary workspace compiler wrapper, with the same shared
target folder, to give this worktree's artifacts distinct names and capture its
own CLI/MCP test executables. The ordinary shared artifacts had supplied foreign
library methods and binaries. CI does not use that local wrapper.

The simplification pass renamed the private command-era helper to `unlist` and
removed obsolete command comments. No `events.rs` changes.

## Turn on failure after relisting

| Issue | User flow                                                             | Expectation                                          | Proof (file::test or command)                                                          | Red before fix?                 | Measured numbers |
| ----- | --------------------------------------------------------------------- | ---------------------------------------------------- | -------------------------------------------------------------------------------------- | ------------------------------- | ---------------- |
| #448  | Edit config after Turn on relists the skill, then fail the lock write | Keep the concurrent edit and return a conflict       | park_install_aware::failed_turn_on_keeps_an_edit_after_relisting_when_lock_write_fails | Yes; both lost the injected row | No timing claim  |
| #448  | Edit config after relisting, then fail the folder move                | Same preservation through the caller's rollback path | park_install_aware::failed_turn_on_keeps_an_edit_after_relisting_when_move_fails       | Yes; both lost the injected row | No timing claim  |

The follow-up simplification uses the existing `write_back` helper for all three
Turn on writes. Core package tests, format, and clippy pass after that change.

Final guard proof: with the rollback comparison removed, the post-write drift
test failed and the injected concurrent comment was absent from agents.toml.
Restoring the guard made the exact test pass. The ignore-rule reorder case was
also checked against origin/main and already passed, so that added test was
removed. Every remaining new race test fails without its fix.

## Failed Turn on and recovery storage

The replacement helper records a completed exchange before directory sync or
conflict detection can fail. Rollback restores the displaced input through the
same guarded exchange. Each target keeps one pending displaced file. Its name records the expected
content hash. The operation that displaced it never deletes it. A later Park or
Turn on reads it again and deletes it only if the hash still matches. A changed
pending file blocks the operation and the conflict reports its path. Rollback
exchanges the pending file back and retains the displaced output for the next
operation. Failed writes, file syncs, and exchanges remove their staging file.

With the production fix reverted once, the install-aware target had 58 passing
and five failing tests. The failures were:

- `failed_turn_on_at_config_replacement_keeps_skill_unlisted_or_install_revives_it`
  and `turn_on_keeps_ignore_edit_at_replacement_or_loses_external_comment`:
  rollback did not restore the displaced input.
- `failed_turn_on_after_directory_sync_keeps_skill_unlisted_or_install_revives_it`:
  the failed Turn on left the skill listed.
- `park_turn_on_cycles_remove_redundant_recovery_files_or_grow_storage`:
  unchanged inputs left recovery files after the first cycle.
- `failed_turn_on_retains_only_external_recovery_copy_or_loses_edit_and_grows_storage`:
  three recovery files remained instead of the one with the external edit.

The tests use filesystem callbacks and four Park / Turn on cycles. They have no
sleeps or wall-clock gates. Local builds use a unique workspace wrapper path in
the required shared target folder to prevent reuse of another worktree's core
library.

Final local checks passed: `cargo fmt --all --check`, workspace Clippy with all
targets/features and `-D warnings`, and all 63 install-aware Park tests on the
final source. The full core suite passed 366 tests before the local run was
stopped after more than 30 minutes of cumulative shared-lock and loader waits.
No test failure was reported in that run. CI runs the full core suite.

The post-green simplification pass found no worthwhile simplification. Final
review kept the changes within the replacement helper, its callers, tests, and
these verification notes.

## Open handles and failed staging

The open-writer tests open a writable handle before replacement. A callback
writes through that handle after the displaced-file comparison has read its
bytes. A second write through a cloned handle runs after Park returns, so it
also runs after the comparison. Config, lock, and ignore edits must survive. Four later Turn on attempts
must report the retained path and leave one pending file for that target.

The repeated-failure tests inject write, file-sync, and exchange errors after
staging creation. Each test tries Park four times and requires zero recovery
files after every failure. Hooks select the filesystem operation, with no sleeps
or wall-clock gates.

The cycle test now requires one pending file per target after each cycle.
Rollback tests accept a recovered external edit in the live file. Rollback
retains its displaced output rather than a second copy of the recovered input.
