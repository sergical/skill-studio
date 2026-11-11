//! Journal: the crash-safety primitive every `fsops` call records against.
//!
//! A plan's manifest and the plan itself are fsynced to disk, through
//! [`Journal::begin`], before the plan's first step runs - see
//! `docs/action-map/plan.md` unit 1.2. [`FsJournal`] is the reference
//! [`Journal`] implementation, built only on [`ScopeFs`] so a host can reuse
//! it verbatim rather than reimplementing the write order. [`reconcile`]
//! resolves every plan a crash left `Pending` by undoing its recorded steps
//! in reverse - or, for a plan with none, marking it `Failed` since nothing
//! mutated anything yet. [`trim_backups`] applies count, age, Undo and recovery protections to
//! the backups plans keep for undo. Neither one ever deletes a plan row.
//!
//! [`PlanWriter`]'s `record_*` methods are how `crate::fsops`'s four
//! primitives themselves record the step that ran - see that module - so a
//! caller cannot call a primitive without a plan step landing for it. Each
//! `record_*` call happens *before* its primitive's own mutation, with
//! everything reversal will need - the quarantine path, the previous link
//! target, the pre-write backup - already computed and durable: `stage`
//! records its temp path before creating it, `swap` records the exchange's
//! quarantine destination before running it, `link` records the previous
//! target before renaming the new one into place, and `write_file` fsyncs
//! its backup before renaming the new bytes into place. A crash can now only
//! ever land between "step recorded" and "mutation done", never the other
//! way around, so [`reverse_steps`] inspects the disk for each step to tell
//! whether its mutation actually landed and reverses only what did -
//! idempotent, since a step whose mutation never landed is already a no-op
//! to reverse.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};

use crate::error::{CoreError, ErrorCode};
use crate::fsops::{self, Root};
use crate::identity::PlanId;
use crate::ports::{
    ExclusiveGuard, FileKind, Journal, PlanBackupEntry, PlanRecord, PlanStatus, PlanStep, ScopeFs,
};

/// Caps how many bytes [`FsJournal`] will read back for one plan or manifest
/// file; a journal file this large is corrupt, not merely large.
const MAX_JOURNAL_JSON_BYTES: u64 = 16 * 1024 * 1024;

/// Caps how many bytes [`FsJournal::read_backup`] will read back for one
/// `write_file` backup. `fsops::write_file` reads the pre-write bytes it
/// hands to [`Journal::write_backup`] with `u64::MAX` - a user's file is
/// not a journal-corruption signal the way an oversized plan/manifest JSON
/// file is - so readback matches that instead of reusing
/// `MAX_JOURNAL_JSON_BYTES`: a backup larger than that cap must still be
/// restorable, not leave the plan `Interrupted` with the post-write bytes
/// stuck in place.
const MAX_BACKUP_BYTES: u64 = u64::MAX;

/// A [`Journal`] implementation built only on [`ScopeFs`], so both the core's
/// own tests and a host adapter can use it directly.
///
/// Layout under `root`: `plans/<id>/manifest.json` (the plan's
/// [`PlanBackupEntry`] list, written first), `plans/<id>/plan.json` (the
/// full [`PlanRecord`], written second), and `plans/<id>/backups/<relative>`
/// for the backed-up bytes [`Self::remove_backup`] trims.
pub struct FsJournal {
    root: PathBuf,
    fs: Arc<dyn ScopeFs>,
}

impl FsJournal {
    /// Roots the journal at `root`, reading and writing through `fs`.
    pub fn new(root: PathBuf, fs: Arc<dyn ScopeFs>) -> Self {
        FsJournal { root, fs }
    }

    fn validate_id(id: &PlanId) -> Result<(), CoreError> {
        let mut components = Path::new(&id.0).components();
        if !matches!(components.next(), Some(std::path::Component::Normal(_)))
            || components.next().is_some()
        {
            return Err(CoreError::new(
                ErrorCode::InvalidRequest,
                "a plan ID must name one folder",
            ));
        }
        Ok(())
    }

    fn checked_backup_path(&self, id: &PlanId, relative: &str) -> Result<PathBuf, CoreError> {
        Self::validate_id(id)?;
        if relative.is_empty()
            || Path::new(relative)
                .components()
                .any(|component| !matches!(component, std::path::Component::Normal(_)))
        {
            return Err(CoreError::new(
                ErrorCode::InvalidRequest,
                "a backup path must stay inside its plan",
            ));
        }
        let root = Root::open(self.fs.as_ref(), self.root.clone())
            .map_err(|error| CoreError::new(ErrorCode::InvalidRequest, error.to_string()))?;
        let path = PathBuf::from("plans")
            .join(&id.0)
            .join("backups")
            .join(relative);
        root.confine(&path)
            .map_err(|error| CoreError::new(ErrorCode::InvalidRequest, error.to_string()))
    }

    fn confine_journal_path(&self, path: &Path) -> std::io::Result<()> {
        let relative = path
            .strip_prefix(&self.root)
            .map_err(std::io::Error::other)?;
        let root =
            Root::open(self.fs.as_ref(), self.root.clone()).map_err(std::io::Error::other)?;
        root.confine(relative)
            .map(|_| ())
            .map_err(std::io::Error::other)
    }

    fn plan_dir(&self, id: &PlanId) -> PathBuf {
        self.root.join("plans").join(&id.0)
    }

    fn manifest_path(&self, id: &PlanId) -> PathBuf {
        self.plan_dir(id).join("manifest.json")
    }

    fn plan_path(&self, id: &PlanId) -> PathBuf {
        self.plan_dir(id).join("plan.json")
    }

    #[cfg(test)]
    fn backup_path(&self, id: &PlanId, relative: &str) -> PathBuf {
        self.plan_dir(id).join("backups").join(relative)
    }

    /// Creates `path` and every missing ancestor under `root`, fsyncing each
    /// one it creates. A no-op when `path` already exists.
    fn ensure_dir(&self, path: &Path) -> std::io::Result<()> {
        if self.fs.symlink_metadata(path).is_ok() {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            if parent != path {
                self.ensure_dir(parent)?;
            }
        }
        match self.fs.fsops_create_dir(path) {
            Ok(()) => {
                self.fs.fsops_fsync_dir(path)?;
                if let Some(parent) = path.parent() {
                    self.fs.fsops_fsync_dir(parent)?;
                }
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Writes `bytes` to `path` through `tmp`, fsync, and rename, so `path`
    /// only ever shows a complete write - the same durability
    /// `fsops::write_file` gives a caller's own files.
    fn write_through_tmp(&self, path: &Path, tmp: &Path, bytes: &[u8]) -> std::io::Result<()> {
        let parent = path.parent().unwrap_or(path);
        match self.fs.symlink_metadata(&self.root) {
            Ok(_) => {
                self.confine_journal_path(path)?;
                self.confine_journal_path(tmp)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        self.ensure_dir(parent)?;
        self.confine_journal_path(path)?;
        self.confine_journal_path(tmp)?;
        if self.fs.symlink_metadata(tmp).is_ok() {
            self.fs.fsops_remove_file(tmp)?;
        }
        self.fs.fsops_write_new_file(tmp, bytes)?;
        self.fs.fsops_fsync_file(tmp)?;
        self.fs.fsops_rename(tmp, path)?;
        self.fs.fsops_fsync_dir(parent)
    }

    /// Writes JSON `bytes` to `path` durably - see [`Self::write_through_tmp`].
    fn write_json(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        self.write_through_tmp(path, &path.with_extension("json.tmp"), bytes)
    }

    /// Writes raw `bytes` to `path` durably - see [`Self::write_through_tmp`].
    fn write_bytes(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        self.write_through_tmp(path, &path.with_extension("tmp"), bytes)
    }

    fn read_json<T: serde::de::DeserializeOwned>(&self, path: &Path) -> Result<T, CoreError> {
        self.confine_journal_path(path)
            .map_err(|error| CoreError::io(path, error))?;
        let facts = self
            .fs
            .symlink_metadata(path)
            .map_err(|error| CoreError::io(path, error))?;
        if facts.kind != FileKind::File {
            return Err(CoreError::new(
                ErrorCode::InvalidRequest,
                "journal metadata must be a regular file",
            )
            .at(path));
        }
        let bytes = self
            .fs
            .read_capped(path, MAX_JOURNAL_JSON_BYTES)
            .map_err(|e| CoreError::io(path, e))?;
        serde_json::from_slice(&bytes).map_err(|e| {
            CoreError::new(ErrorCode::Io, format!("corrupt journal file: {e}")).at(path)
        })
    }

    fn write_record(&self, record: &PlanRecord) -> Result<(), CoreError> {
        Self::validate_id(&record.id)?;
        let path = self.plan_path(&record.id);
        let bytes = serde_json::to_vec_pretty(record)
            .map_err(|e| CoreError::new(ErrorCode::Io, e.to_string()).at(&path))?;
        self.write_json(&path, &bytes)
            .map_err(|e| CoreError::io(path, e))
    }
}

impl Journal for FsJournal {
    fn begin(&self, _guard: &ExclusiveGuard, plan: &PlanRecord) -> Result<(), CoreError> {
        Self::validate_id(&plan.id)?;
        if plan.status != PlanStatus::Pending {
            return Err(CoreError::new(
                ErrorCode::InvalidRequest,
                "a plan must begin Pending",
            ));
        }
        let manifest_path = self.manifest_path(&plan.id);
        let manifest_bytes = serde_json::to_vec_pretty(&plan.backups)
            .map_err(|e| CoreError::new(ErrorCode::Io, e.to_string()).at(&manifest_path))?;
        self.write_json(&manifest_path, &manifest_bytes)
            .map_err(|e| CoreError::io(manifest_path, e))?;
        // The plan itself is written only after the manifest above is
        // durable: a crash between the two leaves an orphaned manifest and
        // no plan row, never a plan row whose manifest never landed.
        self.write_record(plan)
    }

    fn record_step(
        &self,
        _guard: &ExclusiveGuard,
        id: &PlanId,
        step: PlanStep,
    ) -> Result<(), CoreError> {
        Self::validate_id(id)?;
        let mut record: PlanRecord = self.read_json(&self.plan_path(id))?;
        // A `WriteFile` step's backup was written by `write_backup` before
        // this call, but the manifest `begin` fsynced only knows the
        // backups the caller had at the plan's start; append it here too so
        // `trim_backups` and a later `all()` both see it.
        if let PlanStep::WriteFile {
            backup: Some(entry),
            ..
        } = &step
        {
            record.backups.push(entry.clone());
        }
        record.steps.push(step);
        self.write_record(&record)
    }

    fn finish(
        &self,
        guard: &ExclusiveGuard,
        id: &PlanId,
        status: PlanStatus,
    ) -> Result<(), CoreError> {
        Self::validate_id(id)?;
        let mut record: PlanRecord = self.read_json(&self.plan_path(id))?;
        record.status = record.status.settle(status)?;
        self.write_record(&record)?;
        if status == PlanStatus::Done {
            // The write has committed. Failed cleanup is retried at startup.
            let _ = trim_backups(self, guard, Utc::now());
        }
        Ok(())
    }

    fn visit(&self, visit: &mut dyn FnMut(PlanRecord)) -> Result<(), CoreError> {
        let plans_dir = self.root.join("plans");
        let mut failure = None;
        let result = self.fs.visit_dir(&plans_dir, &mut |entry| {
            if entry.kind != FileKind::Dir || failure.is_some() {
                return;
            }
            let path = plans_dir.join(&entry.name).join("plan.json");
            match self.fs.symlink_metadata(&path) {
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
                Err(error) => {
                    failure = Some(CoreError::io(path, error));
                    return;
                }
            }
            let record = self.read_json::<PlanRecord>(&path).and_then(|record| {
                Self::validate_id(&record.id)?;
                if record.id.0 != entry.name {
                    return Err(CoreError::new(
                        ErrorCode::InvalidRequest,
                        "the plan ID does not match its folder",
                    )
                    .at(path));
                }
                Ok(record)
            });
            match record {
                Ok(plan) => visit(plan),
                Err(error) => failure = Some(error),
            }
        });
        match result {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(CoreError::io(plans_dir, error)),
            Ok(()) => {}
        }
        failure.map_or(Ok(()), Err)
    }

    fn all(&self) -> Result<Vec<PlanRecord>, CoreError> {
        let mut records = Vec::new();
        self.visit(&mut |plan| records.push(plan))?;
        records.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(records)
    }

    fn pending(&self) -> Result<Vec<PlanRecord>, CoreError> {
        Ok(self
            .all()?
            .into_iter()
            .filter(|p| matches!(p.status, PlanStatus::Pending | PlanStatus::Interrupted))
            .collect())
    }

    fn remove_backup(
        &self,
        _guard: &ExclusiveGuard,
        id: &PlanId,
        relative: &str,
    ) -> Result<(), CoreError> {
        Self::validate_id(id)?;
        let mut record: PlanRecord = self.read_json(&self.plan_path(id))?;
        match record.status {
            PlanStatus::Pending | PlanStatus::Interrupted => {
                return Err(CoreError::new(
                    ErrorCode::InvalidRequest,
                    "recovery still needs this plan's backups",
                ))
            }
            PlanStatus::Done | PlanStatus::Failed | PlanStatus::Reversed => {}
        }
        let path = self.checked_backup_path(id, relative)?;
        if !record
            .backups
            .iter()
            .any(|backup| backup.relative == relative)
        {
            return Err(CoreError::new(
                ErrorCode::InvalidRequest,
                "the backup is not recorded by this plan",
            ));
        }
        if !record.expired_backups.iter().any(|name| name == relative) {
            record.expired_backups.push(relative.to_owned());
            self.write_record(&record)?;
        }
        match self.fs.fsops_remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(CoreError::io(&path, error)),
        }
        if let Some(parent) = path.parent() {
            self.fs
                .fsops_fsync_dir(parent)
                .map_err(|error| CoreError::io(parent, error))?;
        }
        Ok(())
    }

    fn write_backup(
        &self,
        _guard: &ExclusiveGuard,
        id: &PlanId,
        relative: &str,
        bytes: &[u8],
    ) -> Result<(), CoreError> {
        let path = self.checked_backup_path(id, relative)?;
        self.write_bytes(&path, bytes)
            .map_err(|e| CoreError::io(path, e))
    }

    fn read_backup(&self, id: &PlanId, relative: &str) -> Result<Vec<u8>, CoreError> {
        Self::validate_id(id)?;
        let record: PlanRecord = self.read_json(&self.plan_path(id))?;
        if record.expired_backups.iter().any(|name| name == relative) {
            return Err(CoreError::new(
                ErrorCode::BackupExpired,
                crate::backup_retention::BACKUP_EXPIRED_REASON,
            ));
        }
        let path = self.checked_backup_path(id, relative)?;
        let facts = self
            .fs
            .symlink_metadata(&path)
            .map_err(|error| CoreError::io(&path, error))?;
        if facts.kind != FileKind::File {
            return Err(CoreError::new(
                ErrorCode::InvalidRequest,
                "a plan backup must be a regular file",
            )
            .at(&path));
        }
        self.fs
            .read_capped(&path, MAX_BACKUP_BYTES)
            .map_err(|e| CoreError::io(path, e))
    }
}

/// A plan begun through [`Journal::begin`], held open while its steps run.
///
/// Invariant: dropping a `PlanWriter` without calling [`Self::finish`]
/// leaves the plan `Pending` on disk - the same state a real crash leaves -
/// so [`reconcile`] is the only thing that resolves it.
pub struct PlanWriter<'a> {
    journal: &'a dyn Journal,
    guard: &'a ExclusiveGuard,
    id: PlanId,
}

impl<'a> PlanWriter<'a> {
    /// Begins a plan: writes its manifest and itself, both durable, before
    /// returning. Nothing the caller does after a successful `begin` can be
    /// an unrecorded first step.
    pub fn begin(
        journal: &'a dyn Journal,
        guard: &'a ExclusiveGuard,
        id: PlanId,
        created_at: DateTime<Utc>,
        label: impl Into<String>,
        root: PathBuf,
        backups: Vec<PlanBackupEntry>,
    ) -> Result<Self, CoreError> {
        let record = PlanRecord {
            id: id.clone(),
            created_at,
            label: label.into(),
            root,
            backups,
            expired_backups: Vec::new(),
            steps: Vec::new(),
            status: PlanStatus::Pending,
        };
        journal.begin(guard, &record)?;
        Ok(PlanWriter { journal, guard, id })
    }

    /// The plan's id.
    pub fn id(&self) -> &PlanId {
        &self.id
    }

    /// Records that [`crate::fsops::stage`] is about to build `staged` (its
    /// temp path). Called before the folder is created, so a crash before
    /// it exists leaves nothing for reversal's `remove_tree` to remove -
    /// nothing else has touched it either way.
    pub fn record_stage(&self, staged: &Path) -> Result<(), CoreError> {
        self.journal.record_step(
            self.guard,
            &self.id,
            PlanStep::Stage {
                staged: staged.to_path_buf(),
            },
        )
    }

    /// Records that [`crate::fsops::swap`] is about to put a folder at
    /// `path` by exchanging it with `staged` (captured at `staged_binding`,
    /// its device/inode right before the exchange). `quarantined` is where
    /// the folder that sits at `path` before the exchange will be relocated
    /// to, when one is there - `None` when `path` does not exist yet
    /// (`swap` will create it fresh). Called before the exchange runs.
    pub fn record_swap(
        &self,
        path: &Path,
        staged: &Path,
        staged_binding: (u64, u64),
        quarantined: Option<PathBuf>,
    ) -> Result<(), CoreError> {
        self.record_swap_with_parent(path, staged, staged_binding, quarantined, None)
    }

    /// Records the quarantine directory before the first create can land.
    pub fn record_swap_with_parent(
        &self,
        path: &Path,
        staged: &Path,
        staged_binding: (u64, u64),
        quarantined: Option<PathBuf>,
        created_quarantine_dir: Option<PathBuf>,
    ) -> Result<(), CoreError> {
        self.journal.record_step(
            self.guard,
            &self.id,
            PlanStep::Swap {
                path: path.to_path_buf(),
                staged: staged.to_path_buf(),
                staged_binding,
                quarantined,
                created_quarantine_dir,
            },
        )
    }

    /// Records that [`crate::fsops::link`] is about to set `path` to point
    /// at `target`. `previous_target` is what `path` pointed at before,
    /// when it already existed as a symlink. Called before the rename that
    /// makes the new link visible.
    pub fn record_link(
        &self,
        path: &Path,
        target: &Path,
        previous_target: Option<PathBuf>,
    ) -> Result<(), CoreError> {
        self.record_link_with_temp(path, target, previous_target, None)
    }

    /// Records the exact link temp name before creating it.
    pub fn record_link_with_temp(
        &self,
        path: &Path,
        target: &Path,
        previous_target: Option<PathBuf>,
        temporary: Option<PathBuf>,
    ) -> Result<(), CoreError> {
        self.journal.record_step(
            self.guard,
            &self.id,
            PlanStep::Link {
                path: path.to_path_buf(),
                target: target.to_path_buf(),
                previous_target,
                temporary,
            },
        )
    }

    /// Records that [`crate::fsops::write_file`] is about to write `path`.
    /// When `previous` holds the file's pre-write bytes, writes them
    /// durably into this plan's own backup store first (via
    /// [`Journal::write_backup`]) and points the recorded step at that
    /// backup, so reversal can read them back. Called before the rename
    /// that puts the new bytes in place.
    pub fn record_write_file(&self, path: &Path, previous: Option<&[u8]>) -> Result<(), CoreError> {
        self.record_write_file_with_state(path, previous, None, None, None)
    }

    /// Records the original kind, mode and exact temp name for reversal.
    pub fn record_write_file_with_state(
        &self,
        path: &Path,
        previous: Option<&[u8]>,
        temporary: Option<PathBuf>,
        previous_link: Option<PathBuf>,
        previous_mode: Option<u32>,
    ) -> Result<(), CoreError> {
        let backup = match previous {
            Some(bytes) => {
                let relative = backup_relative_name(path);
                self.journal
                    .write_backup(self.guard, &self.id, &relative, bytes)?;
                Some(PlanBackupEntry {
                    original: path.to_path_buf(),
                    relative,
                    bytes: bytes.len() as u64,
                })
            }
            None => None,
        };
        self.journal.record_step(
            self.guard,
            &self.id,
            PlanStep::WriteFile {
                path: path.to_path_buf(),
                backup,
                temporary,
                previous_link,
                previous_mode,
            },
        )
    }

    /// Sets the plan's final status. Never `Pending`.
    pub fn finish(self, status: PlanStatus) -> Result<(), CoreError> {
        self.journal.finish(self.guard, &self.id, status)
    }
}

/// Turns an absolute path into a name unique enough to use as a backup's
/// `relative` under the plan's own backup directory - the leaf name plus a
/// counter, so two `write_file` steps against files with the same leaf
/// name never collide.
fn backup_relative_name(path: &Path) -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let leaf = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("backup");
    format!("{n}-{leaf}")
}

/// A plan [`reconcile`] found still `Pending` and could not undo, listed
/// with the I/O error reversal hit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterruptedPlan {
    /// The plan's id.
    pub id: PlanId,
    /// What went wrong undoing it.
    pub error: String,
}

/// What startup reconciliation did to every plan it found `Pending`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reconciliation {
    /// Plans that recorded at least one step and were fully undone, in
    /// reverse order, through [`ScopeFs`]: marked `Reversed`.
    pub reversed: Vec<PlanId>,
    /// Plans that recorded at least one step but undoing one hit an I/O
    /// error: marked `Interrupted` and listed with that error, since the
    /// on-disk state can no longer be trusted as either the pre-plan or the
    /// post-plan shape.
    pub interrupted: Vec<InterruptedPlan>,
    /// Plans that recorded no step at all: nothing had mutated anything
    /// yet, so the plan is resolved by marking it `Failed`, the same
    /// terminal state a plan that failed its own first step would reach.
    pub resolved_without_steps: Vec<PlanId>,
}

