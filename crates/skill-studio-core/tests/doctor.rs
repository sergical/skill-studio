// Integration test binaries aren't covered by the lib crate's
// `cfg_attr(test, allow(...))`: this file compiles as its own crate, so
// the same allow needs to be declared here too.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Unit 5.3: `ops::doctor` must run all six lifecycle invariants from
//! `docs/action-map/lifecycle-states.md`'s Invariants section as one
//! whole-scope pass - not one skill at a time like `ops::fix_skill` - and
//! report zero violations on a healthy home. One fixture per invariant,
//! named in the issue; each failure message names the invariant and the
//! offending path so a caller that cannot repair it automatically can
//! still tell the user where to look.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use skill_studio_core::doctor::DoctorInvariant;
use skill_studio_core::dto::DoctorRequest;
use skill_studio_core::harness::HarnessCatalog;
use skill_studio_core::identity::{
    PlanId, MOVE_ASIDE_DIR_NAME, PARKED_ROOT_RELATIVE, UNIVERSAL_ROOT_RELATIVE,
};
use skill_studio_core::journal::{FsJournal, PlanWriter};
use skill_studio_core::ops;
use skill_studio_core::ports::{
    ExclusiveGuard, Journal, LeaseMode, LeaseProvider, PlanStatus, Ports, Runtime, ScopeFs,
};
use skill_studio_core::testing::golden::{ctx, scope_for};
use skill_studio_core::testing::{FakeClock, FakeIds, FakeLease, FixtureBuilder, NoHistory};

const HOME: &str = "/home";

fn skill_md(name: &str) -> Vec<u8> {
    format!("---\nname: {name}\ndescription: fixture skill for the doctor pass.\n---\nBody.\n")
        .into_bytes()
}

fn runtime(fs: Arc<dyn ScopeFs>) -> Runtime {
    runtime_with_projects(fs, Vec::new())
}

fn runtime_with_projects(fs: Arc<dyn ScopeFs>, projects: Vec<std::path::PathBuf>) -> Runtime {
    let ports = Ports {
        fs,
        clock: Arc::new(FakeClock::at(0)),
        ids: Arc::new(FakeIds::default()),
        leases: Arc::new(FakeLease::default()),
        history: Arc::new(NoHistory),
        sink: Arc::new(skill_studio_core::testing::RecordingSink::default()),
        spawner: None,
        discovery: None,
        tools: None,
        catalog: Arc::new(HarnessCatalog::builtin()),

        telemetry: std::sync::Arc::new(skill_studio_core::ports::NoopTelemetry),
    };
    let mut scope = scope_for("doctor", Path::new(HOME));
    scope.projects = skill_studio_core::scope::ProjectSelection::Explicit { paths: projects };
    Runtime::new(&scope, ports).expect("runtime")
}

fn run_doctor(fs: Arc<dyn ScopeFs>) -> skill_studio_core::dto::DoctorReport {
    let rt = runtime(fs);
    ops::doctor(&rt, &ctx(), &DoctorRequest::default()).expect("doctor")
}

/// A home with one properly installed skill and nothing else: no dangling
/// link, no stale registry or lockfile row, no double state, quarantine
/// under its cap, no open journal plan.
fn healthy_home() -> Arc<dyn ScopeFs> {
    Arc::new(
        FixtureBuilder::new()
            .dir(&format!("{HOME}/{UNIVERSAL_ROOT_RELATIVE}/alpha"))
            .file(
                &format!("{HOME}/{UNIVERSAL_ROOT_RELATIVE}/alpha/SKILL.md"),
                &skill_md("alpha"),
            )
            .alias(
                &format!("{HOME}/.claude/skills/alpha"),
                &format!("../../{UNIVERSAL_ROOT_RELATIVE}/alpha"),
            )
            .build_fs(),
    )
}

