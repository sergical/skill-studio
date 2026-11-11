#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use skill_studio_core::dto::RestoreRequest;
use skill_studio_core::events::{EventDraft, EventKind, EventStatus};
use skill_studio_core::identity::{EventId, PlanId, SkillName};
use skill_studio_core::journal::FsJournal;
use skill_studio_core::ports::{
    acquire_exclusive, HistoryAccess, Journal, PlanRecord, PlanStatus, PlanStep, Runtime,
};
use skill_studio_core::scope::{ProjectSelection, RuntimeScope};
use skill_studio_core::testing::golden::ctx;

#[test]
fn restore_waits_for_successful_recovery_or_writes_over_an_interrupted_plan() {
    let home = tempfile::tempdir().unwrap();
    let foreign = tempfile::tempdir().unwrap();
    let sentinel = foreign.path().join("sentinel");
    std::fs::write(&sentinel, b"foreign bytes stay").unwrap();
    let path = home.path().join(".agents/skills/alpha/SKILL.md");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"saved user bytes").unwrap();
    let data = home.path().join(".skill-studio");
    let mut scope = RuntimeScope::live(home.path().to_path_buf(), data.join("history"));
    scope.projects = ProjectSelection::Explicit { paths: Vec::new() };
    let rt = Runtime::new(
        &scope,
        skill_studio_host::default_ports_with_history(
            data.join("leases"),
            Arc::new(skill_studio_core::harness::HarnessCatalog::builtin()),
            data.join("history/events.sqlite3"),
        ),
    )
    .unwrap();
    let id = EventId::from_ulid(ulid::Ulid::generate());
    {
        let guard = acquire_exclusive(rt.ports.leases.as_ref(), &rt.scope).unwrap();
        let mut store = rt
            .ports
            .history
            .open(&rt.scope, HistoryAccess::ReadWrite)
            .unwrap()
            .unwrap();
        let manifest = store
            .backup_paths(&guard, &id, std::slice::from_ref(&path))
            .unwrap();
        let pre = manifest.entries[0].fingerprint.as_ref().unwrap().bare_hex();
        store.record(&guard, &id, &EventDraft { kind: EventKind::Remove, skill: SkillName("alpha".into()), harness: None, scope: Some("global".into()), project_path: None, payload: serde_json::json!({}), inverse: Some(serde_json::json!({"op":"restore_backup","path":path,"pre_fingerprint":pre,"post_fingerprint":"absent"})), backup_dir: Some(manifest.backup_dir) }).unwrap();
        std::fs::remove_file(&path).unwrap();
        store.finish(&guard, &id, EventStatus::Done, None).unwrap();
        let journal = FsJournal::new(
            home.path().join(".agents/skill-studio-journal"),
            rt.ports.fs.clone(),
        );
        journal
            .begin(
                &guard,
                &PlanRecord {
                    id: PlanId("unrecoverable-fixture".into()),
                    created_at: rt.ports.clock.now(),
                    label: "recovery needs an unavailable root".into(),
                    root: foreign.path().to_path_buf(),
                    backups: Vec::new(),
                    expired_backups: Vec::new(),
                    steps: vec![PlanStep::Stage {
                        staged: sentinel.clone(),
                    }],
                    status: PlanStatus::Pending,
                },
            )
            .unwrap();
    }
    let error = skill_studio_core::ops::restore_event(
        &rt,
        &ctx(),
        &RestoreRequest {
            event_id: id.clone(),
            force: true,
        },
    )
    .unwrap_err();
    assert_eq!(error.code, skill_studio_core::ErrorCode::Incomplete);
    assert!(error.message.contains("Recovery"), "{error}");
    assert!(
        !path.exists(),
        "a new Restore must not write while recovery remains interrupted"
    );
    assert_eq!(std::fs::read(sentinel).unwrap(), b"foreign bytes stay");
    let store = rt
        .ports
        .history
        .open(&rt.scope, HistoryAccess::ReadIfExists)
        .unwrap()
        .unwrap();
    assert!(store.get(&id).unwrap().unwrap().reverted_by.is_none());
    assert_eq!(
        store
            .list(&skill_studio_core::events::EventFilter {
                limit: 100,
                ..Default::default()
            })
            .unwrap()
            .len(),
        1
    );
}
