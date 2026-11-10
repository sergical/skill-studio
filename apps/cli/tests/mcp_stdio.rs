// Integration test binaries aren't covered by the lib crate's
// `cfg_attr(test, allow(...))`: this file compiles as its own crate, so
// the same allow needs to be declared here too.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! `skill-studio mcp` serves MCP over stdio: stdout carries JSON-RPC and
//! nothing else.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// Flow: spawn `skill-studio mcp`, send `initialize`, `initialized` and
/// `tools/list`, then close stdin.
/// Expectation: the server names itself `skill-studio`, lists
/// `skill_usage`, and every stdout line is a JSON-RPC message.
/// A failure means an agent cannot start the server from the CLI, or
/// stray output breaks the channel.
#[test]
fn mcp_subcommand_answers_initialize_and_tools_list_over_stdio() {
    let home = tempfile::tempdir().unwrap();
    let mut child = Command::new(undo_fixtures::cli_binary())
        .arg("mcp")
        .env("HOME", home.path())
        .env("SKILL_STUDIO_HOME", home.path())
        .env("SKILL_STUDIO_TELEMETRY", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn skill-studio mcp");
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();

    let (lines_tx, lines) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if lines_tx.send(line.unwrap()).is_err() {
                break;
            }
        }
    });
    let next = || -> serde_json::Value {
        let line = lines
            .recv_timeout(Duration::from_mins(1))
            .expect("the server should answer within a minute");
        serde_json::from_str(&line)
            .unwrap_or_else(|e| panic!("stdout line is not JSON ({e}): {line}"))
    };

    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2025-06-18","capabilities":{{}},"clientInfo":{{"name":"cli-test","version":"0"}}}}}}"#
    )
    .unwrap();
    let init = next();
    assert_eq!(init["id"], 1, "{init}");
    assert_eq!(
        init["result"]["serverInfo"]["name"], "skill-studio",
        "{init}"
    );

    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#
    )
    .unwrap();
    writeln!(stdin, r#"{{"jsonrpc":"2.0","id":2,"method":"tools/list"}}"#).unwrap();
    let list = next();
    assert_eq!(list["id"], 2, "{list}");
    let tools = list["result"]["tools"].as_array().unwrap();
    assert!(
        tools.iter().any(|tool| tool["name"] == "skill_usage"),
        "{list}"
    );

    drop(stdin);
    for _ in 0..600 {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let status = child.try_wait().unwrap();
    if status.is_none() {
        child.kill().ok();
    }
    assert!(
        status.is_some_and(|s| s.success()),
        "the server should exit 0 when stdin closes, got {status:?}"
    );
    while let Ok(line) = lines.try_recv() {
        serde_json::from_str::<serde_json::Value>(&line)
            .unwrap_or_else(|e| panic!("stdout line is not JSON ({e}): {line}"));
    }
}

#[path = "support/undo_fixtures.rs"]
mod undo_fixtures;

struct UndoMcp {
    child: std::process::Child,
    input: std::process::ChildStdin,
    lines: mpsc::Receiver<String>,
    next_id: u64,
}

impl UndoMcp {
    fn new(home: &std::path::Path) -> Self {
        let mut child = Command::new(undo_fixtures::cli_binary())
            .arg("mcp")
            .env("HOME", home)
            .env("SKILL_STUDIO_HOME", home)
            .env("SKILL_STUDIO_TELEMETRY", "0")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if sender.send(line.unwrap()).is_err() {
                    break;
                }
            }
        });
        let mut session = Self {
            child,
            input,
            lines,
            next_id: 1,
        };
        session.rpc("initialize",&serde_json::json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"undo-test","version":"0"}}));
        writeln!(
            session.input,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"})
        )
        .unwrap();
        session
    }

    fn rpc(&mut self, method: &str, params: &serde_json::Value) -> serde_json::Value {
        let id = self.next_id;
        self.next_id += 1;
        writeln!(
            self.input,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
        )
        .unwrap();
        loop {
            let line = self
                .lines
                .recv_timeout(Duration::from_mins(1))
                .expect("MCP must answer the test request");
            let response: serde_json::Value = serde_json::from_str(&line).unwrap();
            if response["id"] == id {
                assert!(response["error"].is_null(), "{response}");
                return response["result"].clone();
            }
        }
    }

    fn call(&mut self, name: &str, arguments: &serde_json::Value) -> serde_json::Value {
        let result = self.rpc(
            "tools/call",
            &serde_json::json!({"name":name,"arguments":arguments}),
        );
        serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
    }
}

