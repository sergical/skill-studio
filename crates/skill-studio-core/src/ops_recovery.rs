//! `recovery list` and `recovery drop`: the way out of a journal plan that
//! recovery cannot undo. Every write first undoes the open plans; while one
//! stays `Interrupted`, every write refuses with `recovery_pending`. The
//! list shows each open plan with the paths it may have left half done, and
//! drop gives one up through the journal, with a history event that keeps
//! the same facts.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, ErrorCode};
use crate::events::{EventDraft, EventKind, EventStatus};
use crate::identity::{EventId, PlanId, SkillName};
use crate::journal::FsJournal;
use crate::ops::{Operation, Outcome};
use crate::ports::{Journal, OpContext, PlanRecord, PlanStatus, Runtime, WriteHold};

/// One journal plan that recovery has not closed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecoveryPlan {
    /// The plan's ID, for `recovery drop`. Read from the journal folder name.
    #[schemars(extend("x-untrusted" = true))]
    pub id: PlanId,
    /// The write the plan belongs to.
    #[schemars(extend("x-untrusted" = true))]
    pub label: String,
    /// When the write began.
    pub created_at: DateTime<Utc>,
    /// The folder the write ran under.
    #[schemars(extend("x-untrusted" = true))]
    pub root: PathBuf,
    /// `pending` until the next write tries to undo it, `interrupted` after a
    /// run failed, `dropped` after `recovery drop`.
    pub status: PlanStatus,
    /// Recovery runs that failed to undo the plan.
    pub recovery_attempts: u32,
    /// What the last failed run hit.
    #[schemars(extend("x-untrusted" = true))]
    pub recovery_error: Option<String>,
    /// Paths the write touched. Each one can hold the old state, the new
    /// state, or a half-done change.
    #[schemars(extend("x-untrusted" = true))]
    pub touched_paths: Vec<PathBuf>,
    /// Saved copies of the files the write changed.
    pub backups: Vec<RecoveryBackup>,
}

/// A file a plan saved before it changed it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecoveryBackup {
    /// The file the copy came from.
    #[schemars(extend("x-untrusted" = true))]
    pub original: PathBuf,
    /// Where the saved copy is.
    #[schemars(extend("x-untrusted" = true))]
    pub saved: PathBuf,
}

/// Every plan recovery has not closed, oldest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecoveryReport {
    /// Open plans. Empty when writes are not blocked.
    pub plans: Vec<RecoveryPlan>,
}

impl Outcome for RecoveryReport {
    const LISTS: &'static [&'static str] = &["/plans"];
}

/// Request to give up one interrupted plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecoveryDropRequest {
    /// ID from `recovery list`.
    pub plan_id: PlanId,
}

/// Result of `recovery drop`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecoveryDropOutcome {
    /// The `recovery_plan_dropped` history event.
    pub event_id: EventId,
    /// The plan as it is now, with what it may have left on disk.
    pub plan: RecoveryPlan,
}

impl Outcome for RecoveryDropOutcome {
    fn event_id(&self) -> Option<EventId> {
        Some(self.event_id.clone())
    }
}

fn scope_journal(rt: &Runtime) -> FsJournal {
    FsJournal::new(
        crate::ops_install::journal_root(&rt.scope.home.lexical),
        rt.ports.fs.clone(),
    )
}

fn describe(journal: &FsJournal, plan: PlanRecord) -> RecoveryPlan {
    let mut touched_paths = Vec::new();
    for path in plan.steps.iter().flat_map(crate::journal::step_paths) {
        if !touched_paths.iter().any(|known: &PathBuf| known == path) {
            touched_paths.push(path.to_path_buf());
        }
    }
    let backups = plan
        .backups
        .iter()
        .map(|backup| RecoveryBackup {
            original: backup.original.clone(),
            saved: journal.backup_path(&plan.id, &backup.relative),
        })
        .collect();
    RecoveryPlan {
        id: plan.id,
        label: plan.label,
        created_at: plan.created_at,
        root: plan.root,
        status: plan.status,
        recovery_attempts: plan.recovery_attempts,
        recovery_error: plan.recovery_error,
        touched_paths,
        backups,
    }
}

/// Lists the plans that block writes or will be undone by the next one.
/// Reads the journal only: takes no lease and changes nothing.
pub fn recovery_list(rt: &Runtime, ctx: &OpContext) -> Result<RecoveryReport, CoreError> {
    rt.run(Operation::RecoveryList, ctx, || {
        ctx.checkpoint()?;
        let journal = scope_journal(rt);
        let plans = journal
            .pending()?
            .into_iter()
            .map(|plan| describe(&journal, plan))
            .collect();
        Ok(RecoveryReport { plans })
    })
}

/// Marks one `Interrupted` plan `Dropped`, so recovery stops retrying it
/// and writes run again. Changes no file the plan touched: the person
/// checks those paths by hand. Records a `recovery_plan_dropped` event
/// first, with the plan as its payload, and has no undo.
///
/// Does not open a [`crate::ports::MutationSession`]: that runs recovery
/// first and would refuse on the plan this call exists to drop.
pub fn recovery_drop(
    rt: &Runtime,
    ctx: &OpContext,
    req: &RecoveryDropRequest,
) -> Result<RecoveryDropOutcome, CoreError> {
    rt.run(Operation::RecoveryDrop, ctx, || {
        let mut hold = WriteHold::take(rt, ctx)?;
        let journal = scope_journal(rt);
        let plan = journal
            .all()?
            .into_iter()
            .find(|plan| plan.id == req.plan_id)
            .ok_or_else(|| {
                CoreError::new(
                    ErrorCode::InvalidRequest,
                    format!("no recovery plan has the ID {}", req.plan_id.0),
                )
            })?;
        if plan.status != PlanStatus::Interrupted {
            return Err(CoreError::new(
                ErrorCode::InvalidRequest,
                format!(
                    "only an interrupted plan can be dropped; plan {} is {:?}",
                    plan.id.0, plan.status
                ),
            ));
        }
        let mut dropped = describe(&journal, plan);
        dropped.status = PlanStatus::Dropped;
        let event_id = rt.ports.ids.next_event_id();
        let payload = serde_json::to_value(&dropped)
            .map_err(|error| CoreError::new(ErrorCode::Io, error.to_string()))?;
        hold.store.record(
            &hold.guard,
            &event_id,
            &EventDraft {
                kind: EventKind::RecoveryPlanDropped,
                skill: SkillName(String::new()),
                harness: None,
                scope: None,
                project_path: None,
                payload,
                inverse: None,
                backup_dir: None,
            },
        )?;
        if let Err(error) = journal.finish(&hold.guard, &dropped.id, PlanStatus::Dropped) {
            hold.store
                .finish(&hold.guard, &event_id, EventStatus::Failed, None)?;
            return Err(error);
        }
        hold.store
            .finish(&hold.guard, &event_id, EventStatus::Done, None)?;
        Ok(RecoveryDropOutcome {
            event_id,
            plan: dropped,
        })
    })
}
