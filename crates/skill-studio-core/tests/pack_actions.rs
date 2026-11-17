#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Real-disk tests for `pack_add`, `pack_update`, and `pack_remove`: each
//! plans first, and a real run hands each skill to the per-skill op.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use skill_studio_core::dto::ListEventsRequest;
use skill_studio_core::harness::HarnessCatalog;
use skill_studio_core::identity::AgentId;
use skill_studio_core::pack::{PackRemote, RemotePack, RemoteSkill};
use skill_studio_core::pack_action::{
    PackActionReport, PackAddRequest, PackRemoveRequest, PackStepState, PackUpdateRequest,
};
use skill_studio_core::ports::{
    CancelToken, Ports, ProcessOutput, ProcessSpawner, ProcessSpec, Runtime,
};
use skill_studio_core::scope::RuntimeScope;
use skill_studio_core::testing::golden::{ctx, unique_temp_dir};
use skill_studio_core::testing::{FakeClock, FakeIds, RecordingSink};
use skill_studio_core::{ops, ErrorCode, OpStatus, Outcome};
use skill_studio_host::{FileLease, RealFs, SqliteHistoryOpener};

const EDITED_TREE: &str = "0000000000000000000000000000000000000000";

/// Stands in for `npx skills`: `add` writes the skill, `remove` deletes it and
/// its lock row, `update` succeeds or, when told, exits 1.
struct FakeSkillsCli {
    home: PathBuf,
    calls: Mutex<Vec<Vec<String>>>,
    fail_updates: AtomicBool,
}

impl FakeSkillsCli {
    fn new(home: &Path) -> Arc<Self> {
        Arc::new(FakeSkillsCli {
            home: home.to_path_buf(),
            calls: Mutex::new(Vec::new()),
            fail_updates: AtomicBool::new(false),
        })
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }
}

impl ProcessSpawner for FakeSkillsCli {
    fn run(
        &self,
        spec: &ProcessSpec,
        _cancel: &dyn CancelToken,
    ) -> Result<ProcessOutput, skill_studio_core::CoreError> {
        self.calls.lock().unwrap().push(spec.args.clone());
        let after = |flag: &str| {
            let at = spec.args.iter().position(|a| a == flag)?;
            spec.args.get(at + 1).cloned()
        };
        let cwd = spec.cwd.clone().unwrap_or_else(|| self.home.clone());
        let mut status = 0;
        if let Some(skill) = after("--skill") {
            write_skill(&cwd, &skill);
        } else if let Some(skill) = after("remove") {
            std::fs::remove_dir_all(cwd.join(".agents/skills").join(&skill)).ok();
            let lock = cwd.join(".agents/.skill-lock.json");
            let mut doc: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&lock).unwrap()).unwrap();
            doc["skills"].as_object_mut().unwrap().shift_remove(&skill);
            std::fs::write(&lock, serde_json::to_vec(&doc).unwrap()).unwrap();
        } else if self.fail_updates.load(Ordering::SeqCst) {
            status = 1;
        }
        Ok(ProcessOutput {
            status: Some(status),
            stdout: String::new(),
            stderr: if status == 0 { "" } else { "update failed" }.to_string(),
            timed_out: false,
        })
    }
}

struct FakeRemote(Vec<&'static str>);

impl PackRemote for FakeRemote {
    fn fetch(&self, url: &str, _: Option<&str>, _: &dyn CancelToken) -> Result<RemotePack, String> {
        assert_eq!(url, "https://github.com/owner/pack.git");
        Ok(RemotePack {
            commit: "abc".to_string(),
            skills: self
                .0
                .iter()
                .map(|name| RemoteSkill {
                    dir: format!("skills/{name}"),
                    name: (*name).to_string(),
                    tree_hash: String::new(),
                    folder_hash: String::new(),
                })
                .collect(),
        })
    }
}

fn write_skill(root: &Path, skill: &str) {
    let dir = root.join(".agents/skills").join(skill);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {skill}\ndescription: A pack skill.\n---\nBody.\n"),
    )
    .unwrap();
}

/// Installs `skill` as `npx skills add owner/pack` would leave it. A 40-char
/// `hash` that is not the folder's tree marks it edited; a short one leaves
/// the edit state unknown.
fn installed_from_pack(home: &Path, skill: &str, hash: &str) {
    write_skill(home, skill);
    let lock = home.join(".agents/.skill-lock.json");
    let mut doc: serde_json::Value = std::fs::read(&lock)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_else(|| serde_json::json!({"version": 3, "skills": {}}));
    doc["skills"][skill] = serde_json::json!({
        "source": "owner/pack",
        "sourceType": "github",
        "sourceUrl": "https://github.com/owner/pack.git",
        "skillPath": format!("skills/{skill}/SKILL.md"),
        "skillFolderHash": hash,
        "installedAt": "2026-01-01T00:00:00Z",
        "updatedAt": "2026-01-01T00:00:00Z",
    });
    std::fs::write(&lock, serde_json::to_vec(&doc).unwrap()).unwrap();
}

