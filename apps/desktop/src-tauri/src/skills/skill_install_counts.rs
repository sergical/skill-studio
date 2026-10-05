// ============================================================================
// Skills Module - Install Counts
// skills.sh install counts for installed skills-sh skills, cached on disk for
// 24 h and fetched in the background at a gentle pace. The cache is keyed by
// source + name and holds one number per skill, so any view that wants a
// skill's popularity (the detail header today) reads it from here.
// ============================================================================

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::api::{self, SkillsShAccess};
use super::skill_dto::{InstallCount, InstallCountKey};

const CACHE_TTL_SECS: u64 = 24 * 60 * 60;

/// A real machine has hundreds of skills; three workers, each pausing between
/// requests, keep the proxy's load flat.
const WORKERS: usize = 3;
pub const REQUEST_SPACING: Duration = Duration::from_millis(250);

/// A hung connection must not hold a lookup open; a timeout is a failed
/// fetch like any other and takes the stale-cache fallback.
const FETCH_TIMEOUT: Duration = Duration::from_secs(8);

/// Only a plain `owner/repo` goes to skills.sh: two non-empty segments, no
/// scheme, no leading slash. A git URL or local path would leak a private
/// source, so those are never looked up.
fn is_owner_repo(source: &str) -> bool {
    let mut parts = source.split('/');
    let valid = |s: Option<&str>| {
        s.is_some_and(|s| {
            !s.is_empty() && !s.contains(':') && s != ".." && !s.chars().any(char::is_whitespace)
        })
    };
    valid(parts.next()) && valid(parts.next()) && parts.next().is_none()
}

/// Fetches one skill's install count, after checking its repo is public.
/// `impl Future + Send` so the Tauri command's future stays `Send`.
pub trait InstallsApi: Send + Sync + 'static {
    /// Whether `source` (`owner/repo`) is a public GitHub repo, asked with no
    /// credentials. Anything but a confirmed yes is an `Err` or `false`: a
    /// private repo's name and skill must never reach skills.sh.
    fn is_public_repo(
        &self,
        source: &str,
    ) -> impl std::future::Future<Output = Result<bool, String>> + Send;

    fn installs(
        &self,
        source: &str,
        name: &str,
    ) -> impl std::future::Future<Output = Result<u32, String>> + Send;
}

/// The real `InstallsApi`: skills.sh's details endpoint
/// (`GET /skills/{owner/repo}/{slug}`), which carries `installs`.
pub struct SkillsShInstallsApi {
    pub access: SkillsShAccess,
}

impl InstallsApi for SkillsShInstallsApi {
    /// `GET https://api.github.com/repos/{owner}/{repo}` with no token (not
    /// even `gh`'s): only a 200 with `"private": false` counts as public, so a
    /// 404, rate limit, or parse failure all fail closed.
    async fn is_public_repo(&self, source: &str) -> Result<bool, String> {
        let response = reqwest::Client::new()
            .get(format!("https://api.github.com/repos/{source}"))
            .header(reqwest::header::USER_AGENT, "AgentStudio/0.1.0")
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if response.status() != reqwest::StatusCode::OK {
            return Ok(false);
        }
        let body: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
        Ok(body.get("private") == Some(&serde_json::Value::Bool(false)))
    }

