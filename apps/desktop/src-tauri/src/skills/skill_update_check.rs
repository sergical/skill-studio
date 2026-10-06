// ============================================================================
// Skills Module - skill_update_check
// One searchable concept: check installed skills for upstream updates. A
// dotagents skill compares its pinned installed commit against the newest
// commit `gh api` reports for its path. A skills.sh skill compares its lock
// file's `skillFolderHash` against the source repo's tree SHA at HEAD for
// that path (unit 3.4: one `gh api` tree call per source repo, cached across
// every skills.sh candidate from that repo, not one commits call per skill).
// Runs on a 6 h timer (`spawn_update_check_loop`); no manual "Check now" is
// exposed to the frontend. Results persist at
// `<app data>/skill-studio/update-check.json` so `skill_refresh::build_snapshot`
// can read them without shelling out on every rebuild. Read-only GitHub
// access via the user's own `gh` login; the app stores no tokens.
//
// The 6 h background loop (`spawn_update_check_loop`) is the only trigger for
// a full check; there is no `check_skill_updates_now` command exposed to the
// frontend. `update_skill`/`update_all_skills` instead call
// `clear_owner_after_update` right after a successful `ops::update`, which
// drops the owner's persisted state directly rather than re-running `gh api`.
// ============================================================================

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use chrono::Utc;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use super::skill_agent_runner::{is_executable_file, pick_executable_line};
use super::skill_dto::InstallScope;
use super::skill_ownership::{load_ownership_ledgers, owner_id_for};
use super::skill_refresh;
use skill_studio_core::dotagents_ledger;

/// How often the background loop re-checks for updates.
pub const UPDATE_CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

/// How long after startup the background loop waits before its first check,
/// so it doesn't compete with the initial skill scan for CPU/network.
const INITIAL_DELAY: Duration = Duration::from_secs(10);

/// How many `gh api` lookups run concurrently.
const LOOKUP_POOL_SIZE: usize = 4;

/// One skill's update-check result, persisted in `UpdateCheckStore`.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SkillUpdateState {
    pub repo: String,
    pub path: String,
    pub installed_commit: Option<String>,
    pub latest_commit: Option<String>,
    pub latest_commit_at: Option<String>,
    pub checked_at: String,
    pub error: Option<String>,
}

/// Result of `gh api ... commits`, or why it couldn't be run.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(tag = "kind", content = "message", rename_all = "kebab-case")]
pub enum GhStatus {
    #[default]
    Ok,
    Missing,
    NotLoggedIn,
    Failed(String),
}

/// Everything one update check produced, persisted as-is at
/// `update_check_path`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpdateCheckStore {
    #[serde(default = "update_store_version")]
    pub version: u32,
    pub checked_at: Option<String>,
    pub gh_status: GhStatus,
    #[serde(default)]
    pub owners: BTreeMap<String, SkillUpdateState>,
    /// Source repos that are forks whose original has newer commits, keyed by
    /// the fork's `owner/repo`. Informational only; never counted as an update.
    #[serde(default)]
    pub upstream_ahead: BTreeMap<String, UpstreamAhead>,
    /// Version 1 used skill names as keys. It is read only as a conservative
    /// migration source and is never serialized again.
    #[serde(skip)]
    pub(crate) legacy_skills: BTreeMap<String, SkillUpdateState>,
}

impl Default for UpdateCheckStore {
    fn default() -> Self {
        Self {
            version: update_store_version(),
            checked_at: None,
            gh_status: GhStatus::Ok,
            owners: BTreeMap::new(),
            upstream_ahead: BTreeMap::new(),
            legacy_skills: BTreeMap::new(),
        }
    }
}

fn update_store_version() -> u32 {
    2
}

/// A source repo that is a fork whose original repo has commits the fork
/// does not. Shown as a note in the skill detail header; no file changes.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema)]
pub struct UpstreamAhead {
    /// The fork the skill is installed from, `owner/repo`.
    pub repo: String,
    /// The original repo, `owner/repo`.
    pub upstream_repo: String,
    /// Commits the original has that the fork does not.
    pub behind_by: u32,
    /// GitHub page listing those commits.
    pub compare_url: String,
    /// Lifecycle owner ids of every installed skill from this repo, sorted.
    /// The frontend matches a skill's deployments against these, because a
    /// dotagents-only install has no lock-file `source` to compare.
    #[serde(default)]
    pub owner_ids: Vec<String>,
}

/// The `SkillSnapshot.update_check` shape sent to the frontend: a flattened,
/// string-tagged view of `UpdateCheckStore` plus a ready-to-display count.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct UpdateCheckSummary {
    pub checked_at: Option<String>,
    pub gh_status: String, // "ok" | "missing" | "not-logged-in" | "failed"
    pub message: Option<String>,
    pub updates_available: u32,
    #[serde(default)]
    pub upstream_ahead: Vec<UpstreamAhead>,
}

impl Default for UpdateCheckSummary {
    fn default() -> Self {
        Self {
            checked_at: None,
            gh_status: "ok".to_string(),
            message: None,
            updates_available: 0,
            upstream_ahead: Vec::new(),
        }
    }
}

/// `<app data>/skill-studio/update-check.json`.
pub fn update_check_path(app_data: &Path) -> PathBuf {
    app_data.join("skill-studio").join("update-check.json")
}

/// Read the persisted store, or a fresh empty one if it doesn't exist yet or
/// fails to parse. `app_data` is the app data dir, not the store file itself;
/// see `read_update_check_store_at` for a caller that already has the exact
/// file path.
pub fn read_update_check_store(app_data: &Path) -> UpdateCheckStore {
    read_update_check_store_at(&update_check_path(app_data))
}

/// Like `read_update_check_store`, but `path` is the exact store file path,
/// not the app data dir - for callers (like `skill_refresh`) that already
/// resolved `update_check_path` once and shouldn't have it joined again.
pub fn read_update_check_store_at(path: &Path) -> UpdateCheckStore {
    let Ok(content) = std::fs::read_to_string(path) else {
        return UpdateCheckStore::default();
    };
    let value: serde_json::Value = match serde_json::from_str(&content) {
        Ok(value) => value,
        Err(e) => {
            eprintln!(
                "skill update check: failed to parse {}: {e}",
                path.display()
            );
            return UpdateCheckStore::default();
        }
    };
    if value.get("owners").is_some() {
        return serde_json::from_value(value).unwrap_or_else(|e| {
            eprintln!(
                "skill update check: failed to parse {}: {e}",
                path.display()
            );
            UpdateCheckStore::default()
        });
    }
    #[derive(Deserialize)]
    struct LegacyStore {
        checked_at: Option<String>,
        gh_status: GhStatus,
        #[serde(default)]
        skills: BTreeMap<String, SkillUpdateState>,
    }
    serde_json::from_value::<LegacyStore>(value).map_or_else(
        |e| {
            eprintln!(
                "skill update check: failed to parse {}: {e}",
                path.display()
            );
            UpdateCheckStore::default()
        },
        |legacy| UpdateCheckStore {
            version: update_store_version(),
            checked_at: legacy.checked_at,
            gh_status: legacy.gh_status,
            owners: BTreeMap::new(),
            upstream_ahead: BTreeMap::new(),
            legacy_skills: legacy.skills,
        },
    )
}

/// Counter appended to `write_store`'s temp file name, so two writers (e.g.
/// a full check and a per-skill check that raced past the `UpdateCheckState`
/// guard) never pick the same temp path and clobber each other mid-write.
static WRITE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Write `store` atomically (unique temp file + rename) to `update_check_path`.
fn write_store(app_data: &Path, store: &UpdateCheckStore) -> Result<(), String> {
    let path = update_check_path(app_data);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(store)
        .map_err(|e| format!("Failed to serialize update check store: {e}"))?;
    let unique = WRITE_COUNTER.fetch_add(1, Ordering::SeqCst);
    let tmp_path = path.with_extension(format!("json.tmp.{}.{unique}", std::process::id()));
    std::fs::write(&tmp_path, json)
        .map_err(|e| format!("Failed to write {}: {e}", tmp_path.display()))?;
    std::fs::rename(&tmp_path, &path)
        .map_err(|e| format!("Failed to rename {}: {e}", tmp_path.display()))
}

/// True when `installed_commit` and `latest_commit` are both known and
/// differ.
pub fn has_update(state: &SkillUpdateState) -> bool {
    match (&state.installed_commit, &state.latest_commit) {
        (Some(installed), Some(latest)) => installed != latest,
        _ => false,
    }
}

/// The legacy skill name `owner_id`'s name-keyed fallback would read from
/// `legacy_skills`, or `None` when the fallback doesn't apply: accepted only
/// for the sole matching Global owner, since Project owners never inherit
/// records the old Global-only checker wrote. Shared by `state_for_owner`
/// and `clear_owner_after_update` (N2, review round 3) so the write side
/// can't rewrite an entry the read side would never have served.
fn legacy_fallback_name(owner_id: &str, current_owner_ids: &[String]) -> Option<String> {
    let parsed = super::skill_ownership::parse_owner_id(owner_id)?;
    if parsed.scope != InstallScope::Global {
        return None;
    }
    let matching = current_owner_ids
        .iter()
        .filter_map(|id| super::skill_ownership::parse_owner_id(id))
        .filter(|candidate| candidate.name == parsed.name)
        .count();
    (matching == 1).then_some(parsed.name)
}

/// Resolve update state for one exact lifecycle owner. A legacy name-keyed
/// state is accepted only for the sole matching Global owner; Project owners
/// never inherit records written by the old Global-only checker.
pub fn state_for_owner<'a>(
    store: &'a UpdateCheckStore,
    owner_id: &str,
    current_owner_ids: &[String],
) -> Option<&'a SkillUpdateState> {
    if let Some(state) = store.owners.get(owner_id) {
        return Some(state);
    }
    let name = legacy_fallback_name(owner_id, current_owner_ids)?;
    store.legacy_skills.get(&name)
}

/// Flatten `store` into the DTO the frontend reads off `SkillSnapshot`.
pub fn summarize(store: &UpdateCheckStore) -> UpdateCheckSummary {
    let (gh_status, message) = match &store.gh_status {
        GhStatus::Ok => ("ok", None),
        GhStatus::Missing => ("missing", None),
        GhStatus::NotLoggedIn => (
            "not-logged-in",
            Some("gh not logged in — run gh auth login".to_string()),
        ),
        GhStatus::Failed(m) => ("failed", Some(m.clone())),
    };
    let updates_available = store.owners.values().filter(|s| has_update(s)).count() as u32;
    UpdateCheckSummary {
        checked_at: store.checked_at.clone(),
        gh_status: gh_status.to_string(),
        message,
        updates_available,
        upstream_ahead: store.upstream_ahead.values().cloned().collect(),
    }
}

/// Looks up the newest commit that touched `path` in `repo`, so the real `gh`
/// implementation and a fake recorder can share one signature in tests.
/// Implementors must be `Sync`: `run_update_check` shares one `&dyn
/// CommitLookup` across a small worker pool.
pub trait CommitLookup: Sync {
    /// Returns `(sha, committer date)` for the newest commit touching `path`
    /// in `repo`, at or before `until` when given, or `Ok(None)` when the
    /// path has no commits (yet). `Err` messages from the real `gh`
    /// implementation may contain "gh auth login", which `run_update_check`
    /// treats as "not logged in" and stops on.
    fn latest_commit(
        &self,
        repo: &str,
        path: &str,
        until: Option<&str>,
    ) -> Result<Option<(String, String)>, String>;

    /// Add-operation lookup. Legacy implementations keep working, while the
    /// real implementation applies the shared cancellation and deadline.
    fn latest_commit_controlled(
        &self,
        repo: &str,
        path: &str,
        until: Option<&str>,
        control: &super::skill_process::AddOperationControl,
    ) -> Result<Option<(String, String)>, String> {
        control.check_message()?;
        let result = self.latest_commit(repo, path, until);
        control.check_message()?;
        result
    }
}

/// Real `CommitLookup` backed by the `gh` CLI.
pub struct GhCommitLookup {
    pub gh_bin: PathBuf,
}

impl CommitLookup for GhCommitLookup {
    fn latest_commit(
        &self,
        repo: &str,
        path: &str,
        until: Option<&str>,
    ) -> Result<Option<(String, String)>, String> {
        let mut api_path = format!(
            "repos/{repo}/commits?path={}&per_page=1",
            urlencoding::encode(path)
        );
        if let Some(until) = until {
            // Writing to a `String` never fails.
            let _ = write!(api_path, "&until={}", urlencoding::encode(until));
        }

        let stdout_bytes = super::gh_cli::run_gh(
            &self.gh_bin,
            &[
                "api",
                &api_path,
                "--jq",
                ".[0] | [.sha, .commit.committer.date] | @tsv",
            ],
            None,
        )
        .map_err(|e| e.message())?;

        let stdout = String::from_utf8_lossy(&stdout_bytes);
        let line = stdout.trim();
        if line.is_empty() {
            return Ok(None);
        }
        let mut parts = line.splitn(2, '\t');
        let sha = parts.next().unwrap_or_default().to_string();
        let date = parts.next().unwrap_or_default().to_string();
        if sha.is_empty() {
            return Ok(None);
        }
        Ok(Some((sha, date)))
    }

