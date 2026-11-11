#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[allow(dead_code)]
#[path = "support/headless.rs"]
mod headless;
use serde_json::json;
use std::path::Path;

fn write(home: &Path, path: &str, body: &str) {
    let path = home.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn moved_home(variable: &str, harness: &str, default: &str, config: &str, body: &str) {
    let sandbox = tempfile::tempdir().unwrap();
    let home = sandbox.path().join("home");
    std::fs::create_dir(&home).unwrap();
    write(
        sandbox.path(),
        "moved/skills/moved/SKILL.md",
        "---\nname: moved\ndescription: Moved skill\n---\n",
    );
    write(sandbox.path(), &format!("moved/{config}"), body);
    write(
        home.as_path(),
        &format!("{default}/skills/default/SKILL.md"),
        "---\nname: default\ndescription: Default skill\n---\n",
    );
    let env = vec![(
        variable.into(),
        sandbox.path().join("moved").display().to_string(),
    )];
    if harness == "claude-code" {
        write(
            sandbox.path(),
            "moved/settings.json",
            r#"{"hooks":{"PreToolUse":[{"hooks":[{"type":"command","command":"true"}]}]}}"#,
        );
    } else if harness == "grok-build" {
        write(
            sandbox.path(),
            "moved/hooks/test.json",
            r#"{"hooks":{"PreToolUse":[{"type":"command","command":"true"}]}}"#,
        );
    }
    let project = sandbox.path().join("moved/project");
    let marker = match harness {
        "claude-code" => ".claude",
        "pi" => ".pi",
        _ => ".grok",
    };
    write(
        &project,
        &format!("{marker}/skills/project-skill/SKILL.md"),
        "---\nname: project-skill\ndescription: Project skill\n---\n",
    );
    let transcript = json!({"type":"session", "cwd": project}).to_string();
    match harness {
        "claude-code" => write(
            sandbox.path(),
            "moved/projects/project/session.jsonl",
            &transcript,
        ),
        "pi" => write(
            sandbox.path(),
            "moved/sessions/project/session.jsonl",
            &transcript,
        ),
        _ => write(
            sandbox.path(),
            "moved/sessions/project-hash/.cwd",
            project.to_str().unwrap(),
        ),
    }
    for op in ["scan", "extras"] {
        let cli = headless::cli_request(home.as_path(), &[op], &env, false);
        let mcp = headless::mcp_request(home.as_path(), op, &json!({}), &env, false);
        assert_eq!(
            cli["data"], mcp["result"]["structuredContent"]["data"],
            "CLI/MCP differ"
        );
        let data = cli["data"].to_string();
        if op == "scan" {
            assert!(
                data.contains("project-skill"),
                "moved project history was missed"
            );
        }
        assert!(
            data.contains(if op == "scan" {
                "moved/skills/moved"
            } else {
                "moved-server"
            }),
            "{variable}: {op} missed moved root"
        );
        assert!(
            !data.contains("skills/default"),
            "{variable}: {op} read default root"
        );
        if op == "extras" && harness != "pi" {
            assert!(data.contains("PreToolUse"), "moved hook was missed");
        }
        let fixture_env = [
            env.clone(),
            vec![(
                "SKILL_STUDIO_FIXTURE".into(),
                home.as_path().display().to_string(),
            )],
        ]
        .concat();
        let fixture = headless::mcp_request(home.as_path(), op, &json!({}), &fixture_env, false);
        assert!(
            !fixture["result"]["structuredContent"]["data"]
                .to_string()
                .contains("moved/"),
            "fixture escaped"
        );
        let scoped = headless::cli_request(home.as_path(), &[op], &env, true);
        assert!(
            !scoped["data"].to_string().contains("moved/"),
            "explicit home escaped"
        );
        assert_eq!(cli["status"], "ok", "{harness}: operation failed");
    }
    for op in ["park", "unpark"] {
        let result = headless::cli_request(home.as_path(), &[op, "moved"], &env, false);
        assert_eq!(result["status"], "ok", "moved skill operation failed");
        assert_eq!(
            sandbox.path().join("moved/skills/moved/SKILL.md").exists(),
            op == "unpark"
        );
    }
}

#[test]
fn claude_env_scan_and_extras_use_moved_home_instead_of_default() {
    moved_home(
        "CLAUDE_CONFIG_DIR",
        "claude-code",
        ".claude",
        ".claude.json",
        r#"{"mcpServers":{"moved-server":{}}}"#,
    );
}
#[test]
fn pi_env_scan_and_extras_use_moved_home_instead_of_default() {
    moved_home(
        "PI_CODING_AGENT_DIR",
        "pi",
        ".pi/agent",
        "mcp.json",
        r#"{"mcpServers":{"moved-server":{}}}"#,
    );
}
#[test]
fn grok_env_scan_and_extras_use_moved_home_instead_of_default() {
    moved_home(
        "GROK_HOME",
        "grok-build",
        ".grok",
        "config.toml",
        "[mcp_servers.moved-server]\n",
    );
}
