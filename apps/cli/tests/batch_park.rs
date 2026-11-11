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
