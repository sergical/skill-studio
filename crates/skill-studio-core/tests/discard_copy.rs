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
    DeploymentDto, DiscardRequest, ListEventsRequest, ParkRequest, RestoreRequest, ScanRequest,
};
use skill_studio_core::harness::HarnessCatalog;
use skill_studio_core::identity::RootKind;
use skill_studio_core::ops;
use skill_studio_core::ports::{Ports, ProcessSpawner, Runtime, ScopeFs};
use skill_studio_core::scope::{ProjectSelection, RuntimeScope};
use skill_studio_core::testing::golden::{ctx, unique_temp_dir};
use skill_studio_core::testing::{FailingFs, FakeClock, FakeIds, RecordingSink};

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

/// Discards the parked copy (`doomed_parked`) or the live one, keeping the other.
fn discard_request(rt: &Runtime, name: &str, doomed_parked: bool) -> DiscardRequest {
    DiscardRequest {
        deployment_id: copy_with(rt, name, doomed_parked).id,
        keep_deployment_id: copy_with(rt, name, !doomed_parked).id,
    }
}

fn failing_runtime(home: &Path) -> (Runtime, Arc<FailingFs>) {
    let failing = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
    let rt = runtime_with(
        home,
        &[],
        failing.clone(),
        Arc::new(RealProcessSpawner::new()),
    );
    (rt, failing)
}

