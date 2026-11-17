#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! A folder under a scope root that another tool swaps for a link to a
//! folder outside the scope, after the path check and before the write.
//! The write must fail, and the outside folder must stay empty.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use skill_studio_core::error::CoreError;
use skill_studio_core::fsops::{self, Root};
use skill_studio_core::identity::PlanId;
use skill_studio_core::journal::{reconcile, FsJournal, PlanWriter};
use skill_studio_core::ports::{
    acquire_exclusive, confine, ExclusiveGuard, Journal, LeaseMode, LeaseProvider, PlanRecord,
    PlanStatus, PlanStep, ScopeFs, ScopedReads,
};
use skill_studio_core::scope::{NormalizedScope, RuntimeScope};
use skill_studio_host::{FileLease, RealFs};

struct Layout {
    _dir: tempfile::TempDir,
    home: PathBuf,
    outside: PathBuf,
}

fn layout() -> Layout {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().canonicalize().unwrap();
    let home = base.join("home");
    let outside = base.join("outside");
    std::fs::create_dir_all(home.join(".claude/skills/x")).unwrap();
    std::fs::create_dir_all(outside.join("x")).unwrap();
    Layout {
        _dir: dir,
        home,
        outside,
    }
}

fn scope_for(home: &Path) -> NormalizedScope {
    NormalizedScope::normalize_with_discovery(&RuntimeScope::fixture(home), &RealFs::new(), None)
        .unwrap()
}

/// Replaces `home/.claude/skills` with a link to `outside`.
fn swap_skills_for_link(layout: &Layout) {
    let skills = layout.home.join(".claude/skills");
    std::fs::remove_dir_all(&skills).unwrap();
    std::os::unix::fs::symlink(&layout.outside, &skills).unwrap();
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            found.extend(files_under(&path));
        } else {
            found.push(path);
        }
    }
    found
}

#[test]
fn write_atomic_refuses_a_folder_swapped_for_a_link_out_of_the_scope() {
    let layout = layout();
    let scope = scope_for(&layout.home);
    let fs = RealFs::new();
    let guard = acquire_exclusive(&FileLease::new(layout.home.join(".lease")), &scope).unwrap();
    let target = confine(&scope, &fs, &layout.home.join(".claude/skills/x/SKILL.md")).unwrap();

    swap_skills_for_link(&layout);
    let result = fs.write_atomic(&guard, &target, b"escaped");

    assert_eq!(files_under(&layout.outside), Vec::<PathBuf>::new());
    assert_eq!(
        result.map_err(|e| e.kind()),
        Err(std::io::ErrorKind::PermissionDenied),
        "the write followed the link out of the scope"
    );
}

#[test]
fn rename_refuses_a_folder_swapped_for_a_link_out_of_the_scope() {
    let layout = layout();
    let scope = scope_for(&layout.home);
    let fs = RealFs::new();
    let guard = acquire_exclusive(&FileLease::new(layout.home.join(".lease")), &scope).unwrap();
    let source = layout.home.join("src.md");
    std::fs::write(&source, b"escaped").unwrap();
    let from = confine(&scope, &fs, &source).unwrap();
    let to = confine(&scope, &fs, &layout.home.join(".claude/skills/x/SKILL.md")).unwrap();

    swap_skills_for_link(&layout);
    let result = fs.rename(&guard, &from, &to);

    assert_eq!(files_under(&layout.outside), Vec::<PathBuf>::new());
    assert_eq!(
        result.map_err(|e| e.kind()),
        Err(std::io::ErrorKind::PermissionDenied),
        "the rename followed the link out of the scope"
    );
    assert!(source.exists());
}

#[test]
fn fsops_swap_refuses_a_folder_swapped_for_a_link_after_its_check() {
    let layout = layout();
    std::fs::remove_dir(layout.outside.join("x")).unwrap();
    // Reads confined to another home see nothing under this root, so
    // `Root::confine` runs as if its check came before the link swap.
    let blind_home = tempfile::tempdir().unwrap();
    let blind_scope = scope_for(blind_home.path());
    let real = RealFs::new();
    let blind = ScopedReads::new(&real, &blind_scope);
    let root = Root::open(&blind, layout.home.clone()).unwrap();
    let lease_dir = layout.home.join(".lease");
    std::fs::create_dir_all(&lease_dir).unwrap();
    let journal_fs: Arc<dyn ScopeFs> = Arc::new(RealFs::new());
    let journal = FsJournal::new(layout.home.join(".journal"), journal_fs);
    let handle = FileLease::new(lease_dir)
        .acquire(&[], LeaseMode::Exclusive, Duration::from_secs(5))
        .unwrap();
    let guard = ExclusiveGuard::from_handle(handle);
    let plan = PlanWriter::begin(
        &journal,
        &guard,
        PlanId("01PLANCONFINERACE000000001".to_string()),
        Utc::now(),
        "confine race test",
        layout.home.clone(),
        Vec::new(),
    )
    .unwrap();
    let staged = fsops::stage(
        &root,
        &plan,
        &[(PathBuf::from("SKILL.md"), b"escaped".to_vec())],
    )
    .unwrap();

    swap_skills_for_link(&layout);
    let result = fsops::swap(
        &root,
        &plan,
        Path::new(".claude/skills/x"),
        &staged,
        Path::new(".trash"),
    );

    assert_eq!(files_under(&layout.outside), Vec::<PathBuf>::new());
    let error = result.expect_err("the swap followed the link out of the root");
    assert!(error.to_string().contains("outside"), "{error}");
}

