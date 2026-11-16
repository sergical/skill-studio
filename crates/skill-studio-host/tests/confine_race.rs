#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! A folder under a scope root that another tool swaps for a link to a
//! folder outside the scope, after the path check and before the write.
//! The write must fail, and the outside folder must stay empty.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use skill_studio_core::fsops::{self, Root};
use skill_studio_core::identity::PlanId;
use skill_studio_core::journal::{FsJournal, PlanWriter};
use skill_studio_core::ports::{
    acquire_exclusive, confine, ExclusiveGuard, LeaseMode, LeaseProvider, ScopeFs, ScopedReads,
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
