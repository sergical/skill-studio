#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/undo_fixtures.rs"]
mod undo_fixtures;

use serde_json::Value;
use std::path::Path;

fn cli(home: &Path, arguments: &[&str]) -> Value {
    let output = std::process::Command::new(undo_fixtures::cli_binary())
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

#[test]
fn cli_history_and_restore_report_backup_expired_or_offer_a_missing_backup() {
    let home = tempfile::tempdir().unwrap();
    let (expired, live) = undo_fixtures::expired_backup(home.path());
    let history = cli(home.path(), &["history"]);
    let row = history["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == expired.0)
        .unwrap();
    assert_eq!(row["restore"]["restorable"], "backup_expired");
    for arguments in [vec!["history"], vec!["restore", "--event-id", &expired.0]] {
        let output = std::process::Command::new(undo_fixtures::cli_binary())
            .args(arguments)
            .args(["--home", home.path().to_str().unwrap()])
            .env("HOME", home.path())
            .env("SKILL_STUDIO_TELEMETRY", "0")
            .output()
            .unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(text.contains("Backup expired"), "{text}");
    }
    let restored = cli(home.path(), &["restore", "--event-id", &expired.0]);
    assert_eq!(restored["status"], "error", "{restored}");
    assert_eq!(
        restored["errors"][0]["code"], "backup_expired",
        "{restored}"
    );
    assert!(restored["errors"][0]["message"]
        .as_str()
        .unwrap()
        .contains("expired"));
    assert_eq!(
        std::fs::read(home.path().join(".agents/skills/alpha/SKILL.md")).unwrap(),
        live
    );
}

#[test]
fn cli_history_and_restore_refuse_unfinished_recovery_or_write_over_an_interrupted_plan() {
    let home = tempfile::tempdir().unwrap();
    let foreign = tempfile::tempdir().unwrap();
    std::fs::write(foreign.path().join("sentinel"), b"foreign bytes stay").unwrap();
    let event = undo_fixtures::interrupted_plan_before_restore(home.path(), foreign.path());
    assert_eq!(cli(home.path(), &["history"])["data"][0]["id"], event.0);
    let result = cli(home.path(), &["restore", "--event-id", &event.0, "--force"]);
    assert_eq!(result["errors"][0]["code"], "incomplete", "{result}");
    assert!(
        result["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("Recovery"),
        "{result}"
    );
    undo_fixtures::assert_secondary_replacement(home.path());
    assert!(!home.path().join(".agents/skills/alpha").exists());
    assert_eq!(
        std::fs::read(foreign.path().join("sentinel")).unwrap(),
        b"foreign bytes stay"
    );
    assert_eq!(cli(home.path(), &["history"])["data"][0]["id"], event.0);
}

#[test]
fn cli_history_and_restore_hash_the_whole_large_asset_or_miss_a_tail_edit() {
    for (drift, failed) in [(false, false), (true, false), (false, true), (true, true)] {
        let home = tempfile::tempdir().unwrap();
        let (event, before, mut after) = undo_fixtures::large_asset_update(home.path(), failed);
        let asset = home.path().join(".agents/skills/alpha/asset.bin");
        if drift {
            *after.last_mut().unwrap() = b'c';
            std::fs::write(&asset, &after).unwrap();
        }
        let history = cli(home.path(), &["history"]);
        assert_eq!(history["data"][0]["id"], event.0);
        assert_eq!(
            history["data"][0]["status"],
            if failed { "failed" } else { "done" }
        );
        let result = cli(home.path(), &["restore", "--event-id", &event.0]);
        if drift {
            assert_eq!(result["errors"][0]["code"], "drift_conflict", "{result}");
            assert!(
                std::fs::read(&asset).unwrap() == after,
                "refused Restore changed the large asset"
            );
        } else {
            assert_eq!(result["status"], "ok", "{result}");
            assert!(
                std::fs::read(&asset).unwrap() == before,
                "Restore did not recover the complete large asset"
            );
        }
    }
}

#[test]
fn cli_update_restore_and_undo_restore_keep_the_owned_lock_row_or_erase_a_later_skill() {
    for forced in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let (event, trace) = undo_fixtures::recorded_update(home.path());
        let edited = undo_fixtures::edit_recorded_lock(home.path(), forced);
        let history = cli(home.path(), &["history"]);
        assert_eq!(history["data"][0]["id"], event.0);
        let mut result = cli(home.path(), &["restore", "--event-id", &event.0]);
        if forced {
            assert_eq!(result["errors"][0]["code"], "drift_conflict", "{result}");
            undo_fixtures::assert_recorded_lock(
                home.path(),
                &edited["skills"]["trace-skill"],
                &edited["skills"]["unrelated"],
            );
            result = cli(home.path(), &["restore", "--event-id", &event.0, "--force"]);
        }
        assert_eq!(result["status"], "ok", "{result}");
        undo_fixtures::assert_recorded_lock(
            home.path(),
            &trace["lock_before"]["skills"]["trace-skill"],
            &edited["skills"]["unrelated"],
        );
        let restored = result["data"]["restore_event_id"].as_str().unwrap();
        let undone = cli(home.path(), &["restore", "--event-id", restored]);
        assert_eq!(undone["status"], "ok", "{undone}");
        undo_fixtures::assert_recorded_lock(
            home.path(),
            &edited["skills"]["trace-skill"],
            &edited["skills"]["unrelated"],
        );
    }
}
