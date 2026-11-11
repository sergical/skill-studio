use std::path::{Path, PathBuf};
use std::sync::Arc;

use skill_studio_core::dto::{
    InstallFile, InstallMethod, InstallRequest, ParkRequest, RemoveRequest, ScanRequest,
    UnparkRequest, UpdateRequest,
};
use skill_studio_core::identity::{DeploymentId, EventId, RootKind, RootScope, SkillName};
use skill_studio_core::ops;
use skill_studio_core::ports::{CancelToken, ProcessOutput, ProcessSpawner, ProcessSpec, Runtime};
use skill_studio_core::scope::{ProjectSelection, RuntimeScope};
use skill_studio_core::testing::{golden::ctx, FailingFs};
use skill_studio_host::RealFs;

pub fn runtime(home: &Path) -> Runtime {
    let data = home.join(".skill-studio");
    let mut scope = RuntimeScope::live(home.to_path_buf(), data.join("history"));
    scope.projects = ProjectSelection::Explicit { paths: Vec::new() };
    let ports = skill_studio_host::default_ports_with_history(
        data.join("leases"),
        Arc::new(skill_studio_core::harness::HarnessCatalog::builtin()),
        data.join("history/events.sqlite3"),
    );
    Runtime::new(&scope, ports).unwrap()
}

fn write_skill(home: &Path, name: &str, body: &str) {
    let dir = home.join(".agents/skills").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: An Undo fixture.\n---\n{body}\n"),
    )
    .unwrap();
}

fn deployment(rt: &Runtime, kind: &RootKind) -> DeploymentId {
    ops::scan(rt, &ctx(), &ScanRequest::default())
        .unwrap()
        .skills
        .into_iter()
        .find(|skill| skill.name.0 == "alpha")
        .unwrap()
        .deployments
        .into_iter()
        .find(|d| &d.root.kind == kind)
        .unwrap()
        .id
}

struct FixtureCli(PathBuf);
impl ProcessSpawner for FixtureCli {
    fn run(
        &self,
        spec: &ProcessSpec,
        _: &dyn CancelToken,
    ) -> Result<ProcessOutput, skill_studio_core::CoreError> {
        assert_eq!(spec.program, "npx");
        if spec.args.iter().any(|arg| arg == "remove") {
            std::fs::remove_dir_all(self.0.join(".agents/skills/alpha")).unwrap();
            std::fs::remove_dir_all(self.0.join(".cursor/skills/alpha")).unwrap();
            std::fs::write(
                self.0.join(".agents/.skill-lock.json"),
                r#"{"version":3,"skills":{}}"#,
            )
            .unwrap();
        } else {
            assert!(spec.args.iter().any(|arg| arg == "install"));
            write_skill(&self.0, "alpha", "after update");
            write_skill(&self.0, "gamma", "created by update");
            std::fs::write(self.0.join(".agents/agents.lock"), "# after update\n").unwrap();
        }
        Ok(ProcessOutput {
            status: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            timed_out: false,
        })
    }
}

