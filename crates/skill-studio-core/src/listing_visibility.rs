//! Per-skill listing state: which skills a harness shows the model with a
//! description, with a name only, or not at all. Claude Code and Codex
//! follow the algorithms in docs/agent-skill-conventions.md "Model listing
//! budgets"; other harnesses have no known limit.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dto::ScanRequest;
use crate::error::{CoreError, ErrorCode};
use crate::identity::{AgentId, ProjectRef, RootScope};
use crate::listing_cost::{
    claude_budget, claude_settings, codex_budget, codex_listing_settings, listed_skills,
    CodexLimit, ListedSkill,
};
use crate::ops::Operation;
use crate::ports::{OpContext, Runtime};

/// What one harness shows the model for one skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ListingState {
    /// Name and full description.
    Described,
    /// Name and a shortened description.
    Truncated,
    /// Name only.
    NameOnly,
    /// Not listed: the budget ran out.
    Omitted,
    /// Not listed: manual-only, off, parked, or rejected.
    Hidden,
}

/// One skill in one harness listing.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListingEntry {
    /// Name as the harness lists it. Claude Code prefixes plugin skills.
    #[schemars(extend("x-untrusted" = true))]
    pub name: String,
    /// Deployment folder. Null for entries known only from a session.
    #[schemars(extend("x-untrusted" = true))]
    pub path: Option<PathBuf>,
    /// Predicted state.
    pub state: ListingState,
    /// Why a hidden skill is hidden.
    pub hidden_reason: Option<String>,
    /// Description characters the model sees.
    pub shown_description_chars: u64,
    /// Description characters before any cut.
    pub full_description_chars: u64,
    /// Claude Code's usage score: uses times a 7-day half-life decay, at least 10%.
    pub usage_score: Option<f64>,
    /// State in the last recorded session, when one was found.
    pub observed: Option<ListingState>,
    /// False for entries Skill Studio does not scan, such as bundled skills.
    pub scanned: bool,
}

/// The setting that controls the budget, and how to change it.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListingSetting {
    /// Setting key.
    pub key: String,
    /// Current value, when set.
    #[schemars(extend("x-untrusted" = true))]
    pub current: Option<String>,
    /// File to edit.
    #[schemars(extend("x-untrusted" = true))]
    pub file: PathBuf,
    /// chezmoi source file when chezmoi manages `file`: edit this one, or
    /// chezmoi reverts the change.
    #[schemars(extend("x-untrusted" = true))]
    pub chezmoi_source: Option<PathBuf>,
    /// The text to put in `file` for the requested budget.
    #[schemars(extend("x-untrusted" = true))]
    pub snippet: String,
}

/// The session the observed states come from.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ObservedSession {
    /// Transcript file.
    #[schemars(extend("x-untrusted" = true))]
    pub transcript: PathBuf,
    /// Time of the listing, as recorded.
    #[schemars(extend("x-untrusted" = true))]
    pub at: Option<String>,
    /// Model id that answered in that session.
    #[schemars(extend("x-untrusted" = true))]
    pub model: Option<String>,
    /// Entries in that listing.
    pub listed: usize,
    /// Entries with a description.
    pub described: usize,
}

/// One harness listing in one scope.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HarnessListing {
    /// Harness id.
    pub harness: AgentId,
    /// Project sessions include global skills.
    pub scope: RootScope,
    /// Budget, or null when the harness has no known limit.
    pub budget: Option<u64>,
    /// `characters` or `tokens` (UTF-8 bytes divided by four).
    pub budget_unit: String,
    /// The whole listing with every description, in `budget_unit`.
    pub full_size: u64,
    /// How the budget was resolved.
    #[schemars(extend("x-untrusted" = true))]
    pub budget_source: String,
    /// Context window in tokens, when known.
    pub context_window_tokens: Option<u64>,
    /// Where the context window came from.
    #[schemars(extend("x-untrusted" = true))]
    pub context_source: String,
    /// True when no model or context size was found.
    pub uses_default_context_window: bool,
    /// What Skill Studio infers instead of reading.
    pub notes: Vec<String>,
    /// The setting that controls the budget.
    pub setting: Option<ListingSetting>,
    /// The session that supplied `observed` states.
    pub observed_session: Option<ObservedSession>,
    /// Entries in listing order; hidden entries last.
    pub entries: Vec<ListingEntry>,
}

/// Input for `listing`.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListingRequest {
    /// Also report this project. It must be in the runtime scope.
    #[schemars(extend("x-untrusted" = true))]
    pub project_path: Option<PathBuf>,
    /// Predict Claude Code at this `skillListingBudgetFraction` (0 to 1).
    pub claude_budget_fraction: Option<f64>,
    /// Predict Claude Code with this context window in tokens.
    pub claude_context_window_tokens: Option<u64>,
    /// Predict Codex at this `skills.max_context_tokens` (capped at 10000).
    pub codex_max_context_tokens: Option<u64>,
    /// Compare Claude Code with this session transcript instead of the newest one.
    #[schemars(extend("x-untrusted" = true))]
    pub claude_transcript: Option<PathBuf>,
}

