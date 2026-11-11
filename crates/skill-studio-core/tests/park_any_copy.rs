// Integration test binaries aren't covered by the lib crate's
// `cfg_attr(test, allow(...))`: this file compiles as its own crate, so
// the same allow needs to be declared here too.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr
)]

//! Real-disk tests for parking any real copy of a skill (#386): a global
//! agent folder, a project folder, the Universal folder, and the old flat
//! parked layout. Each test names the flow, what must hold, and what a
//! failure means.
//!
//! The git cases run the real `git` binary through the process spawner
//! port; they skip the tracked assertion, with a message, when it is absent.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use skill_studio_core::dto::{
    DeploymentDto, ListEventsRequest, ParkCheckRequest, ParkRequest, ScanRequest, UnparkRequest,
};
use skill_studio_core::harness::HarnessCatalog;
use skill_studio_core::identity::{AgentId, RootKind, RootScope, SkillName};
use skill_studio_core::ops;
use skill_studio_core::ports::{Ports, ProcessSpawner, Runtime, ScopeFs};
use skill_studio_core::scope::{ProjectSelection, RuntimeScope};
use skill_studio_core::testing::golden::{ctx, unique_temp_dir};
use skill_studio_core::testing::{FailingFs, FakeClock, FakeIds, RecordingSink};

use skill_studio_host::{FileLease, RealFs, RealProcessSpawner, SqliteHistoryOpener};

const PARKED_ROOT_RELATIVE: &str = ".agents/skills-parked";

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

/// The one live copy under `root_kind` whose path is `path`.
fn live_copy(rt: &Runtime, name: &str, path: &Path) -> DeploymentDto {
    deployments(rt, name)
        .into_iter()
        .find(|d| d.root.kind != RootKind::Parked && d.path == path)
        .unwrap_or_else(|| panic!("no live copy of {name} at {}", path.display()))
}

/// The one parked copy of `name` that came from `origin_kind`.
fn parked_copy(rt: &Runtime, name: &str, origin_kind: &RootKind) -> DeploymentDto {
    deployments(rt, name)
        .into_iter()
        .find(|d| {
            d.root.kind == RootKind::Parked
                && d.parked_origin.as_ref().map(|o| &o.kind) == Some(origin_kind)
        })
        .unwrap_or_else(|| panic!("no parked copy of {name} from {origin_kind:?}"))
}

fn git_present() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|out| out.status.success())
}

fn git_in(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed in {}", dir.display());
}

/// Flow: park then unpark a global Universal copy. Expectation: the copy sits
/// under `skills-parked/universal/`, its origin is the global Universal root,
/// and unpark returns it to `~/.agents/skills`. Failure: the new layout is
/// not used or the copy returns somewhere else.
#[test]
fn global_universal_copy_parks_under_universal_and_unparks_to_the_same_place() {
    let home = unique_temp_dir("park_any_universal");
    let live = home.join(".agents/skills/foo");
    write_skill(&live, "foo");
    let rt = runtime(&home, &[]);

    let outcome = ops::park(
        &rt,
        &ctx(),
        &ParkRequest {
            deployment_id: live_copy(&rt, "foo", &live).id,
        },
    )
    .unwrap();
    assert_eq!(
        outcome.parked_path,
        home.join(PARKED_ROOT_RELATIVE).join("universal/foo")
    );
    assert!(!live.exists());

    let parked = parked_copy(&rt, "foo", &RootKind::Universal);
    assert_eq!(
        parked.parked_origin.as_ref().map(|o| &o.scope),
        Some(&RootScope::Global)
    );
    let restored = ops::unpark(
        &rt,
        &ctx(),
        &UnparkRequest {
            deployment_id: parked.id,
        },
    )
    .unwrap();
    assert_eq!(restored.restored_path, live);
    assert!(live.join("SKILL.md").exists());
    assert!(!outcome.parked_path.exists());

    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park then unpark a real copy in `~/.codex/skills`. Expectation: it
/// parks under `skills-parked/codex/` and unparks to `~/.codex/skills/foo`.
/// Failure: park still refuses a non-Universal copy, or unpark guesses the
/// Universal root.
#[test]
fn global_codex_folder_copy_parks_under_codex_and_unparks_to_the_codex_folder() {
    let home = unique_temp_dir("park_any_codex");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    let rt = runtime(&home, &[]);

    let outcome = ops::park(
        &rt,
        &ctx(),
        &ParkRequest {
            deployment_id: live_copy(&rt, "foo", &live).id,
        },
    )
    .unwrap();
    assert_eq!(
        outcome.parked_path,
        home.join(PARKED_ROOT_RELATIVE).join("codex/foo")
    );

    let codex = RootKind::Harness(AgentId::parse("codex").unwrap());
    let restored = ops::unpark(
        &rt,
        &ctx(),
        &UnparkRequest {
            deployment_id: parked_copy(&rt, "foo", &codex).id,
        },
    )
    .unwrap();
    assert_eq!(restored.restored_path, live);
    assert!(live.join("SKILL.md").exists());

    std::fs::remove_dir_all(&home).ok();
}

/// Flow: a project `.claude/skills/foo` that git tracks. Expectation:
/// `park_check` says `git_tracked`, park files the copy under
/// `projects/<basename>-<12 hex>/claude-code/foo`, and unpark restores it into
/// the project. Failure: the tracked check misses the repo, the project copy
/// is parked as global, or it returns to the wrong project.
#[test]
fn tracked_project_copy_is_flagged_parks_under_its_project_key_and_unparks_into_the_project() {
    let home = unique_temp_dir("park_any_project_git");
    let project = home.join("work/app");
    let live = project.join(".claude/skills/foo");
    write_skill(&live, "foo");
    let have_git = git_present();
    if have_git {
        git_in(&project, &["init", "-q"]);
        git_in(&project, &["add", ".claude/skills/foo/SKILL.md"]);
    } else {
        eprintln!("git is not installed: skipping the git_tracked assertion");
    }
    let rt = runtime(&home, std::slice::from_ref(&project));
    let copy = live_copy(&rt, "foo", &live);

    let check = ops::park_check(
        &rt,
        &ctx(),
        &ParkCheckRequest {
            deployment_id: copy.id.clone(),
        },
    )
    .unwrap();
    if have_git {
        assert_eq!(
            check.git_tracked,
            Some(true),
            "git lists the folder, so it is tracked"
        );
    }
    assert_eq!(check.project.as_deref(), Some(project.as_path()));

    let outcome = ops::park(
        &rt,
        &ctx(),
        &ParkRequest {
            deployment_id: copy.id,
        },
    )
    .unwrap();
    let key_dir = outcome
        .parked_path
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .to_path_buf();
    let key = key_dir.file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        key.starts_with("app-") && key.len() == "app-".len() + 12,
        "unexpected project key {key}"
    );
    assert_eq!(
        outcome.parked_path,
        home.join(PARKED_ROOT_RELATIVE)
            .join("projects")
            .join(&key)
            .join("claude-code/foo")
    );
    assert!(!live.exists());

    let claude = RootKind::Harness(AgentId::parse("claude-code").unwrap());
    let parked = parked_copy(&rt, "foo", &claude);
    assert_eq!(
        parked
            .parked_origin
            .as_ref()
            .map(|o| matches!(o.scope, RootScope::Project(_))),
        Some(true),
        "the scan must report the project the copy came from"
    );
    let restored = ops::unpark(
        &rt,
        &ctx(),
        &UnparkRequest {
            deployment_id: parked.id,
        },
    )
    .unwrap();
    assert_eq!(restored.restored_path, live);
    assert!(live.join("SKILL.md").exists());

    std::fs::remove_dir_all(&home).ok();
}

