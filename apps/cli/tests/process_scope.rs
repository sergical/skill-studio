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
        printf '%s\n' '{"version":3,"skills":{"gamma":{"source":"owner/repo","sourceType":"github","sourceUrl":"https://github.com/owner/repo","skillFolderHash":"fixture","installedAt":"2024-01-01T00:00:00Z","updatedAt":"2024-01-01T00:00:00Z"}}}' > "$HOME/.agents/.skill-lock.json"
        ;;
    update) printf '%s\n' 'Updated' >> "$HOME/.agents/skills/gamma/SKILL.md" ;;
    remove)
        for file in "$HOME/.agents/skills/gamma/"*; do unlink "$file"; done
        rmdir "$HOME/.agents/skills/gamma"
        printf '%s\n' '{"version":3,"skills":{}}' > "$HOME/.agents/.skill-lock.json"
        ;;
esac
"#;
        let runner = format!(
            "exec /bin/sh -c '{}' \"$0\" \"$@\"\n",
            script.replace('\'', "'\\''")
        );
        std::fs::write(home.path().join("skills"), runner).unwrap();
        std::os::unix::fs::symlink("skills", home.path().join("skills@1.7.0")).unwrap();
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
            if record.exists() {
                std::fs::remove_file(&record).unwrap();
            }
            let result = match (surface, operation) {
                ("cli", "add") => cli_with_env(home.path(), &["add", "owner/repo", "--name", "gamma", "--method", "skills-sh"], &env),
                ("cli", "update") => cli_with_env(home.path(), &["update", "--skill", "gamma", "--method", "skills-sh", "--source", "owner/repo"], &env),
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
            assert_eq!(
                values.lines().count(),
                HOME_KEYS.len(),
                "{surface} {operation}: missing environment variable"
            );
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
            assert_eq!(
                result["data"]["skill"], "gamma",
                "{surface} {operation}: {result}"
            );
            if operation == "add" {
                assert_eq!(
                    result["data"]["harness_results"][0]["harness"], "universal",
                    "{surface}: {result}"
                );
            }
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
    for runner in ["cursor-agent", "agent"] {
        let expected = bin.join(runner);
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
        if runner == "cursor-agent" {
            std::fs::remove_file(&expected).unwrap();
        }
    }
}