fn runtime(home: &Path, cli: Arc<FakeSkillsCli>) -> Runtime {
    let ports = Ports {
        fs: Arc::new(RealFs::new()),
        clock: Arc::new(FakeClock::at(0)),
        ids: Arc::new(FakeIds::default()),
        leases: Arc::new(FileLease::new(home.join(".leases"))),
        history: Arc::new(SqliteHistoryOpener::new(
            home.join(".history/events.sqlite3"),
        )),
        sink: Arc::new(RecordingSink::default()),
        spawner: Some(cli),
        discovery: None,
        tools: None,
        catalog: Arc::new(HarnessCatalog::builtin()),
        telemetry: Arc::new(skill_studio_core::ports::NoopTelemetry),
    };
    Runtime::new(&RuntimeScope::fixture(home), ports).unwrap()
}

fn home(label: &str) -> PathBuf {
    let home = unique_temp_dir(label);
    std::fs::create_dir_all(&home).unwrap();
    home
}

fn states(report: &PackActionReport) -> Vec<(String, &'static str)> {
    report
        .steps
        .iter()
        .map(|step| {
            let state = match step.state {
                PackStepState::Planned => "planned",
                PackStepState::Done { .. } => "done",
                PackStepState::Skipped { .. } => "skipped",
                PackStepState::Failed { .. } => "failed",
            };
            (step.skill.clone(), state)
        })
        .collect()
}

fn add(skills: &[&str], ref_pin: Option<&str>, dry_run: bool) -> PackAddRequest {
    PackAddRequest {
        source: "owner/pack".to_string(),
        skills: skills.iter().map(|s| (*s).to_string()).collect(),
        ref_pin: ref_pin.map(String::from),
        dry_run,
        ..PackAddRequest::default()
    }
}

fn update(dry_run: bool) -> PackUpdateRequest {
    PackUpdateRequest {
        source: Some("owner/pack".to_string()),
        dry_run,
        ..PackUpdateRequest::default()
    }
}

