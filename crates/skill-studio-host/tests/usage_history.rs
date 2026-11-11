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

#[test]
fn usage_without_history_is_complete() {
    let dir = fixture();
    let data = report(dir.path());
    assert_eq!(data["partial"], false, "{data}");
    assert_eq!(data["rows"][0]["total_uses"], 0);
}

#[test]
fn usage_reads_large_history_in_one_report_without_a_cache() {
    let dir = fixture();
    let root = dir.path().join(".codex/sessions");
    fs::create_dir_all(&root).unwrap();
    for n in 0..64 {
        fs::write(
            root.join(format!("{n}.jsonl.zst")),
            include_bytes!("fixtures/usage-large.jsonl.zst"),
        )
        .unwrap();
    }
    let data = report(dir.path());
    assert_eq!(data["rows"][0]["total_uses"], 64, "{data}");
    assert_eq!(data["partial"], false, "{data}");
}

#[test]
fn usage_cache_round_trip_preserves_complete_history() {
    let dir = fixture();
    let root = dir.path().join(".codex/sessions");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("a.jsonl.zst"),
        include_bytes!("fixtures/usage.jsonl.zst"),
    )
    .unwrap();
    let mut usage = SkillUsage::default();
    let inventory = scan(dir.path());
    let now = chrono::Utc::now();
    let first = usage.report(dir.path(), &inventory, 45, now);
    let cache = dir.path().join("data/cli-skill-uses.json");
    usage.save_cache(&cache).unwrap();
    let bytes = fs::read(&cache).unwrap();
    let second = SkillUsage::load_read_only(Some(&cache)).report(dir.path(), &inventory, 45, now);
    assert_eq!(first, second);
    assert!(!second.partial);
    assert_eq!(second.rows[0].total_uses, 1);
    assert_eq!(fs::read(&cache).unwrap(), bytes);
    assert_eq!(fs::read_dir(cache.parent().unwrap()).unwrap().count(), 1);
}

#[test]
fn usage_oversized_without_marker_is_complete_instead_of_a_gap() {
    let dir = fixture();
    let root = dir.path().join(".codex/sessions");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("large.jsonl"),
        format!("{{\"output\":\"{}\"}}\n", "x".repeat(300_000)),
    )
    .unwrap();
    assert_eq!(report(dir.path())["partial"], false);
}

#[test]
fn usage_oversized_skill_is_counted_or_names_the_gap() {
    let dir = fixture();
    let root = dir.path().join(".claude/projects/work");
    fs::create_dir_all(&root).unwrap();
    let record = serde_json::json!({"type":"assistant","sessionId":"s","timestamp":"2026-09-29T12:00:00Z","padding":"x".repeat(300_000),"message":{"content":[{"type":"tool_use","name":"Skill","input":{"skill":"gamma"}}]}});
    fs::write(root.join("large.jsonl"), format!("{record}\n")).unwrap();
    let data = report(dir.path());
    assert!(data["rows"][0]["total_uses"] == 1 || data["partial"] == true);
    assert_eq!(data["rows"][0]["total_uses"], 1);
}

#[test]
fn usage_unterminated_tail_waits_instead_of_a_gap() {
    let dir = fixture();
    let root = dir.path().join(".codex/sessions");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("active.jsonl"), "{\"type\":").unwrap();
    assert_eq!(report(dir.path())["partial"], false);
}

#[test]
fn usage_compressed_large_records_and_active_tail_are_complete_instead_of_false_gaps() {
    let dir = fixture();
    let root = dir.path().join(".claude/projects/work");
    fs::create_dir_all(&root).unwrap();
    let record = serde_json::json!({"type":"assistant","sessionId":"s","timestamp":"2026-09-29T12:00:00Z","padding":"x".repeat(300_000),"message":{"content":[{"type":"tool_use","name":"Skill","input":{"skill":"gamma"}}]}});
    let codex_use = serde_json::json!({"type":"response_item","timestamp":"2026-09-29T12:00:00Z","padding":"x".repeat(300_000),"payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<skill>\n<name>gamma</name>\n<path>/u/.agents/skills/gamma/SKILL.md</path>"}]}});
    let text = format!(
        "{{\"output\":\"{}\"}}\n{codex_use}\n{{\"active\":",
        "x".repeat(300_000)
    );
    let codex = dir.path().join(".codex/sessions");
    fs::create_dir_all(&codex).unwrap();
    fs::write(
        codex.join("large.jsonl.zst"),
        zstd::stream::encode_all(text.as_bytes(), 1).unwrap(),
    )
    .unwrap();
    fs::write(root.join("large.jsonl"), format!("{record}\n")).unwrap();
    let data = report(dir.path());
    assert_eq!(data["partial"], false, "{data}");
    assert_eq!(data["rows"][0]["total_uses"], 2, "{data}");
}
/// Flow: `usage --days 30` with one use in the window and one 60 days old.
/// Expect `sessions` and `triggers` to count only the window, like
/// `recent_uses`. Failure: they count all history, so `--days 1` and
/// `--days 45` print the same sessions and triggers.
#[test]
fn usage_sessions_and_triggers_count_only_the_days_window() {
    let dir = fixture();
    let path = dir.path().join(".claude/projects/work");
    fs::create_dir_all(&path).unwrap();
    let records = [
        serde_json::json!({"type":"user","sessionId":"recent","timestamp":"2026-09-29T12:00:00Z","message":{"content":"<command-name>/gamma</command-name>"}}),
        serde_json::json!({"type":"assistant","sessionId":"old","timestamp":"2026-08-01T12:00:00Z","message":{"content":[{"type":"tool_use","name":"Skill","input":{"skill":"gamma"}}]}}),
    ];
    let mut text = String::new();
    for mut record in records {
        record["cwd"] = "/work".into();
        text.push_str(&record.to_string());
        text.push('\n');
    }
    fs::write(path.join("s.jsonl"), text).unwrap();
    let row = &report(dir.path())["rows"][0];
    assert_eq!(row["total_uses"], 2, "{row}");
    assert_eq!(row["recent_uses"], 1, "{row}");
    assert_eq!(row["sessions"], 1, "{row}");
    assert_eq!(
        row["triggers"],
        serde_json::json!({"user":1,"agent":0,"file_read":0,"unknown":0}),
        "{row}"
    );
}
