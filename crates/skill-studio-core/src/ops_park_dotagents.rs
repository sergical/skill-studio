//! Install-aware Park edits dotagents bookkeeping without running its
//! destructive remove command. Turn on restores untouched files byte for
//! byte and merges the skill's entries when other content changed.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use crate::dto::DeploymentDto;
use crate::error::{CoreError, ErrorCode};
use crate::identity::{LifecycleOwnerKind, RootScope, SkillName};
use crate::ports::{ExclusiveGuard, OpContext, Runtime, ScopeFs};

/// A file's text before an edit, to write back when a later step fails.
/// `None` text means the file did not exist.
pub(crate) struct FileSnapshot {
    path: PathBuf,
    text: Option<String>,
}

/// A file write and the exact output required for its safe rollback.
pub(crate) struct FileEdit {
    before: FileSnapshot,
    after: String,
}

/// A `name = "*"` entry that supplied the skill, found again by what it
/// says rather than by position: entries move when the person edits the file.
#[derive(Clone)]
struct WildcardRef {
    source: String,
    path: Option<String>,
    /// Whether the entry supplied the skill. The other recorded entry is the
    /// one `dotagents remove` excludes; turn-on lifts it but does not count it
    /// as a reason dotagents would manage the skill.
    supplies: bool,
    /// Whether the entry had an `exclude` key before the park. Turn-on keeps
    /// that key, and the line and comment it carried, when the list ends up
    /// empty.
    had_exclude: bool,
}

/// What `agents.lock` records about the skill: the fields dotagents matches
/// wildcard entries on.
struct Locked {
    source: String,
    resolved_path: Option<String>,
}

/// What Park must undo in dotagents' files for one skill.
pub(crate) struct DotagentsPark {
    changes: RefCell<Vec<FileEdit>>,
    /// `agents.toml`, or the file its link resolves to.
    pub(crate) config: PathBuf,
    /// `agents.lock`, or the file its link resolves to.
    pub(crate) lock: PathBuf,
    /// `.agents/.gitignore`: `remove` deletes the `/skills/<name>` line from it.
    gitignore: PathBuf,
    /// The files as read, written back when `dotagents remove` fails.
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
    pub(crate) gitignore: Option<PathBuf>,
    /// The files as the park left them. `None` for a row from before this
    /// was recorded; turn-on then edits the files.
    after: Option<PostPark>,
    entry: Option<String>,
    lock_entry: Option<String>,
    wildcards: Vec<WildcardRef>,
}

/// A content hash of each file after `dotagents remove` ran, or `ABSENT`.
/// A file that still has its hash at turn-on was touched by nothing else, so
/// the backed-up original goes back byte for byte.
struct PostPark {
    config: String,
    lock: String,
    gitignore: String,
}

const ABSENT: &str = "absent";

fn content_hash(text: Option<&str>) -> String {
    text.map_or_else(
        || ABSENT.to_string(),
        |text| {
            crate::identity::Fingerprint::of_bytes(text.as_bytes())
                .bare_hex()
                .to_string()
        },
    )
}

impl DotagentsPark {
    /// The files the park row backs up: `.gitignore` only when it exists.
    pub(crate) fn backup_paths(&self) -> Vec<PathBuf> {
        self.originals
            .iter()
            .filter(|file| file.text.is_some())
            .map(|file| file.path.clone())
            .collect()
    }

    /// The `payload.dotagents_after` value: what the three files hold once
    /// Park has written them. `None` when the recorded input or output differs
    /// from the backup and current bytes, so Turn on must merge.
    pub(crate) fn payload_after(
        &self,
        rt: &Runtime,
        _skill: &SkillName,
    ) -> Option<serde_json::Value> {
        let fs = rt.ports.fs.as_ref();
        let changes = self.changes.borrow();
        let hash = |path: &Path| {
            let original = self.originals.iter().find(|file| file.path == path)?;
            let change = changes.iter().find(|edit| edit.before.path == path);
            let expected = match change {
                Some(edit) if edit.before.text == original.text => Some(edit.after.as_str()),
                Some(_) => return None,
                None => original.text.as_deref(),
            };
            let current = read_text(fs, path).ok()?;
            (current.as_deref() == expected).then(|| content_hash(expected))
        };
        Some(serde_json::json!({
            "config": hash(&self.config)?,
            "lock": hash(&self.lock)?,
            "gitignore": hash(&self.gitignore)?,
        }))
    }