#[test]
fn doctor_reports_zero_violations_on_a_healthy_fixture_home_or_names_the_false_positive() {
    let report = run_doctor(healthy_home());
    assert!(
        report.violations.is_empty(),
        "healthy home reported a false positive: {:?}",
        report.violations
    );
    assert_eq!(
        report.checked, 1,
        "expected the one skill in the fixture to be counted as checked"
    );
}

/// Invariant 1: a per-skill Claude Code symlink whose target does not
/// resolve inside the universal root it names - `scan`'s own
/// `ports::confine` (`lifecycle-states.md`'s "link confinement") refuses to
/// walk an out-of-scope target at all, so the observable "does not resolve
/// inside its root" case `diagnose`'s `BrokenLink` detection surfaces is a
/// dangling target, the same shape `fix_and_conflicts.rs`'s
/// `unrepairable_home` fixture uses; reused, not re-derived, by
/// `doctor::check_link_resolves_in_root` (see that function's own doc).
#[test]
fn doctor_finds_a_dangling_link_inside_its_root_or_names_the_check_that_missed_it() {
    let fs: Arc<dyn ScopeFs> = Arc::new(
        FixtureBuilder::new()
            .dir(&format!("{HOME}/{UNIVERSAL_ROOT_RELATIVE}"))
            .alias(
                &format!("{HOME}/.claude/skills/ghost"),
                &format!("../../{UNIVERSAL_ROOT_RELATIVE}/missing"),
            )
            .build_fs(),
    );

    let report = run_doctor(fs);
    let found = report
        .violations
        .iter()
        .find(|v| v.invariant == DoctorInvariant::LinkResolvesInRoot);
    assert!(
        found.is_some(),
        "check_link_resolves_in_root missed the escaping link: {:?}",
        report.violations
    );
}

/// Invariant 2: a registry `copies` entry names a path with no folder there.
#[test]
fn doctor_finds_a_registry_entry_with_no_folder_or_names_the_check_that_missed_it() {
    let expected_path = format!("{HOME}/{UNIVERSAL_ROOT_RELATIVE}/stale-copy");
    let registry = format!(
        r#"{{"copies":{{"stale-copy":{{"name":"stale-copy","path":"{expected_path}","scope":"global","destination":"universal"}}}}}}"#
    );
    let fs: Arc<dyn ScopeFs> = Arc::new(
        FixtureBuilder::new()
            .dir(&format!("{HOME}/{UNIVERSAL_ROOT_RELATIVE}"))
            .file(
                &format!("{HOME}/.agents/skill-studio.json"),
                registry.as_bytes(),
            )
            .build_fs(),
    );

    let report = run_doctor(fs);
    let found = report
        .violations
        .iter()
        .find(|v| v.invariant == DoctorInvariant::RegistryEntryHasFolder);
    assert!(
        found.is_some(),
        "check_registry_entry_has_folder missed the stale copy entry: {:?}",
        report.violations
    );
    assert_eq!(found.unwrap().path, Path::new(&expected_path));
}

/// Invariant 3: a `.skill-lock.json` entry names a skill with no folder
/// anywhere the scan covered.
#[test]
fn doctor_finds_a_lockfile_entry_with_no_folder_or_names_the_check_that_missed_it() {
    let fs: Arc<dyn ScopeFs> = Arc::new(
        FixtureBuilder::new()
            .dir(&format!("{HOME}/{UNIVERSAL_ROOT_RELATIVE}"))
            .file(
                &format!("{HOME}/.agents/.skill-lock.json"),
                br#"{"version":3,"skills":{
                    "ghost-skill":{"source":"o/r","sourceType":"github","sourceUrl":"https://example.com","skillFolderHash":"abc","installedAt":"2024-01-01T00:00:00Z","updatedAt":"2024-01-01T00:00:00Z"}
                }}"#,
            )
            .build_fs(),
    );

    let report = run_doctor(fs);
    let found = report
        .violations
        .iter()
        .find(|v| v.invariant == DoctorInvariant::LockfileEntryHasFolder);
    assert!(
        found.is_some(),
        "check_lockfile_entry_has_folder missed the ghost lockfile row: {:?}",
        report.violations
    );
}

