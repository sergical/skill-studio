//! Per-skill use counts over a rolling window of days, shared by the CLI's
//! `usage` command and the MCP server's `skill_usage` tool so both answer
//! "which skills did I not use?" the same way. Every window sits inside the
//! last [`HISTORY_DAYS`] days: older history is not counted.
//!
//! The desktop app owns the use cache (`skill-uses.json` in its app data
//! folder). This module only ever reads it: it seeds an in-memory
//! [`SkillInvocationIndex`] from the cache so a refresh re-reads only what
//! changed since the app last ran, and never saves the result back. The CLI
//! can save a separate caller-owned cache after it finishes the read.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Serialize;
use skill_studio_core::discovery_sources::DiscoverySources;
use skill_studio_core::dto::{Completeness, Inventory};
use skill_studio_core::identity::RootKind;
use skill_studio_core::ops::{OpStatus, Outcome};
use skill_studio_core::skill_uses::{history_start, SkillInvocation, SkillTrigger, HISTORY_DAYS};

use crate::fs::RealFs;
use crate::skill_uses::SkillInvocationIndex;

/// The window `usage` and `skill_usage` use when the caller names none: the
/// whole history.
pub const DEFAULT_USAGE_DAYS: u32 = HISTORY_DAYS;

/// The desktop app's use cache under the platform data folder (`dirs::data_dir()`,
/// `~/Library/Application Support` on macOS): Tauri's `app_data_dir()` for
/// the `com.skillstudio.app` identifier, joined with the file name the app
/// writes.
pub fn desktop_usage_cache_path(data_dir: &Path) -> PathBuf {
    data_dir.join("com.skillstudio.app").join("skill-uses.json")
}

/// The CLI and MCP cache beside the desktop cache. Only the desktop app writes the desktop cache.
pub fn cli_usage_cache_path(desktop_cache: &Path) -> PathBuf {
    desktop_cache.with_file_name("cli-skill-uses.json")
}

/// One installed skill's uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct SkillUsageRow {
    /// Distinct (harness, normalized session identity) pairs for the standalone review-date report.
    #[serde(skip_serializing)]
    #[schemars(skip)]
    pub sessions: usize,
    /// Uses in the last `days` days, by trigger.
    pub triggers: BTreeMap<String, u32>,
    /// The skill's name.
    #[schemars(extend("x-untrusted" = true))]
    pub skill: String,
    /// Uses inside the report window: `since`..`until`, or the last `days`.
    pub recent_uses: u32,
    /// The most recent use in the last 30 days, outside the window too,
    /// RFC 3339, or `None` when not used in that time.
    pub last_used: Option<String>,
    /// Uses in the last `days` days, by agent id.
    pub agents: BTreeMap<String, u32>,
}

/// Uses for every installed, not parked skill over one window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct UsageReport {
    /// Roots or files whose history was unavailable or skipped.
    pub observations: Vec<crate::skill_uses::UsageObservation>,
    /// Window used by the tally.
    #[serde(flatten)]
    pub window: skill_studio_core::usage_window::UsageWindow,
    /// Skills not used in the window first, by name; then the rest, most
    /// used first.
    pub rows: Vec<SkillUsageRow>,
    /// Names of the skills with no use in the window, in `rows` order.
    /// Empty when the caller asked for the top rows only.
    #[schemars(extend("x-untrusted" = true))]
    pub unused: Vec<String>,
    /// Set when some session history could not be read, so a skill in
    /// `unused` may still have uses that were not counted, or when the scan
    /// skipped a root, so `rows` may miss a skill.
    pub partial: bool,
}

impl Outcome for UsageReport {
    const LISTS: &'static [&'static str] = &["/rows", "/unused"];

    fn status(&self) -> OpStatus {
        if self.partial {
            OpStatus::Partial
        } else {
            OpStatus::Ok
        }
    }
}

/// An in-memory use index that stays alive across reports, so a
/// long-running caller (the MCP server) re-reads only what changed between
/// two calls.
#[derive(Debug, Clone, Default)]
pub struct SkillUsage {
    index: SkillInvocationIndex,
    loaded_index: SkillInvocationIndex,
}

/// Optional exact matches over recorded history. Paths use lexical path equality.
#[derive(Debug, Clone, Default)]
pub struct UsageFilter {
    /// Exact recorded harness id, or all harnesses.
    pub harness: Option<String>,
    /// Exact recorded project paths, or all projects when empty.
    pub projects: Vec<PathBuf>,
    /// Start of the window. Replaces `days` before the end when set.
    pub since: Option<DateTime<Utc>>,
    /// End of the window, not included. Absent counts up to now and later.
    pub until: Option<DateTime<Utc>>,
    /// Keep only this many rows, most used first, and no unused skills.
    pub top: Option<usize>,
}

