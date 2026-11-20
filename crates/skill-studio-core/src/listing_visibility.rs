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
    /// Description characters in the last recorded session, when one was
    /// found and the harness records them (Codex).
    pub observed_description_chars: Option<u64>,
    /// Set when another agent's invocation setting disagrees with this one,
    /// such as a skill Claude Code keeps manual-only that Codex still lists.
    pub invocation_mismatch: Option<String>,
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
    /// The costs that decide which tier the listing is in, when the
    /// harness has tiers (Codex).
    pub thresholds: Option<ListingThresholds>,
    /// Entries in listing order; hidden entries last.
    pub entries: Vec<ListingEntry>,
}

/// Which of Codex's three listing forms applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ListingTier {
    /// Every description is whole.
    AllDescribed,
    /// Every skill is listed; descriptions are cut to about the same length.
    DescriptionsShortened,
    /// No descriptions; skills past the budget are left out.
    NamesOnly,
}

/// The listing costs, in the listing's `budget_unit`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListingThresholds {
    /// The tier the budget puts the listing in.
    pub tier: ListingTier,
    /// Cost with every description whole.
    pub full_cost: u64,
    /// Cost with names and paths only.
    pub names_only_cost: u64,
    /// Cost of the skill roots table that short paths need; included in
    /// both costs above.
    pub roots_table_cost: u64,
    /// Average cost of one more name-and-path line.
    pub average_line_cost: u64,
    /// Average description characters each listed skill keeps.
    pub average_description_chars: u64,
    /// The highest budget the setting accepts.
    pub cap: Option<u64>,
    /// The tier and the distance to the next one, in plain words.
    pub summary: String,
}

/// The tier and how far the budget is from the next tier, in plain words.
fn thresholds_summary(t: &ListingThresholds, budget: u64, unit: &str, omitted: usize) -> String {
    let cap = t
        .cap
        .map(|c| format!(" The setting is capped at {c} {unit}."))
        .unwrap_or_default();
    match t.tier {
        ListingTier::AllDescribed => format!(
            "Every description is whole. {} {unit} are left before Codex starts to shorten descriptions.{cap}",
            budget - t.full_cost
        ),
        ListingTier::DescriptionsShortened => {
            let left = budget - t.names_only_cost;
            format!(
                "Every skill is listed, and descriptions are cut to about {} characters. {} more {unit} would keep every description whole. {left} {unit} are left before Codex drops all descriptions, about {} more skills.{cap}",
                t.average_description_chars,
                t.full_cost - budget,
                left / t.average_line_cost.max(1)
            )
        }
        ListingTier::NamesOnly => format!(
            "No descriptions, and {omitted} skills are left out. {} more {unit} would list every skill with a shortened description.{cap}",
            t.names_only_cost - budget
        ),
    }
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
    /// Compare Codex with this session rollout (`.codex/sessions/**/rollout-*.jsonl`)
    /// instead of the newest one. Its listing also decides which plugins
    /// Codex loaded and the alias order.
    #[serde(default)]
    #[schemars(extend("x-untrusted" = true))]
    pub codex_transcript: Option<PathBuf>,
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
        let scopes = listing_scopes(rt, req)?;
        let _folder_guard = rt.ports.history.guard_read_data_folder()?;
        let _guard = crate::ports::acquire_shared(rt.ports.leases.as_ref(), &rt.scope)?;
        let inventory = crate::ops::scan_inner(rt, ctx, &ScanRequest::default())?;
        listing_rows(rt, ctx, &inventory, &scopes, req)
    })
}

/// The global scope, plus the requested project when it is in the runtime
/// scope.
pub(crate) fn listing_scopes(
    rt: &Runtime,
    req: &ListingRequest,
) -> Result<Vec<RootScope>, CoreError> {
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
                CoreError::new(ErrorCode::InvalidRequest, "Project is not in the scope").at(path)
            })?;
        scopes.push(RootScope::Project(ProjectRef(project.lexical.clone())));
    }
    Ok(scopes)
}