/// Every harness listing for the global scope and the requested project.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListingReport {
    /// Rows in scope, then harness, order.
    pub listings: Vec<HarnessListing>,
}

/// Predicts each harness listing and reads the last recorded Claude Code
/// listing for comparison. Reads only; it never writes agent settings.
pub fn listing(
    rt: &Runtime,
    ctx: &OpContext,
    req: &ListingRequest,
) -> Result<ListingReport, CoreError> {
    rt.run(Operation::Listing, ctx, || {
        if req
            .claude_budget_fraction
            .is_some_and(|f| !(f > 0.0 && f <= 1.0))
        {
            return Err(CoreError::new(
                ErrorCode::InvalidRequest,
                "claude_budget_fraction must be above 0 and at most 1",
            ));
        }
        let mut scopes = vec![RootScope::Global];
        if let Some(path) = &req.project_path {
            let project = rt
                .scope
                .projects
                .iter()
                .find(|p| p.lexical == *path || p.canonical == *path)
                .ok_or_else(|| {
                    CoreError::new(ErrorCode::InvalidRequest, "Project is not in the scope")
                        .at(path)
                })?;
            scopes.push(RootScope::Project(ProjectRef(project.lexical.clone())));
        }
        let _folder_guard = rt.ports.history.guard_read_data_folder()?;
        let _guard = crate::ports::acquire_shared(rt.ports.leases.as_ref(), &rt.scope)?;
        let inventory = crate::ops::scan_inner(rt, ctx, &ScanRequest::default())?;
        let mut listings = Vec::new();
        for scope in &scopes {
            for facts in &rt.ports.catalog.facts {
                ctx.checkpoint()?;
                listings.push(match facts.id.as_str() {
                    AgentId::CLAUDE_CODE => claude_listing(rt, &inventory, facts, scope, req)?,
                    AgentId::CODEX => codex_listing(rt, &inventory, facts, scope, req)?,
                    _ => unlimited_listing(rt, &inventory, facts, scope)?,
                });
            }
        }
        Ok(ListingReport { listings })
    })
}

fn hidden_entry(skill: &ListedSkill, name: String) -> ListingEntry {
    ListingEntry {
        name,
        path: Some(skill.path.clone()),
        state: ListingState::Hidden,
        hidden_reason: skill.hidden.map(str::to_string),
        shown_description_chars: 0,
        full_description_chars: skill.description.chars().count() as u64,
        usage_score: None,
        observed: None,
        scanned: true,
    }
}

fn unlimited_listing(
    rt: &Runtime,
    inventory: &crate::dto::Inventory,
    facts: &crate::harness::HarnessFacts,
    scope: &RootScope,
) -> Result<HarnessListing, CoreError> {
    let mut entries: Vec<ListingEntry> = listed_skills(rt, inventory, facts, scope, None)?
        .into_values()
        .map(|skill| {
            if skill.hidden.is_some() {
                return hidden_entry(&skill, skill.name.clone());
            }
            let chars = skill.description.chars().count() as u64;
            ListingEntry {
                name: skill.name,
                path: Some(skill.path),
                state: ListingState::Described,
                hidden_reason: None,
                shown_description_chars: chars,
                full_description_chars: chars,
                usage_score: None,
                observed: None,
                scanned: true,
            }
        })
        .collect();
    entries.sort_by_key(|e| e.state == ListingState::Hidden);
    let full_size = entries
        .iter()
        .filter(|e| e.state != ListingState::Hidden)
        .map(|e| e.name.chars().count() as u64 + e.full_description_chars + 5)
        .sum();
    Ok(HarnessListing {
        harness: facts.id.clone(),
        scope: scope.clone(),
        budget: None,
        budget_unit: "characters".into(),
        full_size,
        budget_source: "no known limit".into(),
        context_window_tokens: None,
        context_source: "not used".into(),
        uses_default_context_window: false,
        notes: Vec::new(),
        setting: None,
        observed_session: None,
        entries,
    })
}

// ---------------------------------------------------------------------------
// Claude Code
// ---------------------------------------------------------------------------

/// One line of Claude Code's listing, as `oVn` in 2.1.327 measures it.
#[derive(Debug, Clone)]
pub(crate) struct ClaudeLine {
    /// Width of `- name`.
    pub name_width: u64,
    /// Width of `- name: description`.
    pub full_width: u64,
    /// Bundled and `name-only` entries keep their form and are never cut.
    pub fixed: bool,
    /// Usage score; higher keeps its description first.
    pub score: f64,
}

