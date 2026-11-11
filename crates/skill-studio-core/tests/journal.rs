// Integration test binaries aren't covered by the lib crate's
// `cfg_attr(test, allow(...))`: this file compiles as its own crate, so
// the same allow needs to be declared here too.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Unit 1.2: the integration test proving every one of `fsops`'s four
//! primitives records a journal entry itself, since Section B moved the
//! recording into the primitives (no more `journaled_*` wrappers). The five
//! lower-level journal tests live inline in `skill_studio_core::journal`'s
//! own test module.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use skill_studio_core::fsops::{self, read_stamp, Root};
use skill_studio_core::identity::PlanId;
use skill_studio_core::journal::{FsJournal, PlanWriter};
use skill_studio_core::ports::{
    ExclusiveGuard, Journal, LeaseMode, LeaseProvider, PlanStatus, PlanStep, ScopeFs,
};
use skill_studio_core::testing::{FakeLease, FixtureBuilder};

fn guard(lease: &FakeLease) -> ExclusiveGuard {
    let handle = lease
        .acquire(
            &[skill_studio_core::ports::LeaseKey {
                canonical_root: PathBuf::from("/root"),
            }],
            LeaseMode::Exclusive,
            Duration::ZERO,
        )
        .expect("acquire exclusive lease");
    ExclusiveGuard::from_handle(handle)
}

/// Given a plan, when each of `fsops`'s four primitives is called, then the
/// plan's recorded steps name all four, in the order they ran; on failure the
/// panic names whichever primitive's call left no matching step.
#[test]
fn every_fsops_call_records_a_journal_entry_or_names_the_unjournaled_write() {
    let fs: Arc<dyn ScopeFs> = Arc::new(
        FixtureBuilder::new()
            .dir("/root")
            .dir("/journal")
            .build_fs(),
    );
    let root_path = PathBuf::from("/root");
    let root = Root::open(fs.as_ref(), root_path.clone()).expect("open root");
    let journal = FsJournal::new(PathBuf::from("/journal"), fs.clone());
    let lease = FakeLease::default();
    let g = guard(&lease);

    let plan = PlanWriter::begin(
        &journal,
        &g,
        PlanId("01PLANFSOPS000000000001".into()),
        Utc::now(),
        "exercise every fsops primitive",
        root_path.clone(),
        Vec::new(),
    )
    .expect("begin");

    let staged = fsops::stage(
        &root,
        &plan,
        &[(PathBuf::from("SKILL.md"), b"hello".to_vec())],
    )
    .expect("stage");
    fsops::swap(
        &root,
        &plan,
        Path::new("alpha"),
        &staged,
        Path::new(".trash"),
    )
    .expect("swap");
    fsops::link(&root, &plan, Path::new("alpha-link"), Path::new("alpha")).expect("link");
    let target = root_path.join("alpha").join("SKILL.md");
    let stamp = read_stamp(fs.as_ref(), &target).expect("read stamp");
    fsops::write_file(
        &root,
        &plan,
        Path::new("alpha/SKILL.md"),
        b"updated",
        &stamp,
    )
    .expect("write_file");

    let id = plan.id().clone();
    plan.finish(PlanStatus::Done).expect("finish");

    let record = journal
        .all()
        .expect("read plans back")
        .into_iter()
        .find(|p| p.id == id)
        .expect("the plan begun above");

    let names: Vec<&str> = record.steps.iter().map(PlanStep::primitive_name).collect();
    for expected in ["stage", "swap", "link", "write_file"] {
        assert!(
            names.contains(&expected),
            "no journal entry recorded for {expected}; recorded steps were {names:?}"
        );
    }
    assert_eq!(
        names,
        vec!["stage", "swap", "link", "write_file"],
        "steps must be recorded in the order the primitives ran"
    );
}

#[cfg(unix)]
fn real_journal_fixture() -> (
    tempfile::TempDir,
    Arc<skill_studio_host::RealFs>,
    FsJournal,
    ExclusiveGuard,
) {
    let directory = tempfile::tempdir().unwrap();
    let home = directory.path().join("home");
    std::fs::create_dir(&home).unwrap();
    let fs = Arc::new(skill_studio_host::RealFs::new());
    let journal = FsJournal::new(directory.path().join("journal"), fs.clone());
    let lease = FakeLease::default();
    let handle = lease
        .acquire(
            &[skill_studio_core::ports::LeaseKey {
                canonical_root: home.canonicalize().unwrap(),
            }],
            LeaseMode::Exclusive,
            Duration::ZERO,
        )
        .unwrap();
    (directory, fs, journal, ExclusiveGuard::from_handle(handle))
}

#[cfg(unix)]
#[test]
fn journal_recovery_keeps_a_private_files_mode_or_restores_it_world_readable() {
    use std::os::unix::fs::PermissionsExt as _;
    let (directory, fs, journal, guard) = real_journal_fixture();
    let home = directory.path().join("home");
    let path = home.join("private.txt");
    std::fs::write(&path, b"before").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let before_mode = std::fs::metadata(&path).unwrap().permissions().mode();
    let root = Root::open(fs.as_ref(), home.clone()).unwrap();
    let plan = PlanWriter::begin(
        &journal,
        &guard,
        PlanId("private-mode".into()),
        Utc::now(),
        "private file",
        home,
        Vec::new(),
    )
    .unwrap();
    fsops::write_file(
        &root,
        &plan,
        Path::new("private.txt"),
        b"after",
        &read_stamp(fs.as_ref(), &path).unwrap(),
    )
    .unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode(),
        before_mode,
        "writing must keep the original mode"
    );
    drop(plan);
    skill_studio_core::journal::reconcile(&journal, &guard, fs.as_ref()).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"before");
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode(),
        before_mode
    );
}

#[cfg(unix)]
#[test]
fn journal_recovery_recreates_a_raw_relative_link_or_returns_a_plain_file() {
    let (directory, fs, journal, guard) = real_journal_fixture();
    let home = directory.path().join("home");
    let path = home.join("link");
    std::fs::write(home.join("original.txt"), b"before").unwrap();
    std::os::unix::fs::symlink("./original.txt", &path).unwrap();
    let before_target = std::fs::read_link(&path).unwrap();
    let root = Root::open(fs.as_ref(), home.clone()).unwrap();
    let plan = PlanWriter::begin(
        &journal,
        &guard,
        PlanId("raw-link".into()),
        Utc::now(),
        "replace link",
        home.clone(),
        Vec::new(),
    )
    .unwrap();
    fsops::write_file(
        &root,
        &plan,
        Path::new("link"),
        b"after",
        &read_stamp(fs.as_ref(), &path).unwrap(),
    )
    .unwrap();
    assert!(!std::fs::symlink_metadata(&path).unwrap().is_symlink());
    drop(plan);
    for _ in 0..2 {
        skill_studio_core::journal::reconcile(&journal, &guard, fs.as_ref()).unwrap();
    }
    assert_eq!(std::fs::read_link(&path).unwrap(), before_target);
    assert_eq!(std::fs::read(home.join("original.txt")).unwrap(), b"before");
}
