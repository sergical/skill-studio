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
