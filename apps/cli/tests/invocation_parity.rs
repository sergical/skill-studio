#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "support/headless.rs"]
mod headless;
use serde_json::json;

const MD: &str = "---\nname: gamma\ndescription: parity\nlicense: 'MIT' # keep\n---\nBody.\n";
const YAML: &str = "interface:\n  display_name: 'Gamma' # keep\npolicy:\n  other: true # keep\n";
fn fixture() -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    for root in [".claude/skills", ".codex/skills"] {
        let dir = home.path().join(root).join("gamma");
        std::fs::create_dir_all(dir.join("agents")).unwrap();
        std::fs::write(dir.join("SKILL.md"), MD).unwrap();
        std::fs::write(dir.join("agents/openai.yaml"), YAML).unwrap();
    }
    home
}

#[test]
fn invocation_check_flags_missing_codex_policy_instead_of_hiding_mismatch() {
    let home = fixture();
    std::fs::write(
        home.path().join(".claude/skills/gamma/SKILL.md"),
        MD.replace("license:", "disable-model-invocation: true\nlicense:"),
    )
    .unwrap();
    let result = headless::cli(home.path(), &["invocation", "gamma"]);
    assert_eq!(result["data"]["skills"][0]["mismatch"], true, "{result}");
}

#[test]
fn invocation_manual_preserves_other_keys_and_undo_restores_instead_of_losing_bytes() {
    let home = fixture();
    let result = headless::cli(home.path(), &["invocation", "gamma", "--manual"]);
    assert_eq!(result["status"], "ok", "{result}");
    for root in [".claude/skills", ".codex/skills"] {
        let dir = home.path().join(root).join("gamma");
        assert!(std::fs::read_to_string(dir.join("SKILL.md"))
            .unwrap()
            .contains("license: 'MIT' # keep\n"));
        assert!(std::fs::read_to_string(dir.join("SKILL.md"))
            .unwrap()
            .contains("disable-model-invocation: true"));
        let yaml = std::fs::read_to_string(dir.join("agents/openai.yaml")).unwrap();
        assert!(yaml.contains("allow_implicit_invocation: false"));
        assert!(yaml.contains("  other: true # keep\n"));
        assert!(yaml.contains("  display_name: 'Gamma' # keep\n"));
    }
    for id in result["data"]["event_ids"].as_array().unwrap() {
        let undo = headless::cli(
            home.path(),
            &["restore", "--event-id", id.as_str().unwrap()],
        );
        assert_eq!(undo["status"], "ok", "{undo}");
    }
    for root in [".claude/skills", ".codex/skills"] {
        let dir = home.path().join(root).join("gamma");
        assert_eq!(std::fs::read_to_string(dir.join("SKILL.md")).unwrap(), MD);
        assert_eq!(
            std::fs::read_to_string(dir.join("agents/openai.yaml")).unwrap(),
            YAML
        );
    }
}

#[test]
fn invocation_mcp_matches_cli_modes_instead_of_leaving_codex_auto() {
    let cli_home = fixture();
    let mcp_home = fixture();
    let cli = headless::cli(cli_home.path(), &["invocation", "gamma", "--manual"]);
    let rpc = headless::mcp(
        mcp_home.path(),
        "invocation",
        &json!({"skill":"gamma", "policy":"user-only"}),
    );
    let mcp = &rpc["result"]["structuredContent"];
    assert_eq!(mcp["status"], cli["status"], "{rpc}");
    assert_eq!(mcp["data"]["skills"][0]["mismatch"], false, "{rpc}");
    for root in [".claude/skills", ".codex/skills"] {
        for file in ["SKILL.md", "agents/openai.yaml"] {
            let path = format!("{root}/gamma/{file}");
            assert_eq!(
                std::fs::read(cli_home.path().join(&path)).unwrap(),
                std::fs::read(mcp_home.path().join(&path)).unwrap()
            );
        }
    }
}

#[test]
fn invocation_manual_creates_codex_policy_and_auto_clears_it_without_claiming_unsupported_harnesses(
) {
    let home = tempfile::tempdir().unwrap();
    for root in [".agents/skills", ".config/opencode/skills", ".grok/skills"] {
        let dir = home.path().join(root).join("gamma");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), MD).unwrap();
    }
    for flag in ["--manual", "--auto"] {
        let result = headless::cli(home.path(), &["invocation", "gamma", flag]);
        assert_eq!(result["status"], "ok", "{result}");
        let rows = result["data"]["skills"][0]["deployments"]
            .as_array()
            .unwrap();
        for harness in ["open-code", "grok-build"] {
            assert!(
                rows.iter()
                    .any(|r| r["harness"] == harness && r["mode"] == "unsupported"),
                "{result}"
            );
        }
        for row in rows {
            let expected = if matches!(row["harness"].as_str(), Some("open-code" | "grok-build")) {
                "unsupported"
            } else if flag == "--manual" {
                "manual"
            } else {
                "auto"
            };
            assert_eq!(row["mode"], expected, "{result}");
        }
        assert_eq!(result["data"]["skills"][0]["mismatch"], false);
    }
}

#[test]
fn invocation_set_refuses_external_sidecar_instead_of_writing_outside_scope() {
    let home = fixture();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("openai.yaml"), YAML).unwrap();
    let sidecar = home.path().join(".codex/skills/gamma/agents/openai.yaml");
    std::fs::remove_file(&sidecar).unwrap();
    std::os::unix::fs::symlink(outside.path().join("openai.yaml"), &sidecar).unwrap();
    let result = headless::cli(home.path(), &["invocation", "gamma", "--manual"]);
    assert_eq!(result["status"], "error", "{result}");
    assert!(
        result["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("symlink"),
        "{result}"
    );
    assert_eq!(
        std::fs::read_to_string(outside.path().join("openai.yaml")).unwrap(),
        YAML
    );
    assert_eq!(
        std::fs::read_to_string(home.path().join(".claude/skills/gamma/SKILL.md")).unwrap(),
        MD
    );
}
