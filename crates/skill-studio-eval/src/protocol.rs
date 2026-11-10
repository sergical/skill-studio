use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One of the four first-class agents a skill run can target.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum HarnessId {
    ClaudeCode,
    Codex,
    OpenCode,
    Pi,
}

impl HarnessId {
    /// The binary name resolved on `PATH` to start this harness. `OpenCode`
    /// resolves to `opencode2` (the v2 CLI); the variant name and its
    /// `open-code` serde id are unchanged so history and settings still work.
    pub fn bin_name(&self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude",
            Self::Codex => "codex",
            Self::OpenCode => "opencode2",
            Self::Pi => "pi",
        }
    }
}

/// Whether a run may write to its scratch directory or only read it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WriteAccess {
    ReadOnly,
    Workspace,
}

/// Whether the run loaded the skill under test, inferred per-harness from
/// its tool-call events - see `parse_line`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SkillLoaded {
    Yes,
    No,
    #[default]
    Unknown,
}

/// Request to start one headless harness run against one skill.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct SkillAgentRunRequest {
    pub harness: HarnessId,
    pub prompt: String,
    pub cwd: String,
    pub skill_name: String,
    pub write_access: WriteAccess,
    /// Set to continue an earlier run's session/thread.
    pub session_id: Option<String>,
}

/// One line of a run's transcript, in emission order (`seq`). Deserialize is
/// needed alongside Serialize because `skill_run_history` round-trips events
/// through its `.events.jsonl` transcript files.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SkillAgentEvent {
    pub run_id: String,
    pub seq: u64,
    pub at: DateTime<Utc>,
    pub kind: SkillAgentEventKind,
}

/// The discriminated union of everything a run can report.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SkillAgentEventKind {
    Started {
        command: String,
        session_id: Option<String>,
    },
    AssistantText {
        text: String,
        is_delta: bool,
    },
    ToolCall {
        name: String,
        summary: String,
        detail: Option<String>,
    },
    ToolResult {
        name: String,
        summary: String,
    },
    Finished {
        ok: bool,
        /// `true` only for the cancel branch's terminal `Finished`. Carries the
        /// user-initiated cancel signal as a first-class field so the frontend
        /// can render "Cancelled" without keying off the `final_text` magic
        /// string. `#[serde(default)]` so transcripts written before this
        /// field existed deserialize with `cancelled: false` (a genuine
        /// finish, never a cancel).
        #[serde(default)]
        cancelled: bool,
        final_text: String,
        session_id: Option<String>,
        cost_usd: Option<f64>,
        duration_ms: u64,
        skill_loaded: SkillLoaded,
    },
    Error {
        message: String,
    },
}

/// Per-run state `parse_line` accumulates as it walks a harness's lines:
/// the session/thread id (for follow-ups), the latest assistant text seen,
/// whether the skill under test was loaded, and Codex's last completed
/// `agent_message` (its closest thing to a running "final text").
#[derive(Debug, Default)]
pub struct ParseState {
    pub session_id: Option<String>,
    pub last_text: String,
    pub skill_loaded: SkillLoaded,
    pub last_agent_message: Option<String>,
    /// Sum of every distinct `OpenCode` `step_finish.part.cost` seen, keyed by
    /// `part.id` in `seen_step_finish_ids` below - `run_process` folds this
    /// into the `Finished` event it synthesizes for `OpenCode`, since `OpenCode`
    /// never emits its own terminal record the way Claude/Codex do.
    pub cost_usd: Option<f64>,
    /// `part.id` of every `OpenCode` `step_finish` already folded into
    /// `cost_usd`, so a reprint of the same step doesn't double-count it.
    pub seen_step_finish_ids: std::collections::HashSet<String>,
    /// `callID`/`id` of every `OpenCode` tool part already turned into a
    /// `ToolCall`, so the pending/running/completed re-prints of the same
    /// part don't each emit their own `ToolCall`.
    pub announced_open_code_tool_ids: std::collections::HashSet<String>,
    /// `callID`/`id` of every `OpenCode` tool part already turned into a
    /// `ToolResult` or `Error`, so a call is resolved at most once even
    /// though the CLI keeps reprinting it after it settles.
    pub resolved_open_code_tool_ids: std::collections::HashSet<String>,
}