/// Invariant 4: the same skill exists both live at the Universal root and
/// parked from it - the shape `park_skill`'s own lifecycle row says never
/// happens once a park lands cleanly.
#[test]
fn doctor_finds_a_folder_in_two_states_at_once_or_names_the_check_that_missed_it() {
    let name = "double-state";
    let fs: Arc<dyn ScopeFs> = Arc::new(
        FixtureBuilder::new()
            .file(
                &format!("{HOME}/{UNIVERSAL_ROOT_RELATIVE}/{name}/SKILL.md"),
                &skill_md(name),
            )
            .file(
                &format!("{HOME}/{PARKED_ROOT_RELATIVE}/{name}/SKILL.md"),
                &skill_md(name),
            )
            .build_fs(),
    );

    let report = run_doctor(fs);
    let found = report
        .violations
        .iter()
        .find(|v| v.invariant == DoctorInvariant::NoFolderInTwoStates);
    assert!(
        found.is_some(),
        "check_no_folder_in_two_states missed the double-parked skill: {:?}",
        report.violations
    );
}

/// Invariant 5: the quarantine holding directory has more entries than
/// [`skill_studio_core::doctor::QUARANTINE_RETENTION_CAP`].
#[test]
fn doctor_finds_a_quarantine_folder_past_its_retention_cap_or_names_the_check_that_missed_it() {
    let mut builder = FixtureBuilder::new().dir(&format!("{HOME}/{UNIVERSAL_ROOT_RELATIVE}"));
    let cap = skill_studio_core::doctor::QUARANTINE_RETENTION_CAP;
    for i in 0..=cap {
        builder = builder.file(
            &format!(
                "{HOME}/{UNIVERSAL_ROOT_RELATIVE}/.skill-studio-quarantine/{i:04}-quarantined/SKILL.md"
            ),
            &skill_md("quarantined"),
        );
    }
    let fs: Arc<dyn ScopeFs> = Arc::new(builder.build_fs());

    let report = run_doctor(fs);
    let found = report
        .violations
        .iter()
        .find(|v| v.invariant == DoctorInvariant::QuarantineWithinCap);
    assert!(
        found.is_some(),
        "check_quarantine_within_cap missed the over-cap quarantine dir: {:?}",
        report.violations
    );
}

/// Invariant 6: a journal plan begun but never finished - the case startup
/// reconcile never ran against, since nothing restarted the process here.
#[test]
fn doctor_finds_an_open_plan_at_rest_in_the_journal_or_names_the_check_that_missed_it() {
    let fs: Arc<dyn ScopeFs> = Arc::new(
        FixtureBuilder::new()
            .dir(&format!("{HOME}/{UNIVERSAL_ROOT_RELATIVE}"))
            .dir(&format!("{HOME}/.agents/skill-studio-journal"))
            .build_fs(),
    );
    let journal = FsJournal::new(
        Path::new(HOME).join(".agents").join("skill-studio-journal"),
        fs.clone(),
    );
    let lease = FakeLease::default();
    let handle = lease
        .acquire(&[], LeaseMode::Exclusive, Duration::from_secs(0))
        .unwrap();
    let guard = ExclusiveGuard::from_handle(handle);
    PlanWriter::begin(
        &journal,
        &guard,
        PlanId("01PLANDOCTORINTEGRATE0001".into()),
        Utc::now(),
        "doctor integration test plan",
        Path::new(HOME).join(UNIVERSAL_ROOT_RELATIVE),
        Vec::new(),
    )
    .unwrap();

    let report = run_doctor(fs);
    let found = report
        .violations
        .iter()
        .find(|v| v.invariant == DoctorInvariant::JournalHasNoOpenPlan);
    assert!(
        found.is_some(),
        "check_journal_has_no_open_plan missed the open plan: {:?}",
        report.violations
    );
}

