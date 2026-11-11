#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]
#[path = "support/headless.rs"]
mod headless;
use serde_json::{json, Value};

fn write(root: &std::path::Path, path: &str, body: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

#[test]
fn extras_lists_names_and_locations_instead_of_secret_values_with_cli_mcp_parity() {
    let home = tempfile::tempdir().unwrap();
    let project = home.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    let config = r#"{"hooks":{"PreToolUse":[{"matcher":"SECRET_MATCHER","hooks":[{"command":"SECRET_COMMAND"},{"command":"SECRET_COMMAND"}]}]},"mcpServers":{"private-server":{"headers":{"Authorization":"SECRET_AUTH"},"env":{"TOKEN":"SECRET_TOKEN"},"url":"SECRET_URL"}}}"#;
    for path in [
        ".claude/settings.json",
        ".claude/settings.local.json",
        ".cursor/mcp.json",
        ".cursor/hooks.json",
        ".codex/hooks.json",
        ".pi/agent/mcp.json",
        "project/.mcp.json",
        "project/.claude/settings.json",
        "project/.claude/settings.local.json",
        "project/.cursor/mcp.json",
        "project/.cursor/hooks.json",
        "project/.pi/mcp.json",
        "project/.codex/hooks.json",
        ".grok/hooks/session.json",
        "project/.grok/hooks/session.json",
    ] {
        write(home.path(), path, config);
    }
    write(home.path(), ".claude.json", &json!({"mcpServers":{"global-server":{"command":"SECRET_COMMAND"}},"projects":{project.to_str().unwrap():{"mcpServers":{"local-server":{"env":{"TOKEN":"SECRET_TOKEN"}}}}}}).to_string());
    for path in [
        ".codex/config.toml",
        "project/.codex/config.toml",
        ".grok/config.toml",
        "project/.grok/config.toml",
    ] {
        write(home.path(), path, "[mcp_servers.toml-server]\ncommand = 'SECRET_COMMAND'\nheaders = { Authorization = 'SECRET_AUTH' }\nurl = 'SECRET_URL'\n[mcp_servers.toml-server.env]\nTOKEN = 'SECRET_TOKEN'\n[[hooks.Stop]]\n[[hooks.Stop.hooks]]\ncommand = 'SECRET_COMMAND'\n");
    }
    for path in [".config/opencode/opencode.jsonc", "project/opencode.jsonc"] {
        write(
            home.path(),
            path,
            "{// comment\n\"mcp\": {\"jsonc-server\": {\"url\":\"SECRET_URL\",\"headers\":{\"Authorization\":\"SECRET_AUTH\"},\"env\":{\"TOKEN\":\"SECRET_TOKEN\"},},},}",
        );
    }
    for path in [
        ".claude/CLAUDE.md",
        ".codex/AGENTS.md",
        ".config/opencode/AGENTS.md",
        ".pi/agent/AGENTS.md",
        ".grok/AGENTS.md",
        "project/AGENTS.md",
        "project/CLAUDE.md",
        ".pi/agent/SYSTEM.md",
        "project/.pi/APPEND_SYSTEM.md",
    ] {
        write(home.path(), path, "SECRET_INSTRUCTIONS");
    }
    write(home.path(), "project/opencode.json", "{SECRET_BROKEN");
    let cli = headless::cli(
        home.path(),
        &["extras", "--project", project.to_str().unwrap()],
    );
    let env = vec![(
        "SKILL_STUDIO_PROJECT".to_string(),
        project.to_str().unwrap().to_string(),
    )];
    let mcp = headless::mcp_with_env(home.path(), "extras", &json!({}), &env);
    let mcp_text = serde_json::to_string(&mcp).unwrap();
    let mcp = &mcp["result"]["structuredContent"];
    for field in ["schema_version", "operation", "status", "errors"] {
        assert_eq!(cli[field], mcp[field]);
    }
    assert_eq!(cli["operation"], "extras");
    assert_eq!(cli["data"], mcp["data"]);
    let text = serde_json::to_string(&cli).unwrap();
    for secret in [
        "SECRET_MATCHER",
        "SECRET_COMMAND",
        "SECRET_AUTH",
        "SECRET_TOKEN",
        "SECRET_URL",
        "SECRET_INSTRUCTIONS",
        "SECRET_BROKEN",
    ] {
        assert!(
            !text.contains(secret) && !mcp_text.contains(secret),
            "output exposed a config value"
        );
    }
    let rows = cli["data"]["entries"].as_array().unwrap();
    for name in [
        "private-server",
        "global-server",
        "local-server",
        "toml-server",
        "jsonc-server",
        "PreToolUse",
        "Stop",
    ] {
        assert!(
            rows.iter().any(|r| r["name"] == name),
            "missing declaration name"
        );
    }
    assert!(rows.iter().any(|r| r["kind"] == "hook" && r["count"] == 2));
    assert!(rows.iter().any(|r| r["status"] == "unreadable"
        && r["path"] == project.join("opencode.json").to_str().unwrap()));
    for harness in [
        "claude-code",
        "codex",
        "opencode",
        "pi",
        "cursor",
        "grok-build",
    ] {
        assert!(
            rows.iter().any(|r| r["harness"] == harness),
            "missing harness"
        );
    }
    for (path, name) in [
        (".claude/settings.json", "PreToolUse"),
        (".claude/settings.local.json", "PreToolUse"),
        (".cursor/hooks.json", "PreToolUse"),
        (".grok/hooks/session.json", "PreToolUse"),
        ("project/.grok/hooks/session.json", "PreToolUse"),
        (".codex/config.toml", "toml-server"),
        ("project/.codex/config.toml", "toml-server"),
        (".grok/config.toml", "toml-server"),
        ("project/.grok/config.toml", "toml-server"),
        (".pi/agent/mcp.json", "private-server"),
        ("project/.pi/mcp.json", "private-server"),
        (".cursor/mcp.json", "private-server"),
        ("project/.cursor/mcp.json", "private-server"),
        ("project/.mcp.json", "private-server"),
        (".config/opencode/opencode.jsonc", "jsonc-server"),
        ("project/opencode.jsonc", "jsonc-server"),
        (".pi/agent/SYSTEM.md", "SYSTEM.md"),
        ("project/.pi/APPEND_SYSTEM.md", "APPEND_SYSTEM.md"),
    ] {
        assert!(rows.iter().any(|r| r["path"] == home.path().join(path).to_str().unwrap() && r["name"] == name), "missing declaration in {path}");
    }
    let local = rows.iter().find(|r| r["name"] == "local-server").unwrap();
    assert_eq!(local["scope"], "project");
    assert_eq!(local["project"], project.to_str().unwrap());
    assert!(rows.iter().any(|r| r["kind"] == "instruction"));
    assert_eq!(cli["status"], Value::String("ok".into()));
}

#[test]
fn extras_reports_each_bad_file_as_unreadable_instead_of_exposing_parser_errors() {
    let home = tempfile::tempdir().unwrap();
    for path in [
        ".claude/settings.json",
        ".codex/config.toml",
        ".config/opencode/opencode.jsonc",
        ".cursor/mcp.json",
        ".pi/agent/mcp.json",
        ".grok/config.toml",
    ] {
        write(home.path(), path, "SECRET_INVALID_CONFIG");
    }
    let cli = headless::cli(home.path(), &["extras"]);
    assert_eq!(cli["operation"], "extras");
    let rows = cli["data"]["entries"].as_array().unwrap();
    assert_eq!(rows.len(), 6);
    assert!(rows
        .iter()
        .all(|row| row["status"] == "unreadable" && row["name"].is_null()));
    assert!(!serde_json::to_string(&cli)
        .unwrap()
        .contains("SECRET_INVALID_CONFIG"));
}
