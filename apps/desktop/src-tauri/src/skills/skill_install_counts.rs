// ============================================================================
// Skills Module - Install Counts
// skills.sh install counts for installed skills-sh skills, cached on disk for
// 24 h and fetched in the background at a gentle pace. The cache is keyed by
// source + name and holds one number per skill, so any view that wants a
// skill's popularity (the detail header today) reads it from here.
// ============================================================================

use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::{OnceCell, OwnedSemaphorePermit, Semaphore};
use tokio::time::Instant;

use super::api::{self, SkillsShAccess};
use super::skill_dto::{InstallCount, InstallCountKey};

const CACHE_TTL_SECS: u64 = 24 * 60 * 60;

/// A real machine has hundreds of skills; at most three requests at once
/// across every lookup, each slot held for `REQUEST_SPACING` after its
/// request, keep the proxy's and GitHub's load flat.
const MAX_CONCURRENT_REQUESTS: usize = 3;
pub const REQUEST_SPACING: Duration = Duration::from_millis(250);

/// How long a failed or not-public answer blocks new requests for the same
/// repo or skill. It only ever blocks a call, so it cannot leak anything.
const NEGATIVE_TTL: Duration = Duration::from_secs(60 * 60);

/// A hung connection must not hold a lookup open; a timeout is a failed
/// fetch like any other and takes the stale-cache fallback.
const FETCH_TIMEOUT: Duration = Duration::from_secs(8);

/// One safe URL path segment: ASCII letters, digits, `.`, `_`, `-`, and not
/// `.` or `..`. No `%` or `/`, so nothing can encode a traversal or add path
/// levels.
fn is_safe_segment(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Splits a plain `owner/repo` source into its two safe segments. A git URL
/// or local path would leak a private source, so those give `None` and are
/// never looked up.
fn owner_repo(source: &str) -> Option<(&str, &str)> {
    let (owner, repo) = source.split_once('/')?;
    (is_safe_segment(owner) && is_safe_segment(repo)).then_some((owner, repo))
}

/// A key is looked up only when its source and its skill name are all safe
/// segments.
fn is_lookup_key(key: &InstallCountKey) -> bool {
    owner_repo(&key.source).is_some() && is_safe_segment(&key.name)
}

/// What GitHub said about a repo, asked with no credentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Visibility {
    Public,
    /// Private, missing, or any answer that is not a confirmed public repo.
    NotPublic,
    /// A 403/429: stop asking for this long.
    RateLimited(Duration),
}

/// Why a skills.sh count request produced no count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallsFailure {
    /// A 429: stop asking for this long.
    RateLimited(Duration),
    Other(String),
}

/// Fetches one skill's install count, after checking its repo is public.
/// `impl Future + Send` so the Tauri command's future stays `Send`.
pub trait InstallsApi: Send + Sync + 'static {
    /// Whether `source` (`owner/repo`) is a public GitHub repo. Anything but
    /// a confirmed `Public` stops the lookup: a private repo's name and skill
    /// must never reach skills.sh.
    fn is_public_repo(
        &self,
        source: &str,
    ) -> impl Future<Output = Result<Visibility, String>> + Send;

    fn installs(
        &self,
        source: &str,
        name: &str,
    ) -> impl Future<Output = Result<u32, InstallsFailure>> + Send;
}

/// The real `InstallsApi`: skills.sh's details endpoint
/// (`GET /skills/{owner/repo}/{slug}`), which carries `installs`.
pub struct SkillsShInstallsApi {
    pub access: SkillsShAccess,
}

impl InstallsApi for SkillsShInstallsApi {
    /// `GET https://api.github.com/repos/{owner}/{repo}` with no token (not
    /// even `gh`'s): only a 200 with `"private": false` counts as public, so a
    /// 404, rate limit, or parse failure all fail closed. A 403 or 429 also
    /// reports how long to stop asking.
    async fn is_public_repo(&self, source: &str) -> Result<Visibility, String> {
        let response = reqwest::Client::new()
            .get(format!("https://api.github.com/repos/{source}"))
            .header(reqwest::header::USER_AGENT, "AgentStudio/0.1.0")
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = response.status();
        if status == reqwest::StatusCode::FORBIDDEN
            || status == reqwest::StatusCode::TOO_MANY_REQUESTS
        {
            return Ok(Visibility::RateLimited(api::rate_limit_wait(
                response.headers(),
            )));
        }
        if status != reqwest::StatusCode::OK {
            return Ok(Visibility::NotPublic);
        }
        let body: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
        Ok(
            if body.get("private") == Some(&serde_json::Value::Bool(false)) {
                Visibility::Public
            } else {
                Visibility::NotPublic
            },
        )
    }