/// Every harness listing for `scopes`, from an inventory the caller already
/// scanned under the shared lease.
pub(crate) fn listing_rows(
    rt: &Runtime,
    ctx: &OpContext,
    inventory: &crate::dto::Inventory,
    scopes: &[RootScope],
    req: &ListingRequest,
) -> Result<ListingReport, CoreError> {
    let mut listings = Vec::new();
    for scope in scopes {
        for facts in &rt.ports.catalog.facts {
            ctx.checkpoint()?;
            listings.push(match facts.id.as_str() {
                AgentId::CLAUDE_CODE => claude_listing(rt, inventory, facts, scope, req)?,
                AgentId::CODEX => codex_listing(rt, inventory, facts, scope, req)?,
                _ => unlimited_listing(rt, inventory, facts, scope)?,
            });
        }
    }
    Ok(ListingReport { listings })
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
        observed_description_chars: None,
        invocation_mismatch: None,
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
                observed_description_chars: None,
                invocation_mismatch: None,
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
        thresholds: None,
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
pub(crate) fn claude_width(text: &str) -> u64 {
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
pub(crate) fn claude_project_slug(path: &Path) -> String {
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
            .filter(|e| !crate::skill_sessions::temporary_claude_project(rt, &e.name))
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
pub(crate) fn parse_listing(content: &str, names: &[String]) -> Vec<(String, bool, u64)> {
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
                        observed_description_chars: None,
                        invocation_mismatch: None,
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
                observed_description_chars: None,
                invocation_mismatch: None,
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
                observed_description_chars: None,
                invocation_mismatch: None,
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
        thresholds: None,
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
    /// Absolute `SKILL.md` path, or `rN/...` once aliased.
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
        codex_cost(&format!("{}\n", self.render(chars)), limit)
    }
}

fn codex_cost(text: &str, limit: CodexLimit) -> u64 {
    match limit {
        CodexLimit::Tokens(_) => (text.len() as u64).div_ceil(4),
        CodexLimit::Chars(_) => text.chars().count() as u64,
    }
}

fn codex_limit_value(limit: CodexLimit) -> u64 {
    match limit {
        CodexLimit::Tokens(v) | CodexLimit::Chars(v) => v,
    }
}

fn with_limit(limit: CodexLimit, value: u64) -> CodexLimit {
    match limit {
        CodexLimit::Tokens(_) => CodexLimit::Tokens(value),
        CodexLimit::Chars(_) => CodexLimit::Chars(value),
    }
}

/// Description characters each line keeps, or `None` when it is omitted.
/// Mirrors `allocate_skill_lines` in Codex 0.170.0 `ext/skills/src/render.rs`.
pub(crate) fn codex_allocate(lines: &[CodexLine], limit: CodexLimit) -> Vec<Option<usize>> {
    let max = codex_limit_value(limit);
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
    let mut alloc: Vec<Option<usize>> = lines
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
        .collect();
    // The extension renderer adds `- N additional skills omitted ...` and
    // drops listed lines from the end until that line fits too.
    let mut omitted = alloc.iter().filter(|a| a.is_none()).count();
    loop {
        let marker = format!(
            "- {omitted} additional {} omitted from this bounded skills list.\n",
            if omitted == 1 { "skill" } else { "skills" }
        );
        if used + codex_cost(&marker, limit) <= max {
            break;
        }
        let Some(last) = alloc.iter().rposition(Option::is_some) else {
            break;
        };
        used -= lines[last].cost(0, limit);
        alloc[last] = None;
        omitted += 1;
    }
    alloc
}

/// `### Skills` intro when entries carry absolute paths.
const CODEX_INTRO_ABSOLUTE: &str = "A skill is a set of instructions provided through a `SKILL.md` source. Below is the list of skills that can be used. Each entry includes a name, description, and source locator. `file` locators are on the host filesystem, `executor package` locators are owned by their execution environment, `cloud package` locators are opaque package identifiers, and `custom resource` locators use their provider's access mechanism.";
/// `### Skills` intro when entries carry `rN/...` short paths.
const CODEX_INTRO_ALIASES: &str = "A skill is a set of local instructions to follow that is stored in a `SKILL.md` file. Below is the list of skills that can be used. Each entry includes a name, description, and a short path that can be expanded into an absolute path using the skill roots table.";

/// `render_available_skills_body` without skill lines.
fn codex_body(intro: &str, root_lines: &[String]) -> String {
    let mut lines = vec!["## Skills".to_string(), intro.to_string()];
    if !root_lines.is_empty() {
        lines.push("### Skill roots".into());
        lines.extend(root_lines.iter().cloned());
    }
    lines.push("### Available skills".into());
    format!("\n{}\n", lines.join("\n"))
}

/// `plugins/cache/<marketplace>` above a plugin skill root.
fn plugin_marketplace_base(path: &Path) -> Option<PathBuf> {
    let mut candidate = path;
    while let Some(parent) = candidate.parent() {
        if parent.file_name()? == "cache" && parent.parent()?.file_name()? == "plugins" {
            return Some(candidate.to_path_buf());
        }
        candidate = parent;
    }
    None
}

/// `plugins/cache/<marketplace>/<plugin>/<version>` above a plugin skill root.
fn plugin_version_base(path: &Path) -> Option<PathBuf> {
    let base = plugin_marketplace_base(path)?;
    let mut rest = path.strip_prefix(&base).ok()?.components();
    let plugin = rest.next()?;
    let version = rest.next()?;
    Some(base.join(plugin).join(version))
}

/// The alias roots, mirroring `shared_host_alias_roots`: a plugin version
/// with one skill shares its marketplace folder as the root.
fn codex_alias_roots(skill_roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut per_version: BTreeMap<PathBuf, usize> = BTreeMap::new();
    for root in skill_roots {
        if let Some(version) = plugin_version_base(root) {
            *per_version.entry(version).or_default() += 1;
        }
    }
    skill_roots
        .iter()
        .map(|root| match plugin_version_base(root) {
            Some(version) if per_version.get(&version).copied().unwrap_or(0) <= 1 => {
                plugin_marketplace_base(root).unwrap_or_else(|| root.clone())
            }
            _ => root.clone(),
        })
        .collect()
}

/// One rendering of the listing: absolute paths, or short paths plus the
/// roots table.
struct CodexRender {
    lines: Vec<CodexLine>,
    roots_cost: u64,
    alloc: Vec<Option<usize>>,
}

impl CodexRender {
    fn included(&self) -> usize {
        self.alloc.iter().filter(|a| a.is_some()).count()
    }

    fn cut_chars(&self) -> usize {
        self.lines
            .iter()
            .zip(&self.alloc)
            .map(|(l, a)| l.description.len() - a.unwrap_or(0))
            .sum()
    }

    fn cost(&self, limit: CodexLimit) -> u64 {
        self.roots_cost
            + self
                .lines
                .iter()
                .zip(&self.alloc)
                .filter_map(|(l, a)| a.map(|a| l.cost(a, limit)))
                .sum::<u64>()
    }

    /// `aliased_render_is_better`: more skills, then fewer cut characters,
    /// then a lower cost.
    fn better_than(&self, other: &Self, limit: CodexLimit) -> bool {
        (
            self.included(),
            std::cmp::Reverse(self.cut_chars()),
            std::cmp::Reverse(self.cost(limit)),
        ) > (
            other.included(),
            std::cmp::Reverse(other.cut_chars()),
            std::cmp::Reverse(other.cost(limit)),
        )
    }
}

/// One skill in the order Codex lists it.
struct CodexSkill {
    name: String,
    description: String,
    skill_md: PathBuf,
    /// The discovery root the skill was found in.
    root: PathBuf,
    /// System skills, then repository skills, then user skills.
    rank: u8,
    entry: ListingEntry,
}

/// Renders `skills` both ways and keeps the better one, as Codex does.
/// `known_roots` gives the alias order a recorded session used.
fn codex_render(skills: &[CodexSkill], limit: CodexLimit, known_roots: &[PathBuf]) -> CodexRender {
    let base_lines: Vec<CodexLine> = skills
        .iter()
        .map(|s| {
            let chars: Vec<char> = s.description.chars().collect();
            CodexLine {
                name: s.name.clone(),
                description: if chars.len() > 1024 {
                    chars[..1021].iter().copied().chain("...".chars()).collect()
                } else {
                    chars
                },
                locator: s.skill_md.to_string_lossy().into_owned(),
            }
        })
        .collect();
    let absolute = CodexRender {
        alloc: codex_allocate(&base_lines, limit),
        lines: base_lines.clone(),
        roots_cost: 0,
    };
    // Alias plan: recorded order first, then roots in listing order.
    let roots = codex_alias_roots(&skills.iter().map(|s| s.root.clone()).collect::<Vec<_>>());
    let mut plan: Vec<PathBuf> = known_roots.to_vec();
    for root in &roots {
        if !plan.contains(root) {
            plan.push(root.clone());
        }
    }
    let mut used = BTreeSet::new();
    let lines: Vec<CodexLine> = base_lines
        .into_iter()
        .map(|mut line| {
            let best = plan
                .iter()
                .enumerate()
                .filter(|(_, root)| {
                    line.locator
                        .strip_prefix(root.to_string_lossy().trim_end_matches('/'))
                        .is_some_and(|rest| rest.starts_with('/'))
                })
                .max_by_key(|(_, root)| root.as_os_str().len());
            if let Some((index, root)) = best {
                let rest =
                    line.locator[root.to_string_lossy().trim_end_matches('/').len()..].to_string();
                line.locator = format!("r{index}{rest}");
                used.insert(index);
            }
            line
        })
        .collect();
    let root_lines: Vec<String> = used
        .iter()
        .map(|i| format!("- `r{i}` = `{}`", plan[*i].display()))
        .collect();
    let roots_cost = codex_cost(&codex_body(CODEX_INTRO_ALIASES, &root_lines), limit)
        .saturating_sub(codex_cost(&codex_body(CODEX_INTRO_ABSOLUTE, &[]), limit));
    let max = codex_limit_value(limit);
    if root_lines.is_empty() || roots_cost >= max {
        return absolute;
    }
    let aliased = CodexRender {
        alloc: codex_allocate(&lines, with_limit(limit, max - roots_cost)),
        lines,
        roots_cost,
    };
    if aliased.better_than(&absolute, limit) {
        aliased
    } else {
        absolute
    }
}

/// One entry of a listing a Codex session recorded.
#[derive(Debug, Clone)]
pub(crate) struct CodexRecordedEntry {
    pub name: String,
    pub description: String,
    /// The `SKILL.md` path, with the alias expanded.
    pub skill_md: PathBuf,
    /// The alias root the path used, when it used one.
    pub root: Option<PathBuf>,
}

/// The listing one Codex session recorded.
pub(crate) struct CodexRecorded {
    pub session: ObservedSession,
    /// Working folder of the session.
    pub cwd: Option<String>,
    /// The skill roots table, in alias order.
    pub roots: Vec<PathBuf>,
    pub entries: Vec<CodexRecordedEntry>,
}

/// Parses the `<skills_instructions>` text: the roots table and the
/// `### Available skills` entries.
pub(crate) fn parse_codex_block(block: &str) -> (Vec<(String, PathBuf)>, Vec<CodexRecordedEntry>) {
    let mut roots: Vec<(String, PathBuf)> = Vec::new();
    let mut entries = Vec::new();
    let mut in_skills = false;
    for line in block.lines() {
        if line.starts_with("### Available skills") {
            in_skills = true;
            continue;
        }
        if line.starts_with("</skills_instructions>") {
            break;
        }
        let Some(rest) = line.strip_prefix("- ") else {
            continue;
        };
        if !in_skills {
            // - `r0` = `/path`
            if let Some((alias, path)) = rest
                .strip_prefix('`')
                .and_then(|r| r.split_once("` = `"))
                .and_then(|(a, p)| Some((a, p.strip_suffix('`')?)))
            {
                roots.push((alias.to_string(), PathBuf::from(path)));
            }
            continue;
        }
        let Some((head, locator)) = rest.rsplit_once(" (file: ") else {
            continue;
        };
        let locator = locator.strip_suffix(')').unwrap_or(locator);
        let (name, description) = head
            .split_once(": ")
            .unwrap_or((head.trim_end_matches(':'), ""));
        let (skill_md, root) = match locator.split_once('/') {
            Some((alias, suffix)) if roots.iter().any(|(a, _)| a == alias) => {
                let root = roots
                    .iter()
                    .find(|(a, _)| a == alias)
                    .map(|(_, p)| p.clone());
                (root.clone().unwrap_or_default().join(suffix), root)
            }
            _ => (PathBuf::from(locator), None),
        };
        entries.push(CodexRecordedEntry {
            name: name.to_string(),
            description: description.to_string(),
            skill_md,
            root,
        });
    }
    (roots, entries)
}

fn read_codex_recorded(rt: &Runtime, path: &Path) -> Option<CodexRecorded> {
    let (bytes, _) = rt.ports.fs.read_prefix(path, 4 * 1024 * 1024).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let mut cwd = None;
    let mut at = None;
    let mut model = None;
    let mut block = None;
    for line in text.lines() {
        let wanted = (cwd.is_none() && line.contains("\"session_meta\""))
            || (model.is_none() && line.contains("\"turn_context\""))
            || (block.is_none() && line.contains("<skills_instructions>"));
        if !wanted {
            continue;
        }
        let Ok(row) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let payload = &row["payload"];
        match row["type"].as_str() {
            Some("session_meta") => {
                cwd = payload["cwd"].as_str().map(str::to_string);
                at = payload["timestamp"].as_str().map(str::to_string);
            }
            Some("turn_context") => model = payload["model"].as_str().map(str::to_string),
            Some("response_item")
                if payload["type"] == "message" && payload["role"] == "developer" =>
            {
                block = payload["content"].as_array().and_then(|blocks| {
                    blocks.iter().find_map(|b| {
                        let text = b["text"].as_str()?;
                        Some(text[text.find("<skills_instructions>")?..].to_string())
                    })
                });
            }
            _ => {}
        }
        if cwd.is_some() && model.is_some() && block.is_some() {
            break;
        }
    }
    let (roots, entries) = parse_codex_block(&block?);
    Some(CodexRecorded {
        session: ObservedSession {
            transcript: path.to_path_buf(),
            at,
            model,
            listed: entries.len(),
            described: entries.iter().filter(|e| !e.description.is_empty()).count(),
        },
        cwd,
        roots: roots.into_iter().map(|(_, p)| p).collect(),
        entries,
    })
}

/// The newest listing a Codex session recorded for `scope`, skipping
/// sessions in temporary folders. A project scope needs a session in that
/// project.
fn newest_codex_recorded(rt: &Runtime, scope: &RootScope) -> Option<CodexRecorded> {
    let files = crate::skill_sessions::newest(
        crate::skill_sessions::codex_rollouts(rt),
        chrono::DateTime::<chrono::Utc>::MIN_UTC,
        usize::MAX,
    );
    files.into_iter().take(40).find_map(|path| {
        let recorded = read_codex_recorded(rt, &path)?;
        let cwd = recorded.cwd.as_deref()?;
        let fits = match scope {
            RootScope::Global => true,
            RootScope::Project(p) => Path::new(cwd) == p.0,
        };
        (fits && !crate::skill_sessions::temporary_dir(rt, cwd)).then_some(recorded)
    })
}

/// The description a `SKILL.md` gives, read the way Codex reads it.
fn codex_description(rt: &Runtime, skill_md: &Path) -> Option<String> {
    let bytes = rt.ports.fs.read_capped(skill_md, 256 * 1024).ok()?;
    crate::listing_cost::codex_frontmatter(&String::from_utf8_lossy(&bytes)).map(|(_, d)| d)
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
    let recorded = match &req.codex_transcript {
        Some(path) => {
            let rollout = path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("rollout-"))
                && path.extension().is_some_and(|e| e == "jsonl")
                && path.ancestors().any(|a| {
                    a.file_name()
                        .is_some_and(|n| n == "sessions" || n == "archived_sessions")
                        && a.parent().and_then(Path::file_name).is_some_and(|p| {
                            p == ".codex" || Some(p) == rt.scope.codex_home.file_name()
                        })
                });
            if !rollout {
                return Err(CoreError::new(
                    ErrorCode::InvalidRequest,
                    "The rollout must be a Codex session file (.codex/sessions/**/rollout-*.jsonl)",
                )
                .at(path));
            }
            Some(read_codex_recorded(rt, path).ok_or_else(|| {
                CoreError::new(
                    ErrorCode::InvalidRequest,
                    "No skill listing in this rollout",
                )
                .at(path)
            })?)
        }
        None => newest_codex_recorded(rt, scope),
    };
    let system_root = rt.scope.codex_home.join("skills/.system");
    let mut hidden = Vec::new();
    let mut skills: Vec<CodexSkill> = Vec::new();
    for skill in listed_skills(rt, inventory, facts, scope, None)?.into_values() {
        // Codex names a plugin skill `plugin:name`.
        let name = match &skill.plugin {
            Some(plugin) => format!("{plugin}:{}", skill.name),
            None => skill.name.clone(),
        };
        if skill.hidden.is_some() {
            hidden.push(hidden_entry(&skill, name));
            continue;
        }
        // Which installed plugins Codex loads depends on its marketplaces
        // and the app that starts it. A plugin with no skill in the newest
        // session's listing was not loaded there.
        let plugin_dir = skill
            .plugin
            .as_ref()
            .and_then(|_| plugin_version_base(skill.path.parent()?))
            .and_then(|version| version.parent().map(Path::to_path_buf));
        if let (Some(dir), Some(recorded)) = (&plugin_dir, &recorded) {
            if !recorded.entries.iter().any(|e| e.skill_md.starts_with(dir)) {
                let mut entry = hidden_entry(&skill, name);
                entry.hidden_reason = Some("plugin not loaded in the newest Codex session".into());
                hidden.push(entry);
                continue;
            }
        }
        let chars = skill.description.chars().count() as u64;
        skills.push(CodexSkill {
            rank: if skill.project_root { 2 } else { 3 },
            root: skill.path.parent().map(Path::to_path_buf).unwrap_or_default(),
            skill_md: skill.path.join("SKILL.md"),
            entry: ListingEntry {
                name: name.clone(),
                path: Some(skill.path.clone()),
                state: ListingState::Described,
                hidden_reason: None,
                shown_description_chars: chars,
                full_description_chars: chars,
                usage_score: None,
                observed: None,
                scanned: true,
                observed_description_chars: None,
                invocation_mismatch: skill.claude_manual.then(|| {
                    format!(
                        "Manual-only in Claude Code (disable-model-invocation), but Codex lists it: Codex reads only policy.allow_implicit_invocation in agents/openai.yaml. Run `skill-studio invocation {} --manual` to set both.",
                        skill.name
                    )
                }),
            },
            name,
            description: skill.description,
        });
    }
    // Entries Skill Studio does not scan (Codex system skills, skills in
    // nested folders) come from the newest recorded session, with the
    // description read from their SKILL.md.
    let mut unscanned = 0usize;
    if let Some(recorded) = &recorded {
        let known: BTreeSet<PathBuf> = skills
            .iter()
            .flat_map(|s| {
                let real = rt.ports.fs.canonicalize(&s.skill_md).ok();
                std::iter::once(s.skill_md.clone()).chain(real)
            })
            .chain(
                hidden
                    .iter()
                    .filter_map(|h| h.path.as_ref().map(|p| p.join("SKILL.md"))),
            )
            .collect();
        // Repository skills of the recorded session (under its working
        // folder) are not global entries. A session started in the home
        // folder has none.
        let home_agents = rt.scope.global_root_path(Path::new(".agents"));
        // A comparison with one chosen session keeps its repository skills.
        let repo = match scope {
            RootScope::Global if req.codex_transcript.is_none() => {
                recorded.cwd.as_deref().map(PathBuf::from).filter(|cwd| {
                    !home_agents.starts_with(cwd) && !rt.scope.codex_home.starts_with(cwd)
                })
            }
            _ => None,
        };
        // Plugins with a scanned skill: the scanned version is the one
        // installed now; a recorded older version is not listed again.
        let scanned_plugins: BTreeSet<PathBuf> = skills
            .iter()
            .filter_map(|s| plugin_version_base(s.skill_md.parent()?.parent()?))
            .filter_map(|v| v.parent().map(Path::to_path_buf))
            .collect();
        for entry in &recorded.entries {
            if entry
                .skill_md
                .parent()
                .and_then(Path::parent)
                .and_then(plugin_version_base)
                .and_then(|v| v.parent().map(Path::to_path_buf))
                .is_some_and(|dir| scanned_plugins.contains(&dir))
            {
                continue;
            }
            let real = rt.ports.fs.canonicalize(&entry.skill_md).ok();
            if known.contains(&entry.skill_md) || real.as_ref().is_some_and(|r| known.contains(r)) {
                continue;
            }
            // A repository skill of the recorded session is not a global entry.
            if repo.as_ref().is_some_and(|r| entry.skill_md.starts_with(r)) {
                continue;
            }
            let description =
                codex_description(rt, &entry.skill_md).unwrap_or_else(|| entry.description.clone());
            let chars = description.chars().count() as u64;
            unscanned += 1;
            skills.push(CodexSkill {
                name: entry.name.clone(),
                rank: if entry.skill_md.starts_with(&system_root) {
                    0
                } else {
                    3
                },
                root: entry.root.clone().unwrap_or_else(|| {
                    entry
                        .skill_md
                        .parent()
                        .and_then(Path::parent)
                        .map(Path::to_path_buf)
                        .unwrap_or_default()
                }),
                skill_md: entry.skill_md.clone(),
                entry: ListingEntry {
                    name: entry.name.clone(),
                    path: entry.skill_md.parent().map(Path::to_path_buf),
                    state: ListingState::Described,
                    hidden_reason: None,
                    shown_description_chars: chars,
                    full_description_chars: chars,
                    usage_score: None,
                    observed: None,
                    scanned: false,
                    observed_description_chars: None,
                    invocation_mismatch: None,
                },
                description,
            });
        }
    }
    // Catalog order: system, then repository, then user skills; by name,
    // then path.
    skills.sort_by(|a, b| (a.rank, &a.name, &a.skill_md).cmp(&(b.rank, &b.name, &b.skill_md)));
    let known_roots = recorded
        .as_ref()
        .map(|r| r.roots.clone())
        .unwrap_or_default();
    let render = codex_render(&skills, budget.limit, &known_roots);
    let mut entries: Vec<ListingEntry> = Vec::new();
    for ((skill, line), alloc) in skills.iter().zip(&render.lines).zip(&render.alloc) {
        let mut entry = skill.entry.clone();
        let full = line.description.len();
        entry.state = match alloc {
            None => ListingState::Omitted,
            Some(0) if full > 0 => ListingState::NameOnly,
            Some(n) if *n < full => ListingState::Truncated,
            Some(_) => ListingState::Described,
        };
        entry.shown_description_chars = alloc.unwrap_or(0) as u64;
        entries.push(entry);
    }
    if let Some(recorded) = &recorded {
        let observed: BTreeMap<&str, u64> = recorded
            .entries
            .iter()
            .map(|e| (e.name.as_str(), e.description.chars().count() as u64))
            .collect();
        for entry in entries.iter_mut().chain(hidden.iter_mut()) {
            let chars = observed.get(entry.name.as_str()).copied();
            entry.observed_description_chars = chars;
            entry.observed = Some(match chars {
                None => ListingState::Hidden,
                Some(0) => ListingState::NameOnly,
                Some(n) if n < entry.full_description_chars.min(1024) => ListingState::Truncated,
                Some(_) => ListingState::Described,
            });
        }
    }
    let limit = budget.limit;
    let (max, unit) = match limit {
        CodexLimit::Tokens(v) => (v, "tokens"),
        CodexLimit::Chars(v) => (v, "characters"),
    };
    let full_size: u64 = render.roots_cost
        + render
            .lines
            .iter()
            .map(|l| l.cost(l.description.len(), limit))
            .sum::<u64>();
    let names_only: u64 =
        render.roots_cost + render.lines.iter().map(|l| l.cost(0, limit)).sum::<u64>();
    let listed = render.included().max(1) as u64;
    let mut thresholds = ListingThresholds {
        tier: if full_size <= max {
            ListingTier::AllDescribed
        } else if names_only <= max {
            ListingTier::DescriptionsShortened
        } else {
            ListingTier::NamesOnly
        },
        full_cost: full_size,
        names_only_cost: names_only,
        roots_table_cost: render.roots_cost,
        average_line_cost: (names_only - render.roots_cost)
            .div_ceil(render.lines.len().max(1) as u64),
        average_description_chars: render
            .alloc
            .iter()
            .flatten()
            .map(|a| *a as u64)
            .sum::<u64>()
            / listed,
        cap: Some(10_000),
        summary: String::new(),
    };
    thresholds.summary = thresholds_summary(
        &thresholds,
        max,
        unit,
        render.lines.len() - render.included(),
    );
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
    let mut notes = vec![
        "Codex lists every skill with its description when they all fit. When they do not, it keeps every skill and cuts all descriptions to about the same length. When even names and paths do not fit, it drops every description and leaves out the skills at the end of the list.".to_string(),
        "Paths are short (r0/name/SKILL.md) when the skill roots table makes the listing cheaper; the table is charged too.".to_string(),
        "skills.max_context_tokens is capped at 10000 tokens. Codex ignores disable-model-invocation; only policy.allow_implicit_invocation: false in agents/openai.yaml hides a skill.".to_string(),
    ];
    if unscanned > 0 {
        notes.push(format!(
            "{unscanned} entries that Skill Studio does not scan (Codex system skills, skills in nested folders) come from the newest recorded session; their descriptions are read from their SKILL.md."
        ));
    }
    if recorded.is_none() {
        notes.push("No recorded Codex session was found, so Codex system skills are not counted and the alias order is inferred.".into());
    }
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
        notes,
        setting: Some(setting),
        observed_session: recorded.map(|r| r.session),
        thresholds: Some(thresholds),
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
            claude_manual: false,
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
    fn codex_omits_entries_from_the_end_and_makes_room_for_the_omission_line() {
        // Names longer than the omission line, so it costs less than one entry.
        let lines = [
            codex(&"a".repeat(70), "x"),
            codex(&"b".repeat(70), "y"),
            codex(&"c".repeat(70), "z"),
        ];
        let name_cost = lines[0].cost(0, CodexLimit::Chars(0));
        let marker = "- 1 additional skill omitted from this bounded skills list.\n"
            .chars()
            .count() as u64;
        let alloc = codex_allocate(&lines, CodexLimit::Chars(name_cost * 2 + marker));
        assert_eq!(alloc, vec![Some(0), Some(0), None]);
        // Without room for the omission line, listed lines go until it fits.
        let alloc = codex_allocate(&lines, CodexLimit::Chars(name_cost * 2 + marker - 1));
        assert_eq!(alloc, vec![Some(0), None, None]);
    }

    #[test]
    fn codex_quotes_a_value_with_a_colon_but_rejects_a_repeated_key() {
        let repaired = crate::listing_cost::codex_frontmatter(
            "---\nname: a\ndescription: Build for AWS:  ECS\n---\nbody",
        );
        assert_eq!(
            repaired,
            Some((Some("a".into()), "Build for AWS: ECS".into()))
        );
        let repeated = crate::listing_cost::codex_frontmatter(
            "---\nname: a\ndescription: d\nmetadata:\n  x: 1\nmetadata:\n  y: 2\n---\n",
        );
        assert_eq!(repeated, None);
    }

    #[test]
    fn codex_shares_the_marketplace_root_for_a_plugin_with_one_skill() {
        let cache = PathBuf::from("/h/.codex/plugins/cache/m");
        let one = cache.join("p/1.0/skills");
        let many = cache.join("q/2.0/skills");
        let roots = codex_alias_roots(&[
            one,
            many.clone(),
            many.clone(),
            PathBuf::from("/h/.agents/skills"),
        ]);
        assert_eq!(
            roots,
            vec![
                cache,
                many.clone(),
                many,
                PathBuf::from("/h/.agents/skills")
            ]
        );
    }

    #[test]
    fn codex_block_parser_expands_short_paths_with_the_roots_table() {
        let (roots, entries) = parse_codex_block(
            "### Skill roots\n- `r0` = `/h/.agents/skills`\n### Available skills\n- a: Does a (file: r0/a/SKILL.md)\n- p:b: (file: /abs/b/SKILL.md)\n</skills_instructions>",
        );
        assert_eq!(roots.len(), 1);
        assert_eq!(
            entries[0].skill_md,
            PathBuf::from("/h/.agents/skills/a/SKILL.md")
        );
        assert_eq!(entries[0].description, "Does a");
        assert_eq!(
            (entries[1].name.as_str(), entries[1].description.as_str()),
            ("p:b", "")
        );
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
