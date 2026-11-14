// ============================================================================
// Skills Module - skill_mcp_connect
// Adds the Skill Studio MCP server to an agent through the agent's own
// `mcp add` command or install link, so the agent writes its own config
// file. Reads which agents already have it through the core `extras` op.
// Keep the commands in step with packages/lib/src/agent-setup.ts.
// ============================================================================

use std::path::Path;

use skill_studio_core::extras::{self, ExtrasRequest};
use skill_studio_core::identity::CorrelationId;
use skill_studio_core::ports::{NeverCancel, OpContext, ProcessSpawner, ProcessSpec, Runtime};

const SERVER_NAME: &str = "skill-studio";
// `@latest` makes npx fetch the published package: a bare `skill-studio`
// resolves to any local package of that name first and fails with
// "could not determine executable to run".
const SERVER_COMMAND: [&str; 4] = ["npx", "-y", "skill-studio@latest", "mcp"];
// base64 of {"command":"npx","args":["-y","skill-studio@latest","mcp"]}, URL-encoded.
const CURSOR_INSTALL_LINK: &str = "cursor://anysphere.cursor-deeplink/mcp/install?name=skill-studio&config=eyJjb21tYW5kIjoibnB4IiwiYXJncyI6WyIteSIsInNraWxsLXN0dWRpb0BsYXRlc3QiLCJtY3AiXX0%3D";
const ADD_TIMEOUT_MS: u64 = 30_000;
// The first `npx` download of the CLI package can take longer than an agent's
// MCP startup timeout (Codex: 10 s), so Connect downloads it first.
const DOWNLOAD_TIMEOUT_MS: u64 = 120_000;

fn spec(program: &str, args: &[&str], cwd: &Path, timeout_ms: u64) -> ProcessSpec {
    ProcessSpec {
        program: program.to_string(),
        args: args.iter().map(|arg| (*arg).to_string()).collect(),
        cwd: Some(cwd.to_path_buf()),
        env: Vec::new(),
        timeout_ms,
    }
}

struct ConnectCommands {
    display_name: &'static str,
    /// For agents whose `mcp add` refuses a name that exists.
    remove_existing: Option<ProcessSpec>,
    add: ProcessSpec,
}

