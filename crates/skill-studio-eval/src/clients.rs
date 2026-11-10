use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::process::{capture, ExecutionState, ProcessSpec};
use crate::protocol::HarnessId;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ClientInfo {
    pub id: HarnessId,
    pub binary: Option<String>,
    pub version: Option<String>,
    pub available: bool,
    pub no_skill_control: bool,
    pub note: String,
}

pub fn isolation_note(client: HarnessId) -> &'static str {
    match client {
        HarnessId::Pi => "Native skill discovery, context files, extensions and MCP are disabled. Only the selected snapshot is added. This is a local client process, not an operating-system sandbox.",
        HarnessId::Codex => "Fresh session and workspace; user config is ignored. Existing login is used. Other user or bundled skills may remain available. Compare as an environment test.",
        HarnessId::ClaudeCode => "Fresh session and workspace; project settings and an empty MCP list are used. Existing login is used. User skills may remain available. Compare as an environment test.",
        HarnessId::OpenCode => "Fresh standalone session and workspace. Existing login and user configuration are used. Other skills may remain available. Compare as an environment test.",
    }
}

pub async fn inspect_clients() -> Vec<ClientInfo> {
    let mut result = Vec::new();
    for client in [
        HarnessId::ClaudeCode,
        HarnessId::Codex,
        HarnessId::OpenCode,
        HarnessId::Pi,
    ] {
        let inspected = inspect(client).await;
        result.push(match inspected {
            Ok((binary, version)) => ClientInfo {
                id: client,
                binary: Some(binary.to_string_lossy().into()),
                version: Some(version),
                available: true,
                no_skill_control: client == HarnessId::Pi,
                note: format!(
                    "{} Sign-in is checked when a run starts.",
                    isolation_note(client)
                ),
            },
            Err(error) => ClientInfo {
                id: client,
                binary: None,
                version: None,
                available: false,
                no_skill_control: false,
                note: error,
            },
        });
    }
    result
}

pub async fn inspect(client: HarnessId) -> Result<(PathBuf, String), String> {
    let binary = resolve_binary(client).await?;
    let folder = tempfile::tempdir().map_err(|e| e.to_string())?;
    let result = capture(
        &ProcessSpec {
            program: binary.clone(),
            args: vec!["--version".into()],
            env: vec![],
            cwd: folder.path().into(),
            timeout_seconds: 10,
        },
        folder.path(),
        Arc::default(),
        |_| {},
    )
    .await?;
    if result.state != ExecutionState::Completed {
        return Err(result
            .error
            .unwrap_or_else(|| "Client version check failed".into()));
    }
    Ok((binary, result.output.trim().to_string()))
}

pub async fn resolve_binary(client: HarnessId) -> Result<PathBuf, String> {
    let bin = client.bin_name();
    if let Some(paths) = std::env::var_os("PATH") {
        if let Some(path) = std::env::split_paths(&paths)
            .map(|p| p.join(bin))
            .find(|p| executable(p))
        {
            return Ok(path);
        }
    }
    let folder = tempfile::tempdir().map_err(|e| e.to_string())?;
    let result = capture(
        &ProcessSpec {
            program: std::env::var("SHELL")
                .unwrap_or_else(|_| "/bin/zsh".into())
                .into(),
            args: vec!["-lc".into(), format!("command -v {bin}")],
            env: vec![],
            cwd: folder.path().into(),
            timeout_seconds: 10,
        },
        folder.path(),
        Arc::default(),
        |_| {},
    )
    .await?;
    let path = result
        .output
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .map(|line| PathBuf::from(line.trim()));
    path.filter(|p| p.is_absolute() && executable(p))
        .ok_or_else(|| {
            format!("{bin} was not found. Install the client and sign in, then refresh.")
        })
}

fn executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    path.is_file()
}

pub struct ClientRequest<'a> {
    pub client: HarnessId,
    pub binary: &'a Path,
    pub model: Option<&'a str>,
    pub workspace: &'a Path,
    pub skill: Option<&'a Path>,
    pub prompt: &'a str,
    pub timeout_seconds: u64,
    pub judge: bool,
}

pub fn command(request: &ClientRequest<'_>) -> Result<ProcessSpec, String> {
    let mut args: Vec<String> = match request.client {
        HarnessId::Codex => [
            "exec",
            "--json",
            "--ephemeral",
            "--skip-git-repo-check",
            "--ignore-user-config",
            "-s",
            "workspace-write",
        ]
        .map(str::to_string)
        .into(),
        HarnessId::ClaudeCode => {
            let mut args = [
                "-p",
                "--output-format",
                "stream-json",
                "--verbose",
                "--no-session-persistence",
                "--setting-sources",
                "project",
                "--strict-mcp-config",
                "--mcp-config",
                "{\"mcpServers\":{}}",
                "--permission-prompts",
                "none",
                "--permission-mode",
                if request.judge { "dontAsk" } else { "auto" },
                "--tools",
                if request.judge {
                    ""
                } else {
                    "Read,Write,Edit,Glob,Grep,Bash,Skill"
                },
            ]
            .map(str::to_string)
            .to_vec();
            if request.judge {
                args.push("--disable-slash-commands".into());
            }
            args
        }
        HarnessId::OpenCode => [
            "run",
            "--standalone",
            "--format",
            "json",
            "--archived",
            "--auto",
        ]
        .map(str::to_string)
        .into(),
        HarnessId::Pi => {
            let mut args = [
                "-p",
                "--mode",
                "json",
                "--no-session",
                "--no-skills",
                "--no-context-files",
                "--no-extensions",
                "--no-mcp",
                "--no-prompt-templates",
                "--no-themes",
                "--approve",
            ]
            .map(str::to_string)
            .to_vec();
            if let Some(skill) = request.skill {
                args.extend(["--skill".into(), skill.to_string_lossy().into_owned()]);
            }
            if request.judge {
                args.push("--no-tools".into());
            }
            args
        }
    };
    if request.judge && !matches!(request.client, HarnessId::ClaudeCode | HarnessId::Pi) {
        return Err(
            "Choose Claude Code or pi for the separate judge; their tool-free mode is supported."
                .into(),
        );
    }
    if !request.judge && request.skill.is_none() && request.client != HarnessId::Pi {
        return Err("A no-skill control is currently supported only by pi. Use a previous skill version for this client.".into());
    }
    if let Some(model) = request.model {
        args.extend(["--model".into(), model.into()]);
    }
    args.push("--".into());
    args.push(request.prompt.into());
    Ok(ProcessSpec {
        program: request.binary.into(),
        args,
        env: vec![],
        cwd: request.workspace.into(),
        timeout_seconds: request.timeout_seconds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unproved_control_is_refused_and_pi_control_omits_the_snapshot() {
        let mut request = ClientRequest {
            client: HarnessId::Codex,
            binary: Path::new("/bin/client"),
            model: None,
            workspace: Path::new("/tmp/workspace"),
            skill: None,
            prompt: "do work",
            timeout_seconds: 30,
            judge: false,
        };
        assert!(command(&request).is_err());
        request.client = HarnessId::Pi;
        let control = command(&request).unwrap();
        assert!(control.args.contains(&"--no-skills".into()));
        assert!(!control.args.contains(&"--skill".into()));
        request.skill = Some(Path::new("/tmp/workspace/.pi/skills/report"));
        let candidate = command(&request).unwrap();
        assert!(candidate.args.contains(&"--skill".into()));
        assert_eq!(candidate.env, control.env);
    }
}
