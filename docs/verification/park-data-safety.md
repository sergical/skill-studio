# Park data safety verification plan

The checkout has no `docs/state-machines.md`. Read `docs/flows.md` and
`docs/spec-headless-performance-observability.md` before implementation.
Issue bodies and comments were read with GitHub after `gh issue view N --comments`
returned empty output.

| Issue | User flow                                | Expectation                                 | Proof (file::test or command)                                                                 | Red before fix? | Measured numbers                                           |
| ----- | ---------------------------------------- | ------------------------------------------- | --------------------------------------------------------------------------------------------- | --------------- | ---------------------------------------------------------- |
| #447  | Park while dotagents installs            | A new live folder survives                  | Core fs/process fake at unlist step; CLI park/unpark; MCP park/unpark                         | Pending         | Pending                                                    |
| #448  | Failed Park with another config writer   | Keep the other writer's bytes               | Core fs fake at rollback; CLI park/unpark; MCP tool                                           | Pending         | Pending                                                    |
| #418  | Turn on after another wildcard exclusion | Keep unrelated exclusion                    | park_install_aware::turn_on_keeps_unrelated_wildcard_exclusion_added_during_remove            | Pending         | Not a timing change                                        |
| #418  | Turn on after ignore neighbours change   | Refuse before any write                     | park_install_aware::turn_on_refuses_changed_ignore_anchor_without_changing_files; CLI and MCP | Pending         | Not a timing change                                        |
| #404  | Park then Turn on unusual TOML           | Restore exact bytes                         | Table of layouts in core and CLI                                                              | Pending         | Pending                                                    |
| #398  | Cross-volume Park then Turn on           | Durable files and preserved supported modes | Core fs fake, real second volume if available; CLI and MCP                                    | Pending         | Release N >= 10, 1 and 100 skills, median/p95 before/after |
| #408  | Park display in scan and unpark          | Keep project, owner and agent identity      | CLI golden tests                                                                              | Pending         | Not a timing change                                        |