impl Drop for UndoMcp {
    fn drop(&mut self) {
        self.child.kill().ok();
        self.child.wait().ok();
    }
}

#[test]
fn mcp_history_and_two_restores_return_secondary_replacement_bytes_or_leave_the_old_folder() {
    let home = tempfile::tempdir().unwrap();
    let removed = undo_fixtures::removed_with_secondary_replacement(home.path());
    let mut server = UndoMcp::new(home.path());
    let history = server.call("list_events", &serde_json::json!({}));
    assert_eq!(history["data"][0]["id"], removed.0);
    let first = server.call(
        "restore_event",
        &serde_json::json!({"event_id":removed.0,"force":true}),
    );
    assert_eq!(first["status"], "ok", "{first}");
    let restore_id = first["data"]["restore_event_id"].as_str().unwrap();
    let history = server.call("list_events", &serde_json::json!({}));
    assert_eq!(history["data"][0]["id"], restore_id);
    let second = server.call("restore_event", &serde_json::json!({"event_id":restore_id}));
    assert_eq!(second["status"], "ok", "{second}");
    undo_fixtures::assert_secondary_replacement(home.path());
}

#[test]
fn mcp_history_and_restore_remove_a_created_folder_that_was_unreadable_or_leave_an_unowned_skill() {
    let home = tempfile::tempdir().unwrap();
    let updated = undo_fixtures::updated_with_unreadable_created_folder(home.path());
    let mut server = UndoMcp::new(home.path());
    let history = server.call("list_events", &serde_json::json!({}));
    assert_eq!(history["data"][0]["id"], updated.0);
    let restored = server.call("restore_event", &serde_json::json!({"event_id":updated.0}));
    assert_eq!(restored["status"], "ok", "{restored}");
    assert!(!home.path().join(".agents/skills/gamma").exists());
    assert!(
        std::fs::read_to_string(home.path().join(".agents/skills/alpha/SKILL.md"))
            .unwrap()
            .contains("before update")
    );
}

