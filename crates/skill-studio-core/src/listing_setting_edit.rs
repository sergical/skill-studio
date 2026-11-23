//! Edits to the three numeric skill listing settings exposed by the desktop.
use std::path::{Path, PathBuf};

use serde_json::{Number, Value};

use crate::error::{CoreError, ErrorCode};
use crate::events::{EventDraft, EventKind, EventStatus};
use crate::fsops::{FsOpsError, ReadStamp, Root};
use crate::identity::{sha256_hex, AgentId, EventId, PlanId, SkillName};
use crate::listing_visibility::{chezmoi_source, listing_setting_file};
use crate::ports::{FileKind, MutationSession, OpContext, PlanStatus, Runtime};

const CONFIG_LIMIT: u64 = 2 * 1024 * 1024;

fn invalid(message: &str) -> CoreError {
    CoreError::new(ErrorCode::InvalidRequest, message)
}

fn stale() -> CoreError {
    CoreError::new(
        ErrorCode::StaleProposal,
        "Configuration changed. Reload from disk before saving.",
    )
}

fn file_contents(rt: &Runtime, file: &Path) -> Result<Option<Vec<u8>>, CoreError> {
    match rt.ports.fs.read_capped(file, CONFIG_LIMIT) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(CoreError::io(file, error)),
    }
}

fn revision(file: &Path, bytes: Option<&[u8]>) -> String {
    format!(
        "{}:{}",
        sha256_hex(file.as_os_str().as_encoded_bytes()),
        bytes.map_or_else(|| "absent".into(), sha256_hex)
    )
}

fn editable_path(rt: &Runtime, file: &Path) -> Option<()> {
    let root = rt.scope.root_holding(file)?;
    // Project-wide config belongs to the project. Open it in the editor, where
    // the shared change is visible. Personal and project-local config can be saved here.
    if rt.scope.projects.iter().any(|p| {
        [".claude/settings.json", ".codex/config.toml"]
            .iter()
            .any(|path| p.lexical.join(path) == file || p.canonical.join(path) == file)
    }) {
        return None;
    }
    for path in file.ancestors().take_while(|path| *path != root) {
        match rt.ports.fs.symlink_metadata(path) {
            Ok(f) if f.kind == FileKind::Symlink || f.mode.is_some_and(|m| m & 0o222 == 0) => {
                return None
            }
            Ok(f) if path == file && f.kind != FileKind::File => return None,
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return None,
        }
    }
    Some(())
}

pub(crate) fn editable_revision(rt: &Runtime, file: &Path) -> Option<String> {
    editable_path(rt, file)?;
    let bytes = file_contents(rt, file).ok()?;
    Some(revision(file, bytes.as_deref()))
}

