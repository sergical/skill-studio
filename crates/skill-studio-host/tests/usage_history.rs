#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use skill_studio_core::{
    dto::ScanRequest,
    harness::HarnessCatalog,
    identity::CorrelationId,
    ops,
    ports::{OpContext, Runtime},
    scope::ProjectSelection,
    RuntimeScope,
};
use skill_studio_host::SkillUsage;
use std::{fs, path::Path, sync::Arc};
fn scan(home: &Path) -> skill_studio_core::dto::Inventory {
    let mut scope = RuntimeScope::live(home.to_path_buf(), home.join("history"));
    scope.projects = ProjectSelection::Explicit { paths: vec![] };
    let rt = Runtime::new(
        &scope,
        skill_studio_host::default_ports(home.join("leases"), Arc::new(HarnessCatalog::builtin())),
    )
    .unwrap();
    ops::scan(
        &rt,
        &OpContext::uncancellable(CorrelationId("usage".into())),
        &ScanRequest::default(),
    )
    .unwrap()
}
fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let skill = dir.path().join(".agents/skills/gamma");
    fs::create_dir_all(&skill).unwrap();
    fs::write(
        skill.join("SKILL.md"),
        "---\nname: gamma\ndescription: test skill\n---\nBody",
    )
    .unwrap();
    dir
}
fn report(home: &Path) -> serde_json::Value {
    serde_json::to_value(
        SkillUsage::default().report(
            home,
            &scan(home),
            30,
            chrono::DateTime::parse_from_rfc3339("2026-09-30T12:00:00Z")
                .unwrap()
                .to_utc(),
        ),
    )
    .unwrap()
}
/// Flow: read live and archived compressed Codex sessions. Expect two uses and sessions.
/// Failure: compressed history is missing or counted twice across refreshes.
#[test]
fn usage_compressed_codex_history_includes_live_and_archived_sessions() {
    let dir = fixture();
    for root in ["sessions", "archived_sessions"] {
        let path = dir.path().join(".codex").join(root);
        fs::create_dir_all(&path).unwrap();
        fs::write(
            path.join("a.jsonl.zst"),
            if root == "sessions" {
                include_bytes!("fixtures/usage-large.jsonl.zst").as_slice()
            } else {
                include_bytes!("fixtures/usage.jsonl.zst").as_slice()
            },
        )
        .unwrap();
    }
    fs::write(
        dir.path().join(".codex/sessions/broken.jsonl.zst"),
        b"not zstd",
    )
    .unwrap();
    let data = report(dir.path());
    assert!(
        data["observations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["reason"].as_str().is_some_and(|s| s.contains("decode"))),
        "{data}"
    );
    // The same session in live and archived logs is one distinct session.
    assert_eq!(data["rows"][0]["total_uses"], 2, "{data}");
    assert_eq!(data["rows"][0]["sessions"], 1, "{data}");
}
/// Flow: report Claude user, Skill tool and direct reads in separate sessions.
/// Expect three sessions and one of each trigger. Failure: the report loses trigger evidence.
#[test]
fn usage_claude_history_preserves_sessions_and_each_trigger() {
    let dir = fixture();
    let path = dir.path().join(".claude/projects/work");
    fs::create_dir_all(&path).unwrap();
    let records = [
        serde_json::json!({"type":"user","sessionId":"s1","message":{"content":"<command-name>/gamma</command-name>"}}),
        serde_json::json!({"type":"assistant","sessionId":"s2","message":{"content":[{"type":"tool_use","name":"Skill","input":{"skill":"gamma"}}]}}),
        serde_json::json!({"type":"assistant","sessionId":"s3","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"/u/.agents/skills/gamma/SKILL.md"}}]}}),
    ];
    let mut text = String::new();
    for mut record in records {
        record["timestamp"] = "2026-09-29T12:00:00Z".into();
        record["cwd"] = "/work".into();
        text.push_str(&record.to_string());
        text.push('\n');
    }
    fs::write(path.join("s.jsonl"), format!("{text}{{bad json\n")).unwrap();
    let data = report(dir.path());
    assert_eq!(data["rows"][0]["sessions"], 3, "{data}");
    assert!(
        data["observations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["reason"]
                .as_str()
                .is_some_and(|s| s.contains("1 unparseable"))),
        "{data}"
    );
    assert_eq!(
        data["rows"][0]["triggers"],
        serde_json::json!({"user":1,"agent":1,"file_read":1,"unknown":0}),
        "{data}"
    );
}
/// Flow: report with a log root that cannot be listed as a directory.
/// Expect partial status and the exact root and reason. Failure: skipped history is silent.
#[test]
fn usage_unreadable_root_names_the_root_and_marks_partial() {
    let dir = fixture();
    fs::create_dir_all(dir.path().join(".claude")).unwrap();
    fs::write(dir.path().join(".claude/projects"), "not a directory").unwrap();
    let data = report(dir.path());
    assert_eq!(data["partial"], true, "{data}");
    assert!(
        data["observations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["path"]
                .as_str()
                .is_some_and(|p| p.ends_with(".claude/projects"))
                && v["reason"].as_str().is_some_and(|r| !r.is_empty())),
        "{data}"
    );
}