#[test]
fn journal_write_refuses_a_folder_swapped_for_a_link_after_its_check() {
    let layout = layout();
    std::fs::remove_dir(layout.outside.join("x")).unwrap();
    let journal_root = layout.home.join(".journal");
    std::fs::create_dir_all(&journal_root).unwrap();
    std::os::unix::fs::symlink(&layout.outside, journal_root.join("plans")).unwrap();
    // Reads confined to another home see nothing under the journal, so the
    // journal's path check runs as if it came before the link swap.
    let blind_home = tempfile::tempdir().unwrap();
    let blind_scope: &'static NormalizedScope = Box::leak(Box::new(scope_for(blind_home.path())));
    let journal_fs: Arc<dyn ScopeFs> = Arc::new(ScopedReads::new(&RealFs, blind_scope));
    let journal = FsJournal::new(journal_root, journal_fs);
    let lease_dir = layout.home.join(".lease");
    std::fs::create_dir_all(&lease_dir).unwrap();
    let handle = FileLease::new(lease_dir)
        .acquire(&[], LeaseMode::Exclusive, Duration::from_secs(5))
        .unwrap();
    let guard = ExclusiveGuard::from_handle(handle);

    let result = PlanWriter::begin(
        &journal,
        &guard,
        PlanId("01PLANJOURNALRACE000000001".to_string()),
        Utc::now(),
        "journal race test",
        layout.home.clone(),
        Vec::new(),
    );

    assert_eq!(files_under(&layout.outside), Vec::<PathBuf>::new());
    assert!(
        result.is_err(),
        "the journal write followed the link out of its root"
    );
}

/// An [`FsJournal`] that runs `swap` once, when recovery reads a backup:
/// after recovery checked the step's path, before it writes the backup back.
struct SwapOnBackupRead<F: Fn() + Send> {
    inner: FsJournal,
    swap: F,
}

impl<F: Fn() + Send> Journal for SwapOnBackupRead<F> {
    fn begin(&self, guard: &ExclusiveGuard, plan: &PlanRecord) -> Result<(), CoreError> {
        self.inner.begin(guard, plan)
    }
    fn record_step(
        &self,
        guard: &ExclusiveGuard,
        id: &PlanId,
        step: PlanStep,
    ) -> Result<(), CoreError> {
        self.inner.record_step(guard, id, step)
    }
    fn finish(
        &self,
        guard: &ExclusiveGuard,
        id: &PlanId,
        status: PlanStatus,
    ) -> Result<(), CoreError> {
        self.inner.finish(guard, id, status)
    }
    fn record_recovery_failure(
        &self,
        guard: &ExclusiveGuard,
        id: &PlanId,
        error: &str,
    ) -> Result<(), CoreError> {
        self.inner.record_recovery_failure(guard, id, error)
    }
    fn all(&self) -> Result<Vec<PlanRecord>, CoreError> {
        self.inner.all()
    }
    fn pending(&self) -> Result<Vec<PlanRecord>, CoreError> {
        self.inner.pending()
    }
    fn remove_backup(
        &self,
        guard: &ExclusiveGuard,
        id: &PlanId,
        relative: &str,
    ) -> Result<(), CoreError> {
        self.inner.remove_backup(guard, id, relative)
    }
    fn write_backup(
        &self,
        guard: &ExclusiveGuard,
        id: &PlanId,
        relative: &str,
        bytes: &[u8],
    ) -> Result<(), CoreError> {
        self.inner.write_backup(guard, id, relative, bytes)
    }
    fn read_backup(&self, id: &PlanId, relative: &str) -> Result<Vec<u8>, CoreError> {
        (self.swap)();
        self.inner.read_backup(id, relative)
    }
}

#[test]
fn recovery_refuses_a_folder_swapped_for_a_link_after_its_check() {
    let layout = layout();
    let skills = layout.home.join(".claude/skills");
    let skill_file = skills.join("x/SKILL.md");
    std::fs::write(&skill_file, b"new").unwrap();
    std::fs::write(layout.outside.join("x/SKILL.md"), b"user").unwrap();
    let scope = scope_for(&layout.home);
    let guard = acquire_exclusive(&FileLease::new(layout.home.join(".lease")), &scope).unwrap();
    let journal = SwapOnBackupRead {
        inner: FsJournal::new(layout.home.join(".journal"), Arc::new(RealFs::new())),
        swap: || {
            let x = layout.home.join(".claude/skills/x");
            if !x.is_symlink() {
                std::fs::remove_dir_all(&x).unwrap();
                std::os::unix::fs::symlink(layout.outside.join("x"), &x).unwrap();
            }
        },
    };
    let plan = PlanWriter::begin(
        &journal,
        &guard,
        PlanId("01PLANRECOVERYRACE00000001".to_string()),
        Utc::now(),
        "recovery race test",
        skills.clone(),
        Vec::new(),
    )
    .unwrap();
    plan.record_write_file(&skill_file, Some(b"old")).unwrap();
    drop(plan);

    let report = reconcile(&journal, &guard, &RealFs::new()).unwrap();

    assert_eq!(
        std::fs::read(layout.outside.join("x/SKILL.md")).unwrap(),
        b"user",
        "recovery wrote the backup through the link out of the plan root"
    );
    assert_eq!(report.interrupted.len(), 1, "{report:?}");
}
