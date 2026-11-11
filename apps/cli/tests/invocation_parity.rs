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
        assert!(
            rows.iter()
                .any(|r| r["harness"] == "grok-build" && r["mode"] == "unsupported"),
            "{result}"
        );
        for row in rows {
            let expected = if matches!(row["harness"].as_str(), Some("grok-build")) {
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

#[test]
fn invocation_refuses_plugin_directory_alias() {
    let home = tempfile::tempdir().unwrap();
    let plugin = home
        .path()
        .join(".claude/plugins/cache/vendor/tool/1.0/skills/gamma");
    std::fs::create_dir_all(&plugin).unwrap();
    std::fs::write(plugin.join("SKILL.md"), MD).unwrap();
    std::fs::create_dir_all(home.path().join(".claude/skills")).unwrap();
    std::os::unix::fs::symlink(&plugin, home.path().join(".claude/skills/alias")).unwrap();
    let result = headless::cli(home.path(), &["invocation", "alias", "--manual"]);
    assert_eq!(result["status"], "error", "{result}");
    assert_eq!(
        std::fs::read_to_string(plugin.join("SKILL.md")).unwrap(),
        MD
    );
    assert!(!plugin.join("agents/openai.yaml").exists());
}

#[test]
fn invocation_refuses_link_without_editable_owner_in_cursor_and_grok_plugins() {
    for target in [
        ".cursor/plugins/cache/vendor/tool/1.0/skills/gamma",
        ".grok/plugins/tool/skills/gamma",
    ] {
        let home = tempfile::tempdir().unwrap();
        let plugin = home.path().join(target);
        std::fs::create_dir_all(&plugin).unwrap();
        std::fs::write(plugin.join("SKILL.md"), MD).unwrap();
        std::fs::create_dir_all(home.path().join(".claude/skills")).unwrap();
        std::os::unix::fs::symlink(&plugin, home.path().join(".claude/skills/alias")).unwrap();
        let result = headless::cli(home.path(), &["invocation", "alias", "--manual"]);
        assert_eq!(result["status"], "error", "{target}: {result}");
        assert_eq!(
            std::fs::read_to_string(plugin.join("SKILL.md")).unwrap(),
            MD,
            "{target}"
        );
        assert!(!plugin.join("agents/openai.yaml").exists(), "{target}");
    }
}

#[test]
fn invocation_edits_shared_install_once_through_verified_link() {
    let home = tempfile::tempdir().unwrap();
    let shared = home.path().join(".agents/skills/gamma");
    std::fs::create_dir_all(&shared).unwrap();
    std::fs::write(shared.join("SKILL.md"), MD).unwrap();
    std::fs::write(home.path().join(".agents/.skill-lock.json"), r#"{"version":3,"skills":{"gamma":{"source":"owner/repo","sourceType":"github","sourceUrl":"https://github.com/owner/repo","skillFolderHash":"abc","installedAt":"2024-01-01T00:00:00Z","updatedAt":"2024-01-01T00:00:00Z"}}}"#).unwrap();
    std::fs::create_dir_all(home.path().join(".claude/skills")).unwrap();
    std::os::unix::fs::symlink(&shared, home.path().join(".claude/skills/gamma")).unwrap();
    let result = headless::cli(home.path(), &["invocation", "gamma", "--manual"]);
    assert_eq!(result["status"], "ok", "{result}");
    assert_eq!(result["data"]["event_ids"].as_array().unwrap().len(), 1);
    assert_eq!(result["data"]["skills"][0]["mismatch"], false);
}

#[test]
fn invocation_opencode_override_is_effective_and_patched() {
    for flag in ["true", "1", "false", "0"] {
        let home = tempfile::tempdir().unwrap();
        let dir = home.path().join(".config/opencode/skills/gamma");
        std::fs::create_dir_all(&dir).unwrap();
        let md = MD.replace("license:", &format!("disable-model-invocation: true\nmetadata:\n  opencode/autoinvoke: {flag} # keep\nlicense:"));
        std::fs::write(dir.join("SKILL.md"), &md).unwrap();
        std::fs::create_dir_all(dir.join("agents")).unwrap();
        std::fs::write(
            dir.join("agents/openai.yaml"),
            "policy:\n  allow_implicit_invocation: false\n",
        )
        .unwrap();
        let result = headless::cli(home.path(), &["invocation", "gamma"]);
        let auto = matches!(flag, "true" | "1");
        assert_eq!(result["data"]["skills"][0]["mismatch"], auto, "{result}");
        assert_eq!(
            result["data"]["skills"][0]["deployments"][0]["mode"],
            if auto { "auto" } else { "manual" },
            "{result}"
        );
        let result = headless::cli(home.path(), &["invocation", "gamma", "--manual"]);
        assert_eq!(result["status"], "ok", "{result}");
        assert_eq!(result["data"]["skills"][0]["mismatch"], false, "{result}");
        let expected = if auto {
            md.replace(&format!("{flag} # keep"), "false # keep")
        } else {
            md
        };
        assert_eq!(
            std::fs::read_to_string(dir.join("SKILL.md")).unwrap(),
            expected
        );
    }
}

#[test]
fn invocation_codex_quoted_keys_preserve_bytes() {
    let home = fixture();
    let path = home.path().join(".codex/skills/gamma/agents/openai.yaml");
    let yaml = "# keep\r\n\"policy\":\r\n  'allow_implicit_invocation':  true  # keep\r\n";
    std::fs::write(&path, yaml).unwrap();
    let result = headless::cli(home.path(), &["invocation", "gamma", "--manual"]);
    assert_eq!(result["status"], "ok", "{result}");
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        yaml.replace("true", "false")
    );
    assert_eq!(result["data"]["event_ids"].as_array().unwrap().len(), 2);
}

#[test]
fn invocation_codex_flow_mapping_refused_before_any_write() {
    let home = fixture();
    let path = home.path().join(".codex/skills/gamma/agents/openai.yaml");
    let yaml = "# keep\r\npolicy: {allow_implicit_invocation: true} # keep\r\n";
    std::fs::write(&path, yaml).unwrap();
    let result = headless::cli(home.path(), &["invocation", "gamma", "--manual"]);
    assert_eq!(result["status"], "error", "{result}");
    assert_eq!(std::fs::read_to_string(path).unwrap(), yaml);
    assert_eq!(
        std::fs::read_to_string(home.path().join(".claude/skills/gamma/SKILL.md")).unwrap(),
        MD
    );
}

#[test]
fn invocation_frontmatter_preserves_comments_and_spacing() {
    let home = fixture();
    let path = home.path().join(".codex/skills/gamma/SKILL.md");
    let md = MD
        .replace(
            "license:",
            "disable-model-invocation:  false  # keep inline\n  # keep indented\nlicense:",
        )
        .replace('\n', "\r\n");
    std::fs::write(&path, &md).unwrap();
    let result = headless::cli(home.path(), &["invocation", "gamma", "--manual"]);
    assert_eq!(result["status"], "ok", "{result}");
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        md.replace("false", "true")
    );
}