/// Build the `(program, args, env, cwd)` to spawn for `request`, per the
/// per-harness CLI shapes observed in the field - see the module doc in the
/// spec this was built from. Pure: takes the resolved binary as input rather
/// than resolving it itself, so it's unit-testable without a real harness.
pub fn build_command(
    request: &SkillAgentRunRequest,
    binary: &Path,
) -> (String, Vec<String>, Vec<(String, String)>, PathBuf) {
    let program = binary.to_string_lossy().to_string();
    let cwd = PathBuf::from(&request.cwd);
    let env = Vec::new();

    let args = match request.harness {
        HarnessId::ClaudeCode => {
            let mut args = vec![
                "-p".to_string(),
                request.prompt.clone(),
                "--output-format".to_string(),
                "stream-json".to_string(),
                "--verbose".to_string(),
                "--permission-mode".to_string(),
            ];
            match request.write_access {
                WriteAccess::ReadOnly => {
                    args.push("dontAsk".to_string());
                    args.push("--allowedTools".to_string());
                    args.push(
                        "Read,Grep,Glob,Bash(ls *),Bash(cat *),Bash(git diff *),Bash(git status *)"
                            .to_string(),
                    );
                }
                WriteAccess::Workspace => args.push("auto".to_string()),
            }
            if let Some(session_id) = &request.session_id {
                args.push("--resume".to_string());
                args.push(session_id.clone());
            }
            args
        }
        HarnessId::Codex => {
            let mode = match request.write_access {
                WriteAccess::ReadOnly => "read-only",
                WriteAccess::Workspace => "workspace-write",
            };
            let mut args = vec!["exec".to_string()];
            if let Some(thread_id) = &request.session_id {
                args.push("resume".to_string());
                args.push(thread_id.clone());
                args.push("--json".to_string());
            } else {
                args.push("--json".to_string());
                args.push("--skip-git-repo-check".to_string());
                args.push("-s".to_string());
                args.push(mode.to_string());
                args.push("-C".to_string());
                args.push(request.cwd.clone());
            }
            args.push(request.prompt.clone());
            args
        }
        HarnessId::OpenCode => {
            // No `--dir` flag exists; cwd comes from `Command::current_dir`
            // below. `--standalone` bypasses the shared `opencode2 serve
            // --service` background service (a wedged instance otherwise
            // hangs every run with no output). `--auto` is passed in both
            // write modes so a non-TTY run never blocks on an "ask"
            // permission. Read-only is not enforced here: nine live probes of
            // v0.0.0-beta-17498 showed the CLI ignores `OPENCODE_CONFIG`, and
            // cwd-scoped permission denies for `edit`/`write`/`patch`/etc.
            // never blocked `patch` from creating a file, so an OpenCode run
            // always has workspace write access regardless of `write_access`.
            let mut args = vec![
                "run".to_string(),
                "--standalone".to_string(),
                "--format".to_string(),
                "json".to_string(),
                "--auto".to_string(),
            ];
            if let Some(session_id) = &request.session_id {
                args.push("--session".to_string());
                args.push(session_id.clone());
            }
            args.push(request.prompt.clone());
            args
        }
        HarnessId::Pi => {
            let mut args = vec!["-p".to_string(), "--mode".to_string(), "json".to_string()];
            if request.write_access == WriteAccess::ReadOnly {
                args.push("--tools".to_string());
                args.push("read,grep,find,ls".to_string());
            }
            if let Some(session_id) = &request.session_id {
                args.push("--session".to_string());
                args.push(session_id.clone());
            }
            args.push(request.prompt.clone());
            args
        }
    };

    (program, args, env, cwd)
}

/// Parse one line of a harness's stdout into zero or more transcript events,
/// updating `state` along the way. Unrecognized or malformed lines parse to
/// no events, matching the "ignore what we don't understand" stance the
/// observed formats require (hook/rate-limit lines, reasoning steps, etc).
pub fn parse_line(
    harness: HarnessId,
    line: &str,
    skill_name: &str,
    state: &mut ParseState,
) -> Vec<SkillAgentEventKind> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
        return Vec::new();
    };

    match harness {
        HarnessId::ClaudeCode => parse_claude_line(&value, skill_name, state),
        HarnessId::Codex => parse_codex_line(&value, skill_name, state),
        HarnessId::OpenCode => parse_open_code_line(&value, skill_name, state),
        HarnessId::Pi => parse_pi_line(&value, skill_name, state),
    }
}

