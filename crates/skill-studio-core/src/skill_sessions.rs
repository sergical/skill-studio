//! Skill sessions: for each recent coding session, which skills the agent
//! listed for the model with a description, with a name only, or not at
//! all, which ones the session used, and which ones it may have missed.
//! Per skill, the sessions roll up into counts and flags, next to the
//! current listing prediction and the reason a skill is out of reach.
//!
//! Claude Code records each session's listing as a `skill_listing`
//! attachment; Codex records it in the `<skills_instructions>` developer
//! message. Other formats currently contribute uses only. See
//! docs/agent-skill-conventions.md "Skill sessions".
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dto::Inventory;
use crate::error::CoreError;
use crate::identity::{AgentId, RootKind, RootScope};
use crate::listing_visibility::{
    claude_project_slug, listing_rows, listing_scopes, parse_listing, HarnessListing,
    ListingInputs, ListingRequest, ListingState,
};
use crate::ops::Operation;
use crate::ports::{FileKind, OpContext, Runtime};
use crate::skill_uses::{
    contains_marker, history_start, parse_claude_code_uses, parse_codex_uses, SkillInvocation,
    SkillTrigger, TranscriptContext,
};
use crate::timing::{op_timing, step};

/// Sessions read per agent when the request does not say.
pub const DEFAULT_SESSIONS: usize = 100;
/// Most sessions read per agent.
pub const MAX_SESSIONS: usize = 500;
/// Bytes read from one transcript. Uses after this point are not counted.
const MAX_TRANSCRIPT_BYTES: u64 = 512 * 1024 * 1024;
/// A longer line is skipped: it is a tool result or an image, not a
/// listing, a prompt or a skill call.
const MAX_LINE_BYTES: usize = 8 * 1024 * 1024;
/// Characters of the first prompt kept in the report.
const PROMPT_PREVIEW_CHARS: usize = 200;
/// The miss guess reads only short first prompts: a long prompt (a pasted
/// brief, a list of skills) matches many descriptions a little.
const MISS_MAX_PROMPT_CHARS: usize = 400;
/// Lowest description match score that counts as a possible miss. Chosen on
/// labeled eval sessions; see the module doc of [`MissIndex`].
const MISS_MIN_SCORE: f64 = 10.0;
/// A skill listed in at least this many sessions, and in at least 80% of
/// the sessions with a listing, with no use, is "listed, never used".
const NEVER_USED_MIN_SESSIONS: u32 = 3;

/// Input for `skill_sessions`.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SkillSessionsRequest {
    /// Only sessions recorded in this project, and its skills. It must be in
    /// the runtime scope.
    #[schemars(extend("x-untrusted" = true))]
    pub project_path: Option<PathBuf>,
    /// Newest sessions to read per agent, at most 500. Default 100.
    pub sessions: Option<usize>,
}

/// How one session showed one skill to the model.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SessionListingState {
    /// Name and full description.
    Described,
    /// Name and a shortened description.
    Truncated,
    /// Name only. The model guesses from the name.
    NameOnly,
    /// Not in the listing. The model can find it only by path.
    NotListed,
    /// The session recorded no listing.
    Unknown,
}

/// Evidence Skill Studio can read from this agent’s sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionRecords {
    /// The reader recognizes skill listings and skill uses.
    ListingAndUses,
    /// The reader recognizes skill uses. The Activity page counts them.
    UsesOnly,
}

/// One skill a session used.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SessionSkillUse {
    /// Skill name, as the listing shows it when the listing has it.
    #[schemars(extend("x-untrusted" = true))]
    pub skill: String,
    /// How this session listed the skill.
    pub listed_as: SessionListingState,
    /// How the first use started.
    pub trigger: SkillTrigger,
}

/// One entry in a session's listing.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SessionListedSkill {
    /// Index into this agent's captured descriptions when the session contained wording.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_index: Option<usize>,
    /// Name as the session recorded it.
    #[schemars(extend("x-untrusted" = true))]
    pub skill: String,
    /// How the session listed it.
    pub listed_as: SessionListingState,
}

/// A skill the session may have needed but did not use. A guess.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MissedSkill {
    /// Skill name.
    #[schemars(extend("x-untrusted" = true))]
    pub skill: String,
    /// How this session listed the skill.
    pub listed_as: SessionListingState,
    /// Match score of the first prompt against the description (BM25).
    pub score: f64,
    /// Prompt words, stemmed, that matched the description, best first.
    #[schemars(extend("x-untrusted" = true))]
    pub words: Vec<String>,
}

/// One recorded session.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SessionSummary {
    /// Agent id.
    pub harness: AgentId,
    /// The agent's session id.
    #[schemars(extend("x-untrusted" = true))]
    pub id: Option<String>,
    /// Transcript file.
    #[schemars(extend("x-untrusted" = true))]
    pub transcript: PathBuf,
    /// Working folder.
    #[schemars(extend("x-untrusted" = true))]
    pub project_path: Option<String>,
    /// Start time, as recorded.
    #[schemars(extend("x-untrusted" = true))]
    pub at: Option<String>,
    /// Model id.
    #[schemars(extend("x-untrusted" = true))]
    pub model: Option<String>,
    /// Start of the first user prompt.
    #[schemars(extend("x-untrusted" = true))]
    pub prompt: Option<String>,
    /// False when the session recorded no listing.
    pub listing_recorded: bool,
    /// Entries in the listing.
    pub listed: usize,
    /// Entries with a full or shortened description.
    pub described: usize,
    /// Entries with a name only.
    pub name_only: usize,
    /// Every listing entry, in listing order, as recorded.
    pub entries: Vec<SessionListedSkill>,
    /// Skills used, one row per skill.
    pub used: Vec<SessionSkillUse>,
    /// Possible misses. A guess, not a fact.
    pub missed: Vec<MissedSkill>,
}

/// A finding about one skill across sessions.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SkillSessionFlag {
    /// Listed in nearly every session, never used: it costs listing space.
    ListedNeverUsed,
    /// Used in a session that listed it by name only.
    UsedWhileNameOnly,
    /// A session may have needed it and did not use it (a guess).
    PossiblyMissed,
    /// The current listing shows it by name only, leaves it out, or hides it.
    NotDescribedNow,
    /// Installed, but not in a folder this agent reads.
    NotReachable,
}

/// One skill across the sessions read.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SkillSessionRollup {
    /// Agent id.
    pub harness: AgentId,
    /// Name as the agent lists it.
    #[schemars(extend("x-untrusted" = true))]
    pub name: String,
    /// Installed folder, when Skill Studio scans it.
    #[schemars(extend("x-untrusted" = true))]
    pub path: Option<PathBuf>,
    /// Sessions that listed it with its full description.
    pub described: u32,
    /// Sessions that listed it with a shortened description.
    pub truncated: u32,
    /// Sessions that listed it by name only.
    pub name_only: u32,
    /// Sessions with a listing that did not list it.
    pub not_listed: u32,
    /// Sessions that used it.
    pub used: u32,
    /// Sessions that used it while it was listed by name only.
    pub used_name_only: u32,
    /// Sessions that used it while it was not listed.
    pub used_not_listed: u32,
    /// Sessions that may have missed it (a guess).
    pub missed: u32,
    /// The current listing prediction. Null when the agent does not read it
    /// or Skill Studio does not scan it.
    pub now: Option<ListingState>,
    /// Why it is not described now, in plain words.
    #[schemars(extend("x-untrusted" = true))]
    pub reach: Option<String>,
    /// Findings, most important first.
    pub flags: Vec<SkillSessionFlag>,
}

