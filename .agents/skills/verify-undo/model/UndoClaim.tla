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
(* or one file swap. The comment on each action names the code it stands   *)
(* for. ../SKILL.md lists the same code as a table.                       *)
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
    "core_finish_before_mutation"    \* ops.rs restore_event_body: finish(Done) above the file write
}
ASSUME Break \subseteq Mutations
Broken(m) == m \in Break

Attempts == 1..MaxAttempts
None == 0

VARIABLES
    revertedBy,  \* T's `reverted_by` column: None, or the restore row that claims T
    row,         \* row[r]: status of restore row r; "none" means the row was never recorded
    files,       \* "changed": as T left them.  "restored": as they were before T
    plan,        \* core only: the journal plan of the folder swap
    pc,          \* the next step of the one writer that holds the home lease
    job,         \* what a core write session does after its recovery pass
    cur,         \* the restore id this writer allocated (process memory)
    force,       \* whether this Undo was asked with force (process memory)
    asked,       \* how many Undo requests were made so far
    crashes,     \* how many times a writer died so far
    recovered    \* TRUE once a core write session finished recovery after the last crash

vars == <<revertedBy, row, files, plan, pc, job, cur, force, asked, crashes, recovered>>

Statuses == {"none", "pending", "done", "failed", "interrupted"}
Steps == {
    "idle",
    "rec_release", "rec_flip", "rec_journal",
    "core_read", "core_record", "core_claim", "core_write", "core_plan_done",
    "core_finish", "core_fail_release", "core_fail_finish",
    "desk_read", "desk_claim", "desk_check", "desk_record", "desk_apply",
    "desk_finish", "desk_fail_finish", "desk_release"
}

TypeOK ==
    /\ revertedBy \in Attempts \cup {None}
    /\ row \in [Attempts -> Statuses]
    /\ files \in {"changed", "restored"}
    /\ plan \in {"none", "back_to_changed", "back_to_restored"}
    /\ pc \in Steps
    /\ job \in {"none", "undo", "write"}
    /\ cur \in Attempts \cup {None}
    /\ force \in BOOLEAN
    /\ asked \in 0..MaxAttempts
    /\ crashes \in 0..MaxCrashes
    /\ recovered \in BOOLEAN

Init ==
    /\ revertedBy = None
    /\ row = [r \in Attempts |-> "none"]
    /\ files = "changed"
    /\ plan = "none"
    /\ pc = "idle"
    /\ job = "none"
    /\ cur = None
    /\ force = FALSE
    /\ asked = 0
    /\ crashes = 0
    /\ recovered = TRUE

Idle == pc = "idle"

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
    /\ UNCHANGED <<revertedBy, row, files, plan, crashes, recovered>>

\* Any other core write starts: park, install, update, remove, from the
\* desktop, the CLI or MCP. It runs the same recovery pass first.
AnotherWriteStarts ==
    /\ Idle
    /\ job' = "write"
    /\ pc' = "rec_release"
    /\ UNCHANGED <<revertedBy, row, files, plan, cur, force, asked, crashes, recovered>>

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
    /\ UNCHANGED <<row, files, plan, job, cur, force, asked, crashes, recovered>>

\* events.rs `recover_interrupted`, second loop: one `finish(Interrupted)`
\* per pending row.
RecoveryMarksRowInterrupted ==
    /\ pc = "rec_flip"
    /\ \E r \in Attempts :
        /\ row[r] = "pending"
        /\ row' = [row EXCEPT ![r] = "interrupted"]
    /\ UNCHANGED <<revertedBy, files, plan, pc, job, cur, force, asked, crashes, recovered>>

RecoveryDoneWithRows ==
    /\ pc = "rec_flip"
    /\ \A r \in Attempts : row[r] # "pending"
    /\ pc' = "rec_journal"
    /\ UNCHANGED <<revertedBy, row, files, plan, job, cur, force, asked, crashes, recovered>>

\* journal.rs `reconcile_scope_journal`: a plan that never finished is
\* reversed, so the folder a cut-off swap put in place goes back.
RecoveryReversesPlan ==
    /\ pc = "rec_journal"
    /\ files' = CASE plan = "back_to_changed" -> "changed"
                  [] plan = "back_to_restored" -> "restored"
                  [] OTHER -> files
    /\ plan' = "none"
    /\ recovered' = TRUE
    /\ pc' = IF job = "undo" THEN "core_read" ELSE "idle"
    /\ job' = IF job = "undo" THEN job ELSE "none"
    /\ UNCHANGED <<revertedBy, row, cur, force, asked, crashes>>

(***************************************************************************)
(* Core undo.  ops.rs `restore_event_body`.                                *)
(***************************************************************************)

\* Reads only: `restore_capability` refuses a change that is already
\* claimed (AlreadyReverted), and the drift check refuses files that are
\* not as T left them unless force is given (DriftConflict).
CoreChecks ==
    /\ pc = "core_read"
    /\ pc' = IF revertedBy # None \/ (files # "changed" /\ ~force)
                 THEN "idle"
                 ELSE IF Broken("core_claim_before_record")
                          THEN "core_claim" ELSE "core_record"
    /\ job' = IF pc' = "idle" THEN "none" ELSE job
    /\ UNCHANGED <<revertedBy, row, files, plan, cur, force, asked, crashes, recovered>>