fn parse_claude_line(
    value: &Value,
    skill_name: &str,
    state: &mut ParseState,
) -> Vec<SkillAgentEventKind> {
    let mut events = Vec::new();
    match value.get("type").and_then(Value::as_str).unwrap_or("") {
        "system" => {
            if let Some(session_id) = value.get("session_id").and_then(Value::as_str) {
                state.session_id = Some(session_id.to_string());
            }
        }
        "assistant" => {
            if let Some(blocks) = value.pointer("/message/content").and_then(Value::as_array) {
                for block in blocks {
                    match block.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            if let Some(text) = block.get("text").and_then(Value::as_str) {
                                state.last_text = text.to_string();
                                events.push(SkillAgentEventKind::AssistantText {
                                    text: text.to_string(),
                                    is_delta: false,
                                });
                            }
                        }
                        Some("tool_use") => {
                            let name = block
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or("tool")
                                .to_string();
                            let input = block.get("input").cloned().unwrap_or(Value::Null);
                            if name == "Skill" {
                                let loaded_this_skill =
                                    input.get("skill").and_then(Value::as_str) == Some(skill_name);
                                if loaded_this_skill {
                                    state.skill_loaded = SkillLoaded::Yes;
                                } else if state.skill_loaded == SkillLoaded::Unknown {
                                    state.skill_loaded = SkillLoaded::No;
                                }
                            }
                            let summary = input
                                .get("skill")
                                .and_then(Value::as_str)
                                .map_or_else(|| input.to_string(), str::to_string);
                            events.push(SkillAgentEventKind::ToolCall {
                                name,
                                summary,
                                detail: Some(input.to_string()),
                            });
                        }
                        // "thinking" and anything else observed here are ignored.
                        _ => {}
                    }
                }
            }
        }
        "user" => {
            if let Some(blocks) = value.pointer("/message/content").and_then(Value::as_array) {
                for block in blocks {
                    if block.get("type").and_then(Value::as_str) == Some("tool_result") {
                        let summary = block
                            .get("content")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        events.push(SkillAgentEventKind::ToolResult {
                            name: "tool_result".to_string(),
                            summary,
                        });
                    }
                }
            }
        }
        "result" => {
            let is_error = value
                .get("is_error")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let final_text = value
                .get("result")
                .and_then(Value::as_str)
                .map_or_else(|| state.last_text.clone(), str::to_string);
            let session_id = value
                .get("session_id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| state.session_id.clone());
            if let Some(session_id) = &session_id {
                state.session_id = Some(session_id.clone());
            }
            events.push(SkillAgentEventKind::Finished {
                ok: !is_error,
                cancelled: false,
                final_text,
                session_id,
                cost_usd: value.get("total_cost_usd").and_then(Value::as_f64),
                duration_ms: value
                    .get("duration_ms")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                skill_loaded: state.skill_loaded,
            });
        }
        // "system/hook_*", "rate_limit_event": explicitly ignored per the
        // observed shapes; anything else unrecognized is ignored too.
        _ => {}
    }
    events
}

fn parse_codex_line(
    value: &Value,
    skill_name: &str,
    state: &mut ParseState,
) -> Vec<SkillAgentEventKind> {
    let mut events = Vec::new();
    match value.get("type").and_then(Value::as_str).unwrap_or("") {
        "thread.started" => {
            if let Some(thread_id) = value.get("thread_id").and_then(Value::as_str) {
                state.session_id = Some(thread_id.to_string());
            }
        }
        "item.completed" => {
            if let Some(item) = value.get("item") {
                match item.get("type").and_then(Value::as_str).unwrap_or("") {
                    "agent_message" => {
                        if let Some(text) = item.get("text").and_then(Value::as_str) {
                            state.last_agent_message = Some(text.to_string());
                            events.push(SkillAgentEventKind::AssistantText {
                                text: text.to_string(),
                                is_delta: false,
                            });
                        }
                    }
                    "command_execution" => {
                        let command = item
                            .get("command")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        if command.contains(&format!("/{skill_name}/SKILL.md")) {
                            state.skill_loaded = SkillLoaded::Yes;
                        }
                        let detail = item
                            .get("aggregated_output")
                            .and_then(Value::as_str)
                            .map(str::to_string);
                        events.push(SkillAgentEventKind::ToolCall {
                            name: "command_execution".to_string(),
                            summary: command,
                            detail,
                        });
                    }
                    "file_change" => {
                        events.push(SkillAgentEventKind::ToolCall {
                            name: "file_change".to_string(),
                            summary: "file change".to_string(),
                            detail: Some(item.to_string()),
                        });
                    }
                    "error" => {
                        let message = item
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("Unknown error")
                            .to_string();
                        events.push(SkillAgentEventKind::Error { message });
                    }
                    // "reasoning" and anything else observed here are ignored.
                    _ => {}
                }
            }
        }
        // "turn.started"/"turn.completed"/"turn.failed"/"item.started": the
        // runner derives Finished from process exit plus accumulated state
        // instead, so these carry nothing new to surface here.
        _ => {}
    }
    events
}

