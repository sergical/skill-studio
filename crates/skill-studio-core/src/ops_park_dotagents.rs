//! Install-aware Park for skills dotagents manages (#389).
//!
//! `dotagents install` copies every skill `agents.toml` lists into
//! `~/.agents/skills` on every run, so a parked copy would come back at the
//! next install. Park therefore runs `dotagents remove <name> -y` (it drops
//! the entry, or adds the name to a `name = "*"` entry's `exclude`, and
//! deletes the skill's `agents.lock` row), and turning the skill on edits
//! `agents.toml` and `agents.lock` back: dotagents has no command for that.
//!
//! `park` plans first ([`plan_park`]), so a refusal leaves no journal row;
//! the edits to `agents.toml` go through `toml_edit`, which keeps comments
//! and formatting.

use std::path::{Path, PathBuf};

use crate::dto::DeploymentDto;
use crate::error::{CoreError, ErrorCode};
use crate::identity::{LifecycleOwnerKind, RootScope, SkillName};
use crate::ports::{ExclusiveGuard, OpContext, ProcessSpec, Runtime, ScopeFs};

/// A file's text before an edit, to write back when a later step fails.
/// `None` text means the file did not exist.
pub(crate) struct FileSnapshot {
    path: PathBuf,
    text: Option<String>,
}

/// A `name = "*"` entry that supplied the skill, found again by what it
/// says rather than by position: entries move when the person edits the file.
#[derive(Clone)]
struct WildcardRef {
    source: String,
    path: Option<String>,
}

/// What `agents.lock` records about the skill: the fields dotagents matches
/// wildcard entries on.
struct Locked {
    source: String,
    resolved_path: Option<String>,
}

/// What Park must undo in dotagents' files for one skill.
pub(crate) struct DotagentsPark {
    /// The program that runs dotagents: `dotagents` itself, or `npx`.
    program: PathBuf,
    /// Arguments before the dotagents ones; `-y @sentry/dotagents` for `npx`.
    prefix_args: Vec<String>,
    /// `agents.toml`, or the file its link resolves to.
    pub(crate) config: PathBuf,
    /// `agents.lock`, or the file its link resolves to.
    pub(crate) lock: PathBuf,
    /// Both files as read, written back when `dotagents remove` fails.
    originals: Vec<FileSnapshot>,
    /// The skill's own `[[skills]]` entry, kept so turn-on can put it back.
    /// `None` when a wildcard entry supplies the skill.
    entry: Option<String>,
    /// The skill's `[skills.<name>]` table in `agents.lock`. `remove` deletes
    /// it, and with no row dotagents treats the skill as new.
    lock_entry: Option<String>,
    /// The skill's lock fields, read before `remove` deletes the row.
    locked: Option<Locked>,
    /// The `name = "*"` entries that supply the skill and did not exclude it
    /// yet. `remove` adds the exclude to one of them; turn-on lifts it from
    /// these entries only, so an exclude the person wrote stays.
    wildcards: Vec<WildcardRef>,
}

/// The `dotagents` fields a park row records for turn-on.
pub(crate) struct RecordedDotagents {
    pub(crate) config: PathBuf,
    pub(crate) lock: Option<PathBuf>,
    entry: Option<String>,
    lock_entry: Option<String>,
    wildcards: Vec<WildcardRef>,
}

impl DotagentsPark {
    /// The `payload.dotagents` value the park row records.
    pub(crate) fn payload(&self) -> serde_json::Value {
        let wildcards: Vec<serde_json::Value> = self
            .wildcards
            .iter()
            .map(|w| serde_json::json!({ "source": w.source, "path": w.path }))
            .collect();
        serde_json::json!({
            "config": self.config,
            "lock": self.lock,
            "entry": self.entry,
            "lock_entry": self.lock_entry,
            "wildcards": wildcards,
        })
    }
}

