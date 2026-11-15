#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr
)]
#[path = "support/headless.rs"]
mod headless;
use serde_json::json;

#[test]
fn library_matches_usage_scan_and_mcp() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path();
    let md = |name: &str, body: &str| {
        format!("---\nname: {name}\ndescription: Fixture.\ndisable-model-invocation: true\n---\n{body}\n")
    };
    for index in 0..40 {
        let name = format!("skill-{index:02}");
        for dir in [".agents/skills", ".codex/skills"] {
            let path = root.join(dir).join(&name);
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(
                path.join("SKILL.md"),
                md(
                    &name,
                    if dir == ".agents/skills" {
                        "New"
                    } else {
                        "Old"
                    },
                ),
            )
            .unwrap();
        }
    }
    std::fs::create_dir_all(root.join(".claude/skills")).unwrap();
    std::os::unix::fs::symlink(
        "../../.agents/skills/skill-00",
        root.join(".claude/skills/skill-00"),
    )
    .unwrap();
    std::os::unix::fs::symlink("skill-00", root.join(".agents/skills/linked-alias")).unwrap();
    std::fs::create_dir_all(root.join(".agents/skills/not-a-skill")).unwrap();
    std::fs::write(root.join(".agents/.skill-lock.json"), json!({"version":3,"skills":{
        "skill-00":{"source":"fixture/pack","sourceType":"github","sourceUrl":"https://github.com/fixture/pack","skillFolderHash":"old","installedAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z"},
        "missing":{"source":"fixture/pack","sourceType":"github","sourceUrl":"https://github.com/fixture/pack","skillFolderHash":"old","installedAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z"}
    }}).to_string()).unwrap();
    let history = root.join(".claude/projects/work");
    std::fs::create_dir_all(&history).unwrap();
    let record = |id: &str, at: &str| {
        json!({"type":"assistant","uuid":id,"timestamp":at,"sessionId":"one-session","message":{"content":[{"type":"tool_use","id":id,"name":"Skill","input":{"skill":"skill-00"}}]}}).to_string()
    };
    std::fs::write(
        history.join("session.jsonl"),
        format!(
            "{}\n{}\n{}\n{}\n",
            record("a", "2026-10-05T00:00:00Z"),
            record("a", "2026-10-05T00:00:00Z"),
            record("b", "2026-10-06T00:00:00Z"),
            record("c", "2026-11-01T00:00:00Z")
        ),
    )
    .unwrap();
    let args = ["library", "--since", "2026-10-01", "--until", "2026-11-01"];
    let cli = headless::cli(root, &args);
    let input = json!({"since":"2026-10-01","until":"2026-11-01"});
    let mcp = headless::mcp(root, "library", &input);
    let data = &cli["data"];
    assert_eq!(
        data, &mcp["result"]["structuredContent"]["data"],
        "{cli}\n{mcp}"
    );
    assert_eq!(data["count"], 40);
    assert_eq!(data["used"], 1);
    assert_eq!(data["rows"][0]["source"], "fixture/pack");
    assert!(data["rows"][1]["source"].is_null());
    assert!(data["rows"][0].get("sessions").is_none());
    assert_eq!(data["rows"][0]["uses"], 2);
    assert_eq!(data["rows"][0]["agents"]["claude-code"], 2);
    assert_eq!(data["rows"][0]["modes"]["codex"], "auto");
    assert_eq!(data["rows"][0]["modes"]["claude-code"], "manual");
    assert_eq!(data["rows"][0]["copies"]["claude-code"], "link");
    let usage = headless::cli(
        root,
        &["usage", "--since", "2026-10-01", "--until", "2026-11-01"],
    );
    let scan = headless::cli(root, &["scan", "--short"]);
    for row in data["rows"].as_array().unwrap() {
        let used = usage["data"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|u| u["skill"] == row["name"])
            .unwrap();
        assert_eq!(row["uses"], used["recent_uses"]);
        assert_eq!(row["agents"], used["agents"]);
        let scanned = scan["data"]["skills"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == row["name"])
            .unwrap();
        let status = match scanned["copies"]["codex"].as_str().unwrap() {
            "stale" => "old",
            "differs" => "changed",
            other => panic!("unexpected {other}"),
        };
        assert_eq!(row["copies"]["codex"], status);
    }
    let top = headless::cli(
        root,
        &[
            "library",
            "--since",
            "2026-10-01",
            "--until",
            "2026-11-01",
            "--sort",
            "uses",
            "--top",
            "1",
        ],
    );
    assert_eq!(top["data"]["count"], 40);
    assert_eq!(top["data"]["rows"].as_array().unwrap().len(), 1);
    assert_eq!(top["data"]["rows"][0]["name"], "skill-00");
    let text = std::process::Command::new(env!("CARGO_BIN_EXE_skill-studio"))
        .args(args)
        .arg("--home")
        .arg(root)
        .env_clear()
        .env("HOME", root)
        .env("PATH", "/usr/bin:/bin")
        .env("SKILL_STUDIO_TELEMETRY", "0")
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        text.status.success(),
        "{}",
        String::from_utf8_lossy(&text.stderr)
    );
    assert!(String::from_utf8_lossy(&text.stdout).starts_with("Library: 40 skills, 1 used;"));
    eprintln!(
        "40-skill fixture: JSON data={} bytes; JSON envelope={} bytes; text={} bytes",
        data.to_string().len(),
        serde_json::to_vec(&cli).unwrap().len() + 1,
        text.stdout.len()
    );
}