fn forks_registry(home: &Path) -> serde_json::Value {
    std::fs::read(home.join(".agents/skill-studio.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
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

    ops::discard(&rt, &ctx(), &discard_request(&rt, "foo", true)).unwrap();

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

    ops::discard(&rt, &ctx(), &discard_request(&rt, "foo", false)).unwrap();

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
    let outcome = ops::discard(&rt, &ctx(), &discard_request(&rt, "foo", true)).unwrap();

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
            deployment_id: copy.id.clone(),
            keep_deployment_id: copy.id,
        },
    )
    .unwrap_err();

    assert!(err.message.contains("/plugin"), "{}", err.message);
    assert!(shipped.join("SKILL.md").exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: a Universal entry that is a link to a folder elsewhere; discard it.
/// Expectation: refused, and the link target is untouched. Failure: the delete
/// follows the link and removes the real folder.
#[test]
fn discard_refuses_a_universal_entry_that_is_a_link() {
    let home = unique_temp_dir("discard_universal_link");
    let target = home.join("elsewhere/foo");
    write_skill(&target, "foo");
    let entry = home.join(".agents/skills/foo");
    std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&target, &entry).unwrap();
    let rt = runtime(&home, &[]);
    let copy = deployments(&rt, "foo")
        .into_iter()
        .find(|d| d.path == entry)
        .unwrap();

    let err = ops::discard(
        &rt,
        &ctx(),
        &DiscardRequest {
            deployment_id: copy.id.clone(),
            keep_deployment_id: copy.id,
        },
    )
    .unwrap_err();

    assert!(err.message.contains("link"), "{}", err.message);
    assert!(
        target.join("SKILL.md").exists(),
        "the link target must stay"
    );
    assert!(entry.is_symlink());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: an agent folder entry that is a link to the Universal copy; discard
/// it. Expectation: refused, the Universal folder is untouched.
#[test]
fn discard_refuses_an_agent_folder_link() {
    let home = unique_temp_dir("discard_agent_link");
    let real = home.join(".agents/skills/foo");
    write_skill(&real, "foo");
    let entry = home.join(".codex/skills/foo");
    std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&real, &entry).unwrap();
    let rt = runtime(&home, &[]);
    let copy = deployments(&rt, "foo")
        .into_iter()
        .find(|d| d.path == entry)
        .unwrap();

    let err = ops::discard(
        &rt,
        &ctx(),
        &DiscardRequest {
            deployment_id: copy.id.clone(),
            keep_deployment_id: copy.id,
        },
    )
    .unwrap_err();

    assert!(err.message.contains("link"), "{}", err.message);
    assert!(real.join("SKILL.md").exists());
    assert!(entry.is_symlink());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: the copy to keep is deleted after the snapshot; discard the other.
/// Expectation: refused, the other copy is still there. Failure: both copies
/// are gone.
#[test]
fn discard_refuses_when_the_kept_copy_is_gone() {
    let home = unique_temp_dir("discard_kept_gone");
    let live = home.join(".agents/skills/foo");
    write_skill(&live, "foo");
    let rt = runtime(&home, &[]);
    let parked = leave_a_parked_copy_behind(&rt, "foo", &live);
    let req = discard_request(&rt, "foo", false);
    std::fs::remove_dir_all(&parked).unwrap();

    let err = ops::discard(&rt, &ctx(), &req).unwrap_err();

    assert!(!err.message.is_empty());
    assert!(live.join("SKILL.md").exists(), "the live copy must stay");
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: discard a live copy a skills.sh or dotagents ledger owns.
/// Expectation: refused with the installer hint, files stay. Failure: the
/// ledger keeps a row for a folder that is gone.
#[test]
fn discard_refuses_a_live_copy_an_installer_owns() {
    for owner in ["skills.sh", "dotagents"] {
        let home = unique_temp_dir("discard_managed");
        let live = home.join(".agents/skills/foo");
        write_skill(&live, "foo");
        let rt = runtime(&home, &[]);
        let parked = leave_a_parked_copy_behind(&rt, "foo", &live);
        let agents = home.join(".agents");
        if owner == "skills.sh" {
            std::fs::write(
                agents.join(".skill-lock.json"),
                serde_json::json!({"version": 3, "skills": {"foo": {
                    "source": "owner/foo", "sourceType": "github",
                    "sourceUrl": "https://github.com/owner/foo",
                    "skillFolderHash": "deadbeef",
                    "installedAt": "2024-01-01T00:00:00Z",
                    "updatedAt": "2024-01-01T00:00:00Z",
                }}})
                .to_string(),
            )
            .unwrap();
        } else {
            std::fs::write(
                agents.join("agents.lock"),
                "[skills.foo]\nsource = \"owner/foo\"\n",
            )
            .unwrap();
            std::fs::write(agents.join("agents.toml"), "[[skills]]\nname = \"foo\"\n").unwrap();
        }

        let err = ops::discard(&rt, &ctx(), &discard_request(&rt, "foo", false)).unwrap_err();

        assert!(
            err.message.contains("installer"),
            "{owner}: {}",
            err.message
        );
        assert!(live.join("SKILL.md").exists());
        assert!(parked.join("SKILL.md").exists());
        std::fs::remove_dir_all(&home).ok();
    }
}

/// Flow: discard a Fork live copy, then undo. Expectation: the registry row
/// goes with the folder and comes back on undo. Failure: a stale fork row
/// outlives the folder.
#[test]
fn discarding_a_fork_drops_its_registry_row_and_undo_restores_it() {
    let home = unique_temp_dir("discard_fork");
    let live = home.join(".agents/skills/foo");
    write_skill(&live, "foo");
    let rt = runtime(&home, &[]);
    leave_a_parked_copy_behind(&rt, "foo", &live);
    std::fs::write(
        home.join(".agents/skill-studio.json"),
        serde_json::json!({"forks": {"foo": {}}}).to_string(),
    )
    .unwrap();

    let outcome = ops::discard(&rt, &ctx(), &discard_request(&rt, "foo", false)).unwrap();
    assert!(forks_registry(&home)["forks"].get("foo").is_none());

    ops::restore_event(
        &rt,
        &ctx(),
        &RestoreRequest {
            event_id: outcome.event_id,
            force: false,
        },
    )
    .unwrap();
    assert!(live.join("SKILL.md").exists());
    assert!(forks_registry(&home)["forks"].get("foo").is_some());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: a project holds a live and a parked copy; discard each in turn.
/// Expectation: only the doomed copy goes, in the project. Failure: the
/// project scope is confused with global.
#[test]
fn discard_works_for_a_project_scope_pair() {
    for doomed_parked in [true, false] {
        let home = unique_temp_dir("discard_project");
        let project = home.join("proj");
        let live = project.join(".agents/skills/foo");
        write_skill(&live, "foo");
        let rt = runtime(&home, std::slice::from_ref(&project));
        let parked = leave_a_parked_copy_behind(&rt, "foo", &live);

        ops::discard(&rt, &ctx(), &discard_request(&rt, "foo", doomed_parked)).unwrap();

        assert_eq!(live.exists(), doomed_parked);
        assert_eq!(parked.exists(), !doomed_parked);
        std::fs::remove_dir_all(&home).ok();
    }
}

/// Flow: the folder rename fails after the per-skill links went. Expectation:
/// the row is Failed, the folder is intact. Failure: a half delete is
/// reported Done.
#[test]
fn a_failed_delete_leaves_a_failed_row_and_the_folder() {
    let home = unique_temp_dir("discard_failed");
    let live = home.join(".agents/skills/foo");
    write_skill(&live, "foo");
    let (rt, failing) = failing_runtime(&home);
    leave_a_parked_copy_behind(&rt, "foo", &live);
    let req = discard_request(&rt, "foo", false);
    failing.fail_next_rename();

    assert!(ops::discard(&rt, &ctx(), &req).is_err());

    assert!(live.join("SKILL.md").exists());
    let events = ops::list_events(&rt, &ctx(), &ListEventsRequest::default()).unwrap();
    let rows: Vec<(String, String)> = events
        .iter()
        .map(|e| (e.kind.clone(), e.status.clone()))
        .collect();
    assert!(
        rows.iter().any(|(k, s)| k == "remove" && s == "failed"),
        "{rows:?}"
    );
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: the leftover delete fails halfway; undo the event. Expectation:
/// the discard is Done and undo brings every file back.
#[test]
fn a_partial_trash_delete_still_undoes_to_the_full_folder() {
    let home = unique_temp_dir("discard_partial");
    let live = home.join(".agents/skills/foo");
    write_skill(&live, "foo");
    let (rt, failing) = failing_runtime(&home);
    let parked = leave_a_parked_copy_behind(&rt, "foo", &live);
    std::fs::write(parked.join("notes.txt"), "keep me").unwrap();
    let req = discard_request(&rt, "foo", true);
    failing.fail_next_remove_file();

    let outcome = ops::discard(&rt, &ctx(), &req).unwrap();
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
    assert!(parked.join("notes.txt").exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: discard a parked agent copy, then undo. Expectation: the
/// `.origin/<name>` note comes back with the folder. Failure: the restored
/// copy has no origin and cannot be unparked.
#[test]
fn undoing_a_discard_of_a_parked_agent_copy_restores_its_origin_note() {
    let home = unique_temp_dir("discard_origin_note");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    let rt = runtime(&home, &[]);
    let parked = leave_a_parked_copy_behind(&rt, "foo", &live);
    let note = parked.parent().unwrap().join(".origin/foo");
    assert!(note.exists(), "park must leave an origin note");

    let outcome = ops::discard(&rt, &ctx(), &discard_request(&rt, "foo", true)).unwrap();
    assert!(!note.exists());
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
    assert!(note.exists(), "undo must bring the origin note back");
    std::fs::remove_dir_all(&home).ok();
}
