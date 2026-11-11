#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{json, Value};
#[path = "support/headless.rs"]
mod headless;
use headless::{cli, mcp};

fn skill(home: &std::path::Path, name: &str) {
    let path = home.join(".agents/skills").join(name);
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(
        path.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: Test skill\n---\nBody\n"),
    )
    .unwrap();
}

fn park(home: &std::path::Path, surface: &str, names: &[&str]) -> Value {
    if surface == "mcp" {
        mcp(home, "park", &json!({"skills": names}))["result"]["structuredContent"].clone()
    } else {
        let mut args = vec![surface];
        args.extend(names);
        cli(home, &args)
    }
}

#[test]
fn batch_park_and_unpark_restore_all_copies_or_name_the_surface() {
    for surface in ["park", "disable", "mcp"] {
        let home = tempfile::tempdir().unwrap();
        for name in ["alpha", "beta"] {
            skill(home.path(), name);
        }
        let result = park(home.path(), surface, &["alpha", "beta"]);
        assert_eq!(result["status"], "ok", "{surface}: {result}");
        let items = result["data"]["items"].as_array().unwrap();
        assert_eq!(items.len(), 2, "{surface}: {result}");
        for (item, name) in items.iter().zip(["alpha", "beta"]) {
            assert!(!home.path().join(".agents/skills").join(name).exists());
            assert!(std::path::Path::new(item["parked_path"].as_str().unwrap())
                .join("SKILL.md")
                .exists());
            assert!(!item["event_id"].as_str().unwrap().is_empty());
        }
        assert_ne!(items[0]["event_id"], items[1]["event_id"]);
        let restored = if surface == "mcp" {
            mcp(home.path(), "unpark", &json!({"skills": ["alpha", "beta"]}))["result"]
                ["structuredContent"]
                .clone()
        } else {
            cli(
                home.path(),
                &[
                    if surface == "disable" {
                        "enable"
                    } else {
                        "unpark"
                    },
                    "alpha",
                    "beta",
                ],
            )
        };
        assert_eq!(restored["status"], "ok", "{surface}: {restored}");
        assert_eq!(restored["data"]["items"].as_array().unwrap().len(), 2);
        for name in ["alpha", "beta"] {
            assert!(home
                .path()
                .join(".agents/skills")
                .join(name)
                .join("SKILL.md")
                .exists());
        }
    }
}

#[test]
fn batch_park_refuses_every_bad_name_before_moving_a_valid_copy_on_cli_and_mcp() {
    for surface in ["park", "mcp"] {
        let home = tempfile::tempdir().unwrap();
        skill(home.path(), "alpha");
        let result = park(home.path(), surface, &["alpha", "missing", "absent"]);
        assert_eq!(result["status"], "error", "{surface}: {result}");
        let message = result["errors"][0]["message"].as_str().unwrap();
        for name in ["missing", "absent"] {
            assert!(message.contains(name), "{surface}: {result}");
        }
        assert!(home.path().join(".agents/skills/alpha/SKILL.md").exists());
        assert!(!home
            .path()
            .join(".agents/skills-parked/universal/alpha")
            .exists());
    }
}

