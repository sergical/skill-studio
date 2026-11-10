#![allow(clippy::unwrap_used, clippy::expect_used)]
use skill_studio_core::{
    dto::DoctorRequest,
    events::{EventDraft, EventKind},
    harness::HarnessCatalog,
    identity::{CorrelationId, EventId, SkillName},
    ops,
    ports::{acquire_exclusive, HistoryAccess, OpContext, Runtime},
    RuntimeScope,
};
use std::sync::Arc;

#[test]
fn doctor_finds_a_real_sqlite_pending_event_without_reconciling_or_calling_the_journal_empty() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir(&home).unwrap();
    let scope = RuntimeScope::fixture(&home);
    let db = scope.history_root.join("events.sqlite3");
    let ports = skill_studio_host::default_ports_with_history(
        dir.path().join("leases"),
        Arc::new(HarnessCatalog::builtin()),
        db,
    );
    let rt = Runtime::new(&scope, ports).unwrap();
    let id = EventId("pending-fixture".into());
    {
        let guard = acquire_exclusive(rt.ports.leases.as_ref(), &rt.scope).unwrap();
        let mut store = rt
            .ports
            .history
            .open(&rt.scope, HistoryAccess::ReadWrite)
            .unwrap()
            .unwrap();
        let draft = EventDraft {
            kind: EventKind::Park,
            skill: SkillName("alpha".into()),
            harness: None,
            scope: Some("global".into()),
            project_path: None,
            payload: serde_json::json!({}),
            inverse: None,
            backup_dir: None,
        };
        store.record(&guard, &id, &draft).unwrap();
    }
    let report = ops::doctor(
        &rt,
        &OpContext::uncancellable(CorrelationId("doctor".into())),
        &DoctorRequest {},
    )
    .unwrap();
    assert!(
        report.violations.iter().any(|row| row.invariant
            == skill_studio_core::doctor::DoctorInvariant::JournalHasNoOpenPlan
            && row.detail.contains(&id.0)),
        "{report:?}"
    );
    let store = rt
        .ports
        .history
        .open(&rt.scope, HistoryAccess::ReadIfExists)
        .unwrap()
        .unwrap();
    assert_eq!(
        store
            .pending()
            .unwrap()
            .iter()
            .map(|row| &row.id)
            .collect::<Vec<_>>(),
        vec![&id]
    );
}