#[test]
fn pack_add_dry_run_skips_an_installed_skill_and_runs_no_process() {
    let home = home("pack_add_dry_run");
    write_skill(&home, "alpha");
    let cli = FakeSkillsCli::new(&home);
    let rt = runtime(&home, cli.clone());

    let report = ops::pack_add(&rt, &ctx(), &add(&["alpha", "beta"], None, true), None).unwrap();

    assert_eq!(
        states(&report),
        [("alpha".into(), "skipped"), ("beta".into(), "planned")]
    );
    assert_eq!(report.summary, "Would install 1 skill; 1 skipped.");
    assert!(cli.calls().is_empty(), "a dry run ran {:?}", cli.calls());
    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn pack_add_installs_each_skill_from_the_pinned_source_with_its_own_undo_event() {
    let home = home("pack_add_real");
    let cli = FakeSkillsCli::new(&home);
    let rt = runtime(&home, cli.clone());

    let report = ops::pack_add(
        &rt,
        &ctx(),
        &add(&["beta", "alpha"], Some("v1"), false),
        None,
    )
    .unwrap();

    assert_eq!(
        states(&report),
        [("alpha".into(), "done"), ("beta".into(), "done")]
    );
    assert_eq!(report.status(), OpStatus::Ok);
    let calls = cli.calls();
    assert_eq!(calls.len(), 2);
    for (call, skill) in calls.iter().zip(["alpha", "beta"]) {
        assert!(call.contains(&"owner/pack#v1".to_string()), "{call:?}");
        assert!(call.windows(2).any(|w| w == ["--skill", skill]), "{call:?}");
    }
    let events = ops::list_events(&rt, &ctx(), &ListEventsRequest::default()).unwrap();
    let installs: Vec<_> = events.iter().filter(|e| e.kind == "install").collect();
    assert_eq!(installs.len(), 2, "{events:#?}");
    for step in &report.steps {
        let PackStepState::Done { event_id } = &step.state else {
            unreachable!()
        };
        assert!(installs.iter().any(|e| &e.id == event_id), "{event_id:?}");
    }
    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn pack_add_keeps_the_shared_copy_when_agents_are_named_or_update_and_remove_find_no_copy() {
    let home = home("pack_add_agents");
    let cli = FakeSkillsCli::new(&home);
    let rt = runtime(&home, cli.clone());
    let request = PackAddRequest {
        harnesses: vec![AgentId::from(AgentId::CLAUDE_CODE)],
        ..add(&["alpha"], None, false)
    };

    let report = ops::pack_add(&rt, &ctx(), &request, None).unwrap();

    assert_eq!(states(&report), [("alpha".into(), "done")]);
    let calls = cli.calls();
    let agents: Vec<&str> = calls[0]
        .windows(2)
        .filter(|w| w[0] == "--agent")
        .map(|w| w[1].as_str())
        .collect();
    assert_eq!(agents, ["universal", "claude-code"], "{calls:?}");
    let link = home.join(".claude/skills/alpha");
    assert!(
        std::fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink()),
        "{link:?} is not a link to the shared copy"
    );
    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn pack_add_without_skill_names_lists_them_upstream_or_says_it_cannot() {
    let home = home("pack_add_upstream");
    let rt = runtime(&home, FakeSkillsCli::new(&home));

    let err = ops::pack_add(&rt, &ctx(), &add(&[], None, true), None).unwrap_err();
    assert_eq!(err.code, ErrorCode::Unsupported);

    let remote = FakeRemote(vec!["zeta", "eta", "zeta"]);
    let report = ops::pack_add(&rt, &ctx(), &add(&[], None, true), Some(&remote)).unwrap();
    assert_eq!(
        states(&report),
        [("eta".into(), "planned"), ("zeta".into(), "planned")]
    );
    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn pack_update_dry_run_plans_clean_skills_and_skips_edited_ones() {
    let home = home("pack_update_dry_run");
    installed_from_pack(&home, "clean", "deadbeef");
    installed_from_pack(&home, "edited", EDITED_TREE);
    let cli = FakeSkillsCli::new(&home);
    let rt = runtime(&home, cli.clone());

    let report = ops::pack_update(&rt, &ctx(), &update(true)).unwrap();

    assert_eq!(
        states(&report),
        [("clean".into(), "planned"), ("edited".into(), "skipped")]
    );
    assert!(cli.calls().is_empty(), "a dry run ran {:?}", cli.calls());
    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn pack_update_reports_a_failed_cli_run_as_a_failed_step_and_partial_status() {
    let home = home("pack_update_fails");
    installed_from_pack(&home, "clean", "deadbeef");
    let cli = FakeSkillsCli::new(&home);
    cli.fail_updates.store(true, Ordering::SeqCst);
    let rt = runtime(&home, cli.clone());

    let report = ops::pack_update(&rt, &ctx(), &update(false)).unwrap();

    assert_eq!(states(&report), [("clean".into(), "failed")]);
    assert_eq!(report.status(), OpStatus::Partial);
    assert_eq!(report.errors().len(), 1);
    assert!(
        cli.calls()
            .iter()
            .any(|call| call.windows(2).any(|w| w == ["update", "clean"])),
        "{:?}",
        cli.calls()
    );
    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn pack_update_needs_one_source_or_all() {
    let home = home("pack_update_request");
    let rt = runtime(&home, FakeSkillsCli::new(&home));

    for req in [
        PackUpdateRequest::default(),
        PackUpdateRequest {
            all: true,
            ..update(true)
        },
    ] {
        let err = ops::pack_update(&rt, &ctx(), &req).unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidRequest);
    }
    let err = ops::pack_update(&rt, &ctx(), &update(true)).unwrap_err();
    assert_eq!(err.code, ErrorCode::AmbiguousTarget, "{err:?}");
    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn pack_remove_removes_clean_skills_with_undo_and_keeps_edited_ones() {
    let home = home("pack_remove_real");
    installed_from_pack(&home, "z-clean", "deadbeef");
    installed_from_pack(&home, "edited", EDITED_TREE);
    let cli = FakeSkillsCli::new(&home);
    let rt = runtime(&home, cli.clone());
    let req = |dry_run| PackRemoveRequest {
        source: "owner/pack".to_string(),
        project_path: None,
        dry_run,
    };

    let plan = ops::pack_remove(&rt, &ctx(), &req(true)).unwrap();
    assert_eq!(
        states(&plan),
        [("edited".into(), "skipped"), ("z-clean".into(), "planned")]
    );
    assert!(cli.calls().is_empty());

    let report = ops::pack_remove(&rt, &ctx(), &req(false)).unwrap();
    assert_eq!(
        states(&report),
        [("edited".into(), "skipped"), ("z-clean".into(), "done")]
    );
    assert!(!home.join(".agents/skills/z-clean").exists());
    assert!(home.join(".agents/skills/edited/SKILL.md").exists());
    let events = ops::list_events(&rt, &ctx(), &ListEventsRequest::default()).unwrap();
    assert_eq!(
        events.iter().filter(|e| e.kind == "remove").count(),
        1,
        "{events:#?}"
    );
    std::fs::remove_dir_all(&home).ok();
}
