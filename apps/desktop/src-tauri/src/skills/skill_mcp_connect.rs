// ============================================================================
// Skills Module - skill_mcp_connect
// Adds the Skill Studio MCP server to an agent by running the agent's own
// `mcp add` command, so the agent writes its own config file. Reads which
// agents already have it through the core `extras` op.
// ============================================================================

use skill_studio_core::extras::{self, ExtrasRequest};
use skill_studio_core::identity::CorrelationId;
use skill_studio_core::ports::{NeverCancel, OpContext, ProcessSpec, Runtime};

const SERVER_NAME: &str = "skill-studio";
const SERVER_COMMAND: [&str; 4] = ["npx", "-y", "skill-studio", "mcp"];
const ADD_TIMEOUT_MS: u64 = 30_000;

fn add_command(harness: &str) -> Option<ProcessSpec> {
    let (program, flags): (&str, &[&str]) = match harness {
        "claude-code" => ("claude", &["--scope", "user", "--transport", "stdio"]),
        "codex" => ("codex", &[]),
        _ => return None,
    };
    let args = ["mcp", "add"]
        .iter()
        .chain(flags)
        .chain(&[SERVER_NAME, "--"])
        .chain(&SERVER_COMMAND)
        .map(|arg| (*arg).to_string())
        .collect();
    Some(ProcessSpec {
        program: program.to_string(),
        args,
        cwd: None,
        env: Vec::new(),
        timeout_ms: ADD_TIMEOUT_MS,
    })
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

pub fn connect_agent(rt: &Runtime, harness: &str) -> Result<(), String> {
    let spec = add_command(harness)
        .ok_or_else(|| format!("Skill Studio cannot add its MCP server to {harness}."))?;
    let spawner = rt
        .ports
        .spawner
        .as_ref()
        .ok_or("This build cannot run agent commands.")?;
    let output = spawner
        .run(&spec, &NeverCancel)
        .map_err(|err| format!("Could not run `{}`: {}", spec.program, err.message))?;
    if output.timed_out {
        return Err(format!(
            "`{} mcp add` did not finish in 30 seconds.",
            spec.program
        ));
    }
    if output.status != Some(0) {
        let detail = [output.stderr.trim(), output.stdout.trim()]
            .into_iter()
            .find(|text| !text.is_empty())
            .unwrap_or("no output");
        return Err(format!("`{} mcp add` failed: {detail}", spec.program));
    }
    Ok(())
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
        output: ProcessOutput,
    }

    impl ProcessSpawner for RecordingSpawner {
        fn run(&self, spec: &ProcessSpec, _: &dyn CancelToken) -> Result<ProcessOutput, CoreError> {
            self.calls.lock().unwrap().push(spec.clone());
            Ok(self.output.clone())
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

    fn spawner(status: i32, stderr: &str) -> Arc<RecordingSpawner> {
        Arc::new(RecordingSpawner {
            calls: Mutex::new(Vec::new()),
            output: ProcessOutput {
                status: Some(status),
                stdout: String::new(),
                stderr: stderr.to_string(),
                timed_out: false,
            },
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

        let agents = connected_agents(&runtime(home.path(), spawner(0, ""))).unwrap();

        assert_eq!(agents, vec!["claude-code", "codex"]);
    }

    #[test]
    fn connecting_claude_code_runs_its_own_user_scope_mcp_add_or_names_the_wrong_argv() {
        let home = tempfile::tempdir().unwrap();
        let recorder = spawner(0, "");

        connect_agent(&runtime(home.path(), recorder.clone()), "claude-code").unwrap();

        let calls = recorder.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].program, "claude");
        assert_eq!(
            calls[0].args.join(" "),
            "mcp add --scope user --transport stdio skill-studio -- npx -y skill-studio mcp"
        );
    }

    #[test]
    fn a_failed_codex_mcp_add_returns_its_stderr_instead_of_success() {
        let home = tempfile::tempdir().unwrap();
        let recorder = spawner(1, "config.toml is not writable");

        let err = connect_agent(&runtime(home.path(), recorder.clone()), "codex").unwrap_err();

        assert!(err.contains("config.toml is not writable"), "{err}");
        assert_eq!(
            recorder.calls.lock().unwrap()[0].args.join(" "),
            "mcp add skill-studio -- npx -y skill-studio mcp"
        );
    }

    #[test]
    fn an_agent_without_an_mcp_add_command_is_refused_before_anything_runs() {
        let home = tempfile::tempdir().unwrap();
        let recorder = spawner(0, "");

        assert!(connect_agent(&runtime(home.path(), recorder.clone()), "cursor").is_err());
        assert!(recorder.calls.lock().unwrap().is_empty());
    }
}
