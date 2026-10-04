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

/// What Park must undo in dotagents' files for one skill.
pub(crate) struct DotagentsPark {
    /// The dotagents program found on `PATH`.
    program: PathBuf,
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
    /// Positions (among the `[[skills]]` rows) of the `name = "*"` entries
    /// that supply the skill and did not exclude it yet. `remove` adds the
    /// exclude to one of them; turn-on lifts it from these rows only, so an
    /// exclude the person wrote stays.
    wildcard_rows: Vec<usize>,
}

/// The `dotagents` fields a park row records for turn-on.
pub(crate) struct RecordedDotagents {
    pub(crate) config: PathBuf,
    pub(crate) lock: Option<PathBuf>,
    entry: Option<String>,
    lock_entry: Option<String>,
    wildcard_rows: Vec<usize>,
}

impl DotagentsPark {
    /// The `payload.dotagents` value the park row records.
    pub(crate) fn payload(&self) -> serde_json::Value {
        serde_json::json!({
            "config": self.config,
            "lock": self.lock,
            "entry": self.entry,
            "lock_entry": self.lock_entry,
            "wildcard_rows": self.wildcard_rows,
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
        wildcard_rows: value
            .get("wildcard_rows")
            .and_then(|rows| rows.as_array())
            .map(|rows| {
                rows.iter()
                    .filter_map(|row| row.as_u64().and_then(|row| usize::try_from(row).ok()))
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

fn row_name(row: &toml_edit::Table) -> Option<&str> {
    row.get("name").and_then(toml_edit::Item::as_str)
}

fn excludes(row: &toml_edit::Table, name: &str) -> bool {
    row.get("exclude")
        .and_then(toml_edit::Item::as_array)
        .is_some_and(|list| list.iter().any(|item| item.as_str() == Some(name)))
}

fn lists_explicitly(doc: &toml_edit::DocumentMut, name: &str) -> bool {
    rows(doc).any(|row| row_name(row) == Some(name))
}

/// Whether `dotagents install` would install `name` from `doc`: an entry of
/// that name, or a `name = "*"` entry that does not exclude it.
fn lists(doc: &toml_edit::DocumentMut, name: &str) -> bool {
    lists_explicitly(doc, name)
        || rows(doc).any(|row| row_name(row) == Some("*") && !excludes(row, name))
}

fn lock_table_text(doc: &toml_edit::DocumentMut, name: &str) -> Option<String> {
    doc.get("skills")
        .and_then(toml_edit::Item::as_table)
        .and_then(|skills| skills.get(name))
        .and_then(toml_edit::Item::as_table)
        .map(ToString::to_string)
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
    let git_root = project
        .0
        .ancestors()
        .find(|dir| fs.symlink_metadata(&dir.join(".git")).is_ok());
    match git_root {
        Some(root) if root != project.0 => Err(CoreError::new(
            ErrorCode::Unsupported,
            format!(
                "dotagents reads agents.toml from the git root of a project, which is {}, not {}, so it cannot remove {} from here. Park it from a project opened at {}.",
                root.display(),
                project.0.display(),
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
/// bring it back). Refuses when it does list the skill but `dotagents` is not
/// on `PATH`: parking would leave a copy the next `dotagents install` brings
/// back. Also refuses a project that is not its own git root.
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
    let entry = rows(&doc)
        .find(|row| row_name(row) == Some(skill.0.as_str()))
        .map(ToString::to_string);
    let wildcard_rows: Vec<usize> = if entry.is_some() {
        Vec::new()
    } else {
        rows(&doc)
            .enumerate()
            .filter(|(_, row)| row_name(row) == Some("*") && !excludes(row, &skill.0))
            .map(|(index, _)| index)
            .collect()
    };
    if entry.is_none() && wildcard_rows.is_empty() {
        return Ok(None);
    }
    refuse_nested_project(rt, &deployment.root.scope, skill)?;
    let program = rt
        .ports
        .tools
        .as_ref()
        .and_then(|tools| tools.find_binary("dotagents"))
        .ok_or_else(|| {
            CoreError::new(
                ErrorCode::Unsupported,
                format!(
                    "dotagents manages {} but is not installed here, so the next `dotagents install` would bring the copy back. Install dotagents, then park it again.",
                    skill.0
                ),
            )
            .at(&deployment.path)
        })?;
    let lock = crate::ports::resolve_config_link(fs, &dir.join("agents.lock"))?;
    let original_lock = read_text(fs, &lock)?;
    let lock_entry = match &original_lock {
        Some(text) => lock_table_text(&parse(&lock, text)?, &skill.0),
        None => None,
    };
    Ok(Some(DotagentsPark {
        program,
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
        wildcard_rows,
    }))
}

/// Runs `dotagents remove <name> -y` for the scope the copy came from, then
/// checks `agents.toml` really changed: dotagents exits 0 without a change
/// when it has to ask a question and has no terminal.
pub(crate) fn run_remove(
    rt: &Runtime,
    ctx: &OpContext,
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
    let mut args = Vec::new();
    // dotagents reads `DOTAGENTS_HOME` before it looks at the project, so a
    // project run must not inherit one (an empty value removes the variable).
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
    verify_removed(rt, plan, skill)
}

fn verify_removed(rt: &Runtime, plan: &DotagentsPark, skill: &SkillName) -> Result<(), CoreError> {
    let doc = match read_manifest(rt.ports.fs.as_ref(), &plan.config)? {
        Some((_, doc)) => doc,
        None => parse(&plan.config, "")?,
    };
    let done = if plan.entry.is_some() {
        !lists_explicitly(&doc, &skill.0)
    } else {
        rows(&doc)
            .enumerate()
            .any(|(index, row)| plan.wildcard_rows.contains(&index) && excludes(row, &skill.0))
    };
    if done {
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

/// The `agents.toml` text with `skill` listed again, or `None` when it
/// already is. `entry` is the recorded `[[skills]]` entry; without one the
/// skill came from wildcard entries and leaves the `exclude` of
/// `wildcard_rows`.
fn with_skill_back(
    mut doc: toml_edit::DocumentMut,
    skill: &SkillName,
    entry: Option<&str>,
    wildcard_rows: &[usize],
) -> Result<Option<String>, CoreError> {
    let mut changed = false;
    if let Some(entry) = entry {
        if !lists_explicitly(&doc, &skill.0) {
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
    } else if let Some(list) = doc
        .get_mut("skills")
        .and_then(toml_edit::Item::as_array_of_tables_mut)
    {
        for (index, row) in list.iter_mut().enumerate() {
            if !wildcard_rows.contains(&index) || row_name(row) != Some("*") {
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
    Ok(changed.then(|| doc.to_string()))
}

/// The `agents.lock` text with the skill's table back, or `None` when it is
/// there already.
fn with_lock_entry(
    path: &Path,
    text: Option<&str>,
    skill: &SkillName,
    lock_entry: &str,
) -> Result<Option<String>, CoreError> {
    let mut doc = parse(path, text.unwrap_or_default())?;
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
    if let Some((before, doc)) = read_manifest(fs, &recorded.config)? {
        let edited = with_skill_back(
            doc,
            skill,
            recorded.entry.as_deref(),
            &recorded.wildcard_rows,
        )?;
        if let Some(text) = edited {
            write_text(rt, guard, &recorded.config, &text)?;
            changed.push(FileSnapshot {
                path: recorded.config.clone(),
                text: Some(before),
            });
        }
    }
    if let (Some(lock), Some(lock_entry)) = (&recorded.lock, &recorded.lock_entry) {
        let before = read_text(fs, lock)?;
        if let Some(text) = with_lock_entry(lock, before.as_deref(), skill, lock_entry)? {
            write_text(rt, guard, lock, &text)?;
            changed.push(FileSnapshot {
                path: lock.clone(),
                text: before,
            });
        }
    }
    Ok(())
}

/// Refuses `dotagents install` while a parked skill is still listed in the
/// scope's `agents.toml`: install would copy it back. Skills parked before
/// Park ran `dotagents remove` are in that state.
pub(crate) fn refuse_listed_parked(rt: &Runtime, scope: &RootScope) -> Result<(), CoreError> {
    let fs = rt.ports.fs.as_ref();
    let parked_root = rt
        .scope
        .home
        .lexical
        .join(crate::identity::PARKED_ROOT_RELATIVE);
    let project_key = match scope {
        RootScope::Global => None,
        RootScope::Project(project) => Some(crate::park_layout::project_key(
            &fs.canonicalize(&project.0)
                .unwrap_or_else(|_| project.0.clone()),
        )),
    };
    let origin = crate::identity::RootRef {
        scope: scope.clone(),
        kind: crate::identity::RootKind::Universal,
    };
    let Some(slot) =
        crate::park_layout::parked_slot_dir(&parked_root, &origin, project_key.as_deref())
    else {
        return Ok(());
    };
    let Ok(parked) = fs.read_dir(&slot) else {
        return Ok(());
    };
    let config =
        crate::ports::resolve_config_link(fs, &manifest_dir(rt, scope).join("agents.toml"))?;
    let Some((_, doc)) = read_manifest(fs, &config)? else {
        return Ok(());
    };
    let mut names: Vec<String> = parked
        .iter()
        .filter(|entry| crate::ports::is_skill_shaped_entry(entry))
        .map(|entry| entry.name.clone())
        .filter(|name| lists(&doc, name))
        .collect();
    names.sort();
    match names.first() {
        Some(name) => Err(CoreError::new(
            ErrorCode::InvalidRequest,
            format!(
                "{name} is parked but still listed in agents.toml, so dotagents install would bring it back. Turn it on or remove it from agents.toml first."
            ),
        )
        .at(&config)),
        None => Ok(()),
    }
}