#[test]
fn mcp_history_and_restore_refuse_the_latest_park_or_unpark_or_name_the_wrong_action() {
    for unpark in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let latest = undo_fixtures::parked_after_install(home.path(), unpark);
        let mut server = UndoMcp::new(home.path());
        let history = server.call("list_events", &serde_json::json!({}));
        assert_eq!(history["data"][0]["id"], latest.0);
        let restored = server.call("restore_event", &serde_json::json!({"event_id":latest.0}));
        assert_eq!(restored["status"], "error", "{restored}");
        assert_eq!(restored["errors"][0]["code"], "unsupported", "{restored}");
        assert!(
            restored["errors"][0]["message"]
                .as_str()
                .unwrap()
                .contains(if unpark { "Use park " } else { "Use unpark " }),
            "{restored}"
        );
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
fn mcp_history_and_restore_report_backup_expired_or_offer_a_missing_backup() {
    let home = tempfile::tempdir().unwrap();
    let (expired, live) = undo_fixtures::expired_backup(home.path());
    let mut server = UndoMcp::new(home.path());
    let history = server.call("list_events", &serde_json::json!({}));
    let row = history["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == expired.0)
        .unwrap();
    assert_eq!(row["restore"]["restorable"], "backup_expired");
    let restored = server.call("restore_event", &serde_json::json!({"event_id":expired.0}));
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
fn mcp_history_and_restore_refuse_unfinished_recovery_or_write_over_an_interrupted_plan() {
    let home = tempfile::tempdir().unwrap();
    let foreign = tempfile::tempdir().unwrap();
    std::fs::write(foreign.path().join("sentinel"), b"foreign bytes stay").unwrap();
    let event = undo_fixtures::interrupted_plan_before_restore(home.path(), foreign.path());
    let mut mcp = UndoMcp::new(home.path());
    assert_eq!(
        mcp.call("list_events", &serde_json::json!({}))["data"][0]["id"],
        event.0
    );
    let result = mcp.call(
        "restore_event",
        &serde_json::json!({"event_id":event.0,"force":true}),
    );
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
    assert_eq!(
        mcp.call("list_events", &serde_json::json!({}))["data"][0]["id"],
        event.0
    );
}

#[test]
fn mcp_history_and_restore_hash_the_whole_large_asset_or_miss_a_tail_edit() {
    for (drift, failed) in [(false, false), (true, false), (false, true), (true, true)] {
        let home = tempfile::tempdir().unwrap();
        let (event, before, mut after) = undo_fixtures::large_asset_update(home.path(), failed);
        let asset = home.path().join(".agents/skills/alpha/asset.bin");
        if drift {
            *after.last_mut().unwrap() = b'c';
            std::fs::write(&asset, &after).unwrap();
        }
        let mut mcp = UndoMcp::new(home.path());
        let history = mcp.call("list_events", &serde_json::json!({}));
        assert_eq!(history["data"][0]["id"], event.0);
        assert_eq!(
            history["data"][0]["status"],
            if failed { "failed" } else { "done" }
        );
        let result = mcp.call(
            "restore_event",
            &serde_json::json!({"event_id":event.0,"force":false}),
        );
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
fn mcp_update_restore_and_undo_restore_keep_the_owned_lock_row_or_erase_a_later_skill() {
    for forced in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let (event, trace) = undo_fixtures::recorded_update(home.path());
        let edited = undo_fixtures::edit_recorded_lock(home.path(), forced);
        let mut mcp = UndoMcp::new(home.path());
        let history = mcp.call("list_events", &serde_json::json!({}));
        assert_eq!(history["data"][0]["id"], event.0);
        let mut result = mcp.call(
            "restore_event",
            &serde_json::json!({"event_id":event.0,"force":false}),
        );
        if forced {
            assert_eq!(result["errors"][0]["code"], "drift_conflict", "{result}");
            undo_fixtures::assert_recorded_lock(
                home.path(),
                &edited["skills"]["trace-skill"],
                &edited["skills"]["unrelated"],
            );
            result = mcp.call(
                "restore_event",
                &serde_json::json!({"event_id":event.0,"force":true}),
            );
        }
        assert_eq!(result["status"], "ok", "{result}");
        undo_fixtures::assert_recorded_lock(
            home.path(),
            &trace["lock_before"]["skills"]["trace-skill"],
            &edited["skills"]["unrelated"],
        );
        let restored = result["data"]["restore_event_id"].as_str().unwrap();
        let undone = mcp.call(
            "restore_event",
            &serde_json::json!({"event_id":restored,"force":false}),
        );
        assert_eq!(undone["status"], "ok", "{undone}");
        undo_fixtures::assert_recorded_lock(
            home.path(),
            &edited["skills"]["trace-skill"],
            &edited["skills"]["unrelated"],
        );
    }
}

#[test]
fn mcp_history_and_dotagents_remove_undo_restore_exact_saved_entries_or_erase_a_later_skill() {
    for later in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let event = undo_fixtures::removed_dotagents(home.path(), later);
        let mut mcp = UndoMcp::new(home.path());
        let history = mcp.call("list_events", &serde_json::json!({}));
        assert_eq!(history["data"][0]["id"], event.0);
        let result = mcp.call(
            "restore_event",
            &serde_json::json!({"event_id":event.0,"force":false}),
        );
        assert_eq!(result["status"], "ok", "{result}");
        undo_fixtures::assert_dotagents_saved_entries(home.path(), later, true);
        let restored = result["data"]["restore_event_id"].as_str().unwrap();
        let history = mcp.call("list_events", &serde_json::json!({}));
        assert_eq!(history["data"][0]["id"], restored);
        let undone = mcp.call(
            "restore_event",
            &serde_json::json!({"event_id":restored,"force":false}),
        );
        assert_eq!(undone["status"], "ok", "{undone}");
        undo_fixtures::assert_dotagents_saved_entries(home.path(), later, false);
    }
}

#[test]
fn mcp_dotagents_remove_undo_refuses_a_changed_slot_when_forced_or_replaces_another_entry() {
    for (lock, link) in [(false, false), (true, false), (false, true), (true, true)] {
        let home = tempfile::tempdir().unwrap();
        let event = undo_fixtures::removed_dotagents(home.path(), true);
        let (path, newer) = undo_fixtures::conflict_dotagents_slot(home.path(), lock, link);
        let mut mcp = UndoMcp::new(home.path());
        for force in [false, true] {
            let result = mcp.call(
                "restore_event",
                &serde_json::json!({"event_id":event.0,"force":force}),
            );
            assert_eq!(result["errors"][0]["code"], "drift_conflict", "{result}");
            assert!(
                result["errors"][0]["message"]
                    .as_str()
                    .unwrap()
                    .contains("Dotagents"),
                "{result}"
            );
            assert_eq!(std::fs::read_to_string(&path).unwrap(), newer);
            assert_eq!(std::fs::symlink_metadata(&path).unwrap().file_type().is_symlink(), link);
            assert!(!home.path().join(".agents/skills/alpha").exists());
        }
    }
}