/// Reads back what [`DotagentsPark::payload`] recorded.
pub(crate) fn recorded(payload: &serde_json::Value) -> Option<RecordedDotagents> {
    let value = payload.get("dotagents")?;
    let text = |key: &str| {
        value
            .get(key)
            .and_then(|field| field.as_str())
            .map(str::to_string)
    };
    Some(RecordedDotagents {
        config: PathBuf::from(value.get("config")?.as_str()?),
        lock: text("lock").map(PathBuf::from),
        entry: text("entry"),
        lock_entry: text("lock_entry"),
        wildcards: value
            .get("wildcards")
            .and_then(|rows| rows.as_array())
            .map(|rows| {
                rows.iter()
                    .filter_map(|row| {
                        Some(WildcardRef {
                            source: row.get("source")?.as_str()?.to_string(),
                            path: row
                                .get("path")
                                .and_then(|path| path.as_str())
                                .map(str::to_string),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
    })
}

fn manifest_dir(rt: &Runtime, scope: &RootScope) -> PathBuf {
    let project = match scope {
        RootScope::Global => None,
        RootScope::Project(project) => Some(project.0.as_path()),
    };
    crate::dotagents_ledger::dotagents_dir(&rt.scope.home.lexical, project)
}

fn read_text(fs: &dyn ScopeFs, path: &Path) -> Result<Option<String>, CoreError> {
    let bytes = match fs.read_capped(path, crate::dotagents_ledger::DOTAGENTS_FILE_MAX_BYTES) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(CoreError::io(path, e)),
    };
    String::from_utf8(bytes).map(Some).map_err(|e| {
        CoreError::new(
            ErrorCode::InvalidRequest,
            format!("{} is not valid UTF-8: {e}", path.display()),
        )
        .at(path)
    })
}

fn parse(path: &Path, text: &str) -> Result<toml_edit::DocumentMut, CoreError> {
    text.parse::<toml_edit::DocumentMut>().map_err(|e| {
        CoreError::new(
            ErrorCode::InvalidRequest,
            format!("{} is not valid TOML: {e}", path.display()),
        )
        .at(path)
    })
}

fn read_manifest(
    fs: &dyn ScopeFs,
    config: &Path,
) -> Result<Option<(String, toml_edit::DocumentMut)>, CoreError> {
    let Some(text) = read_text(fs, config)? else {
        return Ok(None);
    };
    let doc = parse(config, &text)?;
    Ok(Some((text, doc)))
}

fn rows(doc: &toml_edit::DocumentMut) -> impl Iterator<Item = &toml_edit::Table> {
    doc.get("skills")
        .and_then(toml_edit::Item::as_array_of_tables)
        .into_iter()
        .flat_map(toml_edit::ArrayOfTables::iter)
}

fn row_str<'a>(row: &'a toml_edit::Table, key: &str) -> Option<&'a str> {
    row.get(key).and_then(toml_edit::Item::as_str)
}

fn row_name(row: &toml_edit::Table) -> Option<&str> {
    row_str(row, "name")
}

fn excludes(row: &toml_edit::Table, name: &str) -> bool {
    row.get("exclude")
        .and_then(toml_edit::Item::as_array)
        .is_some_and(|list| list.iter().any(|item| item.as_str() == Some(name)))
}

fn lists_explicitly(doc: &toml_edit::DocumentMut, name: &str) -> bool {
    rows(doc).any(|row| row_name(row) == Some(name))
}

/// `owner/repo` of a hosted URL body such as `owner/repo.git/@ref`; GitLab
/// owners may be nested groups. Mirrors the GITHUB_* and GITLAB_* patterns of
/// dotagents-lib (`sources/repository-source.js`).
fn hosted_owner_repo(rest: &str, nested_groups: bool) -> Option<String> {
    let base = match rest.split_once('@') {
        Some((_, "")) => return None,
        Some((base, _ref)) => base,
        None => rest,
    };
    let base = base.strip_suffix('/').unwrap_or(base);
    let base = base.strip_suffix(".git").unwrap_or(base);
    let (owner, repo) = if nested_groups {
        base.rsplit_once('/')?
    } else {
        base.split_once('/')?
    };
    let starts_alphanumeric = |part: &str| {
        part.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
    };
    if !starts_alphanumeric(owner) || !starts_alphanumeric(repo) || repo.contains('/') {
        return None;
    }
    Some(format!("{owner}/{repo}"))
}

/// dotagents-lib's `normalizeSource` (over `parseSource`): GitHub and GitLab
/// URLs and `owner/repo@ref` shorthand become `owner/repo`, other https URLs
/// get a lowercase host and no trailing slash, and `git:`, `path:` and
/// anything it cannot parse stay as written.
fn normalize_source(source: &str) -> String {
    if source.starts_with("path:") || source.starts_with("git:") {
        return source.to_string();
    }
    let lower = source.to_ascii_lowercase();
    let hosts: [(&[&str], bool); 2] = [
        (
            &[
                "https://github.com/",
                "http://github.com/",
                "git@github.com:",
            ],
            false,
        ),
        (
            &[
                "https://gitlab.com/",
                "http://gitlab.com/",
                "git@gitlab.com:",
            ],
            true,
        ),
    ];
    for (prefixes, nested_groups) in hosts {
        for prefix in prefixes {
            if lower.starts_with(prefix) {
                if let Some(repo) = hosted_owner_repo(&source[prefix.len()..], nested_groups) {
                    return repo;
                }
            }
        }
    }
    if lower.starts_with("https://") {
        let rest = &source["https://".len()..];
        let host_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let (host, tail) = rest.split_at(host_end);
        let path = tail.split(['?', '#']).next().unwrap_or_default();
        return format!(
            "https://{}{}",
            host.to_lowercase(),
            path.trim_end_matches('/')
        );
    }
    let shorthand = source.strip_prefix('@').unwrap_or(source);
    let base = match shorthand.split_once('@') {
        Some((_, "")) => return source.to_string(),
        Some((base, _ref)) => base,
        None => shorthand,
    };
    match base.split('/').collect::<Vec<_>>().as_slice() {
        [owner, repo] if !owner.is_empty() && !repo.is_empty() => format!("{owner}/{repo}"),
        _ => source.to_string(),
    }
}

fn sources_match(a: &str, b: &str) -> bool {
    normalize_source(a) == normalize_source(b)
}

/// `posix.normalize` plus the backslash and trailing-slash handling dotagents
/// applies to a wildcard entry's `path`.
fn normalize_path(path: &str) -> String {
    let path = path.replace('\\', "/");
    let absolute = path.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." if parts.last().is_some_and(|last| *last != "..") => {
                parts.pop();
            }
            ".." if absolute => {}
            _ => parts.push(part),
        }
    }
    let joined = parts.join("/");
    match (absolute, joined.is_empty()) {
        (true, _) => format!("/{joined}"),
        (false, true) => ".".to_string(),
        (false, false) => joined,
    }
}

