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

#[test]
fn doctor_reads_delete_mode_history_without_changing_bytes_schema_or_mode_or_migrates_database() {
    for with_events in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir(&home).unwrap();
        let scope = RuntimeScope::fixture(&home);
        let db = scope.history_root.join("events.sqlite3");
        std::fs::create_dir_all(db.parent().unwrap()).unwrap();
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch("PRAGMA journal_mode=DELETE; CREATE TABLE sentinel (value TEXT);")
            .unwrap();
        if with_events {
            conn.execute_batch("CREATE TABLE events (id TEXT PRIMARY KEY, ts TEXT NOT NULL, kind TEXT NOT NULL, skill TEXT NOT NULL, harness TEXT, scope TEXT, project_path TEXT, payload TEXT NOT NULL, inverse TEXT, backup_dir TEXT, status TEXT NOT NULL, reverted_by TEXT, restorable INTEGER NOT NULL DEFAULT 1);").unwrap();
        }
        let schema = |conn: &rusqlite::Connection| -> Vec<String> {
            conn.prepare("SELECT sql FROM sqlite_master ORDER BY name")
                .unwrap()
                .query_map([], |row| row.get::<_, Option<String>>(0))
                .unwrap()
                .map(|row| row.unwrap().unwrap_or_default())
                .collect()
        };
        let before_schema = schema(&conn);
        drop(conn);
        let before = std::fs::read(&db).unwrap();
        std::fs::create_dir_all(home.join(".agents/skills-trash/old")).unwrap();
        let ports = skill_studio_host::default_ports_with_history(
            dir.path().join("leases"),
            Arc::new(HarnessCatalog::builtin()),
            db.clone(),
        );
        let rt = Runtime::new(&scope, ports).unwrap();
        let report = ops::doctor(
            &rt,
            &OpContext::uncancellable(CorrelationId("read-only".into())),
            &DoctorRequest {},
        )
        .unwrap();
        assert_eq!(
            std::fs::read(&db).unwrap(),
            before,
            "doctor modified database bytes: {report:?}"
        );
        let conn =
            rusqlite::Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .unwrap();
        assert_eq!(schema(&conn), before_schema);
        assert_eq!(
            conn.query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
                .unwrap(),
            "delete"
        );
        assert!(!db.with_file_name("events.sqlite3-wal").exists());
        assert!(!db.with_file_name("events.sqlite3-shm").exists());
    }
}
