#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/undo_fixtures.rs"]
mod undo_fixtures;

use serde_json::Value;
use std::path::Path;

fn cli(home: &Path, arguments: &[&str]) -> Value {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_skill-studio"))
        .args(arguments)
        .args(["--home", home.to_str().unwrap(), "--json"])
        .env("HOME", home)
        .env("SKILL_STUDIO_TELEMETRY", "0")
        .output()
        .unwrap();
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        output.status.success(),
        result["status"] == "ok",
        "{result}"
    );
    result
}

#[test]
fn cli_history_and_two_restores_return_secondary_replacement_bytes_or_leave_the_old_folder() {
    let home = tempfile::tempdir().unwrap();
    let removed = undo_fixtures::removed_with_secondary_replacement(home.path());
    let history = cli(home.path(), &["history"]);
    assert_eq!(history["data"][0]["id"], removed.0);
    let first = cli(
        home.path(),
        &["restore", "--event-id", &removed.0, "--force"],
    );
    assert_eq!(first["status"], "ok", "{first}");
    let restore_id = first["data"]["restore_event_id"].as_str().unwrap();
    let history = cli(home.path(), &["history"]);
    assert_eq!(history["data"][0]["id"], restore_id);
    assert_eq!(history["data"][0]["restore"]["restorable"], "yes");
    let second = cli(home.path(), &["restore", "--event-id", restore_id]);
    assert_eq!(second["status"], "ok", "{second}");
    undo_fixtures::assert_secondary_replacement(home.path());
}

#[test]
fn cli_history_and_restore_remove_a_created_folder_that_was_unreadable_or_leave_an_unowned_skill() {
    let home = tempfile::tempdir().unwrap();
    let updated = undo_fixtures::updated_with_unreadable_created_folder(home.path());
    let history = cli(home.path(), &["history"]);
    assert_eq!(history["data"][0]["id"], updated.0);
    let restored = cli(home.path(), &["restore", "--event-id", &updated.0]);
    assert_eq!(restored["status"], "ok", "{restored}");
    assert!(!home.path().join(".agents/skills/gamma").exists());
    assert!(
        std::fs::read_to_string(home.path().join(".agents/skills/alpha/SKILL.md"))
            .unwrap()
            .contains("before update")
    );
}

#[test]
fn cli_undo_and_restore_refuse_the_latest_park_or_unpark_or_revert_an_older_change() {
    for unpark in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let latest = undo_fixtures::parked_after_install(home.path(), unpark);
        let history = cli(home.path(), &["history"]);
        assert_eq!(history["data"][0]["id"], latest.0);
        for result in [
            cli(home.path(), &["undo"]),
            cli(home.path(), &["restore", "--event-id", &latest.0]),
        ] {
            assert_eq!(result["status"], "error", "{result}");
            assert_eq!(result["errors"][0]["code"], "unsupported", "{result}");
            assert!(
                result["errors"][0]["message"]
                    .as_str()
                    .unwrap()
                    .contains(if unpark { "Use park " } else { "Use unpark " }),
                "{result}"
            );
        }
        let location = if unpark {
            ".agents/skills/alpha/SKILL.md"
        } else {
            ".agents/skills-parked/universal/alpha/SKILL.md"
        };
        assert!(std::fs::read_to_string(home.path().join(location))
            .unwrap()
            .contains("Keep this skill."));
    }
}
