//! Packs: each source that ships installed skills, and the state of its
//! skills. Offline unless the caller passes a [`PackRemote`].
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dto::{Inventory, ScanRequest};
use crate::error::{CoreError, ErrorCode};
use crate::identity::{ProjectRef, RootKind, RootScope};
use crate::lock_file::{self, LocalEdits, SkillLockFile};
use crate::ops::{OpStatus, Operation, Outcome};
use crate::ports::{
    acquire_shared, CancelToken, FileKind, OpContext, Runtime, ScopeFs, ScopedReads,
};

/// `agent_home_roots` key for the folder that holds the skills CLI's global
/// `.skill-lock.json` when `$XDG_STATE_HOME` moves it out of `~/.agents`.
pub const SKILLS_CLI_STATE_ROOT: &str = "skills-cli-state";

const SKILL_FILE_MAX_BYTES: u64 = 64 * 1024 * 1024;

/// Input for `pack_list`.
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct PackListRequest {
    /// Only this tracked project. Absent: the home and every tracked project.
    #[serde(default)]
    pub project_path: Option<PathBuf>,
    /// Ask each git source for its newest commit and skills. Off by default:
    /// without it the call reads local files only.
    #[serde(default)]
    pub remote: bool,
}

/// Input for `pack_show`.
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct PackShowRequest {
    /// Pack source exactly as `pack_list` reports it.
    pub source: String,
    /// Only this tracked project. Absent: the home and every tracked project.
    #[serde(default)]
    pub project_path: Option<PathBuf>,
    /// Ask the git source for its newest commit and skills.
    #[serde(default)]
    pub remote: bool,
}

/// The tool that installed a pack.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PackKind {
    /// `npx skills add` from a git source.
    SkillsSh,
    /// `npx skills experimental_sync` from an npm package (`via` in `skills-lock.json`).
    Npm,
    /// `npx @sentry/dotagents` skills and plugins.
    Dotagents,
    /// A Claude Code or Codex plugin cache.
    Plugin,
}

/// What the remote check found for one pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum RemoteState {
    /// The call did not ask for a remote check.
    NotChecked,
    /// The pack has no git source to check.
    NoGitSource,
    /// The pack is pinned to one commit, so upstream cannot move.
    PinnedToCommit,
    /// The source answered.
    Checked {
        /// Upstream commit the check compared against.
        commit: String,
    },
    /// The source did not answer.
    Failed {
        /// Git's own message.
        reason: String,
    },
}

/// One installed skill of a pack, at one scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PackSkill {
    /// Skill name.
    pub name: String,
    /// Global or one project.
    pub scope: RootScope,
    /// Installed folder, when one is on disk.
    pub path: Option<PathBuf>,
    /// Agents that load this skill at this scope.
    pub agents: Vec<String>,
    /// The folder differs from the hash its lock recorded. Absent: no hash to compare.
    pub edited: Option<bool>,
    /// Upstream has a different version. Absent: no remote check, or upstream lacks the skill.
    pub behind: Option<bool>,
}

/// One source that ships installed skills.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Pack {
    /// `owner/repo`, git URL, npm package, or `plugin@marketplace`.
    pub source: String,
    /// The tool that installed it.
    pub kind: PackKind,
    /// Git URL the remote check uses.
    pub git_url: Option<String>,
    /// Branch, tag, commit, or plugin version the install is pinned to.
    pub ref_pin: Option<String>,
    /// Commands the owning agent runs to update this pack. Skill Studio does not run them.
    pub update_commands: Vec<String>,
    /// Scopes with at least one skill of this pack.
    pub scopes: Vec<RootScope>,
    /// Installed skills, by scope and name.
    pub skills: Vec<PackSkill>,
    /// Upstream skills that are not installed. Empty without a remote check.
    pub new_upstream: Vec<String>,
    /// Skills with local edits.
    pub edited_count: usize,
    /// Skills behind upstream.
    pub behind_count: usize,
    /// Remote check result.
    pub remote: RemoteState,
    /// One line for a person or agent.
    pub summary: String,
}