impl UsageFilter {
    /// Rejects a window that holds no time (`since` at or after `until`, or
    /// after `now` when there is no `until`), which would report every skill
    /// as unused. Also rejects a window that starts before the history
    /// ([`HISTORY_DAYS`] days before `now`, to the second), by `days`, by
    /// `since` or by an old `until`, which would report uses that are not
    /// counted.
    pub fn check_window(&self, days: u32, now: DateTime<Utc>) -> Result<(), String> {
        let end = self.until.unwrap_or(now);
        if let Some(since) = self.since.filter(|since| *since >= end) {
            return Err(format!(
                "the usage window is empty: since {} is not before {}",
                since.to_rfc3339(),
                end.to_rfc3339()
            ));
        }
        let history = history_start(now);
        let too_old = || {
            format!(
                "Skill Studio counts only the last {HISTORY_DAYS} days of history: \
                 the window must start at {} or later",
                history.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
            )
        };
        // Checked first, so `days` stays small enough to subtract from a date.
        if self.since.is_none() && days > HISTORY_DAYS {
            return Err(too_old());
        }
        // `until` can be any date the parser takes, down to chrono's minimum, so the
        // subtraction is checked; a start before that minimum is before the history too.
        let start = match self.since {
            Some(since) => since,
            None => end
                .checked_sub_signed(chrono::Duration::days(i64::from(days)))
                .ok_or_else(too_old)?,
        };
        if start < history {
            return Err(too_old());
        }
        Ok(())
    }
}

/// Reads a window bound: `YYYY-MM-DD` (start of that day, UTC) or RFC 3339.
pub fn parse_usage_time(text: &str) -> Result<DateTime<Utc>, String> {
    if let Ok(day) = chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d") {
        return Ok(day.and_time(chrono::NaiveTime::MIN).and_utc());
    }
    DateTime::parse_from_rfc3339(text)
        .map(|at| at.with_timezone(&Utc))
        .map_err(|_| format!("`{text}` is not a date (YYYY-MM-DD) or an RFC 3339 time"))
}

impl SkillUsage {
    /// Uses the runtime's resolved Codex root instead of ambient `CODEX_HOME`.
    pub fn set_codex_home(&mut self, root: PathBuf) {
        self.loaded_index.set_codex_home(root.clone());
        self.index.set_codex_home(root);
    }

    /// Seeds the index from `cache` when given. Never writes the file.
    pub fn load_read_only(cache: Option<&Path>) -> Self {
        let index = cache
            .map(SkillInvocationIndex::load_read_only)
            .unwrap_or_default();
        Self {
            loaded_index: index.clone(),
            index,
        }
    }

