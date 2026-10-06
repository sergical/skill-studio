// ============================================================================
// Skills Module - skill_plugin_update
// Detects Claude Code plugin updates the way Claude Code itself decides them:
// the release version is the source's `.claude-plugin/plugin.json` `version`,
// else the marketplace entry's `version`, and only a plugin with neither is
// compared by git sha. The installs come from
// `~/.claude/plugins/installed_plugins.json`, the marketplace copy from
// `~/.claude/plugins/marketplaces/<marketplace>/`. A relative-path source
// reads its manifest locally; a github source's manifest, and the catalog of a
// GitHub-hosted marketplace (Claude Code does not refresh third-party
// checkouts), are fetched by the background update check into
// `plugin-versions.json`, which the snapshot overlay only reads. A missing, unreadable, or malformed file means "no
// claim", never an error.
// ============================================================================

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::gh_cli;
use super::skill_process::AddOperationControl;

/// Owner id a plugin update carries in `InstalledSkill::update_owner_ids`.
pub fn plugin_owner_id(plugin_id: &str) -> String {
    format!("plugin:{plugin_id}")
}

/// One entry of `installed_plugins.json`'s per-id install list.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PluginInstall {
    pub scope: Option<String>,
    pub project_path: Option<String>,
    pub version: Option<String>,
    pub git_commit_sha: Option<String>,
}

/// A marketplace entry's `source` that lives in a git repo.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RemoteSource {
    pub url: String,
    /// Folder of the plugin inside the repo (`git-subdir`).
    pub path: Option<String>,
    pub sha: Option<String>,
    /// Branch or tag the marketplace follows when it pins no `sha`.
    pub git_ref: Option<String>,
}

/// Where a marketplace entry's plugin files live.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum PluginSource {
    /// A folder inside the marketplace checkout (`"./plugins/codex"`).
    Relative(String),
    Remote(RemoteSource),
    #[default]
    Unsupported,
}

/// What a marketplace entry says about the latest release of a plugin.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MarketplaceRelease {
    pub version: Option<String>,
    pub source: PluginSource,
}

/// The `version` of the source's `.claude-plugin/plugin.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestVersion {
    Version(String),
    /// The manifest is absent or has no `version`.
    NoVersion,
    /// Not readable (yet): no claim can be made.
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginUpdateVerdict {
    Available,
    Current,
    /// No version to compare and no sha pair: say nothing.
    NoClaim,
}

/// One install of a plugin that has an update, for the snapshot overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginInstallUpdate {
    pub scope: String,
    pub project_path: Option<String>,
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.filter(|v| !v.is_empty())
}

/// Pure decision for one install. Version precedence follows Claude Code:
/// manifest `version`, then marketplace `version`, then the sha.
pub fn plugin_update_verdict(
    install: &PluginInstall,
    manifest: &ManifestVersion,
    release: &MarketplaceRelease,
) -> PluginUpdateVerdict {
    let latest = match manifest {
        ManifestVersion::Unknown => return PluginUpdateVerdict::NoClaim,
        ManifestVersion::Version(version) => Some(version.as_str()),
        ManifestVersion::NoVersion => non_empty(release.version.as_deref()),
    };
    if let Some(latest) = latest {
        let Some(installed) = non_empty(install.version.as_deref()).filter(|v| *v != "unknown")
        else {
            return PluginUpdateVerdict::NoClaim;
        };
        return if latest == installed {
            PluginUpdateVerdict::Current
        } else {
            PluginUpdateVerdict::Available
        };
    }
    if let (PluginSource::Remote(remote), Some(installed)) = (
        &release.source,
        non_empty(install.git_commit_sha.as_deref()),
    ) {
        if let Some(latest) = non_empty(remote.sha.as_deref()) {
            // Claude Code records full shas, marketplaces sometimes abbreviate.
            let same = latest.starts_with(installed) || installed.starts_with(latest);
            return if same {
                PluginUpdateVerdict::Current
            } else {
                PluginUpdateVerdict::Available
            };
        }
    }
    PluginUpdateVerdict::NoClaim
}

fn parse_source(source: &Value) -> PluginSource {
    if let Some(relative) = source.as_str() {
        return PluginSource::Relative(relative.to_string());
    }
    let field = |name: &str| source.get(name).and_then(Value::as_str).map(str::to_string);
    let url = match source.get("source").and_then(Value::as_str) {
        Some("url" | "git-subdir") => field("url"),
        Some("github") => field("repo").map(|repo| format!("https://github.com/{repo}")),
        _ => None,
    };
    match url {
        Some(url) => PluginSource::Remote(RemoteSource {
            url,
            path: field("path"),
            sha: field("sha"),
            git_ref: field("ref"),
        }),
        None => PluginSource::Unsupported,
    }
}

/// Parses the `plugins[]` entry named `plugin` out of a `marketplace.json`
/// body. `None` when the body is not JSON or has no such entry.
pub fn marketplace_release(marketplace_json: &str, plugin: &str) -> Option<MarketplaceRelease> {
    let root: Value = serde_json::from_str(marketplace_json).ok()?;
    let entry = root
        .get("plugins")?
        .as_array()?
        .iter()
        .find(|entry| entry.get("name").and_then(Value::as_str) == Some(plugin))?;
    let plugin_root = root
        .get("metadata")
        .and_then(|metadata| metadata.get("pluginRoot"))
        .and_then(Value::as_str)
        .filter(|root| !root.is_empty());
    let mut source = entry.get("source").map(parse_source).unwrap_or_default();
    // Claude Code prepends `metadata.pluginRoot` to a bare-name relative source.
    if let (PluginSource::Relative(relative), Some(plugin_root)) = (&mut source, plugin_root) {
        if !relative.starts_with("./") {
            *relative = format!("{}/{relative}", plugin_root.trim_end_matches('/'));
        }
    }
    Some(MarketplaceRelease {
        version: entry
            .get("version")
            .and_then(Value::as_str)
            .map(str::to_string),
        source,
    })
}

/// Parses `installed_plugins.json` into installs per plugin id, dropping an
/// id whose install list does not parse.
pub fn parse_installs(installed_json: &str) -> BTreeMap<String, Vec<PluginInstall>> {
    let Ok(root) = serde_json::from_str::<Value>(installed_json) else {
        return BTreeMap::new();
    };
    let Some(plugins) = root.get("plugins").and_then(Value::as_object) else {
        return BTreeMap::new();
    };
    plugins
        .iter()
        .filter_map(|(id, installs)| {
            let installs = serde_json::from_value::<Vec<PluginInstall>>(installs.clone()).ok()?;
            Some((id.clone(), installs))
        })
        .collect()
}

fn manifest_version_from_json(body: &str) -> ManifestVersion {
    let Ok(manifest) = serde_json::from_str::<Value>(body) else {
        return ManifestVersion::Unknown;
    };
    match manifest.get("version").and_then(Value::as_str) {
        Some(version) if !version.is_empty() => ManifestVersion::Version(version.to_string()),
        _ => ManifestVersion::NoVersion,
    }
}

/// Reads `<marketplace_dir>/<relative>/.claude-plugin/plugin.json`. A path
/// that leaves the marketplace folder, lexically or through a symlink, is
/// `Unknown`, as is a manifest that is not JSON.
pub fn local_manifest_version(marketplace_dir: &Path, relative: &str) -> ManifestVersion {
    let relative = Path::new(relative);
    if !relative
        .components()
        .all(|part| matches!(part, Component::Normal(_) | Component::CurDir))
    {
        return ManifestVersion::Unknown;
    }
    let manifest = marketplace_dir
        .join(relative)
        .join(".claude-plugin")
        .join("plugin.json");
    let Ok(root) = marketplace_dir.canonicalize() else {
        return ManifestVersion::Unknown;
    };
    let resolved = match manifest.canonicalize() {
        Ok(resolved) => resolved,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return ManifestVersion::NoVersion
        }
        Err(_) => return ManifestVersion::Unknown,
    };
    if !resolved.starts_with(&root) {
        return ManifestVersion::Unknown;
    }
    match std::fs::read_to_string(resolved) {
        Ok(body) => manifest_version_from_json(&body),
        Err(_) => ManifestVersion::Unknown,
    }
}

// ----------------------------------------------------------------------------
// Remote manifest versions: fetched by the background update check, read by
// the snapshot overlay.
// ----------------------------------------------------------------------------

/// One cached lookup. `version: None` records "the manifest has no version",
/// so the sha fallback applies without asking GitHub again.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedPluginVersion {
    pub version: Option<String>,
}

/// A marketplace's `marketplace.json` as fetched from its GitHub repo, with
/// the commit it was read at. Claude Code does not refresh a third-party
/// marketplace checkout by default, so this is the newer view of the catalog.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedCatalog {
    pub url: String,
    pub commit: String,
    pub body: String,
}

