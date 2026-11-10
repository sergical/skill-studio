#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/headless.rs"]
mod headless;

use headless::{cli, cli_with_env, mcp, mcp_with_env};
use serde_json::{json, Value};
use std::path::Path;

const HOME_KEYS: &[&str] = &[
    "HOME",
    "CODEX_HOME",
    "CLAUDE_CONFIG_DIR",
    "GROK_HOME",
    "XDG_CONFIG_HOME",
    "XDG_STATE_HOME",
    "DOTAGENTS_HOME",
    "VIBE_HOME",
    "HERMES_HOME",
    "AUTOHAND_HOME",
];

fn envelope(response: &Value) -> &Value {
    &response["result"]["structuredContent"]
}

#[test]
fn scoped_install_update_remove_pin_every_child_home_on_cli_and_mcp_or_name_the_escape() {
    for surface in ["cli", "mcp"] {
        let home = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let bin = outside.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        std::os::unix::fs::symlink("/bin/sh", bin.join("npx")).unwrap();
        let record = outside.path().join("environment");
        let script = r#"
: > "$RECORD_PATH"
for key in HOME CODEX_HOME CLAUDE_CONFIG_DIR GROK_HOME XDG_CONFIG_HOME XDG_STATE_HOME DOTAGENTS_HOME VIBE_HOME HERMES_HOME AUTOHAND_HOME; do
    eval "value=\${$key-}"
    printf '%s=%s\n' "$key" "$value" >> "$RECORD_PATH"
done
case "$1" in
    add)
        mkdir -p "$HOME/.agents/skills/gamma"
        printf '%s\n' '---' 'name: gamma' 'description: test skill' '---' 'Body' > "$HOME/.agents/skills/gamma/SKILL.md"
        ;;
    update) printf '%s\n' 'Updated' >> "$HOME/.agents/skills/gamma/SKILL.md" ;;
    remove)
        for file in "$HOME/.agents/skills/gamma/"*; do unlink "$file"; done
        rmdir "$HOME/.agents/skills/gamma"
        ;;
esac
"#;
        std::fs::write(home.path().join("skills"), script).unwrap();
        let mut env = HOME_KEYS
            .iter()
            .map(|key| (key.to_string(), outside.path().display().to_string()))
            .collect::<Vec<_>>();
        env.push((
            "PATH".to_string(),
            format!("{}:/usr/bin:/bin", bin.display()),
        ));
        env.push(("RECORD_PATH".to_string(), record.display().to_string()));
        for operation in ["add", "update", "remove"] {
            let result = match (surface, operation) {
                ("cli", "add") => cli_with_env(home.path(), &["add", "owner/repo", "--name", "gamma", "--method", "skills-sh"], &env),
                ("cli", "update") => cli_with_env(home.path(), &["update", "gamma", "--method", "skills-sh"], &env),
                ("mcp", "add") => envelope(&mcp_with_env(home.path(), "add", &json!({"skill":"gamma","method":"skills_sh","scope":{"scope":"global"},"source":"owner/repo"}), &env)).clone(),
                ("mcp", "update") => envelope(&mcp_with_env(home.path(), "update", &json!({"skill":"gamma","method":"skills_sh","scope":{"scope":"global"},"source":"owner/repo"}), &env)).clone(),
                (_, "remove") => {
                    let scan = if surface == "cli" { cli(home.path(), &["scan"]) } else { envelope(&mcp(home.path(), "scan", &json!({}))).clone() };
                    let id = scan["data"]["skills"][0]["deployments"][0]["id"].as_str().unwrap();
                    if surface == "cli" { cli_with_env(home.path(), &["remove", "--id", id], &env) }
                    else { envelope(&mcp_with_env(home.path(), "remove", &json!({"deployment_id":id}), &env)).clone() }
                }
                _ => unreachable!(),
            };
            let values = std::fs::read_to_string(&record)
                .unwrap_or_else(|e| panic!("{surface} {operation}: {e}: {result}"));
            for line in values.lines() {
                let (key, value) = line.split_once('=').unwrap();
                if key == "XDG_STATE_HOME" {
                    assert!(
                        value.is_empty(),
                        "{surface} {operation}: {line}; lock must use HOME/.agents"
                    );
                } else {
                    assert!(
                        Path::new(value).starts_with(home.path()),
                        "{surface} {operation}: {line} escaped --home"
                    );
                }
            }
            assert_eq!(result["status"], "ok", "{surface} {operation}: {result}");
        }
        assert!(!outside.path().join(".agents/skills/gamma").exists());
    }
}

#[test]
fn cursor_capabilities_and_detection_choose_the_same_specific_runner_or_name_the_missing_path() {
    let home = tempfile::tempdir().unwrap();
    let bin = home.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    for name in ["cursor-agent", "agent"] {
        std::os::unix::fs::symlink("/bin/sh", bin.join(name)).unwrap();
    }
    let env = vec![(
        "PATH".to_string(),
        format!("{}:/usr/bin:/bin", bin.display()),
    )];
    let expected = bin.join("cursor-agent");
    for surface in ["cli", "mcp"] {
        let capabilities = if surface == "cli" {
            cli_with_env(
                home.path(),
                &["capabilities", "--harness", "cursor", "--observe"],
                &env,
            )
        } else {
            envelope(&mcp_with_env(
                home.path(),
                "capabilities",
                &json!({"harnesses":["cursor"],"observe":true,"tools":[]}),
                &env,
            ))
            .clone()
        };
        let detected = if surface == "cli" {
            cli_with_env(home.path(), &["harnesses"], &env)
        } else {
            envelope(&mcp_with_env(home.path(), "harnesses", &json!({}), &env)).clone()
        };
        let report = &capabilities["data"]["harnesses"][0];
        assert_eq!(report["harness"], "cursor", "{surface}: {capabilities}");
        assert_eq!(
            report["observed"]["runner_binary"],
            expected.to_str().unwrap(),
            "{surface}: {capabilities}"
        );
        let detection = detected["data"]["harnesses"]
            .as_array()
            .unwrap()
            .iter()
            .find(|h| h["id"] == "cursor")
            .unwrap();
        assert_eq!(
            detection["executable"], report["observed"]["runner_binary"],
            "{surface}: {detected}"
        );
    }
}
