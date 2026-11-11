#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::path::Path;
use std::sync::Arc;

use skill_studio_core::harness::HarnessCatalog;
use skill_studio_core::ports::{Ports, Runtime};
use skill_studio_core::scope::RuntimeScope;
use skill_studio_core::testing::golden::ctx;
use skill_studio_core::testing::{FakeClock, FakeIds, RecordingSink};

use skill_studio_host::{FileLease, RealFs, SqliteHistoryOpener};

fn runtime_with(home: &Path, fs: Arc<dyn skill_studio_core::ports::ScopeFs>) -> Runtime {
    let history_root = home.join(".history");
    let db_path = history_root.join("events.sqlite3");
    let scope = RuntimeScope::fixture(home);
    let ports = Ports {
        fs,
        clock: Arc::new(FakeClock::at(0)),
        ids: Arc::new(FakeIds::default()),
        leases: Arc::new(FileLease::new(home.join(".leases"))),
        history: Arc::new(SqliteHistoryOpener::new(db_path)),
        sink: Arc::new(RecordingSink::default()),
        spawner: None,
        discovery: None,
        tools: None,
        catalog: Arc::new(HarnessCatalog::builtin()),

        telemetry: std::sync::Arc::new(skill_studio_core::ports::NoopTelemetry),
    };
    Runtime::new(&scope, ports).unwrap()
}

#[test]
fn review_due_uses_fake_clock_and_excludes_today_instead_of_reading_wall_time() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".agents")).unwrap();
    std::fs::write(
        home.path().join(".agents/review-dates.json"),
        r#"{"dates":{"past":"1969-12-31","today":"1970-01-01","future":"1970-01-02"}}"#,
    )
    .unwrap();
    let rt = runtime_with(home.path(), Arc::new(RealFs::new()));
    let due = skill_studio_core::review::review_due(&rt, &ctx()).unwrap();
    assert_eq!(
        due.keys().map(String::as_str).collect::<Vec<_>>(),
        vec!["past"]
    );
}
