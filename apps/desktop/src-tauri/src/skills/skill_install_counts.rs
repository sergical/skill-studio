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

/// Fetches one skill's install count. `impl Future + Send` so the Tauri
/// command's future stays `Send`.
pub trait InstallsApi: Send + Sync + 'static {
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
    now.saturating_sub(entry.fetched_at) < CACHE_TTL_SECS
}

/// Looks up each key's install count: a cache entry younger than 24 h is
/// used as-is; the rest are fetched by `WORKERS` workers, `spacing` apart.
/// A failed fetch (offline, unknown skill) falls back to a stale cached count,
/// else `None` - never an error. Results keep the order of `keys`.
pub async fn lookup_install_counts<A: InstallsApi>(
    api: Arc<A>,
    cache_path: &Path,
    keys: Vec<InstallCountKey>,
    now: u64,
    spacing: Duration,
) -> Vec<InstallCount> {
    let cache = load_cache(cache_path);
    let mut counts: HashMap<String, Option<u32>> = HashMap::new();
    let mut queue: VecDeque<InstallCountKey> = VecDeque::new();
    for key in &keys {
        let id = cache_key(key);
        if counts.contains_key(&id) {
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
        workers.spawn(async move {
            let mut fetched: Vec<(InstallCountKey, Result<u32, String>)> = Vec::new();
            loop {
                let next = queue
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .pop_front();
                let Some(key) = next else { break };
                let result = api.installs(&key.source, &key.name).await;
                fetched.push((key, result));
                tokio::time::sleep(spacing).await;
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
    }

    impl FakeApi {
        fn answering(installs: Option<u32>) -> Arc<Self> {
            Arc::new(Self {
                installs,
                calls: AtomicUsize::new(0),
            })
        }
    }

    impl InstallsApi for FakeApi {
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

        let result =
            lookup_install_counts(Arc::clone(&api), &path, keys, 1_000, Duration::ZERO).await;

        let names: Vec<_> = result.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["one", "two", "one"]);
        assert_eq!(api.calls.load(Ordering::SeqCst), 2);
    }
}
