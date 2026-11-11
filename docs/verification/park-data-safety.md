# Park data safety verification plan

The initial checkout did not contain `docs/state-machines.md`. Read the flow
and performance specs before implementation, and the state-machine document
after it appeared on main.
Issue bodies and comments were read with GitHub after `gh issue view N --comments`
returned empty output.

| Issue | User flow                                | Expectation              | Proof (file::test or command)                                                                        | Red before fix?          | Measured numbers |
| ----- | ---------------------------------------- | ------------------------ | ---------------------------------------------------------------------------------------------------- | ------------------------ | ---------------- |
| #418  | Turn on after another wildcard exclusion | Keep unrelated exclusion | park_install_aware::turn_on_keeps_unrelated_exclusion_added_during_remove_or_erases_user_edit        | Core test failed on base | Not measured     |
| #418  | Turn on after ignore neighbours change   | Refuse before any write  | park_install_aware::turn_on_refuses_changed_rule_anchors_or_overrides_external_negation; CLI and MCP | Core test failed on base | Not measured     |

## Evidence for issue #418

Both new core regressions failed before the production edit. The exclusion
test lost `foo` from the unrelated row. The ignore-anchor test returned success
where it expected refusal. After the edit, all 51 tests in
`park_install_aware` passed.

The CLI and MCP tests cover both cases under temporary HOME folders. Both
passed after a fresh core build. A later controlled check restored main
b76dd207's original dotagents module while keeping these tests and fixtures.
Both surface tests failed: their output showed the unrelated exclusion replaced
with `exclude = []`. The fixed module was then restored byte for byte. An
earlier stale-artifact result is excluded from this proof. The frozen sync
timing baseline already includes the dotagents fixes, so it is also excluded
from before-fix evidence.

The frontend gate, Rust format check, and workspace Clippy passed. The first
workspace test run executed two Remove tests absent from this worktree:

- `remove_with_unreadable_cli_backup_link_target_refuses_before_spawn_or_loses_the_secondary_link`
- `remove_with_unreadable_cli_backup_metadata_refuses_before_spawn_or_loses_the_secondary_copy`

A recursive source search confirmed neither test exists here. They belong to
another worktree's tests for #452 and reached this run through the shared Cargo
build folder. Ignore those results for this branch. CI uses a separate build
folder. Subsequent Park checks use exact package and integration-test targets
and force recompilation of this worktree's test source.

Post-green simplification review found no worthwhile simplification. The shared
CLI/MCP fixture avoids duplicate setup; the production changes add no new
interface. No timing result is claimed for #418.

## P2 follow-up regressions

Turn on now checks each restored rule against the rules adjacent to its actual
insertion point. The ignore-anchor regression includes a surviving `unrelated`
rule and a header moved after a new external negation. It requires DriftConflict
with config files and the parked folder unchanged.

`turn_on_keeps_rule_added_between_reads_or_discards_editor_change` edits the
ignore file during the config write, between the two ignore reads. It requires
that the added `/editor-rule` survives. The hook runs once without sleeps.

Both cases failed with their respective fixes reverted. The anchor test returned
success, and the between-read test lost `/editor-rule`.
