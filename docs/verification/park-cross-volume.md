# Cross-volume Park verification plan

| Issue | User flow                              | Expectation                                                        | Proof (file::test or command)                                                                  | Red before fix?                                     | Measured numbers                                            |
| ----- | -------------------------------------- | ------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------- | --------------------------------------------------- | ----------------------------------------------------------- |
| #398  | Cross-volume Park and Turn on          | Preserve root, nested directory, and executable modes              | park_any_copy::cross_volume_roundtrip_preserves_directory_and_file_modes; CLI/MCP volume flows | Yes; core, CLI and MCP got 0755 for input 0750/0710 | N=10, 1/100 skills, release binary, real APFS second volume |
| #398  | Full sync fails before source disposal | Keep complete source and remove partial destination                | park_any_copy::cross_volume_park_keeps_source_when_full_sync_fails                             | Yes; ran against the old move driver                | Full sync cost in before/after report                       |
| #398  | Destination reports synthetic modes    | Verify bytes and keep operation usable without false mode mismatch | park_any_copy::cross_volume_park_accepts_synthetic_modes_without_losing_bytes                  | Yes; ran against the old move driver                | No wall-clock CI assertion                                  |

The filesystem port has a distinct full-sync operation. On macOS the host
calls F_FULLFSYNC. Directory entries flush before full file sync, and the source
is renamed away only after all syncs succeed. Directory modes are applied after
copying children and verified with file modes. Destination mode support is
probed by writing and reading back file and directory permission bits.

Local physical fixtures use an attached temporary APFS volume. Linux CLI/MCP
tests use /dev/shm when its device differs from HOME; core port fixtures always
exercise EXDEV. All homes and projects are disposable fixtures.

Baseline: Apple M3 Max, release CLI, three files per skill, N=10 per size.
Park-move source blob before the fix: 9157854fbee42f7d4b61b5c94bd6321afa30e5a3.

Core, host, CLI and MCP package tests passed with all features. Physical tests
used the attached APFS fixture; the nested read-only directory (0500) also
round-trips. A baseline command failed on that read-only case. Clippy and
format checks passed. No SMB share was available; the filesystem port test
models a destination that ignores requested modes and checks file bytes.

No events.rs changes. No dependency version bumps. The host uses the locked
nix version for the safe macOS F_FULLFSYNC operation.

The final repeated package run hit the existing MCP
`watch_and_a_fresh_mcp_scan_agree_after_a_cli_mutation` five-second receive
timeout. Its function is byte-identical to origin/main (SHA-256
2466674e049b7de6b9110222b3ec9e0482a71129903285ca5aa5a5b71f997b1b).
That flow scans and repairs frontmatter; it does not call Park. The exact target
is rerun separately. The Park, Turn on, layout, and volume tests passed.

The isolated MCP watch/repair retry passed (one test). Its elapsed time included
waiting for nested Cargo builds. Rust format and scoped clippy passed after simplification.

## Source changes during sync

The copy driver captures the source device and inode before the copy. After
full sync it checks that identity and compares the current source tree with
the complete copy, including file hashes and permission bits. A mismatch
returns `DriftConflict` and keeps both trees. Copy and flush failures remove
the incomplete copy.

Two deterministic `FailingFs` hooks change the source after full file sync.
No test uses sleeps or wall-clock assertions. Both tests failed against the
unfixed driver on 2026-11-10 because Park returned success and deleted the
source:

- `cross_volume_park_keeps_source_edit_during_sync_or_deletes_external_bytes`
- `cross_volume_park_keeps_replaced_source_during_sync_or_deletes_external_folder`

The tests assert `DriftConflict`, the retained source, and the complete copy.
The replacement test retains the original inode and creates a new source
with identical bytes, so a hash-only check cannot pass it.

With the fix, the same command passed both tests locally on 2026-11-10:

```sh
cargo test -p skill-studio-core --all-features \
  --config 'profile.test.package.skill-studio-core.opt-level=1' \
  --test park_any_copy during_sync
```

## Quarantine and destination ownership, 2026-11-10

The final source check runs after a same-volume rename into `.park-quarantine-*`.
A conflict keeps that source and the complete, flushed destination and reports
`DriftConflict` with the retained path. Quarantine names are outside the trash
sweeper's namespace. Only a verified source enters ordinary trash cleanup.
Destination verification uses the hashes of the bytes read during copying.
Failure cleanup checks device/inode identities and removes only recorded entries.

Against `8fbb56e3`, these deterministic hook tests failed at their assertions:

- `cross_volume_park_keeps_edit_before_quarantine_or_deletes_already_hashed_bytes`
- `cross_volume_park_keeps_replacement_before_quarantine_or_deletes_another_inode`
- `cross_volume_park_keeps_edit_while_hashing_later_file_or_deletes_already_hashed_source`
- `cross_volume_park_keeps_complete_copy_on_early_drift_or_returns_io_and_deletes_backup`
- `cross_volume_park_preserves_racing_destination_or_chmods_and_deletes_foreign_skill`
- `cross_volume_park_refuses_cleanup_of_replaced_destination_or_deletes_foreign_tree`
- `cross_volume_park_cleans_only_created_entries_or_removes_foreign_file`