/// Result of `pack_list` and `pack_show`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PackReport {
    /// Packs by kind, source, and pin.
    pub packs: Vec<Pack>,
    /// One line for a person or agent.
    pub summary: String,
}

impl Outcome for PackReport {
    fn status(&self) -> OpStatus {
        if self
            .packs
            .iter()
            .any(|pack| matches!(pack.remote, RemoteState::Failed { .. }))
        {
            OpStatus::Partial
        } else {
            OpStatus::Ok
        }
    }
}

/// One skill folder in an upstream checkout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteSkill {
    /// Folder relative to the repository root, `/`-separated, `""` for the root.
    pub dir: String,
    /// Skill name: the `SKILL.md` frontmatter `name`, else the folder name.
    pub name: String,
    /// Git tree SHA of the folder, as GitHub reports it.
    pub tree_hash: String,
    /// [`skills_cli_folder_hash`] of the folder.
    pub folder_hash: String,
}

/// An upstream checkout at one commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemotePack {
    /// Commit the checkout is at.
    pub commit: String,
    /// Every skill folder in the checkout.
    pub skills: Vec<RemoteSkill>,
}

/// Reads one git source. The host runs git with the user's credentials.
pub trait PackRemote {
    /// Newest commit of `git_ref` (default branch when absent) and its skills.
    fn fetch(
        &self,
        url: &str,
        git_ref: Option<&str>,
        cancel: &dyn CancelToken,
    ) -> Result<RemotePack, String>;
}

/// Lists every installed pack.
pub fn pack_list(
    rt: &Runtime,
    ctx: &OpContext,
    req: &PackListRequest,
    remote: Option<&dyn PackRemote>,
) -> Result<PackReport, CoreError> {
    rt.run(Operation::PackList, ctx, || {
        build(
            rt,
            ctx,
            req.project_path.as_deref(),
            None,
            req.remote,
            remote,
        )
        .map(|(report, _)| report)
    })
}

/// Shows the packs with one source.
pub fn pack_show(
    rt: &Runtime,
    ctx: &OpContext,
    req: &PackShowRequest,
    remote: Option<&dyn PackRemote>,
) -> Result<PackReport, CoreError> {
    rt.run(Operation::PackShow, ctx, || {
        let (report, _) = build(
            rt,
            ctx,
            req.project_path.as_deref(),
            Some(&req.source),
            req.remote,
            remote,
        )?;
        if report.packs.is_empty() {
            return Err(no_pack(&req.source));
        }
        Ok(report)
    })
}

