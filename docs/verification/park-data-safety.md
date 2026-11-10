# Park data safety verification plan

The checkout has no `docs/state-machines.md`. Read `docs/flows.md` and
`docs/spec-headless-performance-observability.md` before implementation.
Issue bodies and comments were read with GitHub after `gh issue view N --comments`
returned empty output.

| Issue | User flow                                | Expectation                                 | Proof (file::test or command)                                                                 | Red before fix?          | Measured numbers                                           |
| ----- | ---------------------------------------- | ------------------------------------------- | --------------------------------------------------------------------------------------------- | ------------------------ | ---------------------------------------------------------- |
| #447  | Park while dotagents installs            | A new live folder survives                  | Core fs/process fake at unlist step; CLI park/unpark; MCP park/unpark                         | Pending                  | Pending                                                    |
| #448  | Failed Park with another config writer   | Keep the other writer's bytes               | Core fs fake at rollback; CLI park/unpark; MCP tool                                           | Pending                  | Pending                                                    |
| #418  | Turn on after another wildcard exclusion | Keep unrelated exclusion                    | park_install_aware::turn_on_keeps_unrelated_wildcard_exclusion_added_during_remove            | Core test failed on base | Not measured                                               |
| #418  | Turn on after ignore neighbours change   | Refuse before any write                     | park_install_aware::turn_on_refuses_changed_ignore_anchor_without_changing_files; CLI and MCP | Core test failed on base | Not measured                                               |
| #404  | Park then Turn on unusual TOML           | Restore exact bytes                         | Table of layouts in core and CLI                                                              | Pending                  | Pending                                                    |
| #398  | Cross-volume Park then Turn on           | Durable files and preserved supported modes | Core fs fake, real second volume if available; CLI and MCP                                    | Pending                  | Release N >= 10, 1 and 100 skills, median/p95 before/after |
| #408  | Park display in scan and unpark          | Keep project, owner and agent identity      | CLI golden tests                                                                              | Pending                  | Not measured                                               |

## Evidence for issue #418

Both new core regressions failed before the production edit. The exclusion
test lost `foo` from the unrelated row. The ignore-anchor test returned success
where it expected refusal. After the edit, all 51 tests in
`park_install_aware` passed.

The CLI and MCP tests each cover both cases under a temporary HOME. Both passed
after a fresh core build. An earlier CLI run used a stale shared build artifact
and failed. No claim of a controlled baseline run is made for that run.

The frontend gate, Rust format check, and workspace Clippy passed. The full
Rust test suite failed in two Remove tests outside this diff:

- `remove_with_unreadable_cli_backup_link_target_refuses_before_spawn_or_loses_the_secondary_link`
- `remove_with_unreadable_cli_backup_metadata_refuses_before_spawn_or_loses_the_secondary_copy`

Both failures say the CLI ran despite unreadable backup input. The complete log
is local at `/tmp/park-workspace.log`. This run did not establish whether those
failures also occur on the base branch. No Remove code was changed.

The full gate is red, so the required post-green simplification, push, PR,
and CI wait have not run. No timing result is claimed. The F_FULLFSYNC change
has not started. Issues #447, #448, #404, #398, and #408 remain unimplemented.