pub fn removed_with_secondary_replacement(home: &Path) -> EventId {
    write_skill(home, "alpha", "before remove");
    std::fs::write(home.join(".agents/.skill-lock.json"),r#"{"version":3,"skills":{"alpha":{"source":"owner/alpha","sourceType":"github","sourceUrl":"https://github.com/owner/alpha","skillFolderHash":"deadbeef","installedAt":"2026-11-01T00:00:00Z","updatedAt":"2026-11-01T00:00:00Z"}}}"#).unwrap();
    let cursor = home.join(".cursor/skills/alpha");
    std::fs::create_dir_all(&cursor).unwrap();
    std::fs::write(cursor.join("old.txt"), "before remove\n").unwrap();
    let mut rt = runtime(home);
    rt.ports.spawner = Some(Arc::new(FixtureCli(home.to_path_buf())));
    let deployment_id = deployment(&rt, &RootKind::Universal);
    let removed = ops::remove(&rt, &ctx(), &RemoveRequest { deployment_id }).unwrap();
    std::fs::create_dir_all(&cursor).unwrap();
    std::fs::write(cursor.join("replacement.txt"), "after remove\n").unwrap();
    removed.event_id
}

pub fn assert_secondary_replacement(home: &Path) {
    let cursor = home.join(".cursor/skills/alpha");
    assert_eq!(
        std::fs::read(cursor.join("replacement.txt")).unwrap(),
        b"after remove\n"
    );
    assert!(!cursor.join("old.txt").exists());
    assert!(!home.join(".agents/skills/alpha").exists());
}

pub fn updated_with_unreadable_created_folder(home: &Path) -> EventId {
    write_skill(home, "alpha", "before update");
    std::fs::write(
        home.join(".agents/agents.toml"),
        "version = 1\n[[skills]]\nname = \"alpha\"\nsource = \"o/r\"\nref = \"aaa\"\n",
    )
    .unwrap();
    std::fs::write(home.join(".agents/agents.lock"), "# before update\n").unwrap();
    let failing = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
    failing.fail_read_dir_for(home.join(".agents/skills/gamma"));
    let mut rt = runtime(home);
    rt.ports.fs = failing;
    rt.ports.spawner = Some(Arc::new(FixtureCli(home.to_path_buf())));
    ops::update(
        &rt,
        &ctx(),
        &UpdateRequest {
            skill: SkillName("alpha".into()),
            method: InstallMethod::Dotagents,
            scope: RootScope::Global,
            files: Vec::new(),
            source: Some("o/r".into()),
            ref_pin: Some("bbb".into()),
        },
    )
    .unwrap()
    .event_id
}

pub fn parked_after_install(home: &Path, unpark: bool) -> EventId {
    let rt = runtime(home);
    ops::install(
        &rt,
        &ctx(),
        &InstallRequest {
            skill: SkillName("alpha".into()),
            method: InstallMethod::Copy,
            scope: RootScope::Global,
            harnesses: Vec::new(),
            files: vec![InstallFile {
                relative_path: PathBuf::from("SKILL.md"),
                contents:
                    b"---\nname: alpha\ndescription: A copied skill.\n---\nKeep this skill.\n"
                        .to_vec(),
                mode: None,
            }],
            source: None,
            trust_identity: None,
            trust_confirmed: false,
            save_as_preference: false,
            link_mode: skill_studio_core::dto::InstallLinkMode::Link,
            destination: skill_studio_core::identity::SkillDestination::Universal,
        },
    )
    .unwrap();
    let parked = ops::park(
        &rt,
        &ctx(),
        &ParkRequest {
            deployment_id: deployment(&rt, &RootKind::Universal),
        },
    )
    .unwrap();
    if unpark {
        ops::unpark(
            &rt,
            &ctx(),
            &UnparkRequest {
                deployment_id: deployment(&rt, &RootKind::Parked),
            },
        )
        .unwrap()
        .event_id
    } else {
        parked.event_id
    }
}

pub fn expired_backup(home: &Path) -> (EventId, Vec<u8>) {
    use skill_studio_core::backup_retention::{
        BackupRetentionScope, BACKUP_RETENTION_AGE, BACKUP_RETENTION_COUNT,
    };
    use skill_studio_core::events::{EventDraft, EventKind, EventStatus};
    use skill_studio_core::ports::{acquire_exclusive, HistoryAccess};
    let rt = runtime(home);
    let guard = acquire_exclusive(rt.ports.leases.as_ref(), &rt.scope).unwrap();
    let mut store = rt
        .ports
        .history
        .open(&rt.scope, HistoryAccess::ReadWrite)
        .unwrap()
        .unwrap();
    let target = home.join(".agents/skills/alpha");
    let mut ids = Vec::new();
    for index in 0..BACKUP_RETENTION_COUNT + 2 {
        write_skill(home, "alpha", &format!("version {index}"));
        let id = EventId::from_ulid(ulid::Ulid::generate());
        let manifest = store
            .backup_paths(&guard, &id, std::slice::from_ref(&target))
            .unwrap();
        let fingerprint = manifest.entries[0].fingerprint.as_ref().unwrap().bare_hex();
        store.record(&guard, &id, &EventDraft {
            kind:EventKind::Update, skill:SkillName("alpha".into()), harness:None, scope:Some("global".into()), project_path:None,
            payload:serde_json::json!({}), inverse:Some(serde_json::json!({"op":"restore_backup","path":target,"pre_fingerprint":fingerprint,"post_fingerprint":fingerprint})), backup_dir:Some(manifest.backup_dir),
        }).unwrap();
        store.finish(&guard, &id, EventStatus::Done, None).unwrap();
        ids.push(id);
    }
    store
        .retain_backups(
            &guard,
            chrono::Utc::now() + BACKUP_RETENTION_AGE + chrono::Duration::days(1),
            BackupRetentionScope::All,
        )
        .unwrap();
    (
        ids.remove(0),
        std::fs::read(target.join("SKILL.md")).unwrap(),
    )
}

pub fn cli_binary() -> PathBuf {
    std::env::var_os("SKILL_STUDIO_TEST_BINARY").map_or_else(
        || PathBuf::from(env!("CARGO_BIN_EXE_skill-studio")),
        PathBuf::from,
    )
}

pub fn interrupted_plan_before_restore(home: &Path, foreign: &Path) -> EventId {
    use skill_studio_core::ports::{acquire_exclusive, Journal, PlanRecord, PlanStatus, PlanStep};
    let event = removed_with_secondary_replacement(home);
    let rt = runtime(home);
    let guard = acquire_exclusive(rt.ports.leases.as_ref(), &rt.scope).unwrap();
    let journal = skill_studio_core::journal::FsJournal::new(
        home.join(".agents/skill-studio-journal"),
        rt.ports.fs.clone(),
    );
    journal
        .begin(
            &guard,
            &PlanRecord {
                id: skill_studio_core::identity::PlanId("unrecoverable-fixture".into()),
                created_at: rt.ports.clock.now(),
                label: "recovery needs an unavailable root".into(),
                root: foreign.to_path_buf(),
                backups: Vec::new(),
                expired_backups: Vec::new(),
                steps: vec![PlanStep::Stage {
                    staged: foreign.join("sentinel"),
                }],
                status: PlanStatus::Pending,
            },
        )
        .unwrap();
    event
}

pub fn large_asset_update(home: &Path, failed: bool) -> (EventId, Vec<u8>, Vec<u8>) {
    use skill_studio_core::events::{EventDraft, EventKind, EventStatus};
    use skill_studio_core::identity::Fingerprint;
    use skill_studio_core::ports::{acquire_exclusive, HistoryAccess};
    write_skill(home, "alpha", "A skill with a large asset.");
    let target = home.join(".agents/skills/alpha");
    let asset = target.join("asset.bin");
    let size = usize::try_from(ops::SKILL_MD_MAX_BYTES).unwrap() + 1;
    let before = vec![b'a'; size];
    std::fs::write(&asset, &before).unwrap();
    let rt = runtime(home);
    let guard = acquire_exclusive(rt.ports.leases.as_ref(), &rt.scope).unwrap();
    let mut store = rt
        .ports
        .history
        .open(&rt.scope, HistoryAccess::ReadWrite)
        .unwrap()
        .unwrap();
    let id = EventId::from_ulid(ulid::Ulid::generate());
    let manifest = store
        .backup_paths(&guard, &id, std::slice::from_ref(&target))
        .unwrap();
    let pre = manifest.entries[0].fingerprint.as_ref().unwrap().bare_hex();
    let mut after = before.clone();
    after[size - 1] = b'b';
    std::fs::write(&asset, &after).unwrap();
    let post = skill_studio_host::hash_entry(&target).unwrap();
    store.record(&guard, &id, &EventDraft {
        kind: EventKind::Update, skill: SkillName("alpha".into()), harness: None,
        scope: Some("global".into()), project_path: None, payload: serde_json::json!({}),
        inverse: Some(serde_json::json!({"op":"restore_backup","path":target,"pre_fingerprint":pre,"post_fingerprint":post})),
        backup_dir: Some(manifest.backup_dir),
    }).unwrap();
    store
        .finish(
            &guard,
            &id,
            if failed {
                EventStatus::Failed
            } else {
                EventStatus::Done
            },
            Some(Fingerprint::parse(&post).unwrap()),
        )
        .unwrap();
    (id, before, after)
}

struct RecordedUpdateCli {
    home: PathBuf,
    trace: serde_json::Value,
}

fn recorded_files(home: &Path, files: &serde_json::Value) {
    use std::os::unix::fs::PermissionsExt;
    let root = home.join(".agents/skills/trace-skill");
    std::fs::create_dir_all(&root).unwrap();
    for file in files.as_array().unwrap() {
        let path = root.join(file["relative_path"].as_str().unwrap());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, file["content"].as_str().unwrap()).unwrap();
        std::fs::set_permissions(
            path,
            std::fs::Permissions::from_mode(u32::try_from(file["mode"].as_u64().unwrap()).unwrap()),
        )
        .unwrap();
    }
}