/// dotagents' `wildcardContainsLockedSkill`: whether the `name = "*"` entry
/// `row` supplies the locked skill `name`. Lock rows without a
/// `resolved_path` count as supplied until `install` refreshes them.
fn wildcard_contains(row: &toml_edit::Table, name: &str, locked: &Locked) -> bool {
    if row_name(row) != Some("*") || excludes(row, name) {
        return false;
    }
    let Some(source) = row_str(row, "source") else {
        return false;
    };
    if !sources_match(source, &locked.source) {
        return false;
    }
    let wildcard_path = row_str(row, "path").filter(|path| !path.is_empty());
    let resolved = locked
        .resolved_path
        .as_deref()
        .filter(|path| !path.is_empty());
    let (Some(wildcard_path), Some(resolved)) = (wildcard_path, resolved) else {
        return true;
    };
    let path = normalize_path(wildcard_path);
    path == "." || resolved == path || resolved.starts_with(&format!("{path}/"))
}

/// A wildcard entry's `path` as dotagents compares it; no path equals `.`.
fn comparable_path(path: Option<&str>) -> String {
    normalize_path(path.unwrap_or_default())
}

/// Whether `row` is one of the recorded wildcard entries.
fn is_recorded(row: &toml_edit::Table, wildcards: &[WildcardRef]) -> bool {
    row_name(row) == Some("*")
        && row_str(row, "source").is_some_and(|source| {
            wildcards.iter().any(|w| {
                sources_match(&w.source, source)
                    && comparable_path(w.path.as_deref()) == comparable_path(row_str(row, "path"))
            })
        })
}

/// Adds `name` to the `exclude` list of `row`, creating the list when the
/// entry has none. A multi-line list keeps its layout: the new item takes the
/// indent of the last one, and a comment after the last item's comma stays
/// on that line.
fn add_exclude(row: &mut toml_edit::Table, name: &str) {
    if row.get("exclude").is_none() {
        row["exclude"] = toml_edit::value(toml_edit::Array::new());
    }
    let Some(exclude) = row
        .get_mut("exclude")
        .and_then(toml_edit::Item::as_array_mut)
    else {
        return;
    };
    let layout = exclude.to_string();
    if !layout.contains('\n') {
        exclude.push(name);
        return;
    }
    let newline = if layout.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let Some(last) = exclude.len().checked_sub(1) else {
        exclude.push(name);
        return;
    };
    let indent = exclude
        .get(last)
        .and_then(|item| item.decor().prefix())
        .and_then(toml_edit::RawString::as_str)
        .and_then(|prefix| {
            prefix
                .rsplit_once('\n')
                .map(|(_, indent)| indent.to_string())
        })
        .unwrap_or_default();
    let trailing = exclude.trailing().as_str().unwrap_or_default().to_string();
    let last_suffix = exclude
        .get(last)
        .and_then(|item| item.decor().suffix().cloned());
    exclude.push(name);
    if let Some(at) = trailing.find('\n') {
        let cut = if trailing[..at].ends_with('\r') {
            at - 1
        } else {
            at
        };
        if let Some(added) = exclude.get_mut(last + 1) {
            added
                .decor_mut()
                .set_prefix(format!("{}{newline}{indent}", &trailing[..cut]));
        }
        exclude.set_trailing(trailing[cut..].to_string());
        exclude.set_trailing_comma(true);
    } else {
        // No trailing comma: the closing bracket's line break sits after
        // the last item and moves to the new one.
        if let Some(added) = exclude.get_mut(last + 1) {
            added.decor_mut().set_prefix(format!("{newline}{indent}"));
            if let Some(suffix) = last_suffix {
                added.decor_mut().set_suffix(suffix);
            }
        }
        if let Some(item) = exclude.get_mut(last) {
            item.decor_mut().set_suffix("");
        }
    }
}