/// Unit 2.7 (captured real homes) is deferred, so this stands in with the
/// largest fixture `FixtureBuilder` can express: every harness (Claude
/// Code, Codex, legacy and current `OpenCode`), a project root, a parked
/// skill, a quarantined entry, a disabled-per-harness (move-aside) skill,
/// a lockfile, and a journal with one finished plan. Follow-up: swap in
/// the 2.7 capture once it exists.
#[test]
fn doctor_completes_without_a_panic_on_a_real_home_directory_snapshot() {
    let mut b = FixtureBuilder::new()
        .dir(&format!("{HOME}/{UNIVERSAL_ROOT_RELATIVE}/gamma"))
        .file(
            &format!("{HOME}/{UNIVERSAL_ROOT_RELATIVE}/gamma/SKILL.md"),
            &skill_md("gamma"),
        )
        .alias(
            &format!("{HOME}/.claude/skills/gamma"),
            &format!("../../{UNIVERSAL_ROOT_RELATIVE}/gamma"),
        )
        .alias(
            &format!("{HOME}/.codex/skills/gamma"),
            &format!("../../{UNIVERSAL_ROOT_RELATIVE}/gamma"),
        )
        .dir(&format!("{HOME}/.config/opencode/skill/delta"))
        .file(
            &format!("{HOME}/.config/opencode/skill/delta/SKILL.md"),
            &skill_md("delta"),
        )
        .dir(&format!("{HOME}/.config/opencode/skills/epsilon"))
        .file(
            &format!("{HOME}/.config/opencode/skills/epsilon/SKILL.md"),
            &skill_md("epsilon"),
        )
        .dir(&format!("{HOME}/proj/.git"))
        .dir(&format!("{HOME}/proj/.claude/skills/eta"))
        .file(
            &format!("{HOME}/proj/.claude/skills/eta/SKILL.md"),
            &skill_md("eta"),
        )
        .dir(&format!("{HOME}/{PARKED_ROOT_RELATIVE}/zeta"))
        .file(
            &format!("{HOME}/{PARKED_ROOT_RELATIVE}/zeta/SKILL.md"),
            &skill_md("zeta"),
        )
        .dir(&format!(
            "{HOME}/.claude/skills/{MOVE_ASIDE_DIR_NAME}/kappa"
        ))
        .file(
            &format!("{HOME}/.claude/skills/{MOVE_ASIDE_DIR_NAME}/kappa/SKILL.md"),
            &skill_md("kappa"),
        )
        .file(
            &format!(
                "{HOME}/{UNIVERSAL_ROOT_RELATIVE}/.skill-studio-quarantine/0001-old/SKILL.md"
            ),
            &skill_md("old"),
        )
        .file(
            &format!("{HOME}/.agents/.skill-lock.json"),
            br#"{"version":3,"skills":{"gamma":{"source":"o/r","sourceType":"github","sourceUrl":"https://example.com","skillFolderHash":"abc","installedAt":"2024-01-01T00:00:00Z","updatedAt":"2024-01-01T00:00:00Z"}}}"#,
        )
        .dir(&format!("{HOME}/.agents/skill-studio-journal"));
    b = b.dir(&format!("{HOME}/.agents/skill-studio-journal"));
    let fs: Arc<dyn ScopeFs> = Arc::new(b.build_fs());

    let journal = FsJournal::new(
        Path::new(HOME).join(".agents").join("skill-studio-journal"),
        fs.clone(),
    );
    let lease = FakeLease::default();
    let handle = lease
        .acquire(&[], LeaseMode::Exclusive, Duration::from_secs(0))
        .unwrap();
    let guard = ExclusiveGuard::from_handle(handle);
    let plan = PlanWriter::begin(
        &journal,
        &guard,
        PlanId("01PLANDOCTORFINISHED00001".into()),
        Utc::now(),
        "finished plan, no longer open",
        Path::new(HOME).join(UNIVERSAL_ROOT_RELATIVE),
        Vec::new(),
    )
    .unwrap();
    plan.finish(PlanStatus::Done).unwrap();

    // Must not panic; a panic here fails the test on its own.
    let report = run_doctor(fs);
    assert!(
        report.checked > 0,
        "expected the largest fixture to count at least one skill checked"
    );
}

