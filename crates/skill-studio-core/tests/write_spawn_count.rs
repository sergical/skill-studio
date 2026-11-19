#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Spawn-count contract for write jobs (#687): adding or updating several
//! skills from one source costs one `npx skills` call, not one per skill.
//! Each `npx` call fetches the source again (about 1 to 2.5 s), so the count
//! is the cost. Like `scan_work_count.rs`, this counts work, not wall time,
//! so it holds on any runner. Each skill must still get its own journal
//! event, so undo stays per skill.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use skill_studio_core::dto::{
    InstallLinkMode, InstallMethod, InstallOutcome, InstallRequest, ListEventsRequest,
    RestoreRequest,
};
use skill_studio_core::harness::HarnessCatalog;
use skill_studio_core::identity::{AgentId, EventId, RootScope, SkillDestination, SkillName};
use skill_studio_core::install_targets;
use skill_studio_core::pack_action::{
    PackActionReport, PackAddRequest, PackStepState, PackUpdateRequest,
};
use skill_studio_core::ports::{
    CancelToken, Ports, ProcessOutput, ProcessSpawner, ProcessSpec, Runtime,
};
use skill_studio_core::scope::RuntimeScope;
use skill_studio_core::testing::golden::{ctx, unique_temp_dir};
use skill_studio_core::testing::{FakeClock, FakeIds, RecordingSink};
use skill_studio_core::{ops, OpStatus, Outcome};
use skill_studio_host::{FileLease, RealFs, SqliteHistoryOpener};

const SKILLS: [&str; 5] = ["alpha", "bravo", "charlie", "delta", "echo"];

/// Stands in for `npx skills`. `add` writes every `--skill` it names and its
/// lock row; `update` rewrites every name it lists, or exits 1 when told.
struct CountingSkillsCli {
    home: PathBuf,
    calls: Mutex<Vec<Vec<String>>>,
    fail: AtomicBool,
}

impl CountingSkillsCli {
    fn new(home: &Path) -> Arc<Self> {
        Arc::new(Self {
            home: home.to_path_buf(),
            calls: Mutex::new(Vec::new()),
            fail: AtomicBool::new(false),
        })
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }
}

impl ProcessSpawner for CountingSkillsCli {
    fn run(
        &self,
        spec: &ProcessSpec,
        _cancel: &dyn CancelToken,
    ) -> Result<ProcessOutput, skill_studio_core::CoreError> {
        self.calls.lock().unwrap().push(spec.args.clone());
        let status = if self.fail.load(Ordering::SeqCst) {
            1
        } else {
            let root = spec.cwd.clone().unwrap_or_else(|| self.home.clone());
            let verb = spec.args.get(1).map(String::as_str);
            if verb == Some("add") {
                let source = spec.args[2].split('#').next().unwrap().to_string();
                let named = spec.args.windows(2).filter(|w| w[0] == "--skill");
                for pair in named {
                    write_skill(&root, &pair[1], "Added.");
                    lock_row(&root, &pair[1], &source);
                }
            } else if verb == Some("update") {
                let names = spec.args[2..].iter().take_while(|a| !a.starts_with("--"));
                for name in names {
                    write_skill(&root, name, "Updated upstream.");
                }
            }
            0
        };
        Ok(ProcessOutput {
            status: Some(status),
            stdout: String::new(),
            stderr: if status == 0 { "" } else { "fetch failed" }.to_string(),
            timed_out: false,
        })
    }
}

fn write_skill(root: &Path, skill: &str, body: &str) {
    let dir = root.join(".agents/skills").join(skill);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {skill}\ndescription: A pack skill.\n---\n{body}\n"),
    )
    .unwrap();
}

fn lock_row(root: &Path, skill: &str, source: &str) {
    let lock = root.join(".agents/.skill-lock.json");
    let mut doc: serde_json::Value = std::fs::read(&lock)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_else(|| serde_json::json!({"version": 3, "skills": {}}));
    doc["skills"][skill] = serde_json::json!({
        "source": source,
        "sourceType": "github",
        "sourceUrl": format!("https://github.com/{source}.git"),
        "skillPath": format!("skills/{skill}/SKILL.md"),
        "skillFolderHash": "deadbeef",
        "installedAt": "2026-01-01T00:00:00Z",
        "updatedAt": "2026-01-01T00:00:00Z",
    });
    std::fs::write(&lock, serde_json::to_vec(&doc).unwrap()).unwrap();
}