fn lock_table<'a>(doc: &'a toml_edit::DocumentMut, name: &str) -> Option<&'a toml_edit::Table> {
    doc.get("skills")
        .and_then(toml_edit::Item::as_table)
        .and_then(|skills| skills.get(name))
        .and_then(toml_edit::Item::as_table)
}

fn lock_table_text(doc: &toml_edit::DocumentMut, name: &str) -> Option<String> {
    lock_table(doc, name).map(ToString::to_string)
}

fn locked_from(doc: &toml_edit::DocumentMut, name: &str) -> Option<Locked> {
    let table = lock_table(doc, name)?;
    Some(Locked {
        source: row_str(table, "source")?.to_string(),
        resolved_path: row_str(table, "resolved_path").map(str::to_string),
    })
}

/// Refuses a project whose git root is an ancestor: dotagents reads
/// `<git root>/agents.toml`, not the file in the project folder.
fn refuse_nested_project(
    rt: &Runtime,
    scope: &RootScope,
    skill: &SkillName,
) -> Result<(), CoreError> {
    let RootScope::Project(project) = scope else {
        return Ok(());
    };
    let fs = rt.ports.fs.as_ref();
    // A linked project folder sits in the repository its target does.
    let start = fs
        .canonicalize(&project.0)
        .unwrap_or_else(|_| project.0.clone());
    let git_root = start
        .ancestors()
        .find(|dir| fs.symlink_metadata(&dir.join(".git")).is_ok());
    match git_root {
        Some(root) if root != start => Err(CoreError::new(
            ErrorCode::Unsupported,
            format!(
                "dotagents reads agents.toml from the git root of a project, which is {}, not {}, so it cannot remove {} from here. Park it from a project opened at {}.",
                root.display(),
                start.display(),
                skill.0,
                root.display()
            ),
        )
        .at(&project.0)),
        _ => Ok(()),
    }
}

/// Decides whether parking `deployment` needs `dotagents remove`.
///
/// `None` when dotagents does not list the skill (so `install` would not
/// bring it back). Refuses when it does list the skill but neither
/// `dotagents` nor `npx` is on `PATH`: parking would leave a copy the next
/// `dotagents install` brings back. Also refuses a project that is not its
/// own git root.
pub(crate) fn plan_park(
    rt: &Runtime,
    deployment: &DeploymentDto,
    skill: &SkillName,
) -> Result<Option<DotagentsPark>, CoreError> {
    if !matches!(
        deployment.owner_kind,
        LifecycleOwnerKind::Dotagents | LifecycleOwnerKind::WildcardDotagents
    ) {
        return Ok(None);
    }
    let fs = rt.ports.fs.as_ref();
    let dir = manifest_dir(rt, &deployment.root.scope);
    let config = crate::ports::resolve_config_link(fs, &dir.join("agents.toml"))?;
    let Some((original_config, doc)) = read_manifest(fs, &config)? else {
        return Ok(None);
    };
    let lock = crate::ports::resolve_config_link(fs, &dir.join("agents.lock"))?;
    let original_lock = read_text(fs, &lock)?;
    let lock_doc = match &original_lock {
        Some(text) => Some(parse(&lock, text)?),
        None => None,
    };
    let lock_entry = lock_doc
        .as_ref()
        .and_then(|doc| lock_table_text(doc, &skill.0));
    let locked = lock_doc.as_ref().and_then(|doc| locked_from(doc, &skill.0));
    let entry = rows(&doc)
        .find(|row| row_name(row) == Some(skill.0.as_str()))
        .map(ToString::to_string);
    let wildcards: Vec<WildcardRef> = match &locked {
        Some(locked) => rows(&doc)
            .filter(|row| wildcard_contains(row, &skill.0, locked))
            .filter_map(|row| {
                Some(WildcardRef {
                    source: row_str(row, "source")?.to_string(),
                    path: row_str(row, "path").map(str::to_string),
                })
            })
            .collect(),
        None => Vec::new(),
    };
    if entry.is_none() && wildcards.is_empty() {
        return Ok(None);
    }
    refuse_nested_project(rt, &deployment.root.scope, skill)?;
    let tools = rt.ports.tools.as_ref();
    let (program, prefix_args) = tools
        .and_then(|tools| tools.find_binary("dotagents"))
        .map(|program| (program, Vec::new()))
        .or_else(|| {
            tools
                .and_then(|tools| tools.find_binary("npx"))
                .map(|program| {
                    (
                        program,
                        vec!["-y".to_string(), "@sentry/dotagents".to_string()],
                    )
                })
        })
        .ok_or_else(|| {
            CoreError::new(
                ErrorCode::Unsupported,
                format!(
                    "dotagents manages {} but is not installed here (neither `dotagents` nor `npx` is on PATH), so the next `dotagents install` would bring the copy back. Install dotagents, then park it again.",
                    skill.0
                ),
            )
            .at(&deployment.path)
        })?;
    Ok(Some(DotagentsPark {
        program,
        prefix_args,
        originals: vec![
            FileSnapshot {
                path: config.clone(),
                text: Some(original_config),
            },
            FileSnapshot {
                path: lock.clone(),
                text: original_lock,
            },
        ],
        lock,
        config,
        entry,
        lock_entry,
        locked,
        wildcards,
    }))
}