    /// Atomically saves a caller-owned cache. The desktop cache must not be used here.
    pub fn save_cache(&self, path: &Path) -> Result<(), String> {
        if path.exists() && self.index == self.loaded_index {
            return Ok(());
        }
        let temporary = path.with_extension(format!("{}.tmp", ulid::Ulid::generate()));
        let result = self
            .index
            .save(&temporary)
            .and_then(|()| std::fs::rename(&temporary, path).map_err(|error| error.to_string()));
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
            let _ = std::fs::remove_file(temporary.with_extension("tmp.tmp"));
        }
        result
    }

    /// Refreshes the index from `home`'s agent session history, in memory
    /// only, and counts uses of every skill in `inventory` that is not
    /// parked over the `days` days before `now`.
    pub fn report(
        &mut self,
        home: &Path,
        inventory: &Inventory,
        days: u32,
        now: DateTime<Utc>,
    ) -> UsageReport {
        self.report_filtered(home, inventory, days, now, &UsageFilter::default())
    }

    /// Reports uses that match the optional harness and project filters.
    pub fn report_filtered(
        &mut self,
        home: &Path,
        inventory: &Inventory,
        days: u32,
        now: DateTime<Utc>,
        selection: &UsageFilter,
    ) -> UsageReport {
        self.report_named(
            home,
            inventory,
            days,
            now,
            selection,
            &active_skill_names(inventory),
        )
    }

    pub(crate) fn report_named(
        &mut self,
        home: &Path,
        inventory: &Inventory,
        days: u32,
        now: DateTime<Utc>,
        selection: &UsageFilter,
        installed: &BTreeSet<String>,
    ) -> UsageReport {
        let sources = DiscoverySources::read(&RealFs::new(), home);
        let refresh = self.index.refresh_all(home, &sources);

        let mut observations = self.index.observations(home, &sources);
        observations.extend(refresh.observations);
        if inventory.completeness == Completeness::Partial {
            observations.push(crate::skill_uses::UsageObservation {
                path: home.to_path_buf(),
                reason: format!(
                    "Installed skill scan is partial: {}",
                    inventory
                        .observations
                        .iter()
                        .map(|o| o.message.as_str())
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect::<Vec<_>>()
                        .join("; ")
                ),
            });
        }
        observations.sort();
        observations.dedup();
        let end = selection.until.unwrap_or(now);
        let cutoff = selection
            .since
            .unwrap_or_else(|| end - chrono::Duration::days(i64::from(days)));
        let history = history_start(now);
        let mut by_skill: BTreeMap<&str, Tally> = installed
            .iter()
            .map(|name| (name.as_str(), Tally::default()))
            .collect();
        let selected: Vec<_> = self
            .index
            .all_uses()
            .filter(|u| u.at >= history)
            .filter(|u| {
                selection.harness.as_ref().is_none_or(|h| {
                    skill_studio_core::identity::AgentId::parse_harness(h)
                        .is_ok_and(|id| id.as_str() == u.harness)
                }) && (selection.projects.is_empty()
                    || selection.projects.iter().any(|p| {
                        u.project_path
                            .as_deref()
                            .is_some_and(|recorded| Path::new(recorded) == p)
                    }))
            })
            .collect();
        for use_ in selected.iter().filter(|u| sources.is_enabled(&u.harness)) {
            if let Some(tally) =
                installed_name(&use_.skill, installed).and_then(|name| by_skill.get_mut(name))
            {
                if tally.last_used.is_none_or(|last| use_.at > last) {
                    tally.last_used = Some(use_.at);
                }
            }
        }
        for use_ in session_uses(
            selected
                .into_iter()
                .filter(|u| u.at >= cutoff && selection.until.is_none_or(|until| u.at < until)),
            installed,
            &sources,
        ) {
            let Some(tally) =
                installed_name(&use_.skill, installed).and_then(|name| by_skill.get_mut(name))
            else {
                continue;
            };
            tally.recent += 1;
            *tally.agents.entry(use_.harness.clone()).or_insert(0) += 1;
            if let Some(session) = use_.use_session.as_ref().or(use_.session.as_ref()) {
                tally
                    .sessions
                    .insert((use_.harness.clone(), session.clone()));
            }
            let trigger = match use_.trigger {
                skill_studio_core::skill_uses::SkillTrigger::User => "user",
                skill_studio_core::skill_uses::SkillTrigger::Agent => "agent",
                skill_studio_core::skill_uses::SkillTrigger::FileRead => "file_read",
            };
            *tally.triggers.entry(trigger.to_string()).or_default() += 1;
        }

        let mut rows: Vec<SkillUsageRow> = by_skill
            .into_iter()
            .map(|(skill, tally)| SkillUsageRow {
                skill: skill.to_string(),
                sessions: tally.sessions.len(),
                triggers: ["user", "agent", "file_read", "unknown"]
                    .into_iter()
                    .map(|key| {
                        (
                            key.to_string(),
                            tally.triggers.get(key).copied().unwrap_or(0),
                        )
                    })
                    .collect(),
                recent_uses: tally.recent,
                last_used: tally.last_used.map(|at| at.to_rfc3339()),
                agents: tally.agents,
            })
            .collect();
        // Stable sort over name-ordered rows: unused skills keep name order.
        rows.sort_by(|a, b| {
            (a.recent_uses > 0)
                .cmp(&(b.recent_uses > 0))
                .then(b.recent_uses.cmp(&a.recent_uses))
        });
        let mut unused: Vec<String> = rows
            .iter()
            .filter(|row| row.recent_uses == 0)
            .map(|row| row.skill.clone())
            .collect();
        if let Some(top) = selection.top {
            unused.clear();
            rows.retain(|row| row.recent_uses > 0);
            rows.truncate(top);
        }

        UsageReport {
            window: skill_studio_core::usage_window::UsageWindow::new(
                days,
                selection.since,
                selection.until,
            ),
            rows,
            unused,
            partial: !observations.is_empty(),
            observations,
        }
    }
}

fn session_uses<'a>(
    uses: impl IntoIterator<Item = &'a SkillInvocation>,
    installed: &BTreeSet<String>,
    sources: &DiscoverySources,
) -> Vec<&'a SkillInvocation> {
    let mut kept = Vec::new();
    let mut by_session: BTreeMap<(&str, &str, &str), &SkillInvocation> = BTreeMap::new();
    for use_ in uses.into_iter().filter(|u| sources.is_enabled(&u.harness)) {
        let Some(skill) = installed_name(&use_.skill, installed) else {
            continue;
        };
        let Some(session) = use_.use_session.as_deref().or(use_.session.as_deref()) else {
            kept.push(use_);
            continue;
        };
        let priority =
            |u: &SkillInvocation| (u.trigger == SkillTrigger::FileRead, std::cmp::Reverse(u.at));
        by_session
            .entry((use_.harness.as_str(), session, skill))
            .and_modify(|current| {
                if priority(use_) < priority(current) {
                    *current = use_;
                }
            })
            .or_insert(use_);
    }
    kept.extend(by_session.into_values());
    kept
}