fn connect_commands(harness: &str, home: &Path) -> Option<ConnectCommands> {
    // (program, name, `mcp add` flags before the server name, whether add refuses an existing name)
    let (program, display_name, add_flags, remove_first): (&str, &'static str, &[&str], bool) =
        match harness {
            "claude-code" => (
                "claude",
                "Claude Code",
                &["--scope", "user", "--transport", "stdio"],
                true,
            ),
            "codex" => ("codex", "Codex", &[], false),
            "open-code" => ("opencode", "OpenCode", &["--global"], false),
            "pi" => ("pi", "pi", &[], false),
            "grok-build" => ("grok", "Grok Build", &[], true),
            "cursor" => {
                return Some(ConnectCommands {
                    display_name: "Cursor",
                    remove_existing: None,
                    add: spec("open", &[CURSOR_INSTALL_LINK], home, ADD_TIMEOUT_MS),
                })
            }
            _ => return None,
        };
    let remove_flags: &[&str] = if harness == "claude-code" {
        &["--scope", "user"]
    } else {
        &[]
    };
    let remove: Vec<&str> = ["mcp", "remove"]
        .into_iter()
        .chain(remove_flags.iter().copied())
        .chain([SERVER_NAME])
        .collect();
    let add: Vec<&str> = ["mcp", "add"]
        .into_iter()
        .chain(add_flags.iter().copied())
        .chain([SERVER_NAME, "--"])
        .chain(SERVER_COMMAND)
        .collect();
    Some(ConnectCommands {
        display_name,
        remove_existing: remove_first.then(|| spec(program, &remove, home, ADD_TIMEOUT_MS)),
        add: spec(program, &add, home, ADD_TIMEOUT_MS),
    })
}

/// Runs one command. The error's first line is a sentence for the user; the
/// rest is the command and its output, for the details view.
fn run(spawner: &dyn ProcessSpawner, spec: &ProcessSpec, summary: &str) -> Result<(), String> {
    let command = format!("{} {}", spec.program, spec.args.join(" "));
    let output = spawner
        .run(spec, &NeverCancel)
        .map_err(|err| format!("{summary}\n$ {command}\n{}", err.message))?;
    if output.timed_out {
        return Err(format!(
            "{summary}\n$ {command}\nStopped after {} seconds.",
            spec.timeout_ms / 1000
        ));
    }
    if output.status != Some(0) {
        let detail = [output.stderr.trim(), output.stdout.trim()]
            .into_iter()
            .find(|text| !text.is_empty())
            .unwrap_or("No output.");
        return Err(format!("{summary}\n$ {command}\n{detail}"));
    }
    Ok(())
}

/// Agent ids whose user-level config declares the Skill Studio MCP server.
pub fn connected_agents(rt: &Runtime) -> Result<Vec<String>, String> {
    let ctx = OpContext::uncancellable(CorrelationId(ulid::Ulid::generate().to_string()));
    let report = extras::extras(rt, &ctx, &ExtrasRequest {}).map_err(|err| err.message)?;
    let mut agents: Vec<String> = report
        .entries
        .into_iter()
        .filter(|entry| {
            entry.scope == "global"
                && entry.kind == "mcp_server"
                && entry.name.as_deref() == Some(SERVER_NAME)
        })
        // `extras` names OpenCode by its config folder; the harness report and `connect_agent` use the agent id.
        .map(|entry| match entry.harness.as_str() {
            "opencode" => "open-code".to_string(),
            _ => entry.harness,
        })
        .collect();
    agents.dedup();
    Ok(agents)
}

/// Adds the server, or replaces an existing `skill-studio` entry (for example
/// one that points to a removed binary) with the published one. For Cursor it
/// opens Cursor's install dialog, and Cursor writes the entry when the user
/// confirms. Commands run in the home folder, so a project's own config or
/// `package.json` cannot change what they do.
pub fn connect_agent(rt: &Runtime, harness: &str) -> Result<(), String> {
    let home = rt.scope.home.lexical.clone();
    let commands = connect_commands(harness, &home)
        .ok_or_else(|| format!("Skill Studio cannot add its MCP server to {harness}."))?;
    let spawner = rt
        .ports
        .spawner
        .as_deref()
        .ok_or("This build cannot run agent commands.")?;
    let download = spec(
        "npx",
        &["-y", "skill-studio@latest", "--version"],
        &home,
        DOWNLOAD_TIMEOUT_MS,
    );
    run(
        spawner,
        &download,
        "Couldn't download the Skill Studio MCP server from npm.",
    )?;
    if let Some(remove) = &commands.remove_existing {
        // A missing entry makes `mcp remove` fail; `mcp add` reports any real problem.
        let _ = run(spawner, remove, "");
    }
    run(
        spawner,
        &commands.add,
        &format!("{} couldn't add the server.", commands.display_name),
    )
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
            let command = format!("{} {}", spec.program, spec.args.join(" "));
            let failure = self
                .failing
                .filter(|(prefix, _)| command.starts_with(prefix))
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
    fn claude_codex_and_opencode_user_configs_report_connected_and_cursor_without_the_server_does_not(
    ) {
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
        std::fs::create_dir_all(home.path().join(".config/opencode")).unwrap();
        std::fs::write(
            home.path().join(".config/opencode/opencode.json"),
            r#"{"mcp":{"skill-studio":{"type":"local"}}}"#,
        )
        .unwrap();

        let agents = connected_agents(&runtime(home.path(), spawner(None))).unwrap();

        // OpenCode comes back under the harness report's id, so its Settings row shows Connected.
        assert_eq!(agents, vec!["claude-code", "codex", "open-code"]);
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
                "npx -y skill-studio@latest --version",
                "claude mcp remove --scope user skill-studio",
                "claude mcp add --scope user --transport stdio skill-studio -- npx -y skill-studio@latest mcp",
            ]
        );
    }

    #[test]
    fn a_failed_codex_mcp_add_returns_its_stderr_instead_of_success() {
        let home = tempfile::tempdir().unwrap();
        let recorder = spawner(Some(("codex mcp add", "config.toml is not writable")));

        let err = connect_agent(&runtime(home.path(), recorder.clone()), "codex").unwrap_err();

        assert!(err.contains("config.toml is not writable"), "{err}");
        assert_eq!(
            recorder.commands(),
            vec![
                "npx -y skill-studio@latest --version",
                "codex mcp add skill-studio -- npx -y skill-studio@latest mcp",
            ]
        );
    }

    #[test]
    fn re_adding_claude_code_without_an_entry_ignores_the_failed_remove_and_still_adds() {
        let home = tempfile::tempdir().unwrap();
        let recorder = spawner(Some(("claude mcp remove", "No MCP server found")));

        connect_agent(&runtime(home.path(), recorder.clone()), "claude-code").unwrap();

        assert_eq!(
            recorder.commands().last().unwrap(),
            "claude mcp add --scope user --transport stdio skill-studio -- npx -y skill-studio@latest mcp"
        );
    }

    #[test]
    fn a_failed_server_download_stops_before_the_agent_config_changes() {
        let home = tempfile::tempdir().unwrap();
        let recorder = spawner(Some(("npx", "network unreachable")));

        let err = connect_agent(&runtime(home.path(), recorder.clone()), "codex").unwrap_err();

        assert!(err.contains("network unreachable"), "{err}");
        assert_eq!(
            recorder.commands(),
            vec!["npx -y skill-studio@latest --version"]
        );
    }

    #[test]
    fn opencode_pi_and_grok_each_run_their_own_user_scope_mcp_add_from_the_home_folder() {
        let home = tempfile::tempdir().unwrap();
        for (harness, expected) in [
            (
                "open-code",
                vec!["opencode mcp add --global skill-studio -- npx -y skill-studio@latest mcp"],
            ),
            (
                "pi",
                vec!["pi mcp add skill-studio -- npx -y skill-studio@latest mcp"],
            ),
            (
                "grok-build",
                vec![
                    "grok mcp remove skill-studio",
                    "grok mcp add skill-studio -- npx -y skill-studio@latest mcp",
                ],
            ),
        ] {
            let recorder = spawner(None);
            connect_agent(&runtime(home.path(), recorder.clone()), harness).unwrap();
            let commands = recorder.commands();
            assert_eq!(
                commands[0], "npx -y skill-studio@latest --version",
                "{harness}"
            );
            assert_eq!(commands[1..], expected[..], "{harness}");
            let cwds: Vec<_> = recorder
                .calls
                .lock()
                .unwrap()
                .iter()
                .map(|call| call.cwd.clone())
                .collect();
            assert!(
                cwds.iter().all(|cwd| cwd.as_deref() == Some(home.path())),
                "{harness} ran outside the home folder: {cwds:?}"
            );
        }
    }

    #[test]
    fn connecting_cursor_opens_its_install_link_whose_config_is_the_published_server() {
        let home = tempfile::tempdir().unwrap();
        let recorder = spawner(None);

        connect_agent(&runtime(home.path(), recorder.clone()), "cursor").unwrap();

        assert_eq!(
            recorder.commands(),
            vec![
                "npx -y skill-studio@latest --version".to_string(),
                format!("open {CURSOR_INSTALL_LINK}"),
            ]
        );
        let encoded = CURSOR_INSTALL_LINK
            .split("config=")
            .nth(1)
            .unwrap()
            .replace("%3D", "=");
        let decoded = decode_base64(&encoded);
        let config: serde_json::Value = serde_json::from_slice(&decoded).unwrap();
        assert_eq!(
            config,
            serde_json::json!({"command": "npx", "args": ["-y", "skill-studio@latest", "mcp"]})
        );
    }

    fn decode_base64(text: &str) -> Vec<u8> {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut bits = 0u32;
        let mut count = 0;
        let mut out = Vec::new();
        for byte in text.bytes().filter(|byte| *byte != b'=') {
            let value = ALPHABET.iter().position(|c| *c == byte).unwrap() as u32;
            bits = (bits << 6) | value;
            count += 6;
            if count >= 8 {
                count -= 8;
                out.push((bits >> count) as u8);
                bits &= (1 << count) - 1;
            }
        }
        out
    }

    #[test]
    fn a_failed_add_puts_a_plain_summary_on_the_first_line_and_the_command_output_below() {
        let home = tempfile::tempdir().unwrap();
        let recorder = spawner(Some(("pi mcp add", "mcp.json is not valid JSON")));

        let err = connect_agent(&runtime(home.path(), recorder), "pi").unwrap_err();

        let mut lines = err.lines();
        assert_eq!(lines.next(), Some("pi couldn't add the server."));
        assert_eq!(
            lines.next(),
            Some("$ pi mcp add skill-studio -- npx -y skill-studio@latest mcp")
        );
        assert_eq!(lines.next(), Some("mcp.json is not valid JSON"));
    }

    #[test]
    fn an_agent_without_an_mcp_add_command_is_refused_before_anything_runs() {
        let home = tempfile::tempdir().unwrap();
        let recorder = spawner(None);

        assert!(connect_agent(&runtime(home.path(), recorder.clone()), "vscode").is_err());
        assert!(recorder.commands().is_empty());
    }
}