/// Resolves every plan [`Journal::pending`] still reports, newest first
/// (LIFO), never deleting a row: a plan with recorded steps has each one
/// undone, in reverse order, through `fs` - `Reversed` on success,
/// `Interrupted` (listed with the error) the moment one step's undo fails.
/// A plan with no recorded steps becomes `Failed` (nothing on disk needed
/// undoing). Idempotent - a second call finds nothing left `Pending` (a
/// reversed plan stays settled; interrupted recovery is retried).
///
/// Newest-first matters when two pending plans stack on the same path -
/// plan A swaps it, left `Pending`; plan B swaps it again, over A's result,
/// and also crashes `Pending`. Oldest-first would reverse A while B's
/// result still sits at the path: A's landed check finds someone else's
/// binding there, treats A as never landed, and marks it `Reversed`
/// without restoring anything; B then reverses correctly, but onto A's
/// (wrong) starting point, stranding what A actually replaced in A's own
/// quarantine folder. Reversing B first before A restores each plan's own
/// precondition in turn, the same order a stack of edits always undoes in.
pub fn reconcile(
    journal: &dyn Journal,
    guard: &ExclusiveGuard,
    fs: &dyn ScopeFs,
) -> Result<Reconciliation, CoreError> {
    let mut report = Reconciliation::default();
    for plan in journal.pending()?.into_iter().rev() {
        let canonical_root = fs
            .canonicalize(&plan.root)
            .map_err(|error| CoreError::io(&plan.root, error));
        let owned = canonical_root.as_ref().is_ok_and(|root| {
            guard
                .keys()
                .iter()
                .any(|key| root.starts_with(&key.canonical_root))
        });
        let (status, reversal_error) = if !owned {
            (
                PlanStatus::Interrupted,
                Some(format!(
                    "{}: recovery root is not covered by the held write lease",
                    plan.root.display()
                )),
            )
        } else if plan.steps.is_empty() {
            (PlanStatus::Failed, None)
        } else {
            match reverse_steps(journal, &plan, fs) {
                Ok(()) => (PlanStatus::Reversed, None),
                Err(error) => (PlanStatus::Interrupted, Some(error.to_string())),
            }
        };
        if let Err(error) = journal.finish(guard, &plan.id, status) {
            report.interrupted.push(InterruptedPlan {
                id: plan.id,
                error: error.to_string(),
            });
            continue;
        }
        match status {
            PlanStatus::Reversed => report.reversed.push(plan.id),
            PlanStatus::Failed => report.resolved_without_steps.push(plan.id),
            PlanStatus::Interrupted => report.interrupted.push(InterruptedPlan {
                id: plan.id,
                error: reversal_error.unwrap_or_else(|| "recovery did not finish".into()),
            }),
            PlanStatus::Pending | PlanStatus::Done => {
                return Err(CoreError::new(
                    ErrorCode::InvalidRequest,
                    "invalid recovery settlement",
                ))
            }
        }
    }
    Ok(report)
}

/// Recovers the core journal using the same trusted roots and lease as a write.
pub fn reconcile_scope_journal(
    rt: &crate::ports::Runtime,
    guard: &ExclusiveGuard,
) -> Result<Reconciliation, CoreError> {
    let journal = FsJournal::new(
        crate::ops_install::journal_root(&rt.scope.home.lexical),
        rt.ports.fs.clone(),
    );
    let report = reconcile(&journal, guard, rt.ports.fs.as_ref())?;
    if !report.interrupted.is_empty() {
        let plans = report
            .interrupted
            .iter()
            .map(|plan| plan.id.0.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(CoreError::new(
            ErrorCode::Incomplete,
            format!("Recovery did not finish for plans: {plans}. No new write ran."),
        ));
    }
    trim_backups(&journal, guard, rt.ports.clock.now())?;
    Ok(report)
}

/// Every path inside `step` that reversal would write to - what
/// [`reverse_steps`] confines under the plan's root before touching
/// anything for that step.
fn step_paths(step: &PlanStep) -> Vec<&Path> {
    match step {
        PlanStep::Stage { staged } => vec![staged],
        PlanStep::Swap {
            path,
            staged,
            quarantined,
            created_quarantine_dir,
            ..
        } => {
            let mut paths = vec![path.as_path(), staged.as_path()];
            if let Some(quarantined) = quarantined {
                paths.push(quarantined);
            }
            if let Some(parent) = created_quarantine_dir {
                paths.push(parent);
            }
            paths
        }
        PlanStep::Link {
            path, temporary, ..
        }
        | PlanStep::WriteFile {
            path, temporary, ..
        } => {
            let mut paths = vec![path.as_path()];
            if let Some(temporary) = temporary {
                paths.push(temporary);
            }
            paths
        }
    }
}

/// Confines `path` under `plan_root` via [`Root::confine`], so a corrupt or
/// hand-edited plan row can never make reversal touch something outside the
/// plan's own root. `path` is already absolute (every `fsops` primitive
/// resolves it through `Root::confine` before recording it), so this is a
/// prefix strip followed by the same ancestor-symlink check `fsops` itself
/// runs, not a second, weaker check.
fn confine_to_root(root: &Root, plan_root: &Path, path: &Path) -> std::io::Result<()> {
    let relative = path.strip_prefix(plan_root).map_err(|_| {
        std::io::Error::other(format!(
            "{}: escapes the plan root {}",
            path.display(),
            plan_root.display()
        ))
    })?;
    root.confine(relative).map(|_| ()).map_err(|e| {
        std::io::Error::other(format!("{}: escapes the plan root ({e})", path.display()))
    })
}

fn sync_step_parents(fs: &dyn ScopeFs, step: &PlanStep) -> std::io::Result<()> {
    let parents: std::collections::BTreeSet<&Path> = step_paths(step)
        .into_iter()
        .filter_map(Path::parent)
        .collect();
    for parent in parents {
        match fs.fsops_fsync_dir(parent) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

/// Whether `name` is a temp link a link restore minted: the `.{leaf}-`
/// prefix followed by the `<pid>-<counter>` shape `fsops::unique_suffix`
/// produces. Anything looser would let the pre-restore sweep delete a
/// user's own dot-prefixed entry that merely shares the prefix.
fn is_link_restore_temp_name(name: &str, temp_prefix: &str) -> bool {
    let Some(suffix) = name.strip_prefix(temp_prefix) else {
        return false;
    };
    let mut parts = suffix.splitn(2, '-');
    let is_digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    matches!((parts.next(), parts.next()), (Some(pid), Some(n)) if is_digits(pid) && is_digits(n))
}

/// Undoes `plan`'s recorded steps in reverse order. Stops at the first
/// step whose undo fails; steps already undone stay undone (there is no
/// partial-undo rollback - a step's own undo is the smallest unit this
/// resolves).
///
/// Every step was recorded before its own mutation ran (see the module
/// doc), so a crash can only ever land between "recorded" and "mutation
/// landed" - never the reverse. Before undoing a step, this checks the disk
/// to tell whether the mutation it describes actually landed, and reverses
/// only what did:
/// - `Stage`: `remove_tree` is already a no-op when nothing is at `staged`.
/// - `Swap`: landed iff `path`'s current device/inode matches
///   `staged_binding` (the staged folder's identity, captured before the
///   exchange - a rename/exchange preserves identity across the name
///   change, so this holds regardless of whether the follow-up move into
///   `quarantined` also landed).
/// - `Link`: landed iff `path` is currently a symlink pointing at `target`,
///   or `path` is absent and `previous_target` is `Some` (a previous
///   reversal attempt that crashed mid-restore, back when the restore was
///   remove-then-create rather than the rename below). Restoring
///   `previous_target` itself goes through the same temp-name-then-rename
///   shape `fsops::link` uses, so `path` only ever shows a complete link -
///   never briefly absent - and a crash mid-restore is simply retried.
/// - `WriteFile`: landed iff the live bytes at `path` no longer match the
///   backup (the backup was fsynced durable before the rename that would
///   have changed them).
///
/// Each of these probes disk state through a fallible call (`read_link`,
/// `fsops_device_inode`, `read_capped`); only a `NotFound` error means "the
/// mutation never landed" - any other I/O error (permissions, EIO, ...)
/// leaves that unknown and is propagated, so the step's undo fails and the
/// plan resolves `Interrupted` instead of guessing "not landed" and
/// deleting or overwriting something that is, in fact, still live.
///
/// Every path a step names is confined under `plan.root` first (via
/// [`confine_to_root`]); a step naming a path outside it aborts reversal
/// immediately, naming the escaped path, without touching anything.
fn reverse_steps(
    journal: &dyn Journal,
    plan: &PlanRecord,
    fs: &dyn ScopeFs,
) -> std::io::Result<()> {
    let root =
        Root::open(fs, plan.root.clone()).map_err(|e| std::io::Error::other(e.to_string()))?;
    for step in plan.steps.iter().rev() {
        for path in step_paths(step) {
            confine_to_root(&root, &plan.root, path)?;
        }
        match step {
            PlanStep::Stage { staged } => remove_tree(fs, staged)?,
            PlanStep::Swap {
                path,
                staged,
                staged_binding,
                quarantined,
                created_quarantine_dir,
            } => {
                let landed = match fs.fsops_device_inode(path) {
                    Ok(inode) => inode == *staged_binding,
                    // Only a missing path means the exchange never ran -
                    // any other error (EACCES, EIO, ...) leaves whether it
                    // landed unknown, and guessing "no" here is what lets
                    // the paired Stage step's reversal delete a folder that
                    // is in fact still live at `path`.
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
                    Err(e) => return Err(e),
                };
                if landed {
                    match quarantined {
                        Some(quarantined) if fs.symlink_metadata(quarantined).is_ok() => {
                            // Both the exchange and the follow-up move into
                            // quarantine landed.
                            fs.fsops_exchange(quarantined, path)?;
                            remove_tree(fs, quarantined)?;
                        }
                        Some(_) => {
                            // The exchange landed but the move into quarantine
                            // never did: the pre-swap folder is still sitting
                            // at `staged`'s temp name. Exchange it back; the
                            // preceding `Stage` step's own reversal then
                            // removes what is left at `staged` (the folder
                            // this step is undoing).
                            fs.fsops_exchange(staged, path)?;
                        }
                        None => remove_tree(fs, path)?,
                    }
                }
                if let Some(quarantined) = quarantined {
                    match fs.fsops_device_inode(quarantined) {
                        Ok(binding) if binding == *staged_binding => remove_tree(fs, quarantined)?,
                        Ok(_) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => return Err(error),
                    }
                }
                if let Some(parent) = created_quarantine_dir {
                    match fs.fsops_remove_dir(parent) {
                        Ok(()) => {
                            if let Some(ancestor) = parent.parent() {
                                fs.fsops_fsync_dir(ancestor)?;
                            }
                        }
                        Err(error)
                            if matches!(
                                error.kind(),
                                std::io::ErrorKind::NotFound
                                    | std::io::ErrorKind::DirectoryNotEmpty
                            ) => {}
                        Err(error) => return Err(error),
                    }
                }
            }

            PlanStep::Link {
                path,
                target,
                previous_target,
                temporary,
            } => {
                if let Some(temporary) = temporary {
                    remove_tree(fs, temporary)?;
                }
                let landed = match fs.read_link(path) {
                    Ok(current) => current == *target,
                    // A missing path is still "landed" when a previous
                    // reversal attempt got as far as removing the link but
                    // crashed before restoring `previous_target` - back
                    // when that was two separate calls with no link at
                    // `path` in between. Any other error leaves whether it
                    // landed unknown.
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => previous_target.is_some(),
                    Err(e) => return Err(e),
                };
                if !landed {
                    sync_step_parents(fs, step)?;
                    continue;
                }
                match previous_target {
                    Some(previous_target) => {
                        // Same temp-name-then-rename shape `fsops::link`
                        // itself uses (fsops.rs): `path` only ever shows a
                        // complete link, so a crash mid-restore leaves the
                        // step still `landed` (still pointing at `target`,
                        // or already absent - both retried above) rather
                        // than losing the link entirely.
                        let parent = path.parent().unwrap_or(Path::new("/")).to_path_buf();
                        let leaf = path.file_name().and_then(|s| s.to_str()).unwrap_or("link");
                        let temp_prefix = format!(".{leaf}-");
                        // A crashed earlier restore attempt (this process or
                        // a previous one) may have left its own temp
                        // symlink under this same prefix behind - minted
                        // then never consumed because the crash landed
                        // before the rename below. Clear every such entry
                        // first so a retry always converges instead of
                        // accumulating orphaned temp links across retries.
                        // A temp this code minted points at
                        // `previous_target`; one `fsops::link` minted points
                        // at `target`. A symlink with the same name shape
                        // but any other target is a user's own and stays.
                        if temporary.is_none() {
                            for entry in fs.read_dir(&parent)? {
                                if entry.kind != FileKind::Symlink
                                    || !is_link_restore_temp_name(&entry.name, &temp_prefix)
                                {
                                    continue;
                                }
                                let stale = parent.join(&entry.name);
                                let points_at_ours = match fs.read_link(&stale) {
                                    Ok(current) => {
                                        current == *previous_target || current == *target
                                    }
                                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
                                    Err(e) => return Err(e),
                                };
                                if !points_at_ours {
                                    continue;
                                }
                                // Under the exclusive lease no other Skill Studio
                                // process removes the entry between `read_dir` and here, so the
                                // `NotFound` arm is defensive only.
                                match fs.fsops_remove_file(&stale) {
                                    Ok(()) => {}
                                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                                    Err(e) => return Err(e),
                                }
                            }
                        }
                        let tmp_path = temporary.clone().unwrap_or_else(|| {
                            parent.join(format!("{temp_prefix}{}", fsops::unique_suffix()))
                        });
                        fs.fsops_symlink(previous_target, &tmp_path)?;
                        fs.fsops_rename(&tmp_path, path)?;
                    }
                    None => fs.fsops_remove_file(path)?,
                }
                if let Some(parent) = path.parent() {
                    fs.fsops_fsync_dir(parent)?;
                }
            }
            PlanStep::WriteFile {
                path,
                backup,
                temporary,
                previous_link,
                previous_mode,
            } => {
                if let Some(temporary) = temporary {
                    remove_tree(fs, temporary)?;
                }
                if let Some(previous_link) = previous_link {
                    if fs.read_link(path).ok().as_ref() != Some(previous_link) {
                        let parent = path
                            .parent()
                            .ok_or_else(|| std::io::Error::other("write path has no parent"))?;
                        let temporary = temporary.clone().unwrap_or_else(|| {
                            parent.join(format!(".restore-{}", fsops::unique_suffix()))
                        });
                        fs.fsops_symlink(previous_link, &temporary)?;
                        fs.fsops_rename(&temporary, path)?;
                        fs.fsops_fsync_dir(parent)?;
                    }
                } else {
                    match backup {
                        Some(entry) => {
                            let backup_bytes = journal
                                .read_backup(&plan.id, &entry.relative)
                                .map_err(|e| std::io::Error::other(e.to_string()))?;
                            let landed = match fs.read_capped(path, u64::MAX) {
                                Ok(live) => live != backup_bytes,
                                // A backup means a file was at `path` before the
                                // plan, and the forward write's rename never leaves
                                // `path` empty. A missing file is what a reversal
                                // from before the atomic restore below left when it
                                // crashed between its remove and its write: still
                                // landed, and the backup is the only copy left.
                                Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
                                Err(e) => return Err(e),
                            };
                            if landed {
                                restore_file_from_backup(
                                    fs,
                                    path,
                                    &backup_bytes,
                                    temporary.as_deref(),
                                    *previous_mode,
                                )?;
                            }
                        }
                        None => {
                            remove_tree(fs, path)?;
                        }
                    }
                }
            }
        }
        sync_step_parents(fs, step)?;
        root.revalidate().map_err(std::io::Error::other)?;
    }
    Ok(())
}

/// Puts `backup` back at `path` through a temp file and a rename, so a
/// crash mid-restore leaves `path` showing either its post-write bytes or
/// the backup, never no file at all. Leftover temps from a crashed earlier
/// attempt are removed first, but only those holding exactly `backup`: a
/// file with the same name pattern and other bytes is the user's own.
fn restore_file_from_backup(
    fs: &dyn ScopeFs,
    path: &Path,
    backup: &[u8],
    temporary: Option<&Path>,
    mode: Option<u32>,
) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or(Path::new("/")).to_path_buf();
    let leaf = path.file_name().and_then(|s| s.to_str()).unwrap_or("write");
    let temp_prefix = format!(".{leaf}-");
    if temporary.is_none() {
        for entry in fs.read_dir(&parent)? {
            if entry.kind != FileKind::File || !is_file_restore_temp_name(&entry.name, &temp_prefix)
            {
                continue;
            }
            let stale = parent.join(&entry.name);
            let holds_backup = match fs.read_capped(&stale, u64::MAX) {
                Ok(bytes) => bytes == backup,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
                Err(e) => return Err(e),
            };
            if holds_backup {
                match fs.fsops_remove_file(&stale) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e),
                }
            }
        }
    }
    let tmp_path = temporary.map_or_else(
        || {
            parent.join(format!(
                "{temp_prefix}{}{FILE_RESTORE_TEMP_SUFFIX}",
                fsops::unique_suffix()
            ))
        },
        Path::to_path_buf,
    );
    fs.fsops_write_new_file_with_mode(&tmp_path, backup, mode.unwrap_or(0o644))?;
    fs.fsops_fsync_file(&tmp_path)?;
    fs.fsops_rename(&tmp_path, path)?;
    fs.fsops_fsync_dir(&parent)
}

