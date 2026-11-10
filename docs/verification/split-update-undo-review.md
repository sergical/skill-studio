# Split update Undo review

The regression run used a temporary fixture home and a pass-through
`RUSTC_WORKSPACE_WRAPPER` to keep this worktree's artifacts distinct from
concurrent builds in the shared target folder. It used no delays or real HOME.

- `split_update_then_undo_keeps_a_copy_edited_after_backup_or_overwrites_the_refused_edit`
  injects an edit after the update backup exists. Before the fix, Undo returned
  success but replaced the editor bytes with v1. After the fix, Undo restores
  only the copies the update changed.
- `split_update_undo_redo_allows_the_next_update_or_the_registry_hash_refuses_unedited_copies`
  runs Update, Undo, redo, and Update again. Before the fix, the last Update
  refused all three unedited copies. The Restore inverse now carries the guarded
  registry rows, so redo restores the hashes with the files.

The isolated red command was:

```sh
cargo test -p skill-studio-core --all-features --test split_copy_update split_update_ -- --nocapture
```

It reported two failures at the assertions above. The full split-copy test
binary then passed all 12 tests. Interrupted split updates that have not recorded
their changed paths refuse Undo and retain their backups for inspection.

The required gates passed before and after the simplify-code pass:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p skill-studio-core --all-features
```

The final core run passed 726 tests. The simplification removed the unused
optional registry-undo branch and the second candidate lookup. Final review
checked the primary-copy refusal, secondary-copy exclusion, registry guards,
and interrupted-update refusal. No further change was needed. CLI and MCP
code was not touched by these review fixes.