/// One agent's sessions and per-skill rollup.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentSessions {
    /// Distinct description strings read from this agent's saved listings, in first-seen order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(extend("x-untrusted" = true))]
    pub captured_descriptions: Vec<String>,
    /// Agent id.
    pub harness: AgentId,
    /// Session evidence supported by this reader.
    pub records: SessionRecords,
    /// A description of reader coverage.
    pub note: String,
    /// The agent's skills documentation.
    pub doc: Option<String>,
    /// Sessions read.
    pub sessions_read: usize,
    /// Sessions that recorded a listing.
    pub sessions_with_listing: usize,
    /// Sessions that used a skill.
    pub sessions_with_use: usize,
    /// Sessions with a possible miss.
    pub sessions_with_miss: usize,
}

/// Every agent's sessions.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SkillSessionsReport {
    /// Skills with findings first, then by agent and name.
    pub skills: Vec<SkillSessionRollup>,
    /// Sessions, by agent, newest first.
    pub sessions: Vec<SessionSummary>,
    /// Agents in catalog order.
    pub agents: Vec<AgentSessions>,
    /// Days of history read.
    pub history_days: u32,
}

/// The agent's own skills documentation.
fn skills_doc(harness: &str) -> Option<&'static str> {
    Some(match harness {
        AgentId::CLAUDE_CODE => "https://code.claude.com/docs/en/skills",
        AgentId::CODEX => "https://developers.openai.com/codex/skills",
        AgentId::CURSOR => "https://cursor.com/docs/skills",
        AgentId::OPEN_CODE => "https://opencode.ai/docs/skills",
        AgentId::PI => "https://docs.pi.dev/latest/skills",
        _ => return None,
    })
}

/// Reads the newest sessions of each agent and joins each session's listing
/// with its uses. Reads only.
pub fn skill_sessions(
    rt: &Runtime,
    ctx: &OpContext,
    req: &SkillSessionsRequest,
) -> Result<SkillSessionsReport, CoreError> {
    rt.run(Operation::SkillSessions, ctx, || {
        let clock = rt.ports.clock.as_ref();
        let started = clock.monotonic();
        let limit = req
            .sessions
            .unwrap_or(DEFAULT_SESSIONS)
            .clamp(1, MAX_SESSIONS);
        let listing_req = ListingRequest {
            project_path: req.project_path.clone(),
            ..ListingRequest::default()
        };
        let scopes = listing_scopes(rt, &listing_req)?;
        let scope = scopes.last().cloned().unwrap_or(RootScope::Global);
        let _folder_guard = rt.ports.history.guard_read_data_folder()?;
        let inputs = ListingInputs::read(rt, ctx, std::slice::from_ref(&scope), &listing_req)?;
        let mut steps = ctx
            .take_timing()
            .map_or_else(Vec::new, |timing| timing.steps);
        let listing_started = clock.monotonic();
        let report = listing_rows(rt, ctx, &inputs, std::slice::from_ref(&scope), &listing_req)?;
        steps.push(step(clock, "listing_prediction", listing_started));
        let since = history_start(clock.now());
        let mut agents = Vec::new();
        let mut skills = Vec::new();
        let mut all_sessions = Vec::new();
        for facts in &rt.ports.catalog.facts {
            ctx.checkpoint()?;
            let row = report.listings.iter().find(|r| r.harness == facts.id);
            let corpus = corpus(rt, &inputs.inventory, facts, &scope, row)?;
            let transcripts_started = clock.monotonic();
            use crate::harness::{registered_harness, SessionListingReader};
            let (records, sessions) =
                match registered_harness(facts.id.as_str()).map(|row| row.session_reader) {
                    Some(SessionListingReader::Claude) => {
                        let sessions = read_claude_sessions(rt, ctx, &scope, since, limit)?;
                        steps.push(step(clock, "claude_transcripts", transcripts_started));
                        (SessionRecords::ListingAndUses, sessions)
                    }
                    Some(SessionListingReader::Codex) => {
                        let sessions = read_codex_sessions(rt, ctx, &scope, since, limit)?;
                        steps.push(step(clock, "codex_transcripts", transcripts_started));
                        (SessionRecords::ListingAndUses, sessions)
                    }
                    Some(SessionListingReader::UsesOnly) | None => {
                        (SessionRecords::UsesOnly, Vec::new())
                    }
                };
            let built = build_agent(
                &facts.id,
                &facts.display_name,
                records,
                row,
                &corpus,
                &sessions,
            );
            agents.push(built.agent);
            skills.extend(built.skills);
            all_sessions.extend(built.sessions);
        }
        // Findings first, so the first page of a paged answer holds them.
        skills.sort_by_key(|s| s.flags.first().copied().map_or(1, |_| 0));
        ctx.record_timing(op_timing(clock, "skill_sessions", started, steps));
        Ok(SkillSessionsReport {
            skills,
            sessions: all_sessions,
            agents,
            history_days: crate::skill_uses::HISTORY_DAYS,
        })
    })
}

// ---------------------------------------------------------------------------
// Transcripts
// ---------------------------------------------------------------------------

/// How one recorded listing showed one entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordedEntry {
    /// Verbatim wording from the record and its width in the client.
    Described {
        /// Literal description from the listing.
        text: String,
        /// Description width used to compare this entry with the installation.
        chars: u64,
    },
    /// By name only.
    NameOnly,
}

/// What one transcript records.
#[derive(Debug, Clone, Default)]
pub struct ParsedSession {
    /// Session id.
    pub id: Option<String>,
    /// Transcript file.
    pub transcript: PathBuf,
    /// Working folder.
    pub project_path: Option<String>,
    /// First recorded time.
    pub at: Option<String>,
    /// Model id.
    pub model: Option<String>,
    /// First user prompt.
    pub prompt: Option<String>,
    /// Listing entries in order; `None` when the session recorded none.
    pub listing: Option<Vec<(String, RecordedEntry)>>,
    /// Skill uses.
    pub uses: Vec<SkillInvocation>,
}

fn str_at<'a>(value: &'a serde_json::Value, path: &[&str]) -> Option<&'a str> {
    let mut v = value;
    for key in path {
        v = v.get(key)?;
    }
    v.as_str()
}

/// Text the user typed: not a command wrapper, a system reminder, or other
/// injected markup.
/// Keeps one character past the miss limit, so a longer prompt is still
/// known to be too long, and no more.
fn typed_prompt(text: &str) -> Option<String> {
    let trimmed = text.trim();
    (!trimmed.is_empty() && !trimmed.starts_with('<'))
        .then(|| trimmed.chars().take(MISS_MAX_PROMPT_CHARS + 1).collect())
}

/// Reads a Claude Code transcript one line at a time: the `skill_listing`
/// attachments (the initial one, plus later ones that add skills), the
/// first prompt, and the skill uses.
#[derive(Debug, Default)]
pub struct ClaudeSessionParser {
    session: ParsedSession,
    listing: Vec<(String, RecordedEntry)>,
    seen: BTreeSet<String>,
    recorded: bool,
}

impl ClaudeSessionParser {
    /// Reads one transcript line.
    pub fn line(&mut self, line: &str) {
        self.line_bytes(line.as_bytes());
    }

