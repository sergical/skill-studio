#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::path::Path;
use std::process::Command;

fn run(home: &Path, args: &[&str]) -> serde_json::Value {
    let out = Command::new(env!("CARGO_BIN_EXE_skill-studio"))
        .arg("review")
        .args(args)
        .args(["--home", home.to_str().unwrap(), "--json"])
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
    let yesterday = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();
    std::fs::write(logs.join("session.jsonl"), concat!(
        "{\"type\":\"assistant\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-01T00:00:00Z\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"name\":\"Skill\",\"input\":{\"skill\":\"gamma\"}}]}}\n",
        "bad json\n").replace("2026-01-01T00:00:00Z", &yesterday)).unwrap();
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

fn gamma_with_one_session(home: &Path) {
    let root = home.join(".claude/skills/gamma");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("SKILL.md"),
        "---\nname: gamma\ndescription: Test.\n---\n",
    )
    .unwrap();
    let logs = home.join(".claude/projects/work");
    std::fs::create_dir_all(&logs).unwrap();
    let yesterday = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();
    std::fs::write(logs.join("session.jsonl"), "{\"type\":\"assistant\",\"sessionId\":\"session-1\",\"timestamp\":\"2026-01-01T00:00:00Z\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"name\":\"Skill\",\"input\":{\"skill\":\"gamma\"}}]}}\n".replace("2026-01-01T00:00:00Z", &yesterday)).unwrap();
}

#[test]
fn review_removed_skill_leaves_due_and_can_be_cleared_instead_of_staying_stuck() {
    let home = tempfile::tempdir().unwrap();
    gamma_with_one_session(home.path());
    assert_eq!(
        run(home.path(), &["set", "gamma", "2000-01-01"])["status"],
        "ok"
    );
    std::fs::remove_dir_all(home.path().join(".claude/skills/gamma")).unwrap();
    assert_eq!(
        run(home.path(), &["due"])["data"]["rows"],
        serde_json::json!([])
    );
    assert_eq!(run(home.path(), &["clear", "gamma"])["status"], "ok");
}

#[test]
fn review_due_reuses_the_cli_usage_cache_instead_of_writing_a_second_index() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().canonicalize().unwrap();
    gamma_with_one_session(&home);
    let cli = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_skill-studio"))
            .args(args)
            .arg("--json")
            .env("HOME", &home)
            .env("SKILL_STUDIO_TELEMETRY", "0")
            .output()
            .unwrap();
        serde_json::from_slice::<serde_json::Value>(&out.stdout).expect("envelope")
    };
    cli(&["usage"]);
    let cache = walkdir(&home)
        .into_iter()
        .find(|p| p.file_name().unwrap() == "cli-skill-uses.json")
        .expect("CLI usage cache");
    let epoch = std::fs::FileTimes::new().set_modified(std::time::SystemTime::UNIX_EPOCH);
    std::fs::File::options()
        .write(true)
        .open(&cache)
        .unwrap()
        .set_times(epoch)
        .unwrap();
    cli(&["review", "set", "gamma", "2000-01-01"]);
    let due = cli(&["review", "due"]);
    assert_eq!(due["data"]["rows"][0]["sessions"], 1, "{due}");
    assert_eq!(
        std::fs::metadata(&cache).unwrap().modified().unwrap(),
        std::time::SystemTime::UNIX_EPOCH
    );
    assert!(!home.join(".agents/review-usage.json").exists());
}

fn walkdir(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walkdir(&path));
        } else {
            out.push(path);
        }
    }
    out
}