/// `<app data>/skill-studio/plugin-versions.json`. `catalogs` maps a
/// marketplace name to its fetched catalog. `entries` is keyed
/// `"{url}#{path}@{sha}"`: a sha never changes content, so an entry never
/// goes stale. `resolved` maps a source with no pinned sha, keyed
/// `"{url}#{path}@{ref|HEAD}"`, to the commit that ref pointed at during the
/// last check.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginVersionCache {
    pub entries: BTreeMap<String, CachedPluginVersion>,
    pub resolved: BTreeMap<String, String>,
    pub catalogs: BTreeMap<String, CachedCatalog>,
}

pub fn plugin_versions_path(app_data: &Path) -> PathBuf {
    app_data.join("skill-studio").join("plugin-versions.json")
}

/// The cache file that sits next to `update_check_path`.
pub fn read_plugin_versions_beside(update_check_path: &Path) -> PluginVersionCache {
    update_check_path
        .parent()
        .map(|dir| read_plugin_versions(&dir.join("plugin-versions.json")))
        .unwrap_or_default()
}

pub fn read_plugin_versions(path: &Path) -> PluginVersionCache {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|body| serde_json::from_str(&body).ok())
        .unwrap_or_default()
}

fn write_plugin_versions(path: &Path, cache: &PluginVersionCache) -> Result<(), String> {
    let dir = path.parent().ok_or("plugin-versions.json has no folder")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let body = serde_json::to_string_pretty(cache).map_err(|e| e.to_string())?;
    let mut temp = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
    std::io::Write::write_all(&mut temp, body.as_bytes()).map_err(|e| e.to_string())?;
    temp.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Cache key of a remote source. `None` without a sha: nothing pins the
/// release, so there is nothing to look up.
pub fn remote_cache_key(remote: &RemoteSource) -> Option<String> {
    let sha = non_empty(remote.sha.as_deref())?;
    Some(format!(
        "{}#{}@{sha}",
        remote.url,
        remote.path.as_deref().unwrap_or("")
    ))
}

/// Cache key of a source that pins no sha, by the ref it follows.
pub fn unpinned_key(remote: &RemoteSource) -> String {
    format!(
        "{}#{}@{}",
        remote.url,
        remote.path.as_deref().unwrap_or(""),
        non_empty(remote.git_ref.as_deref()).unwrap_or("HEAD")
    )
}

/// `remote` with the sha the last check resolved for it, when it pins none.
fn pinned_remote(remote: &RemoteSource, versions: &PluginVersionCache) -> Option<RemoteSource> {
    if non_empty(remote.sha.as_deref()).is_some() {
        return Some(remote.clone());
    }
    let sha = versions.resolved.get(&unpinned_key(remote))?;
    Some(RemoteSource {
        sha: Some(sha.clone()),
        ..remote.clone()
    })
}

/// A branch or tag name safe to put in a `gh api` path.
fn is_safe_ref(git_ref: &str) -> bool {
    !git_ref.is_empty()
        && !git_ref.contains("..")
        && !git_ref.starts_with(['/', '-'])
        && !git_ref.ends_with('/')
        && git_ref
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/'))
}

/// True when a source with no sha can be resolved and fetched through `gh`.
fn is_resolvable(remote: &RemoteSource) -> bool {
    let probe = RemoteSource {
        sha: Some("0".to_string()),
        ..remote.clone()
    };
    gh_manifest_api_path(&probe).is_some()
        && non_empty(remote.git_ref.as_deref()).is_none_or(is_safe_ref)
}

fn is_name_part(part: &str) -> bool {
    !part.is_empty()
        && part != "."
        && part != ".."
        && part
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// `owner/repo` of a `https://github.com/<owner>/<repo>[.git]` URL, with both
/// halves limited to URL-safe name characters.
fn github_repo(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("https://github.com/")?;
    let rest = rest.trim_end_matches('/');
    let rest = rest.strip_suffix(".git").unwrap_or(rest);
    let (owner, repo) = rest.split_once('/')?;
    (is_name_part(owner) && is_name_part(repo)).then(|| (owner.to_string(), repo.to_string()))
}

/// The `gh api` path of a remote source's manifest at its pinned sha. `None`
/// for a non-github URL, an unsafe name or path (`..`, odd characters), or a
/// missing sha.
pub fn gh_manifest_api_path(remote: &RemoteSource) -> Option<String> {
    let (owner, repo) = github_repo(&remote.url)?;
    let sha = non_empty(remote.sha.as_deref())?;
    if !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut folder = String::new();
    let path = remote.path.as_deref().unwrap_or("");
    let path = path.strip_prefix("./").unwrap_or(path).trim_matches('/');
    if !path.is_empty() {
        if !path.split('/').all(is_name_part) {
            return None;
        }
        folder = format!("{path}/");
    }
    Some(format!(
        "repos/{owner}/{repo}/contents/{folder}.claude-plugin/plugin.json?ref={sha}"
    ))
}

/// Decodes the standard-alphabet base64 GitHub returns, newlines included.
fn decode_base64(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for byte in input.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b'\n' | b'\r' | b' ' => continue,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// What one GitHub contents lookup came back with.
pub enum ManifestFetch {
    /// The `.content` field, base64.
    Content(String),
    /// GitHub has no manifest at that sha.
    Missing,
    /// Network, auth, or rate-limit trouble: record nothing.
    Failed,
}

/// What one commit lookup came back with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitLookup {
    Sha(String),
    /// GitHub says the repo or ref does not exist (HTTP 404 or 422).
    Gone,
    /// Network, auth, or rate-limit trouble: keep what the last check knew.
    Failed,
}

/// The GitHub lookups a refresh makes.
pub trait PluginRemote {
    /// The file at `api_path` (see [`gh_manifest_api_path`]); also used for a
    /// marketplace's `marketplace.json`.
    fn manifest(&self, api_path: &str) -> ManifestFetch;
    /// The commit sha `git_ref` points at in `owner/repo`.
    fn commit_sha(&self, owner: &str, repo: &str, git_ref: &str) -> CommitLookup;
}

/// The `gh` binary plus one shared deadline for every call of a refresh.
struct GhRemote<'a>(&'a Path, AddOperationControl);

impl PluginRemote for GhRemote<'_> {
    fn manifest(&self, api_path: &str) -> ManifestFetch {
        match gh_cli::run_gh_controlled(self.0, &["api", api_path, "--jq", ".content"], &self.1) {
            Ok(stdout) => {
                ManifestFetch::Content(String::from_utf8_lossy(&stdout).trim().to_string())
            }
            Err(gh_cli::GhError::Failed(message)) if message.contains("HTTP 404") => {
                ManifestFetch::Missing
            }
            Err(_) => ManifestFetch::Failed,
        }
    }

    fn commit_sha(&self, owner: &str, repo: &str, git_ref: &str) -> CommitLookup {
        let api_path = format!("repos/{owner}/{repo}/commits/{git_ref}");
        match gh_cli::run_gh_controlled(self.0, &["api", &api_path, "--jq", ".sha"], &self.1) {
            Ok(stdout) => {
                let sha = String::from_utf8_lossy(&stdout).trim().to_string();
                if sha.len() >= 7 && sha.bytes().all(|b| b.is_ascii_hexdigit()) {
                    CommitLookup::Sha(sha)
                } else {
                    CommitLookup::Failed
                }
            }
            Err(gh_cli::GhError::Failed(message))
                if message.contains("HTTP 404") || message.contains("HTTP 422") =>
            {
                CommitLookup::Gone
            }
            Err(_) => CommitLookup::Failed,
        }
    }
}

/// One marketplace in `known_marketplaces.json` that lives on GitHub.
struct GithubMarketplace {
    owner: String,
    repo: String,
    git_ref: Option<String>,
}

fn is_safe_marketplace_name(name: &str) -> bool {
    !name.contains(['/', '\\']) && name != ".."
}

/// The GitHub-hosted marketplaces Claude Code knows, by name. A source that is
/// not GitHub, or has an unsafe name or ref, is left out and keeps its local
/// checkout.
fn github_marketplaces(plugins_dir: &Path) -> BTreeMap<String, GithubMarketplace> {
    let Ok(body) = std::fs::read_to_string(plugins_dir.join("known_marketplaces.json")) else {
        return BTreeMap::new();
    };
    let Ok(Value::Object(known)) = serde_json::from_str::<Value>(&body) else {
        return BTreeMap::new();
    };
    known
        .iter()
        .filter(|(name, _)| is_safe_marketplace_name(name))
        .filter_map(|(name, entry)| {
            let source = entry.get("source")?;
            let field = |key: &str| source.get(key).and_then(Value::as_str);
            let (owner, repo) = match field("source")? {
                "github" => {
                    let (owner, repo) = field("repo")?.split_once('/')?;
                    (is_name_part(owner) && is_name_part(repo))
                        .then(|| (owner.to_string(), repo.to_string()))?
                }
                "git" | "url" => github_repo(field("url")?)?,
                _ => return None,
            };
            let git_ref = non_empty(field("ref")).map(str::to_string);
            if git_ref
                .as_deref()
                .is_some_and(|git_ref| !is_safe_ref(git_ref))
            {
                return None;
            }
            Some((
                name.clone(),
                GithubMarketplace {
                    owner,
                    repo,
                    git_ref,
                },
            ))
        })
        .collect()
}

