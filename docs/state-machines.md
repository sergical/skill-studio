# State machines

This file lists the state machines in Skill Studio: where each one lives, its
states and legal moves, and the test that guards it. Read it before you change
a status field, a phase enum, or a busy flag.

## Rules

1. **One owner per machine.** One function decides the next state. Callers ask
   it; they do not set the field.
2. **Name every legal move.** Write the moves as a `match` (Rust) or a
   discriminated union (TypeScript). An illegal move returns an error, not a
   silent overwrite.
3. **Test the whole table.** A machine test walks every `(from, to)` pair, or
   every input row, and names the illegal move in its failure message.
4. **Derive, do not mirror.** If a state follows from other state, compute it.
   Do not store a second flag that can drift from the first.
5. **Keep the wire stable.** Serialized names (`kebab-case` phases, SQL status
   strings) do not change. New states are additive.

## Explicit machines

| Machine                | Where                                                                         | States and moves                                                                                                                                                                                                                                                                                                                                                                                                                                                           | Guard test                                                                                                                                                                                                                             |
| ---------------------- | ----------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Event row lifecycle    | `crates/skill-studio-core/src/events.rs` (`EventStatus::settle`)              | `pending → done \| failed \| interrupted`; `interrupted → done \| failed` (recovery); a move to the current state is a no-op; nothing moves to `pending`. Both the host and the desktop event stores use it.                                                                                                                                                                                                                                                               | `an_event_row_only_moves_along_its_lifecycle_or_names_the_illegal_move`                                                                                                                                                                |
| Desktop event store    | `apps/desktop/src-tauri/src/skills/event_store.rs` (`finish`)                 | Reads the row and settles it through the core rule in one transaction.                                                                                                                                                                                                                                                                                                                                                                                                     | `an_event_settles_once_and_recovery_settles_interrupted_rows_or_names_the_overwrite`                                                                                                                                                   |
| Journal plan status    | `crates/skill-studio-core/src/ports.rs` (`PlanStatus::settle`)                | `pending → done \| failed \| interrupted`; `interrupted → interrupted \| failed \| reversed`; the other states are terminal. `pending` is never a target.                                                                                                                                                                                                                                                                                                                  | `journal.rs` tests                                                                                                                                                                                                                     |
| History store finish   | `crates/skill-studio-host/src/history.rs` (`SqliteHistoryStore::finish`)      | Reads the row and settles it through `EventStatus::settle` in one transaction. A repeat of the current status is a no-op. Recovery may settle an interrupted event. Journal recovery may retry an interrupted plan and settle it as failed or reversed.                                                                                                                                                                                                                    | `history.rs` tests                                                                                                                                                                                                                     |
| Restore claim          | `crates/skill-studio-core/src/events.rs` (`recover_interrupted`)              | Each mutation session releases the `target_event` claim of every `pending` or `interrupted` Restore row, before it marks a `pending` row `interrupted`, so a crash between the two writes repeats both. A restore of a `make_independent_copy` row keeps its claim, because desktop startup resumes it. Recovery reads only the ids and the target kind (`held_restore_claims`), so rows that older desktop builds wrote with display-name harness labels do not block it. | `a_restore_cut_off_by_a_crash_releases_its_target_claim_or_the_target_stays_reverted`                                                                                                                                                  |
| Add Skill operation    | `apps/desktop/src-tauri/src/skills/skill_add_operation.rs` (`can_advance_to`) | `queued → validating → [fetching] → installing… → reconciling → completed \| failed \| needs-trust`; `needs-trust → needs-trust (trusted) \| cancelled`; `queued → cancelled`; work phases `→ failed`. No Rust path enters `finalizing` or `timed-out`.                                                                                                                                                                                                                    | `an_add_operation_only_moves_forward_or_names_the_illegal_move`                                                                                                                                                                        |
| Snapshot refresh loop  | `apps/desktop/src-tauri/src/skills/skill_refresh.rs` (`next_refresh_step`)    | One `RefreshTick` in, one `RefreshStep` out: `Idle`, `Full`, `InvocationsOnly`, `HourlyWindows`. A failed rebuild keeps its dirty flags and waits 5 s before the next try.                                                                                                                                                                                                                                                                                                 | `each_refresh_tick_picks_the_rebuild_its_state_needs_or_names_the_wrong_step`                                                                                                                                                          |
| List selection         | `apps/desktop/src/store/appStore.ts`                                          | Selection mode is `selectedSkillPaths.size > 0`. There is no separate flag.                                                                                                                                                                                                                                                                                                                                                                                                | `appStore.test.ts`                                                                                                                                                                                                                     |
| Home "Update all" stop | `apps/desktop/src/hooks/skillBatchUpdates.ts` (`stopHomeUpdateAll`)           | `running → stopping → stopped`. The stop request is a promise; each phase boundary waits for it. A Cancel that does not reach the backend returns the batch to `running`, so Cancel shows again and no work is skipped.                                                                                                                                                                                                                                                    | `a_cancel_that_fails_to_reach_the_backend_keeps_the_whole_batch_running_or_the_ui_offers_cancel_while_it_stops`, `a_cancel_still_in_flight_at_a_phase_boundary_decides_nothing_until_it_lands_or_the_batch_skips_work_nothing_stopped` |
| Confirm dialog busy    | `packages/ui/src/components/alert-dialog.tsx` (`busy`)                        | `open → busy → closed`. While `busy`, Escape and outside clicks do not close the dialog. Used by Remove, Uninstall plugin, Park copy, Left behind, and the store Remove confirm.                                                                                                                                                                                                                                                                                           | `AlertDialogBusy.test.tsx`                                                                                                                                                                                                             |

