# Dotagents layout round trip verification plan

| Issue | User flow                          | Expectation                | Proof (file::test or command)                                                                                                 | Red before fix?           | Measured numbers |
| ----- | ---------------------------------- | -------------------------- | ----------------------------------------------------------------------------------------------------------------------------- | ------------------------- | ---------------- |
| #404  | Park then Turn on each TOML layout | Restore all original bytes | park_install_aware::park_turn_on_roundtrips_agents_toml_layouts_byte_for_byte_or_names_the_layout; CLI/MCP layout table flows | Yes on all three surfaces | No timing claim  |

Eight layouts each run with LF or CRLF and with or without a final newline.
They cover comments, a header comment, inline metadata tables, odd whitespace,
a multiline string, a same-line array close, both empty-array forms, an existing
exclusion, and three wildcard suppliers. Expectations come from original fixture
bytes. The core legacy remover fake models its no-op on commented headers and
CRLF. Current Park does not call that remover.

The core test failed against origin/main's old implementation on the first
commented header: Park returned an error because the legacy remover left foo
listed. Only its private helper name was adapted to the current caller for
that check. CLI and MCP also failed with the legacy remover; their sentinel
executable returned 79 when called. The production code was then restored.

The parent race-fix PR removes the layout-sensitive external remover. This PR
adds the layout matrix and keeps earlier exclusions safe when config edits
require a merge. It makes no timing claim.

The initial core, CLI, and MCP package gate passed with all features, including
all 32 variants per surface. Rust format and scoped clippy passed.

| Issue | User flow                                            | Expectation                                                                                | Proof (file::test or command)                                                                                                      | Red before fix?                                      | Measured numbers |
| ----- | ---------------------------------------------------- | ------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------- | ---------------- |
| #404  | Edit config while identical wildcard keys are parked | Refuse before changing config, lock, ignore or copy; preserve reviewed exclusions on retry | park_install_aware::turn_on_refuses_ambiguous_wildcards_then_preserves_reviewed_exclusions; CLI envelope and MCP stdio equivalents | Yes; core, CLI and MCP removed the earlier exclusion | No timing claim  |

Changed configs use a row key only if it was unique before Park and still names
one row. Older records can establish that from their backup. Ambiguous rows
stay untouched when the skill is already listed; otherwise Turn on asks for
exclude review. New records retain key uniqueness even if a backup is missing.
The core case also covers deleting the supplying rows while retaining the row
that already excluded the skill. MCP repeats the retry after restarting its
stdio server.

The 58 core Park tests and CLI/MCP ambiguity flows passed. The post-green pass
shared the key-uniqueness check between planning and legacy-record restoration.
Scoped clippy and Rust format passed again after that cleanup. No events.rs
changes.

The final package repeat was stopped after macOS repeatedly held test processes
at `_dyld_start` before Rust started. A one-second sample of the fsops process
showed only that loader frame and a 96 KiB footprint after 32 seconds. The run
had no test failure. Exact Park integration targets and scoped clippy were run
separately after simplification. CI uses its own build folder for the full gate.