    fn latest_commit_controlled(
        &self,
        repo: &str,
        path: &str,
        until: Option<&str>,
        control: &super::skill_process::AddOperationControl,
    ) -> Result<Option<(String, String)>, String> {
        let mut api_path = format!(
            "repos/{repo}/commits?path={}&per_page=1",
            urlencoding::encode(path)
        );
        if let Some(until) = until {
            // Writing to a `String` never fails.
            let _ = write!(api_path, "&until={}", urlencoding::encode(until));
        }
        let stdout = super::gh_cli::run_gh_controlled(
            &self.gh_bin,
            &[
                "api",
                &api_path,
                "--jq",
                ".[0] | [.sha, .commit.committer.date] | @tsv",
            ],
            control,
        )
        .map_err(|error| error.message())?;
        let stdout = String::from_utf8_lossy(&stdout);
        let mut parts = stdout.trim().splitn(2, '\t');
        let sha = parts.next().unwrap_or_default().to_string();
        if sha.is_empty() {
            return Ok(None);
        }
        Ok(Some((sha, parts.next().unwrap_or_default().to_string())))
    }
}

/// Looks up every subtree's SHA at HEAD for a skills.sh source repo, one
/// call per repo rather than one per skill folder in it. Implementors must
/// be `Sync`: `run_update_check` shares one `&dyn TreeLookup` across a small
/// worker pool.
pub trait TreeLookup: Sync {
    /// Returns every subtree path in `repo` mapped to its git tree SHA at
    /// HEAD (uncached - callers go through `tree_shas_cached` for the
    /// one-call-per-repo guarantee).
    fn tree_shas_at_head_uncached(&self, repo: &str) -> Result<HashMap<String, String>, String>;
}

/// Per-run cache of `TreeLookup` results, keyed by the normalised repo name
/// (`normalize_repo_key`). Each repo gets its own `OnceLock` so two different
/// repos' `gh api` calls run in parallel;
/// only two threads racing the *same* repo serialize, on that repo's cell.
type TreeCache = Mutex<HashMap<String, Arc<OnceLock<Result<HashMap<String, String>, String>>>>>;

/// Looks up `repo`'s tree, reusing an already-cached result (or error) for
/// this run instead of calling `gh` again. The outer `cache` lock is held
/// only long enough to fetch-or-create `repo`'s cell, not across the network
/// call - so a worker checking a different repo never waits on this one's
/// `gh api` round trip. Two worker threads racing the same uncached repo
/// still make exactly one call: `OnceLock::get_or_init` blocks the second
/// caller on the first's initialization instead of both running it.
///
/// The cache is keyed by `normalize_repo_key(repo)` (shared with the core
/// crate's own tree cache) so two spellings of the same source - a
/// different case, most often - cost one `gh api` call rather than one
/// each; the lookup itself still receives the caller's original `repo`
/// spelling, which the GitHub API accepts case-insensitively.
fn tree_shas_cached(
    tree_lookup: &dyn TreeLookup,
    cache: &TreeCache,
    repo: &str,
) -> Result<HashMap<String, String>, String> {
    let key = skill_studio_core::skill_update_check::normalize_repo_key(repo);
    let cell = {
        let mut guard = cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard
            .entry(key)
            .or_insert_with(|| Arc::new(OnceLock::new()))
            .clone()
    };
    cell.get_or_init(|| tree_lookup.tree_shas_at_head_uncached(repo))
        .clone()
}

/// Real `TreeLookup` backed by the `gh` CLI, over the same
/// `repos/<repo>/git/trees/HEAD?recursive=1` shape as
/// `skill_studio_host::GhSourceTreeLookup` - this desktop copy stays on the
/// existing `gh_cli::run_gh` wrapper so its errors keep this file's plain
/// `String` shape (`is_not_logged_in` detection, "gh missing" handling) that
/// `CommitLookup` already relies on, rather than converting `CoreError` back
/// and forth for one caller.
pub struct GhTreeLookup {
    pub gh_bin: PathBuf,
}

impl TreeLookup for GhTreeLookup {
    fn tree_shas_at_head_uncached(&self, repo: &str) -> Result<HashMap<String, String>, String> {
        let api_path = format!("repos/{repo}/git/trees/HEAD?recursive=1");
        // No `--jq` filter here (unlike `GhCommitLookup`): the response must
        // be inspected for `truncated` before its `tree` entries are
        // trusted, and `--jq` would already have thrown that field away.
        let stdout_bytes = super::gh_cli::run_gh(&self.gh_bin, &["api", &api_path], None)
            .map_err(|e| e.message())?;
        parse_tree_response(repo, &stdout_bytes)
    }
}

/// Parses a `gh api repos/<repo>/git/trees/HEAD?recursive=1` response body,
/// the same shape `skill_studio_host::gh_currency::parse_tree_response`
/// parses for the core stack's own tree lookup. A `truncated: true` response
/// means GitHub's recursive listing stopped early - the caller cannot tell
/// "not in the tree" from "not fetched yet" for the paths past the cutoff,
/// so this is an error naming the repo rather than a partial (and silently
/// misleading) map.
fn parse_tree_response(repo: &str, stdout: &[u8]) -> Result<HashMap<String, String>, String> {
    let value: serde_json::Value = serde_json::from_slice(stdout)
        .map_err(|e| format!("{repo}: could not parse tree listing: {e}"))?;
    if value.get("truncated").and_then(serde_json::Value::as_bool) == Some(true) {
        return Err(format!("{repo}: tree listing truncated"));
    }
    let mut shas = HashMap::new();
    for entry in value
        .get("tree")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        if entry.get("type").and_then(serde_json::Value::as_str) != Some("tree") {
            continue;
        }
        let (Some(path), Some(sha)) = (
            entry.get("path").and_then(serde_json::Value::as_str),
            entry.get("sha").and_then(serde_json::Value::as_str),
        ) else {
            continue;
        };
        shas.insert(path.to_string(), sha.to_string());
    }
    Ok(shas)
}

/// A repo's fork facts, as `gh api repos/{repo}` reports them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoForkInfo {
    pub default_branch: String,
    /// `None` when the repo is not a fork (or GitHub names no parent).
    pub parent: Option<ForkParent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForkParent {
    /// `owner/repo` of the original.
    pub full_name: String,
    pub default_branch: String,
}

/// The two GitHub questions the fork note asks, behind a trait so the logic
/// runs without network in tests.
pub trait ForkLookup: Sync {
    fn repo_info(&self, repo: &str) -> Result<RepoForkInfo, String>;

    /// Commits `base_repo`'s `base_branch` has that `head_owner:head_branch`
    /// does not (`behind_by` in GitHub's compare response).
    fn commits_behind(
        &self,
        base_repo: &str,
        base_branch: &str,
        head_owner: &str,
        head_branch: &str,
    ) -> Result<u32, String>;
}

/// Parses the `@tsv` line `GhForkLookup::repo_info` asks `gh` for:
/// `fork \t default_branch \t parent.full_name \t parent.default_branch`,
/// the parent columns empty when GitHub names none.
fn parse_repo_fork_info(stdout: &str) -> RepoForkInfo {
    let mut parts = stdout.trim_end_matches('\n').splitn(4, '\t');
    let is_fork = parts.next() == Some("true");
    let default_branch = parts.next().unwrap_or_default().to_string();
    let parent_name = parts.next().unwrap_or_default();
    let parent_branch = parts.next().unwrap_or_default();
    let parent =
        (is_fork && !parent_name.is_empty() && !parent_branch.is_empty()).then(|| ForkParent {
            full_name: parent_name.to_string(),
            default_branch: parent_branch.to_string(),
        });
    RepoForkInfo {
        default_branch,
        parent,
    }
}

/// Deadline for one fork lookup call, so a stalled `gh` cannot hold up the
/// owner update checks that wait for the fork lookups.
const FORK_LOOKUP_TIMEOUT: Duration = Duration::from_secs(20);

/// Real `ForkLookup` backed by the `gh` CLI.
pub struct GhForkLookup {
    pub gh_bin: PathBuf,
    /// Deadline for each `gh` call; a timeout is an ordinary failed lookup.
    pub timeout: Duration,
}

impl GhForkLookup {
    fn run(&self, args: &[&str]) -> Result<Vec<u8>, String> {
        let control = super::skill_process::AddOperationControl::new(
            Arc::new(AtomicBool::new(false)),
            self.timeout,
        );
        super::gh_cli::run_gh_controlled(&self.gh_bin, args, &control).map_err(|e| e.message())
    }
}

impl ForkLookup for GhForkLookup {
    fn repo_info(&self, repo: &str) -> Result<RepoForkInfo, String> {
        let api_path = format!("repos/{repo}");
        let stdout = self.run(&[
            "api",
            &api_path,
            "--jq",
            r#"[.fork, .default_branch, (.parent.full_name // ""), (.parent.default_branch // "")] | @tsv"#,
        ])?;
        Ok(parse_repo_fork_info(&String::from_utf8_lossy(&stdout)))
    }

    fn commits_behind(
        &self,
        base_repo: &str,
        base_branch: &str,
        head_owner: &str,
        head_branch: &str,
    ) -> Result<u32, String> {
        // `per_page=1` keeps the commit list small; `behind_by` is a total.
        let api_path = format!(
            "repos/{base_repo}/compare/{base_branch}...{head_owner}:{head_branch}?per_page=1"
        );
        let stdout = self.run(&["api", &api_path, "--jq", ".behind_by"])?;
        String::from_utf8_lossy(&stdout)
            .trim()
            .parse()
            .map_err(|e| format!("{base_repo}: could not read behind_by: {e}"))
    }
}

/// True for names safe to place in a GitHub API URL path: letters, digits,
/// `.`, `_`, `-`, and (for branches) `/`. Rejects empty names and `..`.
fn is_url_safe_name(name: &str, allow_slash: bool) -> bool {
    !name.is_empty()
        && !name.contains("..")
        && name.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') || (allow_slash && c == '/')
        })
}

/// Splits `owner/repo` when both halves are URL-safe.
fn split_safe_repo(repo: &str) -> Option<(&str, &str)> {
    let (owner, name) = repo.split_once('/')?;
    (is_url_safe_name(owner, false) && is_url_safe_name(name, false)).then_some((owner, name))
}

/// One record for `repo` when it is a fork whose original is ahead. `Ok(None)`
/// for a non-fork, an up-to-date fork, or names that are unsafe for a URL.
fn upstream_ahead_for_repo(
    repo: &str,
    lookup: &dyn ForkLookup,
) -> Result<Option<UpstreamAhead>, String> {
    let Some((owner, _)) = split_safe_repo(repo) else {
        return Ok(None);
    };
    let info = lookup.repo_info(repo)?;
    let Some(parent) = info.parent else {
        return Ok(None);
    };
    let Some((parent_owner, parent_name)) = split_safe_repo(&parent.full_name) else {
        return Ok(None);
    };
    if !is_url_safe_name(&info.default_branch, true)
        || !is_url_safe_name(&parent.default_branch, true)
    {
        return Ok(None);
    }
    let behind_by = lookup.commits_behind(
        &parent.full_name,
        &parent.default_branch,
        owner,
        &info.default_branch,
    )?;
    if behind_by == 0 {
        return Ok(None);
    }
    Ok(Some(UpstreamAhead {
        repo: repo.to_string(),
        compare_url: format!(
            "https://github.com/{repo}/compare/{}...{parent_owner}:{parent_name}:{}",
            info.default_branch, parent.default_branch
        ),
        upstream_repo: parent.full_name,
        behind_by,
        owner_ids: Vec::new(),
    }))
}

/// True when a `gh` error says the repo does not exist (deleted, renamed away,
/// or private to this login).
fn is_repo_not_found(message: &str) -> bool {
    message.contains("HTTP 404") || message.contains("Not Found")
}

/// True when a `gh` error says every further call will fail the same way.
fn is_auth_failure(message: &str) -> bool {
    is_not_logged_in(message)
        || message.contains("HTTP 401")
        || message.to_ascii_lowercase().contains("authentication")
}