impl ProcessSpawner for RecordedUpdateCli {
    fn run(
        &self,
        spec: &ProcessSpec,
        _: &dyn CancelToken,
    ) -> Result<ProcessOutput, skill_studio_core::CoreError> {
        assert_eq!(serde_json::json!(spec.args), self.trace["args"]);
        std::fs::remove_dir_all(self.home.join(".agents/skills/trace-skill")).unwrap();
        recorded_files(&self.home, &self.trace["files_after"]);
        std::fs::write(
            self.home.join(".agents/.skill-lock.json"),
            serde_json::to_vec(&self.trace["lock_after"]).unwrap(),
        )
        .unwrap();
        Ok(ProcessOutput {
            status: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            timed_out: false,
        })
    }
}

pub fn recorded_update(home: &Path) -> (EventId, serde_json::Value) {
    let trace:serde_json::Value=serde_json::from_str(include_str!("../../../../crates/skill-studio-core/tests/fixtures/cli_traces/skills_sh_update_global.trace.json")).unwrap();
    recorded_files(home, &trace["files_before"]);
    std::fs::write(
        home.join(".agents/.skill-lock.json"),
        serde_json::to_vec(&trace["lock_before"]).unwrap(),
    )
    .unwrap();
    for (relative, target) in trace["links_before"].as_object().unwrap() {
        let path = home.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(
            target
                .as_str()
                .unwrap()
                .replace("$HOME", &home.to_string_lossy()),
            path,
        )
        .unwrap();
    }
    let mut rt = runtime(home);
    rt.ports.spawner = Some(Arc::new(RecordedUpdateCli {
        home: home.to_path_buf(),
        trace: trace.clone(),
    }));
    let result = ops::update(
        &rt,
        &ctx(),
        &UpdateRequest {
            skill: SkillName("trace-skill".into()),
            method: InstallMethod::SkillsSh,
            scope: RootScope::Global,
            files: Vec::new(),
            source: Some("skill-studio-trace/fixture".into()),
            ref_pin: None,
        },
    )
    .unwrap();
    (result.event_id, trace)
}

