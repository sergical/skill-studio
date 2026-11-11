#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::path::Path;
use std::process::Command;

fn run(home: &Path, args: &[&str]) -> serde_json::Value {
    let out = Command::new(env!("CARGO_BIN_EXE_skill-studio"))
        .arg("review")
        .args(args)
        .args(["--fixture", home.to_str().unwrap(), "--json"])
        .env("HOME", home)
        .env("SKILL_STUDIO_TELEMETRY", "0")
        .output()
        .unwrap();
    serde_json::from_slice(&out.stdout).expect("review returns an envelope")
}

#[test]
fn review_set_due_counts_fixture_sessions_and_clear_removes_or_future_dates_leak() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join(".claude/skills/gamma");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("SKILL.md"),
        "---\nname: gamma\ndescription: Test.\n---\n",
    )
    .unwrap();
    let logs = home.path().join(".claude/projects/work");
    std::fs::create_dir_all(&logs).unwrap();
    std::fs::write(logs.join("session.jsonl"), concat!(
        "{\"type\":\"assistant\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-01T00:00:00Z\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"name\":\"Skill\",\"input\":{\"skill\":\"gamma\"}}]}}\n",
        "bad json\n")).unwrap();
    assert_eq!(
        run(home.path(), &["set", "gamma", "2000-01-01"])["status"],
        "ok"
    );
    let due = run(home.path(), &["due"]);
    assert_eq!(due["data"]["rows"][0]["skill"], "gamma");
    assert_eq!(due["data"]["rows"][0]["review_date"], "2000-01-01");
    assert_eq!(due["data"]["rows"][0]["sessions"], 1);
    assert_eq!(due["data"]["partial"], true);
    assert!(!due["data"]["observations"].as_array().unwrap().is_empty());
    assert!(home.path().join(".agents/review-usage.json").exists());
    run(home.path(), &["set", "gamma", "2999-01-01"]);
    assert_eq!(
        run(home.path(), &["due"])["data"]["rows"],
        serde_json::json!([])
    );
    run(home.path(), &["set", "gamma", "2000-01-01"]);
    run(home.path(), &["clear", "gamma"]);
    assert_eq!(
        run(home.path(), &["due"])["data"]["rows"],
        serde_json::json!([])
    );
}

#[test]
fn review_unknown_skill_is_refused_instead_of_storing_a_date() {
    let home = tempfile::tempdir().unwrap();
    for args in [
        vec!["set", "missing", "2000-01-01"],
        vec!["clear", "missing"],
    ] {
        let result = run(home.path(), &args);
        assert_eq!(result["errors"][0]["code"], "invalid_request");
        assert!(result["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("missing"));
    }
}