/// Runs `dotagents remove <name> -y` for the scope the copy came from, then
/// checks `agents.toml` really changed: dotagents exits 0 without a change
/// when it has to ask a question and has no terminal.
pub(crate) fn run_remove(
    rt: &Runtime,
    ctx: &OpContext,
    guard: &ExclusiveGuard,
    plan: &DotagentsPark,
    skill: &SkillName,
    scope: &RootScope,
) -> Result<(), CoreError> {
    let spawner = rt.ports.spawner.as_ref().ok_or_else(|| {
        CoreError::new(
            ErrorCode::Unsupported,
            "this host build has no process spawner; parking a dotagents skill is not available",
        )
    })?;
    let home = rt.scope.home.lexical.clone();
    let mut args = plan.prefix_args.clone();
    // With `--project` dotagents never reads `DOTAGENTS_HOME`, but one in the
    // environment must not leak in (an empty value removes the variable). A
    // global run needs it, or dotagents may pick another user-scope folder.
    let (cwd, dotagents_home) = match scope {
        RootScope::Global => (home.clone(), home.join(".agents").display().to_string()),
        RootScope::Project(project) => {
            args.push("--project".to_string());
            (project.0.clone(), String::new())
        }
    };
    args.extend(["remove".to_string(), skill.0.clone(), "-y".to_string()]);
    let spec = ProcessSpec {
        program: plan.program.display().to_string(),
        args,
        cwd: Some(cwd),
        env: vec![
            ("HOME".to_string(), home.display().to_string()),
            ("DOTAGENTS_HOME".to_string(), dotagents_home),
        ],
        timeout_ms: 120_000,
    };
    let output = spawner.run(&spec, ctx.cancel.as_ref())?;
    if output.status != Some(0) {
        return Err(crate::ops_update::cli_failure("dotagents remove", &output));
    }
    verify_removed(rt, guard, plan, skill)
}

/// Whether `dotagents install` would no longer install the skill from `doc`.
fn is_removed(doc: &toml_edit::DocumentMut, plan: &DotagentsPark, skill: &SkillName) -> bool {
    let supplied = plan
        .locked
        .as_ref()
        .is_some_and(|locked| rows(doc).any(|row| wildcard_contains(row, &skill.0, locked)));
    !lists_explicitly(doc, &skill.0) && !supplied
}

fn verify_removed(
    rt: &Runtime,
    guard: &ExclusiveGuard,
    plan: &DotagentsPark,
    skill: &SkillName,
) -> Result<(), CoreError> {
    let text = read_text(rt.ports.fs.as_ref(), &plan.config)?.unwrap_or_default();
    let doc = parse(&plan.config, &text)?;
    if is_removed(&doc, plan, skill) {
        return Ok(());
    }
    if !lists_explicitly(&doc, &skill.0) && exclude_where_supplied(rt, guard, plan, skill, doc)? {
        return Ok(());
    }
    Err(CoreError::new(
        ErrorCode::Io,
        format!(
            "dotagents ran but agents.toml still lists {}, so the next `dotagents install` would bring the copy back",
            skill.0
        ),
    )
    .at(&plan.config))
}