pub fn edit_recorded_lock(home: &Path, forced: bool) -> serde_json::Value {
    let path = home.join(".agents/.skill-lock.json");
    let mut lock: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    lock["skills"]["unrelated"] = lock["skills"]["trace-skill"].clone();
    if forced {
        lock["skills"]["trace-skill"]["skillFolderHash"] = serde_json::json!("a later user edit");
    }
    std::fs::write(path, serde_json::to_vec(&lock).unwrap()).unwrap();
    lock
}

pub fn assert_recorded_lock(home: &Path, own: &serde_json::Value, unrelated: &serde_json::Value) {
    let lock: serde_json::Value =
        serde_json::from_slice(&std::fs::read(home.join(".agents/.skill-lock.json")).unwrap())
            .unwrap();
    assert_eq!(&lock["skills"]["trace-skill"], own);
    assert_eq!(&lock["skills"]["unrelated"], unrelated);
}

const DOTAGENTS_DECLARATION: &str = include_str!(
    "../../../../crates/skill-studio-core/tests/fixtures/dotagents-remove-3.0.1/before-agents.toml"
);
const DOTAGENTS_LOCK: &str = include_str!(
    "../../../../crates/skill-studio-core/tests/fixtures/dotagents-remove-3.0.1/before-agents.lock"
);
const DOTAGENTS_AFTER_DECLARATION: &str = include_str!(
    "../../../../crates/skill-studio-core/tests/fixtures/dotagents-remove-3.0.1/after-agents.toml"
);
const DOTAGENTS_AFTER_LOCK: &str = include_str!(
    "../../../../crates/skill-studio-core/tests/fixtures/dotagents-remove-3.0.1/after-agents.lock"
);
const LATER_DECLARATION: &str =
    "# later beta stays exact\n[[skills]]\nname = 'beta'\nsource = 'owner/beta'\n";
const LATER_LOCK: &str = "# later beta lock stays exact\n[skills.beta]\nsource = 'owner/beta'\nresolved_commit = 'later-beta'\n";

struct DotagentsRemoveCli(PathBuf);
impl ProcessSpawner for DotagentsRemoveCli {
    fn run(
        &self,
        spec: &ProcessSpec,
        _: &dyn CancelToken,
    ) -> Result<ProcessOutput, skill_studio_core::CoreError> {
        assert!(spec.args.iter().any(|arg| arg == "remove"));
        assert!(spec.args.iter().any(|arg| arg == "@sentry/dotagents"));
        std::fs::remove_dir_all(self.0.join(".agents/skills/alpha")).unwrap();
        std::fs::write(
            self.0.join(".agents/agents.toml"),
            DOTAGENTS_AFTER_DECLARATION,
        )
        .unwrap();
        std::fs::write(self.0.join(".agents/agents.lock"), DOTAGENTS_AFTER_LOCK).unwrap();
        Ok(ProcessOutput {
            status: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            timed_out: false,
        })
    }
}