#[test]
fn batch_park_refuses_ambiguous_link_and_plugin_names_before_any_move() {
    for surface in ["park", "mcp"] {
        let home = tempfile::tempdir().unwrap();
        skill(home.path(), "alpha");
        skill(home.path(), "duplicate");
        let other = home.path().join(".codex/skills/duplicate");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::copy(
            home.path().join(".agents/skills/duplicate/SKILL.md"),
            other.join("SKILL.md"),
        )
        .unwrap();
        let external = tempfile::tempdir().unwrap();
        skill(external.path(), "linked");
        std::os::unix::fs::symlink(
            external.path().join(".agents/skills/linked"),
            home.path().join(".codex/skills/linked"),
        )
        .unwrap();
        let plugin_root = home.path().join(".claude/plugins/cache/test/plugin/1.0");
        std::fs::create_dir_all(plugin_root.join(".claude-plugin")).unwrap();
        std::fs::write(
            plugin_root.join(".claude-plugin/plugin.json"),
            r#"{"name":"plugin","version":"1.0"}"#,
        )
        .unwrap();
        let plugin = home
            .path()
            .join(".claude/plugins/cache/test/plugin/1.0/skills/bundled");
        std::fs::create_dir_all(&plugin).unwrap();
        std::fs::write(
            plugin.join("SKILL.md"),
            "---\nname: bundled\ndescription: Test skill\n---\nBody\n",
        )
        .unwrap();
        skill(home.path(), "occupied");
        assert_eq!(cli(home.path(), &["park", "occupied"])["status"], "ok");
        skill(home.path(), "occupied");
        let result = park(
            home.path(),
            surface,
            &["alpha", "duplicate", "linked", "bundled", "occupied"],
        );
        assert_eq!(result["status"], "error", "{surface}: {result}");
        let message = result["errors"][0]["message"].as_str().unwrap();
        for name in ["duplicate", "linked", "bundled", "occupied"] {
            assert!(message.contains(name), "{surface}: {result}");
        }
        assert!(home.path().join(".agents/skills/alpha/SKILL.md").exists());
        assert!(other.join("SKILL.md").exists());
        assert!(plugin.join("SKILL.md").exists());
    }
}

#[test]
fn batch_unpark_refuses_bad_names_before_restoring_any_copy_on_cli_and_mcp() {
    for surface in ["park", "mcp"] {
        let home = tempfile::tempdir().unwrap();
        for name in ["alpha", "beta"] {
            skill(home.path(), name);
        }
        let parked = park(home.path(), surface, &["alpha", "beta"]);
        assert_eq!(parked["status"], "ok", "{parked}");
        let refused = if surface == "mcp" {
            mcp(
                home.path(),
                "unpark",
                &json!({"skills": ["alpha", "missing", "absent"]}),
            )["result"]["structuredContent"]
                .clone()
        } else {
            cli(home.path(), &["unpark", "alpha", "missing", "absent"])
        };
        assert_eq!(refused["status"], "error", "{refused}");
        for name in ["missing", "absent"] {
            assert!(refused["errors"][0]["message"]
                .as_str()
                .unwrap()
                .contains(name));
        }
        for name in ["alpha", "beta"] {
            assert!(!home.path().join(".agents/skills").join(name).exists());
            assert!(home
                .path()
                .join(".agents/skills-parked/universal")
                .join(name)
                .join("SKILL.md")
                .exists());
        }
    }
}