#[test]
fn doctor_checks_links_against_every_scope_or_mislabels_a_valid_development_link() {
    for (target, exists, severity) in [
        ("/home/src/dev-skill", true, None),
        ("/selected/dev-skill", true, None),
        ("/outside/dev-skill", true, Some("warning")),
        ("/home/src/missing", false, Some("error")),
    ] {
        let mut builder = FixtureBuilder::new()
            .dir(HOME)
            .dir("/selected")
            .alias("/home/.claude/skills/dev-skill", target);
        if exists {
            builder = builder.file(&format!("{target}/SKILL.md"), &skill_md("dev-skill"));
        }
        let rt = runtime_with_projects(Arc::new(builder.build_fs()), vec!["/selected".into()]);
        let report = ops::doctor(&rt, &ctx(), &DoctorRequest {}).unwrap();
        let links: Vec<_> = report
            .violations
            .iter()
            .filter(|v| v.invariant == DoctorInvariant::LinkResolvesInRoot)
            .collect();
        match severity {
            None => assert!(links.is_empty(), "valid target {target}: {links:?}"),
            Some(expected) => {
                assert_eq!(links.len(), 1, "target {target}: {links:?}");
                assert_eq!(
                    serde_json::to_value(links[0]).unwrap()["severity"],
                    expected,
                    "target {target}"
                );
            }
        }
    }
}

#[test]
fn doctor_reports_a_corrupt_registry_or_calls_unreadable_state_healthy() {
    let fs = FixtureBuilder::new()
        .dir(HOME)
        .file("/home/.agents/skill-studio.json", b"{broken")
        .build_fs();
    let report = run_doctor(Arc::new(fs));
    assert!(
        report
            .violations
            .iter()
            .any(|v| v.invariant == DoctorInvariant::RegistryEntryHasFolder
                && v.detail.contains("parse")),
        "{report:?}"
    );
}

#[test]
fn doctor_reports_a_corrupt_journal_or_calls_unreadable_plans_empty() {
    let fs = FixtureBuilder::new()
        .dir(HOME)
        .file(
            "/home/.agents/skill-studio-journal/plans/broken/plan.json",
            b"{broken",
        )
        .build_fs();
    let rt = runtime(Arc::new(fs));
    let root = rt.scope.home.lexical.join(".agents/skill-studio-journal");
    let journal = FsJournal::new(root, rt.ports.fs.clone());
    assert!(
        journal.all().is_err(),
        "fixture must contain an unreadable plan"
    );
    let report = ops::doctor(&rt, &ctx(), &DoctorRequest {}).unwrap();
    assert!(
        report
            .violations
            .iter()
            .any(|v| v.invariant == DoctorInvariant::JournalHasNoOpenPlan
                && v.detail.contains("read")),
        "{report:?}"
    );
}

