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

The implementation is in the parent race-fix PR. This PR keeps the full layout
matrix and surface coverage separate. No production code or timing claim.

Scoped core, CLI, and MCP package tests passed with all features, including all
32 layout variants on every surface. Rust formatting and scoped Clippy passed.
Post-green simplification found no worthwhile simplification. The same gate is
run again before push.
