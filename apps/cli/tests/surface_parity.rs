#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;

#[path = "support/headless.rs"]
mod headless;
use headless::{cli, mcp};

fn shared(home: &std::path::Path) {
    let root = home.join(".agents/skills/gamma");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("SKILL.md"),
        "---\nname: gamma\ndescription: test skill\n---\nBody\n",
    )
    .unwrap();
}

#[test]
fn turn_off_requires_confirmation_then_parks_only_codex_or_names_the_surface() {
    for surface in ["cli", "mcp"] {
        let home = tempfile::tempdir().unwrap();
        shared(home.path());
        let scan = cli(home.path(), &["scan"]);
        let deployment = &scan["data"]["skills"][0]["deployments"][0]["id"];
        let id = deployment.as_str().unwrap();
        let call = |confirmed| {
            if surface == "cli" {
                let mut args = vec!["turn-off", "--id", id, "--agent", "codex"];
                if confirmed {
                    args.push("--confirm");
                }
                cli(home.path(), &args)
            } else {
                mcp(
                    home.path(),
                    "turn_off_for_agent",
                    &json!({"deployment_id":id,"agent":"codex","confirmed":confirmed}),
                )["result"]["structuredContent"]
                    .clone()
            }
        };
        let preview = if surface == "cli" {
            cli(
                home.path(),
                &["turn-off", "--id", id, "--agent", "codex", "--check"],
            )
        } else {
            mcp(
                home.path(),
                "turn_off_check",
                &json!({"deployment_id":id,"agent":"codex"}),
            )["result"]["structuredContent"]
                .clone()
        };
        assert_eq!(preview["status"], "ok", "{surface}: {preview}");
        assert!(preview["data"]["refusal"].is_null(), "{preview}");
        let refused = call(false);
        assert_eq!(
            refused["errors"][0]["code"], "invalid_request",
            "{surface}: {refused}"
        );
        assert!(home.path().join(".agents/skills/gamma/SKILL.md").exists());
        let result = call(true);
        assert_eq!(result["data"]["agent"], "codex", "{surface}: {result}");
        assert!(!home.path().join(".codex/skills/gamma").exists());
        for path in [
            ".config/opencode/skills/gamma/SKILL.md",
            ".cursor/skills/gamma/SKILL.md",
        ] {
            assert!(home.path().join(path).exists(), "{surface}: missing {path}");
        }
        let parked = result["data"]["parked_path"].as_str().unwrap();
        assert!(std::path::Path::new(parked).join("SKILL.md").exists());
        assert!(!home.path().join(".codex/config.toml").exists());
        assert!(!home.path().join(".claude/settings.json").exists());
    }
}

#[test]
fn remove_description_names_owned_shared_copies_and_refuses_manual_agent_copies() {
    let home = tempfile::tempdir().unwrap();
    let list = mcp(home.path(), "tools/list", &json!({}));
    let tools = list["result"]["tools"].as_array().unwrap();
    let remove = tools.iter().find(|t| t["name"] == "remove").unwrap();
    let description = remove["description"].as_str().unwrap();
    for word in [".agents/skills", "owned", "unsupported"] {
        assert!(description.contains(word), "missing {word}: {description}");
    }
}

#[test]
fn opencode_alias_installs_with_the_canonical_agent_id_or_names_the_surface() {
    let source = tempfile::tempdir().unwrap();
    let body = "---\nname: gamma\ndescription: test skill\n---\nBody\n";
    std::fs::write(source.path().join("SKILL.md"), body).unwrap();
    for spelling in ["opencode", "open-code"] {
        for surface in ["cli", "mcp"] {
            let home = tempfile::tempdir().unwrap();
            let result = if surface == "cli" {
                cli(
                    home.path(),
                    &[
                        "add",
                        source.path().to_str().unwrap(),
                        "--method",
                        "copy",
                        "--name",
                        "gamma",
                        "--agent",
                        spelling,
                    ],
                )
            } else {
                mcp(home.path(), "add", &json!({"skill":"gamma","method":"copy","scope":{"scope":"global"},"harnesses":[spelling],"files":[{"relative_path":"SKILL.md","contents":body.as_bytes()}]}))["result"]["structuredContent"].clone()
            };
            assert_eq!(result["status"], "ok", "{surface} {spelling}: {result}");
            let results = result["data"]["harness_results"].as_array().unwrap();
            assert_eq!(
                results[0]["harness"], "open-code",
                "{surface} {spelling}: {result}"
            );
        }
    }
}

