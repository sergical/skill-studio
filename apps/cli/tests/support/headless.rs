#[allow(dead_code)]
#[path = "test_home.rs"]
pub mod test_home;

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

thread_local! {
    // Other worktrees can replace Cargo's executable between calls in one flow.
    static BINARY: tempfile::TempDir = {
        let directory = tempfile::tempdir().unwrap();
        std::fs::copy(
            std::env::var_os("SKILL_STUDIO_TEST_BINARY").unwrap_or_else(|| env!("CARGO_BIN_EXE_skill-studio").into()),
            directory.path().join("skill-studio"),
        ).unwrap();
        directory
    };
}

fn command(home: &std::path::Path) -> Command {
    BINARY.with(|directory| test_home::command(directory.path().join("skill-studio"), home))
}

pub fn cli(home: &std::path::Path, args: &[&str]) -> Value {
    cli_with_env(home, args, &[])
}

pub fn cli_with_env(home: &std::path::Path, args: &[&str], env: &[(String, String)]) -> Value {
    cli_request(home, args, env, true)
}

pub fn cli_request(
    home: &std::path::Path,
    args: &[&str],
    env: &[(String, String)],
    scoped: bool,
) -> Value {
    let mut command = command(home);
    command.args(args);
    if scoped {
        command.arg("--home").arg(home);
    }
    command
        .arg("--json")
        .env("HOME", home)
        .env("SKILL_STUDIO_TELEMETRY", "0")
        .envs(env.iter().map(|(key, value)| (key, value)))
        .current_dir(home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = skill_studio_host::spawn_retrying_busy(&mut command).unwrap();
    if args.first() == Some(&"watch") {
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        child.kill().unwrap();
        child.wait().unwrap();
        return serde_json::from_str(&line).unwrap();
    }
    let output = child.wait_with_output().unwrap();
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&output.stderr)))
}

pub fn mcp(home: &std::path::Path, tool: &str, arguments: &Value) -> Value {
    mcp_with_env(home, tool, arguments, &[])
}

pub fn mcp_with_env(
    home: &std::path::Path,
    tool: &str,
    arguments: &Value,
    env: &[(String, String)],
) -> Value {
    mcp_request(home, tool, arguments, env, true)
}

pub fn mcp_request(
    home: &std::path::Path,
    tool: &str,
    arguments: &Value,
    env: &[(String, String)],
    scoped: bool,
) -> Value {
    let mut command = command(home);
    if scoped {
        command.env("SKILL_STUDIO_HOME", home);
    }
    command
        .arg("mcp")
        .env("HOME", home)
        .env("SKILL_STUDIO_TELEMETRY", "0")
        .env("SKILL_STUDIO_MCP_DEV_TOOLS", "1")
        .envs(env.iter().map(|(key, value)| (key, value)))
        .current_dir(home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = skill_studio_host::spawn_retrying_busy(&mut command).unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    writeln!(input, "{}", &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"parity","version":"0"}}})).unwrap();
    output.read_line(&mut line).unwrap();
    writeln!(
        input,
        "{}",
        &json!({"jsonrpc":"2.0","method":"notifications/initialized"})
    )
    .unwrap();
    let method = if tool == "tools/list" {
        "tools/list"
    } else {
        "tools/call"
    };
    writeln!(
        input,
        "{}",
        &json!({"jsonrpc":"2.0","id":2,"method":method,"params":{"name":tool,"arguments":arguments}})
    )
    .unwrap();
    let result = loop {
        line.clear();
        assert_ne!(output.read_line(&mut line).unwrap(), 0, "MCP closed stdout");
        let value: Value = serde_json::from_str(&line).unwrap();
        if value["id"] == 2 {
            break value;
        }
    };
    drop(input);
    child.wait().unwrap();
    result
}

/// The envelope of an MCP tool response without its leading `untrusted_paths`,
/// so that it compares equal to the CLI's envelope.
#[allow(dead_code)]
pub fn mcp_envelope(response: &Value) -> Value {
    let mut envelope = response["result"]["structuredContent"].clone();
    let fields = envelope.as_object_mut().unwrap();
    assert_eq!(
        fields.keys().next().map(String::as_str),
        Some("untrusted_paths")
    );
    assert!(fields.remove("untrusted_paths").unwrap().is_array());
    envelope
}