#[test]
fn doctor_warns_for_old_unreferenced_entries_and_preserves_active_parked_and_fork_bases_or_deletes_data(
) {
    use skill_studio_core::ports::Clock;
    let clock = Arc::new(FakeClock::at(90 * 24 * 60 * 60 * 1000));
    let now = clock.now();
    let old =
        now - chrono::Duration::days(skill_studio_core::doctor::RETAINED_ENTRY_MAX_AGE_DAYS + 1);
    let young = now - chrono::Duration::days(1);
    let trash = "/home/.agents/skills-trash/old";
    let fresh = "/home/.agents/skills-trash/fresh";
    let quarantine = "/home/.agents/skills/.skill-studio-quarantine/old";
    let parked = "/home/.agents/skills-parked/held";
    let fork = "/home/.history/data/skill-studio/forks/forked/base";
    let orphan = "/home/.history/data/skill-studio/forks/orphan/base";
    let registry = serde_json::json!({
        "parked": {"held":{"skill_dir":parked,"parked_at":old}},
        "forks": {"forked":{"skill_dir":"/home/.agents/skills/forked","repo":"o/r","path":"skill","base_commit":"commit"}},
        "trials": {
            "old":{"backup_path":trash,"created_at":old},
            "fresh":{"backup_path":fresh,"created_at":young},
            "quarantine":{"backup_path":quarantine,"created_at":old},
            "parked":{"backup_path":parked,"created_at":old},
            "fork":{"backup_path":fork,"created_at":old},
            "orphan":{"backup_path":orphan,"created_at":old}
        }
    });
    let mut builder = FixtureBuilder::new()
        .file(
            "/home/.agents/skill-studio.json",
            &serde_json::to_vec(&registry).unwrap(),
        )
        .file("/home/.agents/skills/forked/SKILL.md", &skill_md("forked"));
    for (path, name) in [
        (trash, "old"),
        (fresh, "fresh"),
        (quarantine, "quarantined"),
        (parked, "held"),
        (fork, "forked"),
        (orphan, "orphan"),
    ] {
        builder = builder.file(&format!("{path}/SKILL.md"), &skill_md(name));
    }
    let fs: Arc<dyn ScopeFs> = Arc::new(builder.build_fs());
    let mut rt = runtime(fs.clone());
    rt.ports.clock = clock;
    let report = ops::doctor(&rt, &ctx(), &DoctorRequest {}).unwrap();
    let old_paths: std::collections::BTreeSet<_> = report
        .violations
        .iter()
        .filter(|v| v.invariant == DoctorInvariant::QuarantineWithinCap)
        .map(|v| v.path.to_string_lossy().into_owned())
        .collect();
    let expected: std::collections::BTreeSet<_> = [trash, quarantine, orphan]
        .into_iter()
        .map(str::to_string)
        .collect();
    assert_eq!(old_paths, expected, "{report:?}");
    assert!(report
        .violations
        .iter()
        .filter(|v| v.invariant == DoctorInvariant::QuarantineWithinCap)
        .all(|v| v.severity == skill_studio_core::dto::Severity::Warning));
    for path in [trash, fresh, quarantine, parked, fork, orphan] {
        assert!(
            fs.symlink_metadata(Path::new(path)).is_ok(),
            "doctor deleted {path}"
        );
    }
}

#[test]
fn doctor_finds_a_parked_skill_also_retained_in_trash_or_hides_the_second_state() {
    let fs = FixtureBuilder::new()
        .file(
            "/home/.agents/skills-parked/alpha/SKILL.md",
            &skill_md("alpha"),
        )
        .file(
            "/home/.agents/skills-trash/alpha/SKILL.md",
            &skill_md("alpha"),
        )
        .build_fs();
    let report = run_doctor(Arc::new(fs));
    assert!(
        report
            .violations
            .iter()
            .any(|v| v.invariant == DoctorInvariant::NoFolderInTwoStates
                && v.path == Path::new("/home/.agents/skills-trash/alpha")),
        "{report:?}"
    );
}