/// For each distinct source repo among `candidates`, find forks whose original
/// has commits the fork lacks, on the same small worker pool as the commit
/// lookups. A repo that no longer exists loses its record. Any other failure
/// keeps the record from `previous` (if any), so a rate limit neither hides nor
/// invents a note. After an auth failure the remaining repos are not asked and
/// also keep their previous records. Repos no longer among `candidates` are
/// dropped. Keyed by `normalize_repo_key`.
fn find_upstream_ahead(
    candidates: &[Candidate],
    lookup: &dyn ForkLookup,
    previous: &BTreeMap<String, UpstreamAhead>,
) -> BTreeMap<String, UpstreamAhead> {
    // key -> (repo as first seen, owner ids of every skill from it)
    let mut groups: BTreeMap<String, (String, Vec<String>)> = BTreeMap::new();
    for candidate in candidates {
        let key = skill_studio_core::skill_update_check::normalize_repo_key(&candidate.repo);
        let group = groups
            .entry(key)
            .or_insert_with(|| (candidate.repo.clone(), Vec::new()));
        group.1.push(candidate.owner_id.clone());
    }
    for (_, owner_ids) in groups.values_mut() {
        owner_ids.sort();
        owner_ids.dedup();
    }

    let queue: Mutex<VecDeque<(&String, &String)>> =
        Mutex::new(groups.iter().map(|(key, (repo, _))| (key, repo)).collect());
    let results: Mutex<BTreeMap<&String, Result<Option<UpstreamAhead>, String>>> =
        Mutex::new(BTreeMap::new());
    let auth_failed = AtomicBool::new(false);

    std::thread::scope(|scope| {
        for _ in 0..LOOKUP_POOL_SIZE {
            scope.spawn(|| loop {
                if auth_failed.load(Ordering::Relaxed) {
                    break;
                }
                let next = queue
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .pop_front();
                let Some((key, repo)) = next else { break };
                let result = upstream_ahead_for_repo(repo, lookup);
                if let Err(e) = &result {
                    eprintln!("skill update check: fork lookup for {repo} failed: {e}");
                    if is_auth_failure(e) {
                        auth_failed.store(true, Ordering::Relaxed);
                    }
                }
                results
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(key, result);
            });
        }
    });

    let results = results
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut found = BTreeMap::new();
    for (key, (_, owner_ids)) in &groups {
        let record = match results.get(key) {
            Some(Ok(record)) => record.clone().map(|mut record| {
                record.owner_ids.clone_from(owner_ids);
                record
            }),
            Some(Err(e)) if is_repo_not_found(e) => None,
            // Failed (a timeout included), or never asked because an auth
            // failure stopped the run. Owner ids are name+scope, not repo, so
            // only owners the old record named that still install from this
            // repo keep it.
            _ => previous
                .values()
                .find(|old| {
                    skill_studio_core::skill_update_check::normalize_repo_key(&old.repo) == *key
                })
                .map(|old| UpstreamAhead {
                    owner_ids: old
                        .owner_ids
                        .iter()
                        .filter(|id| owner_ids.contains(id))
                        .cloned()
                        .collect(),
                    ..old.clone()
                }),
        };
        if let Some(record) = record.filter(|record| !record.owner_ids.is_empty()) {
            found.insert(key.clone(), record);
        }
    }
    found
}

/// Keeps each record only for owner ids whose current source repo still
/// equals the record's repo, and drops records left with no owner. Owner ids
/// are name+scope, so a skill reinstalled from another repo keeps its id.
fn prune_upstream_ahead(
    records: &BTreeMap<String, UpstreamAhead>,
    candidates: &[Candidate],
) -> BTreeMap<String, UpstreamAhead> {
    use skill_studio_core::skill_update_check::normalize_repo_key;
    records
        .iter()
        .filter_map(|(key, record)| {
            let record_key = normalize_repo_key(&record.repo);
            let owner_ids: Vec<String> = record
                .owner_ids
                .iter()
                .filter(|id| {
                    candidates
                        .iter()
                        .any(|c| &c.owner_id == *id && normalize_repo_key(&c.repo) == record_key)
                })
                .cloned()
                .collect();
            (!owner_ids.is_empty()).then(|| {
                (
                    key.clone(),
                    UpstreamAhead {
                        owner_ids,
                        ..record.clone()
                    },
                )
            })
        })
        .collect()
}

/// `summarize` with the fork notes filtered against the sources installed now
/// (read from the local lock and ledger files), so a note vanishes as soon as
/// its skill is reinstalled from another repo, without waiting for a check.
pub fn summarize_current(
    store: &UpdateCheckStore,
    home: &Path,
    project_paths: &[PathBuf],
) -> UpdateCheckSummary {
    let mut summary = summarize(store);
    summary.upstream_ahead = prune_upstream_ahead(
        &store.upstream_ahead,
        &build_candidates(home, project_paths),
    )
    .into_values()
    .collect();
    summary
}

/// Resolve `gh` on `$PATH` via a login shell, the same way
/// `skill_agent_runner::resolve_binary` finds harness binaries. `None` when
/// `gh` isn't installed.
pub(crate) fn resolve_gh_binary() -> Option<PathBuf> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let output = std::process::Command::new(&shell)
        .arg("-lc")
        .arg("command -v gh")
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    pick_executable_line(&stdout, is_executable_file)
}

/// True when an error message from `CommitLookup::latest_commit` indicates
/// the user isn't logged into `gh`.
fn is_not_logged_in(message: &str) -> bool {
    message.contains("gh auth login")
}

/// One repo/path pair worth checking, and how its `installed_commit` is
/// determined.
#[derive(Clone)]
struct Candidate {
    owner_id: String,
    name: String,
    scope: InstallScope,
    repo: String,
    path: String,
    kind: CandidateKind,
}

#[derive(Clone)]
enum CandidateKind {
    /// `installed_commit` comes straight from `agents.lock`.
    Dotagents { installed_commit: Option<String> },
    /// `installed_commit` is the lock file's `skillFolderHash`, compared
    /// against the source repo's tree SHA for this candidate's `path` at
    /// HEAD (one `gh api` tree call per repo, not per skill).
    SkillsSh { skill_folder_hash: String },
}

/// Build the candidate list from the dotagents ledger and the skills.sh lock
/// file under `home/.agents`, dotagents winning over skills.sh for a name
/// present in both (matches `skill_studio_core::identity::SourceKind`'s precedence). Manual
/// and plugin skills have no ledger entry, so they're never candidates.
fn build_candidates(home: &Path, project_paths: &[PathBuf]) -> Vec<Candidate> {
    // A fork's `base_commit` is the pinned "installed" side of the compare -
    // exactly the shape `CandidateKind::Dotagents` already models - and a
    // fork wins over a same-named ledger entry, same as dotagents wins over
    // skills.sh: it's the more specific, more recently established source.
    let fork_registry = super::skill_fork_registry::read_fork_registry_or_default(home);
    let mut candidates: Vec<Candidate> = fork_registry
        .forks
        .iter()
        .map(|(name, record)| Candidate {
            owner_id: format!("owner:v1/global/{name}"),
            name: name.clone(),
            scope: InstallScope::Global,
            repo: record.repo.clone(),
            path: record.path.clone(),
            kind: CandidateKind::Dotagents {
                installed_commit: Some(record.base_commit.clone()),
            },
        })
        .collect();
    let fork_names: std::collections::BTreeSet<String> =
        fork_registry.forks.keys().cloned().collect();

    for ledger in load_ownership_ledgers(home, project_paths) {
        let owner_id = |name: &str| owner_id_for(&ledger, name);
        let global_fork = ledger.scope == InstallScope::Global;
        let mut dotagents_names: std::collections::BTreeSet<String> = ledger
            .dotagents
            .iter()
            .map(|skill| skill.name.clone())
            .collect();
        if global_fork {
            dotagents_names.extend(fork_names.iter().cloned());
        }
        candidates.extend(ledger.dotagents.iter().filter_map(|skill| {
            if global_fork && fork_names.contains(&skill.name) {
                return None;
            }
            let id = owner_id(&skill.name);
            skill.github_repo.clone().map(|repo| Candidate {
                owner_id: id,
                name: skill.name.clone(),
                scope: ledger.scope,
                repo,
                path: skill.path.clone(),
                kind: CandidateKind::Dotagents {
                    installed_commit: skill.installed_commit.clone(),
                },
            })
        }));

        for (name, entry) in &ledger.lock.skills {
            if dotagents_names.contains(name) || entry.source_type != "github" {
                continue;
            }
            let Some(repo) = dotagents_ledger::github_repo_from_source(&entry.source) else {
                continue;
            };
            let Some(skill_path) = &entry.skill_path else {
                continue;
            };
            let path = skill_path
                .strip_suffix("/SKILL.md")
                .unwrap_or(skill_path)
                .to_string();
            candidates.push(Candidate {
                owner_id: owner_id(name),
                name: name.clone(),
                scope: ledger.scope,
                repo,
                path,
                kind: CandidateKind::SkillsSh {
                    skill_folder_hash: entry.skill_folder_hash.clone(),
                },
            });
        }
    }

    candidates.sort_by(|a, b| a.owner_id.cmp(&b.owner_id));
    candidates
}

/// The two source lookups `check_candidate` needs, bundled so the function
/// stays under clippy's argument-count limit: a dotagents commit lookup and
/// a skills.sh tree-SHA lookup (with its per-repo cache).
struct Lookups<'a> {
    commit: &'a dyn CommitLookup,
    tree: &'a dyn TreeLookup,
    tree_cache: &'a TreeCache,
}

/// On a lookup failure, the previous run's `latest_commit`/`latest_commit_at`
/// are only safe to reuse when `previous.installed_commit` still matches the
/// candidate's current `installed_commit` - otherwise `previous` was
/// computed against an older install (a stale generation), and pairing its
/// `latest_commit` with today's fresh `installed_commit` can make
/// `has_update()` true for a skill nothing actually flagged this run
/// (`a_lookup_error_on_the_first_check_after_upgrade_never_flags_an_update_or_names_the_false_positive`).
fn previous_metadata_if_same_generation(
    previous: Option<&SkillUpdateState>,
    installed_commit: Option<&str>,
) -> (Option<String>, Option<String>) {
    match previous {
        Some(p) if p.installed_commit.as_deref() == installed_commit => {
            (p.latest_commit.clone(), p.latest_commit_at.clone())
        }
        _ => (None, None),
    }
}

/// Check one candidate, given the previous run's state for it (if any).
/// Returns `None` when `stop` was already set before this candidate could be
/// looked up at all - the caller falls back to the previous state, if any.
fn check_candidate(
    candidate: &Candidate,
    previous: Option<&SkillUpdateState>,
    lookups: &Lookups,
    stop: &AtomicBool,
    not_logged_in_message: &Mutex<Option<String>>,
    now: &str,
) -> Option<SkillUpdateState> {
    if stop.load(Ordering::SeqCst) {
        return None;
    }

    let mut error: Option<String> = None;

    let (installed_commit, latest_commit, latest_commit_at) = match &candidate.kind {
        CandidateKind::Dotagents { installed_commit } => {
            match lookups
                .commit
                .latest_commit(&candidate.repo, &candidate.path, None)
            {
                Ok(Some((sha, date))) => (installed_commit.clone(), Some(sha), Some(date)),
                Ok(None) => (installed_commit.clone(), None, None),
                Err(e) if is_not_logged_in(&e) => {
                    stop.store(true, Ordering::SeqCst);
                    *not_logged_in_message
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(e);
                    let (latest_commit, latest_commit_at) =
                        previous_metadata_if_same_generation(previous, installed_commit.as_deref());
                    (installed_commit.clone(), latest_commit, latest_commit_at)
                }
                Err(e) => {
                    error = Some(e);
                    let (latest_commit, latest_commit_at) =
                        previous_metadata_if_same_generation(previous, installed_commit.as_deref());
                    (installed_commit.clone(), latest_commit, latest_commit_at)
                }
            }
        }
        CandidateKind::SkillsSh { skill_folder_hash } => {
            match tree_shas_cached(lookups.tree, lookups.tree_cache, &candidate.repo) {
                Ok(shas) => {
                    if let Some(sha) = shas.get(&candidate.path) {
                        (Some(skill_folder_hash.clone()), Some(sha.clone()), None)
                    } else {
                        // The tree call itself succeeded, so a missing path
                        // isn't a lookup failure - it means the folder this
                        // skill was installed from is gone from the repo's
                        // current tree. Name it rather than read as "no
                        // update" (`shas.get` returning `None` used to look
                        // identical to "already current" to `has_update`)
                        // (`a_skill_folder_missing_from_the_source_tree_reports_unknown_with_the_folder_named_or_names_the_silent_row`).
                        // `latest_commit` is dropped here rather than reused
                        // from a same-generation previous row: a stale
                        // "Update available" next to "not found" would read
                        // as two contradictory signals for the same row.
                        error = Some(format!(
                            "{} not found in {}'s source tree",
                            candidate.path, candidate.repo
                        ));
                        (Some(skill_folder_hash.clone()), None, None)
                    }
                }
                Err(e) if is_not_logged_in(&e) => {
                    stop.store(true, Ordering::SeqCst);
                    *not_logged_in_message
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(e);
                    let (latest_commit, _) = previous_metadata_if_same_generation(
                        previous,
                        Some(skill_folder_hash.as_str()),
                    );
                    (Some(skill_folder_hash.clone()), latest_commit, None)
                }
                Err(e) => {
                    error = Some(e);
                    let (latest_commit, _) = previous_metadata_if_same_generation(
                        previous,
                        Some(skill_folder_hash.as_str()),
                    );
                    (Some(skill_folder_hash.clone()), latest_commit, None)
                }
            }
        }
    };

    Some(SkillUpdateState {
        repo: candidate.repo.clone(),
        path: candidate.path.clone(),
        installed_commit,
        latest_commit,
        latest_commit_at,
        checked_at: now.to_string(),
        error,
    })
}

