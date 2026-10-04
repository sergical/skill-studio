//! Install-aware Park for skills dotagents manages (#389).
//!
//! `dotagents install` copies every skill `agents.toml` lists into
//! `~/.agents/skills` on every run, so a parked copy would come back at the
//! next install. Park therefore runs `dotagents remove <name>` (it drops the
//! entry, or adds the name to a `name = "*"` entry's `exclude`), and turning
//! the skill on edits `agents.toml` back: dotagents has no command for that.
//!
//! `park` plans first ([`plan_park`]), so a refusal leaves no journal row;
//! the edits to `agents.toml` go through `toml_edit`, which keeps comments
//! and formatting.

use std::path::{Path, PathBuf};

use crate::dto::DeploymentDto;
use crate::error::{CoreError, ErrorCode};
use crate::identity::{LifecycleOwnerKind, RootScope, SkillName};
use crate::ports::{ExclusiveGuard, OpContext, ProcessSpec, Runtime, ScopeFs};

/// What Park must undo in dotagents' files for one skill.
pub(crate) struct DotagentsPark {
    /// The dotagents program found on `PATH`.
    program: PathBuf,
    /// `agents.toml`, or the file its link resolves to.
    pub(crate) config: PathBuf,
    /// `agents.lock`, or the file its link resolves to.
    pub(crate) lock: PathBuf,
    /// `agents.toml` as read, written back when `dotagents remove` fails.
    original_config: String,
    /// The skill's own `[[skills]]` entry, kept so turn-on can put it back.
    /// `None` when a wildcard entry supplies the skill.
    pub(crate) entry: Option<String>,
}

/// The `dotagents` fields a park row records for turn-on.
pub(crate) struct RecordedDotagents {
    pub(crate) config: PathBuf,
    pub(crate) entry: Option<String>,
}

impl DotagentsPark {
    /// The `payload.dotagents` value the park row records.
    pub(crate) fn payload(&self) -> serde_json::Value {
        serde_json::json!({ "config": self.config, "entry": self.entry })
    }
}

/// Reads back what [`DotagentsPark::payload`] recorded.
pub(crate) fn recorded(payload: &serde_json::Value) -> Option<RecordedDotagents> {
    let value = payload.get("dotagents")?;
    Some(RecordedDotagents {
        config: PathBuf::from(value.get("config")?.as_str()?),
        entry: value
            .get("entry")
            .and_then(|entry| entry.as_str())
            .map(str::to_string),
    })
}

fn manifest_dir(rt: &Runtime, scope: &RootScope) -> PathBuf {
    let project = match scope {
        RootScope::Global => None,
        RootScope::Project(project) => Some(project.0.as_path()),
    };
    crate::dotagents_ledger::dotagents_dir(&rt.scope.home.lexical, project)
}

fn read_manifest(
    fs: &dyn ScopeFs,
    config: &Path,
) -> Result<Option<(String, toml_edit::DocumentMut)>, CoreError> {
    let bytes = match fs.read_capped(config, crate::dotagents_ledger::DOTAGENTS_FILE_MAX_BYTES) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(CoreError::io(config, e)),
    };
    let text = String::from_utf8(bytes).map_err(|e| {
        CoreError::new(
            ErrorCode::InvalidRequest,
            format!("{} is not valid UTF-8: {e}", config.display()),
        )
        .at(config)
    })?;
    let doc = text.parse::<toml_edit::DocumentMut>().map_err(|e| {
        CoreError::new(
            ErrorCode::InvalidRequest,
            format!("{} is not valid TOML: {e}", config.display()),
        )
        .at(config)
    })?;
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

/// Decides whether parking `deployment` needs `dotagents remove`.
///
/// `None` when dotagents does not list the skill (so `install` would not
/// bring it back). Refuses when it does list the skill but `dotagents` is not
/// on `PATH`: parking would leave a copy the next `dotagents install` brings
/// back.
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
    let explicit = rows(&doc).find(|row| row_name(row) == Some(skill.0.as_str()));
    let entry = explicit.map(ToString::to_string);
    let wildcard_supplies = entry.is_none()
        && rows(&doc).any(|row| row_name(row) == Some("*") && !excludes(row, &skill.0));
    if entry.is_none() && !wildcard_supplies {
        return Ok(None);
    }
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
    Ok(Some(DotagentsPark {
        program,
        lock: crate::ports::resolve_config_link(fs, &dir.join("agents.lock"))?,
        config,
        original_config,
        entry,
    }))
}