/// `dotagents remove` does not always leave the skill unlisted:
/// - the skill had its own entry and a `*` entry from the same source, and
///   `remove` drops only the own entry, so `install` brings the skill back
///   through the `*` entry;
/// - its `exclude` list spans several lines, or the file has CRLF line ends,
///   and dotagents' line-based edit changes nothing;
/// - dotagents excluded the first `*` entry of that source, which is not the
///   one that supplies the skill (a different `path`).
///
/// Adds the name to the `exclude` of the `*` entries that still supply the
/// skill (every one when the skill had its own entry, the first otherwise,
/// as `remove` would), and drops the lock row if dotagents left it. Writes
/// nothing and returns `false` when that would not unlist the skill.
fn exclude_where_supplied(
    rt: &Runtime,
    guard: &ExclusiveGuard,
    plan: &DotagentsPark,
    skill: &SkillName,
    mut doc: toml_edit::DocumentMut,
) -> Result<bool, CoreError> {
    let Some(locked) = &plan.locked else {
        return Ok(false);
    };
    let supplying: Vec<usize> = rows(&doc)
        .enumerate()
        .filter(|(_, row)| wildcard_contains(row, &skill.0, locked))
        .map(|(index, _)| index)
        .collect();
    let take = if plan.entry.is_some() {
        supplying.len()
    } else {
        1
    };
    if let Some(list) = doc
        .get_mut("skills")
        .and_then(toml_edit::Item::as_array_of_tables_mut)
    {
        for (index, row) in list.iter_mut().enumerate() {
            if supplying.iter().take(take).any(|i| *i == index) {
                add_exclude(row, &skill.0);
            }
        }
    }
    if !is_removed(&doc, plan, skill) {
        return Ok(false);
    }
    write_text(rt, guard, &plan.config, &doc.to_string())?;
    drop_lock_entry(rt, guard, plan, skill)?;
    Ok(true)
}

fn drop_lock_entry(
    rt: &Runtime,
    guard: &ExclusiveGuard,
    plan: &DotagentsPark,
    skill: &SkillName,
) -> Result<(), CoreError> {
    let Some(text) = read_text(rt.ports.fs.as_ref(), &plan.lock)? else {
        return Ok(());
    };
    let mut doc = parse(&plan.lock, &text)?;
    let removed = doc
        .get_mut("skills")
        .and_then(toml_edit::Item::as_table_mut)
        .and_then(|skills| skills.remove(&skill.0))
        .is_some();
    if removed {
        write_text(rt, guard, &plan.lock, &doc.to_string())?;
    }
    Ok(())
}

/// Writes `agents.toml` and `agents.lock` back as `plan_park` read them.
pub(crate) fn restore_originals(
    rt: &Runtime,
    guard: &ExclusiveGuard,
    plan: &DotagentsPark,
) -> Result<(), CoreError> {
    restore_files(rt, guard, &plan.originals)
}