The first baseline run failed all six new tests. A separate baseline run of the
later-file hash test returned success and deleted the edit, so its assertion failed.
No sleeps or timing assertions are used. Each test uses a temporary home fixture.

Local acceptance, also repeated after simplification:

- `cargo fmt --all --check`
- `CARGO_TARGET_DIR=/Users/sergiydybskiy/src/skill-studio/target/clippy cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --release -p skill-studio-core --all-features --test park_any_copy --test park_install_aware --test agent_off`: 126 passed.

Builds use Command Line Tools, the shared target folder, and a temporary `HOME`.

## Review fixes for PR 491

Cross-volume moves retain the quarantined source as a backup. Open file handles
can write after verification. The event records the backup path and the
`retain_before_rename` phase before the rename. SQLite uses FULL synchronous
writes and macOS fullfsync. The journal directory and source parent are synced.
Recovery reports existing backups and keeps their bytes.

Failed copies now stay at the destination reported in the error. Cleanup cannot
prove ownership across a path replacement and unlink, so the move no longer
captures destination ownership or deletes failed copies. This also keeps foreign
entries inserted before identity capture or after an identity check.

The mode probe creates one empty file exclusively and changes permissions
through its creation handle. It retains the empty file because unlink cannot
exclude replacement after an identity check. No probe directory is created.

Deterministic tests cover writes through an open handle after hashing, both
destination replacement points, a probe creation race, a probe symlink
replacement, and interruption immediately after the quarantine rename.

The four new core tests fail with `park_move.rs` restored from `6df5b679`.
Both new probe tests fail when unconditional path cleanup is restored in the
probe test hook. The fixes were restored after these checks. All tests use
temporary home directories.

## PR 491 review fixes

Cross-volume copies now use a journaled `.park-stage-*` sibling on the destination
volume. The move verifies bytes and modes, fully syncs the stage, and checks its
identity and bytes again before publication. A failed copy or replaced stage stays
hidden. The live destination stays absent. A staging name collision retries with a
new name. Permission support is checked on the stage itself; no probe file is made.

History keeps recovery notices until explicit acknowledgement. Desktop startup and
move completion read these notices into a persistent toast. CLI emits a stderr
warning and a JSON recovery report. MCP returns the report in the tool result.
Recovery also covers interruptions before the first stage record and failures from
the current call. Retained paths have measured sizes when readable. Successful
Park and Turn on results include source backup paths and size snapshots. Desktop
batch results keep that metadata on success and failure. Activity keeps legacy
paths whose size is unknown. Open writers can still change retained source copies.

The regression suite uses fault hooks and temporary homes. It covers failed Turn
on after copying SKILL.md but before copying a script, interruption before stage
publication, stage replacement during sync, stage name collisions, repeated
recovery through a NoopSink, explicit acknowledgement, open-handle writes, CLI
stderr and JSON, MCP restart delivery, and desktop notice and Activity rendering.

Shared-target stale builds occurred during verification. Workspace crates use
`RUSTC_WORKSPACE_WRAPPER=/tmp/skill-studio-pr491-rustc`, a script that runs its
arguments unchanged. Cargo gives these workspace artifacts a separate cache key
inside the same shared target. No other Cargo process or target file was removed.

## Recovery notice and Activity review fixes

Desktop recovery keeps one persistent notice per record ID across refreshes.
Acknowledgement dismisses that record's notice after the command succeeds.
Activity uses the recovery path resolver to measure copies that still exist.
A partial stage shows its current size. After publication, Activity finds the
source and destination instead of showing the staging path removed by rename.
An existing path whose size cannot be measured keeps an unknown size.

Regressions use temporary folders and IPC fixtures, without sleeps:

- `refresh twice then acknowledge removes the record notice or leaves duplicate persistent warnings`
- `activity_after_partial_copy_measures_stage_or_reports_planned_source_size`
- `activity_after_publication_lists_existing_copies_or_reports_renamed_stage`

All three regressions failed with their corresponding fix removed and passed
with the fixes restored. The partial-copy baseline reported 11 bytes instead
of 5. The publication baseline kept one obsolete stage entry instead of the
two existing paths. The frontend baseline added two notices for one record.

Missing-item errors from shared Cargo artifacts survived two timestamp
refreshes. Validation uses a pass-through compiler wrapper at
`/tmp/pr491-review-rustc` to separate workspace cache keys within
`/Users/sergiydybskiy/src/skill-studio/target`. No target files were deleted
and no other Cargo process was stopped.

Final validation passed: Rust formatting, workspace Clippy with all targets
and features and warnings denied, the full desktop crate tests, the full core
crate tests, frontend typecheck, lint, formatting, and all three recovery-hook
tests. The desktop parity run first used another worktree's CLI binary; building
this branch's CLI before the rerun resolved that schema mismatch.

The post-green simplify-code pass found no worthwhile simplification. Scoped
Rust regressions passed again through the compiled test binary, and frontend
checks and hook tests passed again. Final diff review found no further changes.
