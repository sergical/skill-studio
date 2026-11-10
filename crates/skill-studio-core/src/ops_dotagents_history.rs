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
    let text = std::str::from_utf8(&bytes).map_err(|_| invalid_lock(path))?;
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
    pub(crate) bytes: Option<Vec<u8>>,
    pub(crate) inverse: LockRowUndo,
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
    let mut doc = document(fs, &undo.path)?;
    let live = row(&doc, &undo.skill, &undo.path)?;
    if !force && live != undo.expected {
        return Err(CoreError::new(
            ErrorCode::DriftConflict,
            "the agents.lock skill row changed since this event; pass force to restore anyway",
        )
        .at(&undo.path));
    }
    let bytes = if live == undo.previous {
        None
    } else {
        if let Some(previous) = &undo.previous {
            let saved = previous
                .parse::<DocumentMut>()
                .map_err(|_| invalid_lock(&undo.path))?;
            let item = saved
                .get("row")
                .ok_or_else(|| invalid_lock(&undo.path))?
                .clone();
            if !doc.contains_key("skills") {
                doc["skills"] = Item::Table(Table::new());
            }
            doc["skills"][&undo.skill] = item;
        } else if let Some(skills) = doc.get_mut("skills").and_then(Item::as_table_like_mut) {
            skills.remove(&undo.skill);
        }
        Some(doc.to_string().into_bytes())
    };
    Ok(Some(LockRestore {
        path: undo.path.clone(),
        bytes,
        inverse: LockRowUndo {
            path: undo.path,
            skill: undo.skill,
            previous: live,
            expected: undo.previous,
        },
    }))
}
