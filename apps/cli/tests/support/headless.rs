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

fn command() -> Command {
    BINARY.with(|directory| {
        let mut command = Command::new(directory.path().join("skill-studio"));
        command
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default());
        command
    })
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
    let mut command = command();
    command.args(args);
    if scoped {
        command.arg("--home").arg(home);
    }
    let output = command
        .arg("--json")
        .env("HOME", home)
        .env("SKILL_STUDIO_TELEMETRY", "0")
        .envs(env.iter().map(|(key, value)| (key, value)))
        .current_dir(home)
        .output()
        .unwrap();
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
    let mut command = command();
    if scoped {
        command.env("SKILL_STUDIO_HOME", home);
    }
    let mut child = command
        .arg("mcp")
        .env("HOME", home)
        .env("SKILL_STUDIO_TELEMETRY", "0")
        .env("SKILL_STUDIO_MCP_DEV_TOOLS", "1")
        .envs(env.iter().map(|(key, value)| (key, value)))
        .current_dir(home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
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
