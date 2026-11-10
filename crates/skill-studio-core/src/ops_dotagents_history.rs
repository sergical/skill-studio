//! Per-skill agents.lock inverses for covered dotagents updates and their restore chain.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item, Table};

use crate::error::{CoreError, ErrorCode};
use crate::ports::ScopeFs;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct LockRowUndo {
    path: PathBuf,
    skill: String,
    previous: Option<String>,
    expected: Option<String>,
}

fn document(fs: &dyn ScopeFs, path: &Path) -> Result<DocumentMut, CoreError> {
    let bytes = match fs.read_capped(path, crate::dotagents_ledger::DOTAGENTS_FILE_MAX_BYTES) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(DocumentMut::new()),
        Err(error) => return Err(CoreError::io(path, error)),
    };
    parse_document(&bytes, path)
}

fn parse_document(bytes: &[u8], path: &Path) -> Result<DocumentMut, CoreError> {
    let text = std::str::from_utf8(bytes).map_err(|_| invalid_lock(path))?;
    let mut doc = text
        .parse::<DocumentMut>()
        .map_err(|_| invalid_lock(path))?;
    if let Some(skills) = doc.get_mut("skills") {
        let table = std::mem::take(skills)
            .into_table()
            .map_err(|_| invalid_lock(path))?;
        *skills = Item::Table(table);
    }
    Ok(doc)
}

fn invalid_lock(path: &Path) -> CoreError {
    CoreError::new(
        ErrorCode::InvalidRequest,
        "cannot read the agents.lock skill rows",
    )
    .at(path)
}

fn row(doc: &DocumentMut, skill: &str, path: &Path) -> Result<Option<String>, CoreError> {
    let Some(item) = doc.get("skills").and_then(|skills| skills.get(skill)) else {
        return Ok(None);
    };
    let mut wrapped = DocumentMut::new();
    wrapped["row"] = item.clone();
    // Canonical TOML retains dates and unknown row fields while ignoring formatting drift.
    let value: toml::Table =
        toml::from_str(&wrapped.to_string()).map_err(|_| invalid_lock(path))?;
    toml::to_string(&value)
        .map(Some)
        .map_err(|_| invalid_lock(path))
}

impl LockRowUndo {
    pub(crate) fn before(fs: &dyn ScopeFs, path: &Path, skill: &str) -> Result<Self, CoreError> {
        Ok(Self {
            path: path.to_path_buf(),
            skill: skill.to_string(),
            previous: row(&document(fs, path)?, skill, path)?,
            expected: None,
        })
    }

    pub(crate) fn after(mut self, fs: &dyn ScopeFs) -> Result<Self, CoreError> {
        self.expected = row(&document(fs, &self.path)?, &self.skill, &self.path)?;
        Ok(self)
    }
}

pub(crate) struct LockRestore {
    pub(crate) path: PathBuf,
    pub(crate) needs_write: bool,
    pub(crate) inverse: LockRowUndo,
    request: LockRowUndo,
}

pub(crate) fn prepare_restore(
    fs: &dyn ScopeFs,
    inverse: &serde_json::Value,
    force: bool,
) -> Result<Option<LockRestore>, CoreError> {
    let Some(value) = inverse.get("dotagents_lock_undo") else {
        return Ok(None);
    };
    let undo: LockRowUndo = serde_json::from_value(value.clone()).map_err(|_| {
        CoreError::new(
            ErrorCode::InvalidRequest,
            "cannot read the saved agents.lock row",
        )
    })?;
    let doc = document(fs, &undo.path)?;
    let live = row(&doc, &undo.skill, &undo.path)?;
    if !force && live != undo.expected {
        return Err(CoreError::new(
            ErrorCode::DriftConflict,
            "the agents.lock skill row changed since this event; pass force to restore anyway",
        )
        .at(&undo.path));
    }
    let needs_write = live != undo.previous;
    if needs_write {
        if let Some(previous) = &undo.previous {
            let saved = previous
                .parse::<DocumentMut>()
                .map_err(|_| invalid_lock(&undo.path))?;
            saved.get("row").ok_or_else(|| invalid_lock(&undo.path))?;
        }
    }
    Ok(Some(LockRestore {
        path: undo.path.clone(),
        needs_write,
        request: LockRowUndo {
            expected: live.clone(),
            ..undo.clone()
        },
        inverse: LockRowUndo {
            path: undo.path,
            skill: undo.skill,
            previous: live,
            expected: undo.previous,
        },
    }))
}

