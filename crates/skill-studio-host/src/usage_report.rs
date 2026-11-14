//! Per-skill use counts over a rolling window of days, shared by the CLI's
//! `usage` command and the MCP server's `skill_usage` tool so both answer
//! "which skills did I not use?" the same way.
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
use skill_studio_core::skill_uses::{counted_uses, SkillUseFilter};

use crate::fs::RealFs;
use crate::skill_uses::SkillInvocationIndex;

/// The window `usage` and `skill_usage` use when the caller names none.
pub const DEFAULT_USAGE_DAYS: u32 = 30;

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillUsageRow {
    /// Distinct (harness, session id) pairs in the last `days` days.
    pub sessions: usize,
    /// Uses in the last `days` days, by trigger.
    pub triggers: BTreeMap<String, u32>,
    /// The skill's name.
    pub skill: String,
    /// Uses inside the report window: `since`..`until`, or the last `days`.
    pub recent_uses: u32,
    /// Uses across all the session history that was read, outside the
    /// window too.
    pub total_uses: u32,
    /// The most recent use at any time, outside the window too, RFC 3339,
    /// or `None` when never used.
    pub last_used: Option<String>,
    /// Uses in the last `days` days, by agent id.
    pub agents: BTreeMap<String, u32>,
}

/// Uses for every installed, not parked skill over one window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UsageReport {
    /// Roots or files whose history was unavailable or skipped.
    pub observations: Vec<crate::skill_uses::UsageObservation>,
    /// The window, in days, that `recent_uses` and `unused` cover.
    pub days: u32,
    /// Start of the window, RFC 3339, when the caller gave one instead of `days`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    /// End of the window (not included), RFC 3339, when the caller gave one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,
    /// Skills not used in the window first, by name; then the rest, most
    /// used first.
    pub rows: Vec<SkillUsageRow>,
    /// Names of the skills with no use in the window, in `rows` order.
    /// Empty when the caller asked for the top rows only.
    pub unused: Vec<String>,
    /// Set when some session history could not be read, so a skill in
    /// `unused` may still have uses that were not counted, or when the scan
    /// skipped a root, so `rows` may miss a skill.
    pub partial: bool,
}

