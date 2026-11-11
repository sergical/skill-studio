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
    let mut child = Command::new(env!("CARGO_BIN_EXE_skill-studio"))
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
        let mut child = Command::new(env!("CARGO_BIN_EXE_skill-studio"))
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