/// The remote source of a relative-path plugin of a fetched catalog: its
/// folder in the marketplace repo, at the commit the catalog was read from.
fn catalog_plugin_remote(catalog: &CachedCatalog, relative: &str) -> RemoteSource {
    RemoteSource {
        url: catalog.url.clone(),
        path: Some(relative.to_string()),
        sha: Some(catalog.commit.clone()),
        git_ref: None,
    }
}

/// Fetches the catalog of every installed plugin's GitHub marketplace at the
/// commit its ref points at. A catalog that cannot be fetched keeps the
/// previous one when the failure may be transient, else falls back to the
/// local checkout (no entry).
fn refresh_catalogs(
    plugins_dir: &Path,
    installed_ids: &[String],
    previous: &PluginVersionCache,
    remote_api: &dyn PluginRemote,
    next: &mut PluginVersionCache,
) {
    for (name, marketplace) in github_marketplaces(plugins_dir) {
        let installed_here = installed_ids
            .iter()
            .any(|id| id.split_once('@').is_some_and(|(_, m)| m == name));
        if !installed_here {
            continue;
        }
        let keep_previous = |next: &mut PluginVersionCache| {
            if let Some(catalog) = previous.catalogs.get(&name) {
                next.catalogs.insert(name.clone(), catalog.clone());
            }
        };
        let git_ref = marketplace.git_ref.as_deref().unwrap_or("HEAD");
        let commit = match remote_api.commit_sha(&marketplace.owner, &marketplace.repo, git_ref) {
            CommitLookup::Sha(commit) => commit,
            CommitLookup::Gone => continue,
            CommitLookup::Failed => {
                keep_previous(next);
                continue;
            }
        };
        let url = format!(
            "https://github.com/{}/{}",
            marketplace.owner, marketplace.repo
        );
        if let Some(cached) = previous
            .catalogs
            .get(&name)
            .filter(|cached| cached.commit == commit && cached.url == url)
        {
            next.catalogs.insert(name, cached.clone());
            continue;
        }
        let api_path = format!(
            "repos/{}/{}/contents/.claude-plugin/marketplace.json?ref={commit}",
            marketplace.owner, marketplace.repo
        );
        match remote_api.manifest(&api_path) {
            ManifestFetch::Content(content) => {
                let body = decode_base64(&content)
                    .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                    .filter(|body| serde_json::from_str::<Value>(body).is_ok());
                if let Some(body) = body {
                    next.catalogs
                        .insert(name, CachedCatalog { url, commit, body });
                } else {
                    // GitHub's contents API returns no content for a file over 1 MB.
                    eprintln!(
                        "plugin versions: catalog of {name} has no readable content; using the previous or local catalog"
                    );
                    keep_previous(next);
                }
            }
            ManifestFetch::Missing => {}
            ManifestFetch::Failed => keep_previous(next),
        }
    }
}

/// The marketplace body to read: the fetched catalog when there is one, else
/// the local checkout.
fn marketplace_body(
    plugins_dir: &Path,
    marketplace: &str,
    catalogs: &BTreeMap<String, CachedCatalog>,
) -> Option<String> {
    match catalogs.get(marketplace) {
        Some(catalog) => Some(catalog.body.clone()),
        None => read_marketplace(plugins_dir, marketplace),
    }
}

/// The remote sources of every installed plugin that can be looked up.
fn wanted_remote_sources(
    plugins_dir: &Path,
    installed_ids: &[String],
    catalogs: &BTreeMap<String, CachedCatalog>,
) -> Vec<RemoteSource> {
    let mut wanted: Vec<RemoteSource> = Vec::new();
    for id in installed_ids {
        let Some((plugin, marketplace)) = id.split_once('@') else {
            continue;
        };
        let Some(body) = marketplace_body(plugins_dir, marketplace, catalogs) else {
            continue;
        };
        let Some(release) = marketplace_release(&body, plugin) else {
            continue;
        };
        let remote = match (release.source, catalogs.get(marketplace)) {
            (PluginSource::Remote(remote), _) => remote,
            (PluginSource::Relative(relative), Some(catalog)) => {
                catalog_plugin_remote(catalog, &relative)
            }
            _ => continue,
        };
        let lookup_ok = if non_empty(remote.sha.as_deref()).is_some() {
            gh_manifest_api_path(&remote).is_some()
        } else {
            is_resolvable(&remote)
        };
        if lookup_ok && !wanted.contains(&remote) {
            wanted.push(remote);
        }
    }
    wanted
}

/// Looks up the manifest version of each installed plugin with a github
/// source (or a relative source in a fetched catalog) that
/// `plugin-versions.json` has no entry for yet, and writes the result. A
/// GitHub marketplace's catalog is fetched first, at the commit its ref points
/// at. A source with no pinned sha first has its ref resolved to a commit on
/// every check. A failed lookup records nothing, so the next check retries it,
/// and a transient failure to resolve a commit keeps what the last check knew;
/// a missing manifest counts as "no version" only when the commit itself is
/// readable, since GitHub also answers 404 for a private repo.
pub fn refresh_plugin_versions_with(home: &Path, app_data: &Path, remote_api: &dyn PluginRemote) {
    let plugins_dir = home.join(".claude").join("plugins");
    let installed_ids: Vec<String> =
        std::fs::read_to_string(plugins_dir.join("installed_plugins.json"))
            .map(|installed| parse_installs(&installed).into_keys().collect())
            .unwrap_or_default();
    let path = plugin_versions_path(app_data);
    let previous = read_plugin_versions(&path);
    let mut next = PluginVersionCache::default();
    refresh_catalogs(
        &plugins_dir,
        &installed_ids,
        &previous,
        remote_api,
        &mut next,
    );
    let wanted = wanted_remote_sources(&plugins_dir, &installed_ids, &next.catalogs);
    for source in &wanted {
        let Some((owner, repo)) = github_repo(&source.url) else {
            continue;
        };
        let remote = if non_empty(source.sha.as_deref()).is_some() {
            source.clone()
        } else {
            let git_ref = non_empty(source.git_ref.as_deref()).unwrap_or("HEAD");
            let sha = match remote_api.commit_sha(&owner, &repo, git_ref) {
                CommitLookup::Sha(sha) => sha,
                CommitLookup::Gone => continue,
                CommitLookup::Failed => {
                    keep_previous_resolution(source, &previous, &mut next);
                    continue;
                }
            };
            next.resolved.insert(unpinned_key(source), sha.clone());
            RemoteSource {
                sha: Some(sha),
                ..source.clone()
            }
        };
        let (Some(key), Some(api_path)) =
            (remote_cache_key(&remote), gh_manifest_api_path(&remote))
        else {
            continue;
        };
        if let Some(cached) = previous.entries.get(&key) {
            next.entries.insert(key, cached.clone());
            continue;
        }
        let sha = non_empty(remote.sha.as_deref()).unwrap_or_default();
        let version = match remote_api.manifest(&api_path) {
            ManifestFetch::Content(content) => decode_base64(&content)
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .and_then(|body| match manifest_version_from_json(&body) {
                    ManifestVersion::Version(version) => Some(Some(version)),
                    ManifestVersion::NoVersion => Some(None),
                    ManifestVersion::Unknown => None,
                }),
            ManifestFetch::Missing => matches!(
                remote_api.commit_sha(&owner, &repo, sha),
                CommitLookup::Sha(_)
            )
            .then_some(None),
            ManifestFetch::Failed => None,
        };
        if let Some(version) = version {
            next.entries.insert(key, CachedPluginVersion { version });
        }
    }
    if next != previous {
        if let Err(e) = write_plugin_versions(&path, &next) {
            eprintln!("plugin versions: failed to write cache: {e}");
        }
    }
}

/// Carries the last check's resolution of an unpinned `source`, and the
/// version it found there, over a commit lookup that failed.
fn keep_previous_resolution(
    source: &RemoteSource,
    previous: &PluginVersionCache,
    next: &mut PluginVersionCache,
) {
    let key = unpinned_key(source);
    let Some(sha) = previous.resolved.get(&key) else {
        return;
    };
    next.resolved.insert(key, sha.clone());
    let pinned = RemoteSource {
        sha: Some(sha.clone()),
        ..source.clone()
    };
    if let Some(entry_key) = remote_cache_key(&pinned) {
        if let Some(cached) = previous.entries.get(&entry_key) {
            next.entries.insert(entry_key, cached.clone());
        }
    }
}

