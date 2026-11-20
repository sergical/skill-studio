// Integration test binaries aren't covered by the lib crate's
// `cfg_attr(test, allow(...))`: this file compiles as its own crate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Old-vs-new migration tests for desktop event rows that today's builds
//! no longer write but still read: the pre-shared-history event log, and
//! `distribute_from_shared` rows that still show Undo in Activity.
//!
//! The legacy `<app_data>/events.sqlite3` is written here with raw SQL in the
//! schema older desktop builds created: no `restorable` and no `backup_dir`
//! column on `events`, plus converted-folder rows in `materialized_roots`
//! and `materialized_disabled`. Writing it with today's `EventStore` would
//! give it today's schema and prove nothing about old files.

use std::path::Path;

use serde_json::json;
use skill_studio_core::testing::golden::unique_temp_dir;
use skill_studio_lib::skills::core_runtime::history_db_path;
use skill_studio_lib::skills::event_commands::{dto_from_row, restore_event_with_runtime};
use skill_studio_lib::skills::event_store::{
    allocate_id, fingerprint_path, EventDraft, EventStatus, EventStore,
};

/// The `events` table as the first desktop builds created it, minus
/// `backup_dir` (some old files lack it, see the `ADD COLUMN backup_dir`
/// migration) and minus `restorable` (added later). Both columns come from
/// the migrations the import runs on the legacy file.
const OLD_SCHEMA: &str = "
    CREATE TABLE events (
        id          TEXT PRIMARY KEY,
        ts          TEXT NOT NULL,
        kind        TEXT NOT NULL,
        skill       TEXT NOT NULL,
        harness     TEXT,
        scope       TEXT,
        project_path TEXT,
        payload     TEXT NOT NULL,
        inverse     TEXT,
        status      TEXT NOT NULL,
        reverted_by TEXT REFERENCES events(id)
    );
    CREATE INDEX idx_events_skill ON events(skill, ts DESC);
    CREATE TABLE materialized_roots (
        root_path   TEXT PRIMARY KEY,
        harness     TEXT NOT NULL,
        shared_root TEXT NOT NULL,
        created_by  TEXT REFERENCES events(id)
    );
    CREATE TABLE materialized_disabled (
        root_path   TEXT NOT NULL REFERENCES materialized_roots(root_path),
        skill       TEXT NOT NULL,
        PRIMARY KEY (root_path, skill)
    );
";

fn write_old_format_events_file(path: &Path, root_a: &str, root_b: &str) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(OLD_SCHEMA).unwrap();
    conn.execute(
        "INSERT INTO events (id, ts, kind, skill, harness, scope, project_path, payload, inverse, status, reverted_by)
         VALUES ('legacy-convert', '2026-08-01T10:00:00Z', 'explode_shared_dir', 'alpha', 'claude-code', 'global', NULL,
                 ?1, ?2, 'done', NULL)",
        rusqlite::params![
            json!({"root": root_a}).to_string(),
            json!({"op": "unexplode", "root": root_a}).to_string(),
        ],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO events (id, ts, kind, skill, harness, scope, project_path, payload, inverse, status, reverted_by)
         VALUES ('legacy-reverted', '2026-08-02T10:00:00Z', 'repair_remove_link', 'beta', 'codex', 'project', '/work/app',
                 '{}', NULL, 'done', 'legacy-convert')",
        [],
    )
    .unwrap();
    for (root, harness, shared, created_by) in [
        (
            root_a,
            "claude-code",
            "/home/.agents/skills",
            Some("legacy-convert"),
        ),
        (root_b, "codex", "/home/.agents/skills", None),
    ] {
        conn.execute(
            "INSERT INTO materialized_roots (root_path, harness, shared_root, created_by) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![root, harness, shared, created_by],
        )
        .unwrap();
    }
    for (root, skill) in [(root_a, "alpha"), (root_a, "gamma"), (root_b, "delta")] {
        conn.execute(
            "INSERT INTO materialized_disabled (root_path, skill) VALUES (?1, ?2)",
            rusqlite::params![root, skill],
        )
        .unwrap();
    }
}