    async fn installs(&self, source: &str, name: &str) -> Result<u32, String> {
        let details = api::get_skill_details(&self.access, &format!("{source}/{name}")).await?;
        Ok(details.installs)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    installs: u32,
    fetched_at: u64,
}

type Cache = HashMap<String, CacheEntry>;

/// The on-disk cache: counts per source + name, and when each repo was last
/// confirmed public. Only a "public" verdict is stored; a negative or unknown
/// one is never a reason to send, so it is simply asked again.
#[derive(Debug, Default, Serialize, Deserialize)]
struct CacheFile {
    #[serde(default)]
    counts: Cache,
    #[serde(default)]
    public_repos: HashMap<String, u64>,
}

/// Serializes the read-merge-write of the cache file across concurrent
/// lookups; held only for the file I/O, never across a request.
static CACHE_FILE_LOCK: Mutex<()> = Mutex::new(());

fn cache_key(key: &InstallCountKey) -> String {
    format!("{}/{}", key.source, key.name)
}

/// A missing or unreadable cache is an empty one: the counts are a courtesy,
/// so a damaged file just costs a refetch.
fn load_cache(path: &Path) -> CacheFile {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_fresh_entries(path: &Path, fresh: Cache, fresh_public: HashMap<String, u64>) {
    if fresh.is_empty() && fresh_public.is_empty() {
        return;
    }
    let _guard = CACHE_FILE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut cache = load_cache(path);
    cache.counts.extend(fresh);
    cache.public_repos.extend(fresh_public);
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

fn is_fresh_at(fetched_at: u64, now: u64) -> bool {
    // An entry stamped in the future (clock moved back) cannot be trusted.
    fetched_at <= now && now - fetched_at < CACHE_TTL_SECS
}

fn is_fresh(entry: &CacheEntry, now: u64) -> bool {
    is_fresh_at(entry.fetched_at, now)
}

/// Looks up each key's install count: a cache entry younger than 24 h is
/// used as-is; the rest are fetched by `WORKERS` workers, `spacing` apart.
/// Before a repo's first skills.sh request, GitHub must confirm it is public
/// (cached 24 h); otherwise nothing is sent. A failed fetch (offline, unknown
/// skill, private repo) falls back to a stale cached count,
/// else `None` - never an error. Results keep the order of `keys`.
pub async fn lookup_install_counts<A: InstallsApi>(
    api: Arc<A>,
    cache_path: &Path,
    keys: Vec<InstallCountKey>,
    now: u64,
    spacing: Duration,
) -> Vec<InstallCount> {
    let CacheFile {
        counts: cache,
        public_repos,
    } = load_cache(cache_path);
    let verdicts: HashMap<String, bool> = public_repos
        .iter()
        .filter(|(_, at)| is_fresh_at(**at, now))
        .map(|(repo, _)| (repo.clone(), true))
        .collect();
    let cached_public: Vec<String> = verdicts.keys().cloned().collect();
    let verdicts = Arc::new(Mutex::new(verdicts));
    let mut counts: HashMap<String, Option<u32>> = HashMap::new();
    let mut queue: VecDeque<InstallCountKey> = VecDeque::new();
    for key in &keys {
        let id = cache_key(key);
        if counts.contains_key(&id) {
            continue;
        }
        if !is_owner_repo(&key.source) {
            counts.insert(id, None);
            continue;
        }
        match cache.get(&id) {
            Some(entry) if is_fresh(entry, now) => {
                counts.insert(id, Some(entry.installs));
            }
            _ => {
                counts.insert(id, None);
                queue.push_back(key.clone());
            }
        }
    }

    let queue = Arc::new(Mutex::new(queue));
    let mut workers = tokio::task::JoinSet::new();
    for _ in 0..WORKERS {
        let api = Arc::clone(&api);
        let queue = Arc::clone(&queue);
        let verdicts = Arc::clone(&verdicts);
        workers.spawn(async move {
            let mut fetched: Vec<(InstallCountKey, Result<u32, String>)> = Vec::new();
            loop {
                let next = queue
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .pop_front();
                let Some(key) = next else { break };
                // Pause between fetches, not after the last one.
                if !fetched.is_empty() {
                    tokio::time::sleep(spacing).await;
                }
                let known = verdicts
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .get(&key.source)
                    .copied();
                let public = if let Some(public) = known {
                    public
                } else {
                    let public =
                        tokio::time::timeout(FETCH_TIMEOUT, api.is_public_repo(&key.source))
                            .await
                            .is_ok_and(|checked| checked == Ok(true));
                    verdicts
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .insert(key.source.clone(), public);
                    public
                };
                let result = if public {
                    tokio::time::timeout(FETCH_TIMEOUT, api.installs(&key.source, &key.name))
                        .await
                        .unwrap_or_else(|_| Err("timed out".to_string()))
                } else {
                    Err("repo is not confirmed public".to_string())
                };
                fetched.push((key, result));
            }
            fetched
        });
    }

    let mut fresh = Cache::new();
    while let Some(joined) = workers.join_next().await {
        for (key, result) in joined.unwrap_or_default() {
            let id = cache_key(&key);
            match result {
                Ok(installs) => {
                    fresh.insert(
                        id.clone(),
                        CacheEntry {
                            installs,
                            fetched_at: now,
                        },
                    );
                    counts.insert(id, Some(installs));
                }
                Err(_) => {
                    counts.insert(id.clone(), cache.get(&id).map(|e| e.installs));
                }
            }
        }
    }
    let fresh_public = verdicts
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .filter(|(repo, public)| **public && !cached_public.contains(repo))
        .map(|(repo, _)| (repo.clone(), now))
        .collect();
    save_fresh_entries(cache_path, fresh, fresh_public);

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
        visibility: Result<bool, String>,
        visibility_calls: AtomicUsize,
    }

    impl FakeApi {
        fn answering(installs: Option<u32>) -> Arc<Self> {
            Self::with_visibility(installs, Ok(true))
        }

        fn with_visibility(installs: Option<u32>, visibility: Result<bool, String>) -> Arc<Self> {
            Arc::new(Self {
                installs,
                calls: AtomicUsize::new(0),
                visibility,
                visibility_calls: AtomicUsize::new(0),
            })
        }
    }

    impl InstallsApi for FakeApi {
        async fn is_public_repo(&self, _source: &str) -> Result<bool, String> {
            self.visibility_calls.fetch_add(1, Ordering::SeqCst);
            self.visibility.clone()
        }

        async fn installs(&self, _source: &str, _name: &str) -> Result<u32, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.installs.ok_or_else(|| "offline".to_string())
        }
    }

    fn key(source: &str, name: &str) -> InstallCountKey {
        InstallCountKey {
            source: source.to_string(),
            name: name.to_string(),
        }
    }

    async fn lookup(api: &Arc<FakeApi>, path: &Path, now: u64) -> Vec<InstallCount> {
        lookup_install_counts(
            Arc::clone(api),
            path,
            vec![key("obra/write-tests", "write-tests")],
            now,
            Duration::ZERO,
        )
        .await
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
    /// Expectation: `installs` is `None`, with no panic and no cached count
    /// (the repo's public verdict may be kept).
    /// A failure here means offline use surfaces an error or caches a bogus 0.
    #[tokio::test]
    async fn offline_with_no_cache_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install-counts.json");

        let result = lookup(&FakeApi::answering(None), &path, 1_000).await;

        assert_eq!(result[0].installs, None);
        assert!(load_cache(&path).counts.is_empty());
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

        let result =
            lookup_install_counts(Arc::clone(&api), &path, keys, 1_000, Duration::ZERO).await;

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

        let result =
            lookup_install_counts(Arc::clone(&api), &path, keys, 1_000, Duration::ZERO).await;

        assert!(result.iter().all(|c| c.installs.is_none()));
        assert_eq!(api.calls.load(Ordering::SeqCst), 0);
    }

    struct HangingApi;

    impl InstallsApi for HangingApi {
        async fn is_public_repo(&self, _source: &str) -> Result<bool, String> {
            Ok(true)
        }

        async fn installs(&self, _source: &str, name: &str) -> Result<u32, String> {
            if name == "slow" {
                std::future::pending::<()>().await;
            }
            Ok(5)
        }
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
        lookup_install_counts(
            FakeApi::answering(Some(77)),
            &path,
            vec![key("a/b", "slow")],
            1_000,
            Duration::ZERO,
        )
        .await;
        let keys = vec![key("a/b", "slow"), key("a/b", "fast"), key("a/b", "other")];

        let result = lookup_install_counts(
            Arc::new(HangingApi),
            &path,
            keys,
            1_000 + CACHE_TTL_SECS + 1,
            Duration::ZERO,
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

        let result = lookup_install_counts(
            Arc::new(HangingApi),
            &path,
            vec![key("a/b", "slow")],
            1_000,
            Duration::ZERO,
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
        let api = FakeApi::with_visibility(Some(5), Ok(false));

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

        for visibility in [Ok(false), Err("rate limited".to_string())] {
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

    /// Flow: a repo confirmed public less than 24 h ago has a stale count.
    /// Expectation: the count is refetched without asking GitHub again; after
    /// 24 h GitHub is asked again.
    /// A failure here means every refresh spends GitHub's 60 requests an hour.
    #[tokio::test]
    async fn a_cached_public_verdict_skips_the_github_call_until_it_expires() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("install-counts.json");
        lookup(&FakeApi::answering(Some(5)), &path, 1_000).await;
        let mut file = load_cache(&path);
        file.counts.clear();
        std::fs::write(&path, serde_json::to_string(&file).unwrap()).unwrap();

        let within = FakeApi::answering(Some(6));
        lookup(&within, &path, 2_000).await;
        assert_eq!(within.visibility_calls.load(Ordering::SeqCst), 0);
        assert_eq!(within.calls.load(Ordering::SeqCst), 1);

        let mut file = load_cache(&path);
        file.counts.clear();
        std::fs::write(&path, serde_json::to_string(&file).unwrap()).unwrap();
        let expired = FakeApi::answering(Some(7));
        lookup(&expired, &path, 1_000 + CACHE_TTL_SECS + 1).await;
        assert_eq!(expired.visibility_calls.load(Ordering::SeqCst), 1);
    }
}
