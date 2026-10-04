// Integration test binaries aren't covered by the lib crate's
// `cfg_attr(test, allow(...))`: this file compiles as its own crate, so
// the same allow needs to be declared here too.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr
)]

//! Real-disk tests for `ops::discard`, the fix behind "parked copy left
//! behind" (#388). Each test names the flow, what must hold, and what a
//! failure means.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use skill_studio_core::dto::{
    DeploymentDto, DiscardRequest, ParkRequest, RestoreRequest, ScanRequest,
};
use skill_studio_core::harness::HarnessCatalog;
use skill_studio_core::identity::RootKind;
use skill_studio_core::ops;
use skill_studio_core::ports::{Ports, ProcessSpawner, Runtime, ScopeFs};
use skill_studio_core::scope::{ProjectSelection, RuntimeScope};
use skill_studio_core::testing::golden::{ctx, unique_temp_dir};
use skill_studio_core::testing::{FakeClock, FakeIds, RecordingSink};

use skill_studio_host::{FileLease, RealFs, RealProcessSpawner, SqliteHistoryOpener};

fn write_skill(dir: &Path, name: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: a parkable skill\n---\nBody.\n"),
    )
    .unwrap();
}

fn runtime(home: &Path, projects: &[PathBuf]) -> Runtime {
    runtime_with(
        home,
        projects,
        Arc::new(RealFs::new()),
        Arc::new(RealProcessSpawner::new()),
    )
}

fn runtime_with(
    home: &Path,
    projects: &[PathBuf],
    fs: Arc<dyn ScopeFs>,
    spawner: Arc<dyn ProcessSpawner>,
) -> Runtime {
    let mut scope = RuntimeScope::fixture(home);
    scope.projects = ProjectSelection::Explicit {
        paths: projects.to_vec(),
    };
    let ports = Ports {
        fs,
        clock: Arc::new(FakeClock::at(0)),
        ids: Arc::new(FakeIds::default()),
        leases: Arc::new(FileLease::new(home.join(".leases"))),
        history: Arc::new(SqliteHistoryOpener::new(
            home.join(".history").join("events.sqlite3"),
        )),
        sink: Arc::new(RecordingSink::default()),
        spawner: Some(spawner),
        discovery: None,
        tools: None,
        catalog: Arc::new(HarnessCatalog::builtin()),
        telemetry: Arc::new(skill_studio_core::ports::NoopTelemetry),
    };
    Runtime::new(&scope, ports).unwrap()
}

fn deployments(rt: &Runtime, name: &str) -> Vec<DeploymentDto> {
    let inventory = ops::scan(rt, &ctx(), &ScanRequest::default()).unwrap();
    inventory
        .skills
        .into_iter()
        .filter(|skill| skill.name.0 == name)
        .flat_map(|skill| skill.deployments)
        .collect()
}

fn copy_with(rt: &Runtime, name: &str, parked: bool) -> DeploymentDto {
    deployments(rt, name)
        .into_iter()
        .find(|d| (d.root.kind == RootKind::Parked) == parked)
        .unwrap_or_else(|| panic!("no copy of {name} with parked = {parked}"))
}

/// Parks the copy at `live`, then puts a fresh one back: the state a hand
/// `mv` or an install leaves. Returns the parked folder.
fn leave_a_parked_copy_behind(rt: &Runtime, name: &str, live: &Path) -> PathBuf {
    let parked = ops::park(
        rt,
        &ctx(),
        &ParkRequest {
            deployment_id: copy_with(rt, name, false).id,
        },
    )
    .unwrap()
    .parked_path;
    write_skill(live, name);
    parked
}

/// Flow: a skill is live and parked at the same origin; discard the parked
/// copy ("Keep live"). Expectation: the parked folder is gone and the live
/// one is untouched. Failure: the live copy is deleted instead, or the
/// parked folder stays and the issue never clears.
#[test]
fn discarding_the_parked_copy_removes_it_and_keeps_the_live_copy() {
    let home = unique_temp_dir("discard_parked");
    let live = home.join(".agents/skills/foo");
    write_skill(&live, "foo");
    let rt = runtime(&home, &[]);
    let parked = leave_a_parked_copy_behind(&rt, "foo", &live);

    ops::discard(
        &rt,
        &ctx(),
        &DiscardRequest {
            deployment_id: copy_with(&rt, "foo", true).id,
        },
    )
    .unwrap();

    assert!(!parked.exists(), "the parked copy must be gone");
    assert!(live.join("SKILL.md").exists(), "the live copy must stay");
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: same state; discard the live copy ("Keep parked"). Expectation: the
/// live folder is gone and the parked copy is untouched. Failure: the parked
/// copy, the only backup of the skill, is lost.
#[test]
fn discarding_the_live_copy_removes_it_and_keeps_the_parked_copy() {
    let home = unique_temp_dir("discard_live");
    let live = home.join(".agents/skills/foo");
    write_skill(&live, "foo");
    let rt = runtime(&home, &[]);
    let parked = leave_a_parked_copy_behind(&rt, "foo", &live);

    ops::discard(
        &rt,
        &ctx(),
        &DiscardRequest {
            deployment_id: copy_with(&rt, "foo", false).id,
        },
    )
    .unwrap();

    assert!(!live.exists(), "the live copy must be gone");
    assert!(
        parked.join("SKILL.md").exists(),
        "the parked copy must stay"
    );
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: discard a parked copy, then undo the event from Activity.
/// Expectation: the parked folder comes back with its files. Failure: the
/// delete is journaled without a backup, so a wrong click cannot be undone.
#[test]
fn undoing_a_discard_brings_the_folder_back_with_its_files() {
    let home = unique_temp_dir("discard_undo");
    let live = home.join(".agents/skills/foo");
    write_skill(&live, "foo");
    let rt = runtime(&home, &[]);
    let parked = leave_a_parked_copy_behind(&rt, "foo", &live);
    let outcome = ops::discard(
        &rt,
        &ctx(),
        &DiscardRequest {
            deployment_id: copy_with(&rt, "foo", true).id,
        },
    )
    .unwrap();

    ops::restore_event(
        &rt,
        &ctx(),
        &RestoreRequest {
            event_id: outcome.event_id,
            force: false,
        },
    )
    .unwrap();

    assert!(parked.join("SKILL.md").exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: discard a copy a Claude Code plugin ships. Expectation: refused with
/// the `/plugin` hint and the files stay. Failure: files leave the plugin cache.
#[test]
fn discard_refuses_a_plugin_copy_and_points_at_the_plugin_command() {
    let home = unique_temp_dir("discard_plugin");
    let root = home.join(".claude/plugins/cache/vendor-1/plugin-1/1.0.0");
    std::fs::create_dir_all(root.join(".claude-plugin")).unwrap();
    std::fs::write(
        root.join(".claude-plugin/plugin.json"),
        br#"{"name":"plugin-1","version":"1.0.0"}"#,
    )
    .unwrap();
    let shipped = root.join("skills/shipped");
    write_skill(&shipped, "shipped");
    let rt = runtime(&home, &[]);
    let copy = deployments(&rt, "shipped").into_iter().next().unwrap();

    let err = ops::discard(
        &rt,
        &ctx(),
        &DiscardRequest {
            deployment_id: copy.id,
        },
    )
    .unwrap_err();

    assert!(err.message.contains("/plugin"), "{}", err.message);
    assert!(shipped.join("SKILL.md").exists());
    std::fs::remove_dir_all(&home).ok();
}