fn parse_pi_line(
    value: &Value,
    skill_name: &str,
    state: &mut ParseState,
) -> Vec<SkillAgentEventKind> {
    let mut events = Vec::new();
    match value.get("type").and_then(Value::as_str).unwrap_or("") {
        "session" => {
            if let Some(id) = value.get("id").and_then(Value::as_str) {
                state.session_id = Some(id.to_string());
            }
        }
        "message_update" => {
            let is_text_delta = value
                .pointer("/assistantMessageEvent/type")
                .and_then(Value::as_str)
                == Some("text_delta");
            if is_text_delta {
                if let Some(delta) = value
                    .pointer("/assistantMessageEvent/delta")
                    .and_then(Value::as_str)
                {
                    state.last_text.push_str(delta);
                    events.push(SkillAgentEventKind::AssistantText {
                        text: delta.to_string(),
                        is_delta: true,
                    });
                }
            }
        }
        "tool_execution_start" => {
            let tool_name = value
                .get("toolName")
                .and_then(Value::as_str)
                .unwrap_or("tool")
                .to_string();
            let args = value.get("args").cloned().unwrap_or(Value::Null);
            if tool_name == "read" {
                let path = args.get("path").and_then(Value::as_str).unwrap_or("");
                let needle = format!("/{skill_name}/SKILL.md");
                if path.ends_with(&needle) || path.contains(&needle) {
                    state.skill_loaded = SkillLoaded::Yes;
                }
            }
            events.push(SkillAgentEventKind::ToolCall {
                name: tool_name,
                summary: args.to_string(),
                detail: None,
            });
        }
        "tool_execution_end" => {
            let tool_name = value
                .get("toolName")
                .and_then(Value::as_str)
                .unwrap_or("tool")
                .to_string();
            let result = value.get("result").cloned().unwrap_or(Value::Null);
            events.push(SkillAgentEventKind::ToolResult {
                name: tool_name,
                summary: result.to_string(),
            });
        }
        "turn_end" => {
            if let Some(blocks) = value.pointer("/message/content").and_then(Value::as_array) {
                let text: String = blocks
                    .iter()
                    .filter_map(|block| block.get("text").and_then(Value::as_str))
                    .collect();
                if !text.is_empty() {
                    // Not re-emitted as an event: the same text already streamed
                    // as `text_delta`s above, so this only updates `last_text`
                    // for `Finished`'s `final_text`.
                    state.last_text = text;
                }
            }
        }
        // "agent_end"/"agent_settled": the runner derives Finished from
        // process exit plus accumulated state instead.
        _ => {}
    }
    events
}

