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
