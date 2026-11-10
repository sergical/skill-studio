//! Targeted ledger publication with conflict checks and a reversible write journal.

use std::path::PathBuf;

use crate::error::{CoreError, ErrorCode};
use crate::journal::{FsJournal, PlanWriter};
use crate::ports::{ExclusiveGuard, PlanStatus, Runtime};

/// One ledger's input and isolated CLI result.
pub struct LedgerChange {
    /// Live ledger path.
    pub path: PathBuf,
    /// Bytes copied into staging.
    pub before: Option<Vec<u8>>,
    /// Bytes left by the isolated CLI.
    pub after: Option<Vec<u8>>,
}

fn invalid(error: impl std::fmt::Display) -> CoreError {
    CoreError::new(ErrorCode::InvalidRequest, error.to_string())
}

fn conflict() -> CoreError {
    CoreError::new(
        ErrorCode::DriftConflict,
        "the skill's ledger entry changed during the CLI call",
    )
}

fn text(bytes: Option<&[u8]>) -> Result<&str, CoreError> {
    std::str::from_utf8(bytes.unwrap_or_default()).map_err(invalid)
}

pub(crate) fn merge(
    name: &str,
    file: &str,
    before: Option<&[u8]>,
    after: Option<&[u8]>,
    live: Option<&[u8]>,
) -> Result<Vec<u8>, CoreError> {
    if std::path::Path::new(file)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
    {
        let parse = |bytes: Option<&[u8]>| -> Result<serde_json::Value, CoreError> {
            let document = bytes.map_or_else(
                || Ok(serde_json::json!({"skills": {}})),
                |b| serde_json::from_slice::<serde_json::Value>(b).map_err(invalid),
            )?;
            if !document.is_object()
                || document
                    .get("skills")
                    .is_some_and(|skills| !skills.is_object())
            {
                return Err(invalid("ledger must contain a skills object"));
            }
            Ok(document)
        };
        let before = parse(before)?;
        let after = parse(after)?;
        let mut live = parse(live)?;
        if live["skills"][name] != before["skills"][name] {
            return Err(conflict());
        }
        if let Some(entry) = after["skills"].get(name) {
            if live.get("skills").is_none() {
                live["skills"] = serde_json::json!({});
            }
            live["skills"][name] = entry.clone();
        } else if let Some(skills) = live["skills"].as_object_mut() {
            skills.remove(name);
        }
        return serde_json::to_vec_pretty(&live).map_err(invalid);
    }
    let before = text(before)?
        .parse::<toml_edit::DocumentMut>()
        .map_err(invalid)?;
    let after = text(after)?
        .parse::<toml_edit::DocumentMut>()
        .map_err(invalid)?;
    let mut live = text(live)?
        .parse::<toml_edit::DocumentMut>()
        .map_err(invalid)?;
    if file == "agents.toml" {
        let rows = |doc: &toml_edit::DocumentMut| -> Vec<toml_edit::Table> {
            doc.get("skills")
                .and_then(toml_edit::Item::as_array_of_tables)
                .into_iter()
                .flatten()
                .filter(|row| row.get("name").and_then(toml_edit::Item::as_str) == Some(name))
                .cloned()
                .collect()
        };
        let values = |rows: Vec<toml_edit::Table>| -> Result<Vec<toml::Table>, CoreError> {
            rows.iter()
                .map(|row| toml::from_str::<toml::Table>(&row.to_string()).map_err(invalid))
                .collect()
        };
        if values(rows(&live))? != values(rows(&before))? {
            return Err(conflict());
        }
        let replacement = rows(&after);
        if live.get("skills").is_none() && !replacement.is_empty() {
            live["skills"] = toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new());
        }
        if let Some(rows) = live
            .get_mut("skills")
            .and_then(toml_edit::Item::as_array_of_tables_mut)
        {
            rows.retain(|row| row.get("name").and_then(toml_edit::Item::as_str) != Some(name));
            for row in replacement {
                rows.push(row);
            }
        }
    } else {
        let entry = |doc: &toml_edit::DocumentMut| {
            doc.get("skills")
                .and_then(|t| t.get(name))
                .map(|entry| {
                    let mut document = toml_edit::DocumentMut::new();
                    document.insert("entry", entry.clone());
                    toml::from_str::<toml::Table>(&document.to_string()).map_err(invalid)
                })
                .transpose()
        };
        if entry(&live)? != entry(&before)? {
            return Err(conflict());
        }
        if live.get("skills").is_none() && after.get("skills").and_then(|t| t.get(name)).is_some() {
            live["skills"] = toml_edit::Item::Table(toml_edit::Table::new());
        }
        if let Some(skills) = live
            .get_mut("skills")
            .and_then(toml_edit::Item::as_table_like_mut)
        {
            skills.remove(name);
            if let Some(entry) = after.get("skills").and_then(|t| t.get(name)) {
                skills.insert(name, entry.clone());
            }
        }
    }
    Ok(live.to_string().into_bytes())
}