/// Parses one `opencode2 run --format json` record. Top-level `sessionID` is
/// captured silently the first time it's seen, same as the other three
/// adapters do for their own session/thread ids - none of them turn it into
/// its own event either. `type == "tool"` parts are deduped on `callID`/`id`
/// into two states (announced/resolved, see `open_code_tool_events`) so the
/// CLI's pending/running/completed/error re-prints of the same part yield at
/// most one `ToolCall` and one `ToolResult`/`Error`. Tool parts were never
/// observed in the field; this follows the `OpenCode` SDK's documented part
/// shape as best effort.
fn parse_open_code_line(
    value: &Value,
    skill_name: &str,
    state: &mut ParseState,
) -> Vec<SkillAgentEventKind> {
    let mut events = Vec::new();

    if state.session_id.is_none() {
        if let Some(session_id) = value.get("sessionID").and_then(Value::as_str) {
            state.session_id = Some(session_id.to_string());
        }
    }

    let part = value.get("part");
    let part_type = part
        .and_then(|p| p.get("type"))
        .and_then(Value::as_str)
        .unwrap_or("");

    match value.get("type").and_then(Value::as_str).unwrap_or("") {
        "text" => {
            if let Some(text) = part.and_then(|p| p.get("text")).and_then(Value::as_str) {
                state.last_text = text.to_string();
                events.push(SkillAgentEventKind::AssistantText {
                    text: text.to_string(),
                    is_delta: false,
                });
            }
        }
        "step_finish" => {
            let part_id = part.and_then(|p| p.get("id")).and_then(Value::as_str);
            let already_seen = match part_id {
                Some(id) => !state.seen_step_finish_ids.insert(id.to_string()),
                None => false,
            };
            if !already_seen {
                if let Some(cost) = part.and_then(|p| p.get("cost")).and_then(Value::as_f64) {
                    state.cost_usd = Some(state.cost_usd.unwrap_or(0.0) + cost);
                }
            }
        }
        _ if part_type == "tool" => {
            if let Some(part) = part {
                events.extend(open_code_tool_events(part, skill_name, state));
            }
        }
        // "step_start" carries nothing new beyond the session id already
        // captured above; anything else unrecognized is ignored.
        _ => {}
    }
    events
}

/// Builds the events for one `OpenCode` tool part, deduped by `callID`/`id`
/// into two independent states per call: "announced" (a `ToolCall` has been
/// emitted) and "resolved" (a `ToolResult` or `Error` has been emitted). The
/// CLI reprints the same call across its pending/running/completed/error
/// states, so a call with no `callID`/`id` at all is treated as always-new
/// (announced and resolved every time it's seen) rather than silently
/// dropped. `state.skill_loaded` is updated on every sighting, not only the
/// first, because `input` (the field it's read from) can arrive late - on
/// the `completed` record rather than the first `pending` one.
fn open_code_tool_events(
    part: &Value,
    skill_name: &str,
    state: &mut ParseState,
) -> Vec<SkillAgentEventKind> {
    let mut events = Vec::new();
    let Some(tool) = part.get("tool").and_then(Value::as_str).map(str::to_string) else {
        return events;
    };
    let tool_state = part.get("state");
    let input = tool_state
        .and_then(|s| s.get("input"))
        .cloned()
        .unwrap_or(Value::Null);
    let status = tool_state
        .and_then(|s| s.get("status"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let call_id = part
        .get("callID")
        .or_else(|| part.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string);

    match tool.as_str() {
        "skill" if input.get("name").and_then(Value::as_str) == Some(skill_name) => {
            state.skill_loaded = SkillLoaded::Yes;
        }
        "read" => {
            let needle = format!("/{skill_name}/SKILL.md");
            if input
                .get("filePath")
                .and_then(Value::as_str)
                .is_some_and(|path| path.ends_with(&needle))
            {
                state.skill_loaded = SkillLoaded::Yes;
            }
        }
        _ => {}
    }

    let not_yet_announced = match &call_id {
        Some(id) => state.announced_open_code_tool_ids.insert(id.clone()),
        None => true,
    };
    if not_yet_announced {
        let summary = tool_state
            .and_then(|s| s.get("title"))
            .and_then(Value::as_str)
            .map_or_else(
                || input.to_string().chars().take(120).collect(),
                str::to_string,
            );
        events.push(SkillAgentEventKind::ToolCall {
            name: tool.clone(),
            summary,
            detail: None,
        });
    }

    let not_yet_resolved = match &call_id {
        Some(id) => !state.resolved_open_code_tool_ids.contains(id),
        None => true,
    };
    if not_yet_resolved && (status == "completed" || status == "error") {
        if let Some(id) = &call_id {
            state.resolved_open_code_tool_ids.insert(id.clone());
        }
        if status == "completed" {
            let summary = tool_state
                .and_then(|s| s.get("output"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| {
                    tool_state
                        .and_then(|s| s.get("title"))
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .unwrap_or_default();
            events.push(SkillAgentEventKind::ToolResult {
                name: tool,
                summary,
            });
        } else {
            let message = tool_state
                .and_then(|s| s.get("error"))
                .and_then(Value::as_str)
                .map_or_else(|| "Unknown error".to_string(), str::to_string);
            events.push(SkillAgentEventKind::Error { message });
        }
    }

    events
}