/// Build owner candidates, optionally filter by owner ID, check them in a
/// small worker pool, and write the result. A filtered run preserves every
/// other owner's previous state. `upstream_ahead` replaces the recorded fork
/// notes when `gh` works; otherwise the previous notes stay. Writes the store
/// exactly once, so a concurrent `clear_owner_after_update` is never undone.
fn run_update_check_impl(
    home: &Path,
    project_paths: &[PathBuf],
    app_data: &Path,
    lookup: &dyn CommitLookup,
    tree_lookup: &dyn TreeLookup,
    only_owner_ids: Option<&[String]>,
    upstream_ahead: Option<BTreeMap<String, UpstreamAhead>>,
) -> UpdateCheckStore {
    let previous = read_update_check_store(app_data);
    let now = Utc::now().to_rfc3339();

    let all_candidates = build_candidates(home, project_paths);
    let mut candidates = all_candidates.clone();
    if let Some(only) = only_owner_ids {
        candidates.retain(|candidate| only.contains(&candidate.owner_id));
    }

    let stop = AtomicBool::new(false);
    let not_logged_in_message: Mutex<Option<String>> = Mutex::new(None);
    let queue: Mutex<VecDeque<Candidate>> = Mutex::new(candidates.into_iter().collect());
    let computed: Mutex<BTreeMap<String, SkillUpdateState>> = Mutex::new(BTreeMap::new());
    let tree_cache: TreeCache = Mutex::new(HashMap::new());

    std::thread::scope(|scope| {
        for _ in 0..LOOKUP_POOL_SIZE {
            scope.spawn(|| loop {
                let next = queue
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .pop_front();
                let Some(candidate) = next else { break };

                let legacy_is_unambiguous = candidate.scope == InstallScope::Global
                    && all_candidates
                        .iter()
                        .filter(|other| other.name == candidate.name)
                        .count()
                        == 1;
                let prev_state = previous.owners.get(&candidate.owner_id).or_else(|| {
                    legacy_is_unambiguous
                        .then(|| previous.legacy_skills.get(&candidate.name))
                        .flatten()
                });
                let lookups = Lookups {
                    commit: lookup,
                    tree: tree_lookup,
                    tree_cache: &tree_cache,
                };
                if let Some(state) = check_candidate(
                    &candidate,
                    prev_state,
                    &lookups,
                    &stop,
                    &not_logged_in_message,
                    &now,
                ) {
                    computed
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .insert(candidate.owner_id.clone(), state);
                } else if let Some(state) = prev_state {
                    // Stop was already set before this one could be looked
                    // up; keep whatever we knew about it before.
                    computed
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .insert(candidate.owner_id.clone(), state.clone());
                }
            });
        }
    });

    let gh_status = if not_logged_in_message.into_inner().unwrap_or(None).is_some() {
        GhStatus::NotLoggedIn
    } else {
        GhStatus::Ok
    };

    let computed = computed
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let owners = if only_owner_ids.is_some() {
        let mut merged = previous.owners.clone();
        merged.extend(computed);
        merged
    } else {
        computed
    };

    let upstream_ahead = match upstream_ahead {
        Some(fresh) if gh_status == GhStatus::Ok => fresh,
        _ => prune_upstream_ahead(&previous.upstream_ahead, &all_candidates),
    };
    let store = UpdateCheckStore {
        version: update_store_version(),
        checked_at: Some(now),
        gh_status,
        owners,
        upstream_ahead,
        legacy_skills: BTreeMap::new(),
    };
    if let Err(e) = write_store(app_data, &store) {
        eprintln!("skill update check: failed to write store: {e}");
    }
    store
}

/// Check every dotagents/skills.sh skill for an upstream update, using
/// `lookup` and `tree_lookup` for the actual GitHub queries. Pure aside from
/// the filesystem reads/writes, so it's the unit under test.
pub fn run_update_check(
    home: &Path,
    app_data: &Path,
    lookup: &dyn CommitLookup,
    tree_lookup: &dyn TreeLookup,
) -> UpdateCheckStore {
    run_update_check_impl(home, &[], app_data, lookup, tree_lookup, None, None)
}

/// Check Global and the supplied registered Project Universal owners.
/// `upstream_ahead`, when given, is stored if `gh` turns out to be usable.
pub fn run_update_check_with_projects(
    home: &Path,
    project_paths: &[PathBuf],
    app_data: &Path,
    lookup: &dyn CommitLookup,
    tree_lookup: &dyn TreeLookup,
    upstream_ahead: Option<BTreeMap<String, UpstreamAhead>>,
) -> UpdateCheckStore {
    run_update_check_impl(
        home,
        project_paths,
        app_data,
        lookup,
        tree_lookup,
        None,
        upstream_ahead,
    )
}

/// `run_update_check_with_projects` with the fork lookups done first, so the
/// slow GitHub calls finish before the single store write.
fn run_update_check_with_forks(
    home: &Path,
    project_paths: &[PathBuf],
    app_data: &Path,
    lookup: &dyn CommitLookup,
    tree_lookup: &dyn TreeLookup,
    fork_lookup: &dyn ForkLookup,
) -> UpdateCheckStore {
    let previous = read_update_check_store(app_data);
    let upstream_ahead = find_upstream_ahead(
        &build_candidates(home, project_paths),
        fork_lookup,
        &previous.upstream_ahead,
    );
    run_update_check_with_projects(
        home,
        project_paths,
        app_data,
        lookup,
        tree_lookup,
        Some(upstream_ahead),
    )
}

/// Records `gh_status: Missing` and keeps every previously recorded state,
/// except fork notes whose skill no longer installs from the noted repo.
fn write_gh_missing_store(
    home: &Path,
    project_paths: &[PathBuf],
    app_data: &Path,
) -> UpdateCheckStore {
    let previous = read_update_check_store(app_data);
    let store = UpdateCheckStore {
        version: update_store_version(),
        checked_at: Some(Utc::now().to_rfc3339()),
        gh_status: GhStatus::Missing,
        owners: previous.owners,
        upstream_ahead: prune_upstream_ahead(
            &previous.upstream_ahead,
            &build_candidates(home, project_paths),
        ),
        legacy_skills: previous.legacy_skills,
    };
    if let Err(e) = write_store(app_data, &store) {
        eprintln!("skill update check: failed to write store: {e}");
    }
    store
}

/// Re-check exact lifecycle owners and preserve every other recorded owner.
/// Used after an update so same-named deployments in other scopes are not
/// queried or overwritten.
pub fn run_update_check_for_owners(
    home: &Path,
    project_paths: &[PathBuf],
    app_data: &Path,
    lookup: &dyn CommitLookup,
    tree_lookup: &dyn TreeLookup,
    owner_ids: &[String],
) -> UpdateCheckStore {
    run_update_check_impl(
        home,
        project_paths,
        app_data,
        lookup,
        tree_lookup,
        Some(owner_ids),
        None,
    )
}

/// Resolve `gh`, then run the check for real - the production entry point
/// `spawn_update_check_loop`'s `check_now` call reaches. When `gh` isn't
/// installed, writes `gh_status: Missing` without doing any lookups (and
/// without touching previously recorded skill states).
fn run_update_check_now(
    home: &Path,
    project_paths: &[PathBuf],
    app_data: &Path,
) -> UpdateCheckStore {
    if let Some(gh_bin) = resolve_gh_binary() {
        run_update_check_with_forks(
            home,
            project_paths,
            app_data,
            &GhCommitLookup {
                gh_bin: gh_bin.clone(),
            },
            &GhTreeLookup {
                gh_bin: gh_bin.clone(),
            },
            &GhForkLookup {
                gh_bin,
                timeout: FORK_LOOKUP_TIMEOUT,
            },
        )
    } else {
        write_gh_missing_store(home, project_paths, app_data)
    }
}

/// Shared "a check is already running" guard for the background loop's
/// `check_now` call.
#[derive(Clone, Default)]
pub struct UpdateCheckState {
    in_progress: std::sync::Arc<Mutex<bool>>,
}

impl UpdateCheckState {
    /// Attempts to claim the "in progress" flag; `false` when another check
    /// is already running.
    fn try_begin(&self) -> bool {
        let mut guard = self
            .in_progress
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *guard {
            false
        } else {
            *guard = true;
            true
        }
    }

    fn end(&self) {
        *self
            .in_progress
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = false;
    }
}

/// Run the check now, on the calling thread, then ask `skill_refresh` to
/// rebuild the snapshot so `has_update` reflects the result. Skips the run
/// (returning the last-known summary) when a check is already in flight, or
/// when `data_folder_writable` reports a blocking message - a newer data
/// folder `check_and_migrate` already refused to open must never receive a
/// stray write from the background update-check loop.
pub fn check_now(
    app: &AppHandle,
    state: &UpdateCheckState,
    refresh_state: &skill_refresh::SkillRefreshState,
) -> Result<UpdateCheckSummary, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Could not resolve app data dir: {e}"))?;

    if !crate::skills::data_folder_status::data_folder_writable(app) {
        return Ok(summarize(&read_update_check_store(&app_data)));
    }
    if !state.try_begin() {
        return Ok(summarize(&read_update_check_store(&app_data)));
    }
    let project_paths: Vec<PathBuf> = refresh_state
        .snapshot
        .read()
        .ok()
        .and_then(|snapshot| snapshot.as_ref().map(|snapshot| snapshot.projects.clone()))
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .collect();
    let store = run_update_check_now(&home, &project_paths, &app_data);
    state.end();

    skill_refresh::request_snapshot_rebuild(app);
    Ok(summarize(&store))
}

/// Write the just-updated commit into `owner_id`'s persisted update-check
/// state right after a successful `ops::update` (B1 - the review round 1
/// fix that replaced the old `check_now_for_owner` re-check; B2 - review
/// round 2's fix, which stopped removing the entry outright). Removing the
/// entry (the round-1 shape) exposed `state_for_owner`'s legacy-name
/// fallback: on a migrated v1 store whose background loop hasn't run
/// against this owner yet, there is no `owners` entry to remove, so the
/// fallback to `legacy_skills[skill_name]` kept serving the pre-update
/// commit pair and the badge came back. Reads the existing state from
/// `owners`, falling back to `legacy_skills` only when `legacy_fallback_name`
/// says `state_for_owner` would have used it too (N2, review round 3 - the
/// round-2 shape fell back on `skill_name` alone, so a Project owner with
/// no `owners` entry could read a Global-scoped legacy record it never
/// wrote), sets `installed_commit` to the already-recorded `latest_commit`
/// (the value `has_update` compares it against, for both a Dotagents
/// commit and a skills.sh tree hash), clears `error` (stale now that the
/// update succeeded), and writes it into `owners[owner_id]` - migrating a
/// legacy record forward so the next lookup finds it directly and the
/// fallback never gets a chance to re-serve the stale pair. A no-op (not
/// an error) when neither map has a record for this owner to update.
pub fn clear_owner_after_update(
    app_data: &Path,
    owner_id: &str,
    current_owner_ids: &[String],
) -> Result<(), String> {
    let path = update_check_path(app_data);
    let mut store = read_update_check_store_at(&path);
    let existing = store.owners.get(owner_id).cloned().or_else(|| {
        legacy_fallback_name(owner_id, current_owner_ids)
            .and_then(|name| store.legacy_skills.get(&name).cloned())
    });
    if let Some(mut state) = existing {
        state.installed_commit.clone_from(&state.latest_commit);
        state.checked_at = Utc::now().to_rfc3339();
        state.error = None;
        store.owners.insert(owner_id.to_string(), state);
        write_store(app_data, &store)?;
    }
    Ok(())
}