/// [`refresh_plugin_versions_with`] over the `gh` CLI.
/// `timeout` bounds the whole refresh; calls after it passes fail, and a failed
/// lookup records nothing.
pub fn refresh_plugin_versions(
    home: &Path,
    app_data: &Path,
    gh_bin: &Path,
    timeout: std::time::Duration,
) {
    let control = AddOperationControl::new(Arc::new(AtomicBool::new(false)), timeout);
    refresh_plugin_versions_with(home, app_data, &GhRemote(gh_bin, control));
}

fn read_marketplace(plugins_dir: &Path, marketplace: &str) -> Option<String> {
    // A marketplace name from the file must not walk out of the folder.
    if !is_safe_marketplace_name(marketplace) {
        return None;
    }
    std::fs::read_to_string(
        plugins_dir
            .join("marketplaces")
            .join(marketplace)
            .join(".claude-plugin")
            .join("marketplace.json"),
    )
    .ok()
}

fn manifest_version_for(
    plugins_dir: &Path,
    marketplace: &str,
    release: &MarketplaceRelease,
    versions: &PluginVersionCache,
) -> ManifestVersion {
    let cached_version = |remote: Option<RemoteSource>| {
        remote
            .and_then(|remote| remote_cache_key(&remote))
            .and_then(|key| versions.entries.get(&key))
            .map_or(ManifestVersion::Unknown, |cached| match &cached.version {
                Some(version) => ManifestVersion::Version(version.clone()),
                None => ManifestVersion::NoVersion,
            })
    };
    match &release.source {
        PluginSource::Relative(relative) => match versions.catalogs.get(marketplace) {
            Some(catalog) => cached_version(Some(catalog_plugin_remote(catalog, relative))),
            None => local_manifest_version(
                &plugins_dir.join("marketplaces").join(marketplace),
                relative,
            ),
        },
        PluginSource::Remote(remote) => cached_version(pinned_remote(remote, versions)),
        PluginSource::Unsupported => ManifestVersion::Unknown,
    }
}

/// True when this install can still be updated from where it says it lives:
/// not managed by an organization, and a project or local install whose
/// project folder still exists.
fn install_is_updatable(install: &PluginInstall) -> bool {
    match install.scope.as_deref().unwrap_or("user") {
        "managed" => false,
        "project" | "local" => install
            .project_path
            .as_deref()
            .is_some_and(|path| Path::new(path).is_dir()),
        _ => true,
    }
}

/// Every Claude Code plugin install under `home` with an update available,
/// keyed by `<plugin>@<marketplace>`. Local file reads only.
pub fn read_plugin_updates(
    home: &Path,
    versions: &PluginVersionCache,
) -> BTreeMap<String, Vec<PluginInstallUpdate>> {
    let plugins_dir = home.join(".claude").join("plugins");
    let Ok(installed) = std::fs::read_to_string(plugins_dir.join("installed_plugins.json")) else {
        return BTreeMap::new();
    };
    let mut marketplaces: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut updates = BTreeMap::new();
    for (id, installs) in parse_installs(&installed) {
        let Some((plugin, marketplace)) = id.split_once('@') else {
            continue;
        };
        let body = marketplaces
            .entry(marketplace.to_string())
            .or_insert_with(|| marketplace_body(&plugins_dir, marketplace, &versions.catalogs));
        let Some(release) = body
            .as_deref()
            .and_then(|body| marketplace_release(body, plugin))
        else {
            continue;
        };
        let manifest = manifest_version_for(&plugins_dir, marketplace, &release, versions);
        // The sha fallback compares against the commit the ref resolved to. A
        // relative plugin in a fetched catalog has no version of its own, so
        // Claude Code versions it by the marketplace commit: compare that.
        let release = match (&release.source, versions.catalogs.get(marketplace)) {
            (PluginSource::Remote(remote), _) => MarketplaceRelease {
                source: PluginSource::Remote(
                    pinned_remote(remote, versions).unwrap_or_else(|| remote.clone()),
                ),
                ..release
            },
            (PluginSource::Relative(relative), Some(catalog)) => MarketplaceRelease {
                source: PluginSource::Remote(catalog_plugin_remote(catalog, relative)),
                ..release
            },
            _ => release,
        };
        let outdated: Vec<PluginInstallUpdate> = installs
            .iter()
            .filter(|install| install_is_updatable(install))
            .filter(|install| {
                plugin_update_verdict(install, &manifest, &release)
                    == PluginUpdateVerdict::Available
            })
            .map(|install| PluginInstallUpdate {
                scope: install.scope.clone().unwrap_or_else(|| "user".to_string()),
                project_path: install.project_path.clone(),
            })
            .collect();
        if !outdated.is_empty() {
            updates.insert(id, outdated);
        }
    }
    updates
}