pub(crate) fn no_pack(source: &str) -> CoreError {
    CoreError::new(
        ErrorCode::AmbiguousTarget,
        format!("no installed pack has source {source}; pack list shows the sources"),
    )
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PackKey {
    kind: PackKind,
    source: String,
    ref_pin: Option<String>,
}

struct Entry {
    key: PackKey,
    git_url: Option<String>,
    scope: RootScope,
    name: String,
    upstream_dir: Option<String>,
    recorded_hash: Option<String>,
    path: Option<PathBuf>,
    agents: Vec<String>,
    update_command: Option<String>,
    edited: Option<bool>,
}

/// The packs and the scan they came from.
pub(crate) fn build(
    rt: &Runtime,
    ctx: &OpContext,
    project_path: Option<&Path>,
    source: Option<&str>,
    want_remote: bool,
    remote: Option<&dyn PackRemote>,
) -> Result<(PackReport, Inventory), CoreError> {
    let remote = match (want_remote, remote) {
        (false, _) => None,
        (true, Some(remote)) => Some(remote),
        (true, None) => {
            return Err(CoreError::new(
                ErrorCode::Unsupported,
                "this surface has no git access for a remote check",
            ))
        }
    };
    let inventory = scan(rt, ctx)?;
    let fs = ScopedReads::new(rt.ports.fs.as_ref(), &rt.scope);
    let mut entries = Vec::new();
    for (scope, root) in selected_scopes(rt, project_path)? {
        ctx.checkpoint()?;
        let global_lock = ledger_entries(rt, &fs, &scope, root.as_deref(), &mut entries)?;
        for entry in entries.iter_mut().filter(|e| e.scope == scope) {
            if entry.path.is_none() {
                locate(rt, &inventory, entry);
            }
            entry.edited = local_edits(&fs, global_lock.as_ref(), entry);
        }
        plugin_entries(&inventory, &scope, &mut entries);
    }
    let mut grouped: BTreeMap<PackKey, (PackKey, Vec<Entry>)> = BTreeMap::new();
    for entry in entries {
        let identity = PackKey {
            source: source_identity(entry.key.kind, &entry.key.source),
            ..entry.key.clone()
        };
        if source.is_none_or(|wanted| source_identity(entry.key.kind, wanted) == identity.source) {
            let (shown, members) = grouped
                .entry(identity)
                .or_insert_with(|| (entry.key.clone(), Vec::new()));
            if entry.key.source.len() < shown.source.len() {
                shown.source.clone_from(&entry.key.source);
            }
            members.push(entry);
        }
    }
    let mut packs = Vec::new();
    for (key, entries) in grouped.into_values() {
        ctx.checkpoint()?;
        packs.push(pack(&fs, key, entries, remote, ctx.cancel.as_ref()));
    }
    let summary = report_summary(&packs, want_remote);
    Ok((PackReport { packs, summary }, inventory))
}

pub(crate) fn scan(rt: &Runtime, ctx: &OpContext) -> Result<Inventory, CoreError> {
    ctx.checkpoint()?;
    let _folder_guard = rt.ports.history.guard_read_data_folder()?;
    let _guard = acquire_shared(rt.ports.leases.as_ref(), &rt.scope)?;
    crate::ops::scan_inner(rt, ctx, &ScanRequest::default())
}

/// What makes two spellings of one source the same pack. A remote git
/// source drops its scheme, its user, a trailing `/` and `.git`, so
/// `owner/repo`, `https://github.com/Owner/repo.git`, and
/// `git@github.com:owner/repo.git` match; case is ignored only on GitHub.
/// A port stays. Local paths and other kinds match as written.
fn source_identity(kind: PackKind, source: &str) -> String {
    let source = source.trim();
    let local = source.starts_with(['/', '.', '~', '\\'])
        || source.as_bytes().get(1) == Some(&b':') && source.as_bytes()[0].is_ascii_alphabetic();
    if !matches!(kind, PackKind::SkillsSh | PackKind::Dotagents) || local {
        return source.to_string();
    }
    let (url, mut s) = match source.split_once("://") {
        Some((_, rest)) => (true, rest),
        None => (false, source),
    };
    if let Some((user, rest)) = s.split_once('@') {
        if !user.contains('/') {
            s = rest;
        }
    }
    let mut s = match s.split_once(':') {
        Some((host, rest)) if !url && !host.contains('/') => {
            format!("{host}/{}", rest.trim_start_matches('/'))
        }
        _ => s.to_string(),
    };
    s.truncate(s.trim_end_matches('/').len());
    if let Some(stripped) = s.strip_suffix(".git") {
        s = stripped.to_string();
    }
    if let Some((owner, repo)) = s.split_once('/') {
        if !owner.contains(['.', ':']) && !repo.contains('/') {
            s = format!("github.com/{s}");
        }
    }
    if s.to_ascii_lowercase().starts_with("github.com/") {
        s.make_ascii_lowercase();
    }
    s
}

pub(crate) fn selected_scopes(
    rt: &Runtime,
    project_path: Option<&Path>,
) -> Result<Vec<(RootScope, Option<PathBuf>)>, CoreError> {
    let projects: Vec<_> = rt
        .scope
        .projects
        .iter()
        .filter(|p| project_path.is_none_or(|wanted| p.lexical == wanted || p.canonical == wanted))
        .map(|p| {
            (
                RootScope::Project(ProjectRef(p.lexical.clone())),
                Some(p.lexical.clone()),
            )
        })
        .collect();
    match project_path {
        None => Ok(std::iter::once((RootScope::Global, None))
            .chain(projects)
            .collect()),
        Some(wanted) if projects.is_empty() => Err(CoreError::new(
            ErrorCode::InvalidRequest,
            "the project is not in this scope",
        )
        .at(wanted)),
        Some(_) => Ok(projects),
    }
}

#[derive(Deserialize, Default)]
struct ProjectLock {
    #[serde(default)]
    skills: BTreeMap<String, ProjectLockEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectLockEntry {
    source: String,
    #[serde(default)]
    source_type: String,
    #[serde(default)]
    source_url: Option<String>,
    #[serde(default, rename = "ref")]
    git_ref: Option<String>,
    #[serde(default)]
    skill_path: Option<String>,
    #[serde(default)]
    computed_hash: Option<String>,
    #[serde(default)]
    via: Option<String>,
}

#[derive(Deserialize, Default)]
struct DotagentsManifest {
    #[serde(default)]
    skills: Vec<ManifestRow>,
    #[serde(default)]
    plugins: Vec<ManifestRow>,
}

#[derive(Deserialize)]
struct ManifestRow {
    name: String,
    #[serde(default)]
    source: Option<String>,
    #[serde(default, rename = "ref")]
    git_ref: Option<String>,
    #[serde(default)]
    targets: Vec<String>,
}

#[derive(Deserialize, Default)]
struct DotagentsLock {
    #[serde(default)]
    skills: BTreeMap<String, DotagentsLockRow>,
    #[serde(default)]
    plugins: BTreeMap<String, DotagentsLockRow>,
}

#[derive(Deserialize)]
struct DotagentsLockRow {
    source: String,
    #[serde(default)]
    resolved_url: Option<String>,
    #[serde(default)]
    resolved_path: Option<String>,
}

/// Appends the skills.sh and dotagents entries of one scope. Returns the
/// global skills.sh lock, which [`local_edits`] needs for its 40-hex hashes.
fn ledger_entries(
    rt: &Runtime,
    fs: &dyn ScopeFs,
    scope: &RootScope,
    project: Option<&Path>,
    entries: &mut Vec<Entry>,
) -> Result<Option<SkillLockFile>, CoreError> {
    let home = &rt.scope.home.lexical;
    let mut global_lock = None;
    let skills_sh: Vec<(String, ProjectLockEntry)> = if let Some(project) = project {
        let path = lock_file::project_lock_file_path(project);
        read_json::<ProjectLock>(fs, &path)?
            .skills
            .into_iter()
            .collect()
    } else {
        let path = rt
            .scope
            .agent_home_roots
            .get(SKILLS_CLI_STATE_ROOT)
            .map_or_else(
                || lock_file::lock_file_path(home),
                |root| root.lexical.join(".skill-lock.json"),
            );
        let lock = lock_file::read_lock_file(fs, &path)?;
        let rows = lock
            .skills
            .iter()
            .map(|(name, e)| {
                let text = |key: &str| e.extra.get(key).and_then(|v| v.as_str()).map(String::from);
                let row = ProjectLockEntry {
                    source: e.source.clone(),
                    source_type: e.source_type.clone(),
                    source_url: Some(e.source_url.clone()).filter(|url| !url.is_empty()),
                    git_ref: text("ref"),
                    skill_path: e.skill_path.clone(),
                    computed_hash: Some(e.skill_folder_hash.clone()),
                    via: text("via"),
                };
                (name.clone(), row)
            })
            .collect();
        global_lock = Some(lock);
        rows
    };
    for (name, row) in skills_sh {
        let (kind, source) = match (&row.via, row.source_type.as_str()) {
            (Some(package), _) => (PackKind::Npm, package.clone()),
            (None, "node_modules") => (PackKind::Npm, row.source.clone()),
            (None, _) => (PackKind::SkillsSh, row.source.clone()),
        };
        let git_url = match (kind, row.source_type.as_str()) {
            (PackKind::SkillsSh, "github") => Some(
                row.source_url
                    .clone()
                    .unwrap_or_else(|| format!("https://github.com/{}.git", row.source)),
            ),
            (PackKind::SkillsSh, "gitlab" | "git") => row.source_url.clone(),
            _ => None,
        };
        entries.push(Entry {
            key: PackKey {
                kind,
                source,
                ref_pin: row.git_ref,
            },
            git_url,
            scope: scope.clone(),
            name,
            upstream_dir: row.skill_path.as_deref().map(skill_dir),
            recorded_hash: row.computed_hash,
            path: None,
            agents: Vec::new(),
            update_command: None,
            edited: None,
        });
    }

    let dir = crate::dotagents_ledger::dotagents_dir(home, project);
    let manifest: DotagentsManifest =
        crate::dotagents_ledger::read_toml(fs, &dir.join("agents.toml"))?.unwrap_or_default();
    let lock: DotagentsLock =
        crate::dotagents_ledger::read_toml(fs, &dir.join("agents.lock"))?.unwrap_or_default();
    let declared_ref = |name: &str, source: &str| {
        let row = manifest
            .skills
            .iter()
            .find(|row| row.name == name)
            .or_else(|| {
                manifest
                    .skills
                    .iter()
                    .find(|row| row.name == "*" && row.source.as_deref() == Some(source))
            });
        row.and_then(|row| row.git_ref.clone())
    };
    for (name, row) in &lock.skills {
        entries.push(dotagents_entry(
            scope,
            name.clone(),
            row.source.clone(),
            declared_ref(name, &row.source),
            Some(row),
        ));
    }
    let plugins_root = project
        .map_or_else(|| dir.clone(), |p| p.join(".agents"))
        .join("plugins");
    for row in &manifest.plugins {
        let Some(source) = row.source.clone() else {
            continue;
        };
        let locked = lock.plugins.get(&row.name);
        let skills_dir = plugins_root.join(&row.name).join("skills");
        for skill in skill_folders(fs, &skills_dir) {
            let mut entry = dotagents_entry(
                scope,
                skill.clone(),
                source.clone(),
                row.git_ref.clone(),
                locked,
            );
            entry.upstream_dir = None;
            entry.path = Some(skills_dir.join(&skill));
            entry.agents.clone_from(&row.targets);
            entries.push(entry);
        }
    }
    Ok(global_lock)
}

fn dotagents_entry(
    scope: &RootScope,
    name: String,
    source: String,
    ref_pin: Option<String>,
    locked: Option<&DotagentsLockRow>,
) -> Entry {
    let git_url = locked.and_then(|row| row.resolved_url.clone()).or_else(|| {
        crate::dotagents_ledger::github_repo_from_source(&source)
            .map(|repo| format!("https://github.com/{repo}.git"))
    });
    Entry {
        key: PackKey {
            kind: PackKind::Dotagents,
            source,
            ref_pin,
        },
        git_url,
        scope: scope.clone(),
        name,
        upstream_dir: locked
            .and_then(|row| row.resolved_path.as_deref())
            .map(skill_dir),
        recorded_hash: None,
        path: None,
        agents: Vec::new(),
        update_command: None,
        edited: None,
    }
}

/// Fills the folder and agents of a ledger entry from the scan.
fn locate(rt: &Runtime, inventory: &Inventory, entry: &mut Entry) {
    let Some(skill) = inventory.skills.iter().find(|s| s.name.0 == entry.name) else {
        return;
    };
    let deployments: Vec<_> = skill
        .deployments
        .iter()
        .filter(|d| d.root.scope == entry.scope && d.plugin.is_none())
        .collect();
    let mut agents = BTreeSet::new();
    for deployment in &deployments {
        match &deployment.root.kind {
            RootKind::Harness(agent) | RootKind::Legacy(agent) => {
                agents.insert(agent.as_str().to_string());
            }
            RootKind::Universal => agents.extend(
                crate::ops_agent_off::readers(rt, skill, deployment)
                    .iter()
                    .map(|agent| agent.as_str().to_string()),
            ),
            RootKind::Parked | RootKind::PluginCache(_) => {}
        }
    }
    entry.agents = agents.into_iter().collect();
    entry.path = deployments
        .iter()
        .find(|d| d.root.kind == RootKind::Universal)
        .or_else(|| deployments.iter().find(|d| !d.is_symlink))
        .map(|d| d.resolved_path.clone().unwrap_or_else(|| d.path.clone()));
}

fn plugin_entries(inventory: &Inventory, scope: &RootScope, entries: &mut Vec<Entry>) {
    for skill in &inventory.skills {
        for deployment in skill.deployments.iter().filter(|d| &d.root.scope == scope) {
            let (Some(plugin), RootKind::PluginCache(agent)) =
                (&deployment.plugin, &deployment.root.kind)
            else {
                continue;
            };
            let source = format!("{}@{}", plugin.plugin, plugin.marketplace);
            let update_command = match agent.as_str() {
                "claude-code" => Some(format!("claude plugin update {source}")),
                "codex" => Some("codex plugin marketplace upgrade".to_string()),
                _ => None,
            };
            entries.push(Entry {
                key: PackKey {
                    kind: PackKind::Plugin,
                    source,
                    ref_pin: plugin.version.clone(),
                },
                git_url: None,
                scope: scope.clone(),
                name: skill.name.0.clone(),
                upstream_dir: None,
                recorded_hash: None,
                path: Some(deployment.path.clone()),
                agents: vec![agent.as_str().to_string()],
                update_command,
                edited: None,
            });
        }
    }
}

fn local_edits(
    fs: &dyn ScopeFs,
    global_lock: Option<&SkillLockFile>,
    entry: &Entry,
) -> Option<bool> {
    let hash = entry.recorded_hash.as_deref()?;
    let folder = entry.path.as_deref()?;
    match (hash.len(), global_lock) {
        (40, Some(lock)) => match lock_file::local_edits(fs, lock, &entry.name, folder) {
            LocalEdits::Edited => Some(true),
            LocalEdits::Unedited => Some(false),
            LocalEdits::Unknown => None,
        },
        (64, _) => skills_cli_folder_hash(fs, folder)
            .ok()
            .map(|local| local != hash),
        _ => None,
    }
}

fn pack(
    fs: &dyn ScopeFs,
    key: PackKey,
    entries: Vec<Entry>,
    remote: Option<&dyn PackRemote>,
    cancel: &dyn CancelToken,
) -> Pack {
    let git_url = entries.iter().find_map(|e| e.git_url.clone());
    let pinned = key
        .ref_pin
        .as_deref()
        .is_some_and(|r| r.len() == 40 && r.bytes().all(|b| b.is_ascii_hexdigit()));
    let upstream = match (remote, &git_url) {
        (None, _) => Err(RemoteState::NotChecked),
        (Some(_), None) => Err(RemoteState::NoGitSource),
        (Some(_), Some(_)) if pinned => Err(RemoteState::PinnedToCommit),
        (Some(remote), Some(url)) => remote
            .fetch(url, key.ref_pin.as_deref(), cancel)
            .map_err(|reason| RemoteState::Failed { reason }),
    };
    let mut matched = BTreeSet::new();
    let mut skills = Vec::new();
    let mut scopes = BTreeSet::new();
    let mut update_commands = BTreeSet::new();
    for entry in entries {
        scopes.insert(entry.scope.clone());
        update_commands.extend(entry.update_command.clone());
        let recorded = entry.recorded_hash.as_deref();
        let behind = upstream.as_ref().ok().and_then(|up| {
            let found = up.skills.iter().position(|s| {
                entry
                    .upstream_dir
                    .as_deref()
                    .map_or(s.name == entry.name, |dir| s.dir == dir)
            })?;
            matched.insert(found);
            let upstream = &up.skills[found];
            Some(match recorded.map(str::len) {
                Some(40) => recorded != Some(upstream.tree_hash.as_str()),
                Some(_) => recorded != Some(upstream.folder_hash.as_str()),
                None => entry
                    .path
                    .as_deref()
                    .and_then(|path| skills_cli_folder_hash(fs, path).ok())
                    .is_some_and(|local| local != upstream.folder_hash),
            })
        });
        skills.push(PackSkill {
            name: entry.name,
            scope: entry.scope,
            path: entry.path,
            agents: entry.agents,
            edited: entry.edited,
            behind,
        });
    }
    skills.sort_by(|a, b| (&a.scope, &a.name).cmp(&(&b.scope, &b.name)));
    let (new_upstream, remote) = match upstream {
        Ok(up) => {
            let mut names: Vec<String> = up
                .skills
                .iter()
                .enumerate()
                .filter(|(i, s)| !matched.contains(i) && !skills.iter().any(|k| k.name == s.name))
                .map(|(_, s)| s.name.clone())
                .collect();
            names.sort();
            names.dedup();
            (names, RemoteState::Checked { commit: up.commit })
        }
        Err(state) => (Vec::new(), state),
    };
    let edited_count = skills.iter().filter(|s| s.edited == Some(true)).count();
    let behind_count = skills.iter().filter(|s| s.behind == Some(true)).count();
    let summary = pack_summary(
        skills.len(),
        edited_count,
        behind_count,
        &new_upstream,
        &remote,
    );
    Pack {
        source: key.source,
        kind: key.kind,
        git_url,
        ref_pin: key.ref_pin,
        update_commands: update_commands.into_iter().collect(),
        scopes: scopes.into_iter().collect(),
        skills,
        new_upstream,
        edited_count,
        behind_count,
        remote,
        summary,
    }
}

pub(crate) fn plural(count: usize, one: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{count} {one}s")
    }
}

fn pack_summary(
    skills: usize,
    edited: usize,
    behind: usize,
    new_upstream: &[String],
    remote: &RemoteState,
) -> String {
    let mut parts = vec![plural(skills, "skill")];
    if edited > 0 {
        parts.push(format!("{edited} edited locally"));
    }
    parts.push(match remote {
        RemoteState::NotChecked => "upstream not checked".to_string(),
        RemoteState::NoGitSource => "no git source to check".to_string(),
        RemoteState::PinnedToCommit => "pinned to a commit".to_string(),
        RemoteState::Failed { reason } => format!("upstream check failed: {reason}"),
        RemoteState::Checked { .. } if behind == 0 && new_upstream.is_empty() => {
            "up to date".to_string()
        }
        RemoteState::Checked { .. } => format!("{behind} behind upstream"),
    });
    if !new_upstream.is_empty() {
        parts.push(format!("new upstream: {}", new_upstream.join(", ")));
    }
    parts.join(", ")
}

fn report_summary(packs: &[Pack], remote: bool) -> String {
    let skills: usize = packs.iter().map(|p| p.skills.len()).sum();
    let edited: usize = packs.iter().map(|p| p.edited_count).sum();
    let behind: usize = packs.iter().map(|p| p.behind_count).sum();
    let new: usize = packs.iter().map(|p| p.new_upstream.len()).sum();
    let failed = packs
        .iter()
        .filter(|p| matches!(p.remote, RemoteState::Failed { .. }))
        .count();
    let mut parts = vec![plural(packs.len(), "pack"), plural(skills, "skill")];
    if edited > 0 {
        parts.push(format!("{edited} edited locally"));
    }
    if !remote {
        parts.push("upstream not checked (pass remote to check)".to_string());
        return parts.join(", ");
    }
    parts.push(format!("{behind} behind upstream"));
    parts.push(format!("{new} new upstream"));
    if failed > 0 {
        parts.push(format!("{} could not be checked", plural(failed, "pack")));
    }
    parts.join(", ")
}

/// `skills/foo/SKILL.md` or `skills/foo/` to `skills/foo`.
fn skill_dir(path: &str) -> String {
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix("SKILL.md").unwrap_or(path);
    path.trim_end_matches('/').to_string()
}

fn skill_folders(fs: &dyn ScopeFs, dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs
        .read_dir(dir)
        .unwrap_or_default()
        .into_iter()
        .filter(|e| {
            e.kind == FileKind::Dir
                && fs
                    .symlink_metadata(&dir.join(&e.name).join("SKILL.md"))
                    .is_ok()
        })
        .map(|e| e.name)
        .collect();
    names.sort();
    names
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(
    fs: &dyn ScopeFs,
    path: &Path,
) -> Result<T, CoreError> {
    let bytes = match fs.read_capped(path, lock_file::LOCK_FILE_MAX_BYTES) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(T::default()),
        Err(e) => return Err(CoreError::io(path, e)),
    };
    serde_json::from_slice(&bytes)
        .map_err(|e| CoreError::new(ErrorCode::Io, format!("cannot parse: {e}")).at(path))
}

/// The skills CLI's `computedHash` of a skill folder: SHA-256 over each
/// file's `/`-separated path, a NUL, its byte length, a NUL, and its bytes,
/// in path order. CRLF reads as LF in text files. `.git`, `__pycache__`,
/// `__pypackages__`, and `metadata.json` are skipped; a link is followed
/// only when its target is inside the folder, and a folder already open on
/// the current path is skipped, so a link loop ends.
pub fn skills_cli_folder_hash(fs: &dyn ScopeFs, dir: &Path) -> Result<String, CoreError> {
    let root = fs.canonicalize(dir).map_err(|e| CoreError::io(dir, e))?;
    let mut files = Vec::new();
    collect_files(fs, &root, &root, "", &mut files, &mut vec![root.clone()])?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut data = Vec::new();
    for (relative, path) in files {
        let mut bytes = fs
            .read_capped(&path, SKILL_FILE_MAX_BYTES)
            .map_err(|e| CoreError::io(&path, e))?;
        if !bytes.contains(&0) && bytes.windows(2).any(|w| w == b"\r\n") {
            let mut normalized = Vec::with_capacity(bytes.len());
            let mut iter = bytes.iter().peekable();
            while let Some(&byte) = iter.next() {
                if !(byte == b'\r' && iter.peek() == Some(&&b'\n')) {
                    normalized.push(byte);
                }
            }
            bytes = normalized;
        }
        data.extend_from_slice(relative.as_bytes());
        data.push(0);
        data.extend_from_slice(bytes.len().to_string().as_bytes());
        data.push(0);
        data.extend_from_slice(&bytes);
    }
    Ok(crate::identity::sha256_hex(&data))
}

fn collect_files(
    fs: &dyn ScopeFs,
    root: &Path,
    dir: &Path,
    prefix: &str,
    files: &mut Vec<(String, PathBuf)>,
    ancestors: &mut Vec<PathBuf>,
) -> Result<(), CoreError> {
    for entry in fs.read_dir(dir).map_err(|e| CoreError::io(dir, e))? {
        let path = dir.join(&entry.name);
        let relative = format!("{prefix}{}", entry.name);
        let kind = if entry.kind == FileKind::Symlink {
            match fs.canonicalize(&path) {
                Ok(target) if target.starts_with(root) => fs
                    .symlink_metadata(&target)
                    .map_or(FileKind::Other, |f| f.kind),
                _ => continue,
            }
        } else {
            entry.kind
        };
        match kind {
            FileKind::Dir
                if !matches!(
                    entry.name.as_str(),
                    ".git" | "__pycache__" | "__pypackages__"
                ) =>
            {
                let real = fs
                    .canonicalize(&path)
                    .map_err(|e| CoreError::io(&path, e))?;
                if !ancestors.contains(&real) {
                    ancestors.push(real);
                    collect_files(fs, root, &path, &format!("{relative}/"), files, ancestors)?;
                    ancestors.pop();
                }
            }
            FileKind::File if entry.name != "metadata.json" => files.push((relative, path)),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{source_identity, PackKind};

    #[test]
    fn source_identity_joins_only_spellings_of_one_repo_or_merges_distinct_packs() {
        let same = |a: &str, b: &str| {
            source_identity(PackKind::SkillsSh, a) == source_identity(PackKind::SkillsSh, b)
        };
        assert!(same("owner/repo", "https://github.com/Owner/repo.git/"));
        assert!(same("owner/repo", "git@github.com:OWNER/repo.git"));
        assert!(same("123org/repo", "git@github.com:123org/repo"));
        assert!(!same(
            "ssh://git@example.com:2222/team/repo.git",
            "ssh://git@example.com:3333/team/repo.git"
        ));
        assert!(!same(
            "https://gitlab.com/Team/repo",
            "https://gitlab.com/team/repo"
        ));
        assert!(!same("/srv/Skills", "/srv/skills"));
        assert!(!same("./Skills", "./skills"));
    }
}