#[test]
fn read_only_cli_and_mcp_preserve_absent_and_unversioned_history_or_name_the_writer() {
    for unversioned in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let skill = home.path().join(".agents/skills/gamma");
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(
            skill.join("SKILL.md"),
            "---\nname: gamma\ndescription: Test skill\n---\nBody\n",
        )
        .unwrap();
        let history = home.path().join(".history");
        if unversioned {
            std::fs::create_dir(&history).unwrap();
            std::fs::write(history.join("legacy"), b"keep").unwrap();
        }
        fn history_snapshot(path: &Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
            if !path.exists() {
                return Vec::new();
            }
            let mut entries = vec![(path.to_path_buf(), Vec::new())];
            if path.is_dir() {
                for entry in std::fs::read_dir(path).unwrap() {
                    entries.extend(history_snapshot(&entry.unwrap().path()));
                }
            } else {
                entries[0].1 = std::fs::read(path).unwrap();
            }
            entries.sort();
            entries
        }
        let snapshot = || history_snapshot(&history);
        let before = snapshot();
        let fixture = home.path().to_str().unwrap();
        let scan = headless::cli_request(home.path(), &["scan", "--fixture", fixture], &[], false);
        assert_eq!(scan["status"], "ok", "{scan}");
        assert_eq!(before, snapshot(), "CLI scan created history");
        let id = scan["data"]["skills"][0]["deployments"][0]["id"]
            .as_str()
            .unwrap();
        let cases: &[(&[&str], &str, Value)] = &[
            (&["scan"], "scan", json!({})),
            (&["diagnose"], "diagnose", json!({})),
            (&["conflicts"], "diagnose_conflict", json!({})),
            (&["usage"], "skill_usage", json!({})),
            (
                &["capabilities"],
                "capabilities",
                json!({"harnesses":[],"observe":false,"tools":[]}),
            ),
            (&["harnesses"], "harnesses", json!({})),
            (
                &["preview-repair", "--deployment-id", id],
                "preview_frontmatter_repair",
                json!({"deployment_id":id}),
            ),
            (&["events"], "list_events", json!({})),
            (&["doctor"], "doctor", json!({})),
            (&["outdated"], "outdated", json!({})),
            (&["install-preferences"], "install_preferences", json!({})),
            (
                &["turn-off", "--id", id, "--agent", "codex", "--check"],
                "turn_off_check",
                json!({"deployment_id":id,"agent":"codex"}),
            ),
        ];
        fn assert_read_result(tool: &str, result: &Value, id: &str) {
            let data = &result["data"];
            match tool {
                "scan" => assert_eq!(data["skills"][0]["name"], "gamma", "{result}"),
                "diagnose" => {
                    assert_eq!(data["inventory"]["skills"][0]["name"], "gamma", "{result}");
                }
                "diagnose_conflict" => assert_eq!(data["conflicts"], json!([]), "{result}"),
                "skill_usage" => assert_eq!(data["unused"], json!(["gamma"]), "{result}"),
                "capabilities" | "harnesses" => assert!(
                    !data["harnesses"].as_array().unwrap().is_empty(),
                    "{result}"
                ),
                "preview_frontmatter_repair" => {
                    assert_eq!(result["errors"][0]["code"], "unsupported", "{result}");
                    assert_eq!(
                        result["errors"][0]["message"],
                        "SKILL.md does not have a malformed YAML frontmatter block",
                        "{id}: {result}"
                    );
                }
                "list_events" => assert_eq!(*data, json!([]), "{result}"),
                "outdated" => assert_eq!(
                    *data,
                    json!({"gamma":{"currency":"not_tracked","installed_commit":null,"latest_commit":null,"latest_commit_at":null,"error":null}}),
                    "{result}"
                ),
                "doctor" => {
                    assert_eq!(data["checked"], 1, "{result}");
                    assert_eq!(data["violations"], json!([]), "{result}");
                }
                "install_preferences" => {
                    assert_eq!(data["saved"], false, "{result}");
                    assert!(data["method"].is_string(), "{result}");
                }
                "turn_off_check" => assert!(
                    data.as_object().unwrap().contains_key("refusal"),
                    "{result}"
                ),
                _ => panic!("missing result check for {tool}"),
            }
        }
        for (args, tool, request) in cases {
            let mut args = args.to_vec();
            args.extend(["--fixture", fixture]);
            let result = headless::cli_request(home.path(), &args, &[], false);
            assert_eq!(
                result["status"],
                if *tool == "preview_frontmatter_repair" {
                    "error"
                } else {
                    "ok"
                },
                "{args:?}: {result}"
            );
            assert_read_result(tool, &result, id);
            assert_eq!(
                before,
                snapshot(),
                "CLI {args:?}, unversioned={unversioned}"
            );
            let response = headless::mcp_request(
                home.path(),
                tool,
                request,
                &[("SKILL_STUDIO_FIXTURE".into(), fixture.into())],
                false,
            );
            assert_eq!(
                envelope(&response)["status"],
                if *tool == "preview_frontmatter_repair" {
                    "error"
                } else {
                    "ok"
                },
                "MCP {tool}: {response}"
            );
            assert_read_result(tool, envelope(&response), id);
            assert_eq!(before, snapshot(), "MCP {tool}, unversioned={unversioned}");
        }
        let health = headless::cli_request(home.path(), &["health"], &[], false);
        assert!(health.is_array(), "{health}");
        assert_eq!(before, snapshot(), "CLI health");
        let watch =
            headless::cli_request(home.path(), &["watch", "--fixture", fixture], &[], false);
        assert!(watch["inventory"].is_object(), "{watch}");
        assert_eq!(before, snapshot(), "CLI watch");
    }
}

#[test]
fn empty_outdated_preserves_absent_and_unversioned_history_without_gh_or_migrates_it() {
    for surface in ["cli", "mcp"] {
        for unversioned in [false, true] {
            let home = tempfile::tempdir().unwrap();
            let history = home.path().join(".skill-studio/history");
            if unversioned {
                std::fs::create_dir_all(&history).unwrap();
                std::fs::write(history.join("legacy"), b"keep").unwrap();
            }
            let env = vec![("PATH".into(), "/usr/bin:/bin".into())];
            let result = if surface == "cli" {
                cli_with_env(home.path(), &["outdated"], &env)
            } else {
                envelope(&mcp_with_env(home.path(), "outdated", &json!({}), &env)).clone()
            };
            assert_eq!(result["status"], "ok", "{surface}: {result}");
            assert_eq!(result["data"], json!({}), "{surface}: {result}");
            assert_eq!(history.exists(), unversioned, "{surface} created history");
            if unversioned {
                assert_eq!(std::fs::read(history.join("legacy")).unwrap(), b"keep");
                assert_eq!(
                    std::fs::read_dir(&history).unwrap().count(),
                    1,
                    "{surface} migrated history"
                );
            }
        }
    }
}
