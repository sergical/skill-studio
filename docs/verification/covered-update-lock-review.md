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
