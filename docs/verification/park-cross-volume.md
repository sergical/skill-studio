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
