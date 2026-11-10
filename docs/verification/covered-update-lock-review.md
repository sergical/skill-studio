# Covered update lock restore review

`covered_update_undo_preserves_another_lock_row_edited_during_folder_restore_or_loses_the_edit`
injects an unrelated lock-row edit during the folder swap. On the merged branch
head it failed: `new-delta` replaced `external-delta`. The test uses a temporary
fixture home and a filesystem callback, with no delays or wall-clock inputs.

The fix captures the current lock at commit time and applies only the saved
skill row. It retains that captured file beside agents.lock. The proposed lock
is published with a hard link, which fails if a writer recreated agents.lock.
Failure recovery also uses a hard link and cannot replace an existing file.
The Restore payload records the displaced path. The staged proposal and captured
lock remain available after a conflict. The published file keeps the captured
lock's permission bits.

`covered_update_undo_refuses_a_recreated_lock_and_keeps_both_versions_or_overwrites_the_writer`
injects that recreation between capture and publication. It checks refusal,
unchanged writer bytes, and recovery of the displaced version.

No shared exchange helper was available on origin/main or the Park branch when
this change was prepared. Exclusive publication needs a small filesystem-port
method; the host implements it with std::fs::hard_link.

The required gates passed before and after the simplify-code pass:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p skill-studio-core --all-features
```

The final core run passed 727 tests. One attempt waited before entering a test
binary; its signature was valid and `--list` also waited. That attempt was
stopped, and the full core command passed on retry without code changes.

The simplification removed the unused full-lock preflight snapshot and a
one-use parser wrapper. Saved-row validation still runs before folder writes.
Final review checked row drift, exclusive publication, recovery without
replacement, retained bytes, and permission preservation. No further change
was needed. CLI and MCP code was not touched.

## Follow-up P1 review

The first Update all result now points to its own folder backup and lock-row
inverse. A separate event, marked `full_install_recovery`, retains the complete
install backup. The Undo-order regression checks A then B and B then A. Each
step checks both folders and both lock rows.

The open-descriptor regression opens agents.lock before capture and writes an
unrelated row through that descriptor after the restore proposal is staged,
and again in a separate case at publication. Restore checks the captured inode
before and after publication against the exact bytes used for the merge. Changed bytes cause
DriftConflict, retain the captured file, and report its path.

The Restore payload journals the displaced path before rename. Recovery links
that file back only if agents.lock is absent. An existing file with a different
inode causes DriftConflict. Recovery handles pending restores and interrupted
restores that still hold a claim, including rows already reconciled by desktop
startup. The raw claim query retains support for old harness labels.

All four new tests failed with the production fixes removed. The descriptor
case reported success with stale bytes. Capture recovery left agents.lock
missing, and recovery accepted a replacement without reporting a conflict.
The Undo-order case restored another skill. The tests use fixture homes and
filesystem callbacks, with no sleeps or wall-clock gates.

Completed Restore rows keep the captured lock's identity and bytes until the
next mutation checks it. A late write through an old descriptor returns
DriftConflict with the retained path. An unchanged capture is then removed.
Rollback links sync the parent directory. Commit errors keep the pending
Restore and its claim for recovery. Force Undo can journal an absent original
lock and publish the saved row into the empty name.

The targeted `covered_` tests include late descriptor writes after the final
read, rollback sync, failed rollback recovery, and Force Undo after deletion.