/// Writes every snapshot back and reports the first failure after trying all.
pub(crate) fn restore_files(
    rt: &Runtime,
    guard: &ExclusiveGuard,
    files: &[FileSnapshot],
) -> Result<(), CoreError> {
    let fs = rt.ports.fs.as_ref();
    let mut first_error = None;
    for file in files {
        let result =
            crate::ports::confine_write_through(&rt.scope, fs, &file.path).and_then(|scoped| {
                match &file.text {
                    Some(text) => fs.write_atomic(guard, &scoped, text.as_bytes()),
                    None => fs.remove_file(guard, &scoped),
                }
                .map_err(|e| CoreError::io(&file.path, e))
            });
        if let Err(e) = result {
            first_error.get_or_insert(e);
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn write_text(
    rt: &Runtime,
    guard: &ExclusiveGuard,
    path: &Path,
    text: &str,
) -> Result<(), CoreError> {
    let fs = rt.ports.fs.as_ref();
    let scoped = crate::ports::confine_write_through(&rt.scope, fs, path)?;
    fs.write_atomic(guard, &scoped, text.as_bytes())
        .map_err(|e| CoreError::io(path, e))
}

/// `skill` listed again in `doc`: the recorded `[[skills]]` entry added back,
/// and the name lifted from the `exclude` of the recorded wildcard
/// entries. Reports whether anything changed.
fn with_skill_back(
    doc: &mut toml_edit::DocumentMut,
    skill: &SkillName,
    entry: Option<&str>,
    wildcards: &[WildcardRef],
) -> Result<bool, CoreError> {
    let mut changed = false;
    if let Some(entry) = entry {
        if !lists_explicitly(doc, &skill.0) {
            let table = parse(Path::new("agents.toml"), entry)
                .map_err(|e| {
                    CoreError::new(
                        ErrorCode::Io,
                        format!("the recorded agents.toml entry is not valid: {}", e.message),
                    )
                })?
                .as_table()
                .clone();
            if doc.get("skills").is_none() {
                doc["skills"] = toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new());
            }
            if let Some(list) = doc
                .get_mut("skills")
                .and_then(toml_edit::Item::as_array_of_tables_mut)
            {
                list.push(table);
                changed = true;
            }
        }
    }
    if let Some(list) = doc
        .get_mut("skills")
        .and_then(toml_edit::Item::as_array_of_tables_mut)
    {
        for row in list.iter_mut() {
            if !is_recorded(row, wildcards) {
                continue;
            }
            let Some(exclude) = row
                .get_mut("exclude")
                .and_then(toml_edit::Item::as_array_mut)
            else {
                continue;
            };
            let before = exclude.len();
            exclude.retain(|item| item.as_str() != Some(skill.0.as_str()));
            if exclude.len() != before {
                changed = true;
                if exclude.is_empty() {
                    row.remove("exclude");
                }
            }
        }
    }
    Ok(changed)
}

/// Whether `dotagents install` would install `skill` from `doc`: an entry of
/// that name, or a recorded wildcard entry that does not exclude it.
fn listed_again(
    doc: &toml_edit::DocumentMut,
    skill: &SkillName,
    wildcards: &[WildcardRef],
) -> bool {
    lists_explicitly(doc, &skill.0)
        || rows(doc).any(|row| is_recorded(row, wildcards) && !excludes(row, &skill.0))
}

/// The `agents.lock` text with the skill's table back, or `None` when it is
/// there already.
fn with_lock_entry(
    path: &Path,
    text: &str,
    skill: &SkillName,
    lock_entry: &str,
) -> Result<Option<String>, CoreError> {
    let mut doc = parse(path, text)?;
    if lock_table_text(&doc, &skill.0).is_some() {
        return Ok(None);
    }
    let table = parse(path, lock_entry)?.as_table().clone();
    if doc.get("skills").is_none() {
        let mut skills = toml_edit::Table::new();
        skills.set_implicit(true);
        doc["skills"] = toml_edit::Item::Table(skills);
    }
    let Some(skills) = doc
        .get_mut("skills")
        .and_then(toml_edit::Item::as_table_mut)
    else {
        return Ok(None);
    };
    skills.insert(&skill.0, toml_edit::Item::Table(table));
    Ok(Some(doc.to_string()))
}

/// Lists `skill` again in `agents.toml` and `agents.lock`. It runs before the
/// folder moves back, so a failure here leaves the copy parked and the
/// turn-on can be tried again. Returns what it changed, for [`restore_files`]
/// if the move then fails; on an error it has already written back its own
/// edits.
pub(crate) fn turn_on(
    rt: &Runtime,
    guard: &ExclusiveGuard,
    recorded: &RecordedDotagents,
    skill: &SkillName,
) -> Result<Vec<FileSnapshot>, CoreError> {
    let mut changed = Vec::new();
    match turn_on_files(rt, guard, recorded, skill, &mut changed) {
        Ok(()) => Ok(changed),
        Err(e) => {
            let _ = restore_files(rt, guard, &changed);
            Err(e)
        }
    }
}

fn turn_on_files(
    rt: &Runtime,
    guard: &ExclusiveGuard,
    recorded: &RecordedDotagents,
    skill: &SkillName,
    changed: &mut Vec<FileSnapshot>,
) -> Result<(), CoreError> {
    let fs = rt.ports.fs.as_ref();
    if let Some((before, mut doc)) = read_manifest(fs, &recorded.config)? {
        let edited = with_skill_back(
            &mut doc,
            skill,
            recorded.entry.as_deref(),
            &recorded.wildcards,
        )?;
        let has_record = recorded.entry.is_some() || !recorded.wildcards.is_empty();
        // Nothing is written yet: a turn-on that would not make dotagents
        // manage the skill again must leave the copy parked and the files alone.
        if has_record && !listed_again(&doc, skill, &recorded.wildcards) {
            return Err(CoreError::new(
                ErrorCode::InvalidRequest,
                format!(
                    "agents.toml no longer has an entry that supplies {}, so dotagents would not manage it after turn-on. Add its entry back to agents.toml, then turn it on.",
                    skill.0
                ),
            )
            .at(&recorded.config));
        }
        if edited {
            write_text(rt, guard, &recorded.config, &doc.to_string())?;
            changed.push(FileSnapshot {
                path: recorded.config.clone(),
                text: Some(before),
            });
        }
    }
    // A missing `agents.lock` stays missing: a file without `version = 1`
    // breaks every dotagents command, and dotagents writes its own.
    if let (Some(lock), Some(lock_entry)) = (&recorded.lock, &recorded.lock_entry) {
        if let Some(before) = read_text(fs, lock)? {
            if let Some(text) = with_lock_entry(lock, &before, skill, lock_entry)? {
                write_text(rt, guard, lock, &text)?;
                changed.push(FileSnapshot {
                    path: lock.clone(),
                    text: Some(before),
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Flow: a source in `agents.toml` and the same source in `agents.lock`
    /// are written in different shapes. Expectation: they match exactly when
    /// dotagents-lib's `normalizeSource` makes them equal. Failure: a `*`
    /// entry is not found for its skill, or a different source is taken for it.
    #[test]
    fn sources_match_as_dotagents_normalizes_them() {
        let same = [
            ("owner/repo", "https://github.com/owner/repo"),
            ("owner/repo", "https://github.com/owner/repo.git"),
            ("owner/repo", "http://github.com/owner/repo/"),
            ("owner/repo", "https://github.com/owner/repo@v1"),
            ("owner/repo", "git@github.com:owner/repo.git"),
            ("owner/repo", "owner/repo@abc123"),
            ("owner/repo", "@owner/repo"),
            ("group/sub/repo", "https://gitlab.com/group/sub/repo.git"),
            ("https://x.dev/a", "https://X.dev/a/"),
        ];
        for (a, b) in same {
            assert!(sources_match(a, b), "{a} should match {b}");
        }
        let different = [
            ("owner/repo", "github:owner/repo"),
            ("owner/repo", "ssh://git@github.com/owner/repo"),
            ("owner/repo", "https://www.github.com/owner/repo"),
            ("owner/repo", "owner/other"),
            ("git:https://x.dev/r.git", "https://x.dev/r"),
            ("path:../skills", "../skills"),
            ("owner/repo", "owner/repo/nested"),
        ];
        for (a, b) in different {
            assert!(!sources_match(a, b), "{a} should not match {b}");
        }
    }

    fn table(text: &str) -> toml_edit::DocumentMut {
        text.parse().unwrap()
    }

    /// Flow: a recorded `*` entry is found again after the person rewrote its
    /// `path`. Expectation: paths compare as dotagents compares them
    /// (backslashes, `./`, trailing slash, no path equal to `.`). Failure: a
    /// turn-on skips the entry it must lift the exclude from.
    #[test]
    fn recorded_entries_match_through_path_normalization() {
        let recorded = |path: Option<&str>| {
            vec![WildcardRef {
                source: "owner/repo".to_string(),
                path: path.map(str::to_string),
            }]
        };
        let doc = table(
            "name = \"*\"\nsource = \"https://github.com/owner/repo\"\npath = \"./skills/\"\n",
        );
        assert!(is_recorded(doc.as_table(), &recorded(Some("skills"))));
        assert!(is_recorded(doc.as_table(), &recorded(Some("skills\\"))));
        assert!(!is_recorded(doc.as_table(), &recorded(Some("other"))));
        assert!(!is_recorded(doc.as_table(), &recorded(None)));
        let doc = table("name = \"*\"\nsource = \"owner/repo\"\n");
        assert!(is_recorded(doc.as_table(), &recorded(Some("."))));
        assert!(is_recorded(doc.as_table(), &recorded(None)));
    }

    fn exclude_after_add(text: &str) -> String {
        let mut doc = table(text);
        add_exclude(doc.as_table_mut(), "foo");
        doc.to_string()
    }

    /// Flow: `foo` is added to an `exclude` list. Expectation: a missing list
    /// is created, a one-line list grows on its line, and a multi-line list
    /// keeps its layout: a comment above the last item is not copied, and a
    /// comment after the last item's comma stays on that item's line.
    /// Failure: the person's comments move or double.
    #[test]
    fn add_exclude_keeps_the_layout_of_the_list() {
        assert_eq!(
            exclude_after_add("name = \"*\"\n"),
            "name = \"*\"\nexclude = [\"foo\"]\n"
        );
        assert_eq!(
            exclude_after_add("exclude = [\"a\"]\n"),
            "exclude = [\"a\", \"foo\"]\n"
        );
        assert_eq!(
            exclude_after_add("exclude = [\n  \"a\",\n  # note\n  \"baz\", # tail\n]\n"),
            "exclude = [\n  \"a\",\n  # note\n  \"baz\", # tail\n  \"foo\",\n]\n"
        );
        assert_eq!(
            exclude_after_add("exclude = [\n  \"baz\"\n]\n"),
            "exclude = [\n  \"baz\",\n  \"foo\"\n]\n"
        );
        assert_eq!(
            exclude_after_add("exclude = [\r\n  \"baz\",\r\n]\r\n"),
            // toml_edit reads CRLF files as LF, so the edit writes LF.
            "exclude = [\n  \"baz\",\n  \"foo\",\n]\n"
        );
    }
}