## Checked and left as is

| Item                                             | Where                                                                    | Why it stays                                                                                                                                                                                             |
| ------------------------------------------------ | ------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Double click on Overwrite or Fork                | `apps/desktop/src/hooks/useGuardedSkillUpdate.tsx`                       | React 19 commits the first click before the second runs, so the second click sees no pending update. A test that clicks twice runs one update.                                                           |
| Update-all cancel flag after the batch ends      | `apps/desktop/src-tauri/src/skills/commands.rs` (`UpdateAllCancelState`) | A Cancel that arrives after its batch ended leaves one small map entry. The map must create flags for a Cancel that arrives before the batch starts, so it cannot tell the two apart without more state. |
| Selection-bar Remove and Discard changes dialogs | `SkillListSelectionBar.tsx`, `DiscardChangesDialog.tsx`                  | They close before their action runs, so they have no busy state.                                                                                                                                         |

## Still implicit

These machines still use loose booleans or untyped status strings. Each one
works today. Make one explicit when you next change it.

| Machine                   | Where                                                           | Shape today                                                                                                                          |
| ------------------------- | --------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| Keyed async loads         | `apps/desktop/src/hooks/useSkillSplit.ts`, `useSkillTurnOff.ts` | `loading` and `data` booleans per hook. A shared `{ loading \| ready \| failed }` union keyed by request would remove stale results. |
| Row cursor                | `apps/desktop/src/hooks/useRowCursor.ts`                        | Index plus focus flags.                                                                                                              |
| Skill snapshot first read | `apps/desktop/src/hooks/useSkillSnapshot.ts`                    | `undefined` means "not loaded yet".                                                                                                  |
| Skill page actions        | `apps/desktop/src/components/SkillDetail/skill-page-actions.ts` | One `busy` flag per action; two actions can run at once.                                                                             |
| Add Skill sheet submit    | `apps/desktop/src/components/AddSkill/AddSkillSheet.tsx`        | Form step, operation id, and trust prompt held in separate `useState` calls.                                                         |
| Update check              | `apps/desktop/src-tauri/src/skills/skill_update_check.rs`       | Check results land in a shared store with no request generation.                                                                     |
| Write lease               | `apps/desktop/src-tauri/src/skills/write_lease.rs`              | Held or free, tracked by lock files.                                                                                                 |

## Backup expiry

The constants `BACKUP_RETENTION_COUNT` (20) and `BACKUP_RETENTION_AGE` (30 days)
live in core. A completed backup stays if it is among the newest 20 backups for
its skill target, is younger than 30 days, belongs to that target's newest
undoable event, or is needed by a pending or interrupted restore or plan.
Only a backup outside all four protections expires. Unknown legacy targets and
unfinished rows stay protected.

Expiry does not change event or plan status. Event expiry clears `backup_dir`
and records `backup_expired` in the same SQLite transaction that queues deletion.
Plan expiry records the relative backup name in `expired_backups` through the
journal's durable write before unlinking it. Reads return `backup_expired` even
if cleanup has not yet finished. Startup retries the deletion under the target's
write lease. Rows and expiry reasons stay in history.

`history::tests::retention_crash_at_each_durable_step_preserves_files_and_recovery_or_exposes_a_partial_backup`
and `journal::tests::completed_plan_retention_crash_at_every_durable_step_recovers_or_leaves_an_available_partial_backup`
cut every filesystem mutation and flush, then run recovery.

## Startup recovery readiness