#[test]
fn remove_manual_agent_copy_is_unsupported_on_cli_and_mcp_and_keeps_the_bytes() {
    let home = tempfile::tempdir().unwrap();
    let skill = home.path().join(".codex/skills/gamma");
    std::fs::create_dir_all(&skill).unwrap();
    let body = "---\nname: gamma\ndescription: test skill\n---\nBody\n";
    std::fs::write(skill.join("SKILL.md"), body).unwrap();
    let scan = cli(home.path(), &["scan"]);
    let id = scan["data"]["skills"][0]["deployments"][0]["id"]
        .as_str()
        .unwrap();
    let from_cli = cli(home.path(), &["remove", "--id", id]);
    let from_mcp = mcp(home.path(), "remove", &json!({"deployment_id":id}))["result"]
        ["structuredContent"]
        .clone();
    for result in [from_cli, from_mcp] {
        assert_eq!(result["errors"][0]["code"], "unsupported", "{result}");
        assert_eq!(
            std::fs::read_to_string(skill.join("SKILL.md")).unwrap(),
            body
        );
    }
}

#[test]
fn capabilities_accepts_both_opencode_spellings_and_returns_the_same_agent_id() {
    let home = tempfile::tempdir().unwrap();
    for spelling in ["opencode", "open-code"] {
        let from_cli = cli(home.path(), &["capabilities", "--harness", spelling]);
        let from_mcp = mcp(
            home.path(),
            "capabilities",
            &json!({"harnesses":[spelling],"observe":false,"tools":[]}),
        )["result"]["structuredContent"]
            .clone();
        for result in [from_cli, from_mcp] {
            assert_eq!(result["status"], "ok", "{spelling}: {result}");
            assert_eq!(
                result["data"]["harnesses"][0]["harness"], "open-code",
                "{spelling}: {result}"
            );
        }
    }
}

fn assert_linked_agent_roots_refused(surface: &str) {
    let home = tempfile::tempdir().unwrap();
    shared(home.path());
    std::fs::create_dir_all(home.path().join(".codex/skills")).unwrap();
    std::fs::create_dir_all(home.path().join(".cursor")).unwrap();
    std::os::unix::fs::symlink(
        home.path().join(".codex/skills"),
        home.path().join(".cursor/skills"),
    )
    .unwrap();
    let path = home.path().join(".agents/skills/gamma/SKILL.md");
    let before = std::fs::read(&path).unwrap();
    let scan = cli(home.path(), &["scan"]);
    let id = scan["data"]["skills"][0]["deployments"][0]["id"]
        .as_str()
        .unwrap();
    let (preview, result) = if surface == "cli" {
        (
            cli(
                home.path(),
                &["turn-off", "--id", id, "--agent", "codex", "--check"],
            ),
            cli(
                home.path(),
                &["turn-off", "--id", id, "--agent", "codex", "--confirm"],
            ),
        )
    } else {
        (
            mcp(
                home.path(),
                "turn_off_check",
                &json!({"deployment_id":id,"agent":"codex"}),
            )["result"]["structuredContent"]
                .clone(),
            mcp(
                home.path(),
                "turn_off_for_agent",
                &json!({"deployment_id":id,"agent":"codex","confirmed":true}),
            )["result"]["structuredContent"]
                .clone(),
        )
    };
    assert!(path.exists(), "{surface}: shared skill lost: {result}");
    assert_eq!(std::fs::read(path).unwrap(), before);
    for name in ["Codex", "Cursor"] {
        assert!(
            preview["data"]["refusal"]["reason"]
                .as_str()
                .unwrap_or_else(|| panic!("{surface}: missing linked-folder refusal: {preview}"))
                .contains(name),
            "{surface}: {preview}"
        );
    }
    assert_eq!(
        result["errors"][0]["code"], "unsupported",
        "{surface}: {result}"
    );
    assert!(!home
        .path()
        .join(".agents/skills-parked/codex/gamma")
        .exists());
}

#[test]
fn cli_linked_agent_roots_refuse_without_changing_shared_bytes() {
    assert_linked_agent_roots_refused("cli");
}

#[test]
fn mcp_linked_agent_roots_refuse_without_changing_shared_bytes() {
    assert_linked_agent_roots_refused("mcp");
}