#[test]
fn doctor_checks_every_harness_and_the_shared_root_without_warning_on_healthy_skills_or_skips_a_harness(
) {
    use skill_studio_core::harness::{RootRole, ScopeLevel};
    let catalog = HarnessCatalog::builtin();
    let mut builder =
        FixtureBuilder::new().file("/home/.agents/skills/shared/SKILL.md", &skill_md("shared"));
    let mut expected = std::collections::BTreeSet::from(["shared".to_string()]);
    for facts in &catalog.facts {
        let root = facts
            .roots
            .iter()
            .find(|root| root.role == RootRole::Own && root.level == ScopeLevel::Global)
            .unwrap();
        let name = facts.id.as_str();
        builder = builder.file(
            &format!("{HOME}/{}/{name}/SKILL.md", root.relative_path),
            &skill_md(name),
        );
        expected.insert(name.to_string());
    }
    let rt = runtime(Arc::new(builder.build_fs()));
    let scanned = ops::scan(&rt, &ctx(), &Default::default()).unwrap();
    let found: std::collections::BTreeSet<_> = scanned
        .skills
        .iter()
        .map(|skill| skill.name.0.clone())
        .collect();
    assert_eq!(found, expected);
    let report = ops::doctor(&rt, &ctx(), &DoctorRequest {}).unwrap();
    assert!(report.violations.is_empty(), "{report:?}");
    assert_eq!(report.checked as usize, expected.len());
}

#[test]
fn doctor_warns_for_old_trash_in_a_selected_project_or_checks_only_the_home() {
    use skill_studio_core::ports::Clock;
    let clock = Arc::new(FakeClock::at(90 * 24 * 60 * 60 * 1000));
    let created = clock.now()
        - chrono::Duration::days(skill_studio_core::doctor::RETAINED_ENTRY_MAX_AGE_DAYS + 1);
    let path = "/projects/work/.agents/skills-trash/old";
    let registry = serde_json::json!({"trials":{"old":{"backup_path":path,"created_at":created}}});
    let fs: Arc<dyn ScopeFs> = Arc::new(
        FixtureBuilder::new()
            .file(
                "/home/.agents/skill-studio.json",
                &serde_json::to_vec(&registry).unwrap(),
            )
            .file(&format!("{path}/SKILL.md"), &skill_md("old"))
            .build_fs(),
    );
    let mut rt = runtime_with_projects(fs.clone(), vec!["/projects/work".into()]);
    rt.ports.clock = clock;
    let report = ops::doctor(&rt, &ctx(), &DoctorRequest {}).unwrap();
    assert!(
        report
            .violations
            .iter()
            .any(|row| row.path == Path::new(path)
                && row.severity == skill_studio_core::dto::Severity::Warning),
        "{report:?}"
    );
    assert!(fs.symlink_metadata(Path::new(path)).is_ok());
}

#[test]
fn doctor_retention_warns_only_after_exact_thirty_day_boundary_or_flags_boundary_as_old() {
    let now = chrono::DateTime::from_timestamp(10_000_000, 0).unwrap();
    for (age, expected) in [
        (chrono::Duration::days(30), false),
        (
            chrono::Duration::days(30) + chrono::Duration::nanoseconds(1),
            true,
        ),
    ] {
        let path = "/home/.agents/skills-trash/old";
        let fs = FixtureBuilder::new().dir(path).file("/home/.agents/skill-studio.json", &serde_json::to_vec(&serde_json::json!({"retained": {"to":path,"created_at":(now-age).to_rfc3339()}})).unwrap()).build_fs();
        let mut rt = runtime(Arc::new(fs));
        rt.ports.clock = Arc::new(FakeClock::at(now.timestamp_millis().try_into().unwrap()));
        let report = ops::doctor(&rt, &ctx(), &DoctorRequest::default()).unwrap();
        assert_eq!(
            report
                .violations
                .iter()
                .any(|v| v.path == Path::new(path) && v.detail.contains("older than")),
            expected,
            "{report:?}"
        );
    }
}