    fn line_bytes(&mut self, line: &[u8]) {
        use crate::skill_uses::claude_code::{COMMAND_MARKER, SKILL_MD_MARKER, SKILL_TOOL_MARKER};
        let possible_use = [SKILL_TOOL_MARKER, COMMAND_MARKER, SKILL_MD_MARKER]
            .iter()
            .any(|marker| contains_marker(line, marker));
        let session = &mut self.session;
        let wanted = (session.at.is_none() && contains_marker(line, "\"timestamp\""))
            || contains_marker(line, "\"skill_listing\"")
            || (session.prompt.is_none() && contains_marker(line, "\"user\""))
            || (session.model.is_none() && contains_marker(line, "\"model\""));
        if !possible_use && !wanted {
            return;
        }
        let line = String::from_utf8_lossy(line);
        if possible_use {
            session.uses.extend(parse_claude_code_uses(&line));
        }
        if !wanted {
            return;
        }
        let Ok(row) = serde_json::from_str::<serde_json::Value>(&line) else {
            return;
        };
        if session.at.is_none() {
            session.at = str_at(&row, &["timestamp"]).map(str::to_string);
        }
        if session.id.is_none() {
            session.id = str_at(&row, &["sessionId"]).map(str::to_string);
        }
        if session.project_path.is_none() {
            session.project_path = str_at(&row, &["cwd"]).map(str::to_string);
        }
        match str_at(&row, &["type"]) {
            Some("attachment")
                if str_at(&row, &["attachment", "type"]) == Some("skill_listing") =>
            {
                let attachment = &row["attachment"];
                let Some(content) = attachment.get("content").and_then(|c| c.as_str()) else {
                    return;
                };
                let names: Vec<String> = attachment
                    .get("names")
                    .and_then(|n| n.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|n| n.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                self.recorded = true;
                for (name, description, _) in parse_listing(content, &names) {
                    if self.seen.insert(name.clone()) {
                        let entry = description.filter(|text| !text.is_empty()).map_or(
                            RecordedEntry::NameOnly,
                            |text| RecordedEntry::Described {
                                chars: crate::listing_visibility::claude_width(&text),
                                text,
                            },
                        );
                        self.listing.push((name, entry));
                    }
                }
            }
            Some("assistant") if session.model.is_none() => {
                session.model = str_at(&row, &["message", "model"])
                    .filter(|m| m.starts_with("claude-"))
                    .map(str::to_string);
            }
            Some("user") if session.prompt.is_none() => {
                if row.get("isMeta").and_then(serde_json::Value::as_bool) == Some(true) {
                    return;
                }
                session.prompt = match &row["message"]["content"] {
                    serde_json::Value::String(text) => typed_prompt(text),
                    serde_json::Value::Array(blocks) => blocks
                        .iter()
                        .filter(|b| str_at(b, &["type"]) == Some("text"))
                        .find_map(|b| str_at(b, &["text"]).and_then(typed_prompt)),
                    _ => None,
                };
            }
            _ => {}
        }
    }

    /// The parsed session.
    pub fn finish(mut self) -> ParsedSession {
        self.session.listing = self.recorded.then_some(self.listing);
        self.session
    }
}

/// Parses one whole Claude Code transcript.
pub fn parse_claude_session(text: &str) -> ParsedSession {
    let mut parser = ClaudeSessionParser::default();
    text.lines().for_each(|line| parser.line(line));
    parser.finish()
}

/// The `### Available skills` entries of a Codex `<skills_instructions>`
/// block: `- name: description (file: path)`.
fn parse_codex_listing(block: &str) -> Vec<(String, RecordedEntry)> {
    let Some(start) = block.find("### Available skills") else {
        return Vec::new();
    };
    let end = block.find("</skills_instructions>").unwrap_or(block.len());
    let mut out = Vec::new();
    for line in block[start..end.max(start)].lines().skip(1) {
        if line.starts_with('#') {
            break;
        }
        let Some(rest) = line.strip_prefix("- ") else {
            continue;
        };
        let body = [
            " (file: ",
            " (executor package: ",
            " (cloud package: ",
            " (custom resource: ",
        ]
        .iter()
        .filter_map(|locator| rest.rfind(locator))
        .max()
        .filter(|_| rest.ends_with(')'))
        .map_or(rest, |at| &rest[..at]);
        let (name, entry) = match body.split_once(": ") {
            Some((name, description)) if !description.trim().is_empty() => (
                name,
                RecordedEntry::Described {
                    chars: description.chars().count() as u64,
                    text: description.to_owned(),
                },
            ),
            Some((name, _)) => (name, RecordedEntry::NameOnly),
            None => (body.trim_end_matches(':'), RecordedEntry::NameOnly),
        };
        let name = name.trim();
        if !name.is_empty() {
            out.push((name.to_string(), entry));
        }
    }
    out
}

/// Reads a Codex rollout one line at a time: the session header, the
/// model, the skill listing in the developer message, the first
/// `user_message`, and the uses.
#[derive(Debug, Default)]
pub struct CodexSessionParser {
    session: ParsedSession,
    context: TranscriptContext,
}

impl CodexSessionParser {
    /// Reads one rollout line.
    pub fn line(&mut self, line: &str) {
        self.line_bytes(line.as_bytes());
    }

    fn line_bytes(&mut self, line: &[u8]) {
        use crate::skill_uses::codex::{
            SESSION_META_MARKER, SKILLS_NAMESPACE_MARKER, SKILL_BLOCK_MARKER, SKILL_MD_MARKER,
        };
        let possible_use = [
            SESSION_META_MARKER,
            SKILL_BLOCK_MARKER,
            SKILL_MD_MARKER,
            SKILLS_NAMESPACE_MARKER,
        ]
        .iter()
        .any(|marker| contains_marker(line, marker));
        let session = &mut self.session;
        let wanted = (session.id.is_none() && contains_marker(line, "\"session_meta\""))
            || (session.model.is_none() && contains_marker(line, "\"turn_context\""))
            || (session.listing.is_none() && contains_marker(line, "<skills_instructions>"))
            || (session.prompt.is_none() && contains_marker(line, "\"user_message\""));
        if !possible_use && !wanted {
            return;
        }
        let line = String::from_utf8_lossy(line);
        if possible_use {
            session
                .uses
                .extend(parse_codex_uses(&line, &mut self.context));
        }
        if !wanted {
            return;
        }
        let Ok(row) = serde_json::from_str::<serde_json::Value>(&line) else {
            return;
        };
        let payload = &row["payload"];
        match str_at(&row, &["type"]) {
            Some("session_meta") => {
                session.id = str_at(payload, &["id"]).map(str::to_string);
                session.project_path = str_at(payload, &["cwd"]).map(str::to_string);
                session.at = str_at(payload, &["timestamp"])
                    .or_else(|| str_at(&row, &["timestamp"]))
                    .map(str::to_string);
            }
            Some("turn_context") => {
                session.model = str_at(payload, &["model"]).map(str::to_string);
            }
            Some("response_item")
                if session.listing.is_none()
                    && str_at(payload, &["type"]) == Some("message")
                    && str_at(payload, &["role"]) == Some("developer") =>
            {
                let block = payload["content"].as_array().and_then(|blocks| {
                    blocks.iter().find_map(|b| {
                        let text = str_at(b, &["text"])?;
                        let at = text.find("<skills_instructions>")?;
                        Some(&text[at..])
                    })
                });
                if let Some(block) = block {
                    session.listing = Some(parse_codex_listing(block));
                }
            }
            Some("event_msg") if str_at(payload, &["type"]) == Some("user_message") => {
                session.prompt = str_at(payload, &["message"]).and_then(typed_prompt);
            }
            _ => {}
        }
    }

