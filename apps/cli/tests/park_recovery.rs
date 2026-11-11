#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;
use skill_studio_core::{
    events::{EventDraft, EventKind},
    harness::HarnessCatalog,
    identity::{EventId, SkillName},
    ports::{MutationSession, Runtime},
    testing::golden::ctx,
    RuntimeScope,
};
use std::sync::Arc;
#[path = "support/headless.rs"]
mod headless;

fn interrupted(home: &std::path::Path) -> std::path::PathBuf {
    let data = home.join(".skill-studio");
    let ports = skill_studio_host::default_ports_with_history(
        data.join("leases"),
        Arc::new(HarnessCatalog::builtin()),
        data.join("history/events.sqlite3"),
    );
    let mut scope = RuntimeScope::live(home.to_path_buf(), data.join("history"));
    scope.projects = skill_studio_core::scope::ProjectSelection::Explicit { paths: Vec::new() };
    let rt = Runtime::new(&scope, ports).unwrap();
    let mut session = MutationSession::begin(&rt, &ctx()).unwrap();
    let path = home.join(".agents/skills/.park-stage-crash");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("SKILL.md"), "retained bytes").unwrap();
    session.store.record(&session.guard, &EventId("recovery-event".into()), &EventDraft {
        kind: EventKind::Park, skill: SkillName("foo".into()), harness: None, scope: Some("global".into()), project_path: None,
        payload: json!({"quarantines":[{"path":path,"bytes":14,"phase":"stage_before_create"}]}), inverse: None, backup_dir: None,
    }).unwrap();
    path
}
const ID: &str = "dep:v1/global/universal/universal/foo/-/%2Fmissing";

#[test]
fn cli_park_reports_recovery_on_stderr_and_json_after_restart_or_drops_backup_notice() {
    let home = tempfile::tempdir().unwrap();
    let path = interrupted(home.path());
    for _ in 0..2 {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_skill-studio"))
            .args(["park", "--id", ID, "--json", "--home"])
            .arg(home.path())
            .env("HOME", home.path())
            .env("SKILL_STUDIO_TELEMETRY", "0")
            .output()
            .unwrap();
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            result["recovery"]["retained_quarantines"][0],
            path.to_string_lossy().as_ref()
        );
        assert!(String::from_utf8(output.stderr)
            .unwrap()
            .contains(path.to_str().unwrap()));
    }
    let repeated = headless::cli(home.path(), &["park", "--id", ID]);
    assert_eq!(
        repeated["recovery"]["retained_quarantines"][0],
        path.to_string_lossy().as_ref()
    );
}

#[test]
fn mcp_park_reports_recovery_in_tool_result_after_restart_or_drops_backup_notice() {
    let home = tempfile::tempdir().unwrap();
    let path = interrupted(home.path());
    for _ in 0..2 {
        let result = headless::mcp(home.path(), "park", &json!({"deployment_id":ID}));
        assert_eq!(
            result["result"]["structuredContent"]["recovery"]["retained_quarantines"][0],
            path.to_string_lossy().as_ref()
        );
    }
}