/// Flow: a project copy outside any git repository. Expectation:
/// `park_check` says `git_tracked: false`. Failure: a non-repo folder is
/// reported as tracked, and the confirm would warn for nothing.
#[test]
fn project_copy_outside_git_is_not_flagged_as_tracked() {
    let home = unique_temp_dir("park_any_project_plain");
    let project = home.join("work/plain");
    let live = project.join(".agents/skills/foo");
    write_skill(&live, "foo");
    let rt = runtime(&home, std::slice::from_ref(&project));

    let check = ops::park_check(
        &rt,
        &ctx(),
        &ParkCheckRequest {
            deployment_id: live_copy(&rt, "foo", &live).id,
        },
    )
    .unwrap();

    assert_eq!(check.git_tracked, Some(false));
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: a repository that tracks one skill and holds another untracked.
/// Expectation: only the tracked one reports `git_tracked`. Failure: the
/// check answers per repository, not per folder.
#[test]
fn untracked_folder_inside_a_repository_is_not_flagged_as_tracked() {
    if !git_present() {
        eprintln!("git is not installed: skipping");
        return;
    }
    let home = unique_temp_dir("park_any_project_untracked");
    let project = home.join("work/app");
    let tracked = project.join(".claude/skills/kept");
    let untracked = project.join(".claude/skills/loose");
    write_skill(&tracked, "kept");
    write_skill(&untracked, "loose");
    git_in(&project, &["init", "-q"]);
    git_in(&project, &["add", ".claude/skills/kept/SKILL.md"]);
    let rt = runtime(&home, std::slice::from_ref(&project));

    let check = |name: &str, path: &Path| {
        ops::park_check(
            &rt,
            &ctx(),
            &ParkCheckRequest {
                deployment_id: live_copy(&rt, name, path).id,
            },
        )
        .unwrap()
        .git_tracked
    };

    assert_eq!(check("kept", &tracked), Some(true));
    assert_eq!(check("loose", &untracked), Some(false));
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: the same skill name parked from the Universal folder and from
/// `~/.codex/skills`. Expectation: both parked copies coexist and each unparks
/// to its own origin. Failure: the second park collides with the first, or
/// an unpark restores to the other origin.
#[test]
fn two_parked_copies_of_one_name_coexist_and_each_unparks_to_its_own_origin() {
    let home = unique_temp_dir("park_any_coexist");
    let universal = home.join(".agents/skills/foo");
    let codex_dir = home.join(".codex/skills/foo");
    write_skill(&universal, "foo");
    write_skill(&codex_dir, "foo");
    let rt = runtime(&home, &[]);

    for path in [&universal, &codex_dir] {
        ops::park(
            &rt,
            &ctx(),
            &ParkRequest {
                deployment_id: live_copy(&rt, "foo", path).id,
            },
        )
        .unwrap();
    }
    assert!(home
        .join(PARKED_ROOT_RELATIVE)
        .join("universal/foo")
        .exists());
    assert!(home.join(PARKED_ROOT_RELATIVE).join("codex/foo").exists());

    let codex = RootKind::Harness(AgentId::parse("codex").unwrap());
    let from_codex = ops::unpark(
        &rt,
        &ctx(),
        &UnparkRequest {
            deployment_id: parked_copy(&rt, "foo", &codex).id,
        },
    )
    .unwrap();
    assert_eq!(from_codex.restored_path, codex_dir);
    assert!(!universal.exists(), "the Universal copy is still parked");

    let from_universal = ops::unpark(
        &rt,
        &ctx(),
        &UnparkRequest {
            deployment_id: parked_copy(&rt, "foo", &RootKind::Universal).id,
        },
    )
    .unwrap();
    assert_eq!(from_universal.restored_path, universal);

    std::fs::remove_dir_all(&home).ok();
}

/// Flow: a copy left in the old flat `skills-parked/foo` layout. Expectation:
/// the scan reports it as parked from the Universal root and unpark returns
/// it to `~/.agents/skills/foo`. Failure: the legacy folder is invisible to
/// the scan or goes to the wrong place.
#[test]
fn legacy_flat_parked_copy_is_scanned_and_unparks_to_the_universal_folder() {
    let home = unique_temp_dir("park_any_legacy");
    write_skill(&home.join(PARKED_ROOT_RELATIVE).join("foo"), "foo");
    let rt = runtime(&home, &[]);

    let parked = parked_copy(&rt, "foo", &RootKind::Universal);
    let restored = ops::unpark(
        &rt,
        &ctx(),
        &UnparkRequest {
            deployment_id: parked.id,
        },
    )
    .unwrap();

    assert_eq!(restored.restored_path, home.join(".agents/skills/foo"));
    assert!(home.join(".agents/skills/foo/SKILL.md").exists());
    assert!(!home.join(PARKED_ROOT_RELATIVE).join("foo").exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park a Universal copy, then put a new copy back at its origin and
/// unpark. Expectation: unpark refuses and leaves both folders alone.
/// Failure: unpark overwrites the live copy or deletes the parked one.
#[test]
fn unpark_refuses_when_a_live_copy_already_sits_at_the_origin() {
    let home = unique_temp_dir("park_any_unpark_blocked");
    let live = home.join(".agents/skills/foo");
    write_skill(&live, "foo");
    let rt = runtime(&home, &[]);
    let outcome = ops::park(
        &rt,
        &ctx(),
        &ParkRequest {
            deployment_id: live_copy(&rt, "foo", &live).id,
        },
    )
    .unwrap();
    write_skill(&live, "foo");

    let err = ops::unpark(
        &rt,
        &ctx(),
        &UnparkRequest {
            deployment_id: parked_copy(&rt, "foo", &RootKind::Universal).id,
        },
    )
    .unwrap_err();

    assert!(err.message.contains("already exists"), "{}", err.message);
    assert!(outcome.parked_path.join("SKILL.md").exists());
    assert!(live.join("SKILL.md").exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park a Universal copy, put a new one back, and park that too.
/// Expectation: the second park refuses because a parked copy already exists
/// for that origin. Failure: the first parked copy is overwritten.
#[test]
fn park_refuses_when_a_parked_copy_already_exists_for_the_same_origin() {
    let home = unique_temp_dir("park_any_park_blocked");
    let live = home.join(".agents/skills/foo");
    write_skill(&live, "foo");
    let rt = runtime(&home, &[]);
    let first = ops::park(
        &rt,
        &ctx(),
        &ParkRequest {
            deployment_id: live_copy(&rt, "foo", &live).id,
        },
    )
    .unwrap();
    write_skill(&live, "foo");

    let err = ops::park(
        &rt,
        &ctx(),
        &ParkRequest {
            deployment_id: live_copy(&rt, "foo", &live).id,
        },
    )
    .unwrap_err();

    assert!(
        err.message
            .contains("parked copy from this folder already exists"),
        "{}",
        err.message
    );
    assert!(live.join("SKILL.md").exists());
    assert!(first.parked_path.join("SKILL.md").exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park a skill that a Claude Code plugin ships. Expectation: refused
/// with the `/plugin` hint. Failure: park moves files out of the plugin cache.
#[test]
fn park_refuses_a_plugin_copy_and_points_at_the_plugin_command() {
    let home = unique_temp_dir("park_any_plugin");
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
    let copy = deployments(&rt, "shipped")
        .into_iter()
        .next()
        .expect("the plugin skill is scanned");

    let err = ops::park(
        &rt,
        &ctx(),
        &ParkRequest {
            deployment_id: copy.id,
        },
    )
    .unwrap_err();

    assert!(err.message.contains("/plugin"), "{}", err.message);
    assert!(shipped.join("SKILL.md").exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Forgets every journal row, so an unpark has only the parked folder and
/// its markers to go on.
fn forget_history(home: &Path) {
    std::fs::remove_dir_all(home.join(".history")).unwrap();
}

fn park_copy_at(rt: &Runtime, name: &str, path: &Path) -> PathBuf {
    ops::park(
        rt,
        &ctx(),
        &ParkRequest {
            deployment_id: live_copy(rt, name, path).id,
        },
    )
    .unwrap()
    .parked_path
}

fn only_parked_copy(rt: &Runtime, name: &str) -> DeploymentDto {
    deployments(rt, name)
        .into_iter()
        .find(|d| d.root.kind == RootKind::Parked)
        .unwrap_or_else(|| panic!("no parked copy of {name}"))
}

/// Flow: a Codex-folder copy of `foo` is parked, then a Universal copy of a
/// skill named `codex` is parked. Expectation: the second lands in
/// `universal/codex` and the `codex` slot keeps `foo`; both unpark to their
/// origins. Failure: the skill named like a slot is refused as already parked,
/// or it lands inside the slot.
#[test]
fn universal_skill_named_like_a_slot_parks_beside_that_slot() {
    let home = unique_temp_dir("park_any_slot_name_universal");
    let codex_copy = home.join(".codex/skills/foo");
    let shared = home.join(".agents/skills/codex");
    write_skill(&codex_copy, "foo");
    write_skill(&shared, "codex");
    let rt = runtime(&home, &[]);

    park_copy_at(&rt, "foo", &codex_copy);
    let parked = park_copy_at(&rt, "codex", &shared);

    let parked_root = home.join(PARKED_ROOT_RELATIVE);
    assert_eq!(parked, parked_root.join("universal/codex"));
    assert!(parked_root.join("codex/foo/SKILL.md").exists());
    assert!(!parked_root.join("codex/SKILL.md").exists());
    let restored = ops::unpark(
        &rt,
        &ctx(),
        &UnparkRequest {
            deployment_id: parked_copy(&rt, "codex", &RootKind::Universal).id,
        },
    )
    .unwrap();
    assert_eq!(restored.restored_path, shared);
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: an old flat `skills-parked/codex` (a skill named `codex`) exists,
/// then a Codex-folder copy of `bar` is parked. Expectation: the old copy
/// moves to `universal/codex` first, `bar` lands in the `codex` slot, and the
/// old copy still unparks to `~/.agents/skills/codex`. Failure: `bar` nests
/// inside the old skill's folder, or the old skill is lost.
#[test]
fn legacy_flat_copy_named_like_a_slot_moves_aside_when_a_codex_copy_parks() {
    let home = unique_temp_dir("park_any_slot_name_legacy");
    let parked_root = home.join(PARKED_ROOT_RELATIVE);
    write_skill(&parked_root.join("codex"), "codex");
    let codex_copy = home.join(".codex/skills/bar");
    write_skill(&codex_copy, "bar");
    let rt = runtime(&home, &[]);

    let parked = park_copy_at(&rt, "bar", &codex_copy);

    assert_eq!(parked, parked_root.join("codex/bar"));
    assert!(parked_root.join("universal/codex/SKILL.md").exists());
    assert!(!parked_root.join("codex/SKILL.md").exists());
    let restored = ops::unpark(
        &rt,
        &ctx(),
        &UnparkRequest {
            deployment_id: parked_copy(&rt, "codex", &RootKind::Universal).id,
        },
    )
    .unwrap();
    assert_eq!(restored.restored_path, home.join(".agents/skills/codex"));
    assert!(home.join(".agents/skills/codex/SKILL.md").exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park a Codex copy whose rename fails with EXDEV (another volume).
/// Expectation: the folder is copied with its file modes, verified, and the
/// source removed. Failure: park fails on a second volume, or the copy loses
/// the executable bit or content.
#[test]
fn park_across_volumes_copies_verifies_and_removes_the_source() {
    use std::os::unix::fs::PermissionsExt;
    let home = unique_temp_dir("park_any_exdev");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    std::fs::create_dir_all(live.join("scripts")).unwrap();
    std::fs::write(live.join("scripts/run.sh"), "#!/bin/sh\necho hi\n").unwrap();
    std::fs::set_permissions(
        live.join("scripts/run.sh"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let failing = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
    let rt = runtime_with(
        &home,
        &[],
        failing.clone(),
        Arc::new(RealProcessSpawner::new()),
    );
    let deployment_id = live_copy(&rt, "foo", &live).id;
    failing.fail_next_rename_cross_device();

    let parked = ops::park(&rt, &ctx(), &ParkRequest { deployment_id })
        .unwrap()
        .parked_path;

    assert!(!live.exists(), "the source folder must be gone");
    assert!(parked.join("SKILL.md").exists());
    assert_eq!(
        std::fs::read_to_string(parked.join("scripts/run.sh")).unwrap(),
        "#!/bin/sh\necho hi\n"
    );
    let mode = std::fs::metadata(parked.join("scripts/run.sh"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o755);
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park a project copy across volumes, with one file that cannot be
/// read halfway through the copy. Expectation: park fails, the source is
/// untouched. The partial destination stays for inspection.
#[test]
fn failed_cross_volume_park_keeps_source_and_partial_copy_or_deletes_uncertain_entries() {
    use std::os::unix::fs::PermissionsExt;
    let home = unique_temp_dir("park_any_exdev_fail");
    let project = home.join("work/app");
    let live = project.join(".claude/skills/foo");
    write_skill(&live, "foo");
    std::fs::write(live.join("z-unreadable.md"), "secret").unwrap();
    std::fs::set_permissions(
        live.join("z-unreadable.md"),
        std::fs::Permissions::from_mode(0o000),
    )
    .unwrap();
    let failing = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
    let rt = runtime_with(
        &home,
        std::slice::from_ref(&project),
        failing.clone(),
        Arc::new(RealProcessSpawner::new()),
    );
    let deployment_id = live_copy(&rt, "foo", &live).id;
    failing.fail_next_rename_cross_device();

    let result = ops::park(&rt, &ctx(), &ParkRequest { deployment_id });

    std::fs::set_permissions(
        live.join("z-unreadable.md"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert!(result.is_err(), "an unreadable file must fail the park");
    assert!(live.join("SKILL.md").exists());
    assert!(live.join("z-unreadable.md").exists());
    assert!(
        home.join(PARKED_ROOT_RELATIVE).exists(),
        "the partial copy must remain for inspection"
    );
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park an `OpenCode` copy, check the `.origin` marker, forget the
/// journal, and unpark. Expectation: the marker's first line holds the exact skills
/// folder the catalog spells (`.config/opencode/skills`) and the copy returns there.
/// Failure: the marker is missing or wrong, so a no-row unpark has to guess
/// the folder. (The singular `skill/` folder is a legacy root and park
/// refuses it, so it never needs a marker.)
#[test]
fn unpark_without_a_journal_row_returns_an_opencode_copy_to_its_own_folder() {
    let home = unique_temp_dir("park_any_opencode_skill");
    let live = home.join(".config/opencode/skills/foo");
    write_skill(&live, "foo");
    let rt = runtime(&home, &[]);
    park_copy_at(&rt, "foo", &live);
    let marker = home
        .join(PARKED_ROOT_RELATIVE)
        .join("open-code/.origin/foo");
    assert_eq!(
        std::fs::read_to_string(marker).unwrap().lines().next(),
        Some(".config/opencode/skills")
    );
    forget_history(&home);
    let rt = runtime(&home, &[]);

    let restored = ops::unpark(
        &rt,
        &ctx(),
        &UnparkRequest {
            deployment_id: only_parked_copy(&rt, "foo").id,
        },
    )
    .unwrap();

    assert_eq!(restored.restored_path, live);
    assert!(live.join("SKILL.md").exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park a project copy, forget the journal, unpark. Expectation: the
/// copy returns into its project through the `.origin` markers. Failure: a
/// project copy with no journal row cannot come back, or comes back global.
#[test]
fn unpark_without_a_journal_row_returns_a_project_copy_through_its_markers() {
    let home = unique_temp_dir("park_any_project_no_row");
    let project = home.join("work/app");
    let live = project.join(".claude/skills/foo");
    write_skill(&live, "foo");
    let projects = [project.clone()];
    let rt = runtime(&home, &projects);
    park_copy_at(&rt, "foo", &live);
    forget_history(&home);
    let rt = runtime(&home, &projects);

    let restored = ops::unpark(
        &rt,
        &ctx(),
        &UnparkRequest {
            deployment_id: only_parked_copy(&rt, "foo").id,
        },
    )
    .unwrap();

    assert_eq!(restored.restored_path, live);
    assert!(live.join("SKILL.md").exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park a project copy, forget the journal, then unpark with a scope
/// that no longer covers that project. Expectation: refused, nothing moves.
/// Failure: a marker alone writes into a folder outside the scope.
#[test]
fn unpark_without_a_journal_row_refuses_a_project_outside_the_scope() {
    let home = unique_temp_dir("park_any_project_unknown");
    let project = home.join("work/app");
    let live = project.join(".claude/skills/foo");
    write_skill(&live, "foo");
    let rt = runtime(&home, std::slice::from_ref(&project));
    let parked = park_copy_at(&rt, "foo", &live);
    forget_history(&home);
    let rt = runtime(&home, &[]);

    let result = ops::unpark(
        &rt,
        &ctx(),
        &UnparkRequest {
            deployment_id: only_parked_copy(&rt, "foo").id,
        },
    );

    assert!(
        result.is_err(),
        "a project the scope does not know must be refused"
    );
    assert!(parked.join("SKILL.md").exists());
    assert!(!live.exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park a Codex copy, forget the journal, rewrite its `.origin` marker
/// to a path outside the catalog, and unpark. Expectation: refused, the
/// parked copy stays. Failure: a hand-edited marker steers the copy to any
/// folder it names.
#[test]
fn unpark_without_a_journal_row_refuses_a_marker_that_names_an_unknown_folder() {
    let home = unique_temp_dir("park_any_marker_evil");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    let rt = runtime(&home, &[]);
    let parked = park_copy_at(&rt, "foo", &live);
    forget_history(&home);
    std::fs::write(
        home.join(PARKED_ROOT_RELATIVE).join("codex/.origin/foo"),
        "../../../elsewhere",
    )
    .unwrap();
    let rt = runtime(&home, &[]);

    let result = ops::unpark(
        &rt,
        &ctx(),
        &UnparkRequest {
            deployment_id: only_parked_copy(&rt, "foo").id,
        },
    );

    assert!(
        result.is_err(),
        "a marker naming an unknown folder must be refused"
    );
    assert!(parked.join("SKILL.md").exists());
    assert!(!home.parent().unwrap().join("elsewhere").exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: `~/.claude/skills/foo` links to the real copy in
/// `~/.codex/skills/foo`; park the Codex copy, then unpark. Expectation: park
/// removes the link, unpark puts it back and it resolves. Failure: the link
/// dangles after park, or is lost after unpark.
#[test]
fn links_to_a_parked_agent_folder_copy_are_removed_and_recreated() {
    let home = unique_temp_dir("park_any_agent_link");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    let link = home.join(".claude/skills/foo");
    std::fs::create_dir_all(link.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink("../../.codex/skills/foo", &link).unwrap();
    let rt = runtime(&home, &[]);

    park_copy_at(&rt, "foo", &live);
    assert!(
        std::fs::symlink_metadata(&link).is_err(),
        "the link must not dangle after park"
    );
    ops::unpark(
        &rt,
        &ctx(),
        &UnparkRequest {
            deployment_id: only_parked_copy(&rt, "foo").id,
        },
    )
    .unwrap();

    assert!(live.join("SKILL.md").exists());
    assert!(std::fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(
        link.join("SKILL.md").exists(),
        "the link must resolve again"
    );
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park the same project copy once with the project named by a
/// symlink, and once by its real path. Expectation: both land under one
/// `projects/<key>` folder. Failure: the key follows the spelling of the
/// path, so one project gets two parked folders.
#[test]
fn project_key_is_the_same_for_a_symlinked_and_a_real_project_path() {
    let home = unique_temp_dir("park_any_project_key");
    let project = home.join("work/app");
    let alias = home.join("work/app-alias");
    let live = project.join(".claude/skills/foo");
    write_skill(&live, "foo");
    std::os::unix::fs::symlink(&project, &alias).unwrap();

    let via_alias = runtime(&home, std::slice::from_ref(&alias));
    let alias_live = alias.join(".claude/skills/foo");
    let first = park_copy_at(&via_alias, "foo", &alias_live);
    ops::unpark(
        &via_alias,
        &ctx(),
        &UnparkRequest {
            deployment_id: only_parked_copy(&via_alias, "foo").id,
        },
    )
    .unwrap();
    // A new runtime restarts the fake event ids, so it needs a fresh journal.
    forget_history(&home);
    let via_real = runtime(&home, std::slice::from_ref(&project));
    let second = park_copy_at(&via_real, "foo", &live);

    let key_dir = |parked: &Path| parked.parent().unwrap().parent().unwrap().to_path_buf();
    assert_eq!(key_dir(&first), key_dir(&second));
    let keys = std::fs::read_dir(home.join(PARKED_ROOT_RELATIVE).join("projects"))
        .unwrap()
        .count();
    assert_eq!(keys, 1);
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: `park_check` on a macOS machine whose command line tools are not
/// installed, where `git` is a stub that opens an install dialog.
/// Expectation: `git_tracked` is unknown (`None`) and git is not run.
/// Failure: the stub runs, or the answer claims `false` for a repo it never
/// looked at.
#[cfg(target_os = "macos")]
#[test]
fn park_check_reports_unknown_when_the_command_line_tools_are_missing() {
    use skill_studio_core::testing::FakeProcessSpawner;
    let home = unique_temp_dir("park_any_no_clt");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    let mut spawner = FakeProcessSpawner::default();
    spawner
        .outputs
        .insert("xcode-select".to_string(), (String::new(), 2));
    let rt = runtime_with(&home, &[], Arc::new(RealFs::new()), Arc::new(spawner));

    let check = ops::park_check(
        &rt,
        &ctx(),
        &ParkCheckRequest {
            deployment_id: live_copy(&rt, "foo", &live).id,
        },
    )
    .unwrap();

    assert_eq!(check.git_tracked, None);
    std::fs::remove_dir_all(&home).ok();
}

fn failing_runtime(home: &Path, projects: &[PathBuf]) -> (Runtime, Arc<FailingFs>) {
    let failing = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
    let rt = runtime_with(
        home,
        projects,
        failing.clone(),
        Arc::new(RealProcessSpawner::new()),
    );
    (rt, failing)
}

/// Hidden `.park-trash-*` folders directly under `dir`.
fn leftover_trash(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(".park-trash-"))
        })
        .collect()
}

#[test]
fn park_across_volumes_retains_source_backup_or_loses_open_handle_writes() {
    let home = unique_temp_dir("park_any_exdev_delete_fails");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    std::fs::write(live.join("notes.md"), "notes").unwrap();
    let (rt, failing) = failing_runtime(&home, &[]);
    let deployment_id = live_copy(&rt, "foo", &live).id;
    failing.fail_next_rename_cross_device();
    failing.fail_next_remove_file();

    let parked = ops::park(&rt, &ctx(), &ParkRequest { deployment_id })
        .unwrap()
        .parked_path;

    assert!(parked.join("SKILL.md").exists());
    assert!(home
        .join(PARKED_ROOT_RELATIVE)
        .join("codex/.origin/foo")
        .exists());
    assert!(
        !live.exists(),
        "the source must be out of its own path, even if a hidden copy is left"
    );
    assert!(deployments(&rt, "foo")
        .iter()
        .all(|d| d.root.kind == RootKind::Parked));
    let backup = quarantine_in(&home.join(".codex/skills"));
    assert!(backup.join("SKILL.md").exists());
    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn unpark_across_volumes_retains_source_backup_or_loses_open_handle_writes() {
    let home = unique_temp_dir("park_any_exdev_unpark_delete_fails");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    std::fs::write(live.join("notes.md"), "notes").unwrap();
    let (rt, failing) = failing_runtime(&home, &[]);
    park_copy_at(&rt, "foo", &live);
    failing.fail_next_rename_cross_device();
    failing.fail_next_remove_file();

    let restored = ops::unpark(
        &rt,
        &ctx(),
        &UnparkRequest {
            deployment_id: only_parked_copy(&rt, "foo").id,
        },
    )
    .unwrap();

    assert_eq!(restored.restored_path, live);
    assert!(live.join("SKILL.md").exists());
    let backup = quarantine_in(&home.join(PARKED_ROOT_RELATIVE).join("codex"));
    assert!(backup.join("SKILL.md").exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: a stale `.park-trash-*` folder (with a `SKILL.md`) sits in a root, as
/// an earlier failed delete left it; then a skill in that root is parked.
/// Expectation: the stale folder is removed, a link with the same prefix is
/// not followed (its target stays), and a folder without the prefix stays.
/// Failure: stale copies of skills pile up in agent roots.
#[test]
fn park_sweeps_stale_trash_folders_in_the_roots_it_touches() {
    let home = unique_temp_dir("park_any_sweep");
    let root = home.join(".codex/skills");
    let live = root.join("foo");
    write_skill(&live, "foo");
    write_skill(&root.join(".park-trash-1-1"), "old");
    let outside = home.join("outside");
    write_skill(&outside, "outside");
    std::os::unix::fs::symlink(&outside, root.join(".park-trash-2-2")).unwrap();
    std::fs::create_dir_all(root.join(".keep-me")).unwrap();
    let rt = runtime(&home, &[]);

    park_copy_at(&rt, "foo", &live);

    assert!(!root.join(".park-trash-1-1").exists());
    assert!(
        outside.join("SKILL.md").exists(),
        "a link must not be followed"
    );
    assert!(root.join(".keep-me").exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: a cross-volume park where the copy verifies but the rename of the
/// source to the trash name fails. Expectation: park fails, the copy at the
/// parked path and source are retained for inspection.
#[test]
fn cross_volume_park_keeps_the_copy_when_moving_the_source_aside_fails() {
    let home = unique_temp_dir("park_any_exdev_trash_rename");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    let (rt, failing) = failing_runtime(&home, &[]);
    let deployment_id = live_copy(&rt, "foo", &live).id;
    failing.fail_next_rename_cross_device();
    failing.fail_rename_after_cross_device();

    let result = ops::park(&rt, &ctx(), &ParkRequest { deployment_id });

    assert!(result.is_err());
    assert!(live.join("SKILL.md").exists());
    assert!(leftover_trash(&home.join(".codex/skills")).is_empty());
    assert!(home.join(PARKED_ROOT_RELATIVE).exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: a cross-volume park of a folder with a link inside it. Expectation:
/// refused with a reason, source untouched, nothing parked. Failure: the
/// copy follows the link, or the delete removes files the link points at.
#[test]
fn cross_volume_park_refuses_a_folder_that_holds_a_link() {
    let home = unique_temp_dir("park_any_exdev_inner_link");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    let outside = home.join("outside");
    write_skill(&outside, "outside");
    std::os::unix::fs::symlink(&outside, live.join("ref")).unwrap();
    let (rt, failing) = failing_runtime(&home, &[]);
    let deployment_id = live_copy(&rt, "foo", &live).id;
    failing.fail_next_rename_cross_device();

    let result = ops::park(&rt, &ctx(), &ParkRequest { deployment_id });

    assert!(result.is_err());
    assert!(live.join("SKILL.md").exists());
    assert!(std::fs::symlink_metadata(live.join("ref")).is_ok());
    assert!(outside.join("SKILL.md").exists());
    assert!(!home.join(PARKED_ROOT_RELATIVE).exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: a cross-volume park of a skill that is itself a link (allowed in
/// the Universal folder). Expectation: refused, the link and the folder it
/// points at are untouched. Failure: the walk follows the link and the
/// delete empties the folder it points at.
#[test]
fn cross_volume_park_refuses_a_skill_that_is_a_link() {
    let home = unique_temp_dir("park_any_exdev_skill_link");
    let real = home.join("elsewhere/foo");
    write_skill(&real, "foo");
    let live = home.join(".agents/skills/foo");
    std::fs::create_dir_all(live.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&real, &live).unwrap();
    let (rt, failing) = failing_runtime(&home, &[]);
    let deployment_id = live_copy(&rt, "foo", &live).id;
    failing.fail_next_rename_cross_device();

    let result = ops::park(&rt, &ctx(), &ParkRequest { deployment_id });

    assert!(result.is_err());
    assert!(real.join("SKILL.md").exists(), "the link target must stay");
    assert!(std::fs::symlink_metadata(&live)
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(!home.join(PARKED_ROOT_RELATIVE).exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: a cross-volume park where one copied file comes out wrong.
/// Expectation: park fails and retains the source and failed copy.
/// Failure: the bad copy is trusted and the good source is deleted.
#[test]
fn cross_volume_park_rejects_a_copy_whose_content_differs() {
    let home = unique_temp_dir("park_any_exdev_hash");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    let (rt, failing) = failing_runtime(&home, &[]);
    let deployment_id = live_copy(&rt, "foo", &live).id;
    failing.fail_next_rename_cross_device();
    failing.corrupt_next_new_file_with_mode();

    let result = ops::park(&rt, &ctx(), &ParkRequest { deployment_id });

    assert!(result.is_err());
    assert!(std::fs::read_to_string(live.join("SKILL.md"))
        .unwrap()
        .contains("Body."));
    assert!(home.join(PARKED_ROOT_RELATIVE).exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: unpark moves the skill back, then recreating its link fails.
/// Expectation: unpark errors, the message says the skill is back, the
/// skill is at its origin, and the journal row is Failed, not Pending.
/// Failure: the row stays Pending forever while the files already moved.
#[test]
fn unpark_marks_the_row_failed_when_a_link_cannot_be_recreated() {
    let home = unique_temp_dir("park_any_relink_fails");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    let link = home.join(".claude/skills/foo");
    std::fs::create_dir_all(link.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink("../../.codex/skills/foo", &link).unwrap();
    let (rt, failing) = failing_runtime(&home, &[]);
    park_copy_at(&rt, "foo", &live);
    failing.fail_next_symlink();

    let result = ops::unpark(
        &rt,
        &ctx(),
        &UnparkRequest {
            deployment_id: only_parked_copy(&rt, "foo").id,
        },
    );

    let err = result.unwrap_err();
    assert!(
        err.message.contains("is back at its place"),
        "{}",
        err.message
    );
    assert!(live.join("SKILL.md").exists());
    let events = ops::list_events(
        &rt,
        &ctx(),
        &ListEventsRequest {
            skill: Some(SkillName("foo".to_string())),
            limit: 0,
            after: None,
            check_drift: false,
        },
    )
    .unwrap();
    let unpark_row = events.iter().find(|e| e.kind == "unpark").unwrap();
    assert_eq!(unpark_row.status, "failed");
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: a user folder named `.park-trash-notes` sits in a root; a skill in
/// that root is parked. Expectation: the folder and its file survive, because
/// the sweep only takes the exact `.park-trash-<digits>-<digits>` names.
/// Failure: the loose prefix match deletes a user's own folder.
#[test]
fn park_sweep_leaves_a_user_folder_with_a_similar_name() {
    let home = unique_temp_dir("park_any_sweep_user_folder");
    let root = home.join(".codex/skills");
    let live = root.join("foo");
    write_skill(&live, "foo");
    let user = root.join(".park-trash-notes");
    std::fs::create_dir_all(&user).unwrap();
    std::fs::write(user.join("todo.txt"), "mine").unwrap();
    let rt = runtime(&home, &[]);

    park_copy_at(&rt, "foo", &live);

    assert!(user.join("todo.txt").exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: a stale trash folder holds a link to a folder elsewhere; a park in
/// that root sweeps it. Expectation: the link is removed and the folder it
/// points at is untouched. Failure: the sweep follows the link and deletes
/// the target's files.
#[test]
fn park_sweep_unlinks_a_link_inside_trash_and_keeps_its_target() {
    let home = unique_temp_dir("park_any_sweep_inner_link");
    let root = home.join(".codex/skills");
    let live = root.join("foo");
    write_skill(&live, "foo");
    let stale = root.join(".park-trash-3-3");
    write_skill(&stale, "stale");
    let outside = home.join("outside");
    write_skill(&outside, "outside");
    std::os::unix::fs::symlink(&outside, stale.join("ref")).unwrap();
    let rt = runtime(&home, &[]);

    park_copy_at(&rt, "foo", &live);

    assert!(!stale.exists());
    assert!(outside.join("SKILL.md").exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: a stale trash folder cannot be fully deleted (an injected failure),
/// then a skill in that root is parked. Expectation: park still succeeds; the
/// sweep is best effort. Failure: a stuck stale folder blocks every park.
#[test]
fn park_succeeds_when_the_sweep_cannot_delete_a_stale_trash_folder() {
    let home = unique_temp_dir("park_any_sweep_fails");
    let root = home.join(".codex/skills");
    let live = root.join("foo");
    write_skill(&live, "foo");
    let stale = root.join(".park-trash-4-4");
    std::fs::create_dir_all(&stale).unwrap();
    std::fs::write(stale.join("notes.md"), "old").unwrap();
    let (rt, failing) = failing_runtime(&home, &[]);
    let deployment_id = live_copy(&rt, "foo", &live).id;
    failing.fail_next_remove_file();

    let parked = ops::park(&rt, &ctx(), &ParkRequest { deployment_id })
        .unwrap()
        .parked_path;

    assert!(parked.join("SKILL.md").exists());
    assert!(
        stale.join("notes.md").exists(),
        "the failed delete leaves it"
    );
    std::fs::remove_dir_all(&home).ok();
}

#[path = "../../../apps/cli/tests/support/park_volume.rs"]
mod park_volume;

#[test]
fn cross_volume_roundtrip_preserves_directory_and_file_modes() {
    let Some(fixture) = park_volume::fixture() else {
        return;
    };
    let live = fixture.project.join(".codex/skills/volume-copy");
    let original_bytes = park_volume::bytes(&live);
    let original_modes = park_volume::modes(&live);
    let rt = runtime(&fixture.home, std::slice::from_ref(&fixture.project));
    let parked = park_copy_at(&rt, "volume-copy", &live);
    let parked_modes = park_volume::modes(&parked);
    assert_eq!(park_volume::bytes(&parked), original_bytes);
    ops::unpark(
        &rt,
        &ctx(),
        &UnparkRequest {
            deployment_id: parked_copy(&rt, "volume-copy", &RootKind::Harness("codex".into())).id,
        },
    )
    .unwrap();
    assert_eq!(parked_modes, original_modes);
    assert_eq!(park_volume::modes(&live), original_modes);
    assert_eq!(park_volume::bytes(&live), original_bytes);
}

#[test]
fn cross_volume_park_keeps_source_when_full_sync_fails() {
    let home = unique_temp_dir("park_full_sync_failure");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    let original = std::fs::read(live.join("SKILL.md")).unwrap();
    let fs = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
    let rt = runtime_with(&home, &[], fs.clone(), Arc::new(RealProcessSpawner::new()));
    fs.fail_next_rename_cross_device();
    fs.fail_next_full_sync_file();
    let result = ops::park(
        &rt,
        &ctx(),
        &ParkRequest {
            deployment_id: live_copy(&rt, "foo", &live).id,
        },
    );
    assert!(result.is_err());
    assert_eq!(std::fs::read(live.join("SKILL.md")).unwrap(), original);
    assert!(stage_in(&home.join(".agents/skills-parked/codex")).exists());
    assert_eq!(fs.full_sync_calls(), 1);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn cross_volume_park_accepts_synthetic_modes_without_losing_bytes() {
    use std::os::unix::fs::PermissionsExt;
    let home = unique_temp_dir("park_synthetic_modes");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    std::fs::set_permissions(
        live.join("SKILL.md"),
        std::fs::Permissions::from_mode(0o751),
    )
    .unwrap();
    let original = std::fs::read(live.join("SKILL.md")).unwrap();
    let fs = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
    let rt = runtime_with(&home, &[], fs.clone(), Arc::new(RealProcessSpawner::new()));
    fs.fail_next_rename_cross_device();
    fs.use_synthetic_modes();
    let parked = park_copy_at(&rt, "foo", &live);
    assert_eq!(std::fs::read(parked.join("SKILL.md")).unwrap(), original);
    assert_ne!(
        std::fs::metadata(parked.join("SKILL.md"))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777,
        0o751
    );
    assert!(!live.exists());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn cross_volume_full_sync_finishes_before_source_disposal() {
    let home = unique_temp_dir("park_full_sync_order");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    let fs = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
    let rt = runtime_with(&home, &[], fs.clone(), Arc::new(RealProcessSpawner::new()));
    fs.fail_next_rename_cross_device();
    let checked = fs.clone();
    fs.after_rename_once(live.clone(), move || {
        assert!(
            checked.full_sync_calls() > 0,
            "source moved before full sync"
        );
        Ok(())
    });
    let parked = park_copy_at(&rt, "foo", &live);
    assert!(parked.join("SKILL.md").is_file());
    assert!(!live.exists());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn cross_volume_park_keeps_source_edit_during_sync_or_deletes_external_bytes() {
    let home = unique_temp_dir("park_sync_source_edit");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    std::fs::write(live.join("notes.txt"), "before").unwrap();
    let fs = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
    let rt = runtime_with(&home, &[], fs.clone(), Arc::new(RealProcessSpawner::new()));
    fs.fail_next_rename_cross_device();
    let notes = live.join("notes.txt");
    fs.after_full_sync_once(move || std::fs::write(notes, "external edit"));
    let id = live_copy(&rt, "foo", &live).id;
    let result = ops::park(&rt, &ctx(), &ParkRequest { deployment_id: id });
    let error = result.expect_err("Park deleted a source edited during sync");
    assert_eq!(
        error.code,
        skill_studio_core::error::ErrorCode::DriftConflict
    );
    let live = error.path.unwrap();
    let copy = home.join(".agents/skills-parked/codex/foo");
    assert_eq!(
        std::fs::read_to_string(copy.join("notes.txt")).unwrap(),
        "before"
    );
    assert_eq!(
        std::fs::read(copy.join("SKILL.md")).unwrap(),
        std::fs::read(live.join("SKILL.md")).unwrap()
    );
    assert_eq!(
        std::fs::read_to_string(live.join("notes.txt")).unwrap(),
        "external edit"
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn cross_volume_park_keeps_replaced_source_during_sync_or_deletes_external_folder() {
    let home = unique_temp_dir("park_sync_source_replaced");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    let fs = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
    let rt = runtime_with(&home, &[], fs.clone(), Arc::new(RealProcessSpawner::new()));
    fs.fail_next_rename_cross_device();
    let source = live.clone();
    let retained = home.join(".external-source-backup");
    fs.after_full_sync_once(move || {
        std::fs::rename(&source, retained)?;
        write_skill(&source, "foo");
        Ok(())
    });
    let id = live_copy(&rt, "foo", &live).id;
    let result = ops::park(&rt, &ctx(), &ParkRequest { deployment_id: id });
    let error = result.expect_err("Park deleted a replacement source with identical bytes");
    assert_eq!(
        error.code,
        skill_studio_core::error::ErrorCode::DriftConflict
    );
    let live = error.path.unwrap();
    let copy = home.join(".agents/skills-parked/codex/foo");
    assert_eq!(
        std::fs::read(copy.join("SKILL.md")).unwrap(),
        std::fs::read(live.join("SKILL.md")).unwrap()
    );
    assert_eq!(
        std::fs::read(home.join(".external-source-backup/SKILL.md")).unwrap(),
        std::fs::read(live.join("SKILL.md")).unwrap()
    );
    assert!(live.join("SKILL.md").is_file());
    std::fs::remove_dir_all(home).unwrap();
}

#[path = "../../../apps/cli/tests/support/park_display.rs"]
mod park_display;

#[test]
fn park_scan_keeps_each_project_owner_or_names_the_lost_type() {
    use skill_studio_core::identity::LifecycleOwnerKind;
    for (label, expected) in [
        ("dotagents", LifecycleOwnerKind::Dotagents),
        ("wildcard_dotagents", LifecycleOwnerKind::WildcardDotagents),
        ("in_repo", LifecycleOwnerKind::InRepo),
    ] {
        let home = unique_temp_dir("park_display_owner");
        let project = park_display::project(&home, label);
        let live = project.join(".agents/skills/foo");
        let rt = runtime(&home, std::slice::from_ref(&project));
        let original = live_copy(&rt, "foo", &live);
        assert_eq!(original.owner_kind, expected, "fixture {label}");
        ops::park(
            &rt,
            &ctx(),
            &ParkRequest {
                deployment_id: original.id,
            },
        )
        .unwrap();
        let parked = parked_copy(&rt, "foo", &RootKind::Universal);
        assert_eq!(parked.owner_kind, expected, "lost {label}");
        assert_eq!(parked.owner_id, original.owner_id, "lost {label} owner id");
        assert_eq!(
            parked.source_kind, original.source_kind,
            "lost {label} source"
        );
        assert_eq!(parked.parked_origin.unwrap().scope, original.root.scope);
        std::fs::remove_dir_all(home).unwrap();
    }
}

#[test]
fn turn_on_claude_copy_returns_the_restored_id_or_names_the_parked_id() {
    let home = unique_temp_dir("turn_on_display_id");
    let live = home.join(".claude/skills/foo");
    park_display::skill(&live);
    let rt = runtime(&home, &[]);
    let original = live_copy(&rt, "foo", &live);
    ops::park(
        &rt,
        &ctx(),
        &ParkRequest {
            deployment_id: original.id.clone(),
        },
    )
    .unwrap();
    let parked = parked_copy(&rt, "foo", &RootKind::Harness(AgentId::CLAUDE_CODE.into()));
    let result = ops::unpark(
        &rt,
        &ctx(),
        &UnparkRequest {
            deployment_id: parked.id,
        },
    )
    .unwrap();
    assert_eq!(result.deployment_id, original.id);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn copy_update_after_split_explains_the_skill_page_instead_of_an_absent_shared_copy() {
    let home = unique_temp_dir("split_update_display");
    park_display::skill(&home.join(".claude/skills/foo"));
    let rt = runtime(&home, &[]);
    let error = ops::update(
        &rt,
        &ctx(),
        &skill_studio_core::dto::UpdateRequest {
            skill: SkillName("foo".to_string()),
            method: skill_studio_core::dto::InstallMethod::Copy,
            scope: RootScope::Global,
            files: Vec::new(),
            source: None,
            ref_pin: None,
        },
    )
    .unwrap_err();
    assert!(
        error.message.contains("Skill Studio") && error.message.contains("Update"),
        "{}",
        error.message
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn parked_scan_keeps_the_project_source_type_but_reports_the_current_git_location() {
    let home = unique_temp_dir("park_physical_git");
    let project = park_display::project(&home, "in_repo");
    let rt = runtime(&home, std::slice::from_ref(&project));
    let original = live_copy(&rt, "foo", &project.join(".agents/skills/foo"));
    assert!(original.in_git_repo);
    ops::park(
        &rt,
        &ctx(),
        &ParkRequest {
            deployment_id: original.id,
        },
    )
    .unwrap();
    let parked = parked_copy(&rt, "foo", &RootKind::Universal);
    assert_eq!(parked.source_kind, original.source_kind);
    let physically_in_git = parked
        .path
        .ancestors()
        .any(|path| path.join(".git").exists());
    assert_eq!(
        parked.in_git_repo, physically_in_git,
        "the Git flag describes the old location"
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn cross_volume_park_keeps_edit_before_quarantine_or_deletes_already_hashed_bytes() {
    cross_volume_quarantine_race(false);
}

#[test]
fn cross_volume_park_keeps_replacement_before_quarantine_or_deletes_another_inode() {
    cross_volume_quarantine_race(true);
}

fn cross_volume_quarantine_race(replace: bool) {
    let home = unique_temp_dir("park_quarantine_race");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    let original = std::fs::read(live.join("SKILL.md")).unwrap();
    let fs = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
    let rt = runtime_with(&home, &[], fs.clone(), Arc::new(RealProcessSpawner::new()));
    let id = live_copy(&rt, "foo", &live).id;
    fs.fail_next_rename_cross_device();
    let source = live.clone();
    let backup = home.join("external-backup");
    fs.before_rename_once(live.clone(), move || {
        if replace {
            std::fs::rename(&source, backup)?;
            write_skill(&source, "foo");
        } else {
            std::fs::write(source.join("SKILL.md"), "external edit")?;
        }
        Ok(())
    });
    let error = ops::park(&rt, &ctx(), &ParkRequest { deployment_id: id })
        .expect_err("Park deleted the source changed after final verification");
    assert_eq!(
        error.code,
        skill_studio_core::error::ErrorCode::DriftConflict
    );
    let retained = error.path.unwrap();
    assert_eq!(
        std::fs::read(retained.join("SKILL.md")).unwrap(),
        if replace {
            original.clone()
        } else {
            b"external edit".to_vec()
        }
    );
    assert_eq!(
        std::fs::read(home.join(".agents/skills-parked/codex/foo/SKILL.md")).unwrap(),
        original
    );
    // A later Park must not sweep the conflict quarantine.
    write_skill(&live, "foo");
    let id = live_copy(&rt, "foo", &live).id;
    let _ = ops::park(&rt, &ctx(), &ParkRequest { deployment_id: id });
    assert!(retained.join("SKILL.md").exists());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn cross_volume_park_keeps_complete_copy_on_early_drift_or_returns_io_and_deletes_backup() {
    let home = unique_temp_dir("park_early_drift");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    let original = std::fs::read(live.join("SKILL.md")).unwrap();
    let fs = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
    let rt = runtime_with(&home, &[], fs.clone(), Arc::new(RealProcessSpawner::new()));
    let id = live_copy(&rt, "foo", &live).id;
    fs.fail_next_rename_cross_device();
    let source = live.join("SKILL.md");
    let copy = home.join(".agents/skills-parked/codex/foo/SKILL.md");
    let injected = fs.clone();
    fs.before_stage_create_once(move |_| {
        injected.after_read_once(source.clone(), move || {
            std::fs::write(source, "external edit")
        });
        Ok(())
    });
    let error = ops::park(&rt, &ctx(), &ParkRequest { deployment_id: id }).unwrap_err();
    assert_eq!(
        error.code,
        skill_studio_core::error::ErrorCode::DriftConflict
    );
    assert_eq!(std::fs::read(copy).unwrap(), original);
    assert_eq!(
        std::fs::read(error.path.unwrap().join("SKILL.md")).unwrap(),
        b"external edit"
    );
    assert!(fs.full_sync_calls() > 0);
    std::fs::remove_dir_all(home).unwrap();
}

#[cfg(unix)]
#[test]
fn cross_volume_park_preserves_racing_destination_or_chmods_and_deletes_foreign_skill() {
    use std::os::unix::fs::PermissionsExt;
    let home = unique_temp_dir("park_destination_race");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    let fs = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
    let rt = runtime_with(&home, &[], fs.clone(), Arc::new(RealProcessSpawner::new()));
    let id = live_copy(&rt, "foo", &live).id;
    fs.fail_next_rename_cross_device();
    let copy = home.join(".agents/skills-parked/codex/foo");
    let foreign = copy.clone();
    fs.before_stage_create_once(move |_| {
        write_skill(&foreign, "foreign");
        std::fs::set_permissions(foreign, std::fs::Permissions::from_mode(0o500))
    });
    assert!(ops::park(&rt, &ctx(), &ParkRequest { deployment_id: id }).is_err());
    assert!(live.join("SKILL.md").exists());
    assert!(std::fs::read_to_string(copy.join("SKILL.md"))
        .unwrap()
        .contains("foreign"));
    assert_eq!(
        std::fs::metadata(&copy).unwrap().permissions().mode() & 0o7777,
        0o500
    );
    std::fs::set_permissions(copy, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn cross_volume_park_refuses_cleanup_of_replaced_destination_or_deletes_foreign_tree() {
    let home = unique_temp_dir("park_cleanup_replacement");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    let fs = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
    let rt = runtime_with(&home, &[], fs.clone(), Arc::new(RealProcessSpawner::new()));
    let id = live_copy(&rt, "foo", &live).id;
    fs.fail_next_rename_cross_device();
    let copy = home.join(".agents/skills-parked/codex/foo");
    let foreign = copy.clone();
    let backup = home.join("our-copy");
    fs.after_full_sync_once(move || {
        let foreign = stage_in(foreign.parent().unwrap());
        std::fs::rename(&foreign, backup)?;
        write_skill(&foreign, "foreign");
        Err(std::io::Error::other(
            "sync failed after destination replacement",
        ))
    });
    assert!(ops::park(&rt, &ctx(), &ParkRequest { deployment_id: id }).is_err());
    assert!(live.join("SKILL.md").exists());
    assert!(
        std::fs::read_to_string(stage_in(copy.parent().unwrap()).join("SKILL.md"))
            .unwrap()
            .contains("foreign")
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn cross_volume_park_keeps_failed_copy_or_removes_foreign_file() {
    let home = unique_temp_dir("park_cleanup_foreign_entry");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    let fs = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
    let rt = runtime_with(&home, &[], fs.clone(), Arc::new(RealProcessSpawner::new()));
    let id = live_copy(&rt, "foo", &live).id;
    fs.fail_next_rename_cross_device();
    let copy = home.join(".agents/skills-parked/codex/foo");
    let foreign = copy.join("external.txt");
    let injected = foreign.clone();
    fs.after_full_sync_once(move || {
        let injected = stage_in(injected.parent().unwrap().parent().unwrap()).join("external.txt");
        std::fs::write(injected, "foreign")?;
        Err(std::io::Error::other("sync failed after foreign insertion"))
    });
    assert!(ops::park(&rt, &ctx(), &ParkRequest { deployment_id: id }).is_err());
    let copy = stage_in(copy.parent().unwrap());
    assert_eq!(
        std::fs::read_to_string(copy.join("external.txt")).unwrap(),
        "foreign"
    );
    assert!(copy.join("SKILL.md").exists());
    assert!(live.join("SKILL.md").exists());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn cross_volume_park_keeps_edit_while_hashing_later_file_or_deletes_already_hashed_source() {
    let home = unique_temp_dir("park_hash_race");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    std::fs::write(live.join("notes.txt"), "last file").unwrap();
    let original = std::fs::read(live.join("SKILL.md")).unwrap();
    let fs = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
    let rt = runtime_with(&home, &[], fs.clone(), Arc::new(RealProcessSpawner::new()));
    let id = live_copy(&rt, "foo", &live).id;
    fs.fail_next_rename_cross_device();
    let injected = fs.clone();
    let read_path = live.join("notes.txt");
    let source = live.clone();
    let edit = move || {
        std::fs::create_dir_all(&source)?;
        std::fs::write(source.join("SKILL.md"), "external edit")
    };
    fs.after_full_sync_once(move || {
        injected.after_read_once(read_path, edit);
        Ok(())
    });
    let injected = fs.clone();
    let parent = live.parent().unwrap().to_path_buf();
    let source = live.clone();
    fs.after_rename_once(live.clone(), move || {
        let quarantine = std::fs::read_dir(parent)?
            .map(Result::unwrap)
            .find(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".park-quarantine-")
            });
        if let Some(quarantine) = quarantine {
            injected.after_read_once(quarantine.path().join("notes.txt"), move || {
                std::fs::create_dir_all(&source)?;
                std::fs::write(source.join("SKILL.md"), "external edit")
            });
        }
        Ok(())
    });
    let error = ops::park(&rt, &ctx(), &ParkRequest { deployment_id: id })
        .expect_err("Park deleted a file edited while hashing a later file");
    assert_eq!(
        error.code,
        skill_studio_core::error::ErrorCode::DriftConflict
    );
    assert_eq!(
        std::fs::read(home.join(".agents/skills-parked/codex/foo/SKILL.md")).unwrap(),
        original
    );
    let kept = if live.exists() {
        live
    } else {
        error.path.unwrap()
    };
    assert_eq!(
        std::fs::read(kept.join("SKILL.md")).unwrap(),
        b"external edit"
    );
    std::fs::remove_dir_all(home).unwrap();
}

fn quarantine_in(parent: &Path) -> PathBuf {
    std::fs::read_dir(parent)
        .unwrap()
        .map(Result::unwrap)
        .find(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".park-quarantine-")
        })
        .expect("retained source backup is missing")
        .path()
}

#[test]
fn cross_volume_park_keeps_open_handle_edit_after_hash_or_deletes_latest_bytes() {
    use std::io::{Seek, SeekFrom, Write};
    let home = unique_temp_dir("park_open_handle");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    std::fs::write(live.join("notes.txt"), "last file").unwrap();
    let mut writer = std::fs::OpenOptions::new()
        .write(true)
        .open(live.join("SKILL.md"))
        .unwrap();
    let fs = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
    let rt = runtime_with(&home, &[], fs.clone(), Arc::new(RealProcessSpawner::new()));
    let id = live_copy(&rt, "foo", &live).id;
    fs.fail_next_rename_cross_device();
    let injected = fs.clone();
    let parent = live.parent().unwrap().to_path_buf();
    let backup_parent = parent.clone();
    fs.after_rename_once(live, move || {
        let backup = quarantine_in(&parent);
        injected.after_read_once(backup.join("notes.txt"), move || {
            writer.seek(SeekFrom::Start(0))?;
            writer.write_all(b"latest bytes")?;
            writer.set_len(12)
        });
        Ok(())
    });
    let outcome = ops::park(&rt, &ctx(), &ParkRequest { deployment_id: id }).unwrap();
    assert_eq!(outcome.retained_copies.len(), 1);
    assert_eq!(
        outcome.retained_copies[0].path,
        quarantine_in(&backup_parent)
    );
    assert!(outcome.retained_copies[0].bytes > 0);
    assert_eq!(
        std::fs::read(quarantine_in(&backup_parent).join("SKILL.md")).unwrap(),
        b"latest bytes"
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn cross_volume_park_reports_quarantine_after_interruption_or_hides_latest_bytes() {
    use skill_studio_core::ports::{HistoryAccess, HistoryOpener};
    let home = unique_temp_dir("park_quarantine_interruption");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    let fs = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
    let mut rt = runtime_with(&home, &[], fs.clone(), Arc::new(RealProcessSpawner::new()));
    let sink = Arc::new(RecordingSink::default());
    rt.ports.sink = sink.clone();
    let id = live_copy(&rt, "foo", &live).id;
    fs.fail_next_rename_cross_device();
    let parent = live.parent().unwrap().to_path_buf();
    let check_parent = parent.clone();
    let db = home.join(".history/events.sqlite3");
    let scope = rt.scope.clone();
    fs.after_rename_once(live, move || {
        let store = SqliteHistoryOpener::new(db)
            .open(&scope, HistoryAccess::ReadWrite)
            .unwrap()
            .unwrap();
        let payload = store.pending().unwrap().pop().unwrap().payload;
        assert_eq!(
            payload["quarantines"][1]["path"],
            quarantine_in(&parent).to_string_lossy().as_ref()
        );
        assert_eq!(payload["quarantines"][1]["phase"], "retain_before_rename");
        panic!("injected interruption after quarantine rename");
    });
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ops::park(
            &rt,
            &ctx(),
            &ParkRequest { deployment_id: id }
        )))
        .is_err()
    );
    let guard =
        skill_studio_core::ports::acquire_exclusive(rt.ports.leases.as_ref(), &rt.scope).unwrap();
    let mut store = rt
        .ports
        .history
        .open(&rt.scope, HistoryAccess::ReadWrite)
        .unwrap()
        .unwrap();
    let report = skill_studio_core::events::recover_interrupted(
        &guard,
        store.as_mut(),
        fs.as_ref(),
        rt.ports.sink.as_ref(),
    )
    .unwrap();
    let backup = quarantine_in(&check_parent);
    assert_eq!(report.retained_quarantines, vec![backup.clone()]);
    assert!(sink.notices().iter().any(|notice| matches!(notice,
        skill_studio_core::ports::CoreNotice::Recovered { retained_quarantines, .. }
        if retained_quarantines == &vec![backup.clone()])));
    let repeated = skill_studio_core::events::recover_interrupted(
        &guard,
        store.as_mut(),
        fs.as_ref(),
        &skill_studio_host::NoopSink,
    )
    .unwrap();
    assert_eq!(repeated.retained_quarantines, vec![backup.clone()]);
    store
        .acknowledge_recovery(&guard, &repeated.interrupted)
        .unwrap();
    let acknowledged = skill_studio_core::events::recover_interrupted(
        &guard,
        store.as_mut(),
        fs.as_ref(),
        &skill_studio_host::NoopSink,
    )
    .unwrap();
    assert!(acknowledged.interrupted.is_empty());
    assert!(backup.join("SKILL.md").is_file());
    let rows = store
        .list(&skill_studio_core::events::EventFilter {
            limit: 10,
            ..Default::default()
        })
        .unwrap();
    assert!(rows.iter().any(|row| row.status
        == skill_studio_core::events::EventStatus::Interrupted
        && row.payload["quarantines"][1]["path"] == backup.to_string_lossy().as_ref()));
    drop(store);
    drop(guard);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn cross_volume_park_keeps_file_replaced_before_capture_or_deletes_foreign_inode() {
    destination_cleanup_race(false);
}

#[test]
fn cross_volume_park_keeps_file_replaced_after_identity_check_or_deletes_foreign_inode() {
    destination_cleanup_race(true);
}

fn destination_cleanup_race(after_identity: bool) {
    let home = unique_temp_dir("park_destination_identity_race");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    let fs = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
    let rt = runtime_with(&home, &[], fs.clone(), Arc::new(RealProcessSpawner::new()));
    let id = live_copy(&rt, "foo", &live).id;
    fs.fail_next_rename_cross_device();
    let injected = fs.clone();
    let old = home.join("original-copy");
    fs.before_stage_create_once(move |stage| {
        let copy = stage.join("SKILL.md");
        let replacement = copy.clone();
        let replace = move || {
            std::fs::rename(&replacement, old)?;
            std::fs::write(replacement, "foreign inode")
        };
        if after_identity {
            injected.after_full_sync_once(move || {
                replace()?;
                Err(std::io::Error::other("sync failure after replacement"))
            });
        } else {
            injected.after_new_file_once(copy, replace);
        }
        Ok(())
    });
    assert!(ops::park(&rt, &ctx(), &ParkRequest { deployment_id: id }).is_err());
    let copy = stage_in(&home.join(".agents/skills-parked/codex")).join("SKILL.md");
    assert_eq!(std::fs::read(copy).unwrap(), b"foreign inode");
    assert!(live.join("SKILL.md").exists());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn turn_on_retains_hidden_stage_on_copy_failure_or_exposes_incomplete_live_skill() {
    let home = unique_temp_dir("unpark_stage_failure");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    std::fs::write(live.join("z-run.sh"), "#!/bin/sh\necho kept\n").unwrap();
    let fs = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
    let rt = runtime_with(&home, &[], fs.clone(), Arc::new(RealProcessSpawner::new()));
    park_copy_at(&rt, "foo", &live);
    let parked = only_parked_copy(&rt, "foo");
    let id = parked.id;
    fs.fail_next_rename_cross_device();
    let injected = fs.clone();
    fs.before_stage_create_once(move |stage| {
        injected.fail_read_when_present(parked.path.join("z-run.sh"), stage.join("SKILL.md"));
        Ok(())
    });
    let error = ops::unpark(&rt, &ctx(), &UnparkRequest { deployment_id: id }).unwrap_err();
    assert!(!live.exists());
    let stage = stage_in(live.parent().unwrap());
    assert!(stage.join("SKILL.md").is_file());
    assert!(!stage.join("z-run.sh").exists());
    assert!(error.message.contains(stage.to_str().unwrap()));
    let recovery_ctx = ctx();
    let session = skill_studio_core::ports::MutationSession::begin(&rt, &recovery_ctx).unwrap();
    assert!(recovery_ctx
        .recovery
        .lock()
        .unwrap()
        .retained_quarantines
        .contains(&stage));
    drop(session);
    std::fs::remove_dir_all(home).unwrap();
}

fn stage_in(parent: &std::path::Path) -> std::path::PathBuf {
    std::fs::read_dir(parent)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".park-stage-")
        })
        .unwrap()
}

#[test]
fn turn_on_reports_stage_after_crash_before_publish_or_exposes_partial_skill() {
    let home = unique_temp_dir("unpark_stage_crash");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    let fs = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
    let rt = runtime_with(&home, &[], fs.clone(), Arc::new(RealProcessSpawner::new()));
    park_copy_at(&rt, "foo", &live);
    let id = only_parked_copy(&rt, "foo").id;
    fs.fail_next_rename_cross_device();
    fs.after_full_sync_once(|| panic!("injected interruption before publish"));
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ops::unpark(
            &rt,
            &ctx(),
            &UnparkRequest { deployment_id: id }
        )))
        .is_err()
    );
    assert!(!live.exists());
    let stage = stage_in(live.parent().unwrap());
    let context = ctx();
    let session = skill_studio_core::ports::MutationSession::begin(&rt, &context).unwrap();
    assert!(context
        .recovery
        .lock()
        .unwrap()
        .retained_quarantines
        .contains(&stage));
    assert!(session
        .fresh
        .skills
        .iter()
        .flat_map(|skill| &skill.deployments)
        .all(|copy| copy.path != stage));
    drop(session);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn park_retries_stage_collision_and_keeps_foreign_bytes_or_blocks_valid_move() {
    let home = unique_temp_dir("park_stage_collision");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    let fs = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
    let rt = runtime_with(&home, &[], fs.clone(), Arc::new(RealProcessSpawner::new()));
    let id = live_copy(&rt, "foo", &live).id;
    fs.fail_next_rename_cross_device();
    fs.before_stage_create_once(|stage| {
        std::fs::create_dir(stage)?;
        std::fs::write(stage.join("SKILL.md"), "foreign bytes")
    });
    let outcome = ops::park(&rt, &ctx(), &ParkRequest { deployment_id: id }).unwrap();
    assert!(outcome.parked_path.join("SKILL.md").is_file());
    assert_eq!(outcome.retained_copies.len(), 1);
    assert_eq!(
        std::fs::read(stage_in(outcome.parked_path.parent().unwrap()).join("SKILL.md")).unwrap(),
        b"foreign bytes"
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn turn_on_rejects_stage_replacement_during_sync_or_publishes_foreign_skill() {
    let home = unique_temp_dir("unpark_stage_replacement");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    let fs = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
    let rt = runtime_with(&home, &[], fs.clone(), Arc::new(RealProcessSpawner::new()));
    park_copy_at(&rt, "foo", &live);
    let parked = only_parked_copy(&rt, "foo");
    let parent = live.parent().unwrap().to_path_buf();
    let verified = home.join("verified-stage");
    fs.fail_next_rename_cross_device();
    fs.after_full_sync_once(move || {
        let stage = stage_in(&parent);
        std::fs::rename(&stage, verified)?;
        std::fs::create_dir(&stage)?;
        std::fs::write(stage.join("SKILL.md"), "foreign bytes")
    });
    let context = ctx();
    let result = ops::unpark(
        &rt,
        &context,
        &UnparkRequest {
            deployment_id: parked.id,
        },
    );
    assert_eq!(
        result.as_ref().unwrap_err().code,
        skill_studio_core::error::ErrorCode::DriftConflict
    );
    assert!(!live.exists());
    assert!(parked.path.join("SKILL.md").is_file());
    let envelope = skill_studio_core::ops::ResultEnvelope::from_result(
        skill_studio_core::ops::Operation::Unpark,
        &rt.scope,
        &context,
        result,
    );
    assert!(envelope
        .recovery
        .retained_copies
        .iter()
        .any(|copy| copy.path == stage_in(live.parent().unwrap()) && copy.bytes == 13));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn park_recovery_keeps_notice_before_stage_journal_or_loses_legacy_interruption() {
    use skill_studio_core::ports::HistoryAccess;
    let home = unique_temp_dir("park_before_stage_crash");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    let fs = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
    let rt = runtime_with(&home, &[], fs.clone(), Arc::new(RealProcessSpawner::new()));
    let id = live_copy(&rt, "foo", &live).id;
    fs.before_create_once(home.join(".agents/skills-parked/codex"), || {
        panic!("injected interruption before staging")
    });
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ops::park(
            &rt,
            &ctx(),
            &ParkRequest { deployment_id: id }
        )))
        .is_err()
    );
    let guard =
        skill_studio_core::ports::acquire_exclusive(rt.ports.leases.as_ref(), &rt.scope).unwrap();
    let mut store = rt
        .ports
        .history
        .open(&rt.scope, HistoryAccess::ReadWrite)
        .unwrap()
        .unwrap();
    assert!(store.pending().unwrap()[0]
        .payload
        .get("quarantines")
        .is_none());
    for _ in 0..2 {
        let report = skill_studio_core::events::recover_interrupted(
            &guard,
            store.as_mut(),
            fs.as_ref(),
            &skill_studio_host::NoopSink,
        )
        .unwrap();
        assert_eq!(report.interrupted.len(), 1);
        assert!(report.retained_quarantines.contains(&live));
    }
    drop(store);
    drop(guard);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn park_drift_after_publication_then_turn_on_restores_opencode_folder_and_links_or_loses_origin() {
    let home = unique_temp_dir("park_drift_turn_on");
    let live = home.join(".config/opencode/skill/foo");
    write_skill(&live, "foo");
    let link = home.join(".claude/skills/foo");
    std::fs::create_dir_all(link.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink("../../.config/opencode/skill/foo", &link).unwrap();
    let fs = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
    let mut rt = runtime_with(&home, &[], fs.clone(), Arc::new(RealProcessSpawner::new()));
    // Model a catalog that still owns OpenCode's v1 skill root.
    for root in Arc::make_mut(&mut rt.ports.catalog)
        .facts
        .iter_mut()
        .flat_map(|facts| &mut facts.roots)
    {
        if root.relative_path == ".config/opencode/skill" {
            root.role = skill_studio_core::harness::RootRole::Own;
        }
    }
    let id = live_copy(&rt, "foo", &live).id;
    fs.fail_next_rename_cross_device();
    let edited = live.join("SKILL.md");
    fs.after_full_sync_once(move || std::fs::write(edited, "external edit"));
    let error = ops::park(&rt, &ctx(), &ParkRequest { deployment_id: id }).unwrap_err();
    assert_eq!(
        error.code,
        skill_studio_core::error::ErrorCode::DriftConflict
    );
    let quarantine = error.path.unwrap();
    ops::unpark(
        &rt,
        &ctx(),
        &UnparkRequest {
            deployment_id: only_parked_copy(&rt, "foo").id,
        },
    )
    .unwrap();
    assert!(live.join("SKILL.md").is_file());
    assert!(!home.join(".config/opencode/skills/foo").exists());
    assert_eq!(
        std::fs::read_link(&link).unwrap(),
        PathBuf::from("../../.config/opencode/skill/foo")
    );
    assert!(link.join("SKILL.md").is_file());
    assert_eq!(
        std::fs::read_to_string(quarantine.join("SKILL.md")).unwrap(),
        "external edit"
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn turn_on_cross_volume_full_syncs_published_destination_before_quarantine_or_hides_skill_on_power_loss(
) {
    use std::sync::atomic::{AtomicBool, Ordering};
    let home = unique_temp_dir("turn_on_publish_sync");
    let live = home.join(".codex/skills/foo");
    write_skill(&live, "foo");
    let fs = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
    let rt = runtime_with(&home, &[], fs.clone(), Arc::new(RealProcessSpawner::new()));
    let parked = park_copy_at(&rt, "foo", &live);
    let synced = Arc::new(AtomicBool::new(false));
    let checked = synced.clone();
    let injected = fs.clone();
    let destination = live.clone();
    fs.after_full_sync_once(move || {
        injected.after_full_sync_once(move || {
            assert!(
                destination.join("SKILL.md").is_file(),
                "full sync preceded publication"
            );
            checked.store(true, Ordering::SeqCst);
            Ok(())
        });
        Ok(())
    });
    fs.before_rename_once(parked, move || {
        assert!(
            synced.load(Ordering::SeqCst),
            "quarantine preceded destination full sync"
        );
        Ok(())
    });
    fs.fail_next_rename_cross_device();
    ops::unpark(
        &rt,
        &ctx(),
        &UnparkRequest {
            deployment_id: only_parked_copy(&rt, "foo").id,
        },
    )
    .unwrap();
    assert!(live.join("SKILL.md").is_file());
    std::fs::remove_dir_all(home).unwrap();
}