#[test]
fn doctor_retained_symlink_loop_stays_unfollowed_or_hangs_and_reads_target_children() {
    let root = "/home/.agents/skills-parked";
    let fs = FixtureBuilder::new()
        .dir(root)
        .alias(&format!("{root}/loop"), root)
        .build_fs();
    let mut rt = runtime(Arc::new(fs));
    rt.ports.clock = Arc::new(FakeClock::at(10_000_000_000));
    let report = ops::doctor(&rt, &ctx(), &DoctorRequest::default()).unwrap();
    assert!(
        !report
            .violations
            .iter()
            .any(|v| v.path.to_string_lossy().contains("loop/")),
        "{report:?}"
    );
    assert!(
        rt.ports
            .fs
            .symlink_metadata(Path::new(&format!("{root}/loop")))
            .unwrap()
            .kind
            == skill_studio_core::ports::FileKind::Symlink
    );
}

#[test]
fn doctor_without_retained_candidates_skips_retention_history_or_reads_unneeded_payloads() {
    struct CountOpens(std::sync::atomic::AtomicUsize);
    impl skill_studio_core::ports::HistoryOpener for CountOpens {
        fn open(
            &self,
            _: &skill_studio_core::scope::NormalizedScope,
            _: skill_studio_core::ports::HistoryAccess,
        ) -> Result<
            Option<Box<dyn skill_studio_core::ports::HistoryStore>>,
            skill_studio_core::error::CoreError,
        > {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(None)
        }
    }
    let history = Arc::new(CountOpens(std::sync::atomic::AtomicUsize::new(0)));
    let mut rt = runtime(healthy_home());
    rt.ports.history = history.clone();
    ops::doctor(&rt, &ctx(), &DoctorRequest::default()).unwrap();
    assert_eq!(
        history.0.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "only pending-event detection needs history when no retained candidates exist"
    );
}

#[test]
fn doctor_retained_linked_roots_and_ancestors_report_problem_or_walk_external_tree() {
    for linked in [
        "/home/.agents/skills-parked",
        "/home/.agents",
        "/data/forks",
        "/data/skill-studio",
    ] {
        let fs = FixtureBuilder::new()
            .dir("/home")
            .dir("/external")
            .file(
                "/external/old/SKILL.md",
                b"---\nname: old\ndescription: old\n---\n",
            )
            .alias(linked, "/external")
            .build_fs();
        let mut rt = runtime(Arc::new(fs));
        rt.scope.data_root = Some("/data".into());
        rt.ports.clock = Arc::new(FakeClock::at(10_000_000_000));
        let report = ops::doctor(&rt, &ctx(), &DoctorRequest::default()).unwrap();
        assert!(
            report
                .violations
                .iter()
                .any(|v| v.path == Path::new(linked) && v.detail.contains("linked ancestor")),
            "{report:?}"
        );
        assert!(
            !report
                .violations
                .iter()
                .any(|v| v.detail.contains("retained entry created")),
            "{report:?}"
        );
    }
}

#[test]
fn doctor_finds_desktop_fork_storage_or_checks_only_shared_xdg_forks() {
    let home = Path::new(HOME);
    let data = Path::new("/data/skill-studio");
    let base = skill_studio_core::scope::desktop_fork_storage_root(home, data).join("alpha/base");
    let fs = FixtureBuilder::new()
        .dir(HOME)
        .file(
            base.join("SKILL.md").to_str().unwrap(),
            b"---\nname: alpha\ndescription: alpha\n---\n",
        )
        .file("/home/.agents/skill-studio.json", &serde_json::to_vec(&serde_json::json!({"retained": {"base_dir": base, "created_at": "2000-01-01T00:00:00Z"}})).unwrap())
        .build_fs();
    let mut rt = runtime(Arc::new(fs));
    rt.scope.data_root = Some(data.into());
    rt.ports.clock = Arc::new(FakeClock::at(2_000_000_000_000));
    let report = ops::doctor(&rt, &ctx(), &DoctorRequest::default()).unwrap();
    assert!(
        report
            .violations
            .iter()
            .any(|v| v.path == base && v.detail.contains("older than 30 days")),
        "{report:?}"
    );
}