    /// The parsed session.
    pub fn finish(self) -> ParsedSession {
        self.session
    }
}

/// Parses one whole Codex rollout.
pub fn parse_codex_session(text: &str) -> ParsedSession {
    let mut parser = CodexSessionParser::default();
    text.lines().for_each(|line| parser.line(line));
    parser.finish()
}

fn jsonl_files(rt: &Runtime, dir: &Path, out: &mut Vec<(DateTime<Utc>, PathBuf)>) {
    for entry in rt.ports.fs.read_dir(dir).unwrap_or_default() {
        if entry.kind != FileKind::File
            || !Path::new(&entry.name)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("jsonl"))
        {
            continue;
        }
        let path = dir.join(&entry.name);
        if let Some(modified) = rt
            .ports
            .fs
            .symlink_metadata(&path)
            .ok()
            .and_then(|f| f.modified)
        {
            out.push((modified, path));
        }
    }
}

fn subdirs(rt: &Runtime, dir: &Path) -> Vec<PathBuf> {
    rt.ports
        .fs
        .read_dir(dir)
        .unwrap_or_default()
        .into_iter()
        .filter(|e| e.kind == FileKind::Dir)
        .map(|e| dir.join(e.name))
        .collect()
}

/// The newest files changed since `since`, newest first.
pub(crate) fn newest(
    mut files: Vec<(DateTime<Utc>, PathBuf)>,
    since: DateTime<Utc>,
    limit: usize,
) -> Vec<PathBuf> {
    files.retain(|(at, _)| *at >= since);
    files.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    files.into_iter().take(limit).map(|(_, p)| p).collect()
}

/// Streams a transcript's lines to `each`, so a large transcript is not
/// held in memory. Returns false when the file cannot be opened.
fn read_lines(rt: &Runtime, path: &Path, mut each: impl FnMut(&[u8])) -> bool {
    use std::io::{BufRead, Read};
    let Ok(file) = rt.ports.fs.open_file(path) else {
        return false;
    };
    let mut reader = std::io::BufReader::new(file.reader.take(MAX_TRANSCRIPT_BYTES));
    let mut line = Vec::new();
    // True while draining a line that grew past MAX_LINE_BYTES.
    let mut skipping = false;
    loop {
        let chunk = match reader.fill_buf() {
            Ok([]) | Err(_) => break,
            Ok(chunk) => chunk,
        };
        let (part, ends) = match memchr::memchr(b'\n', chunk) {
            Some(at) => (&chunk[..at], at + 1),
            None => (chunk, chunk.len()),
        };
        let complete = ends > part.len();
        // Most lines fit in the reader's buffer. Borrow those directly and
        // only copy lines that span multiple reads.
        if complete && line.is_empty() && !skipping {
            each(part);
            reader.consume(ends);
            continue;
        }
        if !skipping {
            if line.len() + part.len() > MAX_LINE_BYTES {
                skipping = true;
                line.clear();
            } else {
                line.extend_from_slice(part);
            }
        }
        reader.consume(ends);
        if complete {
            if !skipping {
                each(&line);
            }
            line.clear();
            skipping = false;
        }
    }
    if !skipping && !line.is_empty() {
        each(&line);
    }
    true
}

/// Top-level Claude Code transcripts; subagent transcripts get no listing.
fn read_claude_sessions(
    rt: &Runtime,
    ctx: &OpContext,
    scope: &RootScope,
    since: DateTime<Utc>,
    limit: usize,
) -> Result<Vec<ParsedSession>, CoreError> {
    let projects = rt.scope.global_root_path(Path::new(".claude/projects"));
    let dirs = match scope {
        RootScope::Project(project) => vec![projects.join(claude_project_slug(&project.0))],
        RootScope::Global => subdirs(rt, &projects)
            .into_iter()
            .filter(|dir| {
                !dir.file_name()
                    .is_some_and(|n| temporary_claude_project(rt, &n.to_string_lossy()))
            })
            .collect(),
    };
    let mut files = Vec::new();
    for dir in dirs {
        jsonl_files(rt, &dir, &mut files);
    }
    let mut out = Vec::new();
    for path in newest(files, since, limit) {
        ctx.checkpoint()?;
        let mut parser = ClaudeSessionParser::default();
        if read_lines(rt, &path, |line| parser.line_bytes(line)) {
            let mut session = parser.finish();
            session.transcript = path;
            out.push(session);
        }
    }
    Ok(out)
}

/// Folders the system clears: one-off runs such as evals, not the owner's
/// coding sessions.
const TEMPORARY_DIRS: &[&str] = &[
    "/tmp/",
    "/private/tmp/",
    "/var/folders/",
    "/private/var/folders/",
];

/// True when a session ran in a temporary folder outside the home folder.
/// A disposable home (a test, a verify run) can itself be temporary.
pub(crate) fn temporary_dir(rt: &Runtime, path: &str) -> bool {
    TEMPORARY_DIRS.iter().any(|dir| path.starts_with(dir))
        && !Path::new(path).starts_with(&rt.scope.home.lexical)
}

/// True for a Claude Code project folder name of a temporary folder
/// outside the home folder.
pub(crate) fn temporary_claude_project(rt: &Runtime, slug: &str) -> bool {
    TEMPORARY_DIRS
        .iter()
        .any(|dir| slug.starts_with(&dir.replace('/', "-")))
        && !slug.starts_with(&claude_project_slug(&rt.scope.home.lexical))
}

/// Codex rollouts, `sessions/YYYY/MM/DD/*.jsonl` and
/// `archived_sessions/*.jsonl`, with their change times.
pub(crate) fn codex_rollouts(rt: &Runtime) -> Vec<(DateTime<Utc>, PathBuf)> {
    let home = &rt.scope.codex_home;
    let mut files = Vec::new();
    jsonl_files(rt, &home.join("archived_sessions"), &mut files);
    for year in subdirs(rt, &home.join("sessions")) {
        for month in subdirs(rt, &year) {
            for day in subdirs(rt, &month) {
                jsonl_files(rt, &day, &mut files);
            }
        }
    }
    files
}

