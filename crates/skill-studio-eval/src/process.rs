use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Notify;

use crate::protocol::{parse_line, HarnessId, ParseState, SkillAgentEventKind, SkillLoaded};

const STREAM_LIMIT: usize = 16 * 1024 * 1024;
const LINE_LIMIT: usize = 1024 * 1024;

#[derive(Default)]
pub struct Cancellation {
    requested: AtomicBool,
    notify: Notify,
}

impl Cancellation {
    pub fn cancel(&self) {
        self.requested.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        self.requested.load(Ordering::SeqCst)
    }

    async fn cancelled(&self) {
        let notified = self.notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if !self.is_cancelled() {
            notified.await;
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProcessSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: PathBuf,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionState {
    Completed,
    Failed,
    Cancelled,
    TimedOut,
    Interrupted,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cost_usd: Option<f64>,
    pub source: Option<String>,
    pub reported_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProcessResult {
    pub state: ExecutionState,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub output: String,
    pub error: Option<String>,
    pub skill_loaded: SkillLoaded,
    pub usage: Usage,
    pub events: Vec<SkillAgentEventKind>,
}

impl ProcessResult {
    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            state: ExecutionState::Failed,
            exit_code: None,
            duration_ms: 0,
            output: String::new(),
            error: Some(message.into()),
            skill_loaded: SkillLoaded::Unknown,
            usage: Usage::default(),
            events: Vec::new(),
        }
    }
}

pub async fn run_client(
    spec: &ProcessSpec,
    client: HarnessId,
    skill_name: &str,
    skill_file: Option<&Path>,
    evidence: &Path,
    cancellation: Arc<Cancellation>,
) -> ProcessResult {
    let mut parser = ParseState::default();
    let mut events = Vec::new();
    let mut usage = Usage::default();
    let mut loading = crate::observations::LoadingEvidence::default();
    let mut usage_seen = std::collections::HashSet::new();
    let mut native_error = None;
    let mut final_text = None;
    let result = capture(spec, evidence, cancellation, |line| {
        capture_usage(client, line, &mut usage, &mut usage_seen);
        if let Some(skill_file) = skill_file {
            loading.observe(client, line, skill_file, &spec.cwd);
        }
        for event in parse_line(client, line, skill_name, &mut parser) {
            if let SkillAgentEventKind::Finished {
                ok,
                final_text: text,
                cost_usd,
                ..
            } = &event
            {
                final_text = Some(text.clone());
                if !ok {
                    native_error = Some(text.clone());
                }
                if cost_usd.is_some() {
                    usage.cost_usd = *cost_usd;
                }
            }
            events.push(event);
        }
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            if value["type"] == "message_end" && value["message"]["stopReason"] == "error" {
                native_error = Some(
                    value["message"]["errorMessage"]
                        .as_str()
                        .unwrap_or("Client reported an error")
                        .into(),
                );
            }
            if value["type"] == "turn.failed" || value["type"] == "error" {
                native_error = Some(
                    value["error"]["message"]
                        .as_str()
                        .or_else(|| value["message"].as_str())
                        .unwrap_or("Client reported an error")
                        .to_string(),
                );
            }
        }
    })
    .await;
    let mut result = match result {
        Ok(result) => result,
        Err(error) => return ProcessResult::failed(error),
    };
    result.output = final_text
        .or(parser.last_agent_message)
        .unwrap_or(parser.last_text);
    if result.state == ExecutionState::Completed {
        if let Some(error) = native_error {
            result.state = ExecutionState::Failed;
            result.error = Some(error);
        } else if result.output.is_empty() && events.is_empty() {
            result.state = ExecutionState::Failed;
            result.error = Some(
                "The client returned no supported events. Check its version and saved output."
                    .into(),
            );
        }
    }
    result.skill_loaded = loading.loaded;
    if usage.cost_usd.is_none() {
        usage.cost_usd = parser.cost_usd;
    }
    result.usage = usage;
    result.events = events;
    result
}

pub async fn capture(
    spec: &ProcessSpec,
    evidence: &Path,
    cancellation: Arc<Cancellation>,
    mut on_line: impl FnMut(&str) + Send,
) -> Result<ProcessResult, String> {
    std::fs::create_dir_all(evidence).map_err(|e| e.to_string())?;
    let mut stdout_log = File::create(evidence.join("stdout.jsonl")).map_err(|e| e.to_string())?;
    let mut stderr_log = File::create(evidence.join("stderr.txt")).map_err(|e| e.to_string())?;
    if cancellation.is_cancelled() {
        let mut result = ProcessResult::failed("Cancelled before launch");
        result.state = ExecutionState::Cancelled;
        return Ok(result);
    }
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .envs(spec.env.iter().cloned())
        .current_dir(&spec.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let start = Instant::now();
    let mut child = command
        .spawn()
        .map_err(|e| format!("Could not start {}: {e}", spec.program.display()))?;
    let pid = child.id();
    if let Err(error) = crate::recovery::ProcessIdentity::save(pid, evidence) {
        terminate_group(pid);
        let _ = child.kill().await;
        return Err(error);
    }
    let stdout = child.stdout.take().ok_or("Client stdout is unavailable")?;
    let stderr = child.stderr.take().ok_or("Client stderr is unavailable")?;
    let mut output = String::new();
    let mut stderr_tail = String::new();
    let completion = {
        let read_out = read_lines(stdout, &mut stdout_log, |line| {
            output.push_str(line);
            output.push('\n');
            on_line(line);
        });
        let read_err = read_lines(stderr, &mut stderr_log, |line| {
            stderr_tail.push_str(line);
            stderr_tail.push('\n');
            if stderr_tail.len() > 8192 {
                let cut = stderr_tail
                    .char_indices()
                    .map(|(i, _)| i)
                    .find(|i| *i >= stderr_tail.len() - 4096)
                    .unwrap_or(0);
                stderr_tail.drain(..cut);
            }
        });
        let completion = async {
            let ((), (), status) = tokio::try_join!(read_out, read_err, async {
                child.wait().await.map_err(|e| e.to_string())
            })?;
            Ok::<_, String>(status)
        };
        tokio::select! {
            result = completion => match result {
                Ok(status) => (if status.success() { ExecutionState::Completed } else { ExecutionState::Failed }, status.code(), None),
                Err(error) => (ExecutionState::Failed, None, Some(error)),
            },
            () = cancellation.cancelled() => (ExecutionState::Cancelled, None, Some("Cancelled by user".into())),
            () = tokio::time::sleep(Duration::from_secs(spec.timeout_seconds)) => (ExecutionState::TimedOut, None, Some(format!("Client exceeded {} seconds", spec.timeout_seconds))),
        }
    };
    terminate_group(pid);
    if completion.0 != ExecutionState::Completed {
        let _ = child.kill().await;
    }
    let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
    stdout_log.sync_all().map_err(|e| e.to_string())?;
    stderr_log.sync_all().map_err(|e| e.to_string())?;
    let error = completion.2.or_else(|| {
        (completion.0 == ExecutionState::Failed).then(|| {
            if stderr_tail.trim().is_empty() {
                format!("Process exited with {:?}", completion.1)
            } else {
                stderr_tail.trim().to_string()
            }
        })
    });
    Ok(ProcessResult {
        state: completion.0,
        exit_code: completion.1,
        duration_ms: start.elapsed().as_millis() as u64,
        output,
        error,
        skill_loaded: SkillLoaded::Unknown,
        usage: Usage::default(),
        events: Vec::new(),
    })
}

pub(crate) fn terminate_group(pid: Option<u32>) {
    #[cfg(unix)]
    if let Some(pid) = pid.and_then(|id| i32::try_from(id).ok()) {
        let _ = nix::sys::signal::killpg(
            nix::unistd::Pid::from_raw(pid),
            nix::sys::signal::Signal::SIGKILL,
        );
    }
}

async fn read_lines(
    reader: impl AsyncRead + Unpin,
    log: &mut File,
    mut receive: impl FnMut(&str),
) -> Result<(), String> {
    let mut reader = BufReader::new(reader);
    let mut total = 0;
    loop {
        let mut line = Vec::new();
        let count = (&mut reader)
            .take((LINE_LIMIT + 1) as u64)
            .read_until(b'\n', &mut line)
            .await
            .map_err(|e| e.to_string())?;
        if count == 0 {
            return Ok(());
        }
        total += count;
        if count > LINE_LIMIT || total > STREAM_LIMIT {
            return Err("Client output exceeded the saved evidence limit".into());
        }
        log.write_all(&line)
            .map_err(|e| format!("Could not save client evidence: {e}"))?;
        receive(String::from_utf8_lossy(&line).trim_end_matches(['\r', '\n']));
    }
}

fn capture_usage(
    client: HarnessId,
    line: &str,
    usage: &mut Usage,
    seen: &mut std::collections::HashSet<String>,
) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        return;
    };
    if let Some(model) = value["model"]
        .as_str()
        .or_else(|| value["message"]["model"].as_str())
    {
        usage.reported_model = Some(model.into());
    }
    let event_key = match client {
        HarnessId::OpenCode => value["part"]["id"].as_str().map(str::to_string),
        HarnessId::Pi if value["type"] == "message_end" => {
            value["message"]["responseId"].as_str().map(str::to_string)
        }
        _ => None,
    };
    if event_key.is_some_and(|key| !seen.insert(key)) {
        return;
    }
    let (data, source, accumulate) = match client {
        HarnessId::Codex if value["type"] == "turn.completed" => {
            (&value["usage"], "codex.turn.completed", false)
        }
        HarnessId::ClaudeCode if value["type"] == "result" => {
            (&value["usage"], "claude.result", false)
        }
        HarnessId::Pi if value["type"] == "message_end" => {
            (&value["message"]["usage"], "pi.message_end", true)
        }
        HarnessId::OpenCode if value["type"] == "step_finish" => (
            &value["part"]["tokens"],
            "opencode.step_finish (client reported)",
            true,
        ),
        _ => return,
    };
    if !data.is_object() {
        return;
    }
    let add = |existing: Option<u64>, next: Option<u64>| {
        if accumulate {
            next.map(|n| existing.unwrap_or(0) + n).or(existing)
        } else {
            next.or(existing)
        }
    };
    usage.input_tokens = add(
        usage.input_tokens,
        data["input_tokens"]
            .as_u64()
            .or_else(|| data["input"].as_u64()),
    );
    usage.output_tokens = add(
        usage.output_tokens,
        data["output_tokens"]
            .as_u64()
            .or_else(|| data["output"].as_u64()),
    );
    usage.cached_input_tokens = add(
        usage.cached_input_tokens,
        data["cached_input_tokens"].as_u64().or_else(|| {
            data["cacheRead"]
                .as_u64()
                .or_else(|| data["cache_read_input_tokens"].as_u64())
                .or_else(|| data["cache"]["read"].as_u64())
        }),
    );
    usage.source = Some(source.into());
    if client == HarnessId::OpenCode {
        if let Some(cost) = value["part"]["cost"].as_f64() {
            usage.cost_usd = Some(usage.cost_usd.unwrap_or(0.0) + cost);
        }
    }
    if let Some(cost) = data["cost"]["total"].as_f64() {
        usage.cost_usd = Some(usage.cost_usd.unwrap_or(0.0) + cost);
        if data["cost"]["totalFromProvider"].as_bool() != Some(true) {
            usage.source = Some("pi.message_end (client cost estimate)".into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(script: &str) -> ProcessSpec {
        ProcessSpec {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), script.into()],
            env: vec![],
            cwd: std::env::temp_dir(),
            timeout_seconds: 2,
        }
    }

    #[tokio::test]
    async fn native_error_cannot_pass_even_with_a_zero_process_exit() {
        let dir = tempfile::tempdir().unwrap();
        let result = run_client(&spec("printf '%s\n' '{\"type\":\"turn.failed\",\"error\":{\"message\":\"Sign in required\"}}'"), HarnessId::Codex, "skill", None, dir.path(), Arc::default()).await;
        assert_eq!(result.state, ExecutionState::Failed);
        assert!(result.error.unwrap().contains("Sign in"));
    }

    #[tokio::test]
    async fn deadline_and_cancellation_bound_process_and_inherited_pipes() {
        let dir = tempfile::tempdir().unwrap();
        let mut command = spec("sleep 30 & wait");
        command.timeout_seconds = 1;
        let result = capture(&command, dir.path(), Arc::default(), |_| {})
            .await
            .unwrap();
        assert_eq!(result.state, ExecutionState::TimedOut);
        assert!(result.duration_ms < 4000);
        let cancel = Arc::new(Cancellation::default());
        let pending = capture(&command, dir.path(), cancel.clone(), |_| {});
        let stop = async {
            tokio::time::sleep(Duration::from_millis(30)).await;
            cancel.cancel();
        };
        let (result, ()) = tokio::join!(pending, stop);
        assert_eq!(result.unwrap().state, ExecutionState::Cancelled);
    }

    #[tokio::test]
    async fn oversized_output_stops_without_unbounded_allocation() {
        let dir = tempfile::tempdir().unwrap();
        let result = capture(
            &spec("head -c 2000000 /dev/zero"),
            dir.path(),
            Arc::default(),
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(result.state, ExecutionState::Failed);
        assert!(result.error.unwrap().contains("limit"));
    }

    #[test]
    fn absent_metrics_remain_unknown_and_reported_metrics_keep_their_source() {
        let mut usage = Usage::default();
        capture_usage(
            HarnessId::Codex,
            r#"{"type":"item.completed"}"#,
            &mut usage,
            &mut std::collections::HashSet::new(),
        );
        assert!(usage.input_tokens.is_none());
        capture_usage(
            HarnessId::Codex,
            r#"{"type":"turn.completed","usage":{"input_tokens":12,"cached_input_tokens":4,"output_tokens":3}}"#,
            &mut usage,
            &mut std::collections::HashSet::new(),
        );
        assert_eq!(usage.input_tokens, Some(12));
        assert!(usage.cost_usd.is_none());
    }

    #[test]
    fn installed_client_traces_report_successful_skill_reads_and_real_usage() {
        for (client, trace) in [
            (
                HarnessId::Codex,
                include_str!("../tests/fixtures/codex.jsonl"),
            ),
            (
                HarnessId::ClaudeCode,
                include_str!("../tests/fixtures/claude.jsonl"),
            ),
            (
                HarnessId::OpenCode,
                include_str!("../tests/fixtures/opencode2.jsonl"),
            ),
            (HarnessId::Pi, include_str!("../tests/fixtures/pi.jsonl")),
        ] {
            let mut state = ParseState::default();
            let mut loading = crate::observations::LoadingEvidence::default();
            let mut usage = Usage::default();
            let mut seen = std::collections::HashSet::new();
            let mut events = Vec::new();
            let (folder, skills) = match client {
                HarnessId::ClaudeCode => ("claude", ".claude/skills"),
                HarnessId::Codex => ("codex", ".agents/skills"),
                HarnessId::OpenCode => ("opencode2", ".agents/skills"),
                HarnessId::Pi => ("pi", ".pi/skills"),
            };
            let workspace = PathBuf::from("/tmp/skill-lab-probe").join(folder);
            let skill_file = workspace.join(skills).join("lab-report/SKILL.md");
            for line in trace.lines() {
                loading.observe(client, line, &skill_file, &workspace);
                capture_usage(client, line, &mut usage, &mut seen);
                events.extend(parse_line(client, line, "lab-report", &mut state));
            }
            assert_eq!(loading.loaded, SkillLoaded::Yes, "{client:?}");
            assert!(usage.input_tokens.is_some(), "{client:?}");
            assert!(usage.output_tokens.is_some(), "{client:?}");
            assert!(
                events
                    .iter()
                    .any(|e| matches!(e, SkillAgentEventKind::ToolCall { .. })),
                "{client:?}"
            );
            let tokens = usage.input_tokens;
            if matches!(client, HarnessId::Pi | HarnessId::OpenCode) {
                for line in trace.lines() {
                    capture_usage(client, line, &mut usage, &mut seen);
                }
                assert_eq!(
                    usage.input_tokens, tokens,
                    "Repeated native events must not double-count usage"
                );
            }
        }
    }
}