const FILE_RESTORE_TEMP_SUFFIX: &str = ".restore";

fn is_file_restore_temp_name(name: &str, temp_prefix: &str) -> bool {
    name.strip_suffix(FILE_RESTORE_TEMP_SUFFIX)
        .is_some_and(|stem| is_link_restore_temp_name(stem, temp_prefix))
}

/// Removes `path` and everything under it, deepest first, using only the
/// per-entry primitives [`ScopeFs`] offers (there is no recursive remove on
/// the port itself). A no-op when nothing is at `path`.
fn remove_tree(fs: &dyn ScopeFs, path: &Path) -> std::io::Result<()> {
    let facts = match fs.symlink_metadata(path) {
        Ok(facts) => facts,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    if facts.kind != FileKind::Dir {
        fs.fsops_remove_file(path)?;
        return path
            .parent()
            .map_or(Ok(()), |parent| fs.fsops_fsync_dir(parent));
    }
    for entry in fs.read_dir(path)? {
        remove_tree(fs, &path.join(&entry.name))?;
    }
    fs.fsops_remove_dir(path)?;
    path.parent()
        .map_or(Ok(()), |parent| fs.fsops_fsync_dir(parent))
}

/// One backup [`trim_backups`] removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrimmedBackup {
    /// The plan the backup belonged to.
    pub plan: PlanId,
    /// The backup's relative path inside that plan.
    pub relative: String,
}

fn plan_backup_target(plan: &PlanRecord) -> Option<PathBuf> {
    plan.steps
        .iter()
        .find_map(|step| match step {
            PlanStep::Swap { path, .. } => Some(path),
            _ => None,
        })
        .or_else(|| plan.backups.first().map(|entry| &entry.original))
        .or_else(|| {
            plan.steps.iter().find_map(|step| match step {
                PlanStep::Link { path, .. } | PlanStep::WriteFile { path, .. } => Some(path),
                PlanStep::Stage { .. } | PlanStep::Swap { .. } => None,
            })
        })
        .map(|path| crate::backup_retention::skill_target(path))
}

