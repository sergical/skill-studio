----------------------------- MODULE UndoClaim -----------------------------
(***************************************************************************)
(* Undo of one change in Skill Studio (Activity > Undo, `skill-studio      *)
(* restore`, the MCP tool `restore_event`), with a writer that can die     *)
(* between any two durable steps.                                          *)
(*                                                                         *)
(* One change T sits in the `events` table. Its files are as T left them.  *)
(* An Undo records a `restore` row, claims T by writing the row's id into  *)
(* T's `reverted_by` column, puts the files back, and finishes the row.    *)
(* Activity shows T as undone, and offers no Undo, whenever `reverted_by`  *)
(* is set.                                                                 *)
(*                                                                         *)
(* Two pieces of code undo a change. Which one runs depends on who wrote   *)
(* the change (`restore_desktop_event` in                                  *)
(* apps/desktop/src-tauri/src/skills/event_commands.rs):                   *)
(*   Owner = "core"     crates/skill-studio-core/src/ops.rs                *)
(*                      `restore_event_body`                               *)
(*   Owner = "desktop"  apps/desktop/src-tauri/src/skills/event_store.rs   *)
(*                      `EventStore::restore` and `apply_restore`          *)
(*                                                                         *)
(* Each action below is one durable step of that code: one SQL statement   *)
(* or one file operation. The comment on each action names the code it     *)
(* stands for. ../SKILL.md lists the same code as a table.                 *)
(***************************************************************************)
EXTENDS Naturals, FiniteSets

CONSTANTS
    Owner,        \* "core" or "desktop": which code undoes this change
    MaxAttempts,  \* how many times someone asks for Undo
    MaxCrashes,   \* how many times a writer dies between two steps
    Break         \* code mutations switched on; {} is the code as written

ASSUME Owner \in {"core", "desktop"}
ASSUME MaxAttempts \in Nat /\ MaxCrashes \in Nat

\* Each name mirrors one edit to the real code. `../check` turns them on one
\* at a time and requires a rule to fail, so a pass is known to mean something.
Mutations == {
    "recovery_skips_release",        \* events.rs recover_interrupted: drop the release loop
    "recovery_releases_done_claims", \* history.rs held_restore_claims: drop the status filter
    "core_claim_before_record",      \* ops.rs restore_event_body: claim_revert above record
    "core_fail_finish_before_release", \* ops.rs restore_event_body: finish(Failed) above release_revert
    "core_finish_before_mutation",   \* ops.rs restore_event_body: finish(Done) above the file write
    "desktop_claim_before_record",   \* event_store.rs: claim in `restore`, above `apply_restore`
    "desktop_fail_finish_before_release", \* event_store.rs fail_claimed_restore: finish(Failed) first
    "desktop_finish_before_mutation" \* event_store.rs apply_restore: finish(Done) above apply_inverse_op
}
ASSUME Break \subseteq Mutations
Broken(m) == m \in Break

\* How the undo writes the files. It depends on the change, so it is fixed
\* for one run of the model.
\*   plain:          no journal. Core: `write_atomic`, `remove_file`, or
\*                   `remove_tree_best_effort` deleting an installed folder
\*                   entry by entry. Desktop: undo_files.rs `restore_from`
\*                   removes the live path and then copies the backup, and
\*                   `symlink` removes the old link and then renames the new
\*                   one in.
\*   journaled_swap: core only. `restore_write_dir` records a swap in a
\*                   journal plan, exchanges the folders, moves the old one
\*                   to quarantine, and finishes the plan. Recovery reverses
\*                   a plan that was never finished.
\* Every write may land at once or be caught half way ("partial"). For a
\* write the code does in one atomic operation the half-way state cannot
\* happen, so the model allows more than the code does there, never less.
WriteShapes ==
    IF Owner = "core" THEN {"plain", "journaled_swap"} ELSE {"plain"}

Attempts == 1..MaxAttempts
None == 0

VARIABLES
    revertedBy,  \* T's `reverted_by` column: None, or the restore row that claims T
    row,         \* row[r]: status of restore row r; "none" means the row was never recorded
    files,       \* "changed": as T left them.  "restored": as they were before T.
                 \* "partial": neither, an undo stopped half way through its write
    shape,       \* how the undo writes the files: one of WriteShapes
    plan,        \* core only: "none", or the files an unfinished folder swap replaces
    pc,          \* the next step of the one writer that holds the home lease
    job,         \* what the session does: "undo", another "write", or the desktop "start"
    cur,         \* the restore id this writer allocated (process memory)
    force,       \* whether this Undo was asked with force (process memory)
    asked,       \* how many Undo requests were made so far
    crashes,     \* how many times a writer died so far
    recovered    \* TRUE once a core write session finished recovery after the last crash

vars == <<revertedBy, row, files, shape, plan, pc, job, cur, force, asked, crashes,
          recovered>>

Statuses == {"none", "pending", "done", "failed", "interrupted"}
Steps == {
    "idle",
    "rec_release", "rec_flip", "rec_journal", "rec_plan_close",
    "core_read", "core_record", "core_claim", "core_write", "core_write_rest",
    "core_swap", "core_swap_rest", "core_plan_done", "core_finish",
    "core_fail_release", "core_fail_finish",
    "desk_read", "desk_check", "desk_record", "desk_claim", "desk_apply",
    "desk_apply_rest", "desk_finish", "desk_release", "desk_fail_finish",
    "desk_err_release"
}

TypeOK ==
    /\ revertedBy \in Attempts \cup {None}
    /\ row \in [Attempts -> Statuses]
    /\ files \in {"changed", "partial", "restored"}
    /\ shape \in WriteShapes
    /\ plan \in {"none", "changed", "partial", "restored"}
    /\ pc \in Steps
    /\ job \in {"none", "undo", "write", "start"}
    /\ cur \in Attempts \cup {None}
    /\ force \in BOOLEAN
    /\ asked \in 0..MaxAttempts
    /\ crashes \in 0..MaxCrashes
    /\ recovered \in BOOLEAN

Init ==
    /\ revertedBy = None
    /\ row = [r \in Attempts |-> "none"]
    /\ files = "changed"
    /\ shape \in WriteShapes
    /\ plan = "none"
    /\ pc = "idle"
    /\ job = "none"
    /\ cur = None
    /\ force = FALSE
    /\ asked = 0
    /\ crashes = 0
    /\ recovered = TRUE

Idle == pc = "idle"

\* The writer goes on to `next`. A session that ends forgets its job.
GoTo(next) ==
    /\ pc' = next
    /\ job' = IF next = "idle" THEN "none" ELSE job

(***************************************************************************)
(* The home lease. Every writer takes the exclusive lease on the home      *)
(* before it reads T: core in `WriteHold::take` (ports.rs), desktop in     *)
(* `restore_event_with_state` (event_commands.rs). So one writer runs at a *)
(* time, and the model has one `pc`. The lease is an OS file lock          *)
(* (crates/skill-studio-host/src/lease.rs); the model assumes it holds.    *)
(***************************************************************************)

(***************************************************************************)
(* A core write session starts.  ports.rs `MutationSession::begin_for`:    *)
(* recover_interrupted, then reconcile_scope_journal, then the write.      *)
(***************************************************************************)

\* Someone asks core to undo T: Activity > Undo on a core-written change,
\* `skill-studio restore`, or the MCP tool `restore_event`.
YouAskCoreToUndo ==
    /\ Owner = "core"
    /\ Idle
    /\ asked < MaxAttempts
    /\ asked' = asked + 1
    /\ cur' = asked + 1
    /\ force' \in BOOLEAN
    /\ job' = "undo"
    /\ pc' = "rec_release"
    /\ UNCHANGED <<revertedBy, row, files, shape, plan, crashes, recovered>>

\* Any other core write starts: park, install, update, remove, from the
\* desktop, the CLI or MCP. It runs the same recovery pass first.
AnotherWriteStarts ==
    /\ Idle
    /\ job' = "write"
    /\ pc' = "rec_release"
    /\ UNCHANGED <<revertedBy, row, files, shape, plan, cur, force, asked, crashes,
                   recovered>>

\* events.rs `recover_interrupted`, first loop: release the claim of every
\* restore row that is still pending or interrupted
\* (history.rs `held_restore_claims` and `release_revert`).
\* The query joins on the restore row, so a claim whose row was never
\* recorded is not in it.
RecoveryReleasesClaim ==
    /\ pc = "rec_release"
    /\ LET held ==
            /\ revertedBy # None
            /\ IF Broken("recovery_releases_done_claims")
                   THEN row[revertedBy] # "none"
                   ELSE row[revertedBy] \in {"pending", "interrupted"}
       IN revertedBy' = IF held /\ ~Broken("recovery_skips_release")
                            THEN None ELSE revertedBy
    /\ pc' = "rec_flip"
    /\ UNCHANGED <<row, files, shape, plan, job, cur, force, asked, crashes, recovered>>

\* events.rs `recover_interrupted`, second loop: one `finish(Interrupted)`
\* per pending row.
RecoveryMarksRowInterrupted ==
    /\ pc = "rec_flip"
    /\ \E r \in Attempts :
        /\ row[r] = "pending"
        /\ row' = [row EXCEPT ![r] = "interrupted"]
    /\ UNCHANGED <<revertedBy, files, shape, plan, pc, job, cur, force, asked, crashes,
                   recovered>>

RecoveryDoneWithRows ==
    /\ pc = "rec_flip"
    /\ \A r \in Attempts : row[r] # "pending"
    /\ pc' = "rec_journal"
    /\ UNCHANGED <<revertedBy, row, files, shape, plan, job, cur, force, asked, crashes,
                   recovered>>

\* journal.rs `reconcile_scope_journal`, for a plan that never finished:
\* `reverse_steps` puts back the folder the swap replaced. It can be caught
\* half way too: when no folder was there before the swap, it deletes the
\* swapped-in folder entry by entry (`remove_tree`).
RecoveryStartsPuttingFolderBack ==
    /\ pc = "rec_journal"
    /\ plan # "none"
    /\ files # plan
    /\ files # "partial"
    /\ files' = "partial"
    /\ UNCHANGED <<revertedBy, row, shape, plan, pc, job, cur, force, asked, crashes,
                   recovered>>

RecoveryPutsSwappedFolderBack ==
    /\ pc = "rec_journal"
    /\ plan # "none"
    /\ files' = plan
    /\ pc' = "rec_plan_close"
    /\ UNCHANGED <<revertedBy, row, shape, plan, job, cur, force, asked, crashes,
                   recovered>>

\* Then `journal.finish` marks the plan reversed, in its own write.
RecoveryClosesPlan ==
    /\ pc = "rec_plan_close"
    /\ plan' = "none"
    /\ pc' = "rec_journal"
    /\ UNCHANGED <<revertedBy, row, files, shape, job, cur, force, asked, crashes,
                   recovered>>

\* The pass is over. Only a core write session has released claims, so only
\* that counts as recovered; desktop startup does not.
RecoveryEnds ==
    /\ pc = "rec_journal"
    /\ plan = "none"
    /\ recovered' = IF job = "start" THEN recovered ELSE TRUE
    /\ GoTo(IF job = "undo" THEN "core_read" ELSE "idle")
    /\ UNCHANGED <<revertedBy, row, files, shape, plan, cur, force, asked, crashes>>

(***************************************************************************)
(* Core undo.  ops.rs `restore_event_body`.                                *)
(***************************************************************************)

\* Reads only: `restore_capability` refuses a change that is already
\* claimed (AlreadyReverted), and the drift check refuses files that are
\* not as T left them unless force is given (DriftConflict).
CoreChecks ==
    /\ pc = "core_read"
    /\ GoTo(IF revertedBy # None \/ (files # "changed" /\ ~force)
                THEN "idle"
                ELSE IF Broken("core_claim_before_record")
                         THEN "core_claim" ELSE "core_record")
    /\ UNCHANGED <<revertedBy, row, files, shape, plan, cur, force, asked, crashes,
                   recovered>>

\* `backup_paths` then `record` (history.rs `record` inserts the row as
\* pending, with `target_event` in its payload).
CoreRecordsRestoreRow ==
    /\ pc = "core_record"
    /\ row' = [row EXCEPT ![cur] = "pending"]
    /\ GoTo(IF Broken("core_claim_before_record") THEN "core_write" ELSE "core_claim")
    /\ UNCHANGED <<revertedBy, files, shape, plan, cur, force, asked, crashes, recovered>>

\* `claim_revert`: UPDATE ... SET reverted_by = ? WHERE reverted_by IS NULL.
\* A lost claim finishes the row as failed.
CoreClaims ==
    /\ pc = "core_claim"
    /\ IF revertedBy = None
           THEN /\ revertedBy' = cur
                /\ GoTo(IF Broken("core_claim_before_record")
                            THEN "core_record"
                            ELSE IF Broken("core_finish_before_mutation")
                                     THEN "core_finish" ELSE "core_write")
                /\ UNCHANGED row
           ELSE /\ row' = [row EXCEPT ![cur] = IF @ = "none" THEN @ ELSE "failed"]
                /\ GoTo("idle")
                /\ UNCHANGED revertedBy
    /\ UNCHANGED <<files, shape, plan, cur, force, asked, crashes, recovered>>

\* Where the writer goes once the files are written.
\* (`patch_payload restore_retry` runs just before the write; it changes
\* nothing the rules read, so it has no action here.)
AfterCoreWrite == IF Broken("core_finish_before_mutation") THEN "idle" ELSE "core_finish"

\* A write under a journal plan starts by recording the swap in the plan
\* (fsops.rs `swap`, `record_swap_with_parent`), before it touches the folder.
CoreOpensSwapPlan ==
    /\ pc = "core_write"
    /\ shape = "journaled_swap"
    /\ plan' = files
    /\ pc' = "core_swap"
    /\ UNCHANGED <<revertedBy, row, files, shape, job, cur, force, asked, crashes,
                   recovered>>

\* The write lands at once: `write_atomic`, `remove_file`, or the folder
\* exchange of a swap.
CoreWritesAtOnce ==
    /\ \/ pc = "core_write" /\ shape = "plain"
       \/ pc = "core_swap"
    /\ files' = "restored"
    /\ GoTo(IF pc = "core_swap" THEN "core_plan_done" ELSE AfterCoreWrite)
    /\ UNCHANGED <<revertedBy, row, shape, plan, cur, force, asked, crashes, recovered>>

\* Nothing to write: `RemoveIfPresent` finds the path already gone
\* (`Err(_) => Ok(())`), as when an earlier undo removed the folder and was
\* cut off before it was recorded as finished.
CoreFindsFilesAlreadyBack ==
    /\ pc = "core_write"
    /\ shape = "plain"
    /\ files = "restored"
    /\ GoTo(AfterCoreWrite)
    /\ UNCHANGED <<revertedBy, row, files, shape, plan, cur, force, asked, crashes, recovered>>

\* The write is under way and the files are neither old nor new, as when
\* `remove_tree_best_effort` has deleted some entries of a folder.
CoreStartsWrite ==
    /\ \/ pc = "core_write" /\ shape = "plain"
       \/ pc = "core_swap"
    /\ files' = "partial"
    /\ pc' = IF pc = "core_swap" THEN "core_swap_rest" ELSE "core_write_rest"
    /\ UNCHANGED <<revertedBy, row, shape, plan, job, cur, force, asked, crashes,
                   recovered>>

CoreEndsWrite ==
    /\ pc \in {"core_write_rest", "core_swap_rest"}
    /\ files' = "restored"
    /\ GoTo(IF pc = "core_swap_rest" THEN "core_plan_done" ELSE AfterCoreWrite)
    /\ UNCHANGED <<revertedBy, row, shape, plan, cur, force, asked, crashes, recovered>>

\* `restore_write_dir`: the old folder is in quarantine, and
\* `plan.finish(PlanStatus::Done)` closes the plan.
CoreClosesPlan ==
    /\ pc = "core_plan_done"
    /\ plan' = "none"
    /\ GoTo(AfterCoreWrite)
    /\ UNCHANGED <<revertedBy, row, files, shape, cur, force, asked, crashes, recovered>>

\* The primary write returns an error (`mutation_result` is Err), or a later
\* path cannot be put back (`unrestored` is not empty). The files stay as
\* the failed write left them, and a swap's plan stays open: for instance
\* the move to quarantine fails after the folders were exchanged.
CoreWriteFails ==
    /\ pc \in {"core_write", "core_write_rest", "core_swap", "core_swap_rest",
              "core_plan_done", "core_finish"}
    /\ ~Broken("core_finish_before_mutation")
    /\ pc' = IF Broken("core_fail_finish_before_release")
                 THEN "core_fail_finish" ELSE "core_fail_release"
    /\ UNCHANGED <<revertedBy, row, files, shape, plan, job, cur, force, asked, crashes,
                   recovered>>

\* `release_revert`, so the change stays undoable.
CoreReleasesAfterFailure ==
    /\ pc = "core_fail_release"
    /\ revertedBy' = IF revertedBy = cur THEN None ELSE revertedBy
    /\ GoTo(IF Broken("core_fail_finish_before_release") THEN "idle" ELSE "core_fail_finish")
    /\ UNCHANGED <<row, files, shape, plan, cur, force, asked, crashes, recovered>>

\* `finish(Failed)`.
CoreFinishesFailed ==
    /\ pc = "core_fail_finish"
    /\ row' = [row EXCEPT ![cur] = "failed"]
    /\ GoTo(IF Broken("core_fail_finish_before_release") THEN "core_fail_release" ELSE "idle")
    /\ UNCHANGED <<revertedBy, files, shape, plan, cur, force, asked, crashes, recovered>>

\* `finish(Done)`: the undo is complete.
CoreFinishesDone ==
    /\ pc = "core_finish"
    /\ row' = [row EXCEPT ![cur] = "done"]
    /\ GoTo(IF Broken("core_finish_before_mutation") THEN "core_write" ELSE "idle")
    /\ UNCHANGED <<revertedBy, files, shape, plan, cur, force, asked, crashes, recovered>>

(***************************************************************************)
(* Desktop undo.  event_store.rs `EventStore::restore` and `apply_restore`.*)
(* It takes the home lease but runs no recovery pass of its own. It writes *)
(* in the core's order: record the restore row, then claim; on a failed    *)
(* write, release the claim, then record the failure.                      *)
(***************************************************************************)

\* Activity > Undo on a desktop-written change.
YouAskDesktopToUndo ==
    /\ Owner = "desktop"
    /\ Idle
    /\ asked < MaxAttempts
    /\ asked' = asked + 1
    /\ cur' = asked + 1
    /\ force' \in BOOLEAN
    /\ pc' = "desk_read"
    /\ UNCHANGED <<revertedBy, row, files, shape, plan, job, crashes, recovered>>

\* `restore`: "Event ... was already restored" when `reverted_by` is set.
DesktopChecksClaim ==
    /\ pc = "desk_read"
    /\ pc' = IF revertedBy # None
                 THEN "idle"
                 ELSE IF Broken("desktop_claim_before_record")
                          THEN "desk_claim" ELSE "desk_check"
    /\ UNCHANGED <<revertedBy, row, files, shape, plan, job, cur, force, asked, crashes,
                   recovered>>

\* `apply_restore`: the drift check. A refusal returns Err, and `restore`
\* then runs its release, which finds no claim to release.
DesktopChecksDrift ==
    /\ pc = "desk_check"
    /\ pc' = IF files # "changed" /\ ~force THEN "desk_err_release" ELSE "desk_record"
    /\ UNCHANGED <<revertedBy, row, files, shape, plan, job, cur, force, asked, crashes,
                   recovered>>

\* `apply_restore`: `backup_for_restore`, then `record_with_bound_paths`,
\* which inserts the row as pending with `target_event` in its payload.
DesktopRecordsRestoreRow ==
    /\ pc = "desk_record"
    /\ row' = [row EXCEPT ![cur] = "pending"]
    /\ pc' = IF Broken("desktop_claim_before_record") THEN "desk_apply" ELSE "desk_claim"
    /\ UNCHANGED <<revertedBy, files, shape, plan, job, cur, force, asked, crashes,
                   recovered>>

\* `claim_recorded_restore`: UPDATE events SET reverted_by = ?1 WHERE id = ?2
\* AND reverted_by IS NULL (`claim_event_restore`). A lost claim finishes
\* the row as failed.
DesktopClaims ==
    /\ pc = "desk_claim"
    /\ IF revertedBy = None
           THEN /\ revertedBy' = cur
                /\ pc' = IF Broken("desktop_claim_before_record")
                             THEN "desk_check"
                             ELSE IF Broken("desktop_finish_before_mutation")
                                      THEN "desk_finish" ELSE "desk_apply"
                /\ UNCHANGED row
           ELSE /\ row' = [row EXCEPT ![cur] = IF @ = "none" THEN @ ELSE "failed"]
                /\ pc' = "desk_err_release"
                /\ UNCHANGED revertedBy
    /\ UNCHANGED <<files, shape, plan, job, cur, force, asked, crashes, recovered>>

\* Where the desktop goes once the files are written.
AfterDesktopWrite ==
    IF Broken("desktop_finish_before_mutation") THEN "idle" ELSE "desk_finish"

\* `apply_inverse_op`: the write lands at once, as when a link is removed.
DesktopWritesAtOnce ==
    /\ pc = "desk_apply"
    /\ files' = "restored"
    /\ pc' = AfterDesktopWrite
    /\ UNCHANGED <<revertedBy, row, shape, plan, job, cur, force, asked, crashes,
                   recovered>>

\* Nothing to write: undo_files.rs `remove` finds the path already gone
\* (`NotFound => Ok(())`).
DesktopFindsFilesAlreadyBack ==
    /\ pc = "desk_apply"
    /\ files = "restored"
    /\ pc' = AfterDesktopWrite
    /\ UNCHANGED <<revertedBy, row, files, shape, plan, job, cur, force, asked, crashes,
                   recovered>>

\* `apply_inverse_op`: undo_files.rs `restore_from` and `symlink` remove the
\* live path first.
DesktopRemovesLiveFiles ==
    /\ pc = "desk_apply"
    /\ files' = "partial"
    /\ pc' = "desk_apply_rest"
    /\ UNCHANGED <<revertedBy, row, shape, plan, job, cur, force, asked, crashes,
                   recovered>>

\* Then they copy the backup, or rename the new link, into place.
DesktopPutsOldFilesIn ==
    /\ pc = "desk_apply_rest"
    /\ files' = "restored"
    /\ pc' = AfterDesktopWrite
    /\ UNCHANGED <<revertedBy, row, shape, plan, job, cur, force, asked, crashes,
                   recovered>>

\* `apply_inverse_op` returns an error. The files stay as it left them.
DesktopWriteFails ==
    /\ pc \in {"desk_apply", "desk_apply_rest"}
    /\ ~Broken("desktop_finish_before_mutation")
    /\ pc' = IF Broken("desktop_fail_finish_before_release")
                 THEN "desk_fail_finish" ELSE "desk_release"
    /\ UNCHANGED <<revertedBy, row, files, shape, plan, job, cur, force, asked, crashes,
                   recovered>>

\* `fail_claimed_restore`: `unclaim_event_restore` first, UPDATE events SET
\* reverted_by = NULL WHERE id = ?1 AND reverted_by = ?2.
DesktopReleasesAfterFailure ==
    /\ pc = "desk_release"
    /\ revertedBy' = IF revertedBy = cur THEN None ELSE revertedBy
    /\ pc' = "desk_fail_finish"
    /\ UNCHANGED <<row, files, shape, plan, job, cur, force, asked, crashes, recovered>>

\* `fail_claimed_restore`: then `finish(Failed)`.
DesktopFinishesFailed ==
    /\ pc = "desk_fail_finish"
    /\ row' = [row EXCEPT ![cur] = "failed"]
    /\ pc' = "desk_err_release"
    /\ UNCHANGED <<revertedBy, files, shape, plan, job, cur, force, asked, crashes,
                   recovered>>

\* `restore`, after `apply_restore` returned Err: one more
\* `unclaim_event_restore`. It releases a claim only if one is still held.
DesktopReleasesOnError ==
    /\ pc = "desk_err_release"
    /\ revertedBy' = IF revertedBy = cur THEN None ELSE revertedBy
    /\ pc' = "idle"
    /\ UNCHANGED <<row, files, shape, plan, job, cur, force, asked, crashes, recovered>>

\* `apply_restore`: `patch_inverse_post_fingerprint`, then `finish(Done)`.
\* The patch writes only the restore row's inverse, which no rule reads.
DesktopFinishesDone ==
    /\ pc = "desk_finish"
    /\ row' = [row EXCEPT ![cur] = "done"]
    /\ pc' = IF Broken("desktop_finish_before_mutation") THEN "desk_apply" ELSE "idle"
    /\ UNCHANGED <<revertedBy, files, shape, plan, job, cur, force, asked, crashes,
                   recovered>>

(***************************************************************************)
(* The desktop app starts.  The recovery closure in lib.rs `run` takes the *)
(* lease, then calls `reconcile_event_store_at_startup` (event_store.rs    *)
(* `reconcile_at_startup`: every pending row becomes interrupted) and      *)
(* `reconcile_scope_journal`. So it runs the recovery steps above, without *)
(* the first: it releases no claim. A start with no pending row and no     *)
(* open plan writes nothing, so it has no action here.                     *)
(***************************************************************************)
DesktopAppStarts ==
    /\ Idle
    /\ \/ \E r \in Attempts : row[r] = "pending"
       \/ plan # "none"
    /\ job' = "start"
    /\ pc' = "rec_flip"
    /\ UNCHANGED <<revertedBy, row, files, shape, plan, cur, force, asked, crashes,
                   recovered>>

(***************************************************************************)
(* The writer dies: a crash, a force quit, a power cut. What it wrote      *)
(* stays. What it held in memory is gone. The lease is free again.         *)
(***************************************************************************)

WriterDies ==
    /\ ~Idle
    /\ crashes < MaxCrashes
    /\ crashes' = crashes + 1
    /\ pc' = "idle"
    /\ job' = "none"
    /\ cur' = None
    /\ force' = FALSE
    /\ recovered' = FALSE
    /\ UNCHANGED <<revertedBy, row, files, shape, plan, asked>>

Next ==
    \/ YouAskCoreToUndo
    \/ AnotherWriteStarts
    \/ RecoveryReleasesClaim
    \/ RecoveryMarksRowInterrupted
    \/ RecoveryDoneWithRows
    \/ RecoveryStartsPuttingFolderBack
    \/ RecoveryPutsSwappedFolderBack
    \/ RecoveryClosesPlan
    \/ RecoveryEnds
    \/ CoreChecks
    \/ CoreRecordsRestoreRow
    \/ CoreClaims
    \/ CoreOpensSwapPlan
    \/ CoreWritesAtOnce
    \/ CoreFindsFilesAlreadyBack
    \/ CoreStartsWrite
    \/ CoreEndsWrite
    \/ CoreClosesPlan
    \/ CoreWriteFails
    \/ CoreReleasesAfterFailure
    \/ CoreFinishesFailed
    \/ CoreFinishesDone
    \/ YouAskDesktopToUndo
    \/ DesktopChecksClaim
    \/ DesktopClaims
    \/ DesktopChecksDrift
    \/ DesktopRecordsRestoreRow
    \/ DesktopWritesAtOnce
    \/ DesktopFindsFilesAlreadyBack
    \/ DesktopRemovesLiveFiles
    \/ DesktopPutsOldFilesIn
    \/ DesktopWriteFails
    \/ DesktopFinishesFailed
    \/ DesktopReleasesAfterFailure
    \/ DesktopReleasesOnError
    \/ DesktopFinishesDone
    \/ DesktopAppStarts
    \/ WriterDies

Spec == Init /\ [][Next]_vars

(***************************************************************************)
(* The rules.                                                              *)
(***************************************************************************)

\* Once no write is running and the next write has done its recovery, a
\* change is shown as undone only if an undo of it finished.
\* Otherwise the change can never be undone: Activity offers no Undo, and
\* the CLI answers "this event was already reverted".
NoStrandedUndo ==
    (Idle /\ recovered) =>
        (revertedBy = None \/ row[revertedBy] = "done")

\* An undo is recorded as finished only when the files are back as they
\* were before the change, and no step of an undo or of recovery moves them
\* afterwards. The undo need not have written them itself: a retry finishes
\* with no write when an earlier attempt already put them back. The model
\* does not look inside the files: whether a write reproduces the old tree
\* byte for byte is not checked here.
UndoneMeansFilesBack ==
    \A r \in Attempts : row[r] = "done" => files = "restored"

\* A finished undo keeps its claim: the change stays shown as undone, so it
\* cannot be undone a second time.
FinishedUndoKeepsClaim ==
    \A r \in Attempts : row[r] = "done" => revertedBy = r

(***************************************************************************)
(* For the chart configs only. A recovery pass that starts when nothing is *)
(* left to recover changes nothing, so the chart leaves it out. The check  *)
(* configs do not use this.                                                *)
(***************************************************************************)
ChartLeavesOutIdleRecovery == ~(job = "write" /\ recovered)

\* For the chart configs only: one chart shows one way of writing the files.
\* It also leaves out the half-way state of a write, and, to fit the core
\* chart, the desktop app starting (its steps are the next write's, minus
\* the release).
ChartOfFolderSwap == shape = "journaled_swap" /\ files # "partial" /\ job # "start"
ChartOfWriteAtOnce == files # "partial"

=============================================================================