fn edit_contents(
    key: &str,
    raw: Option<&str>,
    original: Option<&[u8]>,
) -> Result<Vec<u8>, CoreError> {
    if key == "skills.max_context_tokens" {
        let text = std::str::from_utf8(original.unwrap_or_default())
            .map_err(|_| invalid("Invalid configuration encoding"))?;
        let mut doc = text
            .parse::<toml_edit::DocumentMut>()
            .map_err(|_| invalid("Invalid TOML configuration"))?;
        let tokens = raw
            .map(|v| {
                v.parse::<i64>()
                    .ok()
                    .filter(|n| (1..=10_000).contains(n))
                    .ok_or_else(|| invalid("Enter a whole number from 1 to 10,000 tokens."))
            })
            .transpose()?;
        if !doc.contains_key("skills") && tokens.is_some() {
            doc["skills"] = toml_edit::Item::Table(toml_edit::Table::new());
        }
        if let Some(item) = doc.get_mut("skills") {
            let table = item
                .as_table_like_mut()
                .ok_or_else(|| invalid("The skills configuration must be a table."))?;
            if let Some(n) = tokens {
                let mut value = toml_edit::Value::from(n);
                if let Some(previous) = table
                    .get("max_context_tokens")
                    .and_then(toml_edit::Item::as_value)
                {
                    *value.decor_mut() = previous.decor().clone();
                }
                table.insert("max_context_tokens", toml_edit::Item::Value(value));
            } else {
                table.remove("max_context_tokens");
            }
        }
        return Ok(doc.to_string().into_bytes());
    }
    let mut document: Value = serde_json::from_slice(original.unwrap_or(b"{}"))
        .map_err(|_| invalid("Invalid JSON configuration"))?;
    let object = document
        .as_object_mut()
        .ok_or_else(|| invalid("Settings must be a JSON object."))?;
    if let Some(v) = raw {
        let number = match key {
            "skillListingBudgetFraction" => v
                .parse::<f64>()
                .ok()
                .filter(|n| *n > 0.0 && *n <= 1.0)
                .and_then(Number::from_f64),
            "skillListingMaxDescChars" => {
                v.parse::<u64>().ok().filter(|n| *n > 0).map(Number::from)
            }
            _ => None,
        }
        .ok_or_else(|| invalid("Enter a valid value for this setting."))?;
        object.insert(key.into(), Value::Number(number));
    } else {
        object.remove(key);
    }
    let mut bytes = serde_json::to_vec_pretty(&document)
        .map_err(|_| invalid("Could not encode configuration"))?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Resolve the target again and compare the revision shown in the UI before replacing it.
pub fn save(
    rt: &Runtime,
    ctx: &OpContext,
    project: Option<PathBuf>,
    key: &str,
    value: Option<&str>,
    expected: &str,
) -> Result<Option<EventId>, CoreError> {
    let mut session = MutationSession::begin(rt, ctx)?;
    let file = listing_setting_file(rt, project, key)?;
    if chezmoi_source(rt, &file).is_some() {
        return Err(invalid(
            "This setting is managed by chezmoi. Open its source to edit it.",
        ));
    }
    if editable_path(rt, &file).is_none() {
        return Err(stale());
    }
    let old = file_contents(rt, &file)?;
    if revision(&file, old.as_deref()) != expected {
        return Err(stale());
    }
    let next = edit_contents(key, value, old.as_deref())?;
    if old.as_deref() == Some(next.as_slice()) || (old.is_none() && value.is_none()) {
        return Ok(None);
    }
    let fs = rt.ports.fs.as_ref();
    let parent = file
        .parent()
        .ok_or_else(|| invalid("No configuration folder"))?;
    let allowed = rt
        .scope
        .root_holding(parent)
        .ok_or_else(|| invalid("Configuration outside scope"))?;
    let root = Root::open(fs, allowed.into()).map_err(|e| invalid(&e.to_string()))?;
    root.create_dirs(
        parent
            .strip_prefix(allowed)
            .map_err(|_| invalid("Configuration outside scope"))?,
    )
    .map_err(|e| invalid(&e.to_string()))?;
    let relative = file
        .strip_prefix(allowed)
        .map_err(|_| invalid("Configuration outside scope"))?;
    let stamp = match old.as_deref() {
        None => ReadStamp::Absent,
        Some(bytes) => {
            use sha2::{Digest, Sha256};
            ReadStamp::Present(Sha256::digest(bytes).into())
        }
    };
    let id = rt.ports.ids.next_event_id();
    let manifest = session
        .store
        .backup_paths(&session.guard, &id, std::slice::from_ref(&file))?;
    let before = manifest
        .entries
        .first()
        .and_then(|entry| entry.fingerprint.as_ref());
    let client = if key == "skills.max_context_tokens" {
        AgentId::CODEX
    } else {
        AgentId::CLAUDE_CODE
    };
    session.store.record(
        &session.guard,
        &id,
        &EventDraft {
            kind: EventKind::ListingSettingChange,
            skill: SkillName(key.to_owned()),
            harness: Some(AgentId::parse(client)?),
            scope: None,
            project_path: None,
            payload: serde_json::json!({ "setting": key }),
            inverse: Some(crate::events::restore_backup_inverse(&file, before, None)),
            backup_dir: Some(manifest.backup_dir),
        },
    )?;
    crate::ops_install::ensure_journal_root(rt, &session.guard, fs)?;
    let journal = crate::journal::FsJournal::new(
        crate::ops_install::journal_root(&rt.scope.home.lexical),
        rt.ports.fs.clone(),
    );
    let plan = crate::journal::PlanWriter::begin(
        &journal,
        &session.guard,
        PlanId(id.0.clone()),
        rt.ports.clock.now(),
        "listing_setting",
        allowed.into(),
        Vec::new(),
    )?;
    crate::fsops::write_file_with_create_mode(&root, &plan, relative, &next, &stamp, 0o600)
        .map_err(|err| match err {
            FsOpsError::StaleRead { .. } => stale(),
            _ => CoreError::new(ErrorCode::Io, err.to_string()),
        })?;
    plan.finish(PlanStatus::Done)?;
    session.store.finish(
        &session.guard,
        &id,
        EventStatus::Done,
        crate::events::fingerprint_path(fs, &file)?,
    )?;
    session.finish(rt, ctx);
    Ok(Some(id))
}