impl Outcome for UsageReport {
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
    /// Rejects a window that holds no time: `since` at or after `until`, or
    /// after `now` when there is no `until`. Such a window would report
    /// every skill as unused.
    pub fn check_window(&self, now: DateTime<Utc>) -> Result<(), String> {
        match self.since {
            Some(since) if since >= self.until.unwrap_or(now) => Err(format!(
                "the usage window is empty: since {} is not before {}",
                since.to_rfc3339(),
                self.until.unwrap_or(now).to_rfc3339()
            )),
            _ => Ok(()),
        }
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
        let filter = SkillUseFilter {
            known_skills: installed,
            sources: &sources,
        };
        let end = selection.until.unwrap_or(now);
        let cutoff = selection
            .since
            .unwrap_or_else(|| end - chrono::Duration::days(i64::from(days)));
        let mut by_skill: BTreeMap<&str, Tally> = installed
            .iter()
            .map(|name| (name.as_str(), Tally::default()))
            .collect();
        let selected = self.index.all_uses().filter(|u| {
            selection.harness.as_ref().is_none_or(|h| h == &u.harness)
                && (selection.projects.is_empty()
                    || selection.projects.iter().any(|p| {
                        u.project_path
                            .as_deref()
                            .is_some_and(|recorded| Path::new(recorded) == p)
                    }))
        });
        for use_ in counted_uses(selected, &filter) {
            let Some(tally) =
                installed_name(&use_.skill, installed).and_then(|name| by_skill.get_mut(name))
            else {
                continue;
            };
            tally.total += 1;
            if tally.last_used.is_none_or(|last| use_.at > last) {
                tally.last_used = Some(use_.at);
            }
            if use_.at < cutoff || selection.until.is_some_and(|until| use_.at >= until) {
                continue;
            }
            tally.recent += 1;
            *tally.agents.entry(use_.harness.clone()).or_insert(0) += 1;
            if let Some(session) = &use_.session {
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
                total_uses: tally.total,
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
            days,
            since: selection.since.map(|at| at.to_rfc3339()),
            until: selection.until.map(|at| at.to_rfc3339()),
            rows,
            unused,
            partial: !observations.is_empty(),
            observations,
        }
    }
}

#[derive(Default)]
struct Tally {
    sessions: BTreeSet<(String, String)>,
    triggers: BTreeMap<String, u32>,
    recent: u32,
    total: u32,
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
/// `SkillUseFilter` applies.
fn installed_name<'a>(recorded: &'a str, installed: &BTreeSet<String>) -> Option<&'a str> {
    if installed.contains(recorded) {
        return Some(recorded);
    }
    recorded
        .rsplit_once(':')
        .map(|(_, base)| base)
        .filter(|base| installed.contains(*base))
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
            r#"{{"type":"assistant","timestamp":"{at}","cwd":"/work","message":{{"content":[{{"type":"tool_use","name":"Skill","input":{{"skill":"{skill}"}}}}]}}}}"#
        )
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        home: PathBuf,
    }

    /// A home with three skills and one Claude Code transcript:
    /// - `gamma` (universal root) used 3 and 60 days ago, once through a
    ///   plugin-qualified `tools:gamma` name;
    /// - `idle` (Claude Code root) used once, 45 days ago;
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
            claude_skill_line("tools:gamma", &days_ago(60)),
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

    /// Flow: report a 30-day window over the fixture home.
    /// Expectation: `gamma` has 1 recent use by claude-code and 2 in total
    /// (the plugin-qualified use folds into it); `idle` has none recent, 1
    /// in total, and is the only `unused` name, listed first; the parked
    /// `resting` and the uninstalled `ghost` get no row.
    /// Failure means the CLI and MCP would name the wrong skills as safe to
    /// park.
    #[test]
    fn a_thirty_day_report_counts_recent_uses_and_lists_idle_skills_first() {
        let fixture = fixture();
        let inventory = scan(&fixture.home);

        let report = SkillUsage::default().report(&fixture.home, &inventory, 30, now());

        assert_eq!(report.days, 30);
        assert!(!report.partial, "absent history is complete: {report:?}");
        let names: Vec<&str> = report.rows.iter().map(|r| r.skill.as_str()).collect();
        assert_eq!(names, ["idle", "gamma"], "{report:?}");
        assert_eq!(report.unused, ["idle"]);

        let gamma = row(&report, "gamma");
        assert_eq!(gamma.recent_uses, 1);
        assert_eq!(gamma.total_uses, 2);
        assert_eq!(gamma.last_used.as_deref(), Some(days_ago(3).as_str()));
        assert_eq!(
            gamma.agents,
            BTreeMap::from([("claude-code".to_string(), 1)])
        );

        let idle = row(&report, "idle");
        assert_eq!(idle.recent_uses, 0);
        assert_eq!(idle.total_uses, 1);
        assert_eq!(idle.last_used.as_deref(), Some(days_ago(45).as_str()));
        assert!(idle.agents.is_empty());
    }

    /// Flow: report a 90-day window over the same home.
    /// Expectation: `idle`'s 45-day-old use now counts, so nothing is unused
    /// and `gamma` (2 recent) sorts before `idle` (1 recent).
    /// Failure means `days` is ignored and the window is fixed.
    #[test]
    fn a_wider_window_counts_older_uses() {
        let fixture = fixture();
        let inventory = scan(&fixture.home);

        let report = SkillUsage::default().report(&fixture.home, &inventory, 90, now());

        assert!(report.unused.is_empty(), "{report:?}");
        assert_eq!(row(&report, "gamma").recent_uses, 2);
        assert_eq!(row(&report, "idle").recent_uses, 1);
        assert_eq!(report.rows[0].skill, "gamma");
    }

    /// Flow: report the window from 50 to 10 days ago (`since`, `until`).
    /// Expectation: only `idle`'s 45-day-old use counts; `gamma`'s uses 3
    /// and 60 days ago fall outside, and the report echoes both bounds.
    /// Failure means an agent asking about a past month gets uses from
    /// outside it.
    #[test]
    fn a_since_until_window_counts_only_uses_inside_it() {
        let fixture = fixture();
        let inventory = scan(&fixture.home);
        let since = now() - chrono::Duration::days(50);
        let until = now() - chrono::Duration::days(10);
        let filter = UsageFilter {
            since: Some(since),
            until: Some(until),
            ..UsageFilter::default()
        };

        let report =
            SkillUsage::default().report_filtered(&fixture.home, &inventory, 30, now(), &filter);

        assert_eq!(row(&report, "idle").recent_uses, 1, "{report:?}");
        assert_eq!(row(&report, "gamma").recent_uses, 0, "{report:?}");
        assert_eq!(report.unused, ["gamma"]);
        assert_eq!(report.since, Some(since.to_rfc3339()));
        assert_eq!(report.until, Some(until.to_rfc3339()));
    }

    /// Flow: report the 30 days before a date 50 days ago (`until` only).
    /// Expectation: the window is days 80 to 50 back, so only `gamma`'s
    /// 60-day-old use counts, not `idle`'s 45-day-old one.
    /// Failure means `until` alone still counts back from now.
    #[test]
    fn an_until_without_since_counts_the_days_before_it() {
        let fixture = fixture();
        let inventory = scan(&fixture.home);
        let filter = UsageFilter {
            until: Some(now() - chrono::Duration::days(50)),
            ..UsageFilter::default()
        };

        let report =
            SkillUsage::default().report_filtered(&fixture.home, &inventory, 30, now(), &filter);

        assert_eq!(row(&report, "gamma").recent_uses, 1, "{report:?}");
        assert_eq!(row(&report, "idle").recent_uses, 0, "{report:?}");
    }

    /// Flow: report a 90-day window with `top` 1.
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
            SkillUsage::default().report_filtered(&fixture.home, &inventory, 90, now(), &filter);

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
        assert!(window(at("2026-11-01"), at("2026-10-01"))
            .check_window(now)
            .is_err());
        assert!(window(at("2026-10-01"), at("2026-10-01"))
            .check_window(now)
            .is_err());
        assert!(window(at("2026-11-15"), None).check_window(now).is_err());
        assert!(window(at("2026-10-01"), at("2026-11-01"))
            .check_window(now)
            .is_ok());
        assert!(window(None, at("2026-10-01")).check_window(now).is_ok());
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
        assert_eq!(row(&report, "gamma").recent_uses, 1);
        assert_eq!(fs::read(&valid).unwrap(), valid_before);

        let corrupt = cache_dir.path().join("corrupt.json");
        fs::write(&corrupt, b"{not json").unwrap();
        let report =
            SkillUsage::load_read_only(Some(&corrupt)).report(&fixture.home, &inventory, 30, now());
        assert_eq!(row(&report, "gamma").recent_uses, 1);
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
}
