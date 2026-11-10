# PR 493 read-only history audit

## Changes

Scans, frontmatter repair previews, Park git warnings, and CLI target resolution use a non-migrating history guard. Mutation sessions keep the migrating guard. The CLI and MCP can therefore retain their tool lookup and history-capable runtimes for `outdated` without migrating history during its scan.

CLI and MCP fixture readers and writers both use `.skill-studio/leases`, outside `.history`. Live scope leases already use a sibling of history.

## SQLite limitation retained

Finding 3 remains open, under the requested fallback rule. The host event reader and desktop Park-time reader still use `SQLITE_OPEN_READ_ONLY`. A closed WAL database can create `events.sqlite3-wal` and `events.sqlite3-shm` when queried. This was reproduced with a temporary closed WAL database.

The folder guard is shared for the lifetime of writable stores. It prevents a format migration but permits SQL writes. Adding `immutable=1` under that guard would be unsafe. Copying the database and WAL under that same guard would also permit an inconsistent copy. Making read guards exclusive would reject reads while the desktop's long-lived writable store is open. This change does not introduce either behavior. A complete solution needs coordination around SQLite transactions, or a snapshot mechanism that also prevents changes to source side files.

SQLite documents both [read-only WAL side files](https://www.sqlite.org/wal.html#read_only_databases) and the [immutable URI contract](https://www.sqlite.org/uri.html#uriimmutable).

No passing WAL no-write regression is claimed. The requested absent/unversioned cases cover history without a database; they do not establish that existing WAL databases are free of read side effects.

## CLI and MCP cases

The table test runs each row against both absent and unversioned fixture history, compares history filenames and bytes after each call, and checks that each call returns an operation envelope. A real manual skill supplies a deployment id for preview calls. The initial scan must succeed and leave absent history absent. The unversioned folder contains a sentinel that must stay unchanged.

| CLI command           | MCP tool                     | History path                                                               |
| --------------------- | ---------------------------- | -------------------------------------------------------------------------- |
| `scan`                | `scan`                       | Non-migrating scan guard; shared lease outside history                     |
| `diagnose`            | `diagnose`                   | Scan                                                                       |
| `conflicts`           | `diagnose_conflict`          | Scan                                                                       |
| `usage`               | `skill_usage`                | Scan; usage cache only read                                                |
| `capabilities`        | `capabilities`               | No history store                                                           |
| `harnesses`           | `harnesses`                  | No history store                                                           |
| `preview-repair`      | `preview_frontmatter_repair` | Non-migrating guard; shared lease outside history                          |
| `events`              | `list_events`                | Non-migrating store open; SQLite limitation above                          |
| `doctor`              | `doctor`                     | Scan                                                                       |
| `outdated`            | `outdated`                   | Scan with a history-capable runtime                                        |
| `install-preferences` | `install_preferences`        | Registry read                                                              |
| `turn-off --check`    | `turn_off_check`             | Scan                                                                       |
| `health`              | None                         | Timing log read                                                            |
| `watch`               | None                         | Scan; test reads the initial inventory, then kills and waits for the child |

A second table covers empty live homes, without `gh`, for CLI and MCP `outdated`, with absent and unversioned history. Every subprocess uses a temporary HOME. `schema` writes schema files and is not a read-only command. `mcp` is the transport for the tested tools. Help and version flags do not access history.

## Desktop command sweep

These read commands reach scans or inventory rebuilding: `get_installed_skills`, `skill_local_edits`, `turn_off_check`, and the background refresh that supplies `get_skill_snapshot`. They use the desktop scan lease outside history and the non-migrating format guard. Refresh also reads Park event times, subject to the SQLite limitation above.

`park_check` uses the core scan through a history-capable runtime. `split_skill_targets` only calculates destinations. `preview_skill_frontmatter_repair` reads the cached snapshot and skill files. None opens a mutation session for its preview.

`list_skill_events` queries the writable store held since startup. Startup itself is a mutating operation, with the version check before opening that store. This command does not open a second SQLite connection.

The following read commands do not open a history store or create history leases: `app_version`, `data_folder_status`, `get_add_method_defaults`, `get_add_skill_operation`, `detect_harnesses`, `get_harnesses_choice`, `get_update_status`, `get_editor_choices`, `get_telemetry_enabled`, `get_tracked_projects`, `get_discovery_sources`, `get_skill_snapshot`, `read_installed_skill_md`, `list_project_folders`, `list_skill_packs`, `list_skill_runs`, `read_skill_run_events`, and `skill_run_target_diff`.

Remote readers `search_skills`, `get_popular_skills`, `get_skill_details`, `get_install_counts`, `list_github_skills`, and `check_for_update` do not use history. Some cache results or write timing logs outside history. `check_skill_updates_now`, `request_skill_rescan`, and `rescan_skills_now` request cache or snapshot updates and are not pure read commands; their scan/history reads use the same paths listed above.

## Regression checks

`empty_outdated_preserves_absent_and_unversioned_history_without_gh_or_migrates_it` failed when the SQLite opener's non-migrating guard was removed: CLI `outdated` created missing history. With the guard restored, it passed for both surfaces and both history states.

`read_only_cli_and_mcp_preserve_absent_and_unversioned_history_or_name_the_writer` failed when fixture leases were returned to `.history/leases`: the first scan created a history subdirectory. With both lease paths restored, it passed. Its snapshot comparison includes directories and file bytes recursively.

## Build environment

All Cargo runs used `DEVELOPER_DIR=/Library/Developer/CommandLineTools` and `CARGO_TARGET_DIR=/Users/sergiydybskiy/src/skill-studio/target`. The first Rust build followed the requested source touch. The default profile waited on other worktrees' Cargo locks. Validation then used a private profile inside the same shared target folder:

```sh
profile=(--profile pr493 --config 'profile.pr493.inherits="dev"' --config profile.pr493.debug=0 --config profile.pr493.incremental=false)
```

No unique compiler metadata was needed. Full test runs used a temporary HOME, with CARGO_HOME and RUSTUP_HOME kept at their existing toolchain paths. The parallel core run failed `park_sweeps_stale_trash_folders_in_the_roots_it_touches` with data-folder lock contention. The full core rerun uses `--test-threads=1`.

## Validation

The completed validation pass passed all commands below. Cargo commands use the profile arguments listed above.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features "${profile[@]}" -- -D warnings
cargo test -p skill-studio-core --all-features "${profile[@]}" -- --test-threads=1
cargo test -p skill-studio-host --all-features "${profile[@]}"
cargo test -p skill-studio-cli -p skill-studio-mcp --all-features "${profile[@]}"
cargo test -p skill-studio --lib --all-features "${profile[@]}" -- --test-threads=1
cargo test -p skill-studio --all-features "${profile[@]}" --test core_scan_parity --test park_parity --test restore_parity --test undo_activity_history --test fix_parity
pnpm run typecheck && pnpm run lint && pnpm run format:check
```

Results: 736 core tests passed, with one existing timing test ignored; 243 host tests passed; 114 CLI/MCP tests passed; 799 desktop library tests passed; 37 desktop parity tests passed. The known host discovery flake did not occur.

The initial parallel desktop library run had three data-folder lock contention failures: `refresh_marks_a_skill_parked_from_its_parked_deployment_without_a_registry_record`, `the_registry_a_real_split_wrote_reads_back_with_its_copies`, and `update_fetches_once_and_writes_live_copies_but_not_the_parked_one`. The full serial rerun passed all 799 tests. These failures were not ignored.

The desktop parity helper now accepts `SKILL_STUDIO_TEST_BINARY`, like the CLI and MCP harnesses, so a separate shared-target profile does not accidentally test another worktree's debug binary. Both existing CLI contention tests use the new fixture lease location.

Post-check simplification found no worthwhile simplification. The read guard is a separate operation from the mutation guard, and the existing in-memory and filesystem history adapters need different implementations. Final review retained that distinction. No simplification edits were made.