/// Expires completed plan backups only when count, age, newest undoable
/// plan, and recovery protections all permit it. Pending and interrupted
/// plans remain protected. Rows stay; expiry is durable before deletion.
pub fn trim_backups(
    journal: &dyn Journal,
    guard: &ExclusiveGuard,
    now: DateTime<Utc>,
) -> Result<Vec<TrimmedBackup>, CoreError> {
    use crate::backup_retention::{BACKUP_RETENTION_AGE, BACKUP_RETENTION_COUNT};
    let mut plans = journal.all()?;
    plans.sort_by(|left, right| {
        right
            .created_at
            .cmp(&left.created_at)
            .then_with(|| right.id.cmp(&left.id))
    });
    let mut targets: std::collections::BTreeMap<PathBuf, Vec<(&PlanRecord, &PlanBackupEntry)>> =
        std::collections::BTreeMap::new();
    let mut trimmed = Vec::new();
    for plan in &plans {
        match plan.status {
            PlanStatus::Pending | PlanStatus::Interrupted => continue,
            PlanStatus::Done | PlanStatus::Failed | PlanStatus::Reversed => {}
        }
        for backup in &plan.backups {
            if !guard
                .keys()
                .iter()
                .any(|key| backup.original.starts_with(&key.canonical_root))
            {
                continue;
            }
            if plan.expired_backups.contains(&backup.relative) {
                journal.remove_backup(guard, &plan.id, &backup.relative)?;
                continue;
            }
            let Some(target) = plan_backup_target(plan) else {
                continue;
            };
            targets.entry(target).or_default().push((plan, backup));
        }
    }
    for (target, backups) in &targets {
        let newest_undoable = plans
            .iter()
            .find(|plan| {
                plan.status == PlanStatus::Done && plan_backup_target(plan).as_ref() == Some(target)
            })
            .map(|plan| &plan.id);
        let mut completed = Vec::new();
        for (plan, backup) in backups {
            if !completed.contains(&plan.id) {
                completed.push(plan.id.clone());
            }
            if completed.len() <= BACKUP_RETENTION_COUNT
                || now.signed_duration_since(plan.created_at) < BACKUP_RETENTION_AGE
                || newest_undoable == Some(&plan.id)
            {
                continue;
            }
            journal.remove_backup(guard, &plan.id, &backup.relative)?;
            trimmed.push(TrimmedBackup {
                plan: plan.id.clone(),
                relative: backup.relative.clone(),
            });
        }
    }
    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsops::{self, Root};
    use crate::ports::{LeaseMode, LeaseProvider};
    use crate::testing::{FailingFs, FakeLease, FixtureBuilder};
    use std::time::Duration;

    fn guard(lease: &FakeLease) -> ExclusiveGuard {
        let handle = lease
            .acquire(
                &[crate::ports::LeaseKey {
                    canonical_root: PathBuf::from("/root"),
                }],
                LeaseMode::Exclusive,
                Duration::ZERO,
            )
            .expect("acquire exclusive lease");
        ExclusiveGuard::from_handle(handle)
    }

    fn retention_fixture(
        protection: &str,
    ) -> (FsJournal, ExclusiveGuard, Arc<dyn ScopeFs>, Vec<PlanId>) {
        use crate::backup_retention::{BACKUP_RETENTION_AGE, BACKUP_RETENTION_COUNT};
        let fs: Arc<dyn ScopeFs> = Arc::new(
            FixtureBuilder::new()
                .dir("/journal")
                .dir("/root")
                .build_fs(),
        );
        let journal = journal_over(fs.clone());
        let lease = FakeLease::default();
        let guard = guard(&lease);
        let now = Utc::now();
        let mut ids = Vec::new();
        for index in 0..BACKUP_RETENTION_COUNT + 2 {
            let id = PlanId(format!("retention-{index:04}"));
            let age = if protection == "recent" && index == 0 {
                chrono::Duration::days(1)
            } else {
                BACKUP_RETENTION_AGE + chrono::Duration::days(1)
            };
            let plan = PlanWriter::begin(
                &journal,
                &guard,
                id.clone(),
                now - age + chrono::Duration::seconds(i64::try_from(index).unwrap()),
                "retention",
                PathBuf::from("/root"),
                Vec::new(),
            )
            .unwrap();
            plan.record_write_file(Path::new("/root/alpha/SKILL.md"), Some(b"backup bytes"))
                .unwrap();
            let status = match (protection, index) {
                ("pending", 0) => PlanStatus::Pending,
                ("interrupted", 0) => PlanStatus::Interrupted,
                ("undoable", 0) => PlanStatus::Done,
                _ => PlanStatus::Reversed,
            };
            if status != PlanStatus::Pending {
                plan.finish(status).unwrap();
            }
            ids.push(id);
        }
        (journal, guard, fs, ids)
    }

    #[test]
    fn a_newer_undoable_plan_without_a_backup_expires_the_old_backup_or_pins_it_forever() {
        let (journal, guard, fs, ids) = retention_fixture("undoable");
        let old = journal
            .all()
            .unwrap()
            .into_iter()
            .find(|plan| plan.id == ids[0])
            .unwrap();
        let backup = journal.backup_path(&old.id, &old.backups[0].relative);
        assert!(fs.symlink_metadata(&backup).is_ok());
        fs.fsops_create_dir(Path::new("/root/alpha")).unwrap();
        let root = Root::open(fs.as_ref(), PathBuf::from("/root")).unwrap();
        let plan = PlanWriter::begin(
            &journal,
            &guard,
            PlanId("new-without-backup".into()),
            Utc::now(),
            "new file",
            PathBuf::from("/root"),
            Vec::new(),
        )
        .unwrap();
        fsops::write_file(
            &root,
            &plan,
            Path::new("alpha/SKILL.md"),
            b"new user bytes",
            &fsops::ReadStamp::Absent,
        )
        .unwrap();
        plan.finish(PlanStatus::Done).unwrap();
        trim_backups(&journal, &guard, Utc::now()).unwrap();
        assert!(
            fs.symlink_metadata(&backup).is_err(),
            "a newer undoable plan no longer needs the old backup"
        );
        assert_eq!(
            fs.read_capped(Path::new("/root/alpha/SKILL.md"), u64::MAX)
                .unwrap(),
            b"new user bytes"
        );
    }

    #[test]
    fn completed_plan_retention_keeps_the_count_and_age_union_or_deletes_a_protected_backup() {
        use crate::backup_retention::BACKUP_RETENTION_COUNT;
        for protection in ["count", "recent", "undoable", "pending", "interrupted"] {
            let (journal, guard, fs, ids) = retention_fixture(protection);
            trim_backups(&journal, &guard, Utc::now()).unwrap();
            let records = journal.all().unwrap();
            let protected = if protection == "count" {
                ids.len() - BACKUP_RETENTION_COUNT
            } else {
                0
            };
            let control = usize::from(protected == 0);
            let path = |index: usize| {
                journal.backup_path(&ids[index], &records[index].backups[0].relative)
            };
            assert!(
                fs.symlink_metadata(&path(protected)).is_ok(),
                "{protection} must protect this backup"
            );
            assert!(
                fs.symlink_metadata(&path(control)).is_err(),
                "an old unprotected plan backup must expire"
            );
            assert_eq!(records.len(), ids.len(), "retention must keep plan rows");
        }
    }

    fn crash_retention_plans() -> (Arc<dyn ScopeFs>, ExclusiveGuard) {
        let fs: Arc<dyn ScopeFs> = Arc::new(
            FixtureBuilder::new()
                .dir("/journal")
                .dir("/root")
                .file("/root/SKILL.md", b"user file")
                .build_fs(),
        );
        let journal = journal_over(fs.clone());
        let guard = guard(&FakeLease::default());
        for index in 0..crate::backup_retention::BACKUP_RETENTION_COUNT + 2 {
            let plan = PlanWriter::begin(
                &journal,
                &guard,
                PlanId(format!("crash-{index:04}")),
                Utc::now(),
                "plan",
                PathBuf::from("/root"),
                Vec::new(),
            )
            .unwrap();
            plan.record_write_file(Path::new("/root/alpha/SKILL.md"), Some(b"backup bytes"))
                .unwrap();
            plan.finish(PlanStatus::Done).unwrap();
        }
        (fs, guard)
    }

    #[test]
    fn completed_plan_retention_crash_at_every_durable_step_recovers_or_leaves_an_available_partial_backup(
    ) {
        use crate::testing::CrashFs;
        let now =
            Utc::now() + crate::backup_retention::BACKUP_RETENTION_AGE + chrono::Duration::days(1);
        let (fs, guard) = crash_retention_plans();
        let counted = Arc::new(CrashFs::new(fs, 0));
        trim_backups(&journal_over(counted.clone()), &guard, now).unwrap();
        let steps = counted.calls();
        assert!(steps > 0);
        for cut in 1..=steps {
            let (fs, guard) = crash_retention_plans();
            let before = tree_snapshot(fs.as_ref(), Path::new("/root"));
            let crashing = journal_over(Arc::new(CrashFs::new(fs.clone(), cut)));
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| trim_backups(
                    &crashing, &guard, now
                )))
                .is_err()
            );
            let recovered = journal_over(fs.clone());
            for plan in recovered.all().unwrap() {
                for backup in plan.backups {
                    let result = recovered.read_backup(&plan.id, &backup.relative);
                    if plan.expired_backups.contains(&backup.relative) {
                        assert_eq!(result.unwrap_err().code, ErrorCode::BackupExpired);
                    } else {
                        assert_eq!(
                            result.unwrap(),
                            b"backup bytes",
                            "cut {cut}: available backup is incomplete"
                        );
                    }
                }
            }
            reconcile(&recovered, &guard, fs.as_ref()).unwrap();
            trim_backups(&recovered, &guard, now).unwrap();
            assert_eq!(
                tree_snapshot(fs.as_ref(), Path::new("/root")),
                before,
                "cut {cut}: user files changed"
            );
            let remaining = recovered
                .all()
                .unwrap()
                .iter()
                .map(|plan| plan.backups.len() - plan.expired_backups.len())
                .sum::<usize>();
            assert_eq!(remaining, crate::backup_retention::BACKUP_RETENTION_COUNT);
        }
    }

    #[test]
    fn recovery_refuses_a_row_that_widens_its_root_or_changes_foreign_files() {
        let fs: Arc<dyn ScopeFs> = Arc::new(
            FixtureBuilder::new()
                .dir("/journal")
                .dir("/root")
                .file("/foreign/keep.txt", b"foreign")
                .build_fs(),
        );
        let journal = journal_over(fs.clone());
        let guard = guard(&FakeLease::default());
        let plan = PlanWriter::begin(
            &journal,
            &guard,
            PlanId("foreign-root".into()),
            Utc::now(),
            "corrupt root",
            PathBuf::from("/"),
            Vec::new(),
        )
        .unwrap();
        plan.record_write_file(Path::new("/foreign/keep.txt"), None)
            .unwrap();
        let report = reconcile(&journal, &guard, fs.as_ref()).unwrap();
        assert_eq!(
            report.interrupted.len(),
            1,
            "an unowned root must be refused"
        );
        assert_eq!(
            fs.read_capped(Path::new("/foreign/keep.txt"), u64::MAX)
                .unwrap(),
            b"foreign"
        );
    }

    #[test]
    fn recovery_retries_an_interrupted_plan_or_leaves_the_post_write_bytes() {
        let inner: Arc<dyn ScopeFs> = Arc::new(
            FixtureBuilder::new()
                .dir("/journal")
                .file("/root/SKILL.md", b"before")
                .build_fs(),
        );
        let failing = Arc::new(FailingFs::wrap(inner.clone()));
        let journal = journal_over(failing.clone());
        let guard = guard(&FakeLease::default());
        let root = Root::open(failing.as_ref(), PathBuf::from("/root")).unwrap();
        let plan = PlanWriter::begin(
            &journal,
            &guard,
            PlanId("retry".into()),
            Utc::now(),
            "retry",
            PathBuf::from("/root"),
            Vec::new(),
        )
        .unwrap();
        fsops::write_file(
            &root,
            &plan,
            Path::new("SKILL.md"),
            b"after",
            &fsops::read_stamp(failing.as_ref(), Path::new("/root/SKILL.md")).unwrap(),
        )
        .unwrap();
        failing.fail_nth_fsops_device_inode(2);
        assert_eq!(
            reconcile(&journal, &guard, failing.as_ref())
                .unwrap()
                .interrupted
                .len(),
            1
        );
        assert_eq!(journal.all().unwrap()[0].status, PlanStatus::Interrupted);
        assert_eq!(
            reconcile(&journal, &guard, failing.as_ref())
                .unwrap()
                .reversed
                .len(),
            1
        );
        assert_eq!(
            inner
                .read_capped(Path::new("/root/SKILL.md"), u64::MAX)
                .unwrap(),
            b"before"
        );
    }

    #[test]
    fn recovery_continues_after_one_finish_fails_or_leaves_later_plans_pending() {
        let fs: Arc<dyn ScopeFs> = Arc::new(
            FixtureBuilder::new()
                .dir("/journal")
                .dir("/root")
                .build_fs(),
        );
        let failing = Arc::new(FailingFs::wrap(fs));
        let journal = journal_over(failing.clone());
        let guard = guard(&FakeLease::default());
        for id in ["first", "second"] {
            PlanWriter::begin(
                &journal,
                &guard,
                PlanId(id.into()),
                Utc::now(),
                "finish retry",
                PathBuf::from("/root"),
                Vec::new(),
            )
            .unwrap();
        }
        failing.fail_next_fsops_rename();
        let report = reconcile(&journal, &guard, failing.as_ref())
            .expect("one failed finish must not abort the sweep");
        assert_eq!(report.resolved_without_steps, vec![PlanId("first".into())]);
        assert_eq!(report.interrupted.len(), 1);
        assert_eq!(
            reconcile(&journal, &guard, failing.as_ref())
                .unwrap()
                .resolved_without_steps,
            vec![PlanId("second".into())]
        );
    }

    fn primitive_fixture(kind: &str) -> Arc<dyn ScopeFs> {
        let mut fixture = FixtureBuilder::new()
            .dir("/journal")
            .dir("/root")
            .file("/root/keep.txt", b"untouched")
            .file("/root/old-target", b"old link bytes")
            .file("/root/new-target", b"new link bytes");
        if kind == "swap_exchange" {
            fixture = fixture.file("/root/skill/old.txt", b"old tree");
        }
        if kind == "link_replace" || kind == "write_symlink" {
            fixture = fixture.alias("/root/link", "/root/old-target");
        }
        if kind == "write_replace" {
            fixture = fixture.file("/root/a", b"old a").file("/root/b", b"old b");
        }
        Arc::new(fixture.build_fs())
    }

    fn run_primitive_plan(kind: &str, fs: &Arc<dyn ScopeFs>, guard: &ExclusiveGuard) {
        let journal = journal_over(fs.clone());
        let root = Root::open(fs.as_ref(), PathBuf::from("/root")).unwrap();
        let plan = PlanWriter::begin(
            &journal,
            guard,
            PlanId("matrix".into()),
            Utc::now(),
            kind,
            PathBuf::from("/root"),
            Vec::new(),
        )
        .unwrap();
        match kind {
            "swap_fresh" | "swap_exchange" => {
                let staged = fsops::stage(
                    &root,
                    &plan,
                    &[
                        (PathBuf::from("SKILL.md"), b"new tree".to_vec()),
                        (PathBuf::from("nested/new.txt"), b"new child".to_vec()),
                    ],
                )
                .unwrap();
                fsops::swap(
                    &root,
                    &plan,
                    Path::new("skill"),
                    &staged,
                    Path::new(".trash"),
                )
                .unwrap();
            }
            "link_new" | "link_replace" => fsops::link(
                &root,
                &plan,
                Path::new("link"),
                Path::new("/root/new-target"),
            )
            .unwrap(),
            "write_new" | "write_replace" => {
                for name in ["a", "b"] {
                    let stamp =
                        fsops::read_stamp(fs.as_ref(), &Path::new("/root").join(name)).unwrap();
                    fsops::write_file(
                        &root,
                        &plan,
                        Path::new(name),
                        format!("new {name}").as_bytes(),
                        &stamp,
                    )
                    .unwrap();
                }
            }
            "write_symlink" => {
                let stamp = fsops::read_stamp(fs.as_ref(), Path::new("/root/link")).unwrap();
                fsops::write_file(&root, &plan, Path::new("link"), b"replacement file", &stamp)
                    .unwrap();
            }
            _ => panic!("unknown primitive fixture"),
        }
    }

    fn complete_snapshot(
        fs: &dyn ScopeFs,
        root: &Path,
    ) -> std::collections::BTreeMap<PathBuf, (FileKind, Vec<u8>)> {
        fn visit(
            fs: &dyn ScopeFs,
            path: &Path,
            out: &mut std::collections::BTreeMap<PathBuf, (FileKind, Vec<u8>)>,
        ) {
            let kind = fs.symlink_metadata(path).unwrap().kind;
            let content = match kind {
                FileKind::File => fs.read_capped(path, u64::MAX).unwrap(),
                FileKind::Symlink => fs
                    .read_link(path)
                    .unwrap()
                    .to_string_lossy()
                    .as_bytes()
                    .to_vec(),
                FileKind::Dir | FileKind::Other => Vec::new(),
            };
            out.insert(path.to_path_buf(), (kind, content));
            if kind == FileKind::Dir {
                for entry in fs.read_dir(path).unwrap() {
                    visit(fs, &path.join(entry.name), out);
                }
            }
        }
        let mut out = std::collections::BTreeMap::new();
        visit(fs, root, &mut out);
        out
    }

    #[test]
    fn primitive_crash_at_every_durable_step_recovers_the_whole_tree_or_leaves_mixed_files_and_temps(
    ) {
        use crate::testing::CrashFs;
        for kind in [
            "swap_fresh",
            "swap_exchange",
            "link_new",
            "link_replace",
            "write_new",
            "write_replace",
            "write_symlink",
        ] {
            let guard = guard(&FakeLease::default());
            let counted = Arc::new(CrashFs::new(primitive_fixture(kind), 0));
            let counted_port: Arc<dyn ScopeFs> = counted.clone();
            run_primitive_plan(kind, &counted_port, &guard);
            for cut in 1..=counted.calls() {
                let fs = primitive_fixture(kind);
                let before = complete_snapshot(fs.as_ref(), Path::new("/root"));
                let crashing: Arc<dyn ScopeFs> = Arc::new(CrashFs::new(fs.clone(), cut));
                assert!(
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_primitive_plan(
                        kind, &crashing, &guard
                    )))
                    .is_err(),
                    "{kind} cut {cut}"
                );
                let journal = journal_over(fs.clone());
                for _ in 0..2 {
                    assert!(
                        reconcile(&journal, &guard, fs.as_ref())
                            .unwrap()
                            .interrupted
                            .is_empty(),
                        "{kind} cut {cut}"
                    );
                }
                assert_eq!(
                    complete_snapshot(fs.as_ref(), Path::new("/root")),
                    before,
                    "{kind} cut {cut}: recovery left a mixed tree or owned temp"
                );
            }
        }
    }

    #[test]
    fn recovery_crash_at_every_durable_step_can_run_twice_or_loses_old_links_and_quarantined_bytes()
    {
        use crate::testing::CrashFs;
        for kind in [
            "swap_fresh",
            "swap_exchange",
            "link_new",
            "link_replace",
            "write_new",
            "write_replace",
            "write_symlink",
        ] {
            let guard = guard(&FakeLease::default());
            let fs = primitive_fixture(kind);
            run_primitive_plan(kind, &fs, &guard);
            let counted = Arc::new(CrashFs::new(fs, 0));
            reconcile(&journal_over(counted.clone()), &guard, counted.as_ref()).unwrap();
            for cut in 1..=counted.calls() {
                let fs = primitive_fixture(kind);
                let before = complete_snapshot(fs.as_ref(), Path::new("/root"));
                run_primitive_plan(kind, &fs, &guard);
                let crashing = Arc::new(CrashFs::new(fs.clone(), cut));
                assert!(
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| reconcile(
                        &journal_over(crashing.clone()),
                        &guard,
                        crashing.as_ref()
                    )))
                    .is_err(),
                    "{kind} recovery cut {cut}"
                );
                for _ in 0..2 {
                    assert!(
                        reconcile(&journal_over(fs.clone()), &guard, fs.as_ref())
                            .unwrap()
                            .interrupted
                            .is_empty(),
                        "{kind} recovery cut {cut}"
                    );
                }
                assert_eq!(
                    complete_snapshot(fs.as_ref(), Path::new("/root")),
                    before,
                    "{kind} recovery cut {cut}"
                );
            }
        }
    }

    #[test]
    fn finishing_crash_keeps_the_whole_before_or_after_tree_or_marks_a_mixed_tree_done() {
        use crate::testing::CrashFs;
        for kind in [
            "swap_fresh",
            "swap_exchange",
            "link_new",
            "link_replace",
            "write_new",
            "write_replace",
            "write_symlink",
        ] {
            let guard = guard(&FakeLease::default());
            let fs = primitive_fixture(kind);
            run_primitive_plan(kind, &fs, &guard);
            let counted = Arc::new(CrashFs::new(fs, 0));
            journal_over(counted.clone())
                .finish(&guard, &PlanId("matrix".into()), PlanStatus::Done)
                .unwrap();
            for cut in 1..=counted.calls() {
                let fs = primitive_fixture(kind);
                let before = complete_snapshot(fs.as_ref(), Path::new("/root"));
                run_primitive_plan(kind, &fs, &guard);
                let after = complete_snapshot(fs.as_ref(), Path::new("/root"));
                let crashing = journal_over(Arc::new(CrashFs::new(fs.clone(), cut)));
                assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                    || crashing.finish(&guard, &PlanId("matrix".into()), PlanStatus::Done)
                ))
                .is_err());
                reconcile(&journal_over(fs.clone()), &guard, fs.as_ref()).unwrap();
                let actual = complete_snapshot(fs.as_ref(), Path::new("/root"));
                assert!(
                    actual == before || actual == after,
                    "{kind} finish cut {cut}: mixed tree"
                );
            }
        }
    }

    #[test]
    fn journal_backup_paths_refuse_traversal_or_replace_a_user_file_outside_the_plan() {
        let fs: Arc<dyn ScopeFs> = Arc::new(
            FixtureBuilder::new()
                .dir("/journal")
                .file("/root/keep.txt", b"user file")
                .build_fs(),
        );
        let journal = journal_over(fs.clone());
        let guard = guard(&FakeLease::default());
        let id = PlanId("valid".into());
        PlanWriter::begin(
            &journal,
            &guard,
            id.clone(),
            Utc::now(),
            "backup confinement",
            PathBuf::from("/root"),
            Vec::new(),
        )
        .unwrap();
        for (id, relative) in [
            (id, "../../../../root/keep.txt"),
            (PlanId("../../../root".into()), "keep.txt"),
        ] {
            assert!(journal
                .write_backup(&guard, &id, relative, b"bad replacement")
                .is_err());
            assert_eq!(
                fs.read_capped(Path::new("/root/keep.txt"), u64::MAX)
                    .unwrap(),
                b"user file"
            );
        }
    }

    #[test]
    fn every_plan_transition_follows_recovery_rules_or_reopens_a_completed_plan() {
        use PlanStatus::*;
        let legal = [
            (Pending, Done),
            (Pending, Failed),
            (Pending, Reversed),
            (Pending, Interrupted),
            (Interrupted, Interrupted),
            (Interrupted, Failed),
            (Interrupted, Reversed),
        ];
        for from in [Pending, Done, Failed, Reversed, Interrupted] {
            for to in [Pending, Done, Failed, Reversed, Interrupted] {
                let result = from.settle(to);
                assert_eq!(
                    result.is_ok(),
                    legal.contains(&(from, to)),
                    "{from:?} -> {to:?}"
                );
                if let Ok(actual) = result {
                    assert_eq!(actual, to);
                }
            }
        }
    }

    #[test]
    fn retrying_an_already_restored_file_flushes_its_parent_or_settles_before_the_restore_is_durable(
    ) {
        let inner: Arc<dyn ScopeFs> = Arc::new(
            FixtureBuilder::new()
                .dir("/journal")
                .file("/root/value", b"before")
                .build_fs(),
        );
        let failing = Arc::new(FailingFs::wrap(inner.clone()));
        let journal = journal_over(failing.clone());
        let guard = guard(&FakeLease::default());
        let root = Root::open(failing.as_ref(), PathBuf::from("/root")).unwrap();
        let plan = PlanWriter::begin(
            &journal,
            &guard,
            PlanId("flush-retry".into()),
            Utc::now(),
            "flush retry",
            PathBuf::from("/root"),
            Vec::new(),
        )
        .unwrap();
        let path = Path::new("/root/value");
        fsops::write_file(
            &root,
            &plan,
            Path::new("value"),
            b"after",
            &fsops::read_stamp(failing.as_ref(), path).unwrap(),
        )
        .unwrap();
        inner
            .fsops_write_new_file(Path::new("/root/restore-temp"), b"before")
            .unwrap();
        inner
            .fsops_rename(Path::new("/root/restore-temp"), path)
            .unwrap();
        failing.fail_next_fsops_fsync_dir();
        assert_eq!(
            reconcile(&journal, &guard, failing.as_ref())
                .unwrap()
                .interrupted
                .len(),
            1
        );
        assert_eq!(
            journal.all().unwrap()[0].status,
            PlanStatus::Interrupted,
            "the user folder must be durable before the row settles"
        );
        assert_eq!(
            reconcile(&journal, &guard, failing.as_ref())
                .unwrap()
                .reversed
                .len(),
            1
        );
    }

    fn journal_over(fs: Arc<dyn ScopeFs>) -> FsJournal {
        FsJournal::new(PathBuf::from("/journal"), fs)
    }

    /// Given a plan begun through `Journal::begin`, when nothing has run
    /// yet, then the manifest and the plan file both already exist on disk;
    /// on failure the panic names the first step that ran instead (there is
    /// none - `begin` alone must already have written both).
    #[test]
    fn journal_writes_the_manifest_and_plan_before_the_first_step_or_names_the_step_that_ran_first()
    {
        let fs: Arc<dyn ScopeFs> = Arc::new(FixtureBuilder::new().dir("/journal").build_fs());
        let journal = journal_over(fs.clone());
        let lease = FakeLease::default();
        let g = guard(&lease);

        let id = PlanId("01PLAN0000000000000000000".into());
        let plan = PlanWriter::begin(
            &journal,
            &g,
            id.clone(),
            Utc::now(),
            "test plan",
            PathBuf::from("/root"),
            vec![PlanBackupEntry {
                original: PathBuf::from("/root/skill/SKILL.md"),
                relative: "skill/SKILL.md".into(),
                bytes: 3,
            }],
        )
        .expect("begin must write the manifest and plan before returning");

        assert!(
            fs.symlink_metadata(Path::new(
                "/journal/plans/01PLAN0000000000000000000/manifest.json"
            ))
            .is_ok(),
            "begin returned Ok, so the manifest must already be on disk - no step ran first"
        );
        assert!(
            fs.symlink_metadata(Path::new(
                "/journal/plans/01PLAN0000000000000000000/plan.json"
            ))
            .is_ok(),
            "begin returned Ok, so the plan file must already be on disk - no step ran first"
        );

        let record = journal
            .all()
            .expect("read plans back")
            .into_iter()
            .find(|p| p.id == id)
            .expect("the plan begun above");
        assert!(
            record.steps.is_empty(),
            "no step has run yet; the plan must record none"
        );

        plan.finish(PlanStatus::Done).expect("finish");
    }

    /// (R9) Given a plan dir carrying only `manifest.json` - the shape a
    /// crash between `begin`'s manifest write and its `plan.json` rename
    /// leaves behind - when `all()` reads the journal back, then that dir is
    /// skipped rather than failing the whole read; a normal, complete plan
    /// alongside it is still returned. On failure the panic names the
    /// error `all()` raised instead of skipping.
    #[test]
    fn all_skips_a_plan_dir_missing_plan_json_or_names_the_read_error() {
        let fs: Arc<dyn ScopeFs> = Arc::new(FixtureBuilder::new().dir("/journal").build_fs());
        let journal = journal_over(fs.clone());
        let lease = FakeLease::default();
        let g = guard(&lease);

        let ok_id = PlanId("01PLAN0000000000000000002".into());
        PlanWriter::begin(
            &journal,
            &g,
            ok_id.clone(),
            Utc::now(),
            "complete plan",
            PathBuf::from("/root"),
            Vec::new(),
        )
        .expect("begin the complete plan");

        // The crashed plan: only its manifest landed, never `plan.json`.
        fs.fsops_create_dir(Path::new("/journal/plans/01PLAN0000000000000000000"))
            .expect("mkdir the crashed plan's dir");
        fs.fsops_write_new_file(
            Path::new("/journal/plans/01PLAN0000000000000000000/manifest.json"),
            b"[]",
        )
        .expect("write the crashed plan's manifest only");

        let records = journal
            .all()
            .expect("all() must skip the manifest-only dir, not fail outright");
        assert_eq!(
            records.iter().map(|p| p.id.clone()).collect::<Vec<_>>(),
            vec![ok_id],
            "only the complete plan should be returned"
        );
    }

    /// Given `ensure_dir`'s own creation call hits a real I/O error - not
    /// the benign "the directory already exists" race the `AlreadyExists`
    /// arm exists for - when a plan begins, then that error must fail the
    /// plan rather than be swallowed as if the directory were already
    /// there; on failure the panic names the plan that began anyway.
    #[test]
    fn ensure_dir_propagates_a_real_creation_error_or_swallows_it_as_already_exists() {
        let fixture = FixtureBuilder::new().dir("/journal").build_fs();
        let failing = FailingFs::wrap(Arc::new(fixture));
        // Fails the very first `fsops_create_dir` call `begin` makes -
        // creating `/journal/plans`, which does not exist yet - with a
        // generic error, not `AlreadyExists`.
        failing.fail_next_create_dir();
        let journal = journal_over(Arc::new(failing));
        let lease = FakeLease::default();
        let g = guard(&lease);

        let id = PlanId("01PLANENSUREDIRFAIL000001".into());
        let result = PlanWriter::begin(
            &journal,
            &g,
            id,
            Utc::now(),
            "a real creation error",
            PathBuf::from("/root"),
            Vec::new(),
        );
        assert!(
            result.is_err(),
            "an injected fsops_create_dir failure must fail begin, or this test proves nothing about swallowing it as AlreadyExists",
        );
    }

    /// Given `ensure_dir` is called on a single missing directory (not one
    /// requiring recursion into a missing parent, unlike the `begin`-driven
    /// test above), when its own `fsops_create_dir` call hits a real I/O
    /// error, then that error must be returned rather than swallowed as
    /// `AlreadyExists`; on failure the panic names the outcome `ensure_dir`
    /// gave instead.
    #[test]
    fn ensure_dir_on_a_single_missing_directory_propagates_a_real_creation_error() {
        let fixture = FixtureBuilder::new().dir("/journal").build_fs();
        let failing = FailingFs::wrap(Arc::new(fixture));
        failing.fail_next_create_dir();
        let journal = journal_over(Arc::new(failing));

        let result = journal.ensure_dir(Path::new("/journal/plans"));

        assert!(
            result.is_err(),
            "an injected fsops_create_dir failure must propagate as an error, not be treated \
             as the directory already existing: got {result:?}"
        );
    }

    /// Given a directory that already exists on disk, when `ensure_dir`'s
    /// own existence check races with a concurrent creator and reports it
    /// missing anyway - so `ensure_dir` calls `fsops_create_dir` on a
    /// directory that is really already there - then the resulting
    /// `AlreadyExists` error must be swallowed as a no-op, not surfaced as a
    /// failure; on failure the panic names the outcome `ensure_dir` gave
    /// instead.
    #[test]
    fn ensure_dir_swallows_a_genuine_already_exists_race_as_a_no_op() {
        let fixture = FixtureBuilder::new()
            .dir("/journal")
            .dir("/journal/plans")
            .build_fs();
        let failing = FailingFs::wrap(Arc::new(fixture));
        // Makes the existence check on `/journal/plans` report "missing"
        // even though it is really there, so `ensure_dir` proceeds to
        // `fsops_create_dir`, which then hits a genuine `AlreadyExists`.
        failing.fail_symlink_metadata_for(PathBuf::from("/journal/plans"));
        let journal = journal_over(Arc::new(failing));

        let result = journal.ensure_dir(Path::new("/journal/plans"));

        assert!(
            result.is_ok(),
            "a real AlreadyExists from fsops_create_dir must be swallowed as a no-op, not \
             surfaced as a failure: got {result:?}"
        );
    }

    /// Given `all()`'s own `read_dir` call hits a real I/O error - not the
    /// benign "no plans directory has been created yet" case the `NotFound`
    /// arm exists for - when `all()` runs, then that error must propagate
    /// rather than be read as "no plans exist"; on failure the panic names
    /// the empty list `all()` returned instead of the error.
    #[test]
    fn all_propagates_a_real_read_dir_error_or_reads_it_as_no_plans() {
        let fixture = FixtureBuilder::new().dir("/journal").build_fs();
        let failing = FailingFs::wrap(Arc::new(fixture));
        failing.fail_read_dir_for(PathBuf::from("/journal/plans"));
        let journal = journal_over(Arc::new(failing));

        journal.all().expect_err(
            "an injected read_dir failure must propagate, or this test proves nothing about reading it as an empty plans directory",
        );
    }

    /// Given a backup [`Journal::write_backup`] already wrote to disk, when
    /// [`Journal::remove_backup`] runs, then the backup's bytes are gone
    /// from disk; on failure the panic names the backup path still present.
    #[test]
    fn remove_backup_deletes_the_backup_bytes_from_disk_or_leaves_them_in_place() {
        let fs: Arc<dyn ScopeFs> = Arc::new(FixtureBuilder::new().dir("/journal").build_fs());
        let journal = journal_over(fs.clone());
        let lease = FakeLease::default();
        let g = guard(&lease);

        let id = PlanId("01PLANREMOVEBACKUP0000001".into());
        let plan = PlanWriter::begin(
            &journal,
            &g,
            id.clone(),
            Utc::now(),
            "completed backup with metadata",
            PathBuf::from("/root"),
            vec![PlanBackupEntry {
                original: PathBuf::from("/root/SKILL.md"),
                relative: "skill/SKILL.md".into(),
                bytes: 12,
            }],
        )
        .unwrap();
        plan.finish(PlanStatus::Done).unwrap();
        journal
            .write_backup(&g, &id, "skill/SKILL.md", b"backup bytes")
            .expect("write the backup");
        let backup_path =
            Path::new("/journal/plans/01PLANREMOVEBACKUP0000001/backups/skill/SKILL.md");
        assert!(
            fs.symlink_metadata(backup_path).is_ok(),
            "the backup must exist on disk before removal, or this test proves nothing"
        );

        journal
            .remove_backup(&g, &id, "skill/SKILL.md")
            .expect("remove the existing backup");

        assert!(
            fs.symlink_metadata(backup_path).is_err(),
            "remove_backup must delete the backup's bytes from disk, not just return Ok"
        );
    }

    /// Given a backup that was never written, when [`Journal::remove_backup`]
    /// runs against it anyway - the shape [`trim_backups`] hits for a
    /// backup already trimmed by an earlier pass - then removal is a no-op,
    /// not an error; on failure the panic names the spurious error raised
    /// for a backup that was already gone.
    #[test]
    fn remove_backup_on_an_already_removed_backup_is_a_no_op_or_names_the_spurious_error() {
        let fs: Arc<dyn ScopeFs> = Arc::new(FixtureBuilder::new().dir("/journal").build_fs());
        let journal = journal_over(fs);
        let lease = FakeLease::default();
        let g = guard(&lease);

        let id = PlanId("01PLANREMOVEBACKUPTWICE01".into());
        let plan = PlanWriter::begin(
            &journal,
            &g,
            id.clone(),
            Utc::now(),
            "completed backup with metadata",
            PathBuf::from("/root"),
            vec![PlanBackupEntry {
                original: PathBuf::from("/root/SKILL.md"),
                relative: "never-written.bin".into(),
                bytes: 12,
            }],
        )
        .unwrap();
        plan.finish(PlanStatus::Done).unwrap();
        journal
            .write_backup(&g, &id, "never-written.bin", b"old")
            .unwrap();
        journal.remove_backup(&g, &id, "never-written.bin").unwrap();
        journal
            .remove_backup(&g, &id, "never-written.bin")
            .expect("removing a backup that was never written must be a no-op, not an error");
    }

    /// Given two `write_file` steps against files that share a leaf name,
    /// when each is recorded, then their backups get distinct relative
    /// names that each carry the file's own leaf - not a blank name that
    /// would make every backup collide on the same path; on failure the
    /// panic names the two (indistinguishable) relative names recorded.
    #[test]
    fn record_write_file_names_each_backup_after_its_own_leaf_or_leaves_them_blank_and_colliding() {
        let fs: Arc<dyn ScopeFs> = Arc::new(FixtureBuilder::new().dir("/journal").build_fs());
        let journal = journal_over(fs);
        let lease = FakeLease::default();
        let g = guard(&lease);

        let id = PlanId("01PLANBACKUPNAME000000001".into());
        let plan = PlanWriter::begin(
            &journal,
            &g,
            id.clone(),
            Utc::now(),
            "two backups with the same leaf name",
            PathBuf::from("/root"),
            Vec::new(),
        )
        .expect("begin");

        plan.record_write_file(Path::new("/root/a/SKILL.md"), Some(b"a"))
            .expect("record the first backup");
        plan.record_write_file(Path::new("/root/b/SKILL.md"), Some(b"b"))
            .expect("record the second backup");

        let record = journal
            .all()
            .expect("read the plan back")
            .into_iter()
            .find(|p| p.id == id)
            .expect("the plan begun above");
        let names: Vec<&str> = record.backups.iter().map(|b| b.relative.as_str()).collect();
        assert_eq!(
            names.len(),
            2,
            "both write_file steps must have recorded a backup entry"
        );
        assert_ne!(
            names[0], names[1],
            "two backups must get distinct relative names, not both blank"
        );
        for name in &names {
            assert!(
                name.ends_with("SKILL.md"),
                "the relative name must carry the file's own leaf name, was {name:?}"
            );
        }

        plan.finish(PlanStatus::Done).expect("finish");
    }

    /// Given a plan, when it is begun and then finished, then its status
    /// reads `Pending` after `begin` and `Done` only after `finish`; on
    /// failure the panic names whichever transition did not happen.
    #[test]
    fn journal_marks_a_row_pending_before_the_mutation_and_done_after_the_last_step_or_names_the_missing_transition(
    ) {
        let fs: Arc<dyn ScopeFs> = Arc::new(FixtureBuilder::new().dir("/journal").build_fs());
        let journal = journal_over(fs);
        let lease = FakeLease::default();
        let g = guard(&lease);

        let id = PlanId("01PLAN0000000000000000001".into());
        let plan = PlanWriter::begin(
            &journal,
            &g,
            id.clone(),
            Utc::now(),
            "test plan",
            PathBuf::from("/root"),
            Vec::new(),
        )
        .expect("begin");

        let after_begin = journal
            .all()
            .expect("read")
            .into_iter()
            .find(|p| p.id == id)
            .expect("plan exists after begin");
        assert_eq!(
            after_begin.status,
            PlanStatus::Pending,
            "must be Pending before the mutation starts, not {:?}",
            after_begin.status
        );

        plan.record_write_file(Path::new("/root/skill/SKILL.md"), None)
            .expect("record the last step");
        journal
            .finish(&g, &id, PlanStatus::Done)
            .expect("finish must mark the row Done");

        let after_finish = journal
            .all()
            .expect("read")
            .into_iter()
            .find(|p| p.id == id)
            .expect("plan still exists after finish");
        assert_eq!(
            after_finish.status,
            PlanStatus::Done,
            "must be Done after the last step, not {:?}",
            after_finish.status
        );
    }

    /// Given a fixture with an existing `skill` folder, when a plan runs two
    /// of its three intended steps (`stage`+`swap` replacing `skill`, then
    /// `link` pointing `skill-current` at it) and the process dies before
    /// its third step and before `finish`, then startup reconciliation
    /// undoes both recorded steps in reverse and the tree under `/root`
    /// reads back byte-for-byte identical to the snapshot taken before the
    /// plan ran; on failure the panic names the diverging path or the plan
    /// left open.
    #[test]
    fn startup_reconciliation_after_a_simulated_crash_completes_or_reverses_every_pending_plan_or_names_the_plan_left_open(
    ) {
        let fs: Arc<dyn ScopeFs> = Arc::new(
            FixtureBuilder::new()
                .dir("/journal")
                .dir("/root")
                .dir("/root/skill")
                .file("/root/skill/SKILL.md", b"pre-plan content")
                .build_fs(),
        );
        let root_path = PathBuf::from("/root");
        let before = tree_snapshot(fs.as_ref(), &root_path);

        let journal = journal_over(fs.clone());
        let lease = FakeLease::default();
        let g = guard(&lease);
        let root = Root::open(fs.as_ref(), root_path.clone()).expect("open root");

        let id = PlanId("01PLAN0000000000000000002".into());
        let plan = PlanWriter::begin(
            &journal,
            &g,
            id.clone(),
            Utc::now(),
            "crashes after two of three steps",
            root_path.clone(),
            Vec::new(),
        )
        .expect("begin");

        let staged = fsops::stage(
            &root,
            &plan,
            &[(PathBuf::from("SKILL.md"), b"new content".to_vec())],
        )
        .expect("stage");
        fsops::swap(
            &root,
            &plan,
            Path::new("skill"),
            &staged,
            Path::new(".trash"),
        )
        .expect("swap over the existing skill folder");
        fsops::link(&root, &plan, Path::new("skill-current"), Path::new("skill")).expect("link");
        // The plan's third step (e.g. a write_file into the new folder)
        // never runs: the process dies here, before `finish`, and `plan`
        // is simply dropped rather than resolved.
        drop(plan);

        let after_crash = tree_snapshot(fs.as_ref(), &root_path);
        assert_ne!(
            after_crash, before,
            "the swap and link above must actually have changed the tree, or this test proves nothing"
        );

        let report = reconcile(&journal, &g, fs.as_ref()).expect("reconciliation must run");
        assert!(
            report.reversed.contains(&id),
            "a plan whose recorded steps could all be undone must resolve as Reversed, not {report:?}"
        );

        let after_reconcile = tree_snapshot(fs.as_ref(), &root_path);
        assert_eq!(
            after_reconcile, before,
            "reconciliation must put the tree back exactly as the pre-plan snapshot"
        );

        let after = journal
            .all()
            .expect("read")
            .into_iter()
            .find(|p| p.id == id)
            .unwrap_or_else(|| panic!("plan {} must still exist after reconciliation", id.0));
        assert_eq!(
            after.status,
            PlanStatus::Reversed,
            "plan {} was left open (still Pending) or not resolved as Reversed, was {:?}",
            id.0,
            after.status
        );
    }

    /// Given two plans left `Pending` by a crash - one with a `link` step
    /// reconciliation can cleanly undo, and one whose `write_file` step's
    /// backup bytes were deleted from disk before reconciliation runs (so
    /// undoing it cannot proceed) - when reconciliation runs, then both rows
    /// still exist afterward, the first is `Reversed`, and the second is
    /// `Interrupted` and named in the report; on failure the panic names
    /// whichever row went missing or was not listed.
    #[test]
    fn startup_reconciliation_never_deletes_a_row_and_lists_every_interrupted_plan_or_names_the_missing_row(
    ) {
        let fs: Arc<dyn ScopeFs> = Arc::new(
            FixtureBuilder::new()
                .dir("/journal")
                .dir("/root")
                .file("/root/file.txt", b"original")
                .build_fs(),
        );
        let root_path = PathBuf::from("/root");
        let journal = journal_over(fs.clone());
        let lease = FakeLease::default();
        let g = guard(&lease);
        let root = Root::open(fs.as_ref(), root_path.clone()).expect("open root");

        let with_step = PlanId("01PLAN0000000000000000003".into());
        let plan = PlanWriter::begin(
            &journal,
            &g,
            with_step.clone(),
            Utc::now(),
            "with a cleanly reversible step",
            root_path.clone(),
            Vec::new(),
        )
        .expect("begin");
        fsops::link(&root, &plan, Path::new("skill-link"), Path::new("file.txt")).expect("link");
        drop(plan);

        let unresolvable = PlanId("01PLAN0000000000000000004".into());
        let plan = PlanWriter::begin(
            &journal,
            &g,
            unresolvable.clone(),
            Utc::now(),
            "its backup is deleted before reconciliation",
            root_path.clone(),
            Vec::new(),
        )
        .expect("begin");
        let target = root_path.join("file.txt");
        let stamp = fsops::read_stamp(fs.as_ref(), &target).expect("read the stamp");
        fsops::write_file(&root, &plan, Path::new("file.txt"), b"changed", &stamp)
            .expect("write_file");
        drop(plan);

        let record = journal
            .all()
            .expect("read")
            .into_iter()
            .find(|p| p.id == unresolvable)
            .expect("the plan begun above");
        let backup_relative = record
            .backups
            .first()
            .expect("write_file over an existing file must have recorded a backup")
            .relative
            .clone();
        fs.fsops_remove_file(&PathBuf::from(format!(
            "/journal/plans/{}/backups/{backup_relative}",
            unresolvable.0
        )))
        .expect("delete the backup bytes so reversal cannot proceed");

        let report = reconcile(&journal, &g, fs.as_ref()).expect("reconciliation must run");
        assert!(
            report.reversed.contains(&with_step),
            "the plan with a cleanly reversible step must be listed as reversed"
        );
        assert!(
            report.interrupted.iter().any(|p| p.id == unresolvable),
            "the plan whose backup was deleted must be listed as interrupted, naming the plan: {report:?}"
        );

        let after = journal.all().expect("read every row back");
        for id in [&with_step, &unresolvable] {
            assert!(
                after.iter().any(|p| &p.id == id),
                "row {} went missing after reconciliation",
                id.0
            );
        }
        let after_unresolvable = after
            .iter()
            .find(|p| p.id == unresolvable)
            .expect("row still exists");
        assert_eq!(
            after_unresolvable.status,
            PlanStatus::Interrupted,
            "plan {} must be Interrupted since its backup could not be read back, was {:?}",
            unresolvable.0,
            after_unresolvable.status
        );
    }

    /// Given a root with an existing `skill` folder, when `swap`'s exchange
    /// lands but the process dies before the follow-up move of the old
    /// folder into quarantine, then startup reconciliation still restores
    /// `skill` to exactly its pre-plan bytes - using the quarantine path
    /// and the staged folder's identity `swap` recorded *before* the
    /// exchange ran, not the (never-written) quarantine folder itself; on
    /// failure the panic names the diverging path, which would be the old
    /// folder deleted rather than restored.
    #[test]
    fn a_crash_between_the_swap_exchange_and_its_journal_row_reverses_to_the_old_folder_or_names_the_folder_it_deleted(
    ) {
        let fixture = FixtureBuilder::new()
            .dir("/journal")
            .dir("/root")
            .dir("/root/skill")
            .file("/root/skill/SKILL.md", b"old content")
            .build_fs();
        let root_path = PathBuf::from("/root");
        let before = tree_snapshot(&fixture, &root_path);

        let failing = FailingFs::wrap(Arc::new(fixture.clone()));
        let root = Root::open(&failing, root_path.clone()).expect("open root");
        let journal = journal_over(Arc::new(fixture.clone()));
        let lease = FakeLease::default();
        let g = guard(&lease);

        let id = PlanId("01PLANSWAPCRASH00000000001".into());
        let plan = PlanWriter::begin(
            &journal,
            &g,
            id.clone(),
            Utc::now(),
            "crash between exchange and quarantine move",
            root_path.clone(),
            Vec::new(),
        )
        .expect("begin");

        let staged = fsops::stage(
            &root,
            &plan,
            &[(PathBuf::from("SKILL.md"), b"new content".to_vec())],
        )
        .expect("stage");
        // The exchange itself (a `fsops_exchange` call) is unaffected; only
        // the follow-up `fsops_rename` that moves the old folder into
        // quarantine fails, landing the crash exactly between the two.
        failing.fail_next_fsops_rename();
        fsops::swap(
            &root,
            &plan,
            Path::new("skill"),
            &staged,
            Path::new(".trash"),
        )
        .expect_err("the quarantine move must fail, or this test proves nothing about the window between it and the exchange");
        // The process dies here: `plan` is dropped, never finished.
        drop(plan);

        let report = reconcile(&journal, &g, &fixture).expect("reconciliation must run");
        assert!(
            report.reversed.contains(&id),
            "a swap step whose exchange landed must still be fully reversible, not {report:?}"
        );

        let after = tree_snapshot(&fixture, &root_path);
        assert_eq!(
            after, before,
            "reconciliation must restore the old folder byte-for-byte, or name the folder it deleted"
        );
    }

    /// Given a swap plan whose exchange landed but the process died before
    /// the follow-up move into quarantine (the same window as the test
    /// above), when reconciliation's probe of the swapped path's identity
    /// then hits a non-`NotFound` I/O error - EACCES, EIO, or similar,
    /// rather than the path simply being absent - it must not read that as
    /// "the exchange never landed": doing so would let the paired `Stage`
    /// step's reversal delete the folder that is, in fact, still live at
    /// `path`. The plan resolves `Interrupted` and the error is named in
    /// the report; on failure the panic names the folder it deleted anyway.
    #[test]
    fn an_io_error_while_probing_a_swapped_path_interrupts_the_plan_or_names_the_old_folder_it_deleted(
    ) {
        let fixture = FixtureBuilder::new()
            .dir("/journal")
            .dir("/root")
            .dir("/root/skill")
            .file("/root/skill/SKILL.md", b"old content")
            .build_fs();
        let root_path = PathBuf::from("/root");

        let failing = FailingFs::wrap(Arc::new(fixture.clone()));
        let root = Root::open(&failing, root_path.clone()).expect("open root");
        let journal = journal_over(Arc::new(fixture.clone()));
        let lease = FakeLease::default();
        let g = guard(&lease);

        let id = PlanId("01PLANIOPROBE0000000000001".into());
        let plan = PlanWriter::begin(
            &journal,
            &g,
            id.clone(),
            Utc::now(),
            "crash between exchange and quarantine move, then a probe I/O error",
            root_path.clone(),
            Vec::new(),
        )
        .expect("begin");

        let staged = fsops::stage(
            &root,
            &plan,
            &[(PathBuf::from("SKILL.md"), b"new content".to_vec())],
        )
        .expect("stage");
        failing.fail_next_fsops_rename();
        fsops::swap(
            &root,
            &plan,
            Path::new("skill"),
            &staged,
            Path::new(".trash"),
        )
        .expect_err("the quarantine move must fail, or this test proves nothing about the window between it and the exchange");
        // The process dies here: `plan` is dropped, never finished.
        drop(plan);

        let before_reconcile = tree_snapshot(&fixture, &root_path);
        let staged_path = staged.path().to_path_buf();
        assert!(
            failing.symlink_metadata(&staged_path).is_ok(),
            "the staged folder must still be on disk before reconciliation runs"
        );

        // `reverse_steps` opens `Root` (one `fsops_device_inode` call) before
        // it ever reaches the `Swap` step's own probe (the second call): arm
        // the second call so the failure lands on the probe under test
        // rather than being consumed by `Root::open`.
        failing.fail_nth_fsops_device_inode(2);
        let report = reconcile(&journal, &g, &failing).expect("reconciliation must run");
        let interrupted = report
            .interrupted
            .iter()
            .find(|p| p.id == id)
            .unwrap_or_else(|| {
                panic!("a probe error other than NotFound must interrupt the plan, not {report:?}")
            });
        assert!(
            interrupted.error.contains("fsops_device_inode"),
            "the report must name the error the probe hit, got: {}",
            interrupted.error
        );

        let after = tree_snapshot(&fixture, &root_path);
        assert_eq!(
            after, before_reconcile,
            "an I/O error while probing must not delete or change anything, or names the old folder it deleted"
        );
        assert!(
            failing.symlink_metadata(&staged_path).is_ok(),
            "the Swap step's own probe must interrupt the plan before the paired Stage step's reversal runs, or names the staged folder it deleted"
        );
    }

    /// Given an existing file, when `write_file`'s rename lands but the
    /// process dies before the primitive returns, then startup
    /// reconciliation restores the previous bytes from the backup
    /// `write_file` fsynced *before* that rename ran; on failure the panic
    /// names the lost backup (the live content stuck on the new bytes).
    #[test]
    fn a_crash_after_the_write_file_rename_restores_the_previous_bytes_or_names_the_lost_backup() {
        let fixture = FixtureBuilder::new()
            .dir("/journal")
            .dir("/root")
            .file("/root/file.txt", b"original")
            .build_fs();
        let root_path = PathBuf::from("/root");
        let before = tree_snapshot(&fixture, &root_path);

        let failing = FailingFs::wrap(Arc::new(fixture.clone()));
        let root = Root::open(&failing, root_path.clone()).expect("open root");
        let journal = journal_over(Arc::new(fixture.clone()));
        let lease = FakeLease::default();
        let g = guard(&lease);

        let id = PlanId("01PLANWRITECRASH0000000001".into());
        let plan = PlanWriter::begin(
            &journal,
            &g,
            id.clone(),
            Utc::now(),
            "crash after the write_file rename",
            root_path.clone(),
            Vec::new(),
        )
        .expect("begin");

        let target = root_path.join("file.txt");
        let stamp = fsops::read_stamp(&failing, &target).expect("read the stamp");
        // The rename that lands the new bytes is unaffected; the very next
        // call - `fsync_up_to_root`'s first `fsops_fsync_dir` - fails,
        // landing the crash right after the rename.
        failing.fail_next_fsops_fsync_dir();
        fsops::write_file(&root, &plan, Path::new("file.txt"), b"new bytes", &stamp)
            .expect_err("the fsync after the rename must fail, or this test proves nothing about the window after it");
        drop(plan);

        let report = reconcile(&journal, &g, &fixture).expect("reconciliation must run");
        assert!(
            report.reversed.contains(&id),
            "a write_file step whose rename landed must still be fully reversible, not {report:?}"
        );

        let after = tree_snapshot(&fixture, &root_path);
        assert_eq!(
            after, before,
            "reconciliation must restore the previous bytes, or name the lost backup"
        );
    }

    /// Given an existing file one byte larger than `MAX_JOURNAL_JSON_BYTES`
    /// (the cap `read_backup` used to reuse from plan/manifest JSON
    /// readback), when `write_file` backs it up (with no cap, matching how
    /// it read the pre-write bytes) and then crashes before `finish`, then
    /// startup reconciliation still reads that backup back and restores it
    /// byte-for-byte; on failure the panic names the cap that blocked it
    /// (the post-write bytes left in place, plan `Interrupted`).
    #[test]
    fn a_backup_larger_than_the_journal_json_cap_still_restores_or_names_the_cap_that_blocked_it() {
        let big = vec![b'A'; (MAX_JOURNAL_JSON_BYTES + 1) as usize];
        let fixture = FixtureBuilder::new()
            .dir("/journal")
            .dir("/root")
            .file("/root/file.txt", &big)
            .build_fs();
        let root_path = PathBuf::from("/root");
        let before = tree_snapshot(&fixture, &root_path);

        let failing = FailingFs::wrap(Arc::new(fixture.clone()));
        let root = Root::open(&failing, root_path.clone()).expect("open root");
        let journal = journal_over(Arc::new(fixture.clone()));
        let lease = FakeLease::default();
        let g = guard(&lease);

        let id = PlanId("01PLANBIGBACKUP0000000001".into());
        let plan = PlanWriter::begin(
            &journal,
            &g,
            id.clone(),
            Utc::now(),
            "write over a file whose backup exceeds the journal JSON cap",
            root_path.clone(),
            Vec::new(),
        )
        .expect("begin");

        let target = root_path.join("file.txt");
        let stamp = fsops::read_stamp(&failing, &target).expect("read the stamp");
        // Same window as the write_file crash test above: the rename lands,
        // the fsync right after it fails.
        failing.fail_next_fsops_fsync_dir();
        fsops::write_file(&root, &plan, Path::new("file.txt"), b"new bytes", &stamp)
            .expect_err("the fsync after the rename must fail, or this test proves nothing about the window after it");
        drop(plan);

        let report = reconcile(&journal, &g, &fixture).expect("reconciliation must run");
        assert!(
            report.reversed.contains(&id),
            "a write_file step whose backup exceeds MAX_JOURNAL_JSON_BYTES must still be reversible, not {report:?}"
        );

        let after = tree_snapshot(&fixture, &root_path);
        assert_eq!(
            after, before,
            "reconciliation must restore the oversized backup byte-for-byte, or names the cap that blocked it"
        );
    }

    /// Given an existing symlink, when `link`'s rename lands but the
    /// process dies before the primitive returns, then startup
    /// reconciliation restores the previous target `link` recorded before
    /// that rename ran; on failure the panic names the target it forgot
    /// (the live link stuck on the new target instead).
    #[test]
    fn a_crash_after_the_link_rename_restores_the_previous_link_target_or_names_the_target_it_forgot(
    ) {
        let fixture = FixtureBuilder::new()
            .dir("/journal")
            .dir("/root")
            .file("/root/old.txt", b"old")
            .file("/root/new.txt", b"new")
            .build_fs();
        let root_path = PathBuf::from("/root");
        let journal = journal_over(Arc::new(fixture.clone()));
        let lease = FakeLease::default();
        let g = guard(&lease);

        // Seed an existing link pointing at `old.txt`, fully committed, so
        // this test's own crash is about *replacing* a link, not creating
        // one from nothing.
        let seed_root = Root::open(&fixture, root_path.clone()).expect("open root");
        let seed_id = PlanId("01PLANLINKSEED000000000001".into());
        let seed_plan = PlanWriter::begin(
            &journal,
            &g,
            seed_id,
            Utc::now(),
            "seed the existing link",
            root_path.clone(),
            Vec::new(),
        )
        .expect("begin seed plan");
        fsops::link(
            &seed_root,
            &seed_plan,
            Path::new("skill-current"),
            Path::new("old.txt"),
        )
        .expect("seed link");
        seed_plan
            .finish(PlanStatus::Done)
            .expect("finish seed plan");

        let before = tree_snapshot(&fixture, &root_path);

        let failing = FailingFs::wrap(Arc::new(fixture.clone()));
        let root = Root::open(&failing, root_path.clone()).expect("open root");
        let id = PlanId("01PLANLINKCRASH0000000001".into());
        let plan = PlanWriter::begin(
            &journal,
            &g,
            id.clone(),
            Utc::now(),
            "crash after the link rename",
            root_path.clone(),
            Vec::new(),
        )
        .expect("begin");
        // Same window as the write_file test above: the rename lands, the
        // fsync right after it fails.
        failing.fail_next_fsops_fsync_dir();
        fsops::link(&root, &plan, Path::new("skill-current"), Path::new("new.txt"))
            .expect_err("the fsync after the rename must fail, or this test proves nothing about the window after it");
        drop(plan);

        let report = reconcile(&journal, &g, &fixture).expect("reconciliation must run");
        assert!(
            report.reversed.contains(&id),
            "a link step whose rename landed must still be fully reversible, not {report:?}"
        );

        let after = tree_snapshot(&fixture, &root_path);
        assert_eq!(
            after, before,
            "reconciliation must restore the previous link target, or name the target it forgot"
        );
    }

    /// Given an existing symlink whose target reconciliation is replacing -
    /// the same seeded-link setup as the test above - when the *reversal's
    /// own restore* is what crashes (the temp-symlink-then-rename that
    /// swaps `path` onto `previous_target`), then the plan row stays
    /// `Pending` on disk (the crash landed before reconciliation's own
    /// `finish` call - the exact window `reconcile` already protects for a
    /// forward mutation), `path` never shows an absent link in between, and
    /// the next full reconciliation - with a healthy filesystem - retries
    /// and fully restores `previous_target`; on failure the panic names the
    /// target it lost.
    #[test]
    fn a_crash_inside_link_reversal_restores_the_previous_target_on_the_next_reconcile_or_names_the_target_it_lost(
    ) {
        let fixture = FixtureBuilder::new()
            .dir("/journal")
            .dir("/root")
            .file("/root/old.txt", b"old")
            .file("/root/new.txt", b"new")
            .build_fs();
        let root_path = PathBuf::from("/root");
        let journal = journal_over(Arc::new(fixture.clone()));
        let lease = FakeLease::default();
        let g = guard(&lease);

        // Seed an existing link pointing at `old.txt`, fully committed.
        let seed_root = Root::open(&fixture, root_path.clone()).expect("open root");
        let seed_id = PlanId("01PLANLINKATOMSEED00000001".into());
        let seed_plan = PlanWriter::begin(
            &journal,
            &g,
            seed_id,
            Utc::now(),
            "seed the existing link",
            root_path.clone(),
            Vec::new(),
        )
        .expect("begin seed plan");
        fsops::link(
            &seed_root,
            &seed_plan,
            Path::new("skill-current"),
            Path::new("old.txt"),
        )
        .expect("seed link");
        seed_plan
            .finish(PlanStatus::Done)
            .expect("finish seed plan");
        let pristine = tree_snapshot(&fixture, &root_path);

        // A second plan replaces the link with one pointing at `new.txt`,
        // fully landing (no crash in the forward direction), then is left
        // Pending - the ordinary "crash before finish" case.
        let root = Root::open(&fixture, root_path.clone()).expect("open root");
        let id = PlanId("01PLANLINKATOM0000000001".into());
        let plan = PlanWriter::begin(
            &journal,
            &g,
            id.clone(),
            Utc::now(),
            "replace the link, then crash before finish",
            root_path.clone(),
            Vec::new(),
        )
        .expect("begin");
        fsops::link(
            &root,
            &plan,
            Path::new("skill-current"),
            Path::new("new.txt"),
        )
        .expect("link");
        drop(plan);

        // Simulates the reversal itself crashing partway through its own
        // restore - calling `reverse_steps` directly, the way `reconcile`
        // does internally, so the row's status is never written either
        // way: still `Pending` on disk, same as a real process death.
        let record = journal
            .all()
            .expect("read")
            .into_iter()
            .find(|p| p.id == id)
            .expect("the plan begun above");
        let failing = FailingFs::wrap(Arc::new(fixture.clone()));
        failing.fail_next_fsops_rename();
        reverse_steps(&journal, &record, &failing).expect_err(
            "the rename inside the atomic link restore must fail, or this test proves nothing about a crash mid-restore",
        );

        let after_first_attempt = fixture
            .read_link(Path::new("/root/skill-current"))
            .expect("a crash mid-restore must never leave `path` without any link at all");
        assert_eq!(
            after_first_attempt,
            PathBuf::from("new.txt"),
            "the temp-then-rename shape must leave `path` showing its pre-restore link until the rename lands, or names the target it lost"
        );

        let report = reconcile(&journal, &g, &fixture).expect("reconciliation must run");
        assert!(
            report.reversed.contains(&id),
            "the retried reconciliation must fully reverse the link step, not {report:?}"
        );

        let restored = fixture
            .read_link(Path::new("/root/skill-current"))
            .expect("read the restored link");
        assert_eq!(
            restored,
            PathBuf::from("old.txt"),
            "reconciliation must restore the previous target, or name the target it lost"
        );

        let after = tree_snapshot(&fixture, &root_path);
        assert_eq!(
            after, pristine,
            "a fully retried reversal must leave no leaked temp entry behind, or names the extra entry"
        );
    }

    /// Writes `file.txt` from "original" to "new bytes" under a plan that
    /// is then left `Pending` (the ordinary crash before `finish`), and
    /// returns the plan's id plus the tree as it stood before the write.
    fn pending_write_over_original(
        fixture: &crate::testing::FixtureFs,
        journal: &FsJournal,
        g: &ExclusiveGuard,
        id: &str,
    ) -> (PlanId, std::collections::BTreeMap<PathBuf, Vec<u8>>) {
        let root_path = PathBuf::from("/root");
        let before = tree_snapshot(fixture, &root_path);
        let root = Root::open(fixture, root_path.clone()).expect("open root");
        let id = PlanId(id.into());
        let plan = PlanWriter::begin(
            journal,
            g,
            id.clone(),
            Utc::now(),
            "write over the file, then crash before finish",
            root_path.clone(),
            Vec::new(),
        )
        .expect("begin");
        let target = root_path.join("file.txt");
        let stamp = fsops::read_stamp(fixture, &target).expect("read the stamp");
        fsops::write_file(&root, &plan, Path::new("file.txt"), b"new bytes", &stamp)
            .expect("write");
        drop(plan);
        (id, before)
    }

    /// Given a pending write over an existing file, when an older build's
    /// reversal crashed after removing the file and before writing the
    /// backup back (the file is simply gone), then the next reconcile
    /// still restores the backup; on failure the panic names the file the
    /// plan was marked `Reversed` without restoring.
    #[test]
    fn a_reversal_that_crashed_after_removing_the_file_restores_the_backup_or_names_the_lost_file()
    {
        let fixture = FixtureBuilder::new()
            .dir("/journal")
            .dir("/root")
            .file("/root/file.txt", b"original")
            .build_fs();
        let journal = journal_over(Arc::new(fixture.clone()));
        let lease = FakeLease::default();
        let g = guard(&lease);
        let (id, before) =
            pending_write_over_original(&fixture, &journal, &g, "01PLANWRITEUNDOGONE0000001");

        fixture
            .fsops_remove_file(Path::new("/root/file.txt"))
            .expect("simulate the half-done reversal");

        let report = reconcile(&journal, &g, &fixture).expect("reconciliation must run");
        assert!(report.reversed.contains(&id), "not reversed: {report:?}");
        assert_eq!(
            tree_snapshot(&fixture, Path::new("/root")),
            before,
            "the backup must come back at /root/file.txt, or the plan is Reversed with the file lost"
        );
    }

    /// Given a pending write over an existing file, when the reversal's own
    /// rename fails partway, then `file.txt` still shows a complete file
    /// (never no file), and the next reconcile restores the backup and
    /// leaves no restore temp behind; on failure the panic names the
    /// missing file or the leaked temp.
    #[test]
    fn a_crash_inside_write_file_reversal_never_leaves_the_file_missing_or_names_the_gap() {
        let fixture = FixtureBuilder::new()
            .dir("/journal")
            .dir("/root")
            .file("/root/file.txt", b"original")
            .build_fs();
        let journal = journal_over(Arc::new(fixture.clone()));
        let lease = FakeLease::default();
        let g = guard(&lease);
        let (id, before) =
            pending_write_over_original(&fixture, &journal, &g, "01PLANWRITEUNDOCRASH000001");

        let record = journal
            .all()
            .expect("read")
            .into_iter()
            .find(|p| p.id == id)
            .expect("the plan begun above");
        let failing = FailingFs::wrap(Arc::new(fixture.clone()));
        failing.fail_next_fsops_rename();
        reverse_steps(&journal, &record, &failing)
            .expect_err("the restore rename must fail, or this test proves nothing");
        assert_eq!(
            fixture
                .read_capped(Path::new("/root/file.txt"), u64::MAX)
                .expect("a crash mid-restore must never leave /root/file.txt missing"),
            b"new bytes".to_vec(),
        );

        let report = reconcile(&journal, &g, &fixture).expect("reconciliation must run");
        assert!(report.reversed.contains(&id), "not reversed: {report:?}");
        assert_eq!(
            tree_snapshot(&fixture, Path::new("/root")),
            before,
            "the retried reversal must restore the backup and remove its leaked temp"
        );
    }

    /// Given a plan that already settled `Done`, when anything calls
    /// `finish` on it again, then the call fails and the status stays
    /// `Done`; on failure the panic names the status it was overwritten to.
    #[test]
    fn a_settled_plan_refuses_a_second_finish_or_names_the_overwritten_status() {
        let fixture = FixtureBuilder::new()
            .dir("/journal")
            .dir("/root")
            .build_fs();
        let journal = journal_over(Arc::new(fixture.clone()));
        let lease = FakeLease::default();
        let g = guard(&lease);
        let id = PlanId("01PLANSETTLEDONCE000000001".into());
        PlanWriter::begin(
            &journal,
            &g,
            id.clone(),
            Utc::now(),
            "settle once",
            PathBuf::from("/root"),
            Vec::new(),
        )
        .expect("begin")
        .finish(PlanStatus::Done)
        .expect("first finish");

        journal
            .finish(&g, &id, PlanStatus::Reversed)
            .expect_err("a Done plan must not become Reversed");
        let status = journal
            .all()
            .expect("read")
            .into_iter()
            .find(|p| p.id == id)
            .expect("the plan")
            .status;
        assert_eq!(status, PlanStatus::Done, "overwritten to {status:?}");
    }

    /// Given a user's own entries next to a managed link that share the
    /// `.<leaf>-` temp prefix - a regular file with the prefix, a regular
    /// file with the exact `<pid>-<counter>` shape, a symlink with the
    /// prefix, a symlink with the exact shape pointing somewhere of the
    /// user's own, and a symlink with non-digit suffix parts pointing at the
    /// plan's own target - when a crashed link replacement is reconciled, then the
    /// pre-restore sweep removes none of them (it only removes symlinks of
    /// that shape that point at the plan's own targets); on failure the
    /// tree diff names the user entry it deleted.
    #[test]
    fn the_pre_restore_sweep_leaves_a_users_dot_entry_that_shares_the_temp_prefix_or_names_the_entry_it_deleted(
    ) {
        let fixture = FixtureBuilder::new()
            .dir("/journal")
            .dir("/root")
            .file("/root/old.txt", b"old")
            .file("/root/new.txt", b"new")
            .file("/root/other.txt", b"other")
            .file("/root/.skill-current-notes", b"mine")
            .file("/root/.skill-current-123-4", b"mine too")
            .build_fs();
        let root_path = PathBuf::from("/root");
        let journal = journal_over(Arc::new(fixture.clone()));
        let lease = FakeLease::default();
        let g = guard(&lease);

        let seed_root = Root::open(&fixture, root_path.clone()).expect("open root");
        let seed_plan = PlanWriter::begin(
            &journal,
            &g,
            PlanId("01PLANLINKSWEEPSEED000001".into()),
            Utc::now(),
            "seed the existing link",
            root_path.clone(),
            Vec::new(),
        )
        .expect("begin seed plan");
        fsops::link(
            &seed_root,
            &seed_plan,
            Path::new("skill-current"),
            Path::new("old.txt"),
        )
        .expect("seed link");
        seed_plan
            .finish(PlanStatus::Done)
            .expect("finish seed plan");
        // A user's own symlink that shares the prefix but not the pid-counter
        // shape must survive the sweep too.
        fixture
            .fsops_symlink(Path::new("old.txt"), Path::new("/root/.skill-current-mine"))
            .expect("seed the user's own dot-prefixed link");
        // Exact temp-name shape, but pointing at the user's own file, not at
        // either target the plan's steps name.
        fixture
            .fsops_symlink(
                Path::new("other.txt"),
                Path::new("/root/.skill-current-2024-01"),
            )
            .expect("seed the user's own link that fits the temp-name shape");
        // Two hyphen-separated parts that are not digits, pointing at the
        // plan's own previous target: only the digit check keeps this one.
        fixture
            .fsops_symlink(
                Path::new("old.txt"),
                Path::new("/root/.skill-current-foo-bar"),
            )
            .expect("seed the user's own link with non-digit suffix parts");

        let pristine = tree_snapshot(&fixture, &root_path);

        let root = Root::open(&fixture, root_path.clone()).expect("open root");
        let id = PlanId("01PLANLINKSWEEP0000000001".into());
        let plan = PlanWriter::begin(
            &journal,
            &g,
            id.clone(),
            Utc::now(),
            "replace the link, then crash before finish",
            root_path.clone(),
            Vec::new(),
        )
        .expect("begin");
        fsops::link(
            &root,
            &plan,
            Path::new("skill-current"),
            Path::new("new.txt"),
        )
        .expect("link");
        drop(plan);

        let report = reconcile(&journal, &g, &fixture).expect("reconciliation must run");
        assert!(
            report.reversed.contains(&id),
            "the reversal must still converge, not {report:?}"
        );
        let after = tree_snapshot(&fixture, &root_path);
        assert_eq!(
            after, pristine,
            "the sweep must only delete its own pid-counter temp links, or name the user entry it deleted"
        );
    }

    /// Given two temp symlinks a crashed reversal leaked next to a managed
    /// link - one pointing at the step's previous target, one at its new
    /// target - when a fresh process retries the reversal, then the sweep
    /// clears both and the restored link points at the previous target; on
    /// failure the tree diff names the leaked entry it left behind.
    #[test]
    fn a_legacy_plan_clears_old_prefix_temps_on_retry_or_leaves_a_leak() {
        let fixture = FixtureBuilder::new()
            .dir("/journal")
            .dir("/root")
            .file("/root/old.txt", b"old")
            .file("/root/new.txt", b"new")
            .build_fs();
        let root_path = PathBuf::from("/root");
        let journal = journal_over(Arc::new(fixture.clone()));
        let lease = FakeLease::default();
        let g = guard(&lease);

        // Seed an existing link pointing at `old.txt`, fully committed.
        let seed_root = Root::open(&fixture, root_path.clone()).expect("open root");
        let seed_id = PlanId("01PLANLINKCOLLSEED0000001".into());
        let seed_plan = PlanWriter::begin(
            &journal,
            &g,
            seed_id,
            Utc::now(),
            "seed the existing link",
            root_path.clone(),
            Vec::new(),
        )
        .expect("begin seed plan");
        fsops::link(
            &seed_root,
            &seed_plan,
            Path::new("skill-current"),
            Path::new("old.txt"),
        )
        .expect("seed link");
        seed_plan
            .finish(PlanStatus::Done)
            .expect("finish seed plan");

        let pristine = tree_snapshot(&fixture, &root_path);

        // A second plan replaces the link with one pointing at `new.txt`,
        // fully landing, then is left `Pending` - the plan a fresh process
        // finds and retries reversing on.
        let root = Root::open(&fixture, root_path.clone()).expect("open root");
        let id = PlanId("01PLANLINKCOLL0000000001".into());
        let plan = PlanWriter::begin(
            &journal,
            &g,
            id.clone(),
            Utc::now(),
            "replace the link, then crash before finish",
            root_path.clone(),
            Vec::new(),
        )
        .expect("begin");
        fsops::link(
            &root,
            &plan,
            Path::new("skill-current"),
            Path::new("new.txt"),
        )
        .expect("link");
        drop(plan);
        let mut legacy: serde_json::Value = journal.read_json(&journal.plan_path(&id)).unwrap();
        for step in legacy["steps"].as_array_mut().unwrap() {
            step.as_object_mut().unwrap().shift_remove("temporary");
        }
        journal
            .write_json(
                &journal.plan_path(&id),
                &serde_json::to_vec(&legacy).unwrap(),
            )
            .unwrap();

        // Simulates the leaked temp symlink a first reversal attempt left
        // behind after minting it but before the rename that would have
        // consumed it: the exact name a fresh process's first mint of this
        // formula (`fsops::unique_suffix`, pid-then-counter) would produce -
        // this test's own throwaway call names the value only to keep the
        // fixture path visibly derived from that same formula, not to
        // predict what the retry below will itself mint.
        let leaked_temp = root_path.join(format!(".skill-current-{}", fsops::unique_suffix()));
        fixture
            .fsops_symlink(Path::new("old.txt"), &leaked_temp)
            .expect("seed the leaked temp symlink a crashed first attempt left behind");
        // A second leaked temp from an earlier attempt that pointed at the
        // step's own `target` rather than its `previous_target`: the sweep
        // must clear both.
        let leaked_temp_at_target =
            root_path.join(format!(".skill-current-{}", fsops::unique_suffix()));
        fixture
            .fsops_symlink(Path::new("new.txt"), &leaked_temp_at_target)
            .expect("seed the leaked temp symlink that points at the new target");

        let report = reconcile(&journal, &g, &fixture).expect("reconciliation must run");
        assert!(
            report.reversed.contains(&id),
            "a fresh process's retried reversal must still converge and clear the leaked temp entry, not {report:?}"
        );

        let restored = fixture
            .read_link(Path::new("/root/skill-current"))
            .expect("read the restored link");
        assert_eq!(
            restored,
            PathBuf::from("old.txt"),
            "reconciliation must restore the previous target, or name the leaked temp entry that blocked it"
        );

        let after = tree_snapshot(&fixture, &root_path);
        assert_eq!(
            after, pristine,
            "a converged retry must leave no leaked temp entry behind, or name the entry it left"
        );
    }

    /// Given a `Link` step's `path` recorded as a symlink but now showing a
    /// regular file - something else replaced it since the step was
    /// recorded, or the fake's `read_link` used to misreport a regular file
    /// as absent (`NotFound`) rather than `RealFs`'s `EINVAL` - when
    /// reconciliation reverses it, the probe's non-`NotFound` error must
    /// leave "landed" unknown rather than read the absence-shaped error as
    /// "never landed" (which would have renamed the restored target
    /// straight over the file). The plan resolves `Interrupted` and the
    /// file's bytes never move; on failure the panic names the file it
    /// overwrote.
    #[test]
    fn link_reversal_never_replaces_a_regular_file_at_the_link_path_or_names_the_file_it_overwrote()
    {
        let fixture = FixtureBuilder::new()
            .dir("/journal")
            .dir("/root")
            .file("/root/skill-current", b"the user's own file, not a link")
            .build_fs();
        let root_path = PathBuf::from("/root");
        let journal = journal_over(Arc::new(fixture.clone()));
        let lease = FakeLease::default();
        let g = guard(&lease);

        // Record a `Link` step directly - bypassing `fsops::link`'s own
        // `WouldReplaceFile` guard - as if the step was recorded before a
        // regular file ended up at `path` some other way.
        let id = PlanId("01PLANLINKREGULARFILE00001".into());
        let plan = PlanWriter::begin(
            &journal,
            &g,
            id.clone(),
            Utc::now(),
            "a Link step whose path now holds a regular file",
            root_path.clone(),
            Vec::new(),
        )
        .expect("begin");
        plan.record_link(
            Path::new("/root/skill-current"),
            Path::new("new.txt"),
            Some(PathBuf::from("old.txt")),
        )
        .expect("record link");
        drop(plan);

        let before = fixture
            .read_capped(Path::new("/root/skill-current"), u64::MAX)
            .expect("read the file's bytes before reconciliation");

        let report = reconcile(&journal, &g, &fixture).expect("reconciliation must run");
        assert!(
            report.interrupted.iter().any(|p| p.id == id),
            "a regular file at the link path must interrupt the plan rather than being read as an absent link, not {report:?}"
        );

        let after = fixture
            .read_capped(Path::new("/root/skill-current"), u64::MAX)
            .expect("read the file's bytes after reconciliation");
        assert_eq!(
            after, before,
            "reconciliation must never replace a regular file at a link's path, or names the file it overwrote"
        );
    }

    /// Given a step recorded before its mutation ran, when the mutation
    /// itself then fails and never lands - the crash landing in the window
    /// `record_*` guarantees now exists between "recorded" and "mutated" -
    /// then startup reconciliation must still resolve the plan, doing
    /// nothing to a tree that already matches the pre-plan snapshot; on
    /// failure the panic names the step reversal undid a second time
    /// (proof it was not the no-op reversing an unlanded step must be).
    #[test]
    fn a_step_recorded_before_a_mutation_that_never_landed_reverses_to_a_no_op_or_names_the_step_it_undid_twice(
    ) {
        let fixture = FixtureBuilder::new()
            .dir("/journal")
            .dir("/root")
            .build_fs();
        let root_path = PathBuf::from("/root");
        let before = tree_snapshot(&fixture, &root_path);

        let failing = FailingFs::wrap(Arc::new(fixture.clone()));
        let root = Root::open(&failing, root_path.clone()).expect("open root");
        let journal = journal_over(Arc::new(fixture.clone()));
        let lease = FakeLease::default();
        let g = guard(&lease);

        let id = PlanId("01PLANNEVERLANDED000000001".into());
        let plan = PlanWriter::begin(
            &journal,
            &g,
            id.clone(),
            Utc::now(),
            "step recorded, mutation never lands",
            root_path.clone(),
            Vec::new(),
        )
        .expect("begin");

        // `newskill` does not exist yet, so `swap` takes its fresh-target
        // branch: no quarantine, just a rename of the staged folder into
        // place. That rename is the one made to fail, so the only trace
        // left behind is the staged folder itself - which the paired
        // `Stage` step's own (unconditional) reversal already removes,
        // proving the no-op holds even with two steps recorded and only
        // one of them mutating anything.
        let staged = fsops::stage(
            &root,
            &plan,
            &[(PathBuf::from("SKILL.md"), b"new content".to_vec())],
        )
        .expect("stage");
        failing.fail_next_fsops_rename();
        fsops::swap(&root, &plan, Path::new("newskill"), &staged, Path::new(".trash"))
            .expect_err("the rename must fail, or this test proves nothing about a step whose mutation never lands");
        drop(plan);

        let report =
            reconcile(&journal, &g, &fixture).expect("reconciliation must run without error");
        assert!(
            report.reversed.contains(&id),
            "a step recorded but never landed must still resolve as a (no-op) reversal, not {report:?}"
        );

        let after = tree_snapshot(&fixture, &root_path);
        assert_eq!(
            after, before,
            "reversing a step whose mutation never landed must be a no-op, or name the step it undid twice"
        );
    }

    /// Given a plan row whose one recorded step names a path outside the
    /// plan's own root - the shape a corrupt or hand-edited `plan.json`
    /// would take - when reconciliation runs, then it never touches that
    /// path, marks the plan `Interrupted`, and names the escaped path in
    /// the report; on failure the panic names whichever assertion the
    /// escape slipped past.
    #[test]
    fn reconcile_never_touches_a_path_outside_the_plan_root_or_names_the_escaped_path() {
        let fixture = FixtureBuilder::new()
            .dir("/journal")
            .dir("/root")
            .dir("/outside")
            .file("/outside/real.txt", b"do not touch")
            .build_fs();
        let root_path = PathBuf::from("/root");
        let journal = journal_over(Arc::new(fixture.clone()));
        let lease = FakeLease::default();
        let g = guard(&lease);

        let escaped = PathBuf::from("/outside/real.txt");
        let id = PlanId("01PLANESCAPE000000000001".into());
        let plan = PlanWriter::begin(
            &journal,
            &g,
            id.clone(),
            Utc::now(),
            "hand-edited row naming a path outside the root",
            root_path,
            Vec::new(),
        )
        .expect("begin");
        // Bypasses `fsops::stage`'s own confinement on purpose: this
        // simulates a plan row a crash or a bug left pointing outside its
        // root, not a step `fsops` itself would ever record.
        plan.record_stage(&escaped)
            .expect("record the escaping step");
        drop(plan);

        let report = reconcile(&journal, &g, &fixture).expect("reconciliation must run");
        let interrupted = report
            .interrupted
            .iter()
            .find(|p| p.id == id)
            .unwrap_or_else(|| panic!("plan {} must be Interrupted, not {report:?}", id.0));
        assert!(
            interrupted
                .error
                .contains(&escaped.to_string_lossy().into_owned()),
            "the report must name the escaped path, got: {}",
            interrupted.error
        );

        assert_eq!(
            fixture
                .read_capped(&escaped, u64::MAX)
                .expect("the file outside the root must be untouched"),
            b"do not touch",
            "reconciliation must never touch a path outside the plan root"
        );
    }

    /// Given two plans stacked on the same folder - plan A swaps `skill`
    /// from `old0` to `new1` (quarantining `old0`) and is left `Pending`;
    /// plan B then swaps `skill` again, from `new1` to `new2`
    /// (quarantining `new1`), and also crashes `Pending` - when startup
    /// reconciliation runs, then it reverses B before A (LIFO), restoring
    /// `skill` to `old0` byte-for-byte; on failure the panic names the
    /// generation `skill` was left at (oldest-first strands `old0` in A's
    /// own quarantine folder and leaves `skill` at `new1` instead).
    #[test]
    fn stacked_pending_plans_reverse_newest_first_or_names_the_folder_left_at_the_wrong_generation()
    {
        let fixture = FixtureBuilder::new()
            .dir("/journal")
            .dir("/root")
            .dir("/root/skill")
            .file("/root/skill/SKILL.md", b"old0")
            .build_fs();
        let root_path = PathBuf::from("/root");
        let before = tree_snapshot(&fixture, &root_path);

        let journal = journal_over(Arc::new(fixture.clone()));
        let lease = FakeLease::default();
        let g = guard(&lease);
        let root = Root::open(&fixture, root_path.clone()).expect("open root");

        let plan_a = PlanId("01PLANSTACKA0000000000001".into());
        let a = PlanWriter::begin(
            &journal,
            &g,
            plan_a.clone(),
            Utc::now(),
            "plan A: old0 -> new1, left Pending",
            root_path.clone(),
            Vec::new(),
        )
        .expect("begin A");
        let staged_a = fsops::stage(&root, &a, &[(PathBuf::from("SKILL.md"), b"new1".to_vec())])
            .expect("stage A");
        fsops::swap(
            &root,
            &a,
            Path::new("skill"),
            &staged_a,
            Path::new(".trash-a"),
        )
        .expect("swap A over the pre-plan folder");
        drop(a);

        let plan_b = PlanId("01PLANSTACKB0000000000002".into());
        let b = PlanWriter::begin(
            &journal,
            &g,
            plan_b.clone(),
            Utc::now(),
            "plan B: new1 -> new2, then crashes",
            root_path.clone(),
            Vec::new(),
        )
        .expect("begin B");
        let staged_b = fsops::stage(&root, &b, &[(PathBuf::from("SKILL.md"), b"new2".to_vec())])
            .expect("stage B");
        fsops::swap(
            &root,
            &b,
            Path::new("skill"),
            &staged_b,
            Path::new(".trash-b"),
        )
        .expect("swap B over plan A's result");
        drop(b);

        let report = reconcile(&journal, &g, &fixture).expect("reconciliation must run");
        assert!(
            report.reversed.contains(&plan_a) && report.reversed.contains(&plan_b),
            "both stacked plans must resolve as Reversed, not {report:?}"
        );

        let after = tree_snapshot(&fixture, &root_path);
        assert_eq!(
            after, before,
            "reversing newest-first must restore `skill` to old0 (the true pre-plan-A snapshot), or names the folder left at the wrong generation"
        );
    }

    /// Recursively reads every file's and symlink's content under `root`,
    /// keyed by path, so a test can compare a tree before and after some
    /// operation without typing literal paths into the assertion.
    fn tree_snapshot(
        fs: &dyn ScopeFs,
        root: &Path,
    ) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
        let mut out = std::collections::BTreeMap::new();
        collect_tree(fs, root, &mut out);
        out
    }

    fn collect_tree(
        fs: &dyn ScopeFs,
        dir: &Path,
        out: &mut std::collections::BTreeMap<PathBuf, Vec<u8>>,
    ) {
        let mut entries = fs.read_dir(dir).expect("read_dir");
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        for entry in entries {
            let path = dir.join(&entry.name);
            match entry.kind {
                FileKind::Dir => collect_tree(fs, &path, out),
                FileKind::File => {
                    let bytes = fs.read_capped(&path, u64::MAX).expect("read file");
                    out.insert(path, bytes);
                }
                FileKind::Symlink => {
                    let target = fs.read_link(&path).expect("read_link");
                    out.insert(path, target.to_string_lossy().into_owned().into_bytes());
                }
                FileKind::Other => {}
            }
        }
    }
    #[test]
    fn recorded_temp_file_keeps_a_matching_user_temp_or_deletes_unowned_bytes() {
        let fs = FixtureBuilder::new()
            .dir("/journal")
            .file("/root/value", b"before")
            .file("/root/.value-123-4.restore", b"before")
            .build_fs();
        let journal = journal_over(Arc::new(fs.clone()));
        let guard = guard(&FakeLease::default());
        let root = Root::open(&fs, PathBuf::from("/root")).unwrap();
        let before = tree_snapshot(&fs, Path::new("/root"));
        let plan = PlanWriter::begin(
            &journal,
            &guard,
            PlanId("recorded-temp-file".into()),
            Utc::now(),
            "replace value",
            PathBuf::from("/root"),
            Vec::new(),
        )
        .unwrap();
        fsops::write_file(
            &root,
            &plan,
            Path::new("value"),
            b"after",
            &fsops::read_stamp(&fs, Path::new("/root/value")).unwrap(),
        )
        .unwrap();
        drop(plan);
        assert_eq!(reconcile(&journal, &guard, &fs).unwrap().reversed.len(), 1);
        assert_eq!(
            tree_snapshot(&fs, Path::new("/root")),
            before,
            "only the recorded temp belongs to this plan"
        );
    }

    #[test]
    fn recorded_temp_link_keeps_a_matching_user_link_or_deletes_an_unowned_entry() {
        let fs = FixtureBuilder::new()
            .dir("/journal")
            .file("/root/old", b"old")
            .file("/root/new", b"new")
            .build_fs();
        fs.fsops_symlink(Path::new("old"), Path::new("/root/current"))
            .unwrap();
        fs.fsops_symlink(Path::new("old"), Path::new("/root/.current-123-4"))
            .unwrap();
        let journal = journal_over(Arc::new(fs.clone()));
        let guard = guard(&FakeLease::default());
        let root = Root::open(&fs, PathBuf::from("/root")).unwrap();
        let before = tree_snapshot(&fs, Path::new("/root"));
        let plan = PlanWriter::begin(
            &journal,
            &guard,
            PlanId("recorded-temp-link".into()),
            Utc::now(),
            "replace link",
            PathBuf::from("/root"),
            Vec::new(),
        )
        .unwrap();
        fsops::link(&root, &plan, Path::new("current"), Path::new("new")).unwrap();
        drop(plan);
        assert_eq!(reconcile(&journal, &guard, &fs).unwrap().reversed.len(), 1);
        assert_eq!(
            tree_snapshot(&fs, Path::new("/root")),
            before,
            "only the recorded temp belongs to this plan"
        );
    }
}