fn runtime(home: &Path, cli: Arc<CountingSkillsCli>) -> Runtime {
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

/// Each step's event id; panics on a step that is not done.
fn done_events(report: &PackActionReport) -> Vec<String> {
    report
        .steps
        .iter()
        .map(|step| match &step.state {
            PackStepState::Done { event_id } => event_id.0.clone(),
            other => panic!("{}: {other:?}", step.skill),
        })
        .collect()
}

fn add_all() -> PackAddRequest {
    PackAddRequest {
        source: "owner/pack".to_string(),
        skills: SKILLS.iter().map(|s| (*s).to_string()).collect(),
        ..PackAddRequest::default()
    }
}

fn update_pack() -> PackUpdateRequest {
    PackUpdateRequest {
        source: Some("owner/pack".to_string()),
        ..PackUpdateRequest::default()
    }
}

#[test]
fn adding_five_skills_from_one_source_spawns_npx_once_and_keeps_one_undo_per_skill() {
    let home = home("spawn_count_add");
    let cli = CountingSkillsCli::new(&home);
    let rt = runtime(&home, cli.clone());

    let report = ops::pack_add(&rt, &ctx(), &add_all(), None).unwrap();

    let calls = cli.calls();
    assert_eq!(calls.len(), 1, "one npx call for one source: {calls:#?}");
    for skill in SKILLS {
        assert!(
            calls[0].windows(2).any(|w| w == ["--skill", skill]),
            "{skill} missing from {:?}",
            calls[0]
        );
    }
    let events = done_events(&report);
    assert_eq!(events.iter().collect::<BTreeSet<_>>().len(), SKILLS.len());

    // Undo of one skill removes that skill only.
    ops::restore_event(
        &rt,
        &ctx(),
        &RestoreRequest {
            event_id: EventId(events[0].clone()),
            force: false,
        },
    )
    .unwrap();
    let left: Vec<String> = std::fs::read_dir(home.join(".agents/skills"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    assert_eq!(left, ["bravo", "charlie", "delta", "echo"]);
    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn updating_five_skills_of_one_pack_spawns_npx_once_and_keeps_one_undo_per_skill() {
    let home = home("spawn_count_update");
    for skill in SKILLS {
        write_skill(&home, skill, "Installed.");
        lock_row(&home, skill, "owner/pack");
    }
    let cli = CountingSkillsCli::new(&home);
    let rt = runtime(&home, cli.clone());

    let report = ops::pack_update(&rt, &ctx(), &update_pack()).unwrap();

    let calls = cli.calls();
    assert_eq!(calls.len(), 1, "one npx call for one scope: {calls:#?}");
    let events = done_events(&report);
    assert_eq!(events.iter().collect::<BTreeSet<_>>().len(), SKILLS.len());
    let journal = ops::list_events(&rt, &ctx(), &ListEventsRequest::default()).unwrap();
    assert_eq!(
        journal.iter().filter(|e| e.kind == "update").count(),
        SKILLS.len()
    );

    // Undo of one skill puts back that skill's old body only.
    ops::restore_event(
        &rt,
        &ctx(),
        &RestoreRequest {
            event_id: EventId(events[0].clone()),
            force: false,
        },
    )
    .unwrap();
    let body = |skill: &str| {
        std::fs::read_to_string(home.join(".agents/skills").join(skill).join("SKILL.md")).unwrap()
    };
    assert!(body("alpha").contains("Installed."), "{}", body("alpha"));
    assert!(
        body("bravo").contains("Updated upstream."),
        "{}",
        body("bravo")
    );
    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn a_failed_batched_update_fails_every_skill_in_it_and_spawns_once() {
    let home = home("spawn_count_update_fails");
    for skill in SKILLS {
        write_skill(&home, skill, "Installed.");
        lock_row(&home, skill, "owner/pack");
    }
    let cli = CountingSkillsCli::new(&home);
    cli.fail.store(true, Ordering::SeqCst);
    let rt = runtime(&home, cli.clone());

    let report = ops::pack_update(&rt, &ctx(), &update_pack()).unwrap();

    assert_eq!(cli.calls().len(), 1, "{:#?}", cli.calls());
    assert_eq!(report.status(), OpStatus::Partial);
    assert_eq!(report.errors().len(), SKILLS.len());
    std::fs::remove_dir_all(&home).ok();
}

/// `install_batch` of skills that save the install preference writes it
/// once, and the first skill's undo puts the old preference back, as a run
/// of single installs would. Fails when the preference is not saved, or
/// when no row's undo removes it.
#[test]
fn a_batched_install_saves_the_preference_once_and_the_first_undo_removes_it() {
    let home = home("spawn_count_preference");
    let cli = CountingSkillsCli::new(&home);
    let rt = runtime(&home, cli.clone());
    let requests: Vec<InstallRequest> = ["alpha", "bravo"]
        .iter()
        .map(|name| InstallRequest {
            skill: SkillName((*name).to_string()),
            method: InstallMethod::SkillsSh,
            scope: RootScope::Global,
            harnesses: vec![AgentId::from(install_targets::UNIVERSAL_TARGET)],
            link_mode: InstallLinkMode::Link,
            destination: SkillDestination::Universal,
            files: Vec::new(),
            source: Some("owner/pack".to_string()),
            trust_identity: None,
            trust_confirmed: false,
            save_as_preference: true,
        })
        .collect();

    let outcomes = ops::install_batch(&rt, &ctx(), &requests);

    assert_eq!(cli.calls().len(), 1, "{:#?}", cli.calls());
    let preference = || {
        std::fs::read(home.join(".agents/skill-studio.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .map(|doc| doc["preferred_method"].clone())
            .unwrap_or(serde_json::Value::Null)
    };
    assert_eq!(preference(), serde_json::json!("skills_sh"));
    let first = match &outcomes[0] {
        Ok(InstallOutcome::Installed { event_id, .. }) => event_id.clone(),
        other => panic!("{other:?}"),
    };
    ops::restore_event(
        &rt,
        &ctx(),
        &RestoreRequest {
            event_id: first,
            force: false,
        },
    )
    .unwrap();
    assert_eq!(preference(), serde_json::Value::Null);
    std::fs::remove_dir_all(&home).ok();
}