/// Refuses an update for an install `installed_plugins.json` does not list:
/// the command's arguments come from the webview, and this keeps them to
/// installs Claude Code itself recorded.
pub fn require_plugin_install(
    home: &Path,
    plugin_id: &str,
    scope: &str,
    project_path: Option<&str>,
) -> Result<(), String> {
    let installed = std::fs::read_to_string(
        home.join(".claude")
            .join("plugins")
            .join("installed_plugins.json"),
    )
    .unwrap_or_default();
    let found = parse_installs(&installed)
        .get(plugin_id)
        .is_some_and(|installs| {
            installs.iter().any(|install| {
                install.scope.as_deref().unwrap_or("user") == scope
                    && (scope == "user" || install.project_path.as_deref() == project_path)
            })
        });
    if found {
        Ok(())
    } else {
        Err("This plugin install was not found.".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::fs;

    fn install(version: Option<&str>, sha: Option<&str>) -> PluginInstall {
        PluginInstall {
            scope: Some("user".to_string()),
            version: version.map(str::to_string),
            git_commit_sha: sha.map(str::to_string),
            ..Default::default()
        }
    }

    fn remote(sha: Option<&str>) -> RemoteSource {
        RemoteSource {
            url: "https://github.com/getsentry/sentry-for-claude".to_string(),
            path: None,
            sha: sha.map(str::to_string),
            git_ref: None,
        }
    }

    fn release(version: Option<&str>, sha: Option<&str>) -> MarketplaceRelease {
        MarketplaceRelease {
            version: version.map(str::to_string),
            source: PluginSource::Remote(remote(sha)),
        }
    }

    fn manifest(version: &str) -> ManifestVersion {
        ManifestVersion::Version(version.to_string())
    }

    /// Flow: sentry's pinned sha moved (215e5f1b -> 73e53541) but plugin.json
    /// says 1.4.0 at both.
    /// Expectation: Current - `claude plugin update` reports `up_to_date`.
    /// A failure means detection compares shas before versions again.
    #[test]
    fn an_unchanged_pinned_version_with_a_moved_sha_is_current() {
        let verdict = plugin_update_verdict(
            &install(Some("1.4.0"), Some("215e5f1b")),
            &manifest("1.4.0"),
            &release(None, Some("73e53541")),
        );
        assert_eq!(verdict, PluginUpdateVerdict::Current);
    }

    #[test]
    fn a_changed_manifest_version_is_an_update_even_when_the_marketplace_has_none() {
        let verdict = plugin_update_verdict(
            &install(Some("2.1.7"), Some("aaaa")),
            &manifest("2.2.120"),
            &release(None, Some("bbbb")),
        );
        assert_eq!(verdict, PluginUpdateVerdict::Available);
    }

    #[test]
    fn the_manifest_version_wins_over_the_marketplace_version() {
        let verdict = plugin_update_verdict(
            &install(Some("1.0.0"), None),
            &manifest("1.0.0"),
            &release(Some("9.9.9"), None),
        );
        assert_eq!(verdict, PluginUpdateVerdict::Current);
    }

    #[test]
    fn the_marketplace_version_is_used_when_the_manifest_has_none() {
        let no_manifest_version = ManifestVersion::NoVersion;
        assert_eq!(
            plugin_update_verdict(
                &install(Some("1.0.5"), None),
                &no_manifest_version,
                &release(Some("1.0.6"), None)
            ),
            PluginUpdateVerdict::Available
        );
        assert_eq!(
            plugin_update_verdict(
                &install(Some("1.0.6"), None),
                &no_manifest_version,
                &release(Some("1.0.6"), None)
            ),
            PluginUpdateVerdict::Current
        );
    }

    #[test]
    fn the_sha_decides_only_when_no_version_exists_anywhere() {
        let none = ManifestVersion::NoVersion;
        assert_eq!(
            plugin_update_verdict(
                &install(None, Some("215e5f1b")),
                &none,
                &release(None, Some("73e53541"))
            ),
            PluginUpdateVerdict::Available
        );
        let full = "73e53541aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        for latest in [full, "73e53541"] {
            assert_eq!(
                plugin_update_verdict(
                    &install(None, Some(full)),
                    &none,
                    &release(None, Some(latest))
                ),
                PluginUpdateVerdict::Current,
                "latest {latest}"
            );
        }
        assert_eq!(
            plugin_update_verdict(
                &install(None, Some("73e53541")),
                &none,
                &release(None, Some(full))
            ),
            PluginUpdateVerdict::Current
        );
    }

    #[test]
    fn an_unknown_installed_version_makes_no_claim() {
        assert_eq!(
            plugin_update_verdict(
                &install(Some("unknown"), Some("aaaa")),
                &manifest("1.0.6"),
                &release(None, Some("bbbb"))
            ),
            PluginUpdateVerdict::NoClaim
        );
    }

    #[test]
    fn an_unreadable_manifest_makes_no_claim() {
        assert_eq!(
            plugin_update_verdict(
                &install(Some("1.0.0"), Some("aaaa")),
                &ManifestVersion::Unknown,
                &release(Some("2.0.0"), Some("bbbb"))
            ),
            PluginUpdateVerdict::NoClaim
        );
    }

    #[test]
    fn a_release_with_no_version_and_no_sha_makes_no_claim() {
        assert_eq!(
            plugin_update_verdict(
                &install(Some("1.0.0"), Some("abc")),
                &ManifestVersion::NoVersion,
                &MarketplaceRelease::default()
            ),
            PluginUpdateVerdict::NoClaim
        );
    }

    #[test]
    fn a_malformed_marketplace_file_yields_no_release() {
        assert_eq!(marketplace_release("{ not json", "sentry"), None);
        assert_eq!(marketplace_release(r#"{"plugins": 3}"#, "sentry"), None);
        assert_eq!(marketplace_release(r#"{"plugins": []}"#, "sentry"), None);
    }

    #[test]
    fn marketplace_release_reads_each_source_shape() {
        let body = r#"{"plugins":[
            {"name":"sentry","version":null,"source":{"source":"url","url":"https://github.com/o/r.git","sha":"73e5"}},
            {"name":"figma","source":{"source":"git-subdir","url":"https://github.com/o/mono","path":"plugins/figma","ref":"main","sha":"abcd"}},
            {"name":"gh","source":{"source":"github","repo":"o/gh"}},
            {"name":"codex","version":"1.0.6","source":"./plugins/codex"},
            {"name":"odd","source":{"source":"npm","package":"x"}}]}"#;
        let source = |name| marketplace_release(body, name).unwrap().source;
        assert_eq!(
            source("sentry"),
            PluginSource::Remote(RemoteSource {
                url: "https://github.com/o/r.git".to_string(),
                path: None,
                sha: Some("73e5".to_string()),
                git_ref: None
            })
        );
        assert_eq!(
            source("figma"),
            PluginSource::Remote(RemoteSource {
                url: "https://github.com/o/mono".to_string(),
                path: Some("plugins/figma".to_string()),
                sha: Some("abcd".to_string()),
                git_ref: Some("main".to_string())
            })
        );
        assert_eq!(
            source("gh"),
            PluginSource::Remote(RemoteSource {
                url: "https://github.com/o/gh".to_string(),
                path: None,
                sha: None,
                git_ref: None
            })
        );
        assert_eq!(
            marketplace_release(body, "codex"),
            Some(MarketplaceRelease {
                version: Some("1.0.6".to_string()),
                source: PluginSource::Relative("./plugins/codex".to_string())
            })
        );
        assert_eq!(source("odd"), PluginSource::Unsupported);
    }

    #[test]
    fn cache_key_and_gh_path_for_a_url_source() {
        let source = RemoteSource {
            url: "https://github.com/getsentry/sentry-for-claude.git".to_string(),
            path: None,
            sha: Some("73e53541".to_string()),
            git_ref: None,
        };
        assert_eq!(
            remote_cache_key(&source).as_deref(),
            Some("https://github.com/getsentry/sentry-for-claude.git#@73e53541")
        );
        assert_eq!(
            gh_manifest_api_path(&source).as_deref(),
            Some("repos/getsentry/sentry-for-claude/contents/.claude-plugin/plugin.json?ref=73e53541")
        );
    }

    #[test]
    fn cache_key_and_gh_path_for_a_git_subdir_source() {
        let source = RemoteSource {
            url: "https://github.com/figma/mcp".to_string(),
            path: Some("./plugins/figma/".to_string()),
            sha: Some("abcd1234".to_string()),
            git_ref: None,
        };
        assert_eq!(
            remote_cache_key(&source).as_deref(),
            Some("https://github.com/figma/mcp#./plugins/figma/@abcd1234")
        );
        assert_eq!(
            gh_manifest_api_path(&source).as_deref(),
            Some("repos/figma/mcp/contents/plugins/figma/.claude-plugin/plugin.json?ref=abcd1234")
        );
    }

    #[test]
    fn unsafe_or_unpinned_remote_sources_build_no_gh_call() {
        let base = remote(Some("abcd"));
        assert!(gh_manifest_api_path(&base).is_some());
        let with = |edit: &dyn Fn(&mut RemoteSource)| {
            let mut source = base.clone();
            edit(&mut source);
            gh_manifest_api_path(&source)
        };
        assert_eq!(with(&|s| s.sha = None), None);
        assert_eq!(with(&|s| s.sha = Some("abc;rm".to_string())), None);
        assert_eq!(with(&|s| s.path = Some("../etc".to_string())), None);
        assert_eq!(with(&|s| s.path = Some("a/../b".to_string())), None);
        assert_eq!(with(&|s| s.path = Some("a b".to_string())), None);
        assert_eq!(
            with(&|s| s.url = "https://gitlab.com/o/r".to_string()),
            None
        );
        assert_eq!(
            with(&|s| s.url = "https://github.com/../r".to_string()),
            None
        );
        assert_eq!(
            with(&|s| s.url = "https://github.com/o/r?x=1".to_string()),
            None
        );
    }

    #[test]
    fn base64_with_newlines_decodes() {
        assert_eq!(
            decode_base64("eyJ2ZXJz\naW9uIjoiMS4wIn0=\n").as_deref(),
            Some(br#"{"version":"1.0"}"#.as_slice())
        );
        assert_eq!(decode_base64("a$b"), None);
    }

    #[test]
    fn local_manifest_version_reads_the_relative_source() {
        let dir = tempfile::tempdir().unwrap();
        let plugin = dir.path().join("plugins/codex/.claude-plugin");
        fs::create_dir_all(&plugin).unwrap();
        fs::write(plugin.join("plugin.json"), r#"{"version":"1.0.7"}"#).unwrap();
        assert_eq!(
            local_manifest_version(dir.path(), "./plugins/codex"),
            manifest("1.0.7")
        );
        assert_eq!(
            local_manifest_version(dir.path(), "./plugins/none"),
            ManifestVersion::NoVersion
        );
        fs::write(plugin.join("plugin.json"), r#"{"name":"codex"}"#).unwrap();
        assert_eq!(
            local_manifest_version(dir.path(), "./plugins/codex"),
            ManifestVersion::NoVersion
        );
        fs::write(plugin.join("plugin.json"), "{ nope").unwrap();
        assert_eq!(
            local_manifest_version(dir.path(), "./plugins/codex"),
            ManifestVersion::Unknown
        );
    }

    /// Flow: a marketplace entry's relative source walks out of the
    /// marketplace folder, by `..` or by a symlink.
    /// Expectation: the manifest outside is never read (Unknown).
    #[test]
    fn a_relative_source_that_escapes_the_marketplace_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let market = dir.path().join("market");
        fs::create_dir_all(&market).unwrap();
        let outside = dir.path().join("outside/.claude-plugin");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("plugin.json"), r#"{"version":"6.6.6"}"#).unwrap();

        assert_eq!(
            local_manifest_version(&market, "../outside"),
            ManifestVersion::Unknown
        );
        assert_eq!(
            local_manifest_version(&market, dir.path().join("outside").to_str().unwrap()),
            ManifestVersion::Unknown
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path().join("outside"), market.join("link")).unwrap();
            assert_eq!(
                local_manifest_version(&market, "./link"),
                ManifestVersion::Unknown
            );
        }
    }

    fn write_fixture(home: &Path, installed: &str, marketplace: Option<&str>) {
        let plugins = home.join(".claude/plugins");
        fs::create_dir_all(&plugins).unwrap();
        fs::write(plugins.join("installed_plugins.json"), installed).unwrap();
        if let Some(body) = marketplace {
            let dir = plugins.join("marketplaces/claude-plugins-official/.claude-plugin");
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("marketplace.json"), body).unwrap();
        }
    }

    fn write_manifest(home: &Path, relative: &str, body: &str) {
        let dir = home
            .join(".claude/plugins/marketplaces/claude-plugins-official")
            .join(relative)
            .join(".claude-plugin");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("plugin.json"), body).unwrap();
    }

    const MARKETPLACE: &str = r#"{"plugins":[
        {"name":"sentry","version":null,"source":{"source":"url","url":"https://github.com/getsentry/sentry-for-claude","sha":"73e53541"}},
        {"name":"codex","version":"1.0.6","source":"./plugins/codex"},
        {"name":"plugin-dev","version":"2.0.0","source":"./plugin-dev"}]}"#;

    fn sentry_key() -> String {
        "https://github.com/getsentry/sentry-for-claude#@73e53541".to_string()
    }

    fn cache_with(key: String, version: Option<&str>) -> PluginVersionCache {
        PluginVersionCache {
            entries: BTreeMap::from([(
                key,
                CachedPluginVersion {
                    version: version.map(str::to_string),
                },
            )]),
            ..Default::default()
        }
    }

    #[test]
    fn reader_reports_each_outdated_install_with_its_scope_and_project() {
        let home = tempfile::tempdir().unwrap();
        let project_a = tempfile::tempdir().unwrap();
        let project_b = tempfile::tempdir().unwrap();
        let (a, b) = (
            project_a.path().to_str().unwrap(),
            project_b.path().to_str().unwrap(),
        );
        write_fixture(
            home.path(),
            &format!(
                r#"{{"version":2,"plugins":{{
                "sentry@claude-plugins-official":[{{"scope":"user","version":"1.3.0","gitCommitSha":"215e5f1b"}}],
                "codex@claude-plugins-official":[
                    {{"scope":"user","version":"1.0.6"}},
                    {{"scope":"project","projectPath":"{a}","version":"1.0.5"}},
                    {{"scope":"local","projectPath":"{b}","version":"1.0.4"}}],
                "plugin-dev@claude-plugins-official":[{{"scope":"project","projectPath":"{a}","version":"unknown","gitCommitSha":"dbc4"}}]
            }}}}"#
            ),
            Some(MARKETPLACE),
        );
        let versions = cache_with(sentry_key(), Some("1.4.0"));
        let updates = read_plugin_updates(home.path(), &versions);
        assert_eq!(
            updates.get("sentry@claude-plugins-official"),
            Some(&vec![PluginInstallUpdate {
                scope: "user".to_string(),
                project_path: None
            }])
        );
        assert_eq!(
            updates.get("codex@claude-plugins-official"),
            Some(&vec![
                PluginInstallUpdate {
                    scope: "project".to_string(),
                    project_path: Some(a.to_string())
                },
                PluginInstallUpdate {
                    scope: "local".to_string(),
                    project_path: Some(b.to_string())
                },
            ])
        );
        assert!(!updates.contains_key("plugin-dev@claude-plugins-official"));
    }

    #[test]
    fn a_manifest_only_version_change_on_a_relative_source_is_an_update() {
        let home = tempfile::tempdir().unwrap();
        write_fixture(
            home.path(),
            r#"{"plugins":{"codex@claude-plugins-official":[{"scope":"user","version":"1.0.6"}]}}"#,
            Some(MARKETPLACE),
        );
        let none = PluginVersionCache::default();
        assert!(read_plugin_updates(home.path(), &none).is_empty());
        write_manifest(home.path(), "plugins/codex", r#"{"version":"1.0.7"}"#);
        assert!(
            read_plugin_updates(home.path(), &none).contains_key("codex@claude-plugins-official")
        );
    }

    #[test]
    fn a_remote_source_with_no_cache_entry_makes_no_claim() {
        let home = tempfile::tempdir().unwrap();
        write_fixture(
            home.path(),
            r#"{"plugins":{"sentry@claude-plugins-official":[{"scope":"user","version":"1.3.0","gitCommitSha":"215e5f1b"}]}}"#,
            Some(MARKETPLACE),
        );
        assert!(read_plugin_updates(home.path(), &PluginVersionCache::default()).is_empty());
    }

    #[test]
    fn a_cached_no_version_falls_back_to_the_sha() {
        let home = tempfile::tempdir().unwrap();
        write_fixture(
            home.path(),
            r#"{"plugins":{"sentry@claude-plugins-official":[{"scope":"user","version":"1.3.0","gitCommitSha":"215e5f1b"}]}}"#,
            Some(MARKETPLACE),
        );
        let versions = cache_with(sentry_key(), None);
        assert!(read_plugin_updates(home.path(), &versions)
            .contains_key("sentry@claude-plugins-official"));
    }

    #[test]
    fn managed_and_missing_project_installs_are_skipped() {
        let home = tempfile::tempdir().unwrap();
        let gone = home.path().join("deleted-project");
        write_fixture(
            home.path(),
            &format!(
                r#"{{"plugins":{{"codex@claude-plugins-official":[
                    {{"scope":"managed","version":"1.0.0"}},
                    {{"scope":"project","version":"1.0.0"}},
                    {{"scope":"project","projectPath":"{}","version":"1.0.0"}},
                    {{"scope":"local","projectPath":"{}","version":"1.0.0"}}]}}}}"#,
                gone.display(),
                home.path().display()
            ),
            Some(MARKETPLACE),
        );
        let updates = read_plugin_updates(home.path(), &PluginVersionCache::default());
        assert_eq!(
            updates.get("codex@claude-plugins-official"),
            Some(&vec![PluginInstallUpdate {
                scope: "local".to_string(),
                project_path: Some(home.path().display().to_string())
            }])
        );
    }

    #[test]
    fn reader_returns_nothing_when_files_are_missing_or_malformed() {
        let home = tempfile::tempdir().unwrap();
        let none = PluginVersionCache::default();
        assert!(read_plugin_updates(home.path(), &none).is_empty());

        write_fixture(home.path(), "{ not json", Some(MARKETPLACE));
        assert!(read_plugin_updates(home.path(), &none).is_empty());

        write_fixture(
            home.path(),
            r#"{"plugins":{"codex@claude-plugins-official":[{"scope":"user","version":"1.0.0"}]}}"#,
            Some("{ broken"),
        );
        assert!(read_plugin_updates(home.path(), &none).is_empty());

        write_fixture(
            home.path(),
            r#"{"plugins":{"codex@claude-plugins-official":[{"scope":"user","version":"1.0.0"}]}}"#,
            None,
        );
        fs::remove_dir_all(home.path().join(".claude/plugins/marketplaces")).ok();
        assert!(read_plugin_updates(home.path(), &none).is_empty());
    }

    #[test]
    fn update_requires_an_install_that_installed_plugins_lists() {
        let home = tempfile::tempdir().unwrap();
        write_fixture(
            home.path(),
            r#"{"plugins":{"codex@claude-plugins-official":[
                {"scope":"user","version":"1.0.0"},
                {"scope":"project","projectPath":"/p/a","version":"1.0.0"}]}}"#,
            None,
        );
        let id = "codex@claude-plugins-official";
        assert!(require_plugin_install(home.path(), id, "user", None).is_ok());
        assert!(require_plugin_install(home.path(), id, "project", Some("/p/a")).is_ok());
        let refused = "This plugin install was not found.";
        for (id, scope, path) in [
            ("other@claude-plugins-official", "user", None),
            (id, "project", Some("/p/b")),
            (id, "project", None),
            (id, "local", Some("/p/a")),
        ] {
            assert_eq!(
                require_plugin_install(home.path(), id, scope, path),
                Err(refused.to_string()),
                "{id} {scope} {path:?}"
            );
        }
        let empty = tempfile::tempdir().unwrap();
        assert!(require_plugin_install(empty.path(), id, "user", None).is_err());
    }

    fn b64_manifest(body: &str) -> String {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in body.as_bytes().chunks(3) {
            let n = chunk
                .iter()
                .enumerate()
                .fold(0u32, |acc, (i, b)| acc | (u32::from(*b) << (16 - 8 * i)));
            for i in 0..=chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            }
            for _ in chunk.len()..3 {
                out.push('=');
            }
        }
        out
    }

    type ScriptedManifest = (&'static str, fn() -> ManifestFetch);

    /// Scripted GitHub: manifests by api path prefix, commits by `owner/repo`.
    #[derive(Default)]
    struct FakeRemote {
        manifest_calls: RefCell<Vec<String>>,
        commit_calls: RefCell<Vec<String>>,
        manifests: Vec<ScriptedManifest>,
        readable_commits: Vec<&'static str>,
        refs: Vec<(&'static str, &'static str)>,
        /// Every commit lookup fails the way a dropped network does.
        commits_fail: bool,
    }

    impl PluginRemote for FakeRemote {
        fn manifest(&self, api_path: &str) -> ManifestFetch {
            self.manifest_calls.borrow_mut().push(api_path.to_string());
            self.manifests
                .iter()
                .find(|(prefix, _)| api_path.starts_with(prefix))
                .map_or(ManifestFetch::Failed, |(_, make)| make())
        }

        fn commit_sha(&self, owner: &str, repo: &str, git_ref: &str) -> CommitLookup {
            let repo_path = format!("{owner}/{repo}");
            self.commit_calls
                .borrow_mut()
                .push(format!("{repo_path}@{git_ref}"));
            if self.commits_fail {
                return CommitLookup::Failed;
            }
            if let Some((_, sha)) = self
                .refs
                .iter()
                .find(|(r, _)| *r == format!("{repo_path}@{git_ref}"))
            {
                return CommitLookup::Sha((*sha).to_string());
            }
            if self.readable_commits.contains(&repo_path.as_str()) {
                CommitLookup::Sha(git_ref.to_string())
            } else {
                CommitLookup::Gone
            }
        }
    }

    fn sentry_content() -> ManifestFetch {
        ManifestFetch::Content(b64_manifest(r#"{"version":"1.4.0"}"#))
    }
    fn nameonly_content() -> ManifestFetch {
        ManifestFetch::Content(b64_manifest(r#"{"name":"figma"}"#))
    }
    fn missing() -> ManifestFetch {
        ManifestFetch::Missing
    }

    #[test]
    fn refresh_caches_versions_records_missing_manifests_and_skips_failures() {
        let home = tempfile::tempdir().unwrap();
        let app_data = tempfile::tempdir().unwrap();
        write_fixture(
            home.path(),
            r#"{"plugins":{
                "sentry@claude-plugins-official":[{"scope":"user"}],
                "figma@claude-plugins-official":[{"scope":"user"}],
                "gone@claude-plugins-official":[{"scope":"user"}],
                "flaky@claude-plugins-official":[{"scope":"user"}],
                "codex@claude-plugins-official":[{"scope":"user"}]}}"#,
            Some(
                r#"{"plugins":[
                {"name":"sentry","source":{"source":"url","url":"https://github.com/o/sentry","sha":"aa11"}},
                {"name":"figma","source":{"source":"git-subdir","url":"https://github.com/o/mono","path":"plugins/figma","sha":"bb22"}},
                {"name":"gone","source":{"source":"url","url":"https://github.com/o/gone","sha":"cc33"}},
                {"name":"flaky","source":{"source":"url","url":"https://github.com/o/flaky","sha":"dd44"}},
                {"name":"codex","version":"1.0.6","source":"./plugins/codex"}]}"#,
            ),
        );
        let remote = FakeRemote {
            manifests: vec![
                ("repos/o/sentry/", sentry_content),
                ("repos/o/mono/contents/plugins/figma/", nameonly_content),
                ("repos/o/gone/", missing),
            ],
            readable_commits: vec!["o/gone"],
            ..Default::default()
        };
        refresh_plugin_versions_with(home.path(), app_data.path(), &remote);

        let cache = read_plugin_versions(&plugin_versions_path(app_data.path()));
        let version = |key: &str| cache.entries.get(key).map(|entry| entry.version.clone());
        assert_eq!(
            version("https://github.com/o/sentry#@aa11"),
            Some(Some("1.4.0".to_string()))
        );
        assert_eq!(
            version("https://github.com/o/mono#plugins/figma@bb22"),
            Some(None)
        );
        assert_eq!(version("https://github.com/o/gone#@cc33"), Some(None));
        assert_eq!(version("https://github.com/o/flaky#@dd44"), None);
        assert_eq!(remote.manifest_calls.borrow().len(), 4);

        // Cached entries are not fetched again; the failed one is retried.
        remote.manifest_calls.borrow_mut().clear();
        refresh_plugin_versions_with(home.path(), app_data.path(), &remote);
        assert_eq!(
            remote.manifest_calls.borrow().as_slice(),
            ["repos/o/flaky/contents/.claude-plugin/plugin.json?ref=dd44"]
        );
    }

    /// Flow: the manifest request 404s because the gh account cannot read a
    /// private repo (the commit check fails too).
    /// Expectation: nothing is cached, so the next check retries.
    /// A failure means a private repo is cached as "no version" forever.
    #[test]
    fn a_404_for_an_unreadable_repo_is_not_cached_as_no_version() {
        let home = tempfile::tempdir().unwrap();
        let app_data = tempfile::tempdir().unwrap();
        write_fixture(
            home.path(),
            r#"{"plugins":{"priv@claude-plugins-official":[{"scope":"user"}]}}"#,
            Some(
                r#"{"plugins":[{"name":"priv","source":{"source":"url","url":"https://github.com/o/priv","sha":"ee55"}}]}"#,
            ),
        );
        let remote = FakeRemote {
            manifests: vec![("repos/o/priv/", missing)],
            ..Default::default()
        };
        refresh_plugin_versions_with(home.path(), app_data.path(), &remote);
        let cache = read_plugin_versions(&plugin_versions_path(app_data.path()));
        assert!(cache.entries.is_empty());
        assert_eq!(remote.commit_calls.borrow().as_slice(), ["o/priv@ee55"]);
    }

    const UNPINNED: &str = r#"{"metadata":{},"plugins":[
        {"name":"gh","source":{"source":"github","repo":"o/gh","ref":"stable"}},
        {"name":"bare","source":{"source":"github","repo":"o/bare"}},
        {"name":"evil","source":{"source":"github","repo":"o/evil","ref":"a/../b"}}]}"#;

    /// Flow: marketplace sources with no sha (`github` repo, with or without
    /// a `ref`) are installed.
    /// Expectation: the ref (or HEAD) is resolved, the manifest fetched at the
    /// resolved sha, and the resolution saved so the overlay reports an update.
    /// A failure means unpinned sources are always `NoClaim`.
    #[test]
    fn unpinned_sources_are_resolved_fetched_and_reported() {
        let home = tempfile::tempdir().unwrap();
        let app_data = tempfile::tempdir().unwrap();
        write_fixture(
            home.path(),
            r#"{"plugins":{
                "gh@claude-plugins-official":[{"scope":"user","version":"1.3.0"}],
                "bare@claude-plugins-official":[{"scope":"user","version":"1.4.0"}],
                "evil@claude-plugins-official":[{"scope":"user","version":"1.0.0"}]}}"#,
            Some(UNPINNED),
        );
        let remote = FakeRemote {
            manifests: vec![("repos/o/", sentry_content)],
            refs: vec![("o/gh@stable", "abc1234"), ("o/bare@HEAD", "def5678")],
            ..Default::default()
        };
        refresh_plugin_versions_with(home.path(), app_data.path(), &remote);
        assert_eq!(
            remote.manifest_calls.borrow().as_slice(),
            [
                "repos/o/bare/contents/.claude-plugin/plugin.json?ref=def5678",
                "repos/o/gh/contents/.claude-plugin/plugin.json?ref=abc1234",
            ]
        );
        assert!(!remote
            .commit_calls
            .borrow()
            .iter()
            .any(|call| call.contains("..")));

        let versions = read_plugin_versions(&plugin_versions_path(app_data.path()));
        assert_eq!(
            versions.resolved.get("https://github.com/o/gh#@stable"),
            Some(&"abc1234".to_string())
        );
        let updates = read_plugin_updates(home.path(), &versions);
        assert!(updates.contains_key("gh@claude-plugins-official"));
        assert!(!updates.contains_key("bare@claude-plugins-official"));
        assert!(!updates.contains_key("evil@claude-plugins-official"));

        // A ref that no longer resolves drops the claim.
        let unresolved = FakeRemote {
            manifests: vec![("repos/o/", sentry_content)],
            ..Default::default()
        };
        refresh_plugin_versions_with(home.path(), app_data.path(), &unresolved);
        let versions = read_plugin_versions(&plugin_versions_path(app_data.path()));
        assert!(versions.resolved.is_empty());
        assert!(read_plugin_updates(home.path(), &versions).is_empty());
    }

    /// Flow: an unpinned source's manifest has no version.
    /// Expectation: the sha fallback compares the install with the resolved sha.
    #[test]
    fn an_unpinned_source_without_a_version_falls_back_to_the_resolved_sha() {
        let home = tempfile::tempdir().unwrap();
        write_fixture(
            home.path(),
            r#"{"plugins":{"gh@claude-plugins-official":[{"scope":"user","gitCommitSha":"1111111"}]}}"#,
            Some(UNPINNED),
        );
        let mut versions = cache_with("https://github.com/o/gh#@abc1234".to_string(), None);
        assert!(read_plugin_updates(home.path(), &versions).is_empty());
        versions.resolved.insert(
            "https://github.com/o/gh#@stable".to_string(),
            "abc1234".to_string(),
        );
        assert!(
            read_plugin_updates(home.path(), &versions).contains_key("gh@claude-plugins-official")
        );
    }

    /// Flow: the marketplace sets `metadata.pluginRoot` and lists a bare name.
    /// Expectation: the manifest is read under the root; an escaping root is
    /// still rejected. A failure means those plugins never report updates.
    #[test]
    fn plugin_root_is_prepended_to_bare_relative_sources() {
        let body = r#"{"metadata":{"pluginRoot":"./plugins"},"plugins":[
            {"name":"fmt","source":"formatter"},
            {"name":"dot","source":"./other"},
            {"name":"up","source":"../escape"}]}"#;
        let source = |name| marketplace_release(body, name).unwrap().source;
        assert_eq!(
            source("fmt"),
            PluginSource::Relative("./plugins/formatter".to_string())
        );
        assert_eq!(source("dot"), PluginSource::Relative("./other".to_string()));

        let home = tempfile::tempdir().unwrap();
        write_fixture(
            home.path(),
            r#"{"plugins":{
                "fmt@claude-plugins-official":[{"scope":"user","version":"1.0.0"}],
                "up@claude-plugins-official":[{"scope":"user","version":"1.0.0"}]}}"#,
            Some(body),
        );
        write_manifest(home.path(), "plugins/formatter", r#"{"version":"1.1.0"}"#);
        let updates = read_plugin_updates(home.path(), &PluginVersionCache::default());
        assert!(updates.contains_key("fmt@claude-plugins-official"));
        assert!(!updates.contains_key("up@claude-plugins-official"));
    }

    const UPSTREAM_CATALOG: &str = r#"{"plugins":[
        {"name":"codex","version":"1.1.0","source":"./plugins/codex"}]}"#;

    fn upstream_catalog() -> ManifestFetch {
        ManifestFetch::Content(b64_manifest(UPSTREAM_CATALOG))
    }
    fn codex_1_1_0() -> ManifestFetch {
        ManifestFetch::Content(b64_manifest(r#"{"version":"1.1.0"}"#))
    }

    /// A GitHub marketplace whose local checkout is stale: it still lists
    /// codex 1.0.6 while the install is 1.0.6.
    fn stale_checkout_home() -> tempfile::TempDir {
        let home = tempfile::tempdir().unwrap();
        write_fixture(
            home.path(),
            r#"{"plugins":{"codex@claude-plugins-official":[{"scope":"user","version":"1.0.6"}]}}"#,
            Some(r#"{"plugins":[{"name":"codex","version":"1.0.6","source":"./plugins/codex"}]}"#),
        );
        write_manifest(home.path(), "plugins/codex", r#"{"version":"1.0.6"}"#);
        fs::write(
            home.path().join(".claude/plugins/known_marketplaces.json"),
            r#"{"claude-plugins-official":{"source":{"source":"github","repo":"o/market"}}}"#,
        )
        .unwrap();
        home
    }

    fn upstream_remote() -> FakeRemote {
        FakeRemote {
            manifests: vec![
                ("repos/o/market/contents/.claude-plugin/", upstream_catalog),
                ("repos/o/market/contents/plugins/codex/", codex_1_1_0),
            ],
            refs: vec![("o/market@HEAD", "c0ffee1")],
            ..Default::default()
        }
    }

    /// Flow: the marketplace repo released codex 1.1.0 but Claude Code never
    /// refreshed the local checkout, which still says 1.0.6.
    /// Expectation: the refresh reads the remote catalog and its plugin
    /// manifest at one commit, so the update is detected.
    /// A failure means a new release of a third-party plugin never shows.
    #[test]
    fn an_upstream_release_is_detected_while_the_local_checkout_is_stale() {
        let home = stale_checkout_home();
        let app_data = tempfile::tempdir().unwrap();
        assert!(read_plugin_updates(home.path(), &PluginVersionCache::default()).is_empty());

        let remote = upstream_remote();
        refresh_plugin_versions_with(home.path(), app_data.path(), &remote);

        let versions = read_plugin_versions(&plugin_versions_path(app_data.path()));
        assert_eq!(
            versions.catalogs["claude-plugins-official"].commit,
            "c0ffee1"
        );
        assert!(remote.manifest_calls.borrow().contains(
            &"repos/o/market/contents/plugins/codex/.claude-plugin/plugin.json?ref=c0ffee1"
                .to_string()
        ));
        assert!(read_plugin_updates(home.path(), &versions)
            .contains_key("codex@claude-plugins-official"));

        // The same commit is not fetched again.
        remote.manifest_calls.borrow_mut().clear();
        refresh_plugin_versions_with(home.path(), app_data.path(), &remote);
        assert!(remote.manifest_calls.borrow().is_empty());
    }

    /// Flow: the catalog fetch fails (no network) on a first check, and on a
    /// later check after a good one.
    /// Expectation: with nothing cached the local checkout decides; with a
    /// cached catalog the badge stays.
    /// A failure means a dropped connection hides or invents an update.
    #[test]
    fn a_failed_catalog_fetch_falls_back_to_the_local_checkout_or_keeps_the_last_catalog() {
        let home = stale_checkout_home();
        let app_data = tempfile::tempdir().unwrap();
        let offline = FakeRemote {
            commits_fail: true,
            ..Default::default()
        };
        refresh_plugin_versions_with(home.path(), app_data.path(), &offline);
        let versions = read_plugin_versions(&plugin_versions_path(app_data.path()));
        assert!(versions.catalogs.is_empty());
        assert!(read_plugin_updates(home.path(), &versions).is_empty());

        refresh_plugin_versions_with(home.path(), app_data.path(), &upstream_remote());
        refresh_plugin_versions_with(home.path(), app_data.path(), &offline);
        let versions = read_plugin_versions(&plugin_versions_path(app_data.path()));
        assert!(read_plugin_updates(home.path(), &versions)
            .contains_key("codex@claude-plugins-official"));
    }

    /// Flow: an unpinned source resolved to a commit and its version was
    /// cached; the next check cannot reach GitHub.
    /// Expectation: the resolution and version stay, so the badge stays; a
    /// ref GitHub answers 404 for is dropped.
    /// A failure means a network blip makes the badge vanish.
    #[test]
    fn a_network_failure_keeps_the_resolved_commit_but_a_missing_ref_drops_it() {
        let home = tempfile::tempdir().unwrap();
        let app_data = tempfile::tempdir().unwrap();
        write_fixture(
            home.path(),
            r#"{"plugins":{"gh@claude-plugins-official":[{"scope":"user","version":"1.3.0"}]}}"#,
            Some(UNPINNED),
        );
        let online = FakeRemote {
            manifests: vec![("repos/o/", sentry_content)],
            refs: vec![("o/gh@stable", "abc1234")],
            ..Default::default()
        };
        refresh_plugin_versions_with(home.path(), app_data.path(), &online);

        let offline = FakeRemote {
            commits_fail: true,
            ..Default::default()
        };
        refresh_plugin_versions_with(home.path(), app_data.path(), &offline);
        let versions = read_plugin_versions(&plugin_versions_path(app_data.path()));
        assert_eq!(
            versions.resolved.get("https://github.com/o/gh#@stable"),
            Some(&"abc1234".to_string())
        );
        assert!(
            read_plugin_updates(home.path(), &versions).contains_key("gh@claude-plugins-official")
        );

        refresh_plugin_versions_with(home.path(), app_data.path(), &FakeRemote::default());
        let versions = read_plugin_versions(&plugin_versions_path(app_data.path()));
        assert!(versions.resolved.is_empty());
        assert!(read_plugin_updates(home.path(), &versions).is_empty());
    }

    /// Flow: a relative plugin in a fetched catalog has no version in its
    /// manifest or catalog entry; Claude Code then versions it by the
    /// marketplace commit.
    /// Expectation: an install recorded at another commit is an update, one
    /// at the catalog commit (full or abbreviated) is current.
    /// A failure means such a plugin never badges, or badges at its own commit.
    #[test]
    fn a_versionless_relative_plugin_in_a_fetched_catalog_compares_the_marketplace_commit() {
        let catalog_body = r#"{"plugins":[{"name":"codex","source":"./plugins/codex"}]}"#;
        let versions_at = |commit: &str| {
            let mut versions = cache_with(
                format!("https://github.com/o/market#./plugins/codex@{commit}"),
                None,
            );
            versions.catalogs.insert(
                "claude-plugins-official".to_string(),
                CachedCatalog {
                    url: "https://github.com/o/market".to_string(),
                    commit: commit.to_string(),
                    body: catalog_body.to_string(),
                },
            );
            versions
        };
        let updates_for = |installed_sha: &str, commit: &str| {
            let home = tempfile::tempdir().unwrap();
            write_fixture(
                home.path(),
                &format!(
                    r#"{{"plugins":{{"codex@claude-plugins-official":[{{"scope":"user","gitCommitSha":"{installed_sha}"}}]}}}}"#
                ),
                Some(catalog_body),
            );
            read_plugin_updates(home.path(), &versions_at(commit))
                .contains_key("codex@claude-plugins-official")
        };
        assert!(updates_for("1111111aaaa", "c0ffee1bbbbbbbb"));
        assert!(!updates_for("c0ffee1bbbbb", "c0ffee1bbbbbbbb"));
        assert!(!updates_for("c0ffee1bbbbbbbb", "c0ffee1"));
    }
}