#[test]
fn an_old_format_event_log_imports_its_events_and_converted_folder_records_into_the_shared_history()
{
    let home = unique_temp_dir("legacy_event_import_old_schema");
    let app_data = home.join("app_data");
    std::fs::create_dir_all(&app_data).unwrap();
    let root_a = home.join(".claude/skills").to_string_lossy().into_owned();
    let root_b = home.join(".codex/skills").to_string_lossy().into_owned();
    let legacy_path = app_data.join("events.sqlite3");
    write_old_format_events_file(&legacy_path, &root_a, &root_b);

    let shared_db = history_db_path(&home.join(".skill-studio"));
    let store = EventStore::open_with_db(&app_data, &shared_db).unwrap();
    assert_eq!(store.import_legacy_events().unwrap(), 2);

    let convert = store
        .get("legacy-convert")
        .unwrap()
        .expect("event imported");
    assert_eq!(convert.ts, "2026-08-01T10:00:00Z");
    assert_eq!(convert.kind, "explode_shared_dir");
    assert_eq!(convert.skill, "alpha");
    assert_eq!(convert.harness.as_deref(), Some("claude-code"));
    assert_eq!(convert.scope.as_deref(), Some("global"));
    assert_eq!(convert.project_path, None);
    assert_eq!(convert.payload, json!({"root": root_a}));
    assert_eq!(
        convert.inverse,
        Some(json!({"op": "unexplode", "root": root_a}))
    );
    assert_eq!(convert.status, "done");
    assert_eq!(convert.reverted_by, None);
    // Columns the old file did not have take the migration defaults.
    assert_eq!(convert.backup_dir, None);
    assert!(convert.restorable, "old rows default to restorable");

    let reverted = store
        .get("legacy-reverted")
        .unwrap()
        .expect("event imported");
    assert_eq!(reverted.status, "done");
    assert_eq!(reverted.reverted_by.as_deref(), Some("legacy-convert"));
    assert_eq!(reverted.project_path.as_deref(), Some("/work/app"));
    assert_eq!(reverted.inverse, None);

    let a = store
        .materialized_root(Path::new(&root_a))
        .unwrap()
        .expect("converted folder A imported");
    assert_eq!(
        (
            a.harness.as_str(),
            a.shared_root.as_str(),
            a.created_by.as_deref()
        ),
        (
            "claude-code",
            "/home/.agents/skills",
            Some("legacy-convert")
        )
    );
    let b = store
        .materialized_root(Path::new(&root_b))
        .unwrap()
        .expect("converted folder B imported");
    assert_eq!(
        (
            b.harness.as_str(),
            b.shared_root.as_str(),
            b.created_by.as_deref()
        ),
        ("codex", "/home/.agents/skills", None)
    );
    let mut disabled_a = store.materialized_disabled(Path::new(&root_a)).unwrap();
    disabled_a.sort();
    assert_eq!(disabled_a, vec!["alpha".to_string(), "gamma".to_string()]);
    assert_eq!(
        store.materialized_disabled(Path::new(&root_b)).unwrap(),
        vec!["delta".to_string()]
    );

    assert!(
        !legacy_path.exists(),
        "the legacy file is renamed after import"
    );
    assert!(app_data.join("events.sqlite3.migrated").exists());
    let _ = std::fs::remove_dir_all(&home);
}

/// An Activity row of kind `distribute_from_shared`, written the way old
/// builds wrote it: the shared skill folder backed up under
/// `<app_data>/backups/<id>`, a real copy in the Claude Code folder where a
/// link used to be, and the inverse as literal JSON (not today's
/// `InverseOp`, so a renamed field or tag fails here). Undo must put the
/// shared folder back, delete the copy and recreate the link.
#[test]
fn an_old_distribute_from_shared_row_still_undoes_through_the_desktop() {
    let home = unique_temp_dir("legacy_distribute_from_shared");
    let data_root = home.join(".skill-studio");
    let app_data = home.join("app_data");
    std::fs::create_dir_all(&app_data).unwrap();
    let shared_dir = home.join(".agents/skills/alpha");
    std::fs::create_dir_all(&shared_dir).unwrap();
    let skill_md = "---\nname: alpha\ndescription: shared skill\n---\nBody.\n";
    std::fs::write(shared_dir.join("SKILL.md"), skill_md).unwrap();
    let claude_root = home.join(".claude/skills");
    std::fs::create_dir_all(&claude_root).unwrap();
    let link = claude_root.join("alpha");

    let store = EventStore::open_with_db(&app_data, &history_db_path(&data_root)).unwrap();
    let id = allocate_id();
    let pre_fingerprint = fingerprint_path(&shared_dir);
    store
        .backup_paths(&id, std::slice::from_ref(&shared_dir))
        .unwrap();

    // What the old forward operation left behind: the shared folder gone,
    // the link replaced by a real copy.
    std::fs::remove_dir_all(&shared_dir).unwrap();
    std::fs::create_dir_all(&link).unwrap();
    std::fs::write(link.join("SKILL.md"), skill_md).unwrap();
    let copy_fingerprint = fingerprint_path(&link);
    let inverse = json!({
        "op": "undistribute_from_shared",
        "shared_dir": shared_dir,
        "copies": [link],
        "copy_fingerprints": [copy_fingerprint],
        "symlinks": [[link, shared_dir]],
        "pre_fingerprint": pre_fingerprint,
        "post_fingerprint": null,
    });
    store
        .record(
            &id,
            &EventDraft {
                kind: "distribute_from_shared".to_string(),
                skill: "alpha".to_string(),
                harness: Some("claude-code".to_string()),
                scope: Some("global".to_string()),
                project_path: None,
                payload: json!({ "shared_dir": shared_dir }),
                inverse: Some(inverse),
                backup_dir: Some(format!("backups/{id}")),
                restorable: true,
            },
        )
        .unwrap();
    store.finish(&id, EventStatus::Done).unwrap();

    let row = store.get(&id).unwrap().unwrap();
    assert!(dto_from_row(&store, &home, &data_root, row).restorable);
    restore_event_with_runtime(&store, &home, &data_root, &id, false).unwrap();

    assert_eq!(
        std::fs::read_to_string(shared_dir.join("SKILL.md")).unwrap(),
        skill_md,
        "Undo puts the shared folder back from the backup"
    );
    assert_eq!(
        std::fs::read_link(&link).unwrap(),
        shared_dir,
        "Undo replaces the copy with the link it removed"
    );
    assert!(
        store.get(&id).unwrap().unwrap().reverted_by.is_some(),
        "the old row is marked undone"
    );
    let _ = std::fs::remove_dir_all(&home);
}