\* `backup_paths` then `record`: the restore row exists, pending.
CoreRecordsRestoreRow ==
    /\ pc = "core_record"
    /\ row' = [row EXCEPT ![cur] = "pending"]
    /\ pc' = IF Broken("core_claim_before_record") THEN "core_write" ELSE "core_claim"
    /\ UNCHANGED <<revertedBy, files, plan, job, cur, force, asked, crashes, recovered>>

\* `claim_revert`: UPDATE ... SET reverted_by = ? WHERE reverted_by IS NULL.
\* A lost claim finishes the row as failed.
CoreClaims ==
    /\ pc = "core_claim"
    /\ IF revertedBy = None
           THEN /\ revertedBy' = cur
                /\ pc' = IF Broken("core_claim_before_record")
                             THEN "core_record"
                             ELSE IF Broken("core_finish_before_mutation")
                                      THEN "core_finish" ELSE "core_write"
                /\ UNCHANGED <<row, job>>
           ELSE /\ row' = [row EXCEPT ![cur] = IF @ = "none" THEN @ ELSE "failed"]
                /\ pc' = "idle"
                /\ job' = "none"
                /\ UNCHANGED revertedBy
    /\ UNCHANGED <<files, plan, cur, force, asked, crashes, recovered>>

\* The file write. For a folder, `restore_write_dir` begins a journal plan
\* and swaps the staged folder in; the plan is still open.
\* (`patch_payload restore_retry` runs just before; it changes nothing the
\* rules read, so it has no action here.)
CoreSwapsFilesIn ==
    /\ pc = "core_write"
    /\ plan' = IF files = "changed" THEN "back_to_changed" ELSE "back_to_restored"
    /\ files' = "restored"
    /\ pc' = "core_plan_done"
    /\ UNCHANGED <<revertedBy, row, job, cur, force, asked, crashes, recovered>>

\* `plan.finish(PlanStatus::Done)`.
CoreClosesPlan ==
    /\ pc = "core_plan_done"
    /\ plan' = "none"
    /\ pc' = IF Broken("core_finish_before_mutation") THEN "idle" ELSE "core_finish"
    /\ job' = IF Broken("core_finish_before_mutation") THEN "none" ELSE job
    /\ UNCHANGED <<revertedBy, row, files, cur, force, asked, crashes, recovered>>

\* The primary write returns an error (`mutation_result` is Err), or a later
\* path cannot be put back (`unrestored` is not empty).
CoreWriteFails ==
    /\ pc \in {"core_write", "core_finish"}
    /\ ~Broken("core_finish_before_mutation")
    /\ pc' = IF Broken("core_fail_finish_before_release")
                 THEN "core_fail_finish" ELSE "core_fail_release"
    /\ UNCHANGED <<revertedBy, row, files, plan, job, cur, force, asked, crashes, recovered>>

\* `release_revert`, so the change stays undoable.
CoreReleasesAfterFailure ==
    /\ pc = "core_fail_release"
    /\ revertedBy' = IF revertedBy = cur THEN None ELSE revertedBy
    /\ pc' = IF Broken("core_fail_finish_before_release") THEN "idle" ELSE "core_fail_finish"
    /\ job' = IF Broken("core_fail_finish_before_release") THEN "none" ELSE job
    /\ UNCHANGED <<row, files, plan, cur, force, asked, crashes, recovered>>

\* `finish(Failed)`.
CoreFinishesFailed ==
    /\ pc = "core_fail_finish"
    /\ row' = [row EXCEPT ![cur] = "failed"]
    /\ pc' = IF Broken("core_fail_finish_before_release") THEN "core_fail_release" ELSE "idle"
    /\ job' = IF Broken("core_fail_finish_before_release") THEN job ELSE "none"
    /\ UNCHANGED <<revertedBy, files, plan, cur, force, asked, crashes, recovered>>

\* `finish(Done)`: the undo is complete.
CoreFinishesDone ==
    /\ pc = "core_finish"
    /\ row' = [row EXCEPT ![cur] = "done"]
    /\ pc' = IF Broken("core_finish_before_mutation") THEN "core_write" ELSE "idle"
    /\ job' = IF Broken("core_finish_before_mutation") THEN job ELSE "none"
    /\ UNCHANGED <<revertedBy, files, plan, cur, force, asked, crashes, recovered>>

(***************************************************************************)
(* Desktop undo.  event_store.rs `EventStore::restore` and `apply_restore`.*)
(* It takes the home lease but runs no recovery pass of its own.           *)
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
    /\ UNCHANGED <<revertedBy, row, files, plan, job, crashes, recovered>>

\* `restore`: "Event ... was already restored" when `reverted_by` is set.
DesktopChecksClaim ==
    /\ pc = "desk_read"
    /\ pc' = IF revertedBy # None THEN "idle" ELSE "desk_claim"
    /\ UNCHANGED <<revertedBy, row, files, plan, job, cur, force, asked, crashes, recovered>>