/// Publishes only `name` against fresh ledgers, journaling every write before it occurs.
/// The caller must hold the scope's write lease. Pending plans use normal journal recovery.
pub fn publish(
    rt: &Runtime,
    guard: &ExclusiveGuard,
    name: &str,
    changes: &[LedgerChange],
) -> Result<(), CoreError> {
    let fs = rt.ports.fs.as_ref();
    let mut writes = Vec::new();
    for change in changes {
        if change.before == change.after {
            continue;
        }
        let path = crate::ports::resolve_config_link(fs, &change.path)?;
        let previous = match fs.read_capped(&path, 8 * 1024 * 1024) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(CoreError::io(&path, e)),
        };
        let bytes = merge(
            name,
            change
                .path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default(),
            change.before.as_deref(),
            change.after.as_deref(),
            previous.as_deref(),
        )?;
        writes.push((path, previous, bytes));
    }
    if writes.is_empty() {
        return Ok(());
    }
    crate::ops_install::ensure_journal_root(rt, guard, fs)?;
    let journal = FsJournal::new(
        crate::ops_install::journal_root(&rt.scope.home.lexical),
        rt.ports.fs.clone(),
    );
    let mut root = writes[0]
        .0
        .parent()
        .ok_or_else(|| invalid("ledger has no parent"))?
        .to_path_buf();
    while writes.iter().any(|(path, _, _)| !path.starts_with(&root)) {
        root = root
            .parent()
            .ok_or_else(|| invalid("ledgers have no common root"))?
            .to_path_buf();
    }
    let plan = PlanWriter::begin(
        &journal,
        guard,
        crate::identity::PlanId(rt.ports.ids.next_event_id().0),
        rt.ports.clock.now(),
        format!("publish ledger {name}"),
        root,
        Vec::new(),
    )?;
    publish_writes(rt, guard, name, &plan, writes)?;
    plan.finish(PlanStatus::Done)
}

/// Publishes ledgers as part of the caller's tree transaction.
pub(crate) fn publish_in_plan(
    rt: &Runtime,
    guard: &ExclusiveGuard,
    name: &str,
    changes: &[LedgerChange],
    plan: &PlanWriter<'_>,
) -> Result<(), CoreError> {
    let fs = rt.ports.fs.as_ref();
    let mut writes = Vec::new();
    for change in changes {
        if change.before == change.after {
            continue;
        }
        let path = crate::ports::resolve_config_link(fs, &change.path)?;
        let previous = match fs.read_capped(&path, 8 * 1024 * 1024) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(CoreError::io(&path, e)),
        };
        let file = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let bytes = merge(
            name,
            file,
            change.before.as_deref(),
            change.after.as_deref(),
            previous.as_deref(),
        )?;
        writes.push((path, previous, bytes));
    }
    publish_writes(rt, guard, name, plan, writes)
}

type LedgerWrite = (PathBuf, Option<Vec<u8>>, Vec<u8>);

fn publish_writes(
    rt: &Runtime,
    guard: &ExclusiveGuard,
    name: &str,
    plan: &PlanWriter<'_>,
    writes: Vec<LedgerWrite>,
) -> Result<(), CoreError> {
    let fs = rt.ports.fs.as_ref();
    for (path, previous, bytes) in writes {
        let file = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        plan.record_step(crate::ports::PlanStep::LedgerEntry {
            path: path.clone(),
            name: name.into(),
            before: merge(name, file, None, previous.as_deref(), None)?,
            after: merge(name, file, None, Some(&bytes), None)?,
        })?;
        let scoped = crate::ports::confine(&rt.scope, fs, &path)?;
        fs.write_atomic(guard, &scoped, &bytes)
            .map_err(|e| CoreError::io(&path, e))?;
    }
    Ok(())
}