    async fn installs(&self, source: &str, name: &str) -> Result<u32, InstallsFailure> {
        let (owner, repo) = owner_repo(source)
            .ok_or_else(|| InstallsFailure::Other("not an owner/repo source".to_string()))?;
        match api::get_skill_details_by_segments(&self.access, owner, repo, name).await {
            Ok(details) => Ok(details.installs),
            Err(failure) => Err(match failure.rate_limit_wait {
                Some(wait) => InstallsFailure::RateLimited(wait),
                None => InstallsFailure::Other(failure.message),
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    installs: u32,
    fetched_at: u64,
}

type Cache = HashMap<String, CacheEntry>;

/// Serializes the read-merge-write of the cache file across concurrent
/// lookups; held only for the file I/O, never across a request.
static CACHE_FILE_LOCK: Mutex<()> = Mutex::new(());

fn cache_key(key: &InstallCountKey) -> String {
    format!("{}/{}", key.source, key.name)
}

/// A missing or unreadable cache is an empty one: the counts are a courtesy,
/// so a damaged file just costs a refetch.
fn load_cache(path: &Path) -> Cache {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_fresh_entries(path: &Path, fresh: Cache) {
    if fresh.is_empty() {
        return;
    }
    let _guard = CACHE_FILE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut cache = load_cache(path);
    cache.extend(fresh);
    let Ok(text) = serde_json::to_string(&cache) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

fn is_fresh(entry: &CacheEntry, now: u64) -> bool {
    // An entry stamped in the future (clock moved back) cannot be trusted.
    entry.fetched_at <= now && now - entry.fetched_at < CACHE_TTL_SECS
}

/// Concurrent callers of the same key share one run of the work and its
/// result; the entry is dropped when the work finishes, so a later call runs
/// the work again.
struct Inflight<T> {
    cells: Mutex<HashMap<String, Arc<OnceCell<T>>>>,
}

impl<T: Clone> Inflight<T> {
    fn new() -> Self {
        Self {
            cells: Mutex::new(HashMap::new()),
        }
    }

    async fn run(&self, key: &str, work: impl Future<Output = T>) -> T {
        let cell = Arc::clone(
            self.cells
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .entry(key.to_string())
                .or_default(),
        );
        let value = cell.get_or_init(|| work).await.clone();
        let mut cells = self
            .cells
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if cells.get(key).is_some_and(|c| Arc::ptr_eq(c, &cell)) {
            cells.remove(key);
        }
        value
    }
}

#[derive(Default)]
struct SchedulerState {
    github_blocked_until: Option<Instant>,
    skills_blocked_until: Option<Instant>,
    /// "not public", failed or no-count answers, by key, with their expiry.
    /// A public verdict is never stored: a repo can turn private.
    negative: HashMap<String, Instant>,
}

/// The one process-wide gate in front of GitHub and skills.sh, all in memory:
/// every lookup shares its concurrency limit, in-flight requests, rate-limit
/// cooldowns and negative answers, so reopening skill pages over and over
/// cannot drain GitHub's anonymous 60 requests an hour.
pub struct InstallScheduler {
    spacing: Duration,
    permits: Arc<Semaphore>,
    state: Mutex<SchedulerState>,
    visibility_inflight: Inflight<bool>,
    count_inflight: Inflight<Option<u32>>,
}

/// The scheduler the Tauri command uses.
pub fn shared_scheduler() -> Arc<InstallScheduler> {
    static SCHEDULER: OnceLock<Arc<InstallScheduler>> = OnceLock::new();
    Arc::clone(SCHEDULER.get_or_init(|| Arc::new(InstallScheduler::new(REQUEST_SPACING))))
}

impl InstallScheduler {
    pub fn new(spacing: Duration) -> Self {
        Self {
            spacing,
            permits: Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS)),
            state: Mutex::new(SchedulerState::default()),
            visibility_inflight: Inflight::new(),
            count_inflight: Inflight::new(),
        }
    }

    fn with_state<R>(&self, f: impl FnOnce(&mut SchedulerState) -> R) -> R {
        f(&mut self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner))
    }

    fn is_negative(&self, id: &str) -> bool {
        self.with_state(|state| match state.negative.get(id) {
            Some(expires) if Instant::now() < *expires => true,
            Some(_) => {
                state.negative.remove(id);
                false
            }
            None => false,
        })
    }

    fn remember_negative(&self, id: &str) {
        self.with_state(|state| {
            state
                .negative
                .insert(id.to_string(), Instant::now() + NEGATIVE_TTL);
        });
    }

    fn github_blocked(&self) -> bool {
        self.with_state(|state| state.github_blocked_until)
            .is_some_and(|until| Instant::now() < until)
    }

    fn skills_blocked(&self) -> bool {
        self.with_state(|state| state.skills_blocked_until)
            .is_some_and(|until| Instant::now() < until)
    }

    async fn acquire(&self) -> Option<OwnedSemaphorePermit> {
        Arc::clone(&self.permits).acquire_owned().await.ok()
    }

    /// Frees the slot `spacing` after the request, without making the caller
    /// wait for it.
    fn release_after_spacing(&self, permit: OwnedSemaphorePermit) {
        let spacing = self.spacing;
        tokio::spawn(async move {
            tokio::time::sleep(spacing).await;
            drop(permit);
        });
    }

    /// Whether GitHub confirms `source` is public right now. Anything else -
    /// private, unknown, failed, rate limited, still cooling down - is `false`.
    /// Only the call that actually asks GitHub takes a slot; on a `Public`
    /// answer it leaves that slot in `held`, so the caller keeps it through the
    /// skills.sh request. A caller that waits on another's answer holds none.
    async fn is_public<A: InstallsApi>(
        &self,
        api: &A,
        source: &str,
        held: &mut Option<OwnedSemaphorePermit>,
    ) -> bool {
        let id = format!("repo:{source}");
        if self.is_negative(&id) || self.github_blocked() {
            return false;
        }
        self.visibility_inflight
            .run(&id, async {
                let Some(permit) = self.acquire().await else {
                    return false;
                };
                // A request that queued for a slot may have waited out a 429.
                if self.github_blocked() {
                    return false;
                }
                let verdict = tokio::time::timeout(FETCH_TIMEOUT, api.is_public_repo(source)).await;
                if matches!(verdict, Ok(Ok(Visibility::Public))) {
                    *held = Some(permit);
                    return true;
                }
                self.release_after_spacing(permit);
                if let Ok(Ok(Visibility::RateLimited(wait))) = verdict {
                    self.with_state(|state| {
                        state.github_blocked_until = Some(Instant::now() + wait);
                    });
                } else {
                    self.remember_negative(&id);
                }
                false
            })
            .await
    }

    /// One skill's count, or `None` when it cannot be asked for or answered.
    async fn installs<A: InstallsApi>(&self, api: &A, key: &InstallCountKey) -> Option<u32> {
        let id = format!("skill:{}", cache_key(key));
        let repo_id = format!("repo:{}", key.source);
        if self.is_negative(&id) || self.skills_blocked() {
            return None;
        }
        self.count_inflight
            .run(&id, async {
                let mut held = None;
                if !self.is_public(api, &key.source, &mut held).await {
                    return None;
                }
                let permit = match held {
                    Some(permit) => permit,
                    None => self.acquire().await?,
                };
                // The verdict is only as old as the moment it was given: another
                // task may have learned the repo is private, or hit a cooldown,
                // while this one waited.
                if self.is_negative(&repo_id)
                    || self.is_negative(&id)
                    || self.github_blocked()
                    || self.skills_blocked()
                {
                    return None;
                }
                let result =
                    tokio::time::timeout(FETCH_TIMEOUT, api.installs(&key.source, &key.name)).await;
                self.release_after_spacing(permit);
                match result {
                    Ok(Ok(installs)) => Some(installs),
                    Ok(Err(InstallsFailure::RateLimited(wait))) => {
                        self.with_state(|state| {
                            state.skills_blocked_until = Some(Instant::now() + wait);
                        });
                        None
                    }
                    _ => {
                        self.remember_negative(&id);
                        None
                    }
                }
            })
            .await
    }
}

/// Looks up each key's install count: a cache entry younger than 24 h is
/// used as-is; the rest go through `scheduler`, which shares its pacing,
/// in-flight requests, cooldowns and negative answers with every other call.
/// Before any skills.sh request, GitHub must confirm the repo is public right
/// now (never cached, since a repo can turn private); otherwise nothing is
/// sent. A failed fetch (offline, unknown skill, private repo, rate limit)
/// falls back to a stale cached count, else `None` - never an error. Results
/// keep the order of `keys`.
pub async fn lookup_install_counts<A: InstallsApi>(
    api: Arc<A>,
    scheduler: Arc<InstallScheduler>,
    cache_path: &Path,
    keys: Vec<InstallCountKey>,
    now: u64,
) -> Vec<InstallCount> {
    let cache = load_cache(cache_path);
    let mut counts: HashMap<String, Option<u32>> = HashMap::new();
    let mut queue: Vec<InstallCountKey> = Vec::new();
    for key in &keys {
        let id = cache_key(key);
        if counts.contains_key(&id) {
            continue;
        }
        if !is_lookup_key(key) {
            counts.insert(id, None);
            continue;
        }
        match cache.get(&id) {
            Some(entry) if is_fresh(entry, now) => {
                counts.insert(id, Some(entry.installs));
            }
            _ => {
                counts.insert(id, None);
                queue.push(key.clone());
            }
        }
    }

    let mut lookups = tokio::task::JoinSet::new();
    for key in queue {
        let api = Arc::clone(&api);
        let scheduler = Arc::clone(&scheduler);
        lookups.spawn(async move {
            let installs = scheduler.installs(&*api, &key).await;
            (key, installs)
        });
    }

    let mut fresh = Cache::new();
    while let Some(joined) = lookups.join_next().await {
        if let Ok((key, result)) = joined {
            let id = cache_key(&key);
            match result {
                Some(installs) => {
                    fresh.insert(
                        id.clone(),
                        CacheEntry {
                            installs,
                            fetched_at: now,
                        },
                    );
                    counts.insert(id, Some(installs));
                }
                None => {
                    counts.insert(id.clone(), cache.get(&id).map(|e| e.installs));
                }
            }
        }
    }
    save_fresh_entries(cache_path, fresh);

    keys.into_iter()
        .map(|key| {
            let installs = counts.get(&cache_key(&key)).copied().flatten();
            InstallCount {
                source: key.source,
                name: key.name,
                installs,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeApi {
        installs: Option<u32>,
        calls: AtomicUsize,
        /// What GitHub says about every repo.
        visibility: Result<Visibility, String>,
        visibility_calls: AtomicUsize,
    }

    impl FakeApi {
        fn answering(installs: Option<u32>) -> Arc<Self> {
            Self::with_visibility(installs, Ok(Visibility::Public))
        }

        fn with_visibility(
            installs: Option<u32>,
            visibility: Result<Visibility, String>,
        ) -> Arc<Self> {
            Arc::new(Self {
                installs,
                calls: AtomicUsize::new(0),
                visibility,
                visibility_calls: AtomicUsize::new(0),
            })
        }
    }

    impl InstallsApi for FakeApi {
        async fn is_public_repo(&self, _source: &str) -> Result<Visibility, String> {
            self.visibility_calls.fetch_add(1, Ordering::SeqCst);
            // A real answer takes a network round trip; yielding lets a
            // concurrent lookup of the same repo arrive while this one is open.
            tokio::task::yield_now().await;
            self.visibility.clone()
        }

        async fn installs(&self, _source: &str, _name: &str) -> Result<u32, InstallsFailure> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.installs
                .ok_or_else(|| InstallsFailure::Other("offline".to_string()))
        }
    }

    fn key(source: &str, name: &str) -> InstallCountKey {
        InstallCountKey {
            source: source.to_string(),
            name: name.to_string(),
        }
    }

    fn scheduler() -> Arc<InstallScheduler> {
        Arc::new(InstallScheduler::new(Duration::ZERO))
    }

    /// One lookup on its own fresh scheduler, so no memory carries between
    /// calls; tests of that shared memory pass their own scheduler.
    async fn lookup(api: &Arc<FakeApi>, path: &Path, now: u64) -> Vec<InstallCount> {
        lookup_with(api, &scheduler(), path, "write-tests", now).await
    }

    async fn lookup_with(
        api: &Arc<FakeApi>,
        scheduler: &Arc<InstallScheduler>,
        path: &Path,
        name: &str,
        now: u64,
    ) -> Vec<InstallCount> {
        lookup_install_counts(
            Arc::clone(api),
            Arc::clone(scheduler),
            path,
            vec![key("obra/write-tests", name)],
            now,
        )
        .await
    }

    async fn lookup_keys<A: InstallsApi>(
        api: &Arc<A>,
        path: &Path,
        keys: Vec<InstallCountKey>,
        now: u64,
    ) -> Vec<InstallCount> {
        lookup_install_counts(Arc::clone(api), scheduler(), path, keys, now).await
    }

    /// Flow: the same skill is looked up twice within 24 h.
    /// Expectation: the second lookup reads the cache and makes no request.
    /// A failure here means the cache was not written or its TTL check is off,
    /// so a 380-skill machine would re-hit skills.sh on every open.
    #[tokio::test]
    async fn a_count_younger_than_24_hours_is_served_from_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install-counts.json");
        let api = FakeApi::answering(Some(1200));

        let first = lookup(&api, &path, 1_000).await;
        let second = lookup(&api, &path, 1_000 + CACHE_TTL_SECS - 1).await;

        assert_eq!(first[0].installs, Some(1200));
        assert_eq!(second[0].installs, Some(1200));
        assert_eq!(api.calls.load(Ordering::SeqCst), 1);
    }

    /// Flow: a cached count is exactly 24 h old.
    /// Expectation: it is fetched again and the cache holds the new number.
    /// A failure here means counts never refresh after the first fetch.
    #[tokio::test]
    async fn a_count_24_hours_old_is_fetched_again() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install-counts.json");

        lookup(&FakeApi::answering(Some(10)), &path, 1_000).await;
        let api = FakeApi::answering(Some(25));
        let later = lookup(&api, &path, 1_000 + CACHE_TTL_SECS).await;

        assert_eq!(later[0].installs, Some(25));
        assert_eq!(api.calls.load(Ordering::SeqCst), 1);
    }

    /// Flow: skills.sh is unreachable and nothing is cached.
    /// Expectation: `installs` is `None`, with no panic and no cache file.
    /// A failure here means offline use surfaces an error or caches a bogus 0.
    #[tokio::test]
    async fn offline_with_no_cache_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install-counts.json");

        let result = lookup(&FakeApi::answering(None), &path, 1_000).await;

        assert_eq!(result[0].installs, None);
        assert!(!path.exists());
    }

    /// Flow: skills.sh is unreachable and the cached count is stale.
    /// Expectation: the stale count is still shown.
    /// A failure here means going offline blanks counts the user already saw.
    #[tokio::test]
    async fn offline_keeps_a_stale_count() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install-counts.json");
        lookup(&FakeApi::answering(Some(77)), &path, 1_000).await;

        let result = lookup(&FakeApi::answering(None), &path, 1_000 + CACHE_TTL_SECS + 5).await;

        assert_eq!(result[0].installs, Some(77));
    }

    /// Flow: a batch with a repeated key and distinct keys.
    /// Expectation: one request per distinct key, results in input order.
    /// A failure here means a repeat costs a request or rows get another
    /// skill's count.
    #[tokio::test]
    async fn a_batch_makes_one_request_per_distinct_key_and_keeps_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install-counts.json");
        let api = FakeApi::answering(Some(5));
        let keys = vec![key("a/b", "one"), key("a/b", "two"), key("a/b", "one")];

        let result = lookup_keys(&api, &path, keys, 1_000).await;

        let names: Vec<_> = result.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["one", "two", "one"]);
        assert_eq!(api.calls.load(Ordering::SeqCst), 2);
    }

    /// Flow: a cached count is stamped in the future (the clock moved back).
    /// Expectation: it is fetched again, not trusted for up to 24 h.
    /// A failure here means a clock change pins a count indefinitely.
    #[tokio::test]
    async fn a_count_stamped_in_the_future_is_fetched_again() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install-counts.json");
        lookup(&FakeApi::answering(Some(10)), &path, 50_000).await;
        let api = FakeApi::answering(Some(25));

        let result = lookup(&api, &path, 1_000).await;

        assert_eq!(result[0].installs, Some(25));
        assert_eq!(api.calls.load(Ordering::SeqCst), 1);
    }

    /// Flow: the cache file holds garbage.
    /// Expectation: it reads as empty, the lookup still answers, and the next
    /// write replaces it with a valid cache.
    /// A failure here means one bad write disables counts for good.
    #[tokio::test]
    async fn a_corrupt_cache_file_reads_as_empty_and_is_repaired() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install-counts.json");
        std::fs::write(&path, "{not json").unwrap();
        let api = FakeApi::answering(Some(9));

        let first = lookup(&api, &path, 1_000).await;
        let second = lookup(&api, &path, 1_001).await;

        assert_eq!(first[0].installs, Some(9));
        assert_eq!(second[0].installs, Some(9));
        assert_eq!(api.calls.load(Ordering::SeqCst), 1);
    }

    /// Flow: sources that are a git URL, a local path, or not `owner/repo`.
    /// Expectation: nothing is fetched and no count comes back.
    /// A failure here means a private source string is sent to skills.sh.
    #[tokio::test]
    async fn a_source_that_is_not_owner_repo_is_never_fetched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install-counts.json");
        let api = FakeApi::answering(Some(5));
        let keys = [
            "https://git.example.com/team/repo.git",
            "/Users/me/skills",
            "local",
            "a/b/c",
        ]
        .into_iter()
        .map(|source| key(source, "s"))
        .collect();

        let result = lookup_keys(&api, &path, keys, 1_000).await;

        assert!(result.iter().all(|c| c.installs.is_none()));
        assert_eq!(api.calls.load(Ordering::SeqCst), 0);
    }

    struct HangingApi;

    impl InstallsApi for HangingApi {
        async fn is_public_repo(&self, _source: &str) -> Result<Visibility, String> {
            Ok(Visibility::Public)
        }

        async fn installs(&self, _source: &str, name: &str) -> Result<u32, InstallsFailure> {
            if name == "slow" {
                std::future::pending::<()>().await;
            }
            Ok(5)
        }
    }

    /// Answers `Public` only once its gate opens, like a slow GitHub reply.
    struct GatedApi {
        gate: tokio::sync::Notify,
        calls: AtomicUsize,
    }

    impl InstallsApi for GatedApi {
        async fn is_public_repo(&self, _source: &str) -> Result<Visibility, String> {
            self.gate.notified().await;
            Ok(Visibility::Public)
        }

        async fn installs(&self, _source: &str, _name: &str) -> Result<u32, InstallsFailure> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(5)
        }
    }

    /// Flow: task A is told `Public` but has not yet sent its skills.sh
    /// request; meanwhile another task records the repo as not public.
    /// Expectation: A sends nothing to skills.sh.
    /// A failure here means a repo that just turned private still has its
    /// name and skill sent to skills.sh by a task that was already queued.
    #[tokio::test]
    async fn a_newer_not_public_verdict_stops_a_task_holding_a_public_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install-counts.json");
        let api = Arc::new(GatedApi {
            gate: tokio::sync::Notify::new(),
            calls: AtomicUsize::new(0),
        });
        let scheduler = scheduler();
        let task_a = tokio::spawn({
            let (api, scheduler) = (Arc::clone(&api), Arc::clone(&scheduler));
            async move {
                lookup_install_counts(
                    api,
                    scheduler,
                    &path,
                    vec![key("obra/write-tests", "a")],
                    1_000,
                )
                .await
            }
        });
        tokio::task::yield_now().await;
        scheduler.remember_negative("repo:obra/write-tests");
        api.gate.notify_one();

        let result = task_a.await.unwrap();

        assert_eq!(api.calls.load(Ordering::SeqCst), 0);
        assert_eq!(result[0].installs, None);
    }

    /// Flow: more skills of one repo than there are request slots, all
    /// looked up at once.
    /// Expectation: every skill gets its count.
    /// A failure here means tasks waiting on one repo's visibility answer hold
    /// the slots the answering task needs, and the lookup deadlocks.
    #[tokio::test(start_paused = true)]
    async fn waiters_on_one_repo_do_not_starve_the_task_asking_github() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install-counts.json");
        let keys: Vec<_> = (0..MAX_CONCURRENT_REQUESTS * 2)
            .map(|i| key("obra/write-tests", &format!("skill-{i}")))
            .collect();

        let result = lookup_keys(&FakeApi::answering(Some(9)), &path, keys, 1_000).await;

        assert!(result.iter().all(|c| c.installs == Some(9)));
    }

    /// Flow: one fetch never resolves; its cache entry is stale, and other
    /// keys are in the same batch. Time is paused, so the 8 s deadline passes
    /// at once.
    /// Expectation: the batch returns; the hung key shows its stale count and
    /// the other keys their fetched counts.
    /// A failure here means one hung connection stalls the whole lookup or
    /// blanks a count the user already saw.
    #[tokio::test(start_paused = true)]
    async fn a_hung_fetch_times_out_to_the_stale_count_and_the_rest_still_return() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install-counts.json");
        lookup_keys(
            &FakeApi::answering(Some(77)),
            &path,
            vec![key("a/b", "slow")],
            1_000,
        )
        .await;
        let keys = vec![key("a/b", "slow"), key("a/b", "fast"), key("a/b", "other")];

        let result = lookup_keys(
            &Arc::new(HangingApi),
            &path,
            keys,
            1_000 + CACHE_TTL_SECS + 1,
        )
        .await;

        let counts: Vec<_> = result.iter().map(|c| c.installs).collect();
        assert_eq!(counts, [Some(77), Some(5), Some(5)]);
    }

    /// Flow: a hung fetch with nothing cached.
    /// Expectation: `None` for that key, no error.
    #[tokio::test(start_paused = true)]
    async fn a_hung_fetch_with_no_cache_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install-counts.json");

        let result = lookup_keys(
            &Arc::new(HangingApi),
            &path,
            vec![key("a/b", "slow")],
            1_000,
        )
        .await;

        assert_eq!(result[0].installs, None);
    }

    /// Flow: GitHub says the repo is private.
    /// Expectation: no skills.sh request, and no count.
    /// A failure here means a private repo's name and skill reach skills.sh.
    #[tokio::test]
    async fn a_private_repo_makes_no_skills_sh_call() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install-counts.json");
        let api = FakeApi::with_visibility(Some(5), Ok(Visibility::NotPublic));

        let result = lookup(&api, &path, 1_000).await;

        assert_eq!(result[0].installs, None);
        assert_eq!(api.calls.load(Ordering::SeqCst), 0);
        assert!(!path.exists());
    }

    /// Flow: GitHub answers 404, is rate limited, or is unreachable, and an
    /// older count is cached.
    /// Expectation: fail closed: no skills.sh request; the stale count shows.
    /// A failure here means an unknown visibility is treated as public.
    #[tokio::test]
    async fn an_unknown_visibility_makes_no_skills_sh_call() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install-counts.json");
        lookup(&FakeApi::answering(Some(77)), &path, 1_000).await;

        for visibility in [
            Ok(Visibility::NotPublic),
            Ok(Visibility::RateLimited(Duration::from_secs(60))),
            Err("unreachable".to_string()),
        ] {
            let api = FakeApi::with_visibility(Some(5), visibility);
            let result = lookup(&api, &path, 1_000 + 3 * CACHE_TTL_SECS).await;
            assert_eq!(result[0].installs, Some(77));
            assert_eq!(api.calls.load(Ordering::SeqCst), 0);
        }
    }

    /// Flow: a public repo, nothing cached.
    /// Expectation: one GitHub call and one skills.sh call.
    #[tokio::test]
    async fn a_public_repo_makes_one_call_to_each() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install-counts.json");
        let api = FakeApi::answering(Some(5));

        let result = lookup(&api, &path, 1_000).await;

        assert_eq!(result[0].installs, Some(5));
        assert_eq!(api.visibility_calls.load(Ordering::SeqCst), 1);
        assert_eq!(api.calls.load(Ordering::SeqCst), 1);
    }

    /// Flow: a repo was public at an earlier lookup and is private now; a new
    /// skill key from it has no cached count.
    /// Expectation: GitHub is asked again and skills.sh gets no request.
    /// A failure here means a remembered "public" verdict leaks a repo that
    /// has since gone private.
    #[tokio::test]
    async fn a_repo_that_turned_private_makes_no_skills_sh_call_for_a_new_skill() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install-counts.json");
        lookup(&FakeApi::answering(Some(5)), &path, 1_000).await;
        let api = FakeApi::with_visibility(Some(6), Ok(Visibility::NotPublic));

        let result = lookup_with(&api, &scheduler(), &path, "new-skill", 2_000).await;

        assert_eq!(result[0].installs, None);
        assert_eq!(api.visibility_calls.load(Ordering::SeqCst), 1);
        assert_eq!(api.calls.load(Ordering::SeqCst), 0);
    }

    /// Flow: a public source with a skill name that encodes a path traversal
    /// into another repo, or carries `%`, `/` or dots only.
    /// Expectation: no GitHub call and no skills.sh call for any repo, no count.
    /// A failure here means a crafted lock entry sends a private repo's
    /// coordinates to skills.sh without a visibility check.
    #[tokio::test]
    async fn an_unsafe_skill_name_makes_no_calls_at_all() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install-counts.json");
        let api = FakeApi::answering(Some(5));
        let keys = [
            "%2e%2e/%2e%2e/private-owner/private-repo/secret-skill",
            "../x",
            "..",
            ".",
            "a%2Fb",
            "a/b",
            "",
        ]
        .into_iter()
        .map(|name| key("obra/write-tests", name))
        .collect();

        let result = lookup_keys(&api, &path, keys, 1_000).await;

        assert!(result.iter().all(|c| c.installs.is_none()));
        assert_eq!(api.visibility_calls.load(Ordering::SeqCst), 0);
        assert_eq!(api.calls.load(Ordering::SeqCst), 0);
    }

    /// Flow: three skills from one repo in one batch.
    /// Expectation: GitHub is asked once for the repo, skills.sh three times.
    /// A failure here means the 60 requests an hour GitHub allows run out fast.
    #[tokio::test]
    async fn skills_of_one_repo_share_one_visibility_check_per_lookup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install-counts.json");
        let api = FakeApi::answering(Some(5));
        let keys = vec![key("a/b", "one"), key("a/b", "two"), key("a/b", "three")];

        lookup_keys(&api, &path, keys, 1_000).await;

        assert_eq!(api.calls.load(Ordering::SeqCst), 3);
        assert_eq!(api.visibility_calls.load(Ordering::SeqCst), 1);
    }

    /// Flow: two single-key lookups of the same repo run at the same time
    /// (two skill pages opened together), on the one shared scheduler.
    /// Expectation: GitHub is asked once for the repo.
    /// A failure here means every page open spends GitHub's 60 requests an hour.
    #[tokio::test]
    async fn concurrent_lookups_of_one_repo_make_one_github_call() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install-counts.json");
        let api = FakeApi::answering(Some(5));
        let shared = scheduler();

        let (one, two) = tokio::join!(
            lookup_with(&api, &shared, &path, "one", 1_000),
            lookup_with(&api, &shared, &path, "two", 1_000),
        );

        assert_eq!(one[0].installs, Some(5));
        assert_eq!(two[0].installs, Some(5));
        assert_eq!(api.visibility_calls.load(Ordering::SeqCst), 1);
    }

    /// Flow: the same skill is looked up at the same time by two callers.
    /// Expectation: skills.sh gets one request and both callers get the count.
    #[tokio::test]
    async fn concurrent_lookups_of_one_skill_share_one_request() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install-counts.json");
        let api = FakeApi::answering(Some(5));
        let shared = scheduler();

        let (one, two) = tokio::join!(
            lookup_with(&api, &shared, &path, "same", 1_000),
            lookup_with(&api, &shared, &path, "same", 1_000),
        );

        assert_eq!(one[0].installs, Some(5));
        assert_eq!(two[0].installs, Some(5));
        assert_eq!(api.calls.load(Ordering::SeqCst), 1);
    }

    /// Flow: a lookup fails (skills.sh offline), then the same skill is asked
    /// for again, first within the hour and then after it.
    /// Expectation: no new GitHub or skills.sh call within the hour; one new
    /// attempt after it.
    /// A failure here means an offline machine retries every skill on every
    /// page open.
    #[tokio::test(start_paused = true)]
    async fn a_failed_lookup_is_not_retried_within_the_hour() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install-counts.json");
        let api = FakeApi::answering(None);
        let shared = scheduler();

        lookup_with(&api, &shared, &path, "s", 1_000).await;
        assert_eq!(api.calls.load(Ordering::SeqCst), 1);
        assert_eq!(api.visibility_calls.load(Ordering::SeqCst), 1);

        tokio::time::advance(Duration::from_secs(60 * 60 - 1)).await;
        lookup_with(&api, &shared, &path, "s", 1_000).await;
        assert_eq!(api.calls.load(Ordering::SeqCst), 1);
        assert_eq!(api.visibility_calls.load(Ordering::SeqCst), 1);

        tokio::time::advance(Duration::from_secs(2)).await;
        lookup_with(&api, &shared, &path, "s", 1_000).await;
        assert_eq!(api.calls.load(Ordering::SeqCst), 2);
    }

    /// Flow: a repo that is not public is looked up again within the hour.
    /// Expectation: GitHub is not asked again.
    #[tokio::test(start_paused = true)]
    async fn a_not_public_verdict_is_remembered_for_the_hour() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install-counts.json");
        let api = FakeApi::with_visibility(Some(5), Ok(Visibility::NotPublic));
        let shared = scheduler();

        lookup_with(&api, &shared, &path, "one", 1_000).await;
        lookup_with(&api, &shared, &path, "two", 1_000).await;

        assert_eq!(api.visibility_calls.load(Ordering::SeqCst), 1);
        assert_eq!(api.calls.load(Ordering::SeqCst), 0);
    }

    /// Flow: GitHub answers 429 with Retry-After 60 s; later lookups come
    /// before and after that deadline.
    /// Expectation: no GitHub call and no skills.sh call until the deadline;
    /// then GitHub is asked again.
    /// A failure here means a rate-limited client keeps hammering GitHub.
    #[tokio::test(start_paused = true)]
    async fn a_github_rate_limit_blocks_calls_until_its_deadline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install-counts.json");
        let api = FakeApi::with_visibility(
            Some(5),
            Ok(Visibility::RateLimited(Duration::from_secs(60))),
        );
        let shared = scheduler();

        let first = lookup_with(&api, &shared, &path, "one", 1_000).await;
        assert_eq!(first[0].installs, None);
        assert_eq!(api.visibility_calls.load(Ordering::SeqCst), 1);

        tokio::time::advance(Duration::from_secs(59)).await;
        lookup_with(&api, &shared, &path, "two", 1_000).await;
        assert_eq!(api.visibility_calls.load(Ordering::SeqCst), 1);
        assert_eq!(api.calls.load(Ordering::SeqCst), 0);

        tokio::time::advance(Duration::from_secs(2)).await;
        lookup_with(&api, &shared, &path, "three", 1_000).await;
        assert_eq!(api.visibility_calls.load(Ordering::SeqCst), 2);
    }

    /// Flow: skills.sh answers 429 with Retry-After 60 s.
    /// Expectation: no skills.sh request for any skill until the deadline.
    #[tokio::test(start_paused = true)]
    async fn a_skills_sh_rate_limit_blocks_requests_until_its_deadline() {
        struct LimitedApi(AtomicUsize);
        impl InstallsApi for LimitedApi {
            async fn is_public_repo(&self, _source: &str) -> Result<Visibility, String> {
                Ok(Visibility::Public)
            }
            async fn installs(&self, _source: &str, _name: &str) -> Result<u32, InstallsFailure> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Err(InstallsFailure::RateLimited(Duration::from_secs(60)))
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install-counts.json");
        let api = Arc::new(LimitedApi(AtomicUsize::new(0)));
        let shared = scheduler();
        let ask = |name: &'static str| {
            lookup_install_counts(
                Arc::clone(&api),
                Arc::clone(&shared),
                &path,
                vec![key("a/b", name)],
                1_000,
            )
        };

        ask("one").await;
        tokio::time::advance(Duration::from_secs(59)).await;
        ask("two").await;
        assert_eq!(api.0.load(Ordering::SeqCst), 1);

        tokio::time::advance(Duration::from_secs(2)).await;
        ask("three").await;
        assert_eq!(api.0.load(Ordering::SeqCst), 2);
    }
}