\* `restore`: UPDATE events SET reverted_by = ?1 WHERE id = ?2 AND
\* reverted_by IS NULL. The restore row does not exist yet.
DesktopClaims ==
    /\ pc = "desk_claim"
    /\ IF revertedBy = None
           THEN revertedBy' = cur /\ pc' = "desk_check"
           ELSE UNCHANGED revertedBy /\ pc' = "idle"
    /\ UNCHANGED <<row, files, plan, job, cur, force, asked, crashes, recovered>>

\* `apply_restore`: the drift check. A refusal returns Err, and `restore`
\* then releases the claim.
DesktopChecksDrift ==
    /\ pc = "desk_check"
    /\ pc' = IF files # "changed" /\ ~force THEN "desk_release" ELSE "desk_record"
    /\ UNCHANGED <<revertedBy, row, files, plan, job, cur, force, asked, crashes, recovered>>

\* `apply_restore`: `backup_for_restore`, then `record_with_bound_paths`.
DesktopRecordsRestoreRow ==
    /\ pc = "desk_record"
    /\ row' = [row EXCEPT ![cur] = "pending"]
    /\ pc' = "desk_apply"
    /\ UNCHANGED <<revertedBy, files, plan, job, cur, force, asked, crashes, recovered>>

\* `apply_inverse_op` puts the files back.
DesktopPutsFilesBack ==
    /\ pc = "desk_apply"
    /\ files' = "restored"
    /\ pc' = "desk_finish"
    /\ UNCHANGED <<revertedBy, row, plan, job, cur, force, asked, crashes, recovered>>

\* `apply_inverse_op` returns an error.
DesktopWriteFails ==
    /\ pc = "desk_apply"
    /\ pc' = "desk_fail_finish"
    /\ UNCHANGED <<revertedBy, row, files, plan, job, cur, force, asked, crashes, recovered>>

\* `apply_restore`: `finish(Failed)`, before `restore` releases the claim.
DesktopFinishesFailed ==
    /\ pc = "desk_fail_finish"
    /\ row' = [row EXCEPT ![cur] = "failed"]
    /\ pc' = "desk_release"
    /\ UNCHANGED <<revertedBy, files, plan, job, cur, force, asked, crashes, recovered>>

\* `restore`: UPDATE events SET reverted_by = NULL WHERE id = ?1 AND
\* reverted_by = ?2, after `apply_restore` returned Err.
DesktopReleasesAfterFailure ==
    /\ pc = "desk_release"
    /\ revertedBy' = IF revertedBy = cur THEN None ELSE revertedBy
    /\ pc' = "idle"
    /\ UNCHANGED <<row, files, plan, job, cur, force, asked, crashes, recovered>>

\* `apply_restore`: `patch_inverse_post_fingerprint`, then `finish(Done)`.
DesktopFinishesDone ==
    /\ pc = "desk_finish"
    /\ row' = [row EXCEPT ![cur] = "done"]
    /\ pc' = "idle"
    /\ UNCHANGED <<revertedBy, files, plan, job, cur, force, asked, crashes, recovered>>

\* The desktop app starts.  lib.rs `reconcile_event_store_at_startup` calls
\* event_store.rs `reconcile_at_startup`: every pending row becomes
\* interrupted. It releases no claim.
DesktopStartsUp ==
    /\ Idle
    /\ \E r \in Attempts :
        /\ row[r] = "pending"
        /\ row' = [row EXCEPT ![r] = "interrupted"]
    /\ UNCHANGED <<revertedBy, files, plan, pc, job, cur, force, asked, crashes, recovered>>

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
    /\ UNCHANGED <<revertedBy, row, files, plan, asked>>

Next ==
    \/ YouAskCoreToUndo
    \/ AnotherWriteStarts
    \/ RecoveryReleasesClaim
    \/ RecoveryMarksRowInterrupted
    \/ RecoveryDoneWithRows
    \/ RecoveryReversesPlan
    \/ CoreChecks
    \/ CoreRecordsRestoreRow
    \/ CoreClaims
    \/ CoreSwapsFilesIn
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
    \/ DesktopPutsFilesBack
    \/ DesktopWriteFails
    \/ DesktopFinishesFailed
    \/ DesktopReleasesAfterFailure
    \/ DesktopFinishesDone
    \/ DesktopStartsUp
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

\* A change whose undo finished has its files back as they were before it.
UndoneMeansFilesBack ==
    (revertedBy # None /\ row[revertedBy] = "done") => files = "restored"

\* A change is undone at most once.
OneFinishedUndo ==
    Cardinality({r \in Attempts : row[r] = "done"}) <= 1

(***************************************************************************)
(* For the chart configs only. A recovery pass that starts when nothing is *)
(* left to recover changes nothing, so the chart leaves it out. The check  *)
(* configs do not use this.                                                *)
(***************************************************************************)
ChartLeavesOutIdleRecovery == ~(job = "write" /\ recovered)

=============================================================================