/// Codex rollouts, newest first, without sessions in temporary folders.
fn read_codex_sessions(
    rt: &Runtime,
    ctx: &OpContext,
    scope: &RootScope,
    since: DateTime<Utc>,
    limit: usize,
) -> Result<Vec<ParsedSession>, CoreError> {
    let files = codex_rollouts(rt);
    let project = match scope {
        RootScope::Project(p) => Some(p.0.to_string_lossy().into_owned()),
        RootScope::Global => None,
    };
    // A project filter is known only after parsing, so read past `limit`
    // files until `limit` sessions match.
    let mut out = Vec::new();
    for path in newest(files, since, usize::MAX) {
        if out.len() >= limit {
            break;
        }
        ctx.checkpoint()?;
        let mut parser = CodexSessionParser::default();
        if !read_lines(rt, &path, |line| parser.line_bytes(line)) {
            continue;
        }
        let mut session = parser.finish();
        if (project.is_some() && session.project_path != project)
            || session
                .project_path
                .as_deref()
                .is_some_and(|p| temporary_dir(rt, p))
        {
            continue;
        }
        session.transcript = path;
        out.push(session);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Skills the agent could use
// ---------------------------------------------------------------------------

/// One skill the rollup reports and the miss guess can name.
#[derive(Debug, Clone)]
pub struct CorpusSkill {
    /// Name as the agent lists it.
    pub name: String,
    /// Installed folder.
    pub path: Option<PathBuf>,
    /// Full description, before any cut.
    pub description: String,
    /// False when the agent does not read any folder it is in.
    pub reachable: bool,
}

/// The skills in `row` with their descriptions, plus every installed skill
/// in scope that the agent does not read.
fn corpus(
    rt: &Runtime,
    inventory: &Inventory,
    facts: &crate::harness::HarnessFacts,
    scope: &RootScope,
    row: Option<&HarnessListing>,
) -> Result<Vec<CorpusSkill>, CoreError> {
    let by_path: BTreeMap<PathBuf, String> =
        crate::listing_cost::listed_skills(rt, inventory, facts, scope, None)?
            .into_values()
            .map(|s| (s.path, s.description))
            .collect();
    let mut out = Vec::new();
    let mut names = BTreeSet::new();
    for entry in row.map(|r| r.entries.as_slice()).unwrap_or_default() {
        if !names.insert(entry.name.clone()) {
            continue;
        }
        out.push(CorpusSkill {
            name: entry.name.clone(),
            path: entry.path.clone(),
            description: entry
                .path
                .as_ref()
                .and_then(|p| by_path.get(p))
                .cloned()
                .unwrap_or_default(),
            reachable: true,
        });
    }
    let base_names: BTreeSet<&str> = names
        .iter()
        .map(|n| n.rsplit(':').next().unwrap_or(n))
        .collect();
    let mut unreachable = BTreeSet::new();
    for skill in &inventory.skills {
        for d in &skill.deployments {
            let in_scope = d.root.scope == RootScope::Global || &d.root.scope == scope;
            if !in_scope || d.root.kind == RootKind::Parked || d.symlink_is_broken {
                continue;
            }
            let name = d
                .frontmatter
                .as_ref()
                .and_then(|f| f.name.clone())
                .unwrap_or_else(|| skill.name.0.clone());
            if base_names.contains(name.as_str()) || !unreachable.insert(name.clone()) {
                continue;
            }
            out.push(CorpusSkill {
                name,
                path: Some(d.path.clone()),
                description: d
                    .frontmatter
                    .as_ref()
                    .and_then(|f| f.description.clone())
                    .unwrap_or_default(),
                reachable: false,
            });
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Miss guess
// ---------------------------------------------------------------------------

const STOP_WORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "of", "to", "in", "on", "for", "with", "without", "is", "are",
    "be", "it", "its", "this", "that", "these", "those", "when", "use", "using", "used", "you",
    "your", "can", "by", "as", "at", "from", "into", "how", "what", "why", "not", "no", "do",
    "does", "make", "any", "all", "more", "most", "which", "who", "than", "then", "so", "if",
    "but",
];

fn stem(word: &str) -> String {
    for suffix in ["ing", "es", "s", "ed"] {
        if let Some(base) = word.strip_suffix(suffix) {
            if base.len() >= 4 {
                return base.to_string();
            }
        }
    }
    word.to_string()
}

fn terms(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| w.len() > 2 && !STOP_WORDS.contains(w))
        .map(stem)
        .collect()
}

/// BM25 over skill descriptions, queried with a session's first prompt.
///
/// Measured on 2026-11-20 against 194 labeled eval sessions
/// (`~/.agents/evals/ui-skills`): flagging the best match at score 10 or
/// more, in sessions with no skill use, gave precision 0.63 and caught 29
/// of 66 real misses. Phrase lists ("Triggers on:") reached about 0.15.
pub struct MissIndex {
    docs: Vec<(String, BTreeMap<String, u32>, usize)>,
    idf: BTreeMap<String, f64>,
    average_len: f64,
}

impl MissIndex {
    /// Builds the index from `(name, description)` pairs. Empty descriptions
    /// are left out.
    pub fn new<'a>(skills: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let mut docs = Vec::new();
        let mut df: BTreeMap<String, u32> = BTreeMap::new();
        for (name, description) in skills {
            let words = terms(description);
            if words.is_empty() {
                continue;
            }
            let mut tf: BTreeMap<String, u32> = BTreeMap::new();
            for w in &words {
                *tf.entry(w.clone()).or_default() += 1;
            }
            for w in tf.keys() {
                *df.entry(w.clone()).or_default() += 1;
            }
            docs.push((name.to_string(), tf, words.len()));
        }
        let n = docs.len() as f64;
        let idf = df
            .into_iter()
            .map(|(w, c)| {
                let c = f64::from(c);
                (w, ((n - c + 0.5) / (c + 0.5) + 1.0).ln())
            })
            .collect();
        let average_len = if docs.is_empty() {
            1.0
        } else {
            docs.iter().map(|d| d.2 as f64).sum::<f64>() / n
        };
        Self {
            docs,
            idf,
            average_len,
        }
    }

    /// The best match for `prompt`: name, score, matched words best first.
    pub fn best(&self, prompt: &str) -> Option<DescriptionMatch> {
        let query: BTreeSet<String> = terms(prompt).into_iter().collect();
        // Each matched query word's share of one description's score.
        let parts = |(_, tf, len): &(String, BTreeMap<String, u32>, usize)| -> Vec<(f64, String)> {
            query
                .iter()
                .filter_map(|w| {
                    let f = f64::from(*tf.get(w)?);
                    let idf = self.idf.get(w).copied().unwrap_or(0.0);
                    let norm = 0.25 + 0.75 * *len as f64 / self.average_len;
                    Some((idf * f * 2.2 / (f + 1.2 * norm), w.clone()))
                })
                .collect()
        };
        let mut best: Option<(f64, usize)> = None;
        for (i, doc) in self.docs.iter().enumerate() {
            let score: f64 = parts(doc).iter().map(|p| p.0).sum();
            if best.is_none_or(|b| score > b.0) {
                best = Some((score, i));
            }
        }
        let (score, i) = best?;
        let mut words = parts(&self.docs[i]);
        words.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        Some(DescriptionMatch {
            skill: self.docs[i].0.clone(),
            score: (score * 10.0).round() / 10.0,
            words: words.into_iter().take(5).map(|(_, w)| w).collect(),
        })
    }

    /// A possible miss: a short first prompt, no skill used, and a strong
    /// description match.
    pub fn guess(&self, prompt: Option<&str>, used_any: bool) -> Option<DescriptionMatch> {
        let prompt = prompt?;
        if used_any || prompt.chars().count() > MISS_MAX_PROMPT_CHARS {
            return None;
        }
        self.best(prompt).filter(|m| m.score >= MISS_MIN_SCORE)
    }
}

/// The skill whose description best matches a prompt.
#[derive(Debug, Clone, PartialEq)]
pub struct DescriptionMatch {
    /// Skill name.
    pub skill: String,
    /// BM25 score, one decimal.
    pub score: f64,
    /// Matched prompt words, stemmed, best first, at most five.
    pub words: Vec<String>,
}

// ---------------------------------------------------------------------------
// Rollup
// ---------------------------------------------------------------------------

/// The one candidate a recorded name refers to, or the name itself.
///
/// An exact match wins. Otherwise a candidate must have the same part after
/// the last `:`, at most one of the two may carry a `plugin:` prefix, and
/// only one candidate may fit. Codex records plugin skills as `plugin:name`
/// while its scanned listing names them `name`; a Claude Code file read
/// gives the folder name of a `plugin:name` entry. Two different plugins
/// never match each other.
fn resolve_name<'a>(name: &str, candidates: impl IntoIterator<Item = &'a str>) -> String {
    let base = name.rsplit(':').next().unwrap_or(name);
    let qualified = name.contains(':');
    let mut fits = Vec::new();
    for candidate in candidates {
        if candidate == name {
            return name.to_string();
        }
        if candidate.rsplit(':').next() == Some(base) && !(qualified && candidate.contains(':')) {
            fits.push(candidate);
        }
    }
    match fits.as_slice() {
        [only] => (*only).to_string(),
        _ => name.to_string(),
    }
}

/// The state of one recorded entry. A description shorter than the scanned
/// one was cut.
fn entry_state(entry: &RecordedEntry, scanned: Option<&CorpusSkill>) -> SessionListingState {
    let RecordedEntry::Described { chars, .. } = entry else {
        return SessionListingState::NameOnly;
    };
    let full = scanned.map_or(0, |s| {
        s.description
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .count() as u64
    });
    // Codex trims descriptions evenly; Claude Code lists them whole up to
    // 1536 characters, so only a large gap counts.
    if full > chars + 3 && *chars < 1536 {
        SessionListingState::Truncated
    } else {
        SessionListingState::Described
    }
}

fn reach_text(
    state: Option<ListingState>,
    hidden_reason: Option<&str>,
    reachable: bool,
    agent: &str,
) -> Option<String> {
    if !reachable {
        return Some(format!("Not in a folder {agent} reads."));
    }
    Some(match state? {
        ListingState::Described => return None,
        ListingState::Truncated => {
            "Listed with a shortened description: the listing budget is full.".into()
        }
        ListingState::NameOnly => {
            "Listed by name only: the listing budget is full, or the skill has no description."
                .into()
        }
        ListingState::Omitted => "Left out: the listing budget is full.".into(),
        ListingState::Hidden => format!(
            "Hidden: {}.",
            hidden_reason.unwrap_or("the agent does not list it")
        ),
    })
}

/// One agent's part of the report.
#[derive(Debug, Clone)]
pub struct AgentBuild {
    /// Totals and notes.
    pub agent: AgentSessions,
    /// Sessions, newest first.
    pub sessions: Vec<SessionSummary>,
    /// Skills, by name.
    pub skills: Vec<SkillSessionRollup>,
}

/// Joins each session's listing with its uses and rolls them up per skill.
pub fn build_agent(
    harness: &AgentId,
    display_name: &str,
    records: SessionRecords,
    row: Option<&HarnessListing>,
    corpus: &[CorpusSkill],
    parsed: &[ParsedSession],
) -> AgentBuild {
    let by_name: BTreeMap<&str, &CorpusSkill> =
        corpus.iter().map(|s| (s.name.as_str(), s)).collect();
    let index = MissIndex::new(
        corpus
            .iter()
            .map(|s| (s.name.as_str(), s.description.as_str())),
    );
    let mut rollup: BTreeMap<String, SkillSessionRollup> = BTreeMap::new();
    let blank = |name: &str| SkillSessionRollup {
        harness: harness.clone(),
        name: name.to_string(),
        path: by_name.get(name).and_then(|s| s.path.clone()),
        described: 0,
        truncated: 0,
        name_only: 0,
        not_listed: 0,
        used: 0,
        used_name_only: 0,
        used_not_listed: 0,
        missed: 0,
        now: None,
        reach: None,
        flags: Vec::new(),
    };
    for skill in corpus {
        rollup.insert(skill.name.clone(), blank(&skill.name));
    }
    let mut sessions = Vec::new();
    let mut with_listing = 0usize;
    let mut description_ids = BTreeMap::new();
    let mut captured_descriptions = Vec::new();
    for session in parsed {
        // Recorded names stay as recorded: two plugins' `p:foo` and `q:foo`
        // are different entries even when both map to one scanned `foo`.
        let recorded: Option<BTreeMap<&str, &RecordedEntry>> =
            session.listing.as_ref().map(|listing| {
                let mut first_by_name = BTreeMap::new();
                for (name, entry) in listing {
                    first_by_name.entry(name.as_str()).or_insert(entry);
                }
                first_by_name
            });
        let scanned = |name: &str| resolve_name(name, by_name.keys().copied());
        // How this session listed a skill, by a recorded or a scanned name.
        let state_of = |name: &str| -> (String, SessionListingState) {
            let Some(recorded) = &recorded else {
                return (name.to_string(), SessionListingState::Unknown);
            };
            let listed = resolve_name(name, recorded.keys().copied());
            match recorded.get(listed.as_str()) {
                Some(entry) => {
                    let state = entry_state(entry, by_name.get(scanned(&listed).as_str()).copied());
                    (listed, state)
                }
                None => (name.to_string(), SessionListingState::NotListed),
            }
        };
        let mut summary = SessionSummary {
            harness: harness.clone(),
            id: session.id.clone(),
            transcript: session.transcript.clone(),
            project_path: session.project_path.clone(),
            at: session.at.clone(),
            model: session.model.clone(),
            prompt: session
                .prompt
                .as_ref()
                .map(|p| p.chars().take(PROMPT_PREVIEW_CHARS).collect()),
            listing_recorded: recorded.is_some(),
            listed: 0,
            described: 0,
            name_only: 0,
            entries: Vec::new(),
            used: Vec::new(),
            missed: Vec::new(),
        };
        if session.listing.is_some() {
            with_listing += 1;
        }
        // One count per scanned skill per session, even when two recorded
        // names map to it.
        let mut counted = BTreeSet::new();
        for (name, entry) in session.listing.iter().flatten() {
            let state = entry_state(entry, by_name.get(scanned(name).as_str()).copied());
            summary.listed += 1;
            if state == SessionListingState::NameOnly {
                summary.name_only += 1;
            } else {
                summary.described += 1;
            }
            let description_index = match entry {
                RecordedEntry::NameOnly => None,
                RecordedEntry::Described { text, .. } => {
                    Some(*description_ids.entry(text.as_str()).or_insert_with(|| {
                        captured_descriptions.push(text.clone());
                        captured_descriptions.len() - 1
                    }))
                }
            };
            summary.entries.push(SessionListedSkill {
                description_index,
                skill: name.clone(),
                listed_as: state,
            });
            let key = scanned(name);
            if !counted.insert(key.clone()) {
                continue;
            }
            let row = rollup.entry(key.clone()).or_insert_with(|| blank(&key));
            match state {
                SessionListingState::NameOnly => row.name_only += 1,
                SessionListingState::Truncated => row.truncated += 1,
                _ => row.described += 1,
            }
        }
        let mut used_keys = BTreeSet::new();
        for invocation in &session.uses {
            let (name, state) = state_of(&invocation.skill);
            if summary.used.iter().any(|u| u.skill == name) {
                continue;
            }
            // Each count at most once per scanned skill per session, each
            // on its own: a later use of another plugin's copy can still be
            // the one that was name only.
            let key = scanned(&name);
            let row = rollup.entry(key.clone()).or_insert_with(|| blank(&key));
            if used_keys.insert((key.clone(), None)) {
                row.used += 1;
            }
            if matches!(
                state,
                SessionListingState::NameOnly | SessionListingState::NotListed
            ) && used_keys.insert((key, Some(state)))
            {
                if state == SessionListingState::NameOnly {
                    row.used_name_only += 1;
                } else {
                    row.used_not_listed += 1;
                }
            }
            summary.used.push(SessionSkillUse {
                skill: name,
                listed_as: state,
                trigger: invocation.trigger,
            });
        }
        if let Some(found) = index.guess(session.prompt.as_deref(), !summary.used.is_empty()) {
            if let Some(row) = rollup.get_mut(&found.skill) {
                row.missed += 1;
            }
            summary.missed.push(MissedSkill {
                listed_as: state_of(&found.skill).1,
                skill: found.skill,
                score: found.score,
                words: found.words,
            });
        }
        sessions.push(summary);
    }
    // Prefer the first visible copy when several deployments have the same name.
    let mut entries = BTreeMap::new();
    for entry in row.into_iter().flat_map(|row| &row.entries) {
        let best = entries.entry(entry.name.as_str()).or_insert(entry);
        if best.state == ListingState::Hidden && entry.state != ListingState::Hidden {
            *best = entry;
        }
    }
    let preview =
        row.is_some_and(|row| row.evidence == crate::listing_visibility::ListingEvidence::Preview);
    for skill in rollup.values_mut() {
        let entry = entries.get(skill.name.as_str());
        let reachable = by_name.get(skill.name.as_str()).is_none_or(|s| s.reachable);
        skill.now = entry.filter(|_| preview).map(|e| e.state);
        skill.reach = reach_text(
            entry
                .map(|e| e.state)
                .filter(|state| preview || *state == ListingState::Hidden),
            entry.and_then(|e| e.hidden_reason.as_deref()),
            reachable,
            display_name,
        );
        let listed = skill.described + skill.truncated + skill.name_only;
        // Every session with a listing that did not list it, including
        // sessions from before it was installed or after it was removed.
        skill.not_listed = u32::try_from(with_listing)
            .unwrap_or(u32::MAX)
            .saturating_sub(listed);
        if skill.used == 0
            && listed >= NEVER_USED_MIN_SESSIONS
            && u64::from(listed) * 5 >= with_listing as u64 * 4
        {
            skill.flags.push(SkillSessionFlag::ListedNeverUsed);
        }
        if skill.used_name_only > 0 {
            skill.flags.push(SkillSessionFlag::UsedWhileNameOnly);
        }
        if skill.missed > 0 {
            skill.flags.push(SkillSessionFlag::PossiblyMissed);
        }
        if !reachable {
            skill.flags.push(SkillSessionFlag::NotReachable);
        } else if skill.now.is_some_and(|s| s != ListingState::Described) {
            skill.flags.push(SkillSessionFlag::NotDescribedNow);
        }
    }
    let note = match records {
        SessionRecords::ListingAndUses => format!(
            "Skill Studio reads listings and skill uses when present in {display_name} sessions from the last {} days.",
            crate::skill_uses::HISTORY_DAYS
        ),
        SessionRecords::UsesOnly => format!(
            "Skill Studio does not yet read saved listings for {display_name}."
        ),
    };
    AgentBuild {
        agent: AgentSessions {
            captured_descriptions,
            harness: harness.clone(),
            records,
            note,
            doc: skills_doc(harness.as_str()).map(str::to_string),
            sessions_read: sessions.len(),
            sessions_with_listing: with_listing,
            sessions_with_use: sessions.iter().filter(|s| !s.used.is_empty()).count(),
            sessions_with_miss: sessions.iter().filter(|s| !s.missed.is_empty()).count(),
        },
        sessions,
        skills: rollup.into_values().collect(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn claude_session_joins_listing_states_with_uses_and_reads_the_typed_prompt() {
        let text = [
            serde_json::json!({"type":"user","isMeta":true,"timestamp":"2026-11-20T00:00:00Z","sessionId":"s1","cwd":"/w","message":{"content":"<system-reminder>x</system-reminder>"}}).to_string(),
            serde_json::json!({"type":"attachment","timestamp":"2026-11-20T00:00:01Z","attachment":{"type":"skill_listing","isInitial":true,"names":["a","b","p:c"],"content":"- a: does a\n- b\n- p:c: plugin c"}}).to_string(),
            serde_json::json!({"type":"user","timestamp":"2026-11-20T00:00:02Z","message":{"content":"make the tooltips faster"}}).to_string(),
            serde_json::json!({"type":"assistant","timestamp":"2026-11-20T00:00:03Z","sessionId":"s1","cwd":"/w","message":{"model":"claude-opus-5-5","content":[{"type":"tool_use","id":"t1","name":"Skill","input":{"skill":"b"}},{"type":"tool_use","id":"t2","name":"Read","input":{"file_path":"/h/.claude/skills/c/SKILL.md"}}]}}).to_string(),
        ]
        .join("\n");
        let session = parse_claude_session(&text);
        assert_eq!(session.id.as_deref(), Some("s1"));
        assert_eq!(session.prompt.as_deref(), Some("make the tooltips faster"));
        assert_eq!(session.model.as_deref(), Some("claude-opus-5-5"));
        let listing = session.listing.clone().unwrap();
        assert_eq!(listing[1], ("b".to_string(), RecordedEntry::NameOnly));
        assert!(matches!(
            listing[0].1,
            RecordedEntry::Described { chars: 6, .. }
        ));

        let agent = build_agent(
            &AgentId::parse(AgentId::CLAUDE_CODE).unwrap(),
            "Claude Code",
            SessionRecords::ListingAndUses,
            None,
            &[],
            &[session],
        );
        let s = &agent.sessions[0];
        assert_eq!((s.listed, s.described, s.name_only), (3, 2, 1));
        assert_eq!(agent.agent.captured_descriptions, ["does a", "plugin c"]);
        assert_eq!(s.entries[0].description_index, Some(0));
        assert_eq!(s.entries[1].description_index, None);
        let used: Vec<_> = s
            .used
            .iter()
            .map(|u| (u.skill.as_str(), u.listed_as))
            .collect();
        assert_eq!(
            used,
            [
                ("b", SessionListingState::NameOnly),
                ("p:c", SessionListingState::Described)
            ]
        );
        let b = agent.skills.iter().find(|r| r.name == "b").unwrap();
        assert_eq!(b.flags, [SkillSessionFlag::UsedWhileNameOnly]);
    }

    #[test]
    fn codex_session_reads_the_listing_block_and_the_typed_prompt() {
        let dev = "<skills_instructions>\n## Skills\n### Available skills\n- a: Does a thing (file: r1/a/SKILL.md)\n- p:b: Plugin b (file: r4/b/SKILL.md)\n- a: (file: r2/a/SKILL.md)\n### Instructions\n- This is not a skill\n</skills_instructions>";
        let text = [
            serde_json::json!({"type":"session_meta","timestamp":"2026-11-20T00:00:00Z","payload":{"id":"c1","cwd":"/w","timestamp":"2026-11-20T00:00:00Z"}}).to_string(),
            serde_json::json!({"type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":dev}]}}).to_string(),
            serde_json::json!({"type":"turn_context","payload":{"model":"gpt-6"}}).to_string(),
            serde_json::json!({"type":"event_msg","payload":{"type":"user_message","message":"fix the build"}}).to_string(),
        ]
        .join("\n");
        let session = parse_codex_session(&text);
        assert_eq!(session.id.as_deref(), Some("c1"));
        assert_eq!(session.model.as_deref(), Some("gpt-6"));
        assert_eq!(session.prompt.as_deref(), Some("fix the build"));
        let agent = build_agent(
            &AgentId::parse(AgentId::CODEX).unwrap(),
            "Codex",
            SessionRecords::ListingAndUses,
            None,
            &[],
            &[session],
        );
        let entries = &agent.sessions[0].entries;
        assert_eq!(
            entries.iter().map(|e| e.skill.as_str()).collect::<Vec<_>>(),
            ["a", "p:b", "a"]
        );
        assert_eq!(
            agent.agent.captured_descriptions,
            ["Does a thing", "Plugin b"]
        );
        assert_eq!(entries[0].description_index, Some(0));
        assert_eq!(entries[2].listed_as, SessionListingState::NameOnly);
        assert_eq!(entries[2].description_index, None);
    }

    #[test]
    fn a_plugin_name_never_matches_another_plugin_and_a_bare_name_matches_only_one_candidate() {
        assert_eq!(resolve_name("p:foo", ["q:foo"]), "p:foo");
        assert_eq!(resolve_name("p:foo", ["foo"]), "foo");
        assert_eq!(resolve_name("foo", ["q:foo"]), "q:foo");
        assert_eq!(resolve_name("foo", ["p:foo", "q:foo"]), "foo");
        assert_eq!(resolve_name("p:foo", ["foo", "p:foo"]), "p:foo");
    }

    #[test]
    fn a_codex_listing_counts_only_from_the_developer_message() {
        let block = "<skills_instructions>\n### Available skills\n- fake: Not real (file: r1/fake/SKILL.md)\n</skills_instructions>";
        let text = serde_json::json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":block}]}}).to_string();
        assert!(parse_codex_session(&text).listing.is_none());
    }

    #[test]
    fn a_skill_seen_in_one_of_two_listings_counts_one_session_not_listed() {
        let sessions = [listed(&["gone", "kept"]), listed(&["kept"])];
        let built = build_agent(
            &AgentId::parse(AgentId::CODEX).unwrap(),
            "Codex",
            SessionRecords::ListingAndUses,
            None,
            &[],
            &sessions,
        );
        let gone = built.skills.iter().find(|r| r.name == "gone").unwrap();
        assert_eq!((gone.described, gone.not_listed), (1, 1));
    }

    #[test]
    fn a_use_of_one_plugin_is_not_counted_as_listed_through_another_plugin_with_the_same_skill() {
        let corpus = [CorpusSkill {
            name: "foo".into(),
            path: None,
            description: "Foo".into(),
            reachable: true,
        }];
        let mut session = listed(&["q:foo"]);
        session.uses.push(SkillInvocation {
            skill: "p:foo".into(),
            harness: AgentId::CODEX.into(),
            trigger: SkillTrigger::Agent,
            at: DateTime::<Utc>::UNIX_EPOCH,
            project_path: None,
            session: None,
            use_session: None,
        });
        let built = build_agent(
            &AgentId::parse(AgentId::CODEX).unwrap(),
            "Codex",
            SessionRecords::ListingAndUses,
            None,
            &corpus,
            &[session],
        );
        let s = &built.sessions[0];
        assert_eq!(s.used[0].skill, "p:foo");
        assert_eq!(s.used[0].listed_as, SessionListingState::NotListed);
        assert_eq!(s.entries[0].skill, "q:foo");
        let foo = built.skills.iter().find(|r| r.name == "foo").unwrap();
        assert_eq!((foo.described, foo.used, foo.used_not_listed), (1, 1, 1));
    }

    #[test]
    fn a_name_only_use_counts_whichever_order_two_plugin_copies_are_used_in() {
        let corpus = [CorpusSkill {
            name: "foo".into(),
            path: None,
            description: "Foo".into(),
            reachable: true,
        }];
        let use_of = |skill: &str| SkillInvocation {
            skill: skill.into(),
            harness: AgentId::CODEX.into(),
            trigger: SkillTrigger::Agent,
            at: DateTime::<Utc>::UNIX_EPOCH,
            project_path: None,
            session: None,
            use_session: None,
        };
        for order in [["p:foo", "q:foo"], ["q:foo", "p:foo"]] {
            let mut session = listed(&["p:foo"]);
            if let Some(listing) = &mut session.listing {
                listing.push(("q:foo".into(), RecordedEntry::NameOnly));
            }
            session.uses = order.iter().map(|s| use_of(s)).collect();
            let built = build_agent(
                &AgentId::parse(AgentId::CODEX).unwrap(),
                "Codex",
                SessionRecords::ListingAndUses,
                None,
                &corpus,
                &[session],
            );
            let foo = built.skills.iter().find(|r| r.name == "foo").unwrap();
            assert_eq!((foo.used, foo.used_name_only), (1, 1), "{order:?}");
            assert!(foo.flags.contains(&SkillSessionFlag::UsedWhileNameOnly));
        }
    }

    fn listed(names: &[&str]) -> ParsedSession {
        ParsedSession {
            listing: Some(
                names
                    .iter()
                    .map(|n| {
                        (
                            (*n).to_string(),
                            RecordedEntry::Described {
                                chars: 10,
                                text: "described!".into(),
                            },
                        )
                    })
                    .collect(),
            ),
            ..ParsedSession::default()
        }
    }

    #[test]
    fn a_skill_listed_in_every_session_and_never_used_is_flagged_but_a_used_one_is_not() {
        let mut sessions: Vec<ParsedSession> = (0..3).map(|_| listed(&["idle", "busy"])).collect();
        sessions[0].uses.push(SkillInvocation {
            skill: "busy".into(),
            harness: AgentId::CLAUDE_CODE.into(),
            trigger: SkillTrigger::Agent,
            at: DateTime::<Utc>::UNIX_EPOCH,
            project_path: None,
            session: None,
            use_session: None,
        });
        let agent = build_agent(
            &AgentId::parse(AgentId::CLAUDE_CODE).unwrap(),
            "Claude Code",
            SessionRecords::ListingAndUses,
            None,
            &[],
            &sessions,
        );
        let flags = |n: &str| {
            agent
                .skills
                .iter()
                .find(|r| r.name == n)
                .unwrap()
                .flags
                .clone()
        };
        assert_eq!(flags("idle"), [SkillSessionFlag::ListedNeverUsed]);
        assert!(flags("busy").is_empty());
    }

    #[test]
    fn a_short_prompt_that_matches_one_description_well_is_a_possible_miss_only_without_uses() {
        let mut corpus: Vec<CorpusSkill> = (0..40)
            .map(|i| CorpusSkill {
                name: format!("filler{i}"),
                path: None,
                description: format!("Filler skill about subject{i} and topic{i}."),
                reachable: true,
            })
            .collect();
        corpus.extend([
            CorpusSkill {
                name: "tooltips".into(),
                path: None,
                description: "Tooltip delay, hover intent and tooltip timers. Use when tooltips show up too slowly in a toolbar, or at the wrong time.".into(),
                reachable: true,
            },
            CorpusSkill {
                name: "deploy".into(),
                path: None,
                description: "Deploy the app to production servers.".into(),
                reachable: false,
            },
        ]);
        let mut session = listed(&["deploy"]);
        session.prompt =
            Some("Tooltips in the toolbar show up too slowly, the tooltip delay is wrong".into());
        let agent = build_agent(
            &AgentId::parse(AgentId::CLAUDE_CODE).unwrap(),
            "Claude Code",
            SessionRecords::ListingAndUses,
            None,
            &corpus,
            &[session],
        );
        let missed = &agent.sessions[0].missed;
        assert_eq!(missed.len(), 1);
        assert_eq!(missed[0].skill, "tooltips");
        assert_eq!(missed[0].listed_as, SessionListingState::NotListed);
        let deploy = agent.skills.iter().find(|r| r.name == "deploy").unwrap();
        assert_eq!(deploy.flags, [SkillSessionFlag::NotReachable]);
        assert_eq!(
            deploy.reach.as_deref(),
            Some("Not in a folder Claude Code reads.")
        );

        let index = MissIndex::new(
            corpus
                .iter()
                .map(|s| (s.name.as_str(), s.description.as_str())),
        );
        assert!(index
            .guess(Some("tooltip delay is wrong on tooltips"), true)
            .is_none());
    }
}
