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

#[test]
fn doctor_pending_report_caps_examples_and_counts_all_or_returns_every_pending_event() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir(&home).unwrap();
    let scope = RuntimeScope::fixture(&home);
    let db = scope.history_root.join("events.sqlite3");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch("CREATE TABLE events (id TEXT, kind TEXT, status TEXT, payload TEXT, inverse TEXT); WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<4097) INSERT INTO events SELECT x,'park','pending','invalid JSON','invalid JSON' FROM n;").unwrap();
    drop(conn);
    let ports = skill_studio_host::default_ports_with_history(
        dir.path().join("leases"),
        Arc::new(HarnessCatalog::builtin()),
        db,
    );
    let rt = Runtime::new(&scope, ports).unwrap();
    let report = ops::doctor(
        &rt,
        &OpContext::uncancellable(CorrelationId("doctor".into())),
        &DoctorRequest {},
    )
    .unwrap();
    let problems: Vec<_> = report
        .violations
        .iter()
        .filter(|v| v.invariant == skill_studio_core::doctor::DoctorInvariant::JournalHasNoOpenPlan)
        .collect();
    assert_eq!(problems.len(), 1);
    assert!(problems[0]
        .detail
        .contains("4097 history events still pending"));
    assert_eq!(problems[0].detail.matches("(park)").count(), 20);
}

fn complete_tree(
    root: &std::path::Path,
) -> std::collections::BTreeMap<std::path::PathBuf, (u64, Vec<u8>)> {
    fn walk(
        root: &std::path::Path,
        path: &std::path::Path,
        out: &mut std::collections::BTreeMap<std::path::PathBuf, (u64, Vec<u8>)>,
    ) {
        let facts = std::fs::symlink_metadata(path).unwrap();
        let bytes = if facts.is_file() {
            std::fs::read(path).unwrap()
        } else if facts.file_type().is_symlink() {
            std::fs::read_link(path)
                .unwrap()
                .as_os_str()
                .as_encoded_bytes()
                .to_vec()
        } else {
            Vec::new()
        };
        out.insert(
            path.strip_prefix(root).unwrap().to_path_buf(),
            (facts.len(), bytes),
        );
        if facts.is_dir() {
            for entry in std::fs::read_dir(path).unwrap() {
                walk(root, &entry.unwrap().path(), out);
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(root, root, &mut out);
    out
}

#[test]
fn doctor_retention_counts_copies_with_wal_or_skips_filesystem_checks_when_history_is_busy() {
    assert_doctor_retention_counts_copies_without_history(true);
}

#[test]
fn doctor_retention_counts_copies_with_corrupt_history_or_skips_filesystem_checks_after_database_errors(
) {
    assert_doctor_retention_counts_copies_without_history(false);
}

fn assert_doctor_retention_counts_copies_without_history(wal: bool) {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let trash = home.join(".agents/skills-trash");
    for n in 0..21 {
        let copy = trash.join(format!("copy-{n}"));
        std::fs::create_dir_all(&copy).unwrap();
        std::fs::write(
            copy.join("SKILL.md"),
            "---\nname: distinctive-retained-name\ndescription: fixture\n---\n",
        )
        .unwrap();
    }
    let scope = RuntimeScope::fixture(&home);
    std::fs::create_dir_all(&scope.history_root).unwrap();
    let db = scope.history_root.join("events.sqlite3");
    std::fs::write(&db, b"corrupt database").unwrap();
    if wal {
        std::fs::write(db.with_extension("sqlite3-wal"), b"busy").unwrap();
    }
    let ports = skill_studio_host::default_ports_with_history(
        home.join(".history/leases"),
        Arc::new(HarnessCatalog::builtin()),
        db,
    );
    let rt = Runtime::new(&scope, ports).unwrap();
    let before = complete_tree(dir.path());
    let report = ops::doctor(
        &rt,
        &OpContext::uncancellable(CorrelationId("retention".into())),
        &DoctorRequest {},
    )
    .unwrap();
    assert!(
        report
            .violations
            .iter()
            .any(|v| v.detail.contains("21 retained entries")),
        "{report:?}"
    );
    assert!(
        report
            .violations
            .iter()
            .any(|v| v.detail.contains("history") && v.detail.contains("could not be read")),
        "{report:?}"
    );
    assert_eq!(before, complete_tree(dir.path()));
}

#[test]
fn doctor_retention_reports_linked_metadata_without_external_grouping_or_reads_outside_name() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let outside = dir.path().join("SKILL.md");
    std::fs::write(
        &outside,
        "---\nname: distinctive-outside-name\ndescription: fixture\n---\n",
    )
    .unwrap();
    for n in 0..21 {
        let copy = home.join(".agents/skills-trash").join(format!("copy-{n}"));
        std::fs::create_dir_all(&copy).unwrap();
        std::os::unix::fs::symlink(&outside, copy.join("SKILL.md")).unwrap();
    }
    let scope = RuntimeScope::fixture(&home);
    let rt = Runtime::new(
        &scope,
        skill_studio_host::default_ports_with_history(
            home.join(".history/leases"),
            Arc::new(HarnessCatalog::builtin()),
            scope.history_root.join("events.sqlite3"),
        ),
    )
    .unwrap();
    let report = ops::doctor(
        &rt,
        &OpContext::uncancellable(CorrelationId("linked".into())),
        &DoctorRequest {},
    )
    .unwrap();
    assert_eq!(
        report
            .violations
            .iter()
            .filter(|v| v.detail.contains("retained SKILL.md is linked"))
            .count(),
        21
    );
    assert!(
        !report
            .violations
            .iter()
            .any(|v| v.detail.contains("distinctive-outside-name")),
        "{report:?}"
    );
}
