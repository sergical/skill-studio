#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use headless::test_home;

#[allow(dead_code)]
#[path = "support/headless.rs"]
mod headless;

fn write(home: &std::path::Path, path: &str, text: &str) {
    let path = home.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn skill(home: &std::path::Path, root: &str, name: &str, description: &str) {
    write(
        home,
        &format!("{root}/{name}/SKILL.md"),
        &format!("---\nname: {name}\ndescription: {description}\n---\n"),
    );
}

fn sessions(home: &std::path::Path) -> serde_json::Value {
    let mut cmd = test_home::command(env!("CARGO_BIN_EXE_skill-studio"), home);
    cmd.args([
        "sessions",
        "--fixture",
        home.to_str().unwrap(),
        "--json",
        "--limit",
        "1000",
    ])
    .env("HOME", home)
    .env("CODEX_HOME", home.join(".codex"))
    .env("XDG_CONFIG_HOME", home.join(".config"))
    .env("SKILL_STUDIO_TELEMETRY", "0");
    let out = cmd.output().unwrap();
    let envelope = serde_json::from_slice::<serde_json::Value>(&out.stdout)
        .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&out.stderr)));
    assert_eq!(envelope["status"], "ok", "{envelope}");
    envelope["data"].clone()
}

fn claude_session(home: &std::path::Path, id: &str, used: Option<&str>) {
    let mut lines = vec![
        serde_json::json!({"type":"attachment","timestamp":"2026-11-19T00:00:00Z","sessionId":id,"cwd":"/work","attachment":{"type":"skill_listing","isInitial":true,"names":["helper","idle"],"content":"- helper\n- idle: Sits in the listing"}}).to_string(),
        serde_json::json!({"type":"user","timestamp":"2026-11-19T00:00:01Z","message":{"content":"do the thing"}}).to_string(),
    ];
    if let Some(skill) = used {
        lines.push(serde_json::json!({"type":"assistant","timestamp":"2026-11-19T00:00:02Z","sessionId":id,"cwd":"/work","message":{"model":"claude-opus-5-5","content":[{"type":"tool_use","id":format!("t{id}"),"name":"Skill","input":{"skill":skill}}]}}).to_string());
    }
    write(
        home,
        &format!(".claude/projects/-work/{id}.jsonl"),
        &(lines.join("\n") + "\n"),
    );
}

fn rollup<'a>(data: &'a serde_json::Value, harness: &str, name: &str) -> &'a serde_json::Value {
    data["skills"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["harness"] == harness && s["name"] == name)
        .unwrap_or_else(|| panic!("{name} missing for {harness}"))
}

#[test]
fn sessions_flag_a_skill_used_while_name_only_and_one_listed_every_time_but_never_used() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    skill(home, ".claude/skills", "helper", "Helps");
    skill(home, ".claude/skills", "idle", "Sits in the listing");
    // Codex reads .agents/skills, not .claude/skills.
    skill(
        home,
        ".agents/skills",
        "shared-only",
        "Only in the shared folder",
    );
    claude_session(home, "s1", Some("helper"));
    claude_session(home, "s2", None);
    claude_session(home, "s3", None);

    let data = sessions(home);
    let claude = data["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["harness"] == "claude-code")
        .unwrap();
    assert_eq!(claude["sessions_read"], 3, "{claude}");
    assert_eq!(claude["sessions_with_use"], 1, "{claude}");

    let helper = rollup(&data, "claude-code", "helper");
    assert_eq!(helper["name_only"], 3, "{helper}");
    assert_eq!(helper["used_name_only"], 1, "{helper}");
    assert!(helper["flags"]
        .as_array()
        .unwrap()
        .contains(&"used_while_name_only".into()));

    let idle = rollup(&data, "claude-code", "idle");
    assert_eq!(idle["described"], 3, "{idle}");
    assert!(idle["flags"]
        .as_array()
        .unwrap()
        .contains(&"listed_never_used".into()));

    let unreachable = rollup(&data, "claude-code", "shared-only");
    assert_eq!(unreachable["flags"], serde_json::json!(["not_reachable"]));
    assert_eq!(unreachable["reach"], "Not in a folder Claude Code reads.");

    let cursor = data["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["harness"] == "cursor")
        .unwrap();
    assert_eq!(cursor["records"], "uses_only", "{cursor}");
}