pub fn removed_dotagents(home: &Path, later: bool) -> EventId {
    removed_dotagents_with_link(home, later, None)
}

pub fn removed_dotagents_with_link(home: &Path, later: bool, linked_lock: Option<bool>) -> EventId {
    write_skill(home, "alpha", "saved Dotagents bytes");
    std::fs::write(home.join(".agents/agents.toml"), DOTAGENTS_DECLARATION).unwrap();
    std::fs::write(home.join(".agents/agents.lock"), DOTAGENTS_LOCK).unwrap();
    std::fs::write(home.join(".agents/.gitignore"), "/skills/alpha\r\n").unwrap();
    if let Some(lock) = linked_lock {
        let path = home
            .join(".agents")
            .join(if lock { "agents.lock" } else { "agents.toml" });
        let saved = path.with_extension("saved");
        std::fs::rename(&path, &saved).unwrap();
        std::os::unix::fs::symlink(saved.file_name().unwrap(), &path).unwrap();
    }
    let mut rt = runtime(home);
    rt.ports.spawner = Some(Arc::new(DotagentsRemoveCli(home.to_path_buf())));
    let deployment_id = deployment(&rt, &RootKind::Universal);
    let removed = ops::remove(&rt, &ctx(), &RemoveRequest { deployment_id }).unwrap();
    if later {
        std::fs::write(
            home.join(".agents/agents.toml"),
            format!("{DOTAGENTS_AFTER_DECLARATION}{LATER_DECLARATION}"),
        )
        .unwrap();
        std::fs::write(
            home.join(".agents/agents.lock"),
            format!("{DOTAGENTS_AFTER_LOCK}{LATER_LOCK}"),
        )
        .unwrap();
        std::fs::write(
            home.join(".agents/.gitignore"),
            "/skills/alpha\r\n/skills/beta\n",
        )
        .unwrap();
    }
    removed.event_id
}

pub fn assert_dotagents_saved_entries(home: &Path, later: bool, restored: bool) {
    let saved_lock = DOTAGENTS_LOCK.split_once("# exact alpha lock").unwrap().1;
    let restored_lock = format!("{DOTAGENTS_AFTER_LOCK}# exact alpha lock{saved_lock}");
    for (name, expected) in [
        (
            "agents.toml",
            format!(
                "{}{}",
                if restored {
                    DOTAGENTS_DECLARATION
                } else {
                    DOTAGENTS_AFTER_DECLARATION
                },
                if later { LATER_DECLARATION } else { "" }
            ),
        ),
        (
            "agents.lock",
            format!(
                "{}{}",
                if restored {
                    &restored_lock
                } else {
                    DOTAGENTS_AFTER_LOCK
                },
                if later { LATER_LOCK } else { "" }
            ),
        ),
        (
            ".gitignore",
            format!(
                "/skills/alpha\r\n{}",
                if later { "/skills/beta\n" } else { "" }
            ),
        ),
    ] {
        assert_eq!(
            std::fs::read_to_string(home.join(".agents").join(name)).unwrap(),
            expected,
            "{name}, restored={restored}, later={later}"
        );
    }
    assert_eq!(home.join(".agents/skills/alpha").exists(), restored);
}

pub fn conflict_dotagents_slot(home: &Path, lock: bool, link: bool) -> (PathBuf, String) {
    let path = home
        .join(".agents")
        .join(if lock { "agents.lock" } else { "agents.toml" });
    let name = if link { "beta" } else { "alpha" };
    let newer = if lock {
        format!("[skills.{name}]\nsource = 'new-owner/new-entry'\n")
    } else {
        format!("[[skills]]\nname = '{name}'\nsource = 'new-owner/new-entry'\n")
    };
    if link {
        std::fs::remove_file(&path).unwrap();
        let other = path.with_extension("later");
        std::fs::write(&other, &newer).unwrap();
        std::os::unix::fs::symlink(&other, &path).unwrap();
    } else {
        std::fs::write(&path, &newer).unwrap();
    }
    (path, newer)
}
