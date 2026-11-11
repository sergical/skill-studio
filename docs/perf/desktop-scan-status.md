# Desktop scan completion and startup dispatch

The release baseline is in PR #477. Its 378-skill fixture has a fresh core
median of 76.72 ms and p95 of 112.33 ms. A fixture is not the owner's live
home. These numbers do not prove a 100 ms real-home target.

A slow refresh keeps the published inventory visible. Skills and Activity
show “Still scanning” while scan or invocation work runs. A failed refresh
returns an error to the caller and leaves the previous snapshot visible.
`get_background_status` and `skills://background-status` report each scan,
invocation refresh, and update check's running state, timestamps, and last
error. Overlapping runs remain busy until both finish.

## Proof

- `skill_refresh.rs::scan_over_budget_keeps_the_last_snapshot_and_never_publishes_an_empty_list`
  scans a real 400-skill fixture. A logical clock makes the second scan exceed
  its budget. It sends the partial inventory through the same merge and
  publication path as production. Removing that merge makes the test fail.
- `skill_refresh.rs::failed_rebuild_returns_its_error_and_preserves_the_published_snapshot_or_silently_clears_the_list`
  fails if a failed build publishes or loses its error.
- `background_work.rs` tests cover errors and overlapping workers.
- `ScanProgress.test.tsx` renders the real Skills and Activity views. The
  hook tests cover failed requests and unmounted views.
- `timing_log.rs::every_tauri_command_dispatches_off_main_or_names_the_command_that_can_block_the_ui`
  checks every Rust command source. A synchronous command must have Tauri's
  async dispatch attribute. Its parser test covers intervening comments and
  attributes. Cached snapshot cloning and its timing log use the blocking
  pool, as do scan requests. This rule has no synchronous-command allowlist.
- `skill_refresh.rs::a_harness_removed_in_first_run_is_never_read_for_activity`
  checks both bytes and files read, with an enabled-agent positive control.

Usage parsing and aggregation now live in the core `skill_uses` module.
It has no Tauri or `std::fs` dependency. The host owns transcript I/O and the
incremental index; the desktop applies the saved first-run agent choice to
both full and invocation-only refreshes. An unreadable choice fails closed.

The tests use a logical budget, not a wall-clock CI limit. The reference
hardware, raw samples, p95, and maximum stay in the baseline report. The
owner's live-home measurement and final reference budgets remain open.