pub(crate) fn commit_restore(
    rt: &crate::ports::Runtime,
    restore: &LockRestore,
    displaced: &Path,
) -> Result<(), CoreError> {
    let fs = rt.ports.fs.as_ref();
    let path = crate::ports::confine_write_through(&rt.scope, fs, &restore.path)?;
    let displaced = crate::ports::confine(&rt.scope, fs, displaced)?;
    fs.fsops_rename(path.as_path(), displaced.as_path())
        .map_err(|error| CoreError::io(&restore.path, error))?;
    let result = (|| {
        fs.fsops_fsync_dir(path.as_path().parent().unwrap_or(path.as_path()))
            .map_err(|e| CoreError::io(&restore.path, e))?;
        let captured = fs
            .read_capped(
                displaced.as_path(),
                crate::dotagents_ledger::DOTAGENTS_FILE_MAX_BYTES,
            )
            .map_err(|e| CoreError::io(displaced.as_path(), e))?;
        let mut doc = parse_document(&captured, displaced.as_path())?;
        let undo = &restore.request;
        if row(&doc, &undo.skill, &undo.path)? != undo.expected {
            return Err(CoreError::new(
                ErrorCode::DriftConflict,
                "the agents.lock skill row changed during restore; both versions are retained",
            )
            .at(&undo.path));
        }
        if let Some(previous) = &undo.previous {
            let saved = previous
                .parse::<DocumentMut>()
                .map_err(|_| invalid_lock(&undo.path))?;
            if !doc.contains_key("skills") {
                doc["skills"] = Item::Table(Table::new());
            }
            doc["skills"][&undo.skill] = saved
                .get("row")
                .ok_or_else(|| invalid_lock(&undo.path))?
                .clone();
        } else if let Some(skills) = doc.get_mut("skills").and_then(Item::as_table_like_mut) {
            skills.remove(&undo.skill);
        }
        let staged = displaced.as_path().with_extension("restored");
        let mode = fs
            .symlink_metadata(displaced.as_path())
            .map_err(|error| CoreError::io(displaced.as_path(), error))?
            .mode
            .unwrap_or(0o600)
            & 0o777;
        fs.fsops_write_new_file_with_mode(&staged, doc.to_string().as_bytes(), mode)
            .map_err(|error| CoreError::io(&staged, error))?;
        fs.fsops_fsync_file(&staged)
            .map_err(|error| CoreError::io(&staged, error))?;
        verify_capture(fs, displaced.as_path(), &captured)?;
        // Linking publishes only into an empty name. A writer that recreates agents.lock wins.
        fs.fsops_hard_link(&staged, path.as_path())
            .map_err(|error| CoreError::io(&restore.path, error))?;
        fs.fsops_fsync_dir(path.as_path().parent().unwrap_or(path.as_path()))
            .map_err(|error| CoreError::io(&restore.path, error))?;
        verify_capture(fs, displaced.as_path(), &captured)
    })();
    if result.is_err() {
        let _ = fs.fsops_hard_link(displaced.as_path(), path.as_path());
    }
    result
}

pub(crate) fn recover_capture(
    fs: &dyn ScopeFs,
    event: &crate::events::EventRecord,
) -> Result<(), CoreError> {
    let Some(displaced) = event
        .payload
        .get("displaced_agents_lock")
        .and_then(serde_json::Value::as_str)
        .map(PathBuf::from)
    else {
        return Ok(());
    };
    let Some(value) = event
        .inverse
        .as_ref()
        .and_then(|v| v.get("dotagents_lock_undo"))
    else {
        return Err(invalid_lock(&displaced));
    };
    let undo: LockRowUndo =
        serde_json::from_value(value.clone()).map_err(|_| invalid_lock(&displaced))?;
    recover_displaced(fs, &event.id, Some(&displaced), Some(&undo.path))
}

pub(crate) fn recover_displaced(
    fs: &dyn ScopeFs,
    id: &crate::identity::EventId,
    displaced: Option<&Path>,
    path: Option<&Path>,
) -> Result<(), CoreError> {
    let Some(displaced) = displaced else {
        return Ok(());
    };
    let path = path.ok_or_else(|| invalid_lock(displaced))?;
    if displaced != path.with_file_name(format!(".agents-lock-before-restore-{}", id.0)) {
        return Err(invalid_lock(displaced));
    }
    match fs.symlink_metadata(displaced) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(CoreError::io(displaced, e)),
        Ok(_) => {}
    }
    match fs.fsops_hard_link(displaced, path) {
        Ok(()) => fs
            .fsops_fsync_dir(path.parent().ok_or_else(|| invalid_lock(path))?)
            .map_err(|e| CoreError::io(path, e)),
        Err(e)
            if e.kind() == std::io::ErrorKind::AlreadyExists
                && fs
                    .fsops_device_inode(displaced)
                    .ok()
                    .zip(fs.fsops_device_inode(path).ok())
                    .is_some_and(|(left, right)| left == right) =>
        {
            Ok(())
        }
        Err(_) => Err(CoreError::new(
            ErrorCode::DriftConflict,
            format!(
                "agents.lock recovery conflicts with another writer; retained at {}",
                displaced.display()
            ),
        )
        .at(displaced)),
    }
}

fn verify_capture(fs: &dyn ScopeFs, displaced: &Path, captured: &[u8]) -> Result<(), CoreError> {
    if fs
        .read_capped(displaced, crate::dotagents_ledger::DOTAGENTS_FILE_MAX_BYTES)
        .map_err(|e| CoreError::io(displaced, e))?
        != captured
    {
        return Err(CoreError::new(
            ErrorCode::DriftConflict,
            format!(
                "agents.lock changed through an open writer; retained at {}",
                displaced.display()
            ),
        )
        .at(displaced));
    }
    Ok(())
}