#[derive(Default)]
struct Tally {
    sessions: BTreeSet<(String, String)>,
    triggers: BTreeMap<String, u32>,
    recent: u32,
    last_used: Option<DateTime<Utc>>,
    agents: BTreeMap<String, u32>,
}

/// One-shot report for a caller that exits right after (the CLI): loads the
/// desktop cache read-only when `cache` is given, refreshes in memory, and
/// drops the index.
pub fn usage_report(
    home: &Path,
    inventory: &Inventory,
    days: u32,
    cache: Option<&Path>,
) -> UsageReport {
    SkillUsage::load_read_only(cache).report(home, inventory, days, Utc::now())
}

/// Names of skills with at least one deployment outside the parked root: a
/// parked skill is already off for every agent, so it is not a candidate
/// for "unused".
fn active_skill_names(inventory: &Inventory) -> BTreeSet<String> {
    inventory
        .skills
        .iter()
        .filter(|skill| {
            skill
                .deployments
                .iter()
                .any(|deployment| deployment.root.kind != RootKind::Parked)
        })
        .map(|skill| skill.name.0.clone())
        .collect()
}

/// The installed name a recorded use counts toward: the name itself, or the
/// `base` of a plugin-qualified `prefix:base` name, the same rule
/// the desktop's known-skill filter applies.
fn installed_name<'a>(recorded: &'a str, installed: &BTreeSet<String>) -> Option<&'a str> {
    if installed.contains(recorded) {
        return Some(recorded);
    }
    recorded
        .rsplit_once(':')
        .map(|(_, base)| base)
        .filter(|base| installed.contains(*base))
}

/// Reads history without saving a cache, then builds the shared core library table.
pub fn library_report(
    rt: &skill_studio_core::ports::Runtime,
    ctx: &skill_studio_core::ports::OpContext,
    request: &skill_studio_core::library::LibraryRequest,
    cache: Option<&Path>,
) -> Result<skill_studio_core::library::LibraryReport, skill_studio_core::CoreError> {
    let filter = UsageFilter {
        since: request.since,
        until: request.until,
        ..Default::default()
    };
    let now = rt.ports.clock.now();
    let days = request.days.unwrap_or(DEFAULT_USAGE_DAYS);
    filter.check_window(days, now).map_err(|message| {
        skill_studio_core::CoreError::new(skill_studio_core::ErrorCode::InvalidRequest, message)
    })?;
    let inventory = skill_studio_core::ops::scan_for_usage(rt, ctx)?;
    let report = SkillUsage::load_read_only(cache).report_filtered(
        &rt.scope.home.canonical,
        &inventory,
        days,
        now,
        &filter,
    );
    let counts: Vec<_> = report
        .rows
        .into_iter()
        .map(|row| skill_studio_core::library::LibraryUses {
            name: row.skill,
            uses: row.recent_uses,
            agents: row.agents,
        })
        .collect();
    skill_studio_core::library::library(rt, ctx, &inventory, &counts, request, report.partial)
}

#[cfg(test)]
mod tests {
    use super::*;
    use skill_studio_core::dto::ScanRequest;
    use skill_studio_core::harness::HarnessCatalog;
    use skill_studio_core::identity::CorrelationId;
    use skill_studio_core::ops;
    use skill_studio_core::ports::{OpContext, Runtime};
    use skill_studio_core::scope::ProjectSelection;
    use skill_studio_core::RuntimeScope;
    use std::fs;
    use std::sync::Arc;

    /// A fixed "now" keeps every window boundary in these tests exact.
    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-30T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn days_ago(days: i64) -> String {
        (now() - chrono::Duration::days(days)).to_rfc3339()
    }