/// Runs `dotagents remove <name>` for the scope the copy came from.
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
    let mut args = Vec::new();
    let cwd = match scope {
        RootScope::Global => rt.scope.home.lexical.clone(),
        RootScope::Project(project) => {
            args.push("--project".to_string());
            project.0.clone()
        }
    };
    args.push("remove".to_string());
    args.push(skill.0.clone());
    let spec = ProcessSpec {
        program: plan.program.display().to_string(),
        args,
        cwd: Some(cwd),
        env: vec![(
            "HOME".to_string(),
            rt.scope.home.lexical.display().to_string(),
        )],
        timeout_ms: 120_000,
    };
    let output = spawner.run(&spec, ctx.cancel.as_ref())?;
    if output.status != Some(0) {
        return Err(crate::ops_update::cli_failure("dotagents remove", &output));
    }
    Ok(())
}

/// Writes `agents.toml` back as `run_remove` found it, after a failed remove.
/// Best effort: the remove error is the one to report.
pub(crate) fn restore_config(
    rt: &Runtime,
    guard: &ExclusiveGuard,
    plan: &DotagentsPark,
) -> Result<(), CoreError> {
    write_config(rt, guard, &plan.config, &plan.original_config)
}

fn write_config(
    rt: &Runtime,
    guard: &ExclusiveGuard,
    config: &Path,
    text: &str,
) -> Result<(), CoreError> {
    let fs = rt.ports.fs.as_ref();
    let scoped = crate::ports::confine_write_through(&rt.scope, fs, config)?;
    fs.write_atomic(guard, &scoped, text.as_bytes())
        .map_err(|e| CoreError::io(config, e))
}

/// The `agents.toml` text with `skill` listed again, or `None` when it
/// already is. `entry` is the recorded `[[skills]]` entry; without one the
/// skill came from a wildcard entry and leaves its `exclude`.
fn with_skill_back(
    mut doc: toml_edit::DocumentMut,
    skill: &SkillName,
    entry: Option<&str>,
) -> Result<Option<String>, CoreError> {
    let changed = if let Some(entry) = entry {
        {
            if rows(&doc).any(|row| row_name(row) == Some(skill.0.as_str())) {
                false
            } else {
                let table = entry
                    .parse::<toml_edit::DocumentMut>()
                    .map_err(|e| {
                        CoreError::new(
                            ErrorCode::Io,
                            format!("the recorded agents.toml entry is not valid TOML: {e}"),
                        )
                    })?
                    .as_table()
                    .clone();
                if doc.get("skills").is_none() {
                    doc["skills"] = toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new());
                }
                match doc
                    .get_mut("skills")
                    .and_then(toml_edit::Item::as_array_of_tables_mut)
                {
                    Some(list) => {
                        list.push(table);
                        true
                    }
                    None => false,
                }
            }
        }
    } else {
        {
            let mut changed = false;
            if let Some(list) = doc
                .get_mut("skills")
                .and_then(toml_edit::Item::as_array_of_tables_mut)
            {
                for row in list.iter_mut().filter(|row| row_name(row) == Some("*")) {
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
            changed
        }
    };
    Ok(changed.then(|| doc.to_string()))
}

/// Puts `skill` back in `recorded.config` when turn-on needs it. Returns
/// whether the file changed.
pub(crate) fn turn_on(
    rt: &Runtime,
    guard: &ExclusiveGuard,
    recorded: &RecordedDotagents,
    skill: &SkillName,
) -> Result<bool, CoreError> {
    let fs = rt.ports.fs.as_ref();
    let Some((_, doc)) = read_manifest(fs, &recorded.config)? else {
        return Ok(false);
    };
    let Some(text) = with_skill_back(doc, skill, recorded.entry.as_deref())? else {
        return Ok(false);
    };
    write_config(rt, guard, &recorded.config, &text)?;
    Ok(true)
}
