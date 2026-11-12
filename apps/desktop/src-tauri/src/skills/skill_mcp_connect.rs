// ============================================================================
// Skills Module - skill_mcp_connect
// Adds the Skill Studio MCP server to an agent by running the agent's own
// `mcp remove` and `mcp add` commands, so the agent writes its own config
// file. Reads which agents already have it through the core `extras` op.
// ============================================================================

use skill_studio_core::extras::{self, ExtrasRequest};
use skill_studio_core::identity::CorrelationId;
use skill_studio_core::ports::{NeverCancel, OpContext, ProcessSpawner, ProcessSpec, Runtime};

const SERVER_NAME: &str = "skill-studio";
const SERVER_COMMAND: [&str; 4] = ["npx", "-y", "skill-studio", "mcp"];
const ADD_TIMEOUT_MS: u64 = 30_000;
// The first `npx` download of the CLI package can take longer than an agent's
// MCP startup timeout (Codex: 10 s), so Connect downloads it first.
const DOWNLOAD_TIMEOUT_MS: u64 = 120_000;

fn spec(program: &str, args: &[&str], timeout_ms: u64) -> ProcessSpec {
    ProcessSpec {
        program: program.to_string(),
        args: args.iter().map(|arg| (*arg).to_string()).collect(),
        cwd: None,
        env: Vec::new(),
        timeout_ms,
    }
}

/// The download, remove and add commands, in run order.
fn connect_commands(harness: &str) -> Option<[ProcessSpec; 3]> {
    let (program, scope, transport): (&str, &[&str], &[&str]) = match harness {
        "claude-code" => ("claude", &["--scope", "user"], &["--transport", "stdio"]),
        "codex" => ("codex", &[], &[]),
        _ => return None,
    };
    let remove: Vec<&str> = ["mcp", "remove"]
        .into_iter()
        .chain(scope.iter().copied())
        .chain([SERVER_NAME])
        .collect();
    let add: Vec<&str> = ["mcp", "add"]
        .into_iter()
        .chain(scope.iter().copied())
        .chain(transport.iter().copied())
        .chain([SERVER_NAME, "--"])
        .chain(SERVER_COMMAND)
        .collect();
    Some([
        spec(
            "npx",
            &["-y", SERVER_NAME, "--version"],
            DOWNLOAD_TIMEOUT_MS,
        ),
        spec(program, &remove, ADD_TIMEOUT_MS),
        spec(program, &add, ADD_TIMEOUT_MS),
    ])
}

fn run(spawner: &dyn ProcessSpawner, spec: &ProcessSpec) -> Result<(), String> {
    let command = format!("{} {}", spec.program, spec.args.join(" "));
    let output = spawner
        .run(spec, &NeverCancel)
        .map_err(|err| format!("Could not run `{}`: {}", spec.program, err.message))?;
    if output.timed_out {
        return Err(format!(
            "`{command}` did not finish in {} seconds.",
            spec.timeout_ms / 1000
        ));
    }
    if output.status != Some(0) {
        let detail = [output.stderr.trim(), output.stdout.trim()]
            .into_iter()
            .find(|text| !text.is_empty())
            .unwrap_or("no output");
        return Err(format!("`{command}` failed: {detail}"));
    }
    Ok(())
}

/// Agent ids whose user-level config declares the Skill Studio MCP server.
pub fn connected_agents(rt: &Runtime) -> Result<Vec<String>, String> {
    let ctx = OpContext::uncancellable(CorrelationId(ulid::Ulid::generate().to_string()));
    let report = extras::extras(rt, &ctx, &ExtrasRequest {}).map_err(|err| err.message)?;
    Ok(report
        .entries
        .into_iter()
        .filter(|entry| {
            entry.scope == "global"
                && entry.kind == "mcp_server"
                && entry.name.as_deref() == Some(SERVER_NAME)
        })
        .map(|entry| entry.harness)
        .collect())
}

/// Adds the server, or replaces an existing `skill-studio` entry (for example
/// one that points to a removed binary) with the published one.
pub fn connect_agent(rt: &Runtime, harness: &str) -> Result<(), String> {
    let [download, remove, add] = connect_commands(harness)
        .ok_or_else(|| format!("Skill Studio cannot add its MCP server to {harness}."))?;
    let spawner = rt
        .ports
        .spawner
        .as_deref()
        .ok_or("This build cannot run agent commands.")?;
    run(spawner, &download)?;
    // A missing entry makes `mcp remove` fail; `mcp add` reports any real problem.
    let _ = run(spawner, &remove);
    run(spawner, &add)
}

#[tauri::command]
pub async fn get_mcp_connections(
    trace: crate::timing_log::IpcTrace,
    app: tauri::AppHandle,
) -> Result<Vec<String>, String> {
    crate::timing_log::time_command_blocking(&app, trace, "get_mcp_connections", move || {
        connected_agents(&super::core_runtime::build_runtime_write()?)
    })
    .await
}