`EventStoreState` publishes one empty `OnceLock<StartupRecoveryOutcome>` before
commands can run.
The recovery worker holds the event store and waits for the home and every
trusted project write lease. The timing wrappers wait for that same once-only
gate. The worker publishes success or failure before releasing the store. Its
drop guard publishes failure on unwind, so commands return a reason instead of
waiting forever. After startup,
commands do not wait on an active write's event-store mutex to cancel it.

`a_command_during_startup_waits_for_recovery_or_reads_the_unrecovered_store`
uses channels to hold recovery while a command arrives.
`startup_recovery_holds_home_and_project_leases_or_reverses_a_project_under_only_home`
checks both real lease files. Pending and interrupted plans recover before the
startup gate opens; one failed status write does not stop later plans.

### Desktop startup recovery gate

| State      | Stored outcome                                     | Command behavior                  | Next state                |
| ---------- | -------------------------------------------------- | --------------------------------- | ------------------------- |
| Recovering | Outcome not published; worker owns the store mutex | Wait for the worker               | Ready or Failed           |
| Ready      | Completed                                          | Store commands may run            | Terminal for this startup |
| Failed     | Failed with a reason                               | Store commands return that reason | Restart retries recovery  |

The worker owns the mutex before Tauri receives the state. It publishes the
outcome before releasing the mutex. Its drop guard publishes failure if the
worker unwinds. Core writes also retry journal recovery under the write lease
and refuse to start while any plan remains interrupted.

## Audit follow-up UI requests

| Machine                   | Owner                           | States and legal moves                                                                                                                                                                                                                                             | Proof                                                                                                                                                                              |
| ------------------------- | ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Manual skill update check | HomeUpdateCheck useActionState  | Result starts initial. A request from initial, checked or failed settles as checked or failed. React owns pending; Check now is disabled while pending. No request produces initial.                                                                               | HomeUpdateCheck.test.tsx walks initial → failed → checked and initial → checked → checked → failed → failed → checked, plus deferred busy state and both keyboard activation keys. |
| Browse details            | SkillDetailsContent load effect | Remote sources start loading; a local source with no remote id starts failed. Current request settles loaded or failed. Retry moves failed → loading. A new skill id remounts in loading. Effect cleanup ignores old completions after unmount or a newer request. | SkillDetailPanel.test.tsx covers loading → failed → loading → loaded, normal loading → loaded, and a selected source change while loading.                                         |

## Split-update restore retry snapshots

Before a Restore changes files, it saves its id as the target's `restore_retry`.
Recovery can release the claim without losing that first snapshot. A Force retry
keeps that snapshot for Redo and records its own current files and metadata in
`payload.displaced_data`, with a separate backup directory and inverse. Further
failed attempts keep the first retry reference. No event status or legal status
move changes.

Guards: `split_update_undo_crash_force_retry_redo_restores_matching_hashes_or_next_update_refuses`
and `split_update_failed_undo_force_preserves_intervening_files_and_metadata_or_discards_edits`
in `crates/skill-studio-core/tests/split_copy_update.rs`.

Each restore compares metadata with its own captured rows before replacing them,
including under Force. A new edit returns `drift_conflict` and releases the claim,
so Activity can offer Force again with a fresh snapshot. Force can rebuild a
deleted source-lock document. Redo of an interrupted Update derives copy hashes
from the backed-up folders while displaced metadata keeps the original live rows.

Guards: `split_update_crashes_before_registry_undo_redo_allows_update_or_stale_hash_refuses`,
`split_update_force_detects_lock_edits_during_restore_or_loses_the_new_row`,
and `split_update_undo_force_restores_a_lock_deleted_before_retry_or_fails_with_not_found`
in `crates/skill-studio-core/tests/split_copy_update.rs`.

## Desktop background work

`skills/background_work.rs::BackgroundState::run` owns scan, invocation, and
update-check runs. Start adds one in-flight run. Finish removes one and records
the result. The wire `running` value follows the in-flight count. It is not
stored as a second flag. A successful finish clears the last error.
`overlapping_runs_keep_the_worker_running_until_both_finish_or_hide_the_remaining_scan`
and `background_failure_exposes_its_run_and_error_and_a_later_success_clears_it_or_leaves_stale_health`
cover these moves. Each update advances the status revision.

Unreadable update sources keep cached owners under the affected roots and finish
with an error. Invocation cache write failures publish usable memory results and
finish with an error. Startup begins with an empty snapshot and invocation index;
the refresh thread loads the cache before its first scan.
`unreadable_sources_keep_cached_owners_and_report_failure_or_erase_commits`,
`invocation_refresh_publishes_results_and_failure_when_cache_write_fails_or_reports_healthy`,
and `startup_returns_with_empty_state_before_cache_load_or_blocks_setup` guard
these failure and startup paths.