    /// The `payload.dotagents` value the park row records.
    pub(crate) fn payload(&self) -> serde_json::Value {
        let wildcards: Vec<serde_json::Value> = self
            .wildcards
            .iter()
            .map(|w| {
                serde_json::json!({
                    "source": w.source,
                    "path": w.path,
                    "supplies": w.supplies,
                    "had_exclude": w.had_exclude,
                })
            })
            .collect();
        serde_json::json!({
            "config": self.config,
            "lock": self.lock,
            "gitignore": self.gitignore,
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
        gitignore: text("gitignore").map(PathBuf::from),
        after: payload.get("dotagents_after").and_then(|after| {
            let hash = |key: &str| Some(after.get(key)?.as_str()?.to_string());
            Some(PostPark {
                config: hash("config")?,
                lock: hash("lock")?,
                gitignore: hash("gitignore")?,
            })
        }),
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
                            // Older payloads recorded only entries that supply.
                            supplies: row
                                .get("supplies")
                                .and_then(serde_json::Value::as_bool)
                                .unwrap_or(true),
                            had_exclude: row
                                .get("had_exclude")
                                .and_then(serde_json::Value::as_bool)
                                .unwrap_or(false),
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
fn hosted_owner_repo(rest: &str, nested_groups: bool, trailing_slash: bool) -> Option<String> {
    let base = match rest.split_once('@') {
        Some((_, "")) => return None,
        Some((base, _ref)) => base,
        None => rest,
    };
    let base = if trailing_slash {
        base.strip_suffix('/').unwrap_or(base)
    } else {
        base
    };
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
    // The GitHub and GitLab patterns are case-sensitive; only the generic
    // `https://` check below ignores case. SSH sources take no trailing `/`.
    let hosts: [(&str, bool, bool); 6] = [
        ("https://github.com/", false, true),
        ("http://github.com/", false, true),
        ("git@github.com:", false, false),
        ("https://gitlab.com/", true, true),
        ("http://gitlab.com/", true, true),
        ("git@gitlab.com:", true, false),
    ];
    for (prefix, nested_groups, trailing_slash) in hosts {
        if let Some(rest) = source.strip_prefix(prefix) {
            if let Some(repo) = hosted_owner_repo(rest, nested_groups, trailing_slash) {
                return repo;
            }
        }
    }
    if source.to_ascii_lowercase().starts_with("https://") {
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

/// Whether the recorded entry that matches `row` had an `exclude` key before
/// the park.
fn had_exclude(row: &toml_edit::Table, wildcards: &[WildcardRef]) -> bool {
    row_str(row, "source").is_some_and(|source| {
        wildcards.iter().any(|w| {
            w.had_exclude
                && sources_match(&w.source, source)
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
        if let Some(added) = exclude.get_mut(last + 1) {
            added
                .decor_mut()
                .set_prefix(format!("{}\n{indent}", &trailing[..at]));
        }
        exclude.set_trailing(trailing[at..].to_string());
        exclude.set_trailing_comma(true);
    } else {
        // No trailing comma: a comment after the last item stays on that
        // item's line, and the rest of its suffix (the line break before `]`
        // and any comment lines) moves to the new item.
        let last_suffix = last_suffix
            .and_then(|raw| raw.as_str().map(str::to_string))
            .unwrap_or_default();
        let (comment, rest) = last_suffix
            .find('\n')
            .map_or((last_suffix.as_str(), ""), |at| last_suffix.split_at(at));
        if let Some(added) = exclude.get_mut(last + 1) {
            added.decor_mut().set_prefix(format!("{comment}\n{indent}"));
            added
                .decor_mut()
                .set_suffix(if rest.is_empty() { "\n" } else { rest });
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
/// `None` when dotagents does not list the skill. Refuses a project that
/// is not its own git root, where dotagents would use a different manifest.
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
                    supplies: true,
                    had_exclude: row.get("exclude").is_some(),
                })
            })
            .collect(),
        None => Vec::new(),
    };
    if entry.is_none() && wildcards.is_empty() {
        return Ok(None);
    }
    refuse_nested_project(rt, &deployment.root.scope, skill)?;
    let gitignore = crate::ports::resolve_config_link(
        fs,
        &match &deployment.root.scope {
            RootScope::Global => dir.join(".gitignore"),
            RootScope::Project(_) => dir.join(".agents").join(".gitignore"),
        },
    )?;
    // Turn on writes this file back through `confine_write_through`; a link
    // that leaves the home and projects would park the skill for good.
    crate::ports::confine(&rt.scope, fs, &gitignore).map_err(|e| {
        CoreError::new(
            e.code,
            format!(
                "dotagents' ignore file {} resolves outside the home and the configured projects, so the skill could not be turned on again: {}",
                gitignore.display(),
                e.message
            ),
        )
        .at(&deployment.path)
    })?;
    let mut originals = vec![
        FileSnapshot {
            path: config.clone(),
            text: Some(original_config),
        },
        FileSnapshot {
            path: lock.clone(),
            text: original_lock,
        },
    ];
    originals.push(FileSnapshot {
        path: gitignore.clone(),
        text: read_text(fs, &gitignore)?,
    });
    Ok(Some(DotagentsPark {
        changes: RefCell::new(Vec::new()),
        config,
        lock,
        gitignore,
        originals,
        entry,
        lock_entry,
        locked,
        wildcards,
    }))
}

/// Removes only the skill's bookkeeping entries. Never runs a command that
/// can delete a live skill folder.
pub(crate) fn unlist(
    rt: &Runtime,
    ctx: &OpContext,
    guard: &ExclusiveGuard,
    plan: &DotagentsPark,
    skill: &SkillName,
    _scope: &RootScope,
) -> Result<(), CoreError> {
    ctx.checkpoint()?;
    let fs = rt.ports.fs.as_ref();
    let before = read_text(fs, &plan.config)?;
    let text = before.as_deref().unwrap_or_default();
    let mut doc = parse(&plan.config, text)?;
    let own_rows = |doc: &toml_edit::DocumentMut| {
        rows(doc)
            .filter(|row| {
                row_name(row) == Some(skill.0.as_str()) || is_recorded(row, &plan.wildcards)
            })
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    };
    if let Some(original) = plan
        .originals
        .iter()
        .find(|file| file.path == plan.config)
        .and_then(|file| file.text.as_deref())
    {
        if own_rows(&parse(&plan.config, original)?) != own_rows(&doc) {
            return Err(CoreError::new(
                ErrorCode::DriftConflict,
                "the skill entries changed during Park; scan and try again",
            )
            .at(&plan.config));
        }
    }
    if let Some(list) = doc
        .get_mut("skills")
        .and_then(toml_edit::Item::as_array_of_tables_mut)
    {
        list.retain(|row| row_name(row) != Some(skill.0.as_str()));
        for row in list.iter_mut() {
            if plan
                .locked
                .as_ref()
                .is_some_and(|locked| wildcard_contains(row, &skill.0, locked))
            {
                add_exclude(row, &skill.0);
            }
        }
    }
    if !is_removed(&doc, plan, skill) {
        return Err(CoreError::new(
            ErrorCode::DriftConflict,
            "the skill entries changed; scan and try Park again",
        )
        .at(&plan.config));
    }
    let rendered = render_checked(&plan.config, &doc, text.contains("\r\n"))?;
    write_park_file(rt, guard, plan, &plan.config, before, rendered)?;

    if let Some(text) = read_text(fs, &plan.lock)? {
        let mut doc = parse(&plan.lock, &text)?;
        if lock_table_text(&doc, &skill.0) != plan.lock_entry {
            return Err(CoreError::new(
                ErrorCode::DriftConflict,
                "the skill lock entry changed during Park; scan and try again",
            )
            .at(&plan.lock));
        }
        let removed = doc
            .get_mut("skills")
            .and_then(toml_edit::Item::as_table_mut)
            .and_then(|skills| skills.remove(&skill.0))
            .is_some();
        if removed {
            let rendered = render_checked(&plan.lock, &doc, text.contains("\r\n"))?;
            write_park_file(rt, guard, plan, &plan.lock, Some(text), rendered)?;
        }
    }
    if let Some(text) = read_text(fs, &plan.gitignore)? {
        if !plan
            .originals
            .iter()
            .any(|file| file.path == plan.gitignore && file.text.is_some())
        {
            return Err(CoreError::new(
                ErrorCode::DriftConflict,
                "the ignore file appeared during Park; scan and try again",
            )
            .at(&plan.gitignore));
        }
        let own = [
            format!("/skills/{}", skill.0),
            format!("/skills/{}/", skill.0),
        ];
        let kept: String = text
            .split_inclusive('\n')
            .filter(|line| {
                !own.iter()
                    .any(|own| line.trim_end_matches(['\r', '\n']) == own)
            })
            .collect();
        write_park_file(rt, guard, plan, &plan.gitignore, Some(text), kept)?;
    }
    Ok(())
}

fn write_park_file(
    rt: &Runtime,
    guard: &ExclusiveGuard,
    plan: &DotagentsPark,
    path: &Path,
    before: Option<String>,
    after: String,
) -> Result<(), CoreError> {
    if before.as_deref() == Some(&after) {
        return Ok(());
    }
    if read_text(rt.ports.fs.as_ref(), path)? != before {
        return Err(CoreError::new(
            ErrorCode::DriftConflict,
            "the file changed during Park; scan and try again",
        )
        .at(path));
    }
    write_text(rt, guard, path, before.as_deref(), &after)?;
    plan.changes.borrow_mut().push(FileEdit {
        before: FileSnapshot {
            path: path.to_path_buf(),
            text: before,
        },
        after,
    });
    Ok(())
}

/// Whether `dotagents install` would no longer install the skill from `doc`.
fn is_removed(doc: &toml_edit::DocumentMut, plan: &DotagentsPark, skill: &SkillName) -> bool {
    let supplied = plan
        .locked
        .as_ref()
        .is_some_and(|locked| rows(doc).any(|row| wildcard_contains(row, &skill.0, locked)));
    !lists_explicitly(doc, &skill.0) && !supplied
}

impl DotagentsPark {
    /// The live shared folder for this scope, independent of the ignore file's location.
    pub(crate) fn live_folder(
        rt: &Runtime,
        scope: &RootScope,
        skill: &SkillName,
    ) -> Option<PathBuf> {
        let dir = manifest_dir(rt, scope);
        let agents = match scope {
            RootScope::Global => dir,
            RootScope::Project(_) => dir.join(".agents"),
        };
        let folder = agents.join("skills").join(&skill.0);
        rt.ports.fs.symlink_metadata(&folder).ok().map(|_| folder)
    }
}

/// Rolls back this Park's writes only while their recorded output still matches.
pub(crate) fn restore_originals(
    rt: &Runtime,
    guard: &ExclusiveGuard,
    plan: &DotagentsPark,
) -> Result<(), CoreError> {
    restore_files(rt, guard, &plan.changes.borrow())
}

/// Writes every snapshot back and reports the first failure after trying all.
pub(crate) fn restore_files(
    rt: &Runtime,
    guard: &ExclusiveGuard,
    files: &[FileEdit],
) -> Result<(), CoreError> {
    let fs = rt.ports.fs.as_ref();
    let mut first_error = None;
    for edit in files.iter().rev() {
        let file = &edit.before;
        let result = (|| {
            if read_text(fs, &file.path)?.as_deref() != Some(edit.after.as_str()) {
                return Err(CoreError::new(ErrorCode::DriftConflict,
                    "a file changed after this operation wrote it; its backup is retained and the current file was kept").at(&file.path));
            }
            match &file.text {
                Some(text) => write_text(rt, guard, &file.path, Some(&edit.after), text),
                None => Err(CoreError::new(ErrorCode::DriftConflict,
                    "rollback retained the newly created file and its backup instead of deleting it").at(&file.path)),
            }
        })();
        if let Err(e) = result {
            first_error.get_or_insert(e);
        }
    }
    first_error.map_or(Ok(()), Err)
}

/// `doc` as text. `toml_edit` drops the `\r` of every line break on parse
/// except inside multi-line strings, so a file that had CRLF line endings gets
/// them back on each `\n` that has none.
fn render(doc: &toml_edit::DocumentMut, crlf: bool) -> String {
    let text = doc.to_string();
    if !crlf {
        return text;
    }
    let mut out = String::with_capacity(text.len() + text.len() / 16);
    let mut previous = '\0';
    for c in text.chars() {
        if c == '\n' && previous != '\r' {
            out.push('\r');
        }
        out.push(c);
        previous = c;
    }
    out
}

/// [`render`], refusing text that no longer parses: an edit that breaks the
/// file must fail the operation, not be written.
fn render_checked(
    path: &Path,
    doc: &toml_edit::DocumentMut,
    crlf: bool,
) -> Result<String, CoreError> {
    let text = render(doc, crlf);
    parse(path, &text).map_err(|e| {
        CoreError::new(
            ErrorCode::Io,
            format!(
                "the edited file would not be valid TOML, so nothing was written: {}",
                e.message
            ),
        )
        .at(path)
    })?;
    Ok(text)
}

fn write_text(
    rt: &Runtime,
    guard: &ExclusiveGuard,
    path: &Path,
    expected: Option<&str>,
    text: &str,
) -> Result<(), CoreError> {
    let fs = rt.ports.fs.as_ref();
    let scoped = crate::ports::confine_write_through(&rt.scope, fs, path)?;
    let path = scoped.as_path();
    if expected.is_none() {
        return fs
            .fsops_write_new_file(path, text.as_bytes())
            .map_err(|e| CoreError::io(path, e));
    }
    let displaced = path.with_file_name(format!(
        ".{}-park-recovery-{}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        crate::fsops::unique_suffix()
    ));
    let staged = crate::ports::confine(&rt.scope, fs, &displaced)?;
    let mode = fs
        .symlink_metadata(path)
        .map_err(|e| CoreError::io(path, e))?
        .mode
        .unwrap_or(0o600);
    fs.fsops_write_new_file_with_mode(&displaced, b"", mode)
        .map_err(|e| CoreError::io(&displaced, e))?;
    fs.write_atomic(guard, &staged, text.as_bytes())
        .map_err(|e| CoreError::io(&displaced, e))?;
    fs.fsops_fsync_file(&displaced)
        .map_err(|e| CoreError::io(&displaced, e))?;
    fs.fsops_exchange(&displaced, path)
        .map_err(|e| CoreError::io(path, e))?;
    if let Some(parent) = path.parent() {
        fs.fsops_fsync_dir(parent)
            .map_err(|e| CoreError::io(parent, e))?;
    }
    // Keep the displaced inode even after success: an external writer may
    // still hold an open descriptor and write to it after the exchange.
    if read_text(fs, &displaced)?.as_deref() != expected {
        return Err(CoreError::new(
            ErrorCode::DriftConflict,
            format!(
            "the file changed during replacement; both versions are kept, backup is retained at {}",
            displaced.display()
        ),
        )
        .at(path));
    }
    Ok(())
}

/// `comment` in front of `text`, with a line break between them unless `text`
/// already starts with one (after optional spaces): a comment runs to the end
/// of its line and must not swallow the next item or the closing bracket.
fn comment_before(comment: &str, text: &str) -> String {
    if comment.trim().is_empty()
        || text
            .trim_start_matches([' ', '\t'])
            .starts_with(['\n', '\r'])
    {
        format!("{comment}{text}")
    } else {
        format!("{comment}\n{text}")
    }
}

/// Takes `name` out of `exclude`, undoing [`add_exclude`]: the comment that
/// `add_exclude` moved into the item's prefix goes back to where it was, and
/// when the item was the last of a list with no trailing comma its line break
/// goes back to the new last item. Reports whether the name was there.
fn lift_exclude(exclude: &mut toml_edit::Array, name: &str) -> bool {
    let mut lifted = false;
    loop {
        let found = exclude.iter().position(|item| item.as_str() == Some(name));
        let Some(index) = found else {
            break;
        };
        lifted = true;
        let raw = |text: Option<&toml_edit::RawString>| {
            text.and_then(toml_edit::RawString::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let (prefix, suffix) = exclude
            .get(index)
            .map(|item| (raw(item.decor().prefix()), raw(item.decor().suffix())))
            .unwrap_or_default();
        exclude.remove(index);
        let Some((comment, _)) = prefix.split_once('\n') else {
            continue;
        };
        if let Some(next) = exclude.get_mut(index) {
            let next_prefix = raw(next.decor().prefix());
            next.decor_mut()
                .set_prefix(comment_before(comment, &next_prefix));
        } else if exclude.trailing_comma() {
            let trailing = exclude.trailing().as_str().unwrap_or_default().to_string();
            exclude.set_trailing(comment_before(comment, &trailing));
        } else if let Some(last) = exclude.len().checked_sub(1) {
            if let Some(item) = exclude.get_mut(last) {
                item.decor_mut()
                    .set_suffix(comment_before(comment, &suffix));
            }
        }
    }
    lifted
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
            if lift_exclude(exclude, &skill.0) {
                changed = true;
                if exclude.is_empty() && !had_exclude(row, wildcards) {
                    row.remove("exclude");
                }
            }
        }
    }
    Ok(changed)
}

/// Whether `dotagents install` would install `skill` from `doc`: an entry of
/// that name, or a recorded wildcard entry that supplies it and does not
/// exclude it.
fn listed_again(
    doc: &toml_edit::DocumentMut,
    skill: &SkillName,
    wildcards: &[WildcardRef],
) -> bool {
    let suppliers: Vec<WildcardRef> = wildcards.iter().filter(|w| w.supplies).cloned().collect();
    lists_explicitly(doc, &skill.0)
        || rows(doc).any(|row| is_recorded(row, &suppliers) && !excludes(row, &skill.0))
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
    if skills.is_empty() {
        skills.set_implicit(true);
    }
    skills.insert(&skill.0, toml_edit::Item::Table(table));
    render_checked(path, &doc, text.contains("\r\n")).map(Some)
}

/// Lists `skill` again in `agents.toml`, `agents.lock` and `.gitignore`. It
/// runs before the folder moves back, so a failure here leaves the copy
/// parked and the turn-on can be tried again. `originals` are the files as
/// the park backed them up. Returns what it changed, for [`restore_files`]
/// if the move then fails; on an error it has already written back its own
/// edits.
pub(crate) fn turn_on(
    rt: &Runtime,
    guard: &ExclusiveGuard,
    recorded: &RecordedDotagents,
    originals: &[FileSnapshot],
    skill: &SkillName,
) -> Result<Vec<FileEdit>, CoreError> {
    let mut changed = Vec::new();
    match turn_on_files(rt, guard, recorded, originals, skill, &mut changed) {
        Ok(()) => Ok(changed),
        Err(e) => {
            if let Err(mut restore_error) = restore_files(rt, guard, &changed) {
                restore_error.message = format!("{} {}", e.message, restore_error.message);
                return Err(restore_error);
            }
            Err(e)
        }
    }
}

/// The backed-up text of `path` when the file still holds exactly what the
/// park left (`post_park` is its recorded hash): nothing else touched it, so
/// the original goes back wholesale.
fn untouched_original<'a>(
    current: Option<&str>,
    post_park: Option<&str>,
    path: &Path,
    originals: &'a [FileSnapshot],
) -> Option<&'a str> {
    if content_hash(current) != post_park? {
        return None;
    }
    originals
        .iter()
        .find(|file| file.path == path)
        .and_then(|file| file.text.as_deref())
}

/// Writes `text` over `path` unless it is there already, noting what the
/// file held so a later failure can put it back.
fn write_back(
    rt: &Runtime,
    guard: &ExclusiveGuard,
    path: &Path,
    current: Option<String>,
    text: &str,
    changed: &mut Vec<FileEdit>,
) -> Result<(), CoreError> {
    if current.as_deref() == Some(text) {
        return Ok(());
    }
    write_text(rt, guard, path, current.as_deref(), text)?;
    changed.push(FileEdit {
        before: FileSnapshot {
            path: path.to_path_buf(),
            text: current,
        },
        after: text.to_string(),
    });
    Ok(())
}

fn turn_on_files(
    rt: &Runtime,
    guard: &ExclusiveGuard,
    recorded: &RecordedDotagents,
    originals: &[FileSnapshot],
    skill: &SkillName,
    changed: &mut Vec<FileEdit>,
) -> Result<(), CoreError> {
    let fs = rt.ports.fs.as_ref();
    let after = recorded.after.as_ref();
    // Worked out before any file is written, so a merge that cannot be done
    // fails with agents.toml and agents.lock untouched.
    let ignore_merge = planned_ignore_merge(fs, recorded, originals, skill)?;
    if let Some(current) = read_text(fs, &recorded.config)? {
        let wholesale = untouched_original(
            Some(&current),
            after.map(|a| a.config.as_str()),
            &recorded.config,
            originals,
        );
        if let Some(original) = wholesale {
            write_back(
                rt,
                guard,
                &recorded.config,
                Some(current),
                original,
                changed,
            )?;
        } else {
            let mut doc = parse(&recorded.config, &current)?;
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
                let rendered = render_checked(&recorded.config, &doc, current.contains("\r\n"))?;
                write_back(
                    rt,
                    guard,
                    &recorded.config,
                    Some(current),
                    &rendered,
                    changed,
                )?;
            }
        }
    }
    // A missing `agents.lock` stays missing: a file without `version = 1`
    // breaks every dotagents command, and dotagents writes its own.
    if let Some(lock) = &recorded.lock {
        if let Some(current) = read_text(fs, lock)? {
            let wholesale = untouched_original(
                Some(&current),
                after.map(|a| a.lock.as_str()),
                lock,
                originals,
            );
            if let Some(original) = wholesale {
                write_back(rt, guard, lock, Some(current), original, changed)?;
            } else if let Some(lock_entry) = &recorded.lock_entry {
                if let Some(text) = with_lock_entry(lock, &current, skill, lock_entry)? {
                    write_back(rt, guard, lock, Some(current), &text, changed)?;
                }
            }
        }
    }
    // The park backs `.gitignore` up only when it exists, so a row without
    // that backup leaves the file alone.
    if let Some(gitignore) = &recorded.gitignore {
        let current = read_text(fs, gitignore)?;
        let wholesale = untouched_original(
            current.as_deref(),
            after.map(|a| a.gitignore.as_str()),
            gitignore,
            originals,
        );
        if let Some(original) = wholesale {
            write_back(rt, guard, gitignore, current, original, changed)?;
        } else if let Some((text, before)) = ignore_merge {
            write_back(rt, guard, gitignore, Some(before), &text, changed)?;
        }
    }
    Ok(())
}

/// The merged ignore file and the text it replaces, when turn-on has to merge
/// the skill's lines back into a file that changed since the park. `None`
/// when the file goes back wholesale or already has every line.
fn planned_ignore_merge(
    fs: &dyn ScopeFs,
    recorded: &RecordedDotagents,
    originals: &[FileSnapshot],
    skill: &SkillName,
) -> Result<Option<(String, String)>, CoreError> {
    let Some(gitignore) = &recorded.gitignore else {
        return Ok(None);
    };
    let current = read_text(fs, gitignore)?;
    let wholesale = untouched_original(
        current.as_deref(),
        recorded.after.as_ref().map(|a| a.gitignore.as_str()),
        gitignore,
        originals,
    );
    let original = originals
        .iter()
        .find(|file| &file.path == gitignore)
        .and_then(|file| file.text.as_deref());
    let (None, Some(current), Some(original)) = (wholesale, current, original) else {
        return Ok(None);
    };
    let own = [
        format!("/skills/{}", skill.0),
        format!("/skills/{}/", skill.0),
    ];
    let merged = with_lines_restored(original, &current, &own).map_err(|lines| {
        CoreError::new(
            ErrorCode::DriftConflict,
            format!(
                "the ignore file changed too much to merge safely; put these lines back by hand: {}",
                lines.join(", ")
            ),
        )
        .at(gitignore)
    })?;
    Ok(merged.map(|text| (text, current)))
}

/// The most cells the line alignment may use, about 16 MiB of table.
const IGNORE_ALIGN_CELL_CAP: usize = 4_000_000;

/// `current` with each of `own` that `original` had, and `current` lacks,
/// back at its place in `original`; `None` when nothing is missing. The
/// backup's other lines are aligned with `current` by longest common
/// subsequence, so a line that repeats is matched by order, not by text. A
/// restored line goes right after the current line aligned to its nearest
/// earlier backup line, else right before the one aligned to its nearest
/// later line, else at the end. Git takes the last rule that matches, so a
/// line put after a negation would change what is ignored. `Err` carries the
/// missing lines when the alignment is too big or no original rule has an anchor.
fn with_lines_restored(
    original: &str,
    current: &str,
    own: &[String],
) -> Result<Option<String>, Vec<String>> {
    let kept: Vec<&str> = current.lines().collect();
    let mut present: Vec<usize> = own
        .iter()
        .map(|line| kept.iter().filter(|k| k.trim_end() == line).count())
        .collect();
    let mut stay: Vec<&str> = Vec::new();
    // Each missing line with the count of surviving backup lines before it.
    let mut missing: Vec<(usize, &str)> = Vec::new();
    for line in original.lines().map(str::trim_end) {
        match own.iter().position(|o| o == line) {
            Some(i) if present[i] > 0 => present[i] -= 1,
            Some(_) => missing.push((stay.len(), line)),
            None => stay.push(line),
        }
    }
    if missing.is_empty() {
        return Ok(None);
    }
    let Some(align) = align_lines(&stay, &kept) else {
        return Err(missing
            .iter()
            .map(|(_, line)| (*line).to_string())
            .collect());
    };
    if stay
        .iter()
        .any(|line| !line.is_empty() && !line.starts_with('#'))
        && !stay
            .iter()
            .zip(&align)
            .any(|(line, at)| !line.is_empty() && !line.starts_with('#') && at.is_some())
    {
        return Err(missing
            .iter()
            .map(|(_, line)| (*line).to_string())
            .collect());
    }
    let mut inserts: Vec<(usize, usize, &str)> = missing
        .iter()
        .enumerate()
        .map(|(order, (k, line))| {
            let at = align[..*k]
                .iter()
                .rev()
                .find_map(|j| *j)
                .map(|j| j + 1)
                .or_else(|| align[*k..].iter().find_map(|j| *j))
                .unwrap_or(kept.len());
            (at, order, *line)
        })
        .collect();
    inserts.sort_unstable();
    let mut out: Vec<&str> = Vec::with_capacity(kept.len() + inserts.len());
    let mut pending = inserts.into_iter().peekable();
    for (i, line) in kept.iter().enumerate() {
        while let Some((_, _, restored)) = pending.next_if(|(at, _, _)| *at == i) {
            out.push(restored);
        }
        out.push(line);
    }
    out.extend(pending.map(|(_, _, restored)| restored));
    Ok(Some(format!("{}\n", out.join("\n"))))
}

/// For each line of `a`, the index of the line of `b` it is aligned with
/// (trailing whitespace ignored), by longest common subsequence. The common
/// start and end are matched first, so the table covers only what differs;
/// `None` when even that is over [`IGNORE_ALIGN_CELL_CAP`].
fn align_lines(backup: &[&str], current: &[&str]) -> Option<Vec<Option<usize>>> {
    let same = |old: &str, new: &str| old == new.trim_end();
    let mut align = vec![None; backup.len()];
    let limit = backup.len().min(current.len());
    let start = (0..limit)
        .take_while(|&k| same(backup[k], current[k]))
        .count();
    let end = (0..limit - start)
        .take_while(|&k| same(backup[backup.len() - 1 - k], current[current.len() - 1 - k]))
        .count();
    for (k, slot) in align.iter_mut().enumerate().take(start) {
        *slot = Some(k);
    }
    for k in 0..end {
        align[backup.len() - 1 - k] = Some(current.len() - 1 - k);
    }
    let old = &backup[start..backup.len() - end];
    let new = &current[start..current.len() - end];
    let (rows, cols) = (old.len(), new.len());
    if rows == 0 || cols == 0 {
        return Some(align);
    }
    if (rows + 1).checked_mul(cols + 1)? > IGNORE_ALIGN_CELL_CAP {
        return None;
    }
    // lcs[r * (cols + 1) + c]: common subsequence length of old[r..], new[c..].
    let at = |r: usize, c: usize| r * (cols + 1) + c;
    let mut lcs = vec![0u32; (rows + 1) * (cols + 1)];
    for r in (0..rows).rev() {
        for c in (0..cols).rev() {
            lcs[at(r, c)] = if same(old[r], new[c]) {
                lcs[at(r + 1, c + 1)] + 1
            } else {
                lcs[at(r + 1, c)].max(lcs[at(r, c + 1)])
            };
        }
    }
    let (mut r, mut c) = (0, 0);
    while r < rows && c < cols {
        if same(old[r], new[c]) {
            align[start + r] = Some(start + c);
            r += 1;
            c += 1;
        } else if lcs[at(r + 1, c)] >= lcs[at(r, c + 1)] {
            r += 1;
        } else {
            c += 1;
        }
    }
    Some(align)
}

/// The files a park backed up, read back from its backup folder, for
/// [`turn_on`]. Empty when the row has no backup or it cannot be read; the
/// turn-on then edits the files.
pub(crate) fn backed_up_files(
    store: &dyn crate::ports::HistoryStore,
    backup_dir: Option<&str>,
) -> Vec<FileSnapshot> {
    let Some(backup_dir) = backup_dir else {
        return Vec::new();
    };
    let Ok(manifest) = store.read_manifest(backup_dir) else {
        return Vec::new();
    };
    manifest
        .entries
        .into_iter()
        .filter(|entry| !entry.relative.is_empty() && !entry.is_dir)
        .filter_map(|entry| {
            let bytes = store.read_backup_bytes(backup_dir, &entry.relative).ok()?;
            Some(FileSnapshot {
                path: entry.original,
                text: Some(String::from_utf8(bytes).ok()?),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_on_refuses_comment_only_anchor_or_overrides_external_negation() {
        assert!(with_lines_restored(
            "# header\n/skills/foo\n!/skills/foo/\n",
            "!/skills/*/\n# header\n",
            &["/skills/foo".to_string()],
        )
        .is_err());
    }

    fn restore_line(original: &str, current: &str, line: &str) -> String {
        with_lines_restored(original, current, &[line.to_string()])
            .unwrap()
            .unwrap_or_else(|| current.to_string())
    }

    fn big_ignore_file(lines: usize) -> String {
        let others: Vec<String> = (0..lines).map(|i| format!("/skills/other-{i}")).collect();
        format!("/skills/foo\n{}\n", others.join("\n"))
    }

    /// Flow: a 50k-line ignore file with a small edit in the middle. Expectation:
    /// the common start and end are trimmed, so the line still merges back.
    /// Failure: the alignment allocates a table over the whole file.
    #[test]
    fn a_large_ignore_file_with_a_small_edit_still_merges() {
        let original = big_ignore_file(50_000);
        let edited: Vec<&str> = original
            .lines()
            .skip(1)
            .map(|l| {
                if l == "/skills/other-25000" {
                    "# edited"
                } else {
                    l
                }
            })
            .collect();
        let current = edited.join("\n") + "\n";
        let merged = restore_line(&original, &current, "/skills/foo");
        assert!(merged.starts_with("/skills/foo\n/skills/other-0\n"));
        assert!(merged.contains("# edited\n"));
    }

    /// Flow: a 50k-line ignore file where most lines differ from the backup.
    /// Expectation: the merge is refused and names the missing line. Failure:
    /// a table of billions of cells is allocated.
    #[test]
    fn a_large_ignore_file_changed_too_much_is_refused() {
        let original = big_ignore_file(50_000);
        let changed: Vec<String> = (0..50_000).map(|i| format!("# changed {i}")).collect();
        let current = changed.join("\n") + "\n";
        let err =
            with_lines_restored(&original, &current, &["/skills/foo".to_string()]).unwrap_err();
        assert_eq!(err, ["/skills/foo"]);
    }

    /// Flow: the backup has the skill's rule twice, around a negation. Expectation:
    /// both come back at their own places. Failure: only the first returns and the
    /// folder shows up in git.
    #[test]
    fn every_removed_copy_of_the_rule_comes_back() {
        let original = "/skills/foo\n!/skills/foo/\n/skills/foo\n";
        let current = "!/skills/foo/\n# mine\n";
        assert_eq!(
            with_lines_restored(original, current, &["/skills/foo".to_string()])
                .unwrap()
                .unwrap(),
            "/skills/foo\n!/skills/foo/\n/skills/foo\n# mine\n"
        );
        let half = "/skills/foo\n!/skills/foo/\n# mine\n";
        assert_eq!(
            restore_line(original, half, "/skills/foo"),
            "/skills/foo\n!/skills/foo/\n/skills/foo\n# mine\n"
        );
    }

    /// Flow: the backup has `/skills/foo` then `!/skills/foo/`, and the file
    /// has since gained a comment. Expectation: the restored line lands before
    /// the negation. Failure: it is appended after it, so git ignores a folder
    /// that used to be visible.
    #[test]
    fn a_restored_ignore_line_goes_back_before_the_line_that_followed_it() {
        let original = "# top\n/skills/foo\n!/skills/foo/\n";
        let current = "# top\n!/skills/foo/\n# mine\n";
        assert_eq!(
            restore_line(original, current, "/skills/foo"),
            "# top\n/skills/foo\n!/skills/foo/\n# mine\n"
        );
        assert_eq!(
            restore_line("# top\n/skills/foo\n", "# top\n# mine\n", "/skills/foo"),
            "# top\n/skills/foo\n# mine\n"
        );
        assert_eq!(
            restore_line(
                "# group\n!/skills/foo/\n/skills/foo\n# group\n",
                "# group\n!/skills/foo/\n# group\n# mine\n",
                "/skills/foo"
            ),
            "# group\n!/skills/foo/\n/skills/foo\n# group\n# mine\n"
        );
        assert_eq!(
            restore_line("/skills/foo\n", "# mine", "/skills/foo"),
            "# mine\n/skills/foo\n"
        );
    }

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
            ("owner/repo", "https://GitHub.com/owner/repo"),
            ("owner/repo", "HTTPS://github.com/owner/repo"),
            ("owner/repo", "git@GitHub.com:owner/repo"),
            ("owner/repo", "git@github.com:owner/repo/"),
            ("group/sub/repo", "git@gitlab.com:group/sub/repo/"),
            ("git:https://x.dev/r.git", "https://x.dev/r"),
            ("path:../skills", "../skills"),
            ("owner/repo", "owner/repo/nested"),
        ];
        for (a, b) in different {
            assert!(!sources_match(a, b), "{a} should not match {b}");
        }
    }

    /// Flow: a CRLF file with a `"""` value is edited and written back.
    /// Expectation: every line break is CRLF once, also inside the string, and
    /// the text still parses. Failure: `\r\r\n` makes the file invalid TOML.
    #[test]
    fn render_gives_crlf_back_without_doubling_it() {
        let text = "note = \"\"\"a\r\nb\"\"\"\r\n[[skills]]\r\nname = \"*\"\r\n";
        let doc = table(text);
        let rendered = render(&doc, true);
        assert_eq!(rendered, text);
        assert!(rendered.parse::<toml_edit::DocumentMut>().is_ok());
        assert_eq!(render(&doc, false).contains('\r'), text.contains("a\r\nb"));
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
                supplies: true,
                had_exclude: false,
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