#[test]
fn occupied_park_slot_refuses_all_names_instead_of_parking_alpha() {
    for surface in ["park", "mcp"] {
        let home = tempfile::tempdir().unwrap();
        for name in ["alpha", "beta"] {
            skill(home.path(), name);
        }
        std::fs::create_dir_all(home.path().join(".agents/skills-parked/universal/beta")).unwrap();
        let result = park(home.path(), surface, &["alpha", "beta"]);
        assert_eq!(result["status"], "error", "{result}");
        assert!(result["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("beta"));
        assert!(
            home.path().join(".agents/skills/alpha/SKILL.md").exists(),
            "alpha moved: {result}"
        );
    }
}

#[test]
fn occupied_restore_destination_refuses_all_names_instead_of_restoring_alpha() {
    for surface in ["park", "mcp"] {
        let home = tempfile::tempdir().unwrap();
        for name in ["alpha", "beta"] {
            skill(home.path(), name);
        }
        assert_eq!(
            park(home.path(), surface, &["alpha", "beta"])["status"],
            "ok"
        );
        std::fs::create_dir_all(home.path().join(".agents/skills/beta")).unwrap();
        let result = if surface == "mcp" {
            mcp(home.path(), "unpark", &json!({"skills": ["alpha", "beta"]}))["result"]
                ["structuredContent"]
                .clone()
        } else {
            cli(home.path(), &["unpark", "alpha", "beta"])
        };
        assert_eq!(result["status"], "error", "{result}");
        assert!(result["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("beta"));
        assert!(
            !home.path().join(".agents/skills/alpha").exists(),
            "alpha restored: {result}"
        );
    }
}

#[test]
fn mid_batch_io_failure_keeps_done_event_and_names_failed_and_untried_items() {
    use std::os::unix::fs::PermissionsExt;
    for surface in ["park", "mcp"] {
        let home = tempfile::tempdir().unwrap();
        for name in ["alpha", "beta", "gamma"] {
            skill(home.path(), name);
        }
        let parent = home.path().join(".codex/skills");
        std::fs::create_dir_all(&parent).unwrap();
        std::fs::rename(home.path().join(".agents/skills/beta"), parent.join("beta")).unwrap();
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o555)).unwrap();
        let result = park(home.path(), surface, &["alpha", "beta", "gamma"]);
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(result["status"], "partial", "{result}");
        assert!(
            result["data"]["items"][0]["event_id"]
                .as_str()
                .is_some_and(|s| !s.is_empty()),
            "{result}"
        );
        let history = cli(home.path(), &["events", "--skill", "alpha"]);
        assert_eq!(
            history["data"][0]["id"],
            result["data"]["items"][0]["event_id"]
        );
        assert_eq!(result["data"]["failure"]["skill"], "beta");
        assert_eq!(result["data"]["not_tried"], json!(["gamma"]));
        assert!(home.path().join(".agents/skills/gamma/SKILL.md").exists());
    }
}

#[test]
fn one_name_mcp_matches_cli_instead_of_returning_a_batch() {
    let home = tempfile::tempdir().unwrap();
    skill(home.path(), "alpha");
    for operation in ["park", "unpark"] {
        if operation == "unpark" {
            assert_eq!(cli(home.path(), &["park", "alpha"])["status"], "ok");
        }
        let mut cli_result = cli(home.path(), &[operation, "alpha"]);
        let reverse = if operation == "park" {
            "unpark"
        } else {
            "park"
        };
        assert_eq!(cli(home.path(), &[reverse, "alpha"])["status"], "ok");
        let mut mcp_result = mcp(home.path(), operation, &json!({"skills":["alpha"]}))["result"]
            ["structuredContent"]
            .clone();
        assert_eq!(cli_result["status"], "ok", "{cli_result}");
        assert_eq!(mcp_result["status"], "ok", "{mcp_result}");
        for result in [&mut cli_result, &mut mcp_result] {
            for key in ["correlation_id", "event_id", "timings"] {
                result.as_object_mut().unwrap().remove(key);
            }
            result["data"].as_object_mut().unwrap().remove("event_id");
        }
        assert_eq!(cli_result, mcp_result);
        assert_eq!(cli(home.path(), &[reverse, "alpha"])["status"], "ok");
    }
}

/// Flow: park one name that has two real copies. Expectation: the CLI and the
/// MCP both return `ambiguous_target` and list the id of each copy. Failure
/// means the MCP hides the ids the caller needs to pick one copy.
#[test]
fn one_ambiguous_name_returns_the_same_code_and_copy_ids_on_cli_and_mcp() {
    for surface in ["park", "mcp"] {
        let home = tempfile::tempdir().unwrap();
        skill(home.path(), "alpha");
        let other = home.path().join(".codex/skills/alpha");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::copy(
            home.path().join(".agents/skills/alpha/SKILL.md"),
            other.join("SKILL.md"),
        )
        .unwrap();
        let result = park(home.path(), surface, &["alpha"]);
        assert_eq!(
            result["errors"][0]["code"], "ambiguous_target",
            "{surface}: {result}"
        );
        let message = result["errors"][0]["message"].as_str().unwrap();
        assert_eq!(message.matches(" id ").count(), 2, "{surface}: {result}");
        assert!(other.join("SKILL.md").exists());
        assert!(home.path().join(".agents/skills/alpha/SKILL.md").exists());
    }
}