/// Start the background loop on its own thread: waits `INITIAL_DELAY`, checks,
/// sleeps `UPDATE_CHECK_INTERVAL`, repeats for the app's lifetime.
pub fn spawn_update_check_loop(app: AppHandle) {
    let state = UpdateCheckState::default();
    app.manage(state.clone());

    std::thread::spawn(move || {
        std::thread::sleep(INITIAL_DELAY);
        loop {
            let refresh_state = app.state::<skill_refresh::SkillRefreshState>();
            if let Err(e) = check_now(&app, &state, &refresh_state) {
                eprintln!("skill update check: check failed: {e}");
            }
            std::thread::sleep(UPDATE_CHECK_INTERVAL);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex as StdMutex;

    /// Flow: `GhTreeLookup`'s response parser sees `truncated: true` (a repo
    /// with more subtrees than one recursive listing covers).
    /// Expectation: `Err` naming the repo and "truncated", not an empty or
    /// partial map that would read as "no skill folder here".
    /// A failure here means the desktop tree lookup, unlike the host's
    /// `GhSourceTreeLookup`, never saw `truncated` at all (the old `--jq`
    /// program threw the field away before Rust ever read the response), so
    /// a real skill folder past the cutoff would silently read as unknown
    /// with no error on record, or names the wrong repo.
    #[test]
    fn truncated_tree_response_is_an_error_or_names_the_silent_truncation() {
        let body = serde_json::json!({
            "sha": "head-sha",
            "truncated": true,
            "tree": [{"path": "skills/a", "type": "tree", "sha": "sha-a"}]
        });
        let err = parse_tree_response("obra/write-tests", body.to_string().as_bytes()).unwrap_err();
        assert!(err.contains("obra/write-tests"));
        assert!(err.contains("truncated"));
    }

    /// Flow: the tree endpoint's real JSON shape, `tree[]` entries with a
    /// mix of `"tree"` and `"blob"` types.
    /// Expectation: every `"tree"`-typed entry is keyed by path, `"blob"`
    /// entries are skipped - the same parse the dropped `--jq` filter used
    /// to do, now done in Rust so `truncated` can be checked first.
    /// A failure here means a blob (file) entry leaked into the map, or a
    /// path/sha pair was dropped or swapped.
    #[test]
    fn tree_response_parses_tree_entries_by_path_or_names_the_dropped_entry() {
        let body = serde_json::json!({
            "sha": "head-sha",
            "truncated": false,
            "tree": [
                {"path": "skills/a", "type": "tree", "sha": "sha-a"},
                {"path": "skills/a/SKILL.md", "type": "blob", "sha": "sha-blob"},
                {"path": "skills/b", "type": "tree", "sha": "sha-b"},
            ]
        });
        let shas = parse_tree_response("obra/write-tests", body.to_string().as_bytes()).unwrap();
        assert_eq!(shas.get("skills/a"), Some(&"sha-a".to_string()));
        assert_eq!(shas.get("skills/b"), Some(&"sha-b".to_string()));
        assert_eq!(shas.len(), 2);
    }

    /// One scripted answer to a `CommitLookup::latest_commit` call: the sha
    /// and commit date on a hit, or the error message on a failure.
    type LookupAnswer = Result<Option<(String, String)>, String>;

    /// Records every `latest_commit` call and returns scripted answers by
    /// call index, so tests can assert both "what was asked" and "what came
    /// back".
    #[derive(Default)]
    struct FakeLookup {
        calls: StdMutex<Vec<(String, String, Option<String>)>>,
        answers: StdMutex<VecDeque<LookupAnswer>>,
    }

    impl FakeLookup {
        fn with_answers(answers: Vec<LookupAnswer>) -> Self {
            Self {
                calls: StdMutex::new(Vec::new()),
                answers: StdMutex::new(answers.into_iter().collect()),
            }
        }

        fn call_count(&self) -> usize {
            self.calls.lock().unwrap().len()
        }
    }

    impl CommitLookup for FakeLookup {
        fn latest_commit(
            &self,
            repo: &str,
            path: &str,
            until: Option<&str>,
        ) -> Result<Option<(String, String)>, String> {
            self.calls.lock().unwrap().push((
                repo.to_string(),
                path.to_string(),
                until.map(std::string::ToString::to_string),
            ));
            self.answers.lock().unwrap().pop_front().unwrap_or(Ok(None))
        }
    }

    struct RepoLookup;

    impl CommitLookup for RepoLookup {
        fn latest_commit(
            &self,
            repo: &str,
            _path: &str,
            _until: Option<&str>,
        ) -> Result<Option<(String, String)>, String> {
            Ok(Some((
                format!("latest-{repo}"),
                "2026-02-01T00:00:00Z".to_string(),
            )))
        }
    }

    struct AlwaysErrorLookup;

    impl CommitLookup for AlwaysErrorLookup {
        fn latest_commit(
            &self,
            _repo: &str,
            _path: &str,
            _until: Option<&str>,
        ) -> Result<Option<(String, String)>, String> {
            Err("offline".to_string())
        }
    }

    /// A `TreeLookup` for dotagents-only tests: fails loudly if a skills.sh
    /// candidate ever reaches it, since none of these fixtures should build
    /// one.
    struct UnusedTreeLookup;

    impl TreeLookup for UnusedTreeLookup {
        fn tree_shas_at_head_uncached(
            &self,
            repo: &str,
        ) -> Result<HashMap<String, String>, String> {
            panic!("no skills.sh candidate expected a tree lookup, got one for {repo}");
        }
    }

    /// Records every distinct repo `tree_shas_at_head_uncached` is called
    /// for, and returns the scripted tree for that repo. Used to test both
    /// currency results and the "one call per repo" guarantee.
    #[derive(Default)]
    struct FakeTreeLookup {
        calls: StdMutex<Vec<String>>,
        trees: StdMutex<HashMap<String, HashMap<String, String>>>,
    }

    impl FakeTreeLookup {
        fn with_tree(repo: &str, tree: HashMap<String, String>) -> Self {
            let trees = HashMap::from([(repo.to_string(), tree)]);
            Self {
                calls: StdMutex::new(Vec::new()),
                trees: StdMutex::new(trees),
            }
        }

        fn call_count(&self) -> usize {
            self.calls.lock().unwrap().len()
        }
    }

    impl TreeLookup for FakeTreeLookup {
        fn tree_shas_at_head_uncached(
            &self,
            repo: &str,
        ) -> Result<HashMap<String, String>, String> {
            self.calls.lock().unwrap().push(repo.to_string());
            Ok(self
                .trees
                .lock()
                .unwrap()
                .get(repo)
                .cloned()
                .unwrap_or_default())
        }
    }

    fn write_agents_lock(home: &Path, name: &str, repo: &str, path: &str, commit: &str) {
        write_agents_lock_in(&home.join(".agents"), name, repo, path, commit);
    }

    /// dotagents keeps a project's `agents.lock` in the project root.
    fn write_project_agents_lock(project: &Path, name: &str, repo: &str, path: &str, commit: &str) {
        write_agents_lock_in(project, name, repo, path, commit);
    }

    fn write_agents_lock_in(dir: &Path, name: &str, repo: &str, path: &str, commit: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(
            dir.join("agents.lock"),
            format!(
                r#"
[skills.{name}]
source = "{repo}"
resolved_path = "{path}"
resolved_commit = "{commit}"
"#
            ),
        )
        .unwrap();
    }

    fn write_skill_lock(
        home: &Path,
        name: &str,
        source: &str,
        skill_path: &str,
        folder_hash: &str,
    ) {
        fs::create_dir_all(home.join(".agents")).unwrap();
        let json = serde_json::json!({
            "version": 3,
            "skills": {
                name: {
                    "source": source,
                    "sourceType": "github",
                    "sourceUrl": format!("https://github.com/{source}"),
                    "skillPath": skill_path,
                    "skillFolderHash": folder_hash,
                    "installedAt": "2026-01-01T00:00:00Z",
                    "updatedAt": "2026-01-01T00:00:00Z",
                }
            }
        });
        fs::write(
            home.join(".agents/.skill-lock.json"),
            serde_json::to_string(&json).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn dotagents_newer_commit_has_update() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let app_data = tmp.path().join("data");
        write_agents_lock(
            &home,
            "find-bugs",
            "getsentry/find-bugs",
            "skills/find-bugs",
            "a".repeat(40).as_str(),
        );

        let lookup = FakeLookup::with_answers(vec![Ok(Some((
            "b".repeat(40),
            "2026-02-01T00:00:00Z".to_string(),
        )))]);
        let store = run_update_check(&home, &app_data, &lookup, &UnusedTreeLookup);

        let state = store.owners.get("owner:v1/global/find-bugs").unwrap();
        assert!(has_update(state));
        assert_eq!(
            state.installed_commit.as_deref(),
            Some("a".repeat(40).as_str())
        );
        assert_eq!(
            state.latest_commit.as_deref(),
            Some("b".repeat(40).as_str())
        );
    }

    #[test]
    fn dotagents_same_commit_has_no_update() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let app_data = tmp.path().join("data");
        let commit = "a".repeat(40);
        write_agents_lock(
            &home,
            "find-bugs",
            "getsentry/find-bugs",
            "skills/find-bugs",
            &commit,
        );

        let lookup = FakeLookup::with_answers(vec![Ok(Some((
            commit.clone(),
            "2026-02-01T00:00:00Z".to_string(),
        )))]);
        let store = run_update_check(&home, &app_data, &lookup, &UnusedTreeLookup);

        let state = store.owners.get("owner:v1/global/find-bugs").unwrap();
        assert!(!has_update(state));
    }

    /// A stale `skillFolderHash` (the lock file's baseline) against a
    /// different tree SHA at HEAD must surface as an update - a failure here
    /// means the tree-hash compare always trusts the lock file, or names the
    /// wrong skill's row.
    #[test]
    fn skills_sh_stale_hash_shows_update_available_or_names_the_missing_row() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let app_data = tmp.path().join("data");
        write_skill_lock(
            &home,
            "write-tests",
            "obra/write-tests",
            "apps/skills/extra/write-tests/SKILL.md",
            "old-hash",
        );

        let tree_lookup = FakeTreeLookup::with_tree(
            "obra/write-tests",
            HashMap::from([(
                "apps/skills/extra/write-tests".to_string(),
                "new-hash".to_string(),
            )]),
        );
        let store = run_update_check(&home, &app_data, &AlwaysErrorLookup, &tree_lookup);

        let state = store.owners.get("owner:v1/global/write-tests").unwrap();
        assert_eq!(state.installed_commit.as_deref(), Some("old-hash"));
        assert_eq!(state.latest_commit.as_deref(), Some("new-hash"));
        assert!(has_update(state));
    }

    /// A `skillFolderHash` that already matches the tree SHA must not report
    /// an update - a failure here means the compare gives a false positive
    /// on every skills.sh skill.
    #[test]
    fn skills_sh_current_hash_shows_no_update_or_names_the_false_positive() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let app_data = tmp.path().join("data");
        write_skill_lock(
            &home,
            "write-tests",
            "obra/write-tests",
            "apps/skills/extra/write-tests/SKILL.md",
            "same-hash",
        );

        let tree_lookup = FakeTreeLookup::with_tree(
            "obra/write-tests",
            HashMap::from([(
                "apps/skills/extra/write-tests".to_string(),
                "same-hash".to_string(),
            )]),
        );
        let store = run_update_check(&home, &app_data, &AlwaysErrorLookup, &tree_lookup);

        let state = store.owners.get("owner:v1/global/write-tests").unwrap();
        assert!(!has_update(state));
    }

    /// A skill folder absent from the repo's current tree must not read as
    /// "no update" - `shas.get` returning `None` used to look identical to
    /// an up-to-date compare, silently hiding the row from the user.
    ///
    /// A same-generation previous row (its `skillFolderHash` unchanged) must
    /// not leak its old `latest_commit` into this run either, even when that
    /// previous `latest_commit` already differed from the hash: pairing a
    /// stale "Update available" with "not found in ... source tree" reads as
    /// two contradictory signals for one row.
    /// A failure here means the missing-folder branch reused
    /// `previous_metadata_if_same_generation`'s `latest_commit` instead of
    /// dropping it, so `has_update` stayed true alongside the error.
    #[test]
    fn a_skill_folder_missing_from_the_source_tree_reports_unknown_with_the_folder_named_or_names_the_silent_row(
    ) {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let app_data = tmp.path().join("data");
        write_skill_lock(
            &home,
            "write-tests",
            "obra/write-tests",
            "apps/skills/extra/write-tests/SKILL.md",
            "old-hash",
        );

        let seeded = UpdateCheckStore {
            version: update_store_version(),
            checked_at: Some("2026-01-01T00:00:00Z".to_string()),
            gh_status: GhStatus::Ok,
            owners: BTreeMap::from([(
                "owner:v1/global/write-tests".to_string(),
                SkillUpdateState {
                    repo: "obra/write-tests".to_string(),
                    path: "apps/skills/extra/write-tests".to_string(),
                    installed_commit: Some("old-hash".to_string()),
                    latest_commit: Some("stale-newer-hash".to_string()),
                    latest_commit_at: None,
                    checked_at: "2026-01-01T00:00:00Z".to_string(),
                    error: None,
                },
            )]),
            upstream_ahead: BTreeMap::new(),
            legacy_skills: BTreeMap::new(),
        };
        write_store(&app_data, &seeded).unwrap();

        let tree_lookup = FakeTreeLookup::with_tree(
            "obra/write-tests",
            HashMap::from([("some/other/folder".to_string(), "new-hash".to_string())]),
        );
        let store = run_update_check(&home, &app_data, &AlwaysErrorLookup, &tree_lookup);

        let state = store.owners.get("owner:v1/global/write-tests").unwrap();
        assert_eq!(state.latest_commit, None);
        assert!(!has_update(state));
        let error = state.error.as_deref().unwrap_or_default();
        assert!(
            error.contains("apps/skills/extra/write-tests") && error.contains("obra/write-tests"),
            "expected the error to name the missing folder and repo, got {error:?}"
        );
    }

    /// Three skills.sh skills from the same source repo must cost exactly
    /// one tree lookup, not one per skill - a failure here means the desktop
    /// cache is keyed wrong (or missing) and every check re-fetches the
    /// whole repo tree per skill.
    #[test]
    fn skills_sh_tree_lookup_runs_once_per_repo_not_per_skill() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let app_data = tmp.path().join("data");
        fs::create_dir_all(home.join(".agents")).unwrap();
        let json = serde_json::json!({
            "version": 3,
            "skills": {
                "one": {
                    "source": "obra/write-tests", "sourceType": "github",
                    "sourceUrl": "https://github.com/obra/write-tests",
                    "skillPath": "apps/skills/one/SKILL.md",
                    "skillFolderHash": "hash-one",
                    "installedAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z",
                },
                "two": {
                    "source": "obra/write-tests", "sourceType": "github",
                    "sourceUrl": "https://github.com/obra/write-tests",
                    "skillPath": "apps/skills/two/SKILL.md",
                    "skillFolderHash": "hash-two",
                    "installedAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z",
                },
                "three": {
                    "source": "obra/write-tests", "sourceType": "github",
                    "sourceUrl": "https://github.com/obra/write-tests",
                    "skillPath": "apps/skills/three/SKILL.md",
                    "skillFolderHash": "hash-three",
                    "installedAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z",
                }
            }
        });
        fs::write(
            home.join(".agents/.skill-lock.json"),
            serde_json::to_string(&json).unwrap(),
        )
        .unwrap();

        let tree_lookup = FakeTreeLookup::with_tree("obra/write-tests", HashMap::new());
        run_update_check(&home, &app_data, &AlwaysErrorLookup, &tree_lookup);

        assert_eq!(tree_lookup.call_count(), 1);
    }

    /// Two skills.sh skills whose `source` differs only by case must still
    /// cost one tree lookup - a failure here means the desktop tree cache
    /// keys by raw repo spelling, so "Obra/Write-Tests" and
    /// "obra/write-tests" each pay for their own `gh api` call instead of
    /// sharing the one this repo already fetched.
    #[test]
    fn skills_sh_tree_lookup_normalises_repo_case_or_names_the_extra_call() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let app_data = tmp.path().join("data");
        fs::create_dir_all(home.join(".agents")).unwrap();
        let json = serde_json::json!({
            "version": 3,
            "skills": {
                "one": {
                    "source": "Obra/Write-Tests", "sourceType": "github",
                    "sourceUrl": "https://github.com/Obra/Write-Tests",
                    "skillPath": "apps/skills/one/SKILL.md",
                    "skillFolderHash": "hash-one",
                    "installedAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z",
                },
                "two": {
                    "source": "obra/write-tests", "sourceType": "github",
                    "sourceUrl": "https://github.com/obra/write-tests",
                    "skillPath": "apps/skills/two/SKILL.md",
                    "skillFolderHash": "hash-two",
                    "installedAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z",
                }
            }
        });
        fs::write(
            home.join(".agents/.skill-lock.json"),
            serde_json::to_string(&json).unwrap(),
        )
        .unwrap();

        let tree_lookup = FakeTreeLookup::with_tree("Obra/Write-Tests", HashMap::new());
        run_update_check(&home, &app_data, &AlwaysErrorLookup, &tree_lookup);

        assert_eq!(tree_lookup.call_count(), 1);
    }

    #[test]
    fn forked_skill_is_a_candidate_pinned_to_its_base_commit_and_wins_over_the_ledger() {
        use super::super::skill_fork_registry::{ForkRecord, ForkRegistry, OriginTool};

        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let app_data = tmp.path().join("data");
        // A ledger entry for the same name would normally win via dotagents,
        // but the fork must take precedence and use its own base_commit.
        write_agents_lock(
            &home,
            "find-bugs",
            "getsentry/find-bugs",
            "skills/find-bugs",
            "z".repeat(40).as_str(),
        );

        let mut registry = ForkRegistry::default();
        let base_commit = "a".repeat(40);
        registry.forks.insert(
            "find-bugs".to_string(),
            ForkRecord {
                deployment_id: String::new(),
                skill_dir: PathBuf::new(),
                forked_at: "2026-01-01T00:00:00Z".to_string(),
                origin_tool: OriginTool::Dotagents,
                origin_source: "getsentry/find-bugs".to_string(),
                repo: "getsentry/find-bugs".to_string(),
                path: "skills/find-bugs".to_string(),
                declared_ref: None,
                base_commit: base_commit.clone(),
            },
        );
        super::super::skill_fork_registry::write_fork_registry(&home, &registry).unwrap();

        let lookup = FakeLookup::with_answers(vec![Ok(Some((
            "b".repeat(40),
            "2026-02-01T00:00:00Z".to_string(),
        )))]);
        let store = run_update_check(&home, &app_data, &lookup, &UnusedTreeLookup);

        assert_eq!(lookup.call_count(), 1); // one candidate, not two
        let state = store.owners.get("owner:v1/global/find-bugs").unwrap();
        assert_eq!(
            state.installed_commit.as_deref(),
            Some(base_commit.as_str())
        );
        assert!(has_update(state));
    }

    #[test]
    fn non_github_source_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let app_data = tmp.path().join("data");
        fs::create_dir_all(home.join(".agents")).unwrap();
        let json = serde_json::json!({
            "version": 3,
            "skills": {
                "gitlab-skill": {
                    "source": "someorg/gitlab-skill",
                    "sourceType": "gitlab",
                    "sourceUrl": "https://gitlab.com/someorg/gitlab-skill",
                    "skillPath": "skill/SKILL.md",
                    "skillFolderHash": "abc",
                    "installedAt": "2026-01-01T00:00:00Z",
                    "updatedAt": "2026-01-01T00:00:00Z",
                }
            }
        });
        fs::write(
            home.join(".agents/.skill-lock.json"),
            serde_json::to_string(&json).unwrap(),
        )
        .unwrap();

        let lookup = FakeLookup::default();
        let store = run_update_check(&home, &app_data, &lookup, &UnusedTreeLookup);

        assert!(store.owners.is_empty());
        assert_eq!(lookup.call_count(), 0);
    }

    #[test]
    fn lookup_error_keeps_previous_commits_and_records_error() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let app_data = tmp.path().join("data");
        let commit = "a".repeat(40);
        write_agents_lock(
            &home,
            "find-bugs",
            "getsentry/find-bugs",
            "skills/find-bugs",
            &commit,
        );

        let previous_latest = "b".repeat(40);
        let seeded = UpdateCheckStore {
            version: update_store_version(),
            checked_at: Some("2026-01-01T00:00:00Z".to_string()),
            gh_status: GhStatus::Ok,
            owners: BTreeMap::from([(
                "owner:v1/global/find-bugs".to_string(),
                SkillUpdateState {
                    repo: "getsentry/find-bugs".to_string(),
                    path: "skills/find-bugs".to_string(),
                    installed_commit: Some(commit.clone()),
                    latest_commit: Some(previous_latest.clone()),
                    latest_commit_at: Some("2026-01-01T00:00:00Z".to_string()),
                    checked_at: "2026-01-01T00:00:00Z".to_string(),
                    error: None,
                },
            )]),
            upstream_ahead: BTreeMap::new(),
            legacy_skills: BTreeMap::new(),
        };
        write_store(&app_data, &seeded).unwrap();

        let lookup = FakeLookup::with_answers(vec![Err("network unreachable".to_string())]);
        let store = run_update_check(&home, &app_data, &lookup, &UnusedTreeLookup);

        let state = store.owners.get("owner:v1/global/find-bugs").unwrap();
        assert_eq!(
            state.latest_commit.as_deref(),
            Some(previous_latest.as_str())
        );
        assert_eq!(state.error.as_deref(), Some("network unreachable"));
    }

    /// Flow: a skill upgraded since the previous check (its `agents.lock`
    /// `installed_commit` moved), then a lookup error on the very next
    /// check.
    /// Expectation: `has_update()` is false - the previous run's
    /// `latest_commit` was computed against the old install and is not
    /// paired with the new one, so nothing false-positives as "update
    /// available" from a network error alone.
    /// A failure here means `previous.latest_commit` was reused across the
    /// generation boundary, pairing a stale "latest" with a fresh
    /// "installed" and flagging every post-upgrade lookup failure as an
    /// update.
    #[test]
    fn a_lookup_error_on_the_first_check_after_upgrade_never_flags_an_update_or_names_the_false_positive(
    ) {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let app_data = tmp.path().join("data");
        let old_commit = "a".repeat(40);
        let new_commit = "c".repeat(40);
        write_agents_lock(
            &home,
            "find-bugs",
            "getsentry/find-bugs",
            "skills/find-bugs",
            &new_commit,
        );

        let previous_latest = "b".repeat(40);
        let seeded = UpdateCheckStore {
            version: update_store_version(),
            checked_at: Some("2026-01-01T00:00:00Z".to_string()),
            gh_status: GhStatus::Ok,
            owners: BTreeMap::from([(
                "owner:v1/global/find-bugs".to_string(),
                SkillUpdateState {
                    repo: "getsentry/find-bugs".to_string(),
                    path: "skills/find-bugs".to_string(),
                    installed_commit: Some(old_commit),
                    latest_commit: Some(previous_latest),
                    latest_commit_at: Some("2026-01-01T00:00:00Z".to_string()),
                    checked_at: "2026-01-01T00:00:00Z".to_string(),
                    error: None,
                },
            )]),
            upstream_ahead: BTreeMap::new(),
            legacy_skills: BTreeMap::new(),
        };
        write_store(&app_data, &seeded).unwrap();

        let lookup = FakeLookup::with_answers(vec![Err("network unreachable".to_string())]);
        let store = run_update_check(&home, &app_data, &lookup, &UnusedTreeLookup);

        let state = store.owners.get("owner:v1/global/find-bugs").unwrap();
        assert_eq!(state.installed_commit.as_deref(), Some(new_commit.as_str()));
        assert_eq!(state.latest_commit, None);
        assert!(!has_update(state));
    }

    #[test]
    fn store_round_trips_through_the_file() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let app_data = tmp.path().join("data");
        write_agents_lock(
            &home,
            "find-bugs",
            "getsentry/find-bugs",
            "skills/find-bugs",
            "a".repeat(40).as_str(),
        );

        let lookup = FakeLookup::with_answers(vec![Ok(Some((
            "b".repeat(40),
            "2026-02-01T00:00:00Z".to_string(),
        )))]);
        run_update_check(&home, &app_data, &lookup, &UnusedTreeLookup);

        let reloaded = read_update_check_store(&app_data);
        let state = reloaded.owners.get("owner:v1/global/find-bugs").unwrap();
        assert_eq!(
            state.latest_commit.as_deref(),
            Some("b".repeat(40).as_str())
        );
        assert_eq!(reloaded.gh_status, GhStatus::Ok);
    }

    #[test]
    fn same_name_global_and_multiple_projects_have_independent_owner_state() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project_a = tmp.path().join("project-a");
        let project_b = tmp.path().join("project-b");
        let app_data = tmp.path().join("data");
        write_agents_lock(
            &home,
            "shared-name",
            "org/global",
            "skills/global",
            "global-old",
        );
        write_project_agents_lock(
            &project_a,
            "shared-name",
            "org/project-a",
            "skills/project-a",
            "project-a-old",
        );
        write_project_agents_lock(
            &project_b,
            "shared-name",
            "org/project-b",
            "skills/project-b",
            "project-b-old",
        );

        let store = run_update_check_with_projects(
            &home,
            &[project_a.clone(), project_b.clone()],
            &app_data,
            &RepoLookup,
            &UnusedTreeLookup,
            None,
        );
        let global = store.owners.get("owner:v1/global/shared-name").unwrap();
        let project_a_id = owner_id_for(
            &load_ownership_ledgers(&home, std::slice::from_ref(&project_a))[1],
            "shared-name",
        );
        let project_b_id = owner_id_for(
            &load_ownership_ledgers(&home, std::slice::from_ref(&project_b))[1],
            "shared-name",
        );
        let a = store.owners.get(&project_a_id).unwrap();
        let b = store.owners.get(&project_b_id).unwrap();

        assert_eq!(store.owners.len(), 3);
        assert_eq!(global.repo, "org/global");
        assert_eq!(global.installed_commit.as_deref(), Some("global-old"));
        assert_eq!(a.repo, "org/project-a");
        assert_eq!(a.installed_commit.as_deref(), Some("project-a-old"));
        assert_eq!(b.repo, "org/project-b");
        assert_eq!(b.installed_commit.as_deref(), Some("project-b-old"));
        assert_ne!(project_a_id, project_b_id);
    }

    #[test]
    fn legacy_name_state_migrates_only_for_one_unambiguous_global_owner() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let app_data = tmp.path().join("data");
        write_agents_lock(&home, "find-bugs", "org/global", "skills/find-bugs", "old");
        let path = update_check_path(&app_data);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            serde_json::json!({
                "checked_at": "2026-01-01T00:00:00Z",
                "gh_status": {"kind": "ok"},
                "skills": {
                    "find-bugs": {
                        "repo": "org/global",
                        "path": "skills/find-bugs",
                        "installed_commit": "old",
                        "latest_commit": "legacy-latest",
                        "latest_commit_at": "2026-01-01T00:00:00Z",
                        "checked_at": "2026-01-01T00:00:00Z",
                        "error": null
                    }
                }
            })
            .to_string(),
        )
        .unwrap();

        let store = run_update_check(&home, &app_data, &AlwaysErrorLookup, &UnusedTreeLookup);
        let state = store.owners.get("owner:v1/global/find-bugs").unwrap();
        assert_eq!(state.latest_commit.as_deref(), Some("legacy-latest"));
        let persisted: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(persisted["version"], 2);
        assert!(persisted.get("owners").is_some());
        assert!(persisted.get("skills").is_none());
    }

    #[test]
    fn legacy_name_state_is_not_adopted_when_global_and_project_names_collide() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let project = tmp.path().join("project");
        let app_data = tmp.path().join("data");
        write_agents_lock(&home, "same", "org/global", "skills/global", "global-old");
        write_project_agents_lock(
            &project,
            "same",
            "org/project",
            "skills/project",
            "project-old",
        );
        let path = update_check_path(&app_data);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            serde_json::json!({
                "checked_at": "2026-01-01T00:00:00Z",
                "gh_status": {"kind": "ok"},
                "skills": {
                    "same": {
                        "repo": "org/global",
                        "path": "skills/global",
                        "installed_commit": "global-old",
                        "latest_commit": "must-not-migrate",
                        "latest_commit_at": null,
                        "checked_at": "2026-01-01T00:00:00Z",
                        "error": null
                    }
                }
            })
            .to_string(),
        )
        .unwrap();

        let store = run_update_check_with_projects(
            &home,
            std::slice::from_ref(&project),
            &app_data,
            &AlwaysErrorLookup,
            &UnusedTreeLookup,
            None,
        );
        assert_eq!(store.owners.len(), 2);
        assert!(store
            .owners
            .values()
            .all(|state| state.latest_commit.is_none()));
    }

    #[test]
    fn not_logged_in_short_circuits() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let app_data = tmp.path().join("data");
        write_agents_lock(
            &home,
            "find-bugs",
            "getsentry/find-bugs",
            "skills/find-bugs",
            "a".repeat(40).as_str(),
        );

        let lookup = FakeLookup::with_answers(vec![Err(
            "gh: To get started with GitHub CLI, run: gh auth login".to_string(),
        )]);
        let store = run_update_check(&home, &app_data, &lookup, &UnusedTreeLookup);

        assert_eq!(store.gh_status, GhStatus::NotLoggedIn);
    }

    #[test]
    fn update_check_state_guard_rejects_second_concurrent_begin() {
        let state = UpdateCheckState::default();
        assert!(state.try_begin());
        assert!(
            !state.try_begin(),
            "a second begin must be rejected while the first is in progress"
        );
        state.end();
        assert!(
            state.try_begin(),
            "begin must succeed again once the first ends"
        );
    }

    #[test]
    fn concurrent_writers_use_distinct_temp_files_and_dont_clobber() {
        let tmp = tempfile::tempdir().unwrap();
        let app_data = tmp.path().to_path_buf();

        // Two writers racing `write_store` (e.g. a full check and a
        // per-skill check that both got past the guard) must each get their
        // own temp file, so neither's partial write can land in the other's
        // rename.
        std::thread::scope(|scope| {
            for i in 0..8 {
                let app_data = app_data.clone();
                scope.spawn(move || {
                    let store = UpdateCheckStore {
                        version: update_store_version(),
                        checked_at: Some(format!("run-{i}")),
                        gh_status: GhStatus::Ok,
                        owners: BTreeMap::new(),
                        upstream_ahead: BTreeMap::new(),
                        legacy_skills: BTreeMap::new(),
                    };
                    write_store(&app_data, &store).unwrap();
                });
            }
        });

        // The store file is valid JSON left by whichever writer finished
        // last - not a mix of two half-written payloads.
        let final_store = read_update_check_store(&app_data);
        assert!(final_store
            .checked_at
            .as_deref()
            .is_some_and(|c| c.starts_with("run-")));

        // No leftover temp files: every writer's rename succeeded.
        let leftover_temp_files = std::fs::read_dir(app_data.join("skill-studio"))
            .unwrap()
            .filter_map(std::result::Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp."))
            .count();
        assert_eq!(leftover_temp_files, 0);
    }

    // --- fork note: the original repo is ahead of the fork ----------------

    /// Fake `ForkLookup`: `repos` answers `repo_info`, `behind` answers the
    /// compare; every call is recorded so tests can assert what was (not) asked.
    struct FakeForkLookup {
        repos: HashMap<String, Result<RepoForkInfo, String>>,
        behind: HashMap<String, u32>,
        info_calls: StdMutex<Vec<String>>,
        compare_calls: StdMutex<Vec<String>>,
    }

    impl FakeForkLookup {
        fn new() -> Self {
            Self {
                repos: HashMap::new(),
                behind: HashMap::new(),
                info_calls: StdMutex::new(Vec::new()),
                compare_calls: StdMutex::new(Vec::new()),
            }
        }

        fn failing(mut self, repo: &str, message: &str) -> Self {
            self.repos
                .insert(repo.to_string(), Err(message.to_string()));
            self
        }

        fn fork(mut self, repo: &str, parent: &str, behind_by: u32) -> Self {
            self.repos.insert(
                repo.to_string(),
                Ok(RepoForkInfo {
                    default_branch: "main".to_string(),
                    parent: Some(ForkParent {
                        full_name: parent.to_string(),
                        default_branch: "main".to_string(),
                    }),
                }),
            );
            self.behind.insert(parent.to_string(), behind_by);
            self
        }
    }

    impl ForkLookup for FakeForkLookup {
        fn repo_info(&self, repo: &str) -> Result<RepoForkInfo, String> {
            self.info_calls.lock().unwrap().push(repo.to_string());
            self.repos
                .get(repo)
                .cloned()
                .unwrap_or_else(|| Err(format!("{repo}: HTTP 404")))
        }

        fn commits_behind(
            &self,
            base_repo: &str,
            _base_branch: &str,
            _head_owner: &str,
            _head_branch: &str,
        ) -> Result<u32, String> {
            self.compare_calls
                .lock()
                .unwrap()
                .push(base_repo.to_string());
            Ok(self.behind.get(base_repo).copied().unwrap_or(0))
        }
    }

    fn repo_candidate(name: &str, repo: &str) -> Candidate {
        Candidate {
            owner_id: format!("owner:v1/global/{name}"),
            name: name.to_string(),
            scope: InstallScope::Global,
            repo: repo.to_string(),
            path: format!("skills/{name}"),
            kind: CandidateKind::Dotagents {
                installed_commit: None,
            },
        }
    }

    #[test]
    fn a_fork_behind_its_original_yields_one_record_with_the_exact_compare_url() {
        let lookup =
            FakeForkLookup::new().fork("sergical/mattpocock-skills", "mattpocock/skills", 12);
        let found = find_upstream_ahead(
            &[repo_candidate("tdd", "sergical/mattpocock-skills")],
            &lookup,
            &BTreeMap::new(),
        );
        assert_eq!(
            found.into_values().collect::<Vec<_>>(),
            vec![UpstreamAhead {
                repo: "sergical/mattpocock-skills".to_string(),
                upstream_repo: "mattpocock/skills".to_string(),
                behind_by: 12,
                compare_url:
                    "https://github.com/sergical/mattpocock-skills/compare/main...mattpocock:skills:main"
                        .to_string(),
                owner_ids: vec!["owner:v1/global/tdd".to_string()],
            }]
        );
    }

    #[test]
    fn a_dotagents_candidate_with_no_skills_sh_lock_entry_puts_its_owner_id_in_the_record() {
        let home = tempfile::tempdir().unwrap();
        seed_one_dotagents_skill(home.path(), "me/fork");
        let lookup = FakeForkLookup::new().fork("me/fork", "them/orig", 3);
        let candidates = build_candidates(home.path(), &[]);
        let found = find_upstream_ahead(&candidates, &lookup, &BTreeMap::new());
        assert_eq!(
            found["me/fork"].owner_ids,
            vec![candidates[0].owner_id.clone()]
        );
    }

    #[test]
    fn owner_ids_cover_every_skill_from_the_repo_sorted_and_without_duplicates() {
        let lookup = FakeForkLookup::new().fork("me/fork", "them/orig", 1);
        let found = find_upstream_ahead(
            &[
                repo_candidate("b", "me/fork"),
                repo_candidate("a", "Me/Fork"),
                repo_candidate("b", "me/fork"),
            ],
            &lookup,
            &BTreeMap::new(),
        );
        assert_eq!(
            found["me/fork"].owner_ids,
            vec!["owner:v1/global/a", "owner:v1/global/b"]
        );
    }

    #[test]
    fn a_fork_level_with_its_original_yields_no_record() {
        let lookup = FakeForkLookup::new().fork("me/fork", "them/orig", 0);
        let found =
            find_upstream_ahead(&[repo_candidate("a", "me/fork")], &lookup, &BTreeMap::new());
        assert!(found.is_empty());
    }

    #[test]
    fn a_non_fork_yields_no_record_and_makes_no_compare_call() {
        let mut lookup = FakeForkLookup::new();
        lookup.repos.insert(
            "me/plain".to_string(),
            Ok(RepoForkInfo {
                default_branch: "main".to_string(),
                parent: None,
            }),
        );
        let found = find_upstream_ahead(
            &[repo_candidate("a", "me/plain")],
            &lookup,
            &BTreeMap::new(),
        );
        assert!(found.is_empty());
        assert!(lookup.compare_calls.lock().unwrap().is_empty());
    }

    #[test]
    fn a_failing_repo_lookup_is_skipped_while_another_repo_still_gets_its_record() {
        let lookup = FakeForkLookup::new().fork("me/good", "them/orig", 3);
        let found = find_upstream_ahead(
            &[
                repo_candidate("a", "me/gone"),
                repo_candidate("b", "me/good"),
            ],
            &lookup,
            &BTreeMap::new(),
        );
        assert_eq!(found.keys().collect::<Vec<_>>(), vec!["me/good"]);
    }

    #[test]
    fn two_skills_from_the_same_repo_cause_one_lookup() {
        let lookup = FakeForkLookup::new().fork("me/fork", "them/orig", 1);
        let found = find_upstream_ahead(
            &[
                repo_candidate("a", "me/fork"),
                repo_candidate("b", "Me/Fork"),
            ],
            &lookup,
            &BTreeMap::new(),
        );
        assert_eq!(found.len(), 1);
        assert_eq!(lookup.info_calls.lock().unwrap().len(), 1);
        assert_eq!(lookup.compare_calls.lock().unwrap().len(), 1);
    }

    #[test]
    fn an_invalid_owner_or_branch_is_skipped_without_a_call() {
        let lookup = FakeForkLookup::new();
        let found = find_upstream_ahead(
            &[
                repo_candidate("a", "bad owner/repo"),
                repo_candidate("b", "me/re?po"),
            ],
            &lookup,
            &BTreeMap::new(),
        );
        assert!(found.is_empty());
        assert!(lookup.info_calls.lock().unwrap().is_empty());

        let mut bad_branch = FakeForkLookup::new().fork("me/fork", "them/orig", 5);
        bad_branch.repos.insert(
            "me/fork".to_string(),
            Ok(RepoForkInfo {
                default_branch: "main?x=1".to_string(),
                parent: Some(ForkParent {
                    full_name: "them/orig".to_string(),
                    default_branch: "main".to_string(),
                }),
            }),
        );
        let found = find_upstream_ahead(
            &[repo_candidate("a", "me/fork")],
            &bad_branch,
            &BTreeMap::new(),
        );
        assert!(found.is_empty());
        assert!(bad_branch.compare_calls.lock().unwrap().is_empty());
    }

    fn previous_record(repo: &str, owner: &str, behind_by: u32) -> UpstreamAhead {
        UpstreamAhead {
            repo: repo.to_string(),
            upstream_repo: "them/orig".to_string(),
            behind_by,
            compare_url: format!("https://github.com/{repo}/compare/old"),
            owner_ids: vec![format!("owner:v1/global/{owner}")],
        }
    }

    fn previous_map(records: &[UpstreamAhead]) -> BTreeMap<String, UpstreamAhead> {
        records
            .iter()
            .map(|r| (r.repo.clone(), r.clone()))
            .collect()
    }

    #[test]
    fn a_failing_lookup_keeps_the_previous_record_for_that_repo() {
        let lookup = FakeForkLookup::new().failing("me/gone", "HTTP 502: Bad Gateway");
        let previous = previous_map(&[previous_record("me/gone", "a", 4)]);
        let found = find_upstream_ahead(&[repo_candidate("a", "me/gone")], &lookup, &previous);
        let mut expected = previous_record("me/gone", "a", 4);
        expected.owner_ids = vec!["owner:v1/global/a".to_string()];
        assert_eq!(found.into_values().collect::<Vec<_>>(), vec![expected]);
    }

    #[test]
    fn a_deleted_or_private_repo_loses_its_previous_record() {
        let lookup = FakeForkLookup::new().failing("me/gone", "gh: Not Found (HTTP 404)");
        let previous = previous_map(&[previous_record("me/gone", "a", 4)]);
        let found = find_upstream_ahead(&[repo_candidate("a", "me/gone")], &lookup, &previous);
        assert!(found.is_empty());
    }

    #[test]
    fn the_previous_record_is_found_by_normalised_repo_key() {
        let lookup = FakeForkLookup::new().failing("Me/Gone", "HTTP 502: Bad Gateway");
        let previous = previous_map(&[previous_record("me/gone", "a", 4)]);
        let found = find_upstream_ahead(&[repo_candidate("a", "Me/Gone")], &lookup, &previous);
        assert_eq!(found["me/gone"].behind_by, 4);
    }

    /// Fails the first repo with an auth error at once; every other repo
    /// waits until that failure was returned, then fails with a transient
    /// error, so the pool is still busy when the auth failure lands.
    struct AuthThenSlowForkLookup {
        calls: StdMutex<Vec<String>>,
        auth_failed: (StdMutex<bool>, std::sync::Condvar),
    }

    impl ForkLookup for AuthThenSlowForkLookup {
        fn repo_info(&self, repo: &str) -> Result<RepoForkInfo, String> {
            self.calls.lock().unwrap().push(repo.to_string());
            let (flag, condvar) = &self.auth_failed;
            if repo == "me/a" {
                *flag.lock().unwrap() = true;
                condvar.notify_all();
                return Err("To get started with GitHub CLI, run: gh auth login".to_string());
            }
            let mut failed = flag.lock().unwrap();
            while !*failed {
                failed = condvar.wait(failed).unwrap();
            }
            Err("HTTP 502: Bad Gateway".to_string())
        }

        fn commits_behind(&self, _: &str, _: &str, _: &str, _: &str) -> Result<u32, String> {
            unreachable!("repo_info never succeeds")
        }
    }

    #[test]
    fn an_auth_failure_stops_the_remaining_lookups_and_keeps_previous_records() {
        let lookup = AuthThenSlowForkLookup {
            calls: StdMutex::new(Vec::new()),
            auth_failed: (StdMutex::new(false), std::sync::Condvar::new()),
        };
        let candidates: Vec<Candidate> = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"]
            .iter()
            .map(|name| repo_candidate(name, &format!("me/{name}")))
            .collect();
        let previous = previous_map(&[
            previous_record("me/j", "j", 4),
            previous_record("me/a", "a", 2),
        ]);
        let found = find_upstream_ahead(&candidates, &lookup, &previous);
        assert_eq!(found.keys().collect::<Vec<_>>(), vec!["me/a", "me/j"]);
        assert!(
            lookup.calls.lock().unwrap().len() < candidates.len(),
            "an auth failure must stop the remaining lookups"
        );
    }

    #[test]
    fn a_fork_now_level_loses_its_previous_record() {
        let lookup = FakeForkLookup::new().fork("me/fork", "them/orig", 0);
        let previous = previous_map(&[previous_record("me/fork", "a", 4)]);
        let found = find_upstream_ahead(&[repo_candidate("a", "me/fork")], &lookup, &previous);
        assert!(found.is_empty());
    }

    #[test]
    fn a_successful_lookup_replaces_the_previous_record() {
        let lookup = FakeForkLookup::new().fork("me/fork", "them/orig", 9);
        let previous = previous_map(&[previous_record("me/fork", "a", 4)]);
        let found = find_upstream_ahead(&[repo_candidate("a", "me/fork")], &lookup, &previous);
        assert_eq!(found["me/fork"].behind_by, 9);
    }

    #[test]
    fn a_repo_no_longer_installed_loses_its_previous_record() {
        let lookup = FakeForkLookup::new().fork("me/kept", "them/orig", 2);
        let previous = previous_map(&[previous_record("me/removed", "a", 4)]);
        let found = find_upstream_ahead(&[repo_candidate("a", "me/kept")], &lookup, &previous);
        assert_eq!(found.keys().collect::<Vec<_>>(), vec!["me/kept"]);
    }

    #[test]
    fn a_skill_reinstalled_from_another_repo_loses_the_old_note_even_when_the_lookup_fails() {
        let lookup = FakeForkLookup::new().failing("them/orig", "HTTP 502: Bad Gateway");
        let previous = previous_map(&[previous_record("me/fork", "a", 4)]);
        let found = find_upstream_ahead(&[repo_candidate("a", "them/orig")], &lookup, &previous);
        assert!(found.is_empty(), "stale note survived: {found:?}");
    }

    #[test]
    fn a_kept_note_names_only_the_owners_still_installed_from_its_repo() {
        let lookup = FakeForkLookup::new().failing("me/fork", "HTTP 502: Bad Gateway");
        let mut record = previous_record("me/fork", "a", 4);
        record.owner_ids.push("owner:v1/global/b".to_string());
        let previous = previous_map(&[record]);
        let candidates = [
            repo_candidate("a", "me/fork"),
            repo_candidate("b", "them/orig"),
            repo_candidate("c", "me/fork"),
        ];
        let found = find_upstream_ahead(&candidates, &lookup, &previous);
        assert_eq!(
            found["me/fork"].owner_ids,
            vec!["owner:v1/global/a".to_string()]
        );
    }

    #[test]
    fn a_run_with_gh_not_logged_in_drops_the_note_of_a_skill_reinstalled_elsewhere() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let app_data = tmp.path().join("data");
        seed_one_dotagents_skill(&home, "them/orig");
        let seeded = UpdateCheckStore {
            upstream_ahead: previous_map(&[previous_record("me/fork", "tdd", 4)]),
            ..UpdateCheckStore::default()
        };
        write_store(&app_data, &seeded).unwrap();
        let lookup = FakeLookup::with_answers(vec![Err("run: gh auth login".to_string())]);

        let store = run_update_check_with_forks(
            &home,
            &[],
            &app_data,
            &lookup,
            &UnusedTreeLookup,
            &FakeForkLookup::new(),
        );

        assert_eq!(store.gh_status, GhStatus::NotLoggedIn);
        assert!(store.upstream_ahead.is_empty());
    }

    #[test]
    fn a_run_with_gh_missing_drops_the_note_of_a_skill_reinstalled_elsewhere() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let app_data = tmp.path().join("data");
        seed_one_dotagents_skill(&home, "them/orig");
        let seeded = UpdateCheckStore {
            upstream_ahead: previous_map(&[previous_record("me/fork", "tdd", 4)]),
            ..UpdateCheckStore::default()
        };
        write_store(&app_data, &seeded).unwrap();

        let store = write_gh_missing_store(&home, &[], &app_data);

        assert_eq!(store.gh_status, GhStatus::Missing);
        assert!(store.upstream_ahead.is_empty());
        assert!(read_update_check_store(&app_data).upstream_ahead.is_empty());
    }

    #[test]
    fn the_summary_hides_a_note_whose_skill_now_installs_from_another_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let store = UpdateCheckStore {
            upstream_ahead: previous_map(&[previous_record("me/fork", "tdd", 4)]),
            ..UpdateCheckStore::default()
        };

        seed_one_dotagents_skill(&home, "me/fork");
        assert_eq!(
            summarize_current(&store, &home, &[]).upstream_ahead.len(),
            1
        );

        seed_one_dotagents_skill(&home, "them/orig");
        assert!(summarize_current(&store, &home, &[])
            .upstream_ahead
            .is_empty());
    }

    #[test]
    fn a_stalled_fork_lookup_still_lets_owner_checks_run_and_the_store_persist() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let app_data = tmp.path().join("data");
        seed_one_dotagents_skill(&home, "me/fork");
        let seeded = UpdateCheckStore {
            upstream_ahead: previous_map(&[previous_record("me/fork", "tdd", 4)]),
            ..UpdateCheckStore::default()
        };
        write_store(&app_data, &seeded).unwrap();
        let lookup = FakeLookup::with_answers(vec![Ok(Some((
            "b".repeat(40),
            "2026-01-02T00:00:00Z".to_string(),
        )))]);

        let store = run_update_check_with_forks(
            &home,
            &[],
            &app_data,
            &lookup,
            &UnusedTreeLookup,
            &FakeForkLookup::new().failing(
                "me/fork",
                super::super::skill_process::PROCESS_TIMED_OUT_MESSAGE,
            ),
        );

        assert_eq!(store.gh_status, GhStatus::Ok);
        assert_eq!(lookup.call_count(), 1);
        let stored = read_update_check_store(&app_data);
        assert!(stored.checked_at.is_some());
        assert_eq!(stored.owners.len(), 1);
        assert_eq!(stored.upstream_ahead["me/fork"].behind_by, 4);
    }

    #[cfg(unix)]
    #[test]
    fn the_real_fork_lookup_gives_up_on_a_gh_that_never_answers() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let gh_bin = tmp.path().join("gh");
        fs::write(&gh_bin, "#!/bin/sh\nexec sleep 30\n").unwrap();
        fs::set_permissions(&gh_bin, fs::Permissions::from_mode(0o755)).unwrap();
        let lookup = GhForkLookup {
            gh_bin,
            timeout: Duration::from_millis(200),
        };

        let started = std::time::Instant::now();
        let error = lookup.repo_info("me/fork").unwrap_err();

        assert_eq!(
            error,
            super::super::skill_process::PROCESS_TIMED_OUT_MESSAGE
        );
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    /// Reads the store while the fork lookups run, to show nothing was
    /// written to it yet.
    struct StoreProbingForkLookup<'a> {
        app_data: &'a Path,
        inner: FakeForkLookup,
        checked_at_seen: StdMutex<Vec<Option<String>>>,
    }

    impl ForkLookup for StoreProbingForkLookup<'_> {
        fn repo_info(&self, repo: &str) -> Result<RepoForkInfo, String> {
            self.checked_at_seen
                .lock()
                .unwrap()
                .push(read_update_check_store(self.app_data).checked_at);
            self.inner.repo_info(repo)
        }

        fn commits_behind(
            &self,
            base_repo: &str,
            base_branch: &str,
            head_owner: &str,
            head_branch: &str,
        ) -> Result<u32, String> {
            self.inner
                .commits_behind(base_repo, base_branch, head_owner, head_branch)
        }
    }

    fn seed_one_dotagents_skill(home: &Path, repo: &str) {
        write_agents_lock(home, "tdd", repo, "skills/tdd", "a".repeat(40).as_str());
    }

    #[test]
    fn a_run_does_its_fork_lookups_before_its_only_store_write_and_stores_the_notes_in_it() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let app_data = tmp.path().join("data");
        seed_one_dotagents_skill(&home, "me/fork");
        let forks = StoreProbingForkLookup {
            app_data: &app_data,
            inner: FakeForkLookup::new().fork("me/fork", "them/orig", 7),
            checked_at_seen: StdMutex::new(Vec::new()),
        };

        let returned = run_update_check_with_forks(
            &home,
            &[],
            &app_data,
            &FakeLookup::with_answers(vec![Ok(None)]),
            &UnusedTreeLookup,
            &forks,
        );

        assert_eq!(*forks.checked_at_seen.lock().unwrap(), vec![None]);
        let stored = read_update_check_store(&app_data);
        assert_eq!(stored.upstream_ahead["me/fork"].behind_by, 7);
        assert_eq!(stored.upstream_ahead, returned.upstream_ahead);
    }

    #[test]
    fn a_run_with_gh_not_logged_in_keeps_the_previous_fork_notes() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let app_data = tmp.path().join("data");
        seed_one_dotagents_skill(&home, "me/fork");
        let seeded = UpdateCheckStore {
            upstream_ahead: previous_map(&[previous_record("me/fork", "tdd", 4)]),
            ..UpdateCheckStore::default()
        };
        write_store(&app_data, &seeded).unwrap();
        let lookup = FakeLookup::with_answers(vec![Err("run: gh auth login".to_string())]);

        let store = run_update_check_with_forks(
            &home,
            &[],
            &app_data,
            &lookup,
            &UnusedTreeLookup,
            &FakeForkLookup::new().fork("me/fork", "them/orig", 9),
        );

        assert_eq!(store.gh_status, GhStatus::NotLoggedIn);
        assert_eq!(store.upstream_ahead["me/fork"].behind_by, 4);
        assert_eq!(
            read_update_check_store(&app_data).upstream_ahead["me/fork"].behind_by,
            4
        );
    }

    // `run_update_check_now` reaches this when `resolve_gh_binary` finds no
    // `gh`; that call needs a login shell, so the branch is tested below it.
    #[test]
    fn a_run_with_gh_missing_keeps_the_previous_fork_notes_and_owner_states() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let app_data = tmp.path().join("data");
        seed_one_dotagents_skill(&home, "me/fork");
        let seeded = UpdateCheckStore {
            upstream_ahead: previous_map(&[previous_record("me/fork", "tdd", 4)]),
            ..UpdateCheckStore::default()
        };
        write_store(&app_data, &seeded).unwrap();

        let store = write_gh_missing_store(&home, &[], &app_data);

        assert_eq!(store.gh_status, GhStatus::Missing);
        assert_eq!(store.upstream_ahead, seeded.upstream_ahead);
        assert_eq!(
            read_update_check_store(&app_data).upstream_ahead,
            seeded.upstream_ahead
        );
    }

    #[test]
    fn a_fork_tsv_line_yields_the_parent_and_both_default_branches() {
        assert_eq!(
            parse_repo_fork_info("true\tmain\tthem/orig\ttrunk\n"),
            RepoForkInfo {
                default_branch: "main".to_string(),
                parent: Some(ForkParent {
                    full_name: "them/orig".to_string(),
                    default_branch: "trunk".to_string(),
                }),
            }
        );
    }

    #[test]
    fn a_non_fork_tsv_line_yields_no_parent() {
        assert_eq!(
            parse_repo_fork_info("false\tmain\t\t\n"),
            RepoForkInfo {
                default_branch: "main".to_string(),
                parent: None,
            }
        );
    }

    #[test]
    fn a_fork_tsv_line_with_an_empty_parent_yields_no_parent() {
        assert_eq!(parse_repo_fork_info("true\tmain\t\t\n").parent, None);
        assert_eq!(parse_repo_fork_info("true\tmain\n").parent, None);
    }

    #[test]
    fn an_old_cache_file_without_upstream_ahead_still_loads() {
        let tmp = tempfile::tempdir().unwrap();
        let path = update_check_path(tmp.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            r#"{"version":2,"checked_at":"t","gh_status":{"kind":"ok"},"owners":{}}"#,
        )
        .unwrap();
        let store = read_update_check_store(tmp.path());
        assert_eq!(store.checked_at.as_deref(), Some("t"));
        assert!(store.upstream_ahead.is_empty());
    }
}