    fn write_skill(dir: &Path, name: &str) {
        let skill_dir = dir.join(name);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: A test skill.\n---\nBody.\n"),
        )
        .unwrap();
    }

    /// One Claude Code transcript line where the model called the Skill tool.
    fn claude_skill_line(skill: &str, at: &str) -> String {
        format!(
            r#"{{"type":"assistant","sessionId":"{at}","timestamp":"{at}","cwd":"/work","message":{{"content":[{{"type":"tool_use","name":"Skill","input":{{"skill":"{skill}"}}}}]}}}}"#
        )
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        home: PathBuf,
    }

    /// A home with three skills and one Claude Code transcript:
    /// - `gamma` (universal root) used 3 and 20 days ago, once through a
    ///   plugin-qualified `tools:gamma` name;
    /// - `idle` (Claude Code root) used once, 45 days ago, before the history;
    /// - `resting` (parked root) used 2 days ago, but parked;
    /// - `ghost`, used 1 day ago, is not installed anywhere.
    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().canonicalize().unwrap();
        write_skill(&home.join(".agents/skills"), "gamma");
        write_skill(&home.join(".claude/skills"), "idle");
        write_skill(&home.join(".agents/skills-parked"), "resting");

        let transcript_dir = home.join(".claude/projects/-work");
        fs::create_dir_all(&transcript_dir).unwrap();
        let lines = [
            claude_skill_line("gamma", &days_ago(3)),
            claude_skill_line("tools:gamma", &days_ago(20)),
            claude_skill_line("idle", &days_ago(45)),
            claude_skill_line("resting", &days_ago(2)),
            claude_skill_line("ghost", &days_ago(1)),
        ];
        fs::write(
            transcript_dir.join("session.jsonl"),
            format!("{}\n", lines.join("\n")),
        )
        .unwrap();
        Fixture { _dir: dir, home }
    }

    fn scan(home: &Path) -> Inventory {
        let mut scope = RuntimeScope::live(home.to_path_buf(), home.join(".skill-studio/history"));
        scope.projects = ProjectSelection::Explicit { paths: Vec::new() };
        let ports = crate::default_ports(
            home.join(".skill-studio/leases"),
            Arc::new(HarnessCatalog::builtin()),
        );
        let rt = Runtime::new(&scope, ports).unwrap();
        let ctx = OpContext::uncancellable(CorrelationId("usage-test".into()));
        ops::scan(&rt, &ctx, &ScanRequest::default()).unwrap()
    }

    fn row<'a>(report: &'a UsageReport, skill: &str) -> &'a SkillUsageRow {
        report
            .rows
            .iter()
            .find(|row| row.skill == skill)
            .expect("every installed skill has a row")
    }

    #[test]
    fn reporting_resumed_history_rebuilds_old_cache_instead_of_returning_double_counts() {
        let fixture = fixture();
        let inventory = scan(&fixture.home);
        let path = fixture.home.join(".claude/projects/-work/resumed.jsonl");
        let mut record: serde_json::Value =
            serde_json::from_str(&claude_skill_line("gamma", &days_ago(1))).unwrap();
        record["uuid"] = "resumed-use".into();
        record["sessionId"] = "resumed-session".into();
        fs::write(&path, (record.to_string() + "\n").repeat(2)).unwrap();
        let cache = fixture.home.join("usage-cache.json");
        let mut index = SkillInvocationIndex::default();
        index.refresh_all(&fixture.home, &DiscoverySources::default());
        index.save(&cache).unwrap();
        let mut old: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&cache).unwrap()).unwrap();
        old["reader_version"] = 3.into();
        for file in old["files"].as_object_mut().unwrap().values_mut() {
            file["context"]
                .as_object_mut()
                .unwrap()
                .remove("claude_records");
        }
        let doubled = skill_studio_core::skill_uses::parse_claude_code_uses(
            &fs::read_to_string(&path).unwrap(),
        );
        old["files"][path.to_str().unwrap()]["uses"] = serde_json::to_value(doubled).unwrap();
        fs::write(&cache, serde_json::to_string(&old).unwrap()).unwrap();
        let original = fs::read(&cache).unwrap();
        let report =
            SkillUsage::load_read_only(Some(&cache)).report(&fixture.home, &inventory, 30, now());
        assert_eq!(
            row(&report, "gamma").recent_uses,
            3,
            "CLI and MCP must discard stale doubled counts, and the resumed use counts once"
        );
        assert_eq!(
            fs::read(&cache).unwrap(),
            original,
            "read-only reports must leave the desktop cache untouched"
        );
    }

    #[test]
    fn usage_legacy_harness_filter_counts_sessions_instead_of_returning_zero() {
        let fixture = fixture();
        let database = fixture.home.join(".local/share/opencode/opencode.db");
        fs::create_dir_all(database.parent().unwrap()).unwrap();
        let connection = rusqlite::Connection::open(&database).unwrap();
        connection.execute_batch(
            "CREATE TABLE session_message (id TEXT PRIMARY KEY, session_id TEXT, type TEXT, time_created INTEGER, time_updated INTEGER, data TEXT);
             CREATE TABLE session (id TEXT PRIMARY KEY, directory TEXT);
             INSERT INTO session VALUES ('s1', '/work');"
        ).unwrap();
        connection
            .execute(
                "INSERT INTO session_message VALUES ('m1', 's1', 'skill', ?1, ?1, ?2)",
                rusqlite::params![
                    (now() - chrono::Duration::days(1)).timestamp_millis(),
                    r#"{"skill":"gamma"}"#
                ],
            )
            .unwrap();
        drop(connection);
        let inventory = scan(&fixture.home);
        let report = SkillUsage::default().report_filtered(
            &fixture.home,
            &inventory,
            30,
            now(),
            &UsageFilter {
                harness: Some("open-code".into()),
                ..Default::default()
            },
        );
        assert_eq!(row(&report, "gamma").sessions, 1);
    }

    #[test]
    fn review_due_returns_report_when_cache_parent_is_a_file_instead_of_failing_save() {
        let fixture = fixture();
        let mut scope = RuntimeScope::live(
            fixture.home.clone(),
            fixture.home.join(".skill-studio/history"),
        );
        scope.projects = ProjectSelection::Explicit { paths: Vec::new() };
        let rt = Runtime::new(
            &scope,
            crate::default_ports(
                fixture.home.join(".skill-studio/leases"),
                Arc::new(HarnessCatalog::builtin()),
            ),
        )
        .unwrap();
        let blocked = fixture.home.join("blocked");
        fs::write(&blocked, "file").unwrap();
        let ctx = OpContext::uncancellable(CorrelationId("review-cache".into()));
        assert!(
            crate::review_due_report(&rt, &ctx, Some(&blocked.join("skill-uses.json"))).is_ok()
        );
    }

    /// Flow: report a 30-day window over the fixture home.
    /// Expectation: `gamma` has 2 uses by claude-code (the plugin-qualified
    /// use folds into it); `idle`'s only use is before the history, so it has
    /// no use and no `last_used`, and is the only `unused` name, listed first;
    /// the parked `resting` and the uninstalled `ghost` get no row.
    /// Failure means the CLI and MCP would name the wrong skills as safe to
    /// park.
    #[test]
    fn a_thirty_day_report_counts_recent_uses_and_lists_idle_skills_first() {
        let fixture = fixture();
        let inventory = scan(&fixture.home);

        let report = SkillUsage::default().report(&fixture.home, &inventory, 30, now());

        assert_eq!(report.window.days, Some(30));
        assert!(!report.partial, "absent history is complete: {report:?}");
        let names: Vec<&str> = report.rows.iter().map(|r| r.skill.as_str()).collect();
        assert_eq!(names, ["idle", "gamma"], "{report:?}");
        assert_eq!(report.unused, ["idle"]);

        let gamma = row(&report, "gamma");
        assert_eq!(gamma.recent_uses, 2);
        assert_eq!(gamma.last_used.as_deref(), Some(days_ago(3).as_str()));
        assert_eq!(
            gamma.agents,
            BTreeMap::from([("claude-code".to_string(), 2)])
        );

        let idle = row(&report, "idle");
        assert_eq!(idle.recent_uses, 0);
        assert_eq!(idle.last_used, None);
        assert!(idle.agents.is_empty());
    }

    /// Flow: report the window from 25 to 10 days ago (`since`, `until`).
    /// Expectation: only `gamma`'s 20-day-old use counts; its 3-day-old use
    /// falls outside, and the report echoes both bounds.
    /// Failure means an agent asking about a past week gets uses from
    /// outside it.
    #[test]
    fn a_since_until_window_counts_only_uses_inside_it() {
        let fixture = fixture();
        let inventory = scan(&fixture.home);
        let since = now() - chrono::Duration::days(25);
        let until = now() - chrono::Duration::days(10);
        let filter = UsageFilter {
            since: Some(since),
            until: Some(until),
            ..UsageFilter::default()
        };

        let report =
            SkillUsage::default().report_filtered(&fixture.home, &inventory, 30, now(), &filter);

        assert_eq!(row(&report, "gamma").recent_uses, 1, "{report:?}");
        assert_eq!(row(&report, "idle").recent_uses, 0, "{report:?}");
        assert_eq!(report.unused, ["idle"]);
        assert_eq!(report.window.since, Some(since.to_rfc3339()));
        assert_eq!(report.window.until, Some(until.to_rfc3339()));
    }

    /// Flow: report the 30 days before a date 10 days ago (`until` only).
    /// Expectation: the window is days 40 to 10 back, cut at the 30-day
    /// history, so `gamma`'s 20-day-old use counts and its 3-day-old use and
    /// `idle`'s 45-day-old use do not.
    /// Failure means `until` alone still counts back from now, or a use
    /// before the history counts.
    #[test]
    fn an_until_without_since_counts_the_history_days_before_it() {
        let fixture = fixture();
        let inventory = scan(&fixture.home);
        let filter = UsageFilter {
            until: Some(now() - chrono::Duration::days(10)),
            ..UsageFilter::default()
        };

        let report =
            SkillUsage::default().report_filtered(&fixture.home, &inventory, 30, now(), &filter);

        assert_eq!(row(&report, "gamma").recent_uses, 1, "{report:?}");
        assert_eq!(row(&report, "idle").recent_uses, 0, "{report:?}");
    }

    /// Flow: report a 30-day window with `top` 1.
    /// Expectation: one row, `gamma` (2 uses), and no unused names.
    /// Failure means "my most used skill" still returns every installed
    /// skill.
    #[test]
    fn top_keeps_only_the_most_used_rows() {
        let fixture = fixture();
        let inventory = scan(&fixture.home);
        let filter = UsageFilter {
            top: Some(1),
            ..UsageFilter::default()
        };

        let report =
            SkillUsage::default().report_filtered(&fixture.home, &inventory, 30, now(), &filter);

        let names: Vec<&str> = report.rows.iter().map(|r| r.skill.as_str()).collect();
        assert_eq!(names, ["gamma"]);
        assert!(report.unused.is_empty());
    }

    /// A window that holds no time is refused; a real one passes.
    /// Failure means an empty window reports every skill as unused.
    #[test]
    fn an_empty_window_is_refused() {
        let now = parse_usage_time("2026-11-14").unwrap();
        let at = |text| Some(parse_usage_time(text).unwrap());
        let window = |since, until| UsageFilter {
            since,
            until,
            ..UsageFilter::default()
        };
        assert!(window(at("2026-11-01"), at("2026-10-20"))
            .check_window(30, now)
            .is_err());
        assert!(window(at("2026-10-20"), at("2026-10-20"))
            .check_window(30, now)
            .is_err());
        assert!(window(at("2026-11-15"), None)
            .check_window(30, now)
            .is_err());
        assert!(window(at("2026-10-20"), at("2026-11-01"))
            .check_window(30, now)
            .is_ok());
        assert!(window(None, at("2026-11-10")).check_window(7, now).is_ok());
    }

    /// Flow: ask for more than the 30-day history, by `days`, by `since`, or
    /// by an `until` so old that `days` before it reach past the history.
    /// Expectation: 31 days (and `u32::MAX` days, without a panic), a `since`
    /// before 2026-10-15 (30 days before 2026-11-14), 7 days before
    /// 2026-10-20, and a window before the history's start hour on its first
    /// day are refused with the start named; 30 days and a `since` exactly at
    /// the start pass.
    /// Failure means a report counts a window whose older part has no
    /// history, and shows every skill used only then as unused.
    #[test]
    fn a_window_before_the_history_is_refused() {
        let now = parse_usage_time("2026-11-14").unwrap();
        let since = |text| UsageFilter {
            since: Some(parse_usage_time(text).unwrap()),
            ..UsageFilter::default()
        };
        let error = UsageFilter::default().check_window(31, now).unwrap_err();
        assert!(error.contains("last 30 days"), "{error}");
        assert!(UsageFilter::default().check_window(30, now).is_ok());
        let error = since("2026-10-14").check_window(30, now).unwrap_err();
        assert!(error.contains("2026-10-15T00:00:00Z or later"), "{error}");
        assert!(since("2026-10-15").check_window(30, now).is_ok());
        let old_until = UsageFilter {
            until: Some(parse_usage_time("2026-10-20").unwrap()),
            ..UsageFilter::default()
        };
        let error = old_until.check_window(7, now).unwrap_err();
        assert!(error.contains("2026-10-15T00:00:00Z or later"), "{error}");
        assert!(UsageFilter::default().check_window(u32::MAX, now).is_err());
        let earliest = UsageFilter {
            until: Some(DateTime::<Utc>::MIN_UTC),
            ..UsageFilter::default()
        };
        assert!(earliest.check_window(1, now).is_err());
        // At noon the history starts at noon 30 days back; midnight that day is before it.
        let noon = parse_usage_time("2026-11-14T12:00:00Z").unwrap();
        let first_morning = UsageFilter {
            since: Some(parse_usage_time("2026-10-15").unwrap()),
            until: Some(parse_usage_time("2026-10-15T11:00:00Z").unwrap()),
            ..UsageFilter::default()
        };
        assert!(first_morning.check_window(30, noon).is_err());
    }
    /// Flow: parse the window bounds a user types.
    /// Expectation: a bare date is the start of that day in UTC, an RFC 3339
    /// time keeps its instant, and other text is refused.
    /// Failure means `--since 2026-09-01` starts at the wrong time or bad
    /// input counts from the start of history.
    #[test]
    fn usage_times_parse_a_date_or_an_rfc3339_time() {
        assert_eq!(
            parse_usage_time("2026-09-01").unwrap().to_rfc3339(),
            "2026-09-01T00:00:00+00:00"
        );
        assert_eq!(
            parse_usage_time("2026-09-01T10:00:00+02:00")
                .unwrap()
                .to_rfc3339(),
            "2026-09-01T08:00:00+00:00"
        );
        assert!(parse_usage_time("last week").is_err());
    }

    /// Flow: seed from a desktop cache file, then report; separately, seed
    /// from a corrupt cache file.
    /// Expectation: both files are byte-for-byte unchanged afterwards and no
    /// `.corrupt` sibling appears; the report still counts the transcript.
    /// Failure means the CLI or MCP server wrote to the desktop app's cache.
    #[test]
    fn a_report_never_writes_the_desktop_cache() {
        let fixture = fixture();
        let inventory = scan(&fixture.home);
        let cache_dir = tempfile::tempdir().unwrap();

        let valid = cache_dir.path().join("skill-uses.json");
        SkillInvocationIndex::default().save(&valid).unwrap();
        let valid_before = fs::read(&valid).unwrap();
        let report =
            SkillUsage::load_read_only(Some(&valid)).report(&fixture.home, &inventory, 30, now());
        assert_eq!(row(&report, "gamma").recent_uses, 2);
        assert_eq!(fs::read(&valid).unwrap(), valid_before);

        let corrupt = cache_dir.path().join("corrupt.json");
        fs::write(&corrupt, b"{not json").unwrap();
        let report =
            SkillUsage::load_read_only(Some(&corrupt)).report(&fixture.home, &inventory, 30, now());
        assert_eq!(row(&report, "gamma").recent_uses, 2);
        assert_eq!(fs::read(&corrupt).unwrap(), b"{not json");
        assert!(!cache_dir.path().join("corrupt.json.corrupt").exists());
    }

    /// Flow: report once, append a new use to the transcript, report again
    /// with the same `SkillUsage`.
    /// Expectation: the second report sees the new use.
    /// Failure means the MCP server, which keeps one `SkillUsage` for its
    /// whole life, would serve stale counts.
    #[test]
    fn a_kept_index_picks_up_new_uses() {
        let fixture = fixture();
        let inventory = scan(&fixture.home);
        let mut usage = SkillUsage::default();
        assert_eq!(
            usage.report(&fixture.home, &inventory, 30, now()).unused,
            ["idle"]
        );

        let transcript = fixture.home.join(".claude/projects/-work/session.jsonl");
        let mut content = fs::read_to_string(&transcript).unwrap();
        content.push_str(&claude_skill_line("idle", &days_ago(1)));
        content.push('\n');
        fs::write(&transcript, content).unwrap();

        let report = usage.report(&fixture.home, &inventory, 30, now());
        assert!(report.unused.is_empty(), "{report:?}");
        assert_eq!(row(&report, "idle").recent_uses, 1);
    }
    #[test]
    fn window_counts_a_session_even_when_its_latest_load_is_outside_the_window() {
        let fixture = fixture();
        let records: Vec<_> = [60, 3, 1]
            .into_iter()
            .map(|days| {
                let mut record: serde_json::Value =
                    serde_json::from_str(&claude_skill_line("gamma", &days_ago(days))).unwrap();
                record["sessionId"] = "shared".into();
                record.to_string()
            })
            .collect();
        fs::write(
            fixture.home.join(".claude/projects/-work/session.jsonl"),
            records.join("\n") + "\n",
        )
        .unwrap();
        let inventory = scan(&fixture.home);
        let filter = UsageFilter {
            since: Some(now() - chrono::Duration::days(10)),
            until: Some(now() - chrono::Duration::days(2)),
            ..Default::default()
        };
        let report =
            SkillUsage::default().report_filtered(&fixture.home, &inventory, 30, now(), &filter);
        let gamma = row(&report, "gamma");
        assert_eq!(gamma.recent_uses, 1);
        assert_eq!(gamma.last_used.as_deref(), Some(days_ago(1).as_str()));
    }
    #[test]
    fn usage_counts_each_installed_skill_once_per_session_for_all_triggers() {
        let installed = BTreeSet::from(["write-tests".to_string()]);
        let sources = DiscoverySources::default();
        let uses: Vec<_> = [
            ("write-tests", "parent", SkillTrigger::Agent),
            ("plugin:write-tests", "parent", SkillTrigger::User),
            ("write-tests", "parent", SkillTrigger::FileRead),
            ("write-tests", "child", SkillTrigger::FileRead),
            ("removed", "child", SkillTrigger::Agent),
        ]
        .into_iter()
        .map(|(skill, session, trigger)| SkillInvocation {
            skill: skill.into(),
            session: Some("parent".into()),
            use_session: Some(session.into()),
            harness: "claude-code".into(),
            at: now(),
            project_path: None,
            trigger,
        })
        .collect();
        let counted = session_uses(&uses, &installed, &sources);
        assert_eq!(counted.len(), 2);
        assert_eq!(
            counted
                .iter()
                .filter(|u| u.use_session.as_deref() == Some("parent"))
                .count(),
            1
        );
    }
}