/// Which lines keep their description. Mirrors Claude Code 2.1.327 `oVn`:
/// when the full listing fits, every line keeps it. Otherwise fixed lines
/// stay as they are, every other line starts as a name, and descriptions
/// come back in score order (stable, so ties keep listing order) while
/// each one still fits in what is left.
pub(crate) fn claude_allocate(lines: &[ClaudeLine], budget: u64) -> Vec<bool> {
    let joins = lines.len().saturating_sub(1) as u64;
    let total: u64 = lines.iter().map(|l| l.full_width).sum::<u64>() + joins;
    if total <= budget {
        return vec![true; lines.len()];
    }
    let base: u64 = lines
        .iter()
        .map(|l| if l.fixed { l.full_width } else { l.name_width })
        .sum::<u64>()
        + joins;
    let mut left = budget.cast_signed() - base.cast_signed();
    let mut order: Vec<usize> = (0..lines.len()).filter(|i| !lines[*i].fixed).collect();
    order.sort_by(|a, b| {
        lines[*b]
            .score
            .partial_cmp(&lines[*a].score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut kept: Vec<bool> = lines.iter().map(|l| l.fixed).collect();
    for i in order {
        let gain = (lines[i].full_width - lines[i].name_width).cast_signed();
        if gain <= left {
            kept[i] = true;
            left -= gain;
        }
    }
    kept
}

/// Display width the way Claude Code counts it for ASCII text: UTF-16
/// units, without newlines. Wide and combining characters can differ.
fn claude_width(text: &str) -> u64 {
    text.encode_utf16()
        .filter(|u| *u != u16::from(b'\n'))
        .count() as u64
}

/// `ibt`: the description plus `when_to_use`, cut to `cap` with an ellipsis.
fn claude_description(skill: &ListedSkill, cap: usize) -> (String, bool) {
    let text = match &skill.when_to_use {
        Some(when) => format!("{} - {when}", skill.description),
        None => skill.description.clone(),
    };
    let units: Vec<u16> = text.encode_utf16().collect();
    if units.len() > cap {
        let cut = String::from_utf16_lossy(&units[..cap.saturating_sub(1)]);
        (format!("{cut}\u{2026}"), true)
    } else {
        (text, false)
    }
}

/// `jBr`: uses times `max(0.5^(days/7), 0.1)`, from `skillUsage` in
/// `~/.claude.json`.
fn claude_scores(rt: &Runtime) -> BTreeMap<String, f64> {
    let path = rt.scope.global_root_path(Path::new(".claude.json"));
    let Ok(bytes) = rt.ports.fs.read_capped(&path, 32 * 1024 * 1024) else {
        return BTreeMap::new();
    };
    let Ok(config) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return BTreeMap::new();
    };
    let now = rt.ports.clock.now().timestamp_millis() as f64;
    config
        .get("skillUsage")
        .and_then(serde_json::Value::as_object)
        .map(|usage| {
            usage
                .iter()
                .filter_map(|(name, row)| {
                    let count = row.get("usageCount")?.as_f64()?;
                    let last = row.get("lastUsedAt")?.as_f64()?;
                    let days = (now - last) / 86_400_000.0;
                    let score = count * 0.5_f64.powf(days / 7.0).max(0.1);
                    // Three decimals: enough to order skills, stable across calls.
                    Some((name.clone(), (score * 1000.0).round() / 1000.0))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The first full skill listing recorded in a Claude Code transcript.
struct RecordedListing {
    session: ObservedSession,
    /// Names in listing order, with whether each kept a description and its
    /// line width.
    lines: Vec<(String, bool, u64)>,
}

/// What recorded sessions show about one entry Skill Studio does not scan.
#[derive(Debug, Clone, Default)]
struct Unscanned {
    /// Width of the line with its description, when one was seen.
    full_width: Option<u64>,
    /// True when a session listed it without a description.
    seen_name_only: bool,
}

/// Claude Code's project folder name: every character that is not ASCII
/// alphanumeric becomes `-`.
fn claude_project_slug(path: &Path) -> String {
    path.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

fn newest_transcripts(rt: &Runtime, scope: &RootScope) -> Vec<PathBuf> {
    let projects = rt.scope.global_root_path(Path::new(".claude/projects"));
    let dirs: Vec<PathBuf> = match scope {
        RootScope::Project(project) => vec![projects.join(claude_project_slug(&project.0))],
        RootScope::Global => rt
            .ports
            .fs
            .read_dir(&projects)
            .unwrap_or_default()
            .into_iter()
            .map(|e| projects.join(e.name))
            .collect(),
    };
    let mut files = Vec::new();
    for dir in dirs {
        for entry in rt.ports.fs.read_dir(&dir).unwrap_or_default() {
            if !Path::new(&entry.name)
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
                files.push((modified, path));
            }
        }
    }
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    files.into_iter().take(20).map(|(_, p)| p).collect()
}

/// The newest recorded listings for `scope`, newest first.
fn recorded_listings(rt: &Runtime, scope: &RootScope) -> Vec<RecordedListing> {
    newest_transcripts(rt, scope)
        .into_iter()
        .filter_map(|path| read_recorded_listing(rt, &path))
        .collect()
}

fn read_recorded_listing(rt: &Runtime, path: &Path) -> Option<RecordedListing> {
    let (bytes, _) = rt.ports.fs.read_prefix(path, 2 * 1024 * 1024).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let mut listing = None;
    let mut model = None;
    for line in text.lines() {
        if listing.is_none() && line.contains("\"skill_listing\"") {
            let Ok(row) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            let Some(attachment) = row.get("attachment") else {
                continue;
            };
            if attachment.get("type").and_then(serde_json::Value::as_str) != Some("skill_listing")
                || attachment
                    .get("isInitial")
                    .and_then(serde_json::Value::as_bool)
                    != Some(true)
            {
                continue;
            }
            listing = Some((
                attachment.clone(),
                row.get("timestamp")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
            ));
        }
        if model.is_none() && line.contains("\"assistant\"") && line.contains("\"model\"") {
            if let Ok(row) = serde_json::from_str::<serde_json::Value>(line) {
                model = row
                    .get("message")
                    .and_then(|m| m.get("model"))
                    .and_then(serde_json::Value::as_str)
                    .filter(|m| m.starts_with("claude-"))
                    .map(str::to_string);
            }
        }
        if listing.is_some() && model.is_some() {
            break;
        }
    }
    let (attachment, at) = listing?;
    let content = attachment.get("content")?.as_str()?;
    let names: Vec<String> = attachment
        .get("names")?
        .as_array()?
        .iter()
        .filter_map(|n| n.as_str().map(str::to_string))
        .collect();
    let lines = parse_listing(content, &names);
    Some(RecordedListing {
        session: ObservedSession {
            transcript: path.to_path_buf(),
            at,
            model,
            listed: lines.len(),
            described: lines.iter().filter(|(_, described, _)| *described).count(),
        },
        lines,
    })
}

/// The name and description flag of a line that starts a listing entry:
/// `- name`, `- name: description`, or `- name (alias): description`.
fn entry_head<'a>(line: &'a str, known: &BTreeSet<&str>) -> Option<(&'a str, bool)> {
    let rest = line.strip_prefix("- ")?;
    let (head, described) = match rest.split_once(": ") {
        Some((head, _)) => (head, true),
        None => (rest, false),
    };
    let name = head.split(" (").next().unwrap_or(head);
    known.contains(name).then_some((name, described))
}

/// Splits a listing into entries. Each entry starts with `- <name>` at the
/// start of a line; descriptions can span lines.
fn parse_listing(content: &str, names: &[String]) -> Vec<(String, bool, u64)> {
    let known: BTreeSet<&str> = names.iter().map(String::as_str).collect();
    let mut out: Vec<(String, bool, u64)> = Vec::new();
    let mut text_lines = content.split('\n').peekable();
    while let Some(line) = text_lines.next() {
        let Some((name, described)) = entry_head(line, &known) else {
            continue;
        };
        let mut width = claude_width(line);
        while let Some(next) = text_lines.peek() {
            if entry_head(next, &known).is_some() {
                break;
            }
            width += claude_width(next);
            text_lines.next();
        }
        out.push((name.to_string(), described, width));
    }
    out
}

fn claude_listing(
    rt: &Runtime,
    inventory: &crate::dto::Inventory,
    facts: &crate::harness::HarnessFacts,
    scope: &RootScope,
    req: &ListingRequest,
) -> Result<HarnessListing, CoreError> {
    let settings = claude_settings(rt, scope)?;
    let mut history = recorded_listings(rt, scope);
    if let Some(path) = &req.claude_transcript {
        let projects = rt.scope.global_root_path(Path::new(".claude/projects"));
        let inside = match (
            rt.ports.fs.canonicalize(path),
            rt.ports.fs.canonicalize(&projects),
        ) {
            (Ok(file), Ok(root)) => file.starts_with(&root),
            _ => false,
        };
        if !inside {
            return Err(CoreError::new(
                ErrorCode::InvalidRequest,
                "The transcript must be a Claude Code session file in this home",
            )
            .at(path));
        }
        let chosen = read_recorded_listing(rt, path).ok_or_else(|| {
            CoreError::new(
                ErrorCode::InvalidRequest,
                "No skill listing in this transcript",
            )
            .at(path)
        })?;
        history.insert(0, chosen);
    }
    let recorded = history.first();
    let budget = claude_budget(
        rt,
        &settings,
        recorded.and_then(|r| r.session.model.as_deref()),
        req.claude_budget_fraction,
        req.claude_context_window_tokens,
    );
    let scores = claude_scores(rt);
    let skills = listed_skills(rt, inventory, facts, scope, Some(&settings))?;
    let mut hidden = Vec::new();
    // Claude Code lists one entry per name (`fi(..., "name")`): the first wins.
    let mut by_name: BTreeMap<String, ListedSkill> = BTreeMap::new();
    for skill in skills.into_values() {
        let name = match &skill.plugin {
            Some(plugin) => format!("{plugin}:{}", skill.name),
            None => skill.name.clone(),
        };
        let override_value = settings.overrides.get(&name).map(String::as_str);
        if skill.hidden.is_some() || matches!(override_value, Some("off" | "user-invocable-only")) {
            let mut entry = hidden_entry(&skill, name);
            if entry.hidden_reason.is_none() {
                entry.hidden_reason = Some("off in agent settings".into());
            }
            hidden.push(entry);
            continue;
        }
        by_name.entry(name).or_insert(skill);
    }
    // Entries Skill Studio does not scan (bundled skills, plugin commands,
    // MCP prompts) come from recorded sessions. One seen both with and
    // without a description is not bundled, so it competes for the budget.
    // A session recorded in another project lists that project's skills;
    // they are not this scope's entries. Names that only match a skill in
    // some other project (a built-in command, say) stay.
    let session_dir = recorded.and_then(|r| {
        r.session
            .transcript
            .parent()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
    });
    let other_project_names: BTreeSet<String> = inventory
        .skills
        .iter()
        .flat_map(|skill| skill.deployments.iter().map(move |d| (skill, d)))
        .filter(|(_, d)| match &d.root.scope {
            RootScope::Project(p) => {
                &d.root.scope != scope
                    && session_dir.as_deref() == Some(claude_project_slug(&p.0).as_str())
            }
            RootScope::Global => false,
        })
        .map(|(skill, d)| {
            d.frontmatter
                .as_ref()
                .and_then(|f| f.name.clone())
                .unwrap_or_else(|| skill.name.0.clone())
        })
        .collect();
    let mut seen: BTreeMap<String, Unscanned> = BTreeMap::new();
    for listing in &history {
        for (name, described, width) in &listing.lines {
            let facts = seen.entry(name.clone()).or_default();
            if *described {
                facts.full_width = Some(*width);
            } else {
                facts.seen_name_only = true;
            }
        }
    }
    // Listing order: the recorded session's order, then scanned names it
    // did not have, plain skills before plugin skills, by name.
    let mut order: Vec<String> = Vec::new();
    let mut unscanned: BTreeMap<String, Unscanned> = BTreeMap::new();
    if let Some(recorded) = recorded {
        for (name, _, _) in &recorded.lines {
            if by_name.contains_key(name) {
                order.push(name.clone());
            } else if !other_project_names.contains(name) && hidden.iter().all(|h| &h.name != name)
            {
                // Current settings can turn off an entry the session listed.
                if matches!(
                    settings.overrides.get(name).map(String::as_str),
                    Some("off" | "user-invocable-only")
                ) {
                    hidden.push(ListingEntry {
                        name: name.clone(),
                        path: None,
                        state: ListingState::Hidden,
                        hidden_reason: Some("off in agent settings".into()),
                        shown_description_chars: 0,
                        full_description_chars: 0,
                        usage_score: None,
                        observed: None,
                        scanned: false,
                    });
                    continue;
                }
                unscanned.insert(name.clone(), seen.get(name).cloned().unwrap_or_default());
                order.push(name.clone());
            }
        }
    }
    let mut rest: Vec<&String> = by_name.keys().filter(|n| !order.contains(n)).collect();
    rest.sort_by_key(|n| (n.contains(':'), (*n).clone()));
    order.extend(rest.into_iter().cloned());

    let mut lines = Vec::new();
    let mut entries = Vec::new();
    for name in &order {
        let score = scores.get(name).copied().unwrap_or(0.0);
        if let Some(skill) = by_name.get(name) {
            let (description, cut) = claude_description(skill, settings.max_desc_chars);
            // An empty description is listed as a name. Claude Code may
            // fill it from the body, which Skill Studio does not read.
            let name_only = settings.overrides.get(name).map(String::as_str) == Some("name-only")
                || description.trim().is_empty();
            let name_width = claude_width(&format!("- {name}"));
            let full_width = if name_only {
                name_width
            } else {
                claude_width(&format!("- {name}: {description}"))
            };
            lines.push(ClaudeLine {
                name_width,
                full_width,
                fixed: name_only,
                score,
            });
            entries.push(ListingEntry {
                name: name.clone(),
                path: Some(skill.path.clone()),
                state: if cut {
                    ListingState::Truncated
                } else {
                    ListingState::Described
                },
                hidden_reason: None,
                shown_description_chars: claude_width(&description),
                full_description_chars: claude_width(&skill.description)
                    + skill
                        .when_to_use
                        .as_deref()
                        .map_or(0, |w| claude_width(w) + 3),
                usage_score: Some(score),
                observed: None,
                scanned: true,
            });
        } else if let Some(facts) = unscanned.get(name) {
            let name_width = claude_width(&format!("- {name}"));
            let full_width = facts.full_width.unwrap_or(name_width);
            lines.push(ClaudeLine {
                name_width,
                full_width,
                // Never seen without a description: bundled, always whole.
                // Never seen with one: its length is unknown, so it stays a name.
                fixed: !(facts.seen_name_only && facts.full_width.is_some()),
                score,
            });
            let description = full_width.saturating_sub(name_width + 2);
            entries.push(ListingEntry {
                name: name.clone(),
                path: None,
                state: ListingState::Described,
                hidden_reason: None,
                shown_description_chars: description,
                full_description_chars: description,
                usage_score: Some(score),
                observed: None,
                scanned: false,
            });
        }
    }
    let kept = claude_allocate(&lines, budget.chars);
    for ((entry, kept), line) in entries.iter_mut().zip(&kept).zip(&lines) {
        if !kept || line.full_width == line.name_width {
            entry.state = ListingState::NameOnly;
            entry.shown_description_chars = 0;
        }
    }
    if let Some(recorded) = recorded {
        let observed: BTreeMap<&str, bool> = recorded
            .lines
            .iter()
            .map(|(n, d, _)| (n.as_str(), *d))
            .collect();
        for entry in entries.iter_mut().chain(hidden.iter_mut()) {
            entry.observed = Some(match observed.get(entry.name.as_str()) {
                Some(true) => ListingState::Described,
                Some(false) => ListingState::NameOnly,
                None => ListingState::Hidden,
            });
        }
    }
    let full_size =
        lines.iter().map(|l| l.full_width).sum::<u64>() + lines.len().saturating_sub(1) as u64;
    let mut notes = vec![
        "Claude Code drops descriptions of the least-used skills first: uses in ~/.claude.json skillUsage, halved every 7 days, never below 10%.".to_string(),
        "Skills with the same score keep listing order. The order comes from the last recorded session; skills it did not list are placed by name, which is inferred.".to_string(),
    ];
    if !unscanned.is_empty() {
        notes.push(format!(
            "{} entries that Skill Studio does not scan (bundled skills, plugin commands, MCP prompts) are sized from the last 20 recorded sessions. One never seen with a description stays a name.",
            unscanned.len()
        ));
    }
    if recorded.is_none() {
        notes.push("No recorded session was found, so bundled skills are not counted.".into());
    }
    let requested = req.claude_budget_fraction;
    // The file that supplies the value now; a project layer overrides the user file.
    let claude_file = settings
        .fraction
        .as_ref()
        .map_or_else(|| settings.user_path.clone(), |(_, path)| path.clone());
    let setting = ListingSetting {
        key: "skillListingBudgetFraction".into(),
        current: settings.fraction.as_ref().map(|(f, _)| f.to_string()),
        file: claude_file.clone(),
        chezmoi_source: chezmoi_source(rt, &claude_file),
        snippet: format!(
            "\"skillListingBudgetFraction\": {}",
            requested.unwrap_or(budget.fraction)
        ),
    };
    entries.extend(hidden);
    Ok(HarnessListing {
        harness: facts.id.clone(),
        scope: scope.clone(),
        budget: Some(budget.chars),
        budget_unit: "characters".into(),
        full_size,
        budget_source: format!(
            "{} of {} tokens at {} characters per token ({})",
            budget.fraction, budget.context_tokens, budget.chars_per_token, budget.fraction_source
        ),
        context_window_tokens: Some(budget.context_tokens),
        context_source: budget.context_source,
        uses_default_context_window: budget.default_context,
        notes,
        setting: Some(setting),
        observed_session: recorded.map(|r| r.session.clone()),
        entries,
    })
}

// ---------------------------------------------------------------------------
// Codex
// ---------------------------------------------------------------------------

/// One line of Codex's listing: `- {name}: {description} (file: {path})`.
#[derive(Debug, Clone)]
pub(crate) struct CodexLine {
    pub name: String,
    /// Description after the 1024-character cap.
    pub description: Vec<char>,
    pub locator: String,
}

impl CodexLine {
    fn render(&self, chars: usize) -> String {
        if chars == 0 {
            format!("- {}: (file: {})", self.name, self.locator)
        } else {
            let text: String = self.description[..chars].iter().collect();
            format!("- {}: {text} (file: {})", self.name, self.locator)
        }
    }

    /// Cost with the trailing newline: `(bytes + 3) / 4` tokens, or chars.
    fn cost(&self, chars: usize, limit: CodexLimit) -> u64 {
        let line = self.render(chars);
        match limit {
            CodexLimit::Tokens(_) => (line.len() as u64 + 1).div_ceil(4),
            CodexLimit::Chars(_) => line.chars().count() as u64 + 1,
        }
    }
}

/// Description characters each line keeps, or `None` when it is omitted.
/// Mirrors `allocate_skill_lines` in Codex 0.170.0 `ext/skills/src/render.rs`.
pub(crate) fn codex_allocate(lines: &[CodexLine], limit: CodexLimit) -> Vec<Option<usize>> {
    let max = match limit {
        CodexLimit::Tokens(v) | CodexLimit::Chars(v) => v,
    };
    let full: u64 = lines
        .iter()
        .map(|l| l.cost(l.description.len(), limit))
        .sum();
    if full <= max {
        return lines.iter().map(|l| Some(l.description.len())).collect();
    }
    let minimum: u64 = lines.iter().map(|l| l.cost(0, limit)).sum();
    if minimum <= max {
        // Round robin: one more character per line per pass, while it fits.
        let mut alloc = vec![0usize; lines.len()];
        let mut current: Vec<u64> = lines.iter().map(|l| l.cost(0, limit)).collect();
        let mut left = max - minimum;
        loop {
            let mut changed = false;
            for (i, line) in lines.iter().enumerate() {
                if alloc[i] >= line.description.len() {
                    continue;
                }
                let next = line.cost(alloc[i] + 1, limit);
                let delta = next - current[i];
                if delta <= left {
                    alloc[i] += 1;
                    current[i] = next;
                    left -= delta;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        return alloc.into_iter().map(Some).collect();
    }
    let mut used = 0;
    lines
        .iter()
        .map(|l| {
            let next = used + l.cost(0, limit);
            if next <= max {
                used = next;
                Some(0)
            } else {
                None
            }
        })
        .collect()
}

fn codex_listing(
    rt: &Runtime,
    inventory: &crate::dto::Inventory,
    facts: &crate::harness::HarnessFacts,
    scope: &RootScope,
    req: &ListingRequest,
) -> Result<HarnessListing, CoreError> {
    let settings = codex_listing_settings(rt, scope)?;
    let budget = codex_budget(rt, &settings, req.codex_max_context_tokens);
    let mut hidden = Vec::new();
    let mut listed = Vec::new();
    for skill in listed_skills(rt, inventory, facts, scope, None)?.into_values() {
        if skill.hidden.is_some() {
            hidden.push(hidden_entry(&skill, skill.name.clone()));
        } else {
            listed.push(skill);
        }
    }
    // Core-compatible order: repo skills before user skills, then name, then path.
    listed.sort_by(|a, b| {
        (!a.project_root, &a.name, &a.path).cmp(&(!b.project_root, &b.name, &b.path))
    });
    let lines: Vec<CodexLine> = listed
        .iter()
        .map(|skill| {
            let chars: Vec<char> = skill.description.chars().collect();
            let description = if chars.len() > 1024 {
                chars[..1021].iter().copied().chain("...".chars()).collect()
            } else {
                chars
            };
            CodexLine {
                name: skill.name.clone(),
                description,
                locator: skill.path.join("SKILL.md").to_string_lossy().into_owned(),
            }
        })
        .collect();
    let alloc = codex_allocate(&lines, budget.limit);
    let mut entries: Vec<ListingEntry> = listed
        .iter()
        .zip(&lines)
        .zip(&alloc)
        .map(|((skill, line), alloc)| {
            let full = skill.description.chars().count() as u64;
            let state = match alloc {
                None => ListingState::Omitted,
                Some(0) if !line.description.is_empty() => ListingState::NameOnly,
                Some(n) if (*n as u64) < full => ListingState::Truncated,
                Some(_) => ListingState::Described,
            };
            ListingEntry {
                name: skill.name.clone(),
                path: Some(skill.path.clone()),
                state,
                hidden_reason: None,
                shown_description_chars: alloc.unwrap_or(0) as u64,
                full_description_chars: full,
                usage_score: None,
                observed: None,
                scanned: true,
            }
        })
        .collect();
    let (max, unit) = match budget.limit {
        CodexLimit::Tokens(v) => (v, "tokens"),
        CodexLimit::Chars(v) => (v, "characters"),
    };
    let full_size = lines
        .iter()
        .map(|l| l.cost(l.description.len(), budget.limit))
        .sum();
    let requested = req
        .codex_max_context_tokens
        .or(settings.max_context_tokens.as_ref().map(|(v, _)| *v));
    // The file that supplies the value now; a trusted project layer overrides the user file.
    let codex_file = settings
        .max_context_tokens
        .as_ref()
        .map_or_else(|| settings.user_path.clone(), |(_, path)| path.clone());
    let setting = ListingSetting {
        key: "skills.max_context_tokens".into(),
        current: settings
            .max_context_tokens
            .as_ref()
            .map(|(v, _)| v.to_string()),
        file: codex_file.clone(),
        chezmoi_source: chezmoi_source(rt, &codex_file),
        snippet: format!(
            "[skills]\nmax_context_tokens = {}",
            requested.unwrap_or(max).min(10_000)
        ),
    };
    entries.extend(hidden);
    Ok(HarnessListing {
        harness: facts.id.clone(),
        scope: scope.clone(),
        budget: Some(max),
        budget_unit: unit.into(),
        full_size,
        budget_source: budget.source,
        context_window_tokens: budget.context_tokens,
        context_source: if budget.default_context {
            "unknown".into()
        } else {
            "model_context_window or ~/.codex/models_cache.json".into()
        },
        uses_default_context_window: budget.default_context,
        notes: vec![
            "Codex shortens every description by the same amount first, then lists names only, then drops entries from the end of the list.".into(),
            "The order (project skills, then user skills, by name) is the core-compatible order; extension listings keep catalog order instead.".into(),
        ],
        setting: Some(setting),
        observed_session: None,
        entries,
    })
}

// ---------------------------------------------------------------------------
// chezmoi
// ---------------------------------------------------------------------------

/// The chezmoi source file for `target`, when `~/.local/share/chezmoi`
/// manages it. A direct edit of `target` is reverted on the next apply.
fn chezmoi_source(rt: &Runtime, target: &Path) -> Option<PathBuf> {
    let home = &rt.scope.home.lexical;
    let relative = target.strip_prefix(home).ok()?;
    let mut dir = home.join(".local/share/chezmoi");
    if let Ok(root) = rt.ports.fs.read_capped(&dir.join(".chezmoiroot"), 4096) {
        dir = dir.join(String::from_utf8_lossy(&root).trim());
    }
    for component in relative.components() {
        let wanted = component.as_os_str().to_string_lossy();
        let entry = rt
            .ports
            .fs
            .read_dir(&dir)
            .ok()?
            .into_iter()
            .find(|e| chezmoi_target_name(&e.name) == wanted)?;
        dir = dir.join(entry.name);
    }
    Some(dir)
}

/// Removes chezmoi's source attributes from one file or folder name.
fn chezmoi_target_name(source: &str) -> String {
    const PREFIXES: [&str; 17] = [
        "create_",
        "modify_",
        "remove_",
        "run_",
        "once_",
        "onchange_",
        "before_",
        "after_",
        "symlink_",
        "encrypted_",
        "private_",
        "readonly_",
        "empty_",
        "executable_",
        "exact_",
        "external_",
        "literal_",
    ];
    let mut name = source.strip_suffix(".tmpl").unwrap_or(source);
    while let Some(rest) = PREFIXES.iter().find_map(|p| name.strip_prefix(p)) {
        name = rest;
    }
    match name.strip_prefix("dot_") {
        Some(rest) => format!(".{rest}"),
        None => name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(name: u64, full: u64, score: f64) -> ClaudeLine {
        ClaudeLine {
            name_width: name,
            full_width: full,
            fixed: false,
            score,
        }
    }

    #[test]
    fn claude_keeps_every_description_when_the_listing_fits() {
        let lines = [line(5, 20, 0.0), line(5, 30, 0.0)];
        assert_eq!(claude_allocate(&lines, 51), vec![true, true]);
    }

    #[test]
    fn claude_gives_descriptions_to_used_skills_first_and_skips_ones_that_do_not_fit() {
        // Base: 5 + 5 + 5 + 2 joins = 17. Budget 60 leaves 43.
        let lines = [line(5, 40, 0.0), line(5, 40, 3.0), line(5, 10, 0.0)];
        // Score 3 takes 35, leaving 8: the first zero-score line (35) does
        // not fit, the next one (5) does.
        assert_eq!(claude_allocate(&lines, 60), vec![false, true, true]);
    }

    #[test]
    fn claude_keeps_fixed_lines_whole_even_when_nothing_else_fits() {
        let mut fixed = line(5, 50, 0.0);
        fixed.fixed = true;
        assert_eq!(
            claude_allocate(&[fixed, line(5, 40, 9.0)], 10),
            vec![true, false]
        );
    }

    #[test]
    fn claude_cuts_long_descriptions_with_an_ellipsis_at_the_cap() {
        let skill = ListedSkill {
            name: "a".into(),
            plugin: None,
            description: "abcdef".into(),
            when_to_use: Some("go".into()),
            path: PathBuf::from("/a"),
            project_root: false,
            hidden: None,
        };
        assert_eq!(claude_description(&skill, 5), ("abcd\u{2026}".into(), true));
        assert_eq!(
            claude_description(&skill, 20),
            ("abcdef - go".into(), false)
        );
    }

    fn codex(name: &str, description: &str) -> CodexLine {
        CodexLine {
            name: name.into(),
            description: description.chars().collect(),
            locator: "/s/SKILL.md".into(),
        }
    }

    #[test]
    fn codex_shortens_every_description_before_it_drops_any_entry() {
        let lines = [codex("a", &"x".repeat(80)), codex("b", &"y".repeat(80))];
        let full: u64 = lines.iter().map(|l| l.cost(80, CodexLimit::Chars(0))).sum();
        let alloc = codex_allocate(&lines, CodexLimit::Chars(full - 40));
        assert_eq!(alloc, vec![Some(60), Some(60)]);
    }

    #[test]
    fn codex_omits_entries_from_the_end_when_names_alone_do_not_fit() {
        let lines = [codex("a", "x"), codex("b", "y"), codex("c", "z")];
        let name_cost = lines[0].cost(0, CodexLimit::Chars(0));
        let alloc = codex_allocate(&lines, CodexLimit::Chars(name_cost * 2));
        assert_eq!(alloc, vec![Some(0), Some(0), None]);
    }

    #[test]
    fn listing_parser_reads_multi_line_descriptions_as_one_entry() {
        let names = vec!["a".to_string(), "p:b".to_string(), "c".to_string()];
        let parsed = parse_listing("- a: one\nmore text\n- p:b\n- c (alias): two", &names);
        assert_eq!(
            parsed,
            vec![
                ("a".into(), true, 17),
                ("p:b".into(), false, 5),
                ("c".into(), true, 16)
            ]
        );
    }

    #[test]
    fn chezmoi_names_lose_their_source_attributes() {
        assert_eq!(chezmoi_target_name("private_dot_claude"), ".claude");
        assert_eq!(
            chezmoi_target_name("private_settings.json.tmpl"),
            "settings.json"
        );
        assert_eq!(chezmoi_target_name("dot_codex"), ".codex");
    }

    #[test]
    fn claude_counts_new_models_at_three_characters_per_token() {
        use crate::listing_cost::claude_chars_per_token;
        assert_eq!(claude_chars_per_token("claude-opus-5-5"), 3);
        assert_eq!(claude_chars_per_token("claude-haiku-4-5-20251001"), 4);
        assert_eq!(claude_project_slug(Path::new("/a/b.c")), "-a-b-c");
    }
}
