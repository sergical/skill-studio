# Park display verification

| Issue | User flow                                                       | Expectation                                             | Proof (file::test or command)                                                    | Red before fix?                                           | Measured numbers |
| ----- | --------------------------------------------------------------- | ------------------------------------------------------- | -------------------------------------------------------------------------------- | --------------------------------------------------------- | ---------------- |
| #408  | Park a project dotagents, wildcard, or in-repo copy; scan/watch | Show its project origin and original owner while parked | core park_any_copy table; CLI display golden and first watch inventory; MCP scan | Yes; all new surface tests failed before production edits | No timing claim  |
| #408  | Turn on a Claude Code copy                                      | Result names the restored deployment                    | core, CLI golden, MCP stdio                                                      | Yes; all new surface tests failed before production edits | No timing claim  |
| #408  | Park a plugin by name                                           | Same actionable /plugin refusal as --id                 | CLI display golden                                                               | Yes; all new surface tests failed before production edits | No timing claim  |
| #408  | Copy update after split                                         | Explain how to update agent copies in the desktop app   | CLI display golden; MCP update refusal                                           | Yes; all new surface tests failed before production edits | No timing claim  |

The parked root remains the global holding location. `parked_origin` supplies
the project and agent for display. Owner metadata sits beside each parked copy,
not inside its user files. Existing copies without metadata keep legacy scan
classification. This change does not read history during scan.

Red runs: core had 36 passing and three failing tests; CLI had four failing
goldens; MCP had five passing and three failing tests. All failures named the
expected owner, restored ID, or guidance difference. The inherited MCP watch/repair test also passed in the red run.

The first green run passed 39 core Park tests, four CLI display goldens, and
seven MCP Park tests. Format and scoped clippy passed. Simplification then
replaced a nested optional read with a parent/filter/map chain. Full scoped
package tests, format, and clippy are the final gate.

Owner snapshots use the second line of the existing origin note, outside the
skill folder. The first line keeps the original folder spelling. Old one-line
notes still work. Existing backup and restore code carries the whole note.
No events.rs changes. The physical parked root remains global; parked_origin
and the human scan line show the original project.

An earlier full scoped run passed core and CLI, and 25 of 26 MCP tests. The
remaining test was main's unchanged
`watch_and_a_fresh_mcp_scan_agree_after_a_cli_mutation`, which hit its five-second
receive timeout while nested Cargo builds waited on shared locks. Its function
matches origin/main byte for byte, SHA-256
2466674e049b7de6b9110222b3ec9e0482a71129903285ca5aa5a5b71f997b1b.
It passed in the isolated #398 retry and in this branch's red run. Its fixture
has no parked copy, and its flow calls scan and frontmatter repair. CI runs the
complete suite without the shared local build folder.

Two existing origin-note assertions now check the first line, which still
names the original folder. The new flow tests check owner preservation.

An earlier clippy repeat read foreign core metadata from the shared target. It
reported that ScopeFs lacks this branch's fsops_full_sync_file,
fsops_set_directory_mode and fsops_preserves_modes methods, and required
held_restore_claims/HeldRestoreClaim, which do not occur in this tree. Recursive
source search confirmed both facts. The earlier scoped clippy run passed before
the read-chain simplification and origin-note assertion update. These foreign
metadata errors are ignored under the lead's shared-target rule. CI is the
isolated full gate. Rust format and changed-document checks passed.

| Issue | User flow                        | Expectation                                                            | Proof (file::test or command)                                                                                           | Red before fix?                                                 | Measured numbers |
| ----- | -------------------------------- | ---------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------- | ---------------- |
| #408  | Scan/watch a parked project copy | Keep original source type and report the current physical Git location | park_any_copy::parked_scan_keeps_the_project_source_type_but_reports_the_current_git_location; same CLI/MCP owner flows | Core, CLI and MCP all failed while the physical flag was frozen | No timing claim  |

The correction keeps only owner kind and ID in the origin note. The Git flag
comes from the current folder. All 40 core Park tests, four CLI goldens, seven
MCP Park tests, Rust format, and scoped clippy passed on main 4d1d4d52. The
post-green pass found no further worthwhile simplification. The final scoped
package run covers core, host, CLI, and MCP.

The final core, host, CLI and MCP package run passed all tests on main 4d1d4d52.
The repeated four-package clippy run again read a ScopeFs metadata file without
this branch's three filesystem methods. The immediately preceding scoped clippy
run passed. CI remains the isolated full gate.