#[tauri::command]
pub async fn connect_mcp(
    trace: crate::timing_log::IpcTrace,
    harness: String,
    app: tauri::AppHandle,
) -> Result<(), String> {
    crate::timing_log::time_command_blocking(&app, trace, "connect_mcp", move || {
        connect_agent(&super::core_runtime::build_runtime_write()?, &harness)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use skill_studio_core::ports::{CancelToken, ProcessOutput, ProcessSpawner};
    use skill_studio_core::CoreError;
    use std::sync::{Arc, Mutex};

    struct RecordingSpawner {
        calls: Mutex<Vec<ProcessSpec>>,
        failing: Option<(&'static str, &'static str)>,
    }

    impl ProcessSpawner for RecordingSpawner {
        fn run(&self, spec: &ProcessSpec, _: &dyn CancelToken) -> Result<ProcessOutput, CoreError> {
            self.calls.lock().unwrap().push(spec.clone());
            let failure = self
                .failing
                .filter(|(program, _)| *program == spec.program)
                .map(|(_, stderr)| stderr);
            Ok(ProcessOutput {
                status: Some(i32::from(failure.is_some())),
                stdout: String::new(),
                stderr: failure.unwrap_or_default().to_string(),
                timed_out: false,
            })
        }
    }

    impl RecordingSpawner {
        fn commands(&self) -> Vec<String> {
            self.calls
                .lock()
                .unwrap()
                .iter()
                .map(|call| format!("{} {}", call.program, call.args.join(" ")))
                .collect()
        }
    }

    fn runtime(home: &std::path::Path, spawner: Arc<RecordingSpawner>) -> Runtime {
        let mut rt = super::super::core_runtime::build_runtime_write_at_with_search_dirs(
            home,
            &home.join(".data"),
            Vec::new(),
        )
        .unwrap();
        rt.ports.spawner = Some(spawner);
        rt
    }

    fn spawner(failing: Option<(&'static str, &'static str)>) -> Arc<RecordingSpawner> {
        Arc::new(RecordingSpawner {
            calls: Mutex::new(Vec::new()),
            failing,
        })
    }

    #[test]
    fn claude_and_codex_user_configs_report_connected_and_cursor_without_the_server_does_not() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join(".claude.json"),
            r#"{"mcpServers":{"skill-studio":{"command":"npx"},"other":{}}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(home.path().join(".codex")).unwrap();
        std::fs::write(
            home.path().join(".codex/config.toml"),
            "[mcp_servers.skill-studio]\ncommand = \"npx\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(home.path().join(".cursor")).unwrap();
        std::fs::write(
            home.path().join(".cursor/mcp.json"),
            r#"{"mcpServers":{"other":{}}}"#,
        )
        .unwrap();

        let agents = connected_agents(&runtime(home.path(), spawner(None))).unwrap();

        assert_eq!(agents, vec!["claude-code", "codex"]);
    }

    #[test]
    fn connecting_claude_code_downloads_the_server_then_replaces_its_user_entry_or_names_the_wrong_argv(
    ) {
        let home = tempfile::tempdir().unwrap();
        let recorder = spawner(None);

        connect_agent(&runtime(home.path(), recorder.clone()), "claude-code").unwrap();

        assert_eq!(
            recorder.commands(),
            vec![
                "npx -y skill-studio --version",
                "claude mcp remove --scope user skill-studio",
                "claude mcp add --scope user --transport stdio skill-studio -- npx -y skill-studio mcp",
            ]
        );
    }

    #[test]
    fn a_failed_codex_mcp_add_returns_its_stderr_instead_of_success() {
        let home = tempfile::tempdir().unwrap();
        let recorder = spawner(Some(("codex", "config.toml is not writable")));

        let err = connect_agent(&runtime(home.path(), recorder.clone()), "codex").unwrap_err();

        assert!(err.contains("config.toml is not writable"), "{err}");
        assert_eq!(
            recorder.commands().last().unwrap(),
            "codex mcp add skill-studio -- npx -y skill-studio mcp"
        );
    }

    #[test]
    fn a_failed_server_download_stops_before_the_agent_config_changes() {
        let home = tempfile::tempdir().unwrap();
        let recorder = spawner(Some(("npx", "network unreachable")));

        let err = connect_agent(&runtime(home.path(), recorder.clone()), "codex").unwrap_err();

        assert!(err.contains("network unreachable"), "{err}");
        assert_eq!(recorder.commands(), vec!["npx -y skill-studio --version"]);
    }

    #[test]
    fn an_agent_without_an_mcp_add_command_is_refused_before_anything_runs() {
        let home = tempfile::tempdir().unwrap();
        let recorder = spawner(None);

        assert!(connect_agent(&runtime(home.path(), recorder.clone()), "cursor").is_err());
        assert!(recorder.commands().is_empty());
    }
}