#[test]
fn library_keeps_partial_tables_and_the_worst_agent_copy() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path();
    for (path, manual) in [
        (".agents/skills/alpha", false),
        (".config/opencode/skill/alpha", true),
        (".codex/skills/unrelated", false),
    ] {
        let folder = root.join(path);
        std::fs::create_dir_all(&folder).unwrap();
        let name = if path.ends_with("unrelated") {
            "unrelated"
        } else {
            "alpha"
        };
        std::fs::write(folder.join("SKILL.md"), format!("---\nname: {name}\ndescription: Fixture.\ndisable-model-invocation: {manual}\n---\n")).unwrap();
    }
    let bad_sidecar = root.join(".codex/skills/unrelated/agents");
    std::fs::create_dir_all(&bad_sidecar).unwrap();
    std::fs::write(bad_sidecar.join("openai.yaml"), "policy: [unterminated").unwrap();
    std::fs::create_dir_all(root.join(".config/opencode/skills")).unwrap();
    std::os::unix::fs::symlink(
        root.join(".agents/skills/alpha"),
        root.join(".config/opencode/skills/alpha"),
    )
    .unwrap();
    let call = || {
        let cli = headless::cli(root, &["library", "--days", "45", "--read-timeout-ms", "0"]);
        let mcp = headless::mcp(root, "library", &json!({"days":45}));
        assert_eq!(
            cli["data"], mcp["result"]["structuredContent"]["data"],
            "{cli}\n{mcp}"
        );
        assert_eq!(cli["data"]["count"], 1, "{cli}");
        assert_eq!(
            cli["data"]["rows"][0]["copies"]["open-code"], "changed",
            "{cli}"
        );
        assert_eq!(
            cli["data"]["rows"][0]["modes"]["open-code"], "manual",
            "{cli}"
        );
        cli
    };
    let intact = call();
    assert_eq!(intact["status"], "ok", "{intact}");
    assert_eq!(intact["data"]["days"], 45);
    let lock = root.join(".agents/.skill-lock.json");
    std::fs::write(&lock, "invalid json").unwrap();
    let bad_lock = call();
    assert_eq!(bad_lock["status"], "partial", "{bad_lock}");
    assert!(bad_lock["data"]["rows"][0]["source"].is_null());
    std::fs::remove_file(lock).unwrap();
    let library_agents = root.join(".agents/skills/alpha/agents");
    std::fs::create_dir_all(&library_agents).unwrap();
    std::fs::write(library_agents.join("openai.yaml"), "policy: [unterminated").unwrap();
    let bad_mode = call();
    assert_eq!(bad_mode["status"], "partial", "{bad_mode}");
    assert!(bad_mode["data"]["rows"][0]["modes"].get("codex").is_none());
}

#[test]
fn library_ignores_broken_agent_links_for_modes() {
    let home = tempfile::tempdir().unwrap();
    let folder = home.path().join(".agents/skills/alpha");
    std::fs::create_dir_all(folder.join("agents")).unwrap();
    std::fs::write(
        folder.join("SKILL.md"),
        "---\nname: alpha\ndescription: Fixture.\n---\n",
    )
    .unwrap();
    std::fs::write(
        folder.join("agents/openai.yaml"),
        "policy:\n  allow_implicit_invocation: false\n",
    )
    .unwrap();
    let codex = home.path().join(".codex/skills");
    std::fs::create_dir_all(&codex).unwrap();
    std::os::unix::fs::symlink("missing", codex.join("alpha")).unwrap();
    let cli = headless::cli(home.path(), &["library"]);
    let mcp = headless::mcp(home.path(), "library", &json!({}));
    assert_eq!(cli["data"], mcp["result"]["structuredContent"]["data"]);
    assert_eq!(cli["data"]["count"], 1, "{cli}");
    assert_eq!(cli["data"]["rows"][0]["modes"]["codex"], "manual", "{cli}");
    assert!(cli["data"]["rows"][0]["copies"].get("codex").is_none());
}
