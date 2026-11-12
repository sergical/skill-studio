//! [`HistoryOpener`]/[`HistoryStore`] adapters: a no-op placeholder and a
//! real SQLite-backed store byte-compatible with the desktop's
//! `events.sqlite3` and its `backups/<event-id>/manifest.json` layout
//! (`apps/desktop/src-tauri/src/skills/event_store.rs`), so a user
//! upgrading from the shipped app keeps every existing event restorable.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use skill_studio_core::backup_retention::{
    expired_event_backups, BackupRetentionEntry, BackupRetentionScope, BACKUP_EXPIRED_PAYLOAD_KEY,
};
use skill_studio_core::error::{CoreError, ErrorCode};
use skill_studio_core::events::{
    BackupEntry, BackupManifest, EventDraft, EventFilter, EventRecord, EventStatus,
};
use skill_studio_core::fsops::{Root, StageFile};
use skill_studio_core::identity::{AgentId, EventId, Fingerprint, SkillName};
use skill_studio_core::journal::MAX_BACKUP_BYTES;
use skill_studio_core::ports::{
    ExclusiveGuard, FileKind, HeldRestoreClaim, HistoryAccess, HistoryOpener, HistoryStore, ScopeFs,
};
use skill_studio_core::scope::NormalizedScope;

/// A `HistoryOpener` with no store behind it.
///
/// Read-only operations such as a scan never open history, so this is
/// enough for them today. A mutation that asks for
/// [`HistoryAccess::ReadWrite`] fails with [`ErrorCode::Unsupported`];
/// [`SqliteHistoryOpener`] is the real event log.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoHistoryOpener;

impl HistoryOpener for NoHistoryOpener {
    fn open(
        &self,
        _scope: &NormalizedScope,
        access: HistoryAccess,
    ) -> Result<Option<Box<dyn HistoryStore>>, CoreError> {
        match access {
            HistoryAccess::ReadIfExists | HistoryAccess::Inspect => Ok(None),
            HistoryAccess::ReadWrite => Err(CoreError::new(
                ErrorCode::Unsupported,
                "this host build has no history store; mutations are not available yet",
            )),
        }
    }
}

/// A history-free runtime that still protects its lease folder's data format.
#[derive(Debug, Clone)]
pub struct GuardedNoHistoryOpener {
    folder: PathBuf,
}

impl GuardedNoHistoryOpener {
    /// Binds read-lease protection to the history folder.
    pub fn new(folder: PathBuf) -> Self {
        Self { folder }
    }
}

impl HistoryOpener for GuardedNoHistoryOpener {
    fn guard_data_folder(&self) -> Result<Box<dyn Send>, CoreError> {
        crate::data_folder_version::guard_read_data_folder(&self.folder)
            .map(|guard| Box::new(guard) as Box<dyn Send>)
            .map_err(|message| CoreError::new(ErrorCode::Io, message))
    }

    fn open(
        &self,
        scope: &NormalizedScope,
        access: HistoryAccess,
    ) -> Result<Option<Box<dyn HistoryStore>>, CoreError> {
        NoHistoryOpener.open(scope, access)
    }
}

/// Schema shared with the desktop's `event_store::open`. Kept in sync by
/// hand; the round-trip test in this module opens a database seeded with
/// the desktop's own `CREATE TABLE` statements to catch drift.
const SCHEMA_SQL: &str = "
CREATE TABLE IF NOT EXISTS events (
    id          TEXT PRIMARY KEY,
    ts          TEXT NOT NULL,
    kind        TEXT NOT NULL,
    skill       TEXT NOT NULL,
    harness     TEXT,
    scope       TEXT,
    project_path TEXT,
    payload     TEXT NOT NULL,
    inverse     TEXT,
    backup_dir  TEXT,
    status      TEXT NOT NULL,
    reverted_by TEXT REFERENCES events(id),
    restorable  INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS idx_events_skill ON events(skill, ts DESC);
CREATE INDEX IF NOT EXISTS idx_events_ts ON events(ts);

CREATE TABLE IF NOT EXISTS backup_targets (
    event_id TEXT PRIMARY KEY,
    target TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS expired_backups (
    event_id TEXT PRIMARY KEY,
    backup_dir TEXT NOT NULL,
    target TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS materialized_roots (
    root_path   TEXT PRIMARY KEY,
    harness     TEXT NOT NULL,
    shared_root TEXT NOT NULL,
    created_by  TEXT REFERENCES events(id)
);
CREATE TABLE IF NOT EXISTS materialized_disabled (
    root_path   TEXT NOT NULL REFERENCES materialized_roots(root_path),
    skill       TEXT NOT NULL,
    PRIMARY KEY (root_path, skill)
);
";

/// Opens the `SQLite` event log at a fixed path.
///
/// [`HistoryAccess::ReadIfExists`] never creates `db_path`; it returns
/// `None` when the file is absent. [`HistoryAccess::ReadWrite`] creates the
/// parent directory and the schema on first use.
#[derive(Debug, Clone)]
pub struct SqliteHistoryOpener {
    db_path: PathBuf,
    backup_roots: Vec<PathBuf>,
}

impl SqliteHistoryOpener {
    /// Builds an opener bound to `db_path`.
    pub fn new(db_path: impl Into<PathBuf>) -> Self {
        SqliteHistoryOpener {
            db_path: db_path.into(),
            backup_roots: Vec::new(),
        }
    }

    /// Runs retention under the caller's existing write lease. This lets
    /// a legacy desktop writer share the same sweep without a second lease.
    pub fn retain_backups(
        &self,
        guard: &ExclusiveGuard,
        now: DateTime<Utc>,
        scope: BackupRetentionScope,
    ) -> Result<(), CoreError> {
        SqliteHistoryStore::open_with_backup_roots(&self.db_path, &self.backup_roots)?
            .retain_backups(guard, now, scope)
    }

    /// Adds host-owned legacy backup folders. Never derive these from an
    /// event row; the desktop supplies its app-data folder directly.
    #[must_use]
    pub fn with_backup_roots(mut self, roots: Vec<PathBuf>) -> Self {
        self.backup_roots = roots;
        self
    }
}

impl HistoryOpener for SqliteHistoryOpener {
    fn guard_read_data_folder(&self) -> Result<Box<dyn Send>, CoreError> {
        let parent = self.db_path.parent().unwrap_or_else(|| Path::new("."));
        crate::data_folder_version::guard_read_data_folder(parent)
            .map(|guard| Box::new(guard) as Box<dyn Send>)
            .map_err(|message| CoreError::new(ErrorCode::Io, message))
    }

    fn guard_data_folder(&self) -> Result<Box<dyn Send>, CoreError> {
        let parent = self.db_path.parent().unwrap_or_else(|| Path::new("."));
        crate::lock_data_folders(&[parent])
            .map(|guard| Box::new(guard) as Box<dyn Send>)
            .map_err(|message| CoreError::new(ErrorCode::Io, message))
    }

    fn open(
        &self,
        _scope: &NormalizedScope,
        access: HistoryAccess,
    ) -> Result<Option<Box<dyn HistoryStore>>, CoreError> {
        match access {
            HistoryAccess::ReadIfExists => {
                let parent = self.db_path.parent().unwrap_or_else(|| Path::new("."));
                let folder_guard = crate::data_folder_version::guard_read_data_folder(parent)
                    .map_err(|message| CoreError::new(ErrorCode::Io, message))?;
                if !self.db_path.exists() {
                    return Ok(None);
                }
                Ok(Some(Box::new(SqliteHistoryStore::open_read_only(
                    &self.db_path,
                    folder_guard,
                    &self.backup_roots,
                )?)))
            }
            HistoryAccess::Inspect => {
                let parent = self.db_path.parent().unwrap_or_else(|| Path::new("."));
                let version = crate::data_folder_version::read_version(
                    &crate::data_folder_version::RealFs,
                    parent,
                )
                .map_err(|error| CoreError::new(ErrorCode::Io, error.to_string()))?;
                crate::data_folder_version::check_compatible(version, crate::CURRENT_DATA_VERSION)
                    .map_err(|error| {
                        CoreError::new(
                            ErrorCode::Io,
                            crate::data_folder_version::newer_data_folder_message(error),
                        )
                    })?;
                if !self.db_path.exists() {
                    return Ok(None);
                }
                let mut wal = self.db_path.as_os_str().to_os_string();
                wal.push("-wal");
                if fs::symlink_metadata(&wal).is_ok() {
                    return Err(CoreError::new(
                        ErrorCode::Io,
                        "history busy, skipped: WAL exists",
                    ));
                }
                let mut uri = url::Url::from_file_path(&self.db_path)
                    .map_err(|()| CoreError::new(ErrorCode::Io, "history path must be absolute"))?;
                uri.set_query(Some("immutable=1"));
                let conn = Connection::open_with_flags(
                    uri.as_str(),
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                        | rusqlite::OpenFlags::SQLITE_OPEN_URI,
                )
                .map_err(sql_err)?;
                let root = self
                    .db_path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .to_path_buf();
                Ok(Some(Box::new(SqliteHistoryStore {
                    conn,
                    backups_root: root.join("backups"),
                    fs: Arc::new(crate::RealFs::new()),
                    backup_roots: std::iter::once(root.join("backups"))
                        .chain(self.backup_roots.iter().cloned())
                        .collect(),
                    root,
                    _folder_guard: None,
                })))
            }
            HistoryAccess::ReadWrite => Ok(Some(Box::new(
                SqliteHistoryStore::open_with_backup_roots(&self.db_path, &self.backup_roots)?,
            ))),
        }
    }
}

/// The real event log: `SQLite` via `rusqlite`, plus byte backups on disk
/// under `<db_path's directory>/backups/<event-id>/`.
pub struct SqliteHistoryStore {
    conn: Connection,
    /// `db_path`'s parent directory; `backup_dir` values (e.g.
    /// `backups/<event-id>`) are relative to this.
    root: PathBuf,
    backups_root: PathBuf,
    fs: Arc<dyn ScopeFs>,
    backup_roots: Vec<PathBuf>,
    _folder_guard: Option<crate::DataFolderGuard>,
}

impl SqliteHistoryStore {
    fn open_read_only(
        db_path: &Path,
        folder_guard: crate::DataFolderGuard,
        legacy_roots: &[PathBuf],
    ) -> Result<Self, CoreError> {
        let parent = db_path.parent().unwrap_or_else(|| Path::new("."));
        let conn = Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(sql_err)?;
        Ok(Self {
            conn,
            root: parent.to_path_buf(),
            backups_root: parent.join("backups"),
            fs: Arc::new(crate::RealFs::new()),
            backup_roots: std::iter::once(parent.join("backups"))
                .chain(legacy_roots.iter().cloned())
                .collect(),
            _folder_guard: Some(folder_guard),
        })
    }

    #[cfg(test)]
    fn open(db_path: &Path) -> Result<Self, CoreError> {
        Self::open_with_backup_roots(db_path, &[])
    }

    fn open_with_backup_roots(db_path: &Path, legacy_roots: &[PathBuf]) -> Result<Self, CoreError> {
        let parent = db_path.parent().unwrap_or_else(|| Path::new("."));
        let folder_guard = crate::lock_data_folders(&[parent])
            .map_err(|message| CoreError::new(ErrorCode::Io, message))?;
        let conn = Connection::open(db_path).map_err(sql_err)?;
        // The desktop's `EventStore` opens this same file from a second
        // connection (`core_runtime::history_db_path`); WAL lets both read
        // concurrently, but a `busy_timeout` keeps a losing writer waiting
        // instead of failing immediately with `SQLITE_BUSY` - matches the
        // desktop's `event_store::open`. Set before `journal_mode = WAL`
        // itself, since that pragma is its own write that can hit a busy
        // database - a desktop write in flight at CLI/MCP startup could
        // otherwise fail this whole open instead of just waiting.
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(sql_err)?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(sql_err)?;
        // `reverted_by` is claimed before the restore row that references it
        // exists (see `claim_revert`), so foreign key enforcement on that
        // column must stay off - matches the desktop's `event_store::open`.
        conn.pragma_update(None, "foreign_keys", "OFF")
            .map_err(sql_err)?;
        conn.pragma_update(None, "synchronous", "FULL")
            .map_err(sql_err)?;
        conn.execute_batch(SCHEMA_SQL).map_err(sql_err)?;
        Ok(SqliteHistoryStore {
            conn,
            root: parent.to_path_buf(),
            backups_root: parent.join("backups"),
            fs: Arc::new(crate::RealFs::new()),
            backup_roots: std::iter::once(parent.join("backups"))
                .chain(legacy_roots.iter().cloned())
                .collect(),
            _folder_guard: Some(folder_guard),
        })
    }

    fn backup_dir_for(&self, id: &EventId) -> PathBuf {
        self.backups_root.join(&id.0)
    }

    /// Merges `patch`'s top-level keys into the JSON object stored in
    /// `column` for event `id`. A missing or non-object value is left as-is.
    fn patch_json_column(
        &mut self,
        column: JsonColumn,
        id: &EventId,
        patch: &serde_json::Value,
    ) -> Result<(), CoreError> {
        let Some(patch_obj) = patch.as_object() else {
            return Ok(());
        };
        let (select, update) = match column {
            JsonColumn::Payload => (
                "SELECT payload FROM events WHERE id = ?1",
                "UPDATE events SET payload = ?1 WHERE id = ?2",
            ),
            JsonColumn::Inverse => (
                "SELECT inverse FROM events WHERE id = ?1",
                "UPDATE events SET inverse = ?1 WHERE id = ?2",
            ),
        };
        let transaction = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(sql_err)?;
        let stored: Option<String> = transaction
            .query_row(select, params![id.0], |row| row.get(0))
            .map_err(sql_err)?;
        let Some(stored) = stored else {
            return Ok(());
        };
        let mut value: serde_json::Value =
            serde_json::from_str(&stored).unwrap_or(serde_json::Value::Null);
        let Some(obj) = value.as_object_mut() else {
            return Ok(());
        };
        for (key, patch_value) in patch_obj {
            obj.insert(key.clone(), patch_value.clone());
        }
        let updated = serde_json::to_string(&value).map_err(json_err)?;
        transaction
            .execute(update, params![updated, id.0])
            .map_err(sql_err)?;
        transaction.commit().map_err(sql_err)
    }
    fn managed_backup_dir(&self, id: &EventId, backup_dir: &str) -> Result<PathBuf, CoreError> {
        let mut components = Path::new(&id.0).components();
        if !matches!(components.next(), Some(std::path::Component::Normal(_)))
            || components.next().is_some()
        {
            return Err(CoreError::new(
                ErrorCode::InvalidRequest,
                "backup ID must name one folder",
            ));
        }
        let requested = Path::new(backup_dir);
        let relative = PathBuf::from("backups").join(&id.0);
        let directory = if requested == relative {
            let mut found = None;
            for root in &self.backup_roots {
                let candidate = root.join(&id.0);
                match fs::symlink_metadata(&candidate) {
                    Ok(_) => {
                        found = Some(candidate);
                        break;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(CoreError::io(candidate, error)),
                }
            }
            found.unwrap_or_else(|| self.backup_dir_for(id))
        } else if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            return Err(
                CoreError::new(ErrorCode::InvalidRequest, "invalid managed backup path")
                    .at(requested),
            );
        };
        let parent = directory.parent().ok_or_else(|| {
            CoreError::new(ErrorCode::InvalidRequest, "backup folder has no parent")
        })?;
        let canonical_parent =
            canonical_ancestor_path(parent).map_err(|error| CoreError::io(parent, error))?;
        let owned_parent = self
            .backup_roots
            .iter()
            .map(|root| canonical_ancestor_path(root).map_err(|error| CoreError::io(root, error)))
            .collect::<Result<Vec<_>, _>>()?;
        if !owned_parent.contains(&canonical_parent)
            || directory.file_name() != Some(std::ffi::OsStr::new(&id.0))
        {
            return Err(CoreError::new(
                ErrorCode::InvalidRequest,
                "backup folder is outside the history store",
            )
            .at(directory));
        }
        Ok(directory)
    }

    fn backup_directory(&self, backup_dir: &str) -> Result<PathBuf, CoreError> {
        let id = Path::new(backup_dir)
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| CoreError::new(ErrorCode::InvalidRequest, "invalid backup folder"))?;
        let directory = self.managed_backup_dir(&EventId(id.to_owned()), backup_dir)?;
        let facts = self
            .fs
            .symlink_metadata(&directory)
            .map_err(|error| CoreError::io(&directory, error))?;
        if facts.kind != FileKind::Dir {
            return Err(CoreError::new(
                ErrorCode::InvalidRequest,
                "a backup folder must be a directory",
            )
            .at(&directory));
        }
        Ok(directory)
    }

    fn backup_entry_path(&self, backup_dir: &str, relative: &str) -> Result<PathBuf, CoreError> {
        if relative.is_empty()
            || Path::new(relative)
                .components()
                .any(|part| !matches!(part, std::path::Component::Normal(_)))
        {
            return Err(CoreError::new(
                ErrorCode::InvalidRequest,
                "a backup entry must stay inside its folder",
            ));
        }
        let directory = self.backup_directory(backup_dir)?;
        let root = Root::open(self.fs.as_ref(), directory)
            .map_err(|error| CoreError::new(ErrorCode::InvalidRequest, error.to_string()))?;
        root.confine(Path::new(relative))
            .map_err(|error| CoreError::new(ErrorCode::InvalidRequest, error.to_string()))
    }

    fn collect_expired_backups(&mut self, guard: &ExclusiveGuard) -> Result<(), CoreError> {
        let pending = {
            let mut statement = self
                .conn
                .prepare(
                    "SELECT event_id,backup_dir,target FROM expired_backups ORDER BY rowid ASC",
                )
                .map_err(sql_err)?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        EventId(row.get(0)?),
                        row.get::<_, String>(1)?,
                        PathBuf::from(row.get::<_, String>(2)?),
                    ))
                })
                .map_err(sql_err)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(sql_err)?
        };
        for (id, backup_dir, target) in pending {
            if !target_is_leased(guard, &target)? {
                continue;
            }
            let directory = self.managed_backup_dir(&id, &backup_dir)?;
            remove_backup_tree(self.fs.as_ref(), &directory)?;
            if let Some(parent) = directory.parent() {
                self.fs
                    .fsops_fsync_dir(parent)
                    .map_err(|error| CoreError::io(parent, error))?;
            }
            self.conn
                .execute(
                    "DELETE FROM expired_backups WHERE event_id=?1",
                    params![id.0],
                )
                .map_err(sql_err)?;
        }
        Ok(())
    }
}

fn target_is_leased(guard: &ExclusiveGuard, target: &Path) -> Result<bool, CoreError> {
    let canonical =
        canonical_ancestor_path(target).map_err(|error| CoreError::io(target, error))?;
    Ok(guard
        .keys()
        .iter()
        .any(|key| canonical.starts_with(&key.canonical_root)))
}

fn remove_backup_tree(fs: &dyn ScopeFs, path: &Path) -> Result<(), CoreError> {
    let facts = match fs.symlink_metadata(path) {
        Ok(facts) => facts,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(CoreError::io(path, error)),
    };
    if facts.kind == FileKind::Dir {
        let mut entries = fs
            .read_dir(path)
            .map_err(|error| CoreError::io(path, error))?;
        entries.sort_by(|left, right| left.name.cmp(&right.name));
        for entry in entries {
            remove_backup_tree(fs, &path.join(entry.name))?;
        }
        fs.fsops_remove_dir(path)
            .map_err(|error| CoreError::io(path, error))
    } else {
        fs.fsops_remove_file(path)
            .map_err(|error| CoreError::io(path, error))
    }
}

fn canonical_ancestor_path(path: &Path) -> std::io::Result<PathBuf> {
    let mut ancestor = path;
    let mut missing = Vec::new();
    loop {
        match fs::canonicalize(ancestor) {
            Ok(mut canonical) => {
                for component in missing.iter().rev() {
                    canonical.push(component);
                }
                return Ok(canonical);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let leaf = ancestor.file_name().ok_or(error)?;
                missing.push(leaf);
                ancestor = ancestor
                    .parent()
                    .ok_or_else(|| std::io::Error::other("path has no existing parent"))?;
            }
            Err(error) => return Err(error),
        }
    }
}

#[derive(Clone, Copy)]
enum JsonColumn {
    Payload,
    Inverse,
}

/// Adds `post_fingerprint` to an event's `inverse` JSON, inside the same
/// transaction as the status change so a crash never leaves a `done` row
/// whose inverse lacks it.
fn set_post_fingerprint(
    conn: &rusqlite::Connection,
    id: &EventId,
    fingerprint: &Fingerprint,
) -> Result<(), CoreError> {
    let inverse_str: Option<String> = conn
        .query_row(
            "SELECT inverse FROM events WHERE id = ?1",
            params![id.0],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_err)?
        .flatten();
    let Some(inverse_str) = inverse_str else {
        return Ok(());
    };
    let mut value: serde_json::Value =
        serde_json::from_str(&inverse_str).unwrap_or(serde_json::Value::Null);
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "post_fingerprint".to_string(),
            serde_json::Value::String(fingerprint.bare_hex().to_string()),
        );
        let updated = serde_json::to_string(&value).map_err(json_err)?;
        conn.execute(
            "UPDATE events SET inverse = ?1 WHERE id = ?2",
            params![updated, id.0],
        )
        .map_err(sql_err)?;
    }
    Ok(())
}

const DOCTOR_RETENTION_SQL: &str =
    "SELECT rowid, ts, payload, inverse FROM events WHERE rowid > ?1 ORDER BY rowid LIMIT 256";
const DOCTOR_PENDING_SQL: &str = "SELECT rowid, substr(id, 1, 256), substr(kind, 1, 256) FROM events WHERE rowid > ?1 AND status IN ('pending', 'interrupted') ORDER BY rowid LIMIT 256";

impl HistoryStore for SqliteHistoryStore {
    fn doctor_pending(&self, after: i64) -> Result<Vec<(i64, String, String)>, CoreError> {
        let mut stmt = self.conn.prepare(DOCTOR_PENDING_SQL).map_err(sql_err)?;
        let rows = stmt
            .query_map([after], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .map_err(sql_err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(sql_err)
    }
    fn doctor_retention(
        &self,
        after: i64,
    ) -> Result<Vec<skill_studio_core::ports::DoctorHistoryRow>, CoreError> {
        let mut stmt = self.conn.prepare(DOCTOR_RETENTION_SQL).map_err(sql_err)?;
        let rows = stmt
            .query_map([after], |row| {
                let ts: String = row.get(1)?;
                let payload: String = row.get(2)?;
                let inverse: Option<String> = row.get(3)?;
                Ok((row.get(0)?, ts, payload, inverse))
            })
            .map_err(sql_err)?;
        rows.map(|row| {
            let (rowid, ts, payload, inverse) = row.map_err(sql_err)?;
            Ok(skill_studio_core::ports::DoctorHistoryRow {
                rowid,
                ts: DateTime::parse_from_rfc3339(&ts)
                    .map_err(|e| CoreError::new(ErrorCode::Io, e.to_string()))?
                    .with_timezone(&Utc),
                payload: serde_json::from_str(&payload).map_err(json_err)?,
                inverse: inverse
                    .map(|s| serde_json::from_str(&s))
                    .transpose()
                    .map_err(json_err)?,
            })
        })
        .collect()
    }

    fn list(&self, filter: &EventFilter) -> Result<Vec<EventRecord>, CoreError> {
        let mut clauses = Vec::new();
        if filter.skill.is_some() {
            clauses.push("skill = ?");
        }
        if filter.after.is_some() {
            clauses.push("(ts, rowid) < (SELECT ts, rowid FROM events WHERE id = ?)");
        }
        let mut sql = String::from("SELECT * FROM events");
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }
        // `ts DESC` first, `rowid DESC` only as a tiebreaker: legacy rows the
        // desktop's `EventStore::import_legacy_events` imports get appended
        // at the end of the table (highest `rowid`) regardless of their
        // original `ts`, so `rowid` alone would sort an old imported row
        // above events written just now. The `after` cursor uses the same
        // `(ts, rowid)` key, or a page boundary would skip imported rows.
        sql.push_str(" ORDER BY ts DESC, rowid DESC LIMIT ?");

        let mut owned_params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(skill) = &filter.skill {
            owned_params.push(Box::new(skill.0.clone()));
        }
        if let Some(after) = &filter.after {
            owned_params.push(Box::new(after.0.clone()));
        }
        owned_params.push(Box::new(i64::from(filter.limit)));
        let bound: Vec<&dyn rusqlite::ToSql> = owned_params
            .iter()
            .map(std::convert::AsRef::as_ref)
            .collect();

        let mut stmt = self.conn.prepare(&sql).map_err(sql_err)?;
        let rows = stmt
            .query_map(bound.as_slice(), row_from)
            .map_err(sql_err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(sql_err)
    }

    fn get(&self, id: &EventId) -> Result<Option<EventRecord>, CoreError> {
        self.conn
            .query_row(
                "SELECT * FROM events WHERE id = ?1",
                params![id.0],
                row_from,
            )
            .optional()
            .map_err(sql_err)
    }

    fn backup_paths(
        &mut self,
        _guard: &ExclusiveGuard,
        id: &EventId,
        paths: &[PathBuf],
    ) -> Result<BackupManifest, CoreError> {
        let mut total: u64 = 0;
        for path in paths {
            total = total.saturating_add(
                backup_size(self.fs.as_ref(), path).map_err(|e| CoreError::io(path, e))?,
            );
        }
        if total > MAX_BACKUP_BYTES {
            return Err(CoreError::new(
                ErrorCode::InvalidRequest,
                "Backup exceeds the 64 MiB limit; write refused before changing the target.",
            ));
        }
        let dir = self.backup_dir_for(id);
        self.managed_backup_dir(id, &format!("backups/{}", id.0))?;
        ensure_backup_directory(self.fs.as_ref(), &dir)
            .map_err(|error| CoreError::io(&dir, error))?;

        // Every path gets a core `BackupEntry`, present or not: `fingerprint`
        // is `None` exactly when the original was absent at backup time,
        // which a later drift check reads as "this must still be absent"
        // rather than as a hash mismatch. The on-disk `manifest.json` mirrors
        // this with the desktop's literal `"absent"` string - see
        // `DesktopBackupManifest` - so a restore reads the real
        // desktop-compatible record either way.
        let mut entries = Vec::new();
        let mut on_disk = DesktopBackupManifest::default();
        for (i, path) in paths.iter().enumerate() {
            let key = path.to_string_lossy().into_owned();
            if fs::symlink_metadata(path).is_err() {
                on_disk.entries.insert(
                    key,
                    DesktopBackupEntry {
                        relative_path: String::new(),
                        fingerprint: "absent".to_string(),
                    },
                );
                entries.push(BackupEntry {
                    original: path.clone(),
                    relative: String::new(),
                    fingerprint: None,
                    is_dir: false,
                });
                continue;
            }
            let is_dir = fs::symlink_metadata(path)
                .map(|m| m.is_dir())
                .map_err(|e| CoreError::io(path, e))?;
            let basename = path
                .file_name()
                .map_or_else(|| format!("path-{i}"), |n| n.to_string_lossy().into_owned());
            let relative_path = format!("{i}-{basename}");
            copy_recursive(self.fs.as_ref(), path, &dir.join(&relative_path))
                .map_err(|e| CoreError::io(path, e))?;
            let hex = hash_entry(path).map_err(|e| CoreError::io(path, e))?;
            on_disk.entries.insert(
                key,
                DesktopBackupEntry {
                    relative_path: relative_path.clone(),
                    fingerprint: hex.clone(),
                },
            );
            let fingerprint = Fingerprint::parse(&hex).map_err(|_| {
                CoreError::new(
                    ErrorCode::ExecutionFailed,
                    "hash_entry did not return 64 lowercase hex chars",
                )
                .at(path)
            })?;
            entries.push(BackupEntry {
                original: path.clone(),
                relative: relative_path,
                fingerprint: Some(fingerprint),
                is_dir,
            });
        }

        let manifest_path = dir.join("manifest.json");
        let bytes = serde_json::to_vec_pretty(&on_disk).map_err(json_err)?;
        self.fs
            .fsops_write_new_file(&manifest_path, &bytes)
            .map_err(|error| CoreError::io(&manifest_path, error))?;
        self.fs
            .fsops_fsync_file(&manifest_path)
            .map_err(|error| CoreError::io(&manifest_path, error))?;
        self.fs
            .fsops_fsync_dir(&dir)
            .map_err(|error| CoreError::io(&dir, error))?;
        self.fs
            .fsops_fsync_dir(&self.backups_root)
            .map_err(|error| CoreError::io(&self.backups_root, error))?;
        if let Some(target) = paths.first() {
            self.conn
                .execute(
                    "INSERT OR REPLACE INTO backup_targets (event_id, target) VALUES (?1, ?2)",
                    params![id.0, target.to_string_lossy()],
                )
                .map_err(sql_err)?;
        }
        Ok(BackupManifest {
            event_id: id.clone(),
            backup_dir: format!("backups/{}", id.0),
            entries,
        })
    }

    fn record(
        &mut self,
        _guard: &ExclusiveGuard,
        id: &EventId,
        draft: &EventDraft,
    ) -> Result<(), CoreError> {
        let ts = Utc::now().to_rfc3339();
        let payload_json = serde_json::to_string(&draft.payload).map_err(json_err)?;
        let inverse_json = draft
            .inverse
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(json_err)?;
        let project_path = draft
            .project_path
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned());
        self.conn
            .execute(
                "INSERT INTO events
                    (id, ts, kind, skill, harness, scope, project_path, payload, inverse, backup_dir, status, reverted_by, restorable)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'pending', NULL, 1)",
                params![
                    id.0,
                    ts,
                    draft.kind.as_str(),
                    draft.skill.0,
                    draft.harness.as_ref().map(skill_studio_core::identity::AgentId::as_str),
                    draft.scope,
                    project_path,
                    payload_json,
                    inverse_json,
                    draft.backup_dir,
                ],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn finish(
        &mut self,
        guard: &ExclusiveGuard,
        id: &EventId,
        status: EventStatus,
        post_fingerprint: Option<Fingerprint>,
    ) -> Result<(), CoreError> {
        let tx = self.conn.transaction().map_err(sql_err)?;
        let current: Option<String> = tx
            .query_row(
                "SELECT status FROM events WHERE id = ?1",
                params![id.0],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_err)?;
        let Some(current) = current else {
            return Err(CoreError::new(
                ErrorCode::InvalidRequest,
                format!("no event {} to finish", id.0),
            ));
        };
        let current = EventStatus::parse(&current).ok_or_else(|| {
            CoreError::new(
                ErrorCode::Io,
                format!("event {} has unknown status {current}", id.0),
            )
        })?;
        let status = current.settle(status)?;
        tx.execute(
            "UPDATE events SET status = ?1 WHERE id = ?2",
            params![status.as_str(), id.0],
        )
        .map_err(sql_err)?;

        if let Some(fingerprint) = post_fingerprint {
            set_post_fingerprint(&tx, id, &fingerprint)?;
        }
        tx.commit().map_err(sql_err)?;
        if status == EventStatus::Done {
            if let Err(error) = self.retain_backups(
                guard,
                Utc::now(),
                BackupRetentionScope::TargetOf(id.clone()),
            ) {
                eprintln!("[backup_retention] cleanup deferred: {error}");
            }
        }
        Ok(())
    }

    fn retain_backups(
        &mut self,
        guard: &ExclusiveGuard,
        now: DateTime<Utc>,
        scope: BackupRetentionScope,
    ) -> Result<(), CoreError> {
        let entries = {
            let mut statement = self.conn.prepare(
                "SELECT e.id,e.ts,e.kind,e.skill,NULL AS harness,e.scope,e.project_path,e.payload,e.inverse,e.backup_dir,e.status,e.reverted_by,e.restorable,t.target AS backup_target \
                 FROM events e LEFT JOIN backup_targets t ON t.event_id=e.id \
                 ORDER BY e.ts DESC,e.rowid DESC"
            ).map_err(sql_err)?;
            let rows = statement
                .query_map([], |row| {
                    Ok(BackupRetentionEntry {
                        event: row_from(row)?,
                        target: row
                            .get::<_, Option<String>>("backup_target")?
                            .map(PathBuf::from),
                    })
                })
                .map_err(sql_err)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(sql_err)?
        };
        for id in expired_event_backups(&entries, now, &scope) {
            let Some(entry) = entries.iter().find(|entry| entry.event.id == id) else {
                continue;
            };
            let Some(target) = entry.target() else {
                continue;
            };
            if !target_is_leased(guard, &target)? {
                continue;
            }
            let Some(backup_dir) = entry.event.backup_dir.as_deref() else {
                continue;
            };
            let managed = self.managed_backup_dir(&id, backup_dir)?;
            match fs::symlink_metadata(&managed) {
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(CoreError::io(&managed, error)),
            }
            let transaction = self
                .conn
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(sql_err)?;
            let stored: String = transaction
                .query_row(
                    "SELECT payload FROM events WHERE id=?1",
                    params![id.0],
                    |row| row.get(0),
                )
                .map_err(sql_err)?;
            let mut payload: serde_json::Value = serde_json::from_str(&stored).map_err(json_err)?;
            if !payload.is_object() {
                payload = serde_json::json!({"original_payload": payload});
            }
            payload[BACKUP_EXPIRED_PAYLOAD_KEY] = serde_json::Value::Bool(true);
            let changed = transaction.execute(
                "UPDATE events SET backup_dir=NULL,restorable=0,payload=?1 WHERE id=?2 AND backup_dir=?3 AND status IN ('done','failed')",
                params![serde_json::to_string(&payload).map_err(json_err)?,id.0,backup_dir],
            ).map_err(sql_err)?;
            if changed > 0 {
                transaction.execute("INSERT OR REPLACE INTO expired_backups (event_id,backup_dir,target) VALUES (?1,?2,?3)",params![id.0,managed.to_string_lossy(),target.to_string_lossy()]).map_err(sql_err)?;
            }
            transaction.commit().map_err(sql_err)?;
            self.fs
                .fsops_fsync_dir(&self.root)
                .map_err(|error| CoreError::io(&self.root, error))?;
        }
        self.collect_expired_backups(guard)
    }

    fn patch_payload(
        &mut self,
        _guard: &ExclusiveGuard,
        id: &EventId,
        patch: serde_json::Value,
    ) -> Result<(), CoreError> {
        self.patch_json_column(JsonColumn::Payload, id, &patch)
    }

    fn patch_inverse(
        &mut self,
        _guard: &ExclusiveGuard,
        id: &EventId,
        patch: serde_json::Value,
    ) -> Result<(), CoreError> {
        self.patch_json_column(JsonColumn::Inverse, id, &patch)
    }

    fn claim_revert(
        &mut self,
        _guard: &ExclusiveGuard,
        target: &EventId,
        by: &EventId,
    ) -> Result<bool, CoreError> {
        let changed = self
            .conn
            .execute(
                "UPDATE events SET reverted_by = ?1 WHERE id = ?2 AND reverted_by IS NULL",
                params![by.0, target.0],
            )
            .map_err(sql_err)?;
        Ok(changed > 0)
    }

    fn release_revert(
        &mut self,
        _guard: &ExclusiveGuard,
        target: &EventId,
        restore: &EventId,
    ) -> Result<bool, CoreError> {
        let changed = self
            .conn
            .execute(
                "UPDATE events SET reverted_by = NULL WHERE id = ?1 AND reverted_by = ?2",
                params![target.0, restore.0],
            )
            .map_err(sql_err)?;
        Ok(changed > 0)
    }

    fn pending(&self) -> Result<Vec<EventRecord>, CoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM events WHERE status = 'pending' ORDER BY rowid ASC")
            .map_err(sql_err)?;
        let rows = stmt.query_map([], row_from).map_err(sql_err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(sql_err)
    }

    fn held_restore_claims(&self) -> Result<Vec<HeldRestoreClaim>, CoreError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT r.id, t.id, t.kind FROM events r \
                 JOIN events t ON t.reverted_by = r.id \
                   AND t.id = json_extract(r.payload, '$.target_event') \
                 WHERE r.kind = 'restore' AND r.status IN ('pending', 'interrupted') \
                 ORDER BY r.rowid ASC",
            )
            .map_err(sql_err)?;
        let rows = stmt
            .query_map([], |row| {
                Ok(HeldRestoreClaim {
                    restore: EventId(row.get(0)?),
                    target: EventId(row.get(1)?),
                    target_kind: row.get(2)?,
                })
            })
            .map_err(sql_err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(sql_err)
    }

    fn read_manifest(&self, backup_dir: &str) -> Result<BackupManifest, CoreError> {
        let dir = self.backup_directory(backup_dir)?;
        let manifest_path = dir.join("manifest.json");
        let facts = self
            .fs
            .symlink_metadata(&manifest_path)
            .map_err(|error| CoreError::io(&manifest_path, error))?;
        if facts.kind != FileKind::File {
            return Err(CoreError::new(
                ErrorCode::InvalidRequest,
                "a backup manifest must be a regular file",
            )
            .at(&manifest_path));
        }
        let on_disk = read_manifest(&dir)?;
        let entries = on_disk
            .entries
            .into_iter()
            .map(|(original, entry)| {
                let fingerprint =
                    if entry.fingerprint == "absent" {
                        None
                    } else {
                        Some(Fingerprint::parse(&entry.fingerprint).unwrap_or_else(|_| {
                            Fingerprint::of_bytes(entry.fingerprint.as_bytes())
                        }))
                    };
                // Read from the copy itself, not stored in `manifest.json`
                // (see `BackupEntry::is_dir`'s own doc): an absent entry's
                // `relative_path` is empty and never resolves under `dir`,
                // so it falls through to `false` the same as any other
                // path `symlink_metadata` cannot see.
                let is_dir = if entry.relative_path.is_empty() {
                    false
                } else {
                    let path = self.backup_entry_path(backup_dir, &entry.relative_path)?;
                    self.fs
                        .symlink_metadata(&path)
                        .map_err(|error| CoreError::io(&path, error))?
                        .kind
                        == FileKind::Dir
                };
                Ok(BackupEntry {
                    original: PathBuf::from(original),
                    relative: entry.relative_path,
                    fingerprint,
                    is_dir,
                })
            })
            .collect::<Result<Vec<_>, CoreError>>()?;

        Ok(BackupManifest {
            // The event id is not recoverable from the manifest alone; the
            // caller already has it (it is the id that named `backup_dir`).
            event_id: EventId(String::new()),
            backup_dir: backup_dir.to_string(),
            entries,
        })
    }

    fn read_backup_bytes(&self, backup_dir: &str, relative: &str) -> Result<Vec<u8>, CoreError> {
        let path = self.backup_entry_path(backup_dir, relative)?;
        let facts = self
            .fs
            .symlink_metadata(&path)
            .map_err(|error| CoreError::io(&path, error))?;
        if facts.kind != FileKind::File {
            return Err(CoreError::new(
                ErrorCode::Unsupported,
                "a file backup must be a regular file",
            )
            .at(path));
        }
        self.fs
            .read_capped(&path, u64::MAX)
            .map_err(|error| CoreError::io(&path, error))
    }

    fn read_backup_files(
        &self,
        backup_dir: &str,
        relative: &str,
    ) -> Result<Vec<StageFile>, CoreError> {
        let root = self.backup_entry_path(backup_dir, relative)?;
        let facts = self
            .fs
            .symlink_metadata(&root)
            .map_err(|error| CoreError::io(&root, error))?;
        if facts.kind != FileKind::Dir {
            return Err(CoreError::new(
                ErrorCode::Unsupported,
                "a directory backup must be a directory",
            )
            .at(root));
        }
        let mut out = Vec::new();
        read_backup_files_into(&root, &root, &mut out)?;
        Ok(out)
    }
}

/// Recursion for [`SqliteHistoryStore::read_backup_files`]: walks `dir`
/// (under `root`) and appends every regular file with its path relative to
/// `root`, its bytes, and its permission bits (the backup is an `fs::copy`,
/// which keeps them). A symlink is an error - see the trait method's own doc.
fn read_backup_files_into(
    root: &Path,
    dir: &Path,
    out: &mut Vec<StageFile>,
) -> Result<(), CoreError> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(|e| CoreError::io(dir, e))?
        .collect::<Result<_, _>>()
        .map_err(|e| CoreError::io(dir, e))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let meta = fs::symlink_metadata(&path).map_err(|e| CoreError::io(&path, e))?;
        let file_type = meta.file_type();
        if file_type.is_dir() {
            read_backup_files_into(root, &path, out)?;
        } else if file_type.is_symlink() {
            return Err(CoreError::new(
                ErrorCode::Unsupported,
                "read_backup_files does not support a symlink inside a backed-up directory",
            )
            .at(&path));
        } else {
            let bytes = fs::read(&path).map_err(|e| CoreError::io(&path, e))?;
            let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
            out.push(StageFile {
                relative,
                bytes,
                mode: Some(meta.permissions().mode() & 0o777),
            });
        }
    }
    Ok(())
}

/// On-disk `manifest.json` shape, byte-for-byte the desktop's
/// `event_store::BackupManifest`/`BackupEntry`
/// (`apps/desktop/src-tauri/src/skills/event_store.rs`): keyed by the
/// original absolute path as a lossy string, so a backup directory written
/// by either implementation reads back in the other. `fingerprint` is a
/// plain string here (not the core's typed `Fingerprint`) because the
/// desktop writes the literal `"absent"` for a path that did not exist,
/// which the typed form cannot hold.
#[derive(Debug, Default, Serialize, Deserialize)]
struct DesktopBackupManifest {
    entries: std::collections::BTreeMap<String, DesktopBackupEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DesktopBackupEntry {
    /// Path relative to the backup dir; empty when the original was absent.
    relative_path: String,
    fingerprint: String,
}

/// Reads and parses `<dir>/manifest.json`, in the desktop-compatible shape.
fn read_manifest(dir: &Path) -> Result<DesktopBackupManifest, CoreError> {
    let manifest_path = dir.join("manifest.json");
    let bytes = fs::read(&manifest_path).map_err(|e| CoreError::io(&manifest_path, e))?;
    serde_json::from_slice(&bytes).map_err(json_err)
}

fn row_from(row: &rusqlite::Row) -> rusqlite::Result<EventRecord> {
    let id: String = row.get("id")?;
    let ts: String = row.get("ts")?;
    let kind: String = row.get("kind")?;
    let skill: String = row.get("skill")?;
    let harness: Option<String> = row.get("harness")?;
    let scope: Option<String> = row.get("scope")?;
    let project_path: Option<String> = row.get("project_path")?;
    let payload_str: String = row.get("payload")?;
    let inverse_str: Option<String> = row.get("inverse")?;
    let backup_dir: Option<String> = row.get("backup_dir")?;
    let status: String = row.get("status")?;
    let reverted_by: Option<String> = row.get("reverted_by")?;
    let restorable: bool = row.get("restorable")?;

    let ts = DateTime::parse_from_rfc3339(&ts)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?;
    let status = EventStatus::parse(&status).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::<dyn std::error::Error + Send + Sync>::from(format!(
                "unknown event status `{status}`"
            )),
        )
    })?;
    let harness = harness
        .map(|h| AgentId::parse(&h))
        .transpose()
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::<dyn std::error::Error + Send + Sync>::from(e.to_string()),
            )
        })?;
    let payload = serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Null);
    let inverse = inverse_str.map(|s| serde_json::from_str(&s).unwrap_or(serde_json::Value::Null));

    Ok(EventRecord {
        id: EventId(id),
        ts,
        kind,
        skill: SkillName(skill),
        harness,
        scope,
        project_path: project_path.map(PathBuf::from),
        payload,
        inverse,
        backup_dir,
        status,
        reverted_by: reverted_by.map(EventId),
        restorable,
    })
}

// Taken by value so both convert directly as `.map_err(sql_err)` /
// `.map_err(json_err)` function pointers at every call site instead of a
// closure that reborrows; that is the only reason the error isn't consumed.
#[allow(clippy::needless_pass_by_value)]
fn sql_err(e: rusqlite::Error) -> CoreError {
    CoreError::new(ErrorCode::Io, format!("sqlite error: {e}"))
}

#[allow(clippy::needless_pass_by_value)]
fn json_err(e: serde_json::Error) -> CoreError {
    CoreError::new(ErrorCode::Io, format!("json error: {e}"))
}

/// Test seam: re-files the event `id` in the `events.sqlite3` under `data_dir`
/// as an imported legacy row, with id `imported_id`, timestamp `ts`, and a
/// `rowid` above every existing row, the way the desktop's
/// `import_legacy_events` appends older events.
#[doc(hidden)]
pub fn reimport_event_as_legacy(
    data_dir: &Path,
    id: &EventId,
    imported_id: &str,
    ts: &str,
) -> rusqlite::Result<()> {
    let folder = data_dir.join(".history");
    let _guard =
        crate::lock_data_folders(&[&folder]).map_err(rusqlite::Error::InvalidParameterName)?;
    let db = Connection::open(folder.join("events.sqlite3"))?;
    db.execute(
        "INSERT INTO events
            (id, ts, kind, skill, harness, scope, project_path, payload, inverse, backup_dir, status, reverted_by, restorable)
         SELECT ?2, ?3, kind, skill, harness, scope, project_path, payload, inverse, backup_dir, status, reverted_by, restorable
         FROM events WHERE id = ?1",
        params![id.0, imported_id, ts],
    )?;
    db.execute("DELETE FROM events WHERE id = ?1", params![id.0])?;
    Ok(())
}

/// Content hash for an existing path: matches the desktop's
/// `fingerprint_path`/`hash_entry` byte for byte (same tag bytes, same
/// length framing), so the two implementations compute identical hex for
/// identical filesystem content. Callers only reach this for a path that
/// exists; an absent path is handled separately (see `backup_paths`).
pub fn hash_entry(path: &Path) -> std::io::Result<String> {
    skill_studio_core::events::fingerprint_path(&crate::RealFs::new(), path)
        .map_err(std::io::Error::other)?
        .map(|fingerprint| fingerprint.bare_hex().to_owned())
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "backup source is missing")
        })
}

/// Copies `src` into `dest`, preserving regular files as bytes, directories
/// recursively, and symlinks as the literal link (never following it).
fn ensure_backup_directory(fs: &dyn ScopeFs, path: &Path) -> std::io::Result<()> {
    match fs.symlink_metadata(path) {
        Ok(facts) if facts.kind == FileKind::Dir => return Ok(()),
        Ok(_) => return Err(std::io::Error::other("backup parent is not a directory")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    if let Some(parent) = path.parent() {
        ensure_backup_directory(fs, parent)?;
    }
    fs.fsops_create_dir(path)?;
    fs.fsops_fsync_dir(path)?;
    if let Some(parent) = path.parent() {
        fs.fsops_fsync_dir(parent)?;
    }
    Ok(())
}

fn backup_size(fs: &dyn ScopeFs, path: &Path) -> std::io::Result<u64> {
    let facts = match fs.symlink_metadata(path) {
        Ok(facts) => facts,
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            return Ok(0)
        }
        Err(error) => return Err(error),
    };
    match facts.kind {
        FileKind::File => Ok(facts.len),
        FileKind::Dir => {
            let mut total: u64 = 0;
            for entry in fs.read_dir(path)? {
                total = total.saturating_add(backup_size(fs, &path.join(&entry.name))?);
                if total > MAX_BACKUP_BYTES {
                    break;
                }
            }
            Ok(total)
        }
        FileKind::Symlink | FileKind::Other => Ok(0),
    }
}

fn copy_recursive(fs: &dyn ScopeFs, src: &Path, dest: &Path) -> std::io::Result<()> {
    let facts = fs.symlink_metadata(src)?;
    match facts.kind {
        FileKind::Symlink => fs.fsops_symlink(&fs.read_link(src)?, dest)?,
        FileKind::Dir => {
            ensure_backup_directory(fs, dest)?;
            for entry in fs.read_dir(src)? {
                copy_recursive(fs, &src.join(&entry.name), &dest.join(&entry.name))?;
            }
            fs.fsops_fsync_dir(dest)?;
        }
        FileKind::File => {
            let bytes = fs.read_capped(src, MAX_BACKUP_BYTES)?;
            fs.fsops_write_new_file_with_mode(dest, &bytes, facts.mode.unwrap_or(0o644))?;
            fs.fsops_fsync_file(dest)?;
        }
        FileKind::Other => return Err(std::io::Error::other("cannot back up this entry type")),
    }
    if let Some(parent) = dest.parent() {
        fs.fsops_fsync_dir(parent)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use skill_studio_core::events::EventKind;
    use skill_studio_core::ports::{acquire_exclusive, HistoryOpener};
    use skill_studio_core::scope::RuntimeScope;

    #[test]
    fn history_connection_keeps_migration_locked_until_drop_or_allows_a_newer_writer() {
        let tmp = tempfile::tempdir().unwrap();
        let folder = tmp.path().join("history");
        let store = SqliteHistoryStore::open(&folder.join("events.sqlite3")).unwrap();
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "data_folder_version::tests::competing_process_cannot_lock_during_startup_open_or_migrations_can_race"])
            .env("SKILL_STUDIO_LOCK_PROBE", &folder).output().unwrap();
        assert!(
            child.status.success(),
            "{}",
            String::from_utf8_lossy(&child.stdout)
        );
        drop(store);
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "data_folder_version::tests::competing_process_cannot_lock_during_startup_open_or_migrations_can_race"])
            .env("SKILL_STUDIO_LOCK_PROBE", &folder)
            .env("SKILL_STUDIO_LOCK_AVAILABLE", "1").output().unwrap();
        assert!(
            child.status.success(),
            "{}",
            String::from_utf8_lossy(&child.stdout)
        );
    }

    #[test]
    fn cli_mutation_migration_refuses_before_lease_or_blocks_other_mutations() {
        use skill_studio_core::harness::HarnessCatalog;
        use skill_studio_core::identity::CorrelationId;
        use skill_studio_core::ports::{MutationSession, OpContext, Runtime};
        use std::sync::Arc;

        let dir = tempfile::tempdir().unwrap();
        let history = dir.path().join(".history");
        fs::create_dir(&history).unwrap();
        fs::write(history.join("schema_version"), b"1\n").unwrap();
        let older = SqliteHistoryStore::open(&history.join("events.sqlite3")).unwrap();
        fs::remove_file(history.join("schema_version")).unwrap();
        let leases = history.join("leases");
        let ports = crate::default_ports_with_history(
            leases.clone(),
            Arc::new(HarnessCatalog::builtin()),
            history.join("events.sqlite3"),
        );
        let rt = Runtime::new(&RuntimeScope::fixture(dir.path()), ports).unwrap();
        let ctx = OpContext::uncancellable(CorrelationId("migration-refusal".to_owned()));
        let Err(error) = MutationSession::begin(&rt, &ctx) else {
            unreachable!("migration opened while an older desktop held history");
        };
        assert!(error
            .message
            .contains("Close the other Skill Studio process"));
        assert!(
            !leases.exists(),
            "failed startup created mutation lease files"
        );
        drop(older);
        let guard = acquire_exclusive(rt.ports.leases.as_ref(), &rt.scope).unwrap();
        drop(guard);
        crate::lock_data_folders(&[&history]).unwrap();
    }

    fn scope_for(home: &Path) -> NormalizedScope {
        let raw = RuntimeScope::fixture(home);
        NormalizedScope::normalize(&raw, &crate::fs::RealFs::new()).unwrap()
    }

    fn guard_for(dir: &Path, scope: &NormalizedScope) -> ExclusiveGuard {
        let leases = crate::lease::FileLease::new(dir.join("leases"));
        acquire_exclusive(&leases, scope).unwrap()
    }

    fn draft(
        kind: EventKind,
        skill: &str,
        payload: serde_json::Value,
        inverse: Option<serde_json::Value>,
    ) -> EventDraft {
        EventDraft {
            kind,
            skill: SkillName(skill.to_string()),
            harness: Some(AgentId::parse("claude-code").unwrap()),
            scope: Some("global".to_string()),
            project_path: None,
            payload,
            inverse,
            backup_dir: None,
        }
    }

    fn folder_bytes(root: &Path) -> std::collections::BTreeMap<PathBuf, (u64, Vec<u8>)> {
        fs::read_dir(root)
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                (
                    path.file_name().unwrap().into(),
                    (fs::metadata(&path).unwrap().len(), fs::read(path).unwrap()),
                )
            })
            .collect()
    }

    #[test]
    fn doctor_wal_without_sidecars_preserves_folder_or_creates_wal_and_shm() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("events.sqlite3");
        drop(SqliteHistoryStore::open(&path).unwrap());
        assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 2);
        assert!(tmp.path().join("schema_version").is_file());
        let before = folder_bytes(tmp.path());
        let store = SqliteHistoryOpener::new(&path)
            .open(&scope_for(tmp.path()), HistoryAccess::Inspect)
            .unwrap()
            .unwrap();
        assert!(store.doctor_pending(0).unwrap().is_empty());
        assert!(store.doctor_retention(0).unwrap().is_empty());
        drop(store);
        assert_eq!(folder_bytes(tmp.path()), before);
    }

    #[test]
    fn doctor_wal_with_sidecars_reports_skipped_or_writes_shared_memory() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("events.sqlite3");
        let writer = SqliteHistoryStore::open(&path).unwrap();
        writer.conn.execute("INSERT INTO events (id,ts,kind,skill,payload,status) VALUES ('wal-only','2026-01-01T00:00:00Z','park','alpha','{}','pending')", []).unwrap();
        assert!(path.with_extension("sqlite3-wal").exists());
        assert!(path.with_extension("sqlite3-shm").exists());
        let before = folder_bytes(tmp.path());
        let result =
            SqliteHistoryOpener::new(&path).open(&scope_for(tmp.path()), HistoryAccess::Inspect);
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("history busy, skipped"));
        assert_eq!(folder_bytes(tmp.path()), before);
    }

    #[test]
    fn doctor_pending_pages_identifiers_and_counts_all_or_decodes_unbounded_payloads() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteHistoryStore::open(&tmp.path().join("events.sqlite3")).unwrap();
        store.conn.execute_batch("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<4097) INSERT INTO events (id,ts,kind,skill,payload,inverse,status) SELECT x,'invalid','park','alpha','invalid JSON','invalid JSON',CASE WHEN x % 2 = 0 THEN 'interrupted' ELSE 'pending' END FROM n").unwrap();
        let mut after = 0;
        let mut total = 0;
        loop {
            let rows = store.doctor_pending(after).unwrap();
            assert!(rows.len() <= 256);
            if rows.is_empty() {
                break;
            }
            assert!(rows[0].0 > after);
            after = rows.last().unwrap().0;
            total += rows.len();
        }
        assert_eq!(total, 4097);
    }

    #[test]
    fn doctor_retention_uses_rowid_keyset_or_sorts_each_history_page() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteHistoryStore::open(&tmp.path().join("events.sqlite3")).unwrap();
        let mut statement = store
            .conn
            .prepare(&format!("EXPLAIN QUERY PLAN {DOCTOR_RETENTION_SQL}"))
            .unwrap();
        let plan: Vec<String> = statement
            .query_map([256], |row| row.get(3))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert!(
            plan.iter()
                .any(|line| line.contains("INTEGER PRIMARY KEY (rowid>?)")),
            "{plan:?}"
        );
        assert!(
            plan.iter().all(|line| !line.contains("TEMP B-TREE")),
            "{plan:?}"
        );
    }

    #[derive(Clone, Copy)]
    enum RetentionProtection {
        Count,
        FailedControl,
        Recent,
        NewestUndoable,
        NewestUndoableNeedsNoBackup,
        Restore(EventStatus),
    }

    fn retention_protects_one_backup_and_expires_the_control(
        protection: RetentionProtection,
    ) -> EventRecord {
        use skill_studio_core::backup_retention::{BACKUP_RETENTION_AGE, BACKUP_RETENTION_COUNT};
        let tmp = tempfile::tempdir().unwrap();
        let scope = scope_for(tmp.path());
        let guard = guard_for(tmp.path(), &scope);
        let mut store = SqliteHistoryStore::open(&tmp.path().join("events.sqlite3")).unwrap();
        let target = tmp.path().join(".agents/skills/alpha");
        fs::create_dir_all(&target).unwrap();
        let count = BACKUP_RETENTION_COUNT + 2;
        let mut ids = Vec::new();
        for index in 0..count {
            let id = EventId(format!("retention-{index:04}"));
            fs::write(target.join("SKILL.md"), format!("version {index}\n")).unwrap();
            let manifest = store
                .backup_paths(&guard, &id, std::slice::from_ref(&target))
                .unwrap();
            let fingerprint = manifest.entries[0].fingerprint.as_ref().unwrap().bare_hex();
            let mut row = draft(
                EventKind::Update,
                "alpha",
                if index == 0 {
                    serde_json::Value::Null
                } else {
                    serde_json::json!({})
                },
                Some(serde_json::json!({
                    "op":"restore_backup", "path":target, "pre_fingerprint":fingerprint, "post_fingerprint":fingerprint,
                })),
            );
            row.backup_dir = Some(manifest.backup_dir);
            store.record(&guard, &id, &row).unwrap();
            store
                .conn
                .execute(
                    "UPDATE events SET restorable = 0 WHERE id = ?1",
                    params![id.0],
                )
                .unwrap();
            if index + 1 < count {
                let status =
                    if matches!(protection, RetentionProtection::FailedControl) && index == 0 {
                        EventStatus::Failed
                    } else {
                        EventStatus::Done
                    };
                store.finish(&guard, &id, status, None).unwrap();
            }
            ids.push(id);
        }
        let now = Utc::now();
        for (index, id) in ids.iter().enumerate().take(count - 1) {
            let age = match protection {
                RetentionProtection::Recent if index > 0 => chrono::Duration::days(1),
                _ => BACKUP_RETENTION_AGE + chrono::Duration::days(1),
            };
            let timestamp = now - age + chrono::Duration::seconds(i64::try_from(index).unwrap());
            store
                .conn
                .execute(
                    "UPDATE events SET ts = ?1 WHERE id = ?2",
                    params![timestamp.to_rfc3339(), id.0],
                )
                .unwrap();
        }
        let (protected, control, expected_kept) = match protection {
            RetentionProtection::Count | RetentionProtection::FailedControl => {
                (count - BACKUP_RETENTION_COUNT, 0, BACKUP_RETENTION_COUNT)
            }
            RetentionProtection::Recent => (1, 0, count - 1),
            RetentionProtection::NewestUndoable => {
                store
                    .conn
                    .execute(
                        "UPDATE events SET restorable = 1 WHERE id = ?1",
                        params![ids[0].0],
                    )
                    .unwrap();
                (0, 1, count - 1)
            }
            RetentionProtection::NewestUndoableNeedsNoBackup => {
                store
                    .conn
                    .execute(
                        "UPDATE events SET restorable=1 WHERE id=?1",
                        params![ids[0].0],
                    )
                    .unwrap();
                let latest = EventId("latest-without-backup".into());
                store
                    .record(
                        &guard,
                        &latest,
                        &draft(
                            EventKind::Update,
                            "alpha",
                            serde_json::json!({}),
                            Some(serde_json::json!({"op":"restore_backup", "path": target})),
                        ),
                    )
                    .unwrap();
                store
                    .finish(&guard, &latest, EventStatus::Done, None)
                    .unwrap();
                (count - BACKUP_RETENTION_COUNT, 0, BACKUP_RETENTION_COUNT)
            }
            RetentionProtection::Restore(status) => {
                let holder = EventId("retention-restore-holder".into());
                store
                    .record(
                        &guard,
                        &holder,
                        &draft(
                            EventKind::Restore,
                            "alpha",
                            serde_json::json!({"target_event":ids[0].0}),
                            None,
                        ),
                    )
                    .unwrap();
                if status != EventStatus::Pending {
                    store.finish(&guard, &holder, status, None).unwrap();
                }
                (0, 1, count - 1)
            }
        };
        let live_before = fs::read(target.join("SKILL.md")).unwrap();
        store
            .finish(&guard, ids.last().unwrap(), EventStatus::Done, None)
            .unwrap();
        assert!(
            !store.backup_dir_for(&ids[control]).exists(),
            "an old unprotected backup must expire"
        );
        assert!(
            store
                .backup_dir_for(&ids[protected])
                .join("manifest.json")
                .is_file(),
            "the selected protection must keep its otherwise-expired backup"
        );
        assert_eq!(
            fs::read_dir(&store.backups_root).unwrap().count(),
            expected_kept
        );
        assert!(
            store.get(&ids[control]).unwrap().is_some(),
            "expiry keeps the history row"
        );
        assert_eq!(fs::read(target.join("SKILL.md")).unwrap(), live_before);
        store.get(&ids[control]).unwrap().unwrap()
    }

    #[test]
    fn an_old_failed_event_expires_under_the_same_limits_or_keeps_its_backup_forever() {
        let row = retention_protects_one_backup_and_expires_the_control(
            RetentionProtection::FailedControl,
        );
        assert_eq!(row.status, EventStatus::Failed);
        assert!(skill_studio_core::backup_retention::backup_is_expired(
            &row.payload
        ));
    }

    #[test]
    fn expired_legacy_payload_keeps_its_value_and_reason_or_drops_the_expiry_marker() {
        let row = retention_protects_one_backup_and_expires_the_control(RetentionProtection::Count);
        assert!(
            skill_studio_core::backup_retention::backup_is_expired(&row.payload),
            "legacy payload: {}",
            row.payload
        );
        assert_eq!(row.payload["original_payload"], serde_json::Value::Null);
    }

    #[test]
    fn expired_history_row_reports_backup_expired_or_looks_like_an_ordinary_non_undoable_row() {
        let row = retention_protects_one_backup_and_expires_the_control(RetentionProtection::Count);
        let dto = serde_json::to_value(row.to_dto()).unwrap();
        assert_eq!(dto["restore"]["restorable"], "backup_expired");
    }

    #[test]
    fn successful_write_keeps_the_newest_completed_backups_or_prunes_a_protected_count_entry() {
        retention_protects_one_backup_and_expires_the_control(RetentionProtection::Count);
    }

    #[test]
    fn successful_write_keeps_recent_backups_beyond_the_count_or_deletes_a_young_backup() {
        retention_protects_one_backup_and_expires_the_control(RetentionProtection::Recent);
    }

    #[test]
    fn successful_write_keeps_the_newest_undoable_backup_beyond_both_limits_or_loses_undo() {
        retention_protects_one_backup_and_expires_the_control(RetentionProtection::NewestUndoable);
    }

    #[test]
    fn a_newer_undoable_row_without_a_backup_does_not_pin_an_old_backup_or_keep_expired_disk_use() {
        retention_protects_one_backup_and_expires_the_control(
            RetentionProtection::NewestUndoableNeedsNoBackup,
        );
    }

    #[test]
    fn successful_write_keeps_a_pending_or_interrupted_restores_backup_or_breaks_recovery() {
        for status in [EventStatus::Pending, EventStatus::Interrupted] {
            retention_protects_one_backup_and_expires_the_control(RetentionProtection::Restore(
                status,
            ));
        }
    }

    fn retention_crash_fixture(
        directory: &Path,
    ) -> (
        SqliteHistoryStore,
        skill_studio_core::scope::NormalizedScope,
        ExclusiveGuard,
        PathBuf,
    ) {
        let scope = scope_for(directory);
        let guard = guard_for(directory, &scope);
        let mut store = SqliteHistoryStore::open(&directory.join("events.sqlite3")).unwrap();
        let target = directory.join(".agents/skills/alpha");
        fs::create_dir_all(&target).unwrap();
        for index in 0..skill_studio_core::backup_retention::BACKUP_RETENTION_COUNT + 2 {
            let id = EventId(format!("crash-{index:04}"));
            fs::write(target.join("SKILL.md"), format!("revision {index}\n")).unwrap();
            fs::write(target.join("notes.txt"), b"keep these user notes").unwrap();
            let manifest = store
                .backup_paths(&guard, &id, std::slice::from_ref(&target))
                .unwrap();
            let mut row = draft(
                EventKind::Update,
                "alpha",
                serde_json::json!({}),
                Some(serde_json::json!({"op":"restore_backup","path":target})),
            );
            row.backup_dir = Some(manifest.backup_dir);
            store.record(&guard, &id, &row).unwrap();
            store.finish(&guard, &id, EventStatus::Done, None).unwrap();
        }
        (store, scope, guard, target)
    }

    #[test]
    fn retention_crash_at_each_durable_step_preserves_files_and_recovery_or_exposes_a_partial_backup(
    ) {
        use skill_studio_core::backup_retention::{BackupRetentionScope, BACKUP_RETENTION_AGE};
        use skill_studio_core::testing::CrashFs;
        let now = Utc::now() + BACKUP_RETENTION_AGE + chrono::Duration::days(1);
        let baseline = tempfile::tempdir().unwrap();
        let (mut store, _, guard, _) = retention_crash_fixture(baseline.path());
        let counting = Arc::new(CrashFs::new(Arc::new(crate::RealFs::new()), 0));
        store.fs = counting.clone();
        store
            .retain_backups(&guard, now, BackupRetentionScope::All)
            .unwrap();
        let steps = counting.calls();
        assert!(
            steps > 0,
            "retention must use the filesystem port at durable boundaries"
        );
        for cut in 1..=steps {
            let directory = tempfile::tempdir().unwrap();
            let (mut store, _, guard, target) = retention_crash_fixture(directory.path());
            let user_before = hash_entry(&target).unwrap();
            store.fs = Arc::new(CrashFs::new(Arc::new(crate::RealFs::new()), cut));
            let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                store.retain_backups(&guard, now, BackupRetentionScope::All)
            }));
            assert!(
                crashed.is_err(),
                "cut {cut} did not interrupt a durable step"
            );
            let expected_rows = skill_studio_core::backup_retention::BACKUP_RETENTION_COUNT + 2;
            let rows = store
                .list(&EventFilter {
                    limit: expected_rows.try_into().unwrap(),
                    ..EventFilter::default()
                })
                .unwrap();
            assert_eq!(
                rows.len(),
                expected_rows,
                "the crash check must inspect every fixture row"
            );
            for row in rows {
                if let Some(ref backup_dir) = row.backup_dir {
                    let manifest = store.read_manifest(backup_dir);
                    assert!(
                        manifest.is_ok(),
                        "cut {cut}: usable row has no manifest: {manifest:?}"
                    );
                    let manifest = manifest.unwrap();
                    for entry in manifest.entries {
                        let actual =
                            hash_entry(&store.root.join(backup_dir).join(entry.relative)).unwrap();
                        assert_eq!(
                            Some(Fingerprint::parse(&actual).unwrap()),
                            entry.fingerprint,
                            "cut {cut}: usable row points to a partial backup"
                        );
                    }
                } else {
                    assert!(skill_studio_core::backup_retention::backup_is_expired(
                        &row.payload
                    ));
                }
            }
            drop(store);
            let mut recovered =
                SqliteHistoryStore::open(&directory.path().join("events.sqlite3")).unwrap();
            skill_studio_core::events::recover_interrupted(
                &guard,
                &mut recovered,
                &crate::RealFs::new(),
                &crate::NoopSink,
            )
            .unwrap();
            recovered
                .retain_backups(&guard, now, BackupRetentionScope::All)
                .unwrap();
            assert_eq!(
                hash_entry(&target).unwrap(),
                user_before,
                "cut {cut}: retention changed user files"
            );
            assert_eq!(
                fs::read_dir(&recovered.backups_root).unwrap().count(),
                skill_studio_core::backup_retention::BACKUP_RETENTION_COUNT
            );
            assert_eq!(
                recovered
                    .conn
                    .query_row("SELECT count(*) FROM expired_backups", [], |row| row
                        .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
    }

    #[test]
    #[ignore = "manual disk measurement: creates 1,000 real update backups"]
    fn thousand_updates_report_disk_use_before_and_after_retention_or_keep_expired_backups() {
        use skill_studio_core::backup_retention::{
            BackupRetentionScope, BACKUP_RETENTION_AGE, BACKUP_RETENTION_COUNT,
        };
        use std::os::unix::fs::MetadataExt as _;
        fn usage(root: &Path) -> (u64, u64) {
            let mut logical = 0;
            let mut allocated = 0;
            for entry in fs::read_dir(root).unwrap() {
                let entry = entry.unwrap();
                let metadata = entry.metadata().unwrap();
                if metadata.is_dir() {
                    let child = usage(&entry.path());
                    logical += child.0;
                    allocated += child.1;
                } else {
                    logical += metadata.len();
                    allocated += metadata.blocks() * 512;
                }
            }
            (logical, allocated)
        }
        let directory = tempfile::tempdir().unwrap();
        let scope = scope_for(directory.path());
        let guard = guard_for(directory.path(), &scope);
        let mut store = SqliteHistoryStore::open(&directory.path().join("events.sqlite3")).unwrap();
        let target = directory.path().join(".agents/skills/alpha");
        fs::create_dir_all(&target).unwrap();
        for index in 0..1000u64 {
            let mut bytes = vec![b'x'; 64 * 1024];
            bytes[..8].copy_from_slice(&index.to_le_bytes());
            fs::write(target.join("SKILL.md"), bytes).unwrap();
            let id = EventId(format!("update-{index:04}"));
            let manifest = store
                .backup_paths(&guard, &id, std::slice::from_ref(&target))
                .unwrap();
            let mut row = draft(
                EventKind::Update,
                "alpha",
                serde_json::json!({}),
                Some(serde_json::json!({"op":"restore_backup","path":target})),
            );
            row.backup_dir = Some(manifest.backup_dir);
            store.record(&guard, &id, &row).unwrap();
            store.finish(&guard, &id, EventStatus::Done, None).unwrap();
        }
        let before = usage(&store.backups_root);
        store
            .retain_backups(&guard, Utc::now(), BackupRetentionScope::All)
            .unwrap();
        let recent = usage(&store.backups_root);
        assert_eq!(recent, before, "all backups are younger than 30 days");
        store
            .retain_backups(
                &guard,
                Utc::now() + BACKUP_RETENTION_AGE + chrono::Duration::days(1),
                BackupRetentionScope::All,
            )
            .unwrap();
        let aged = usage(&store.backups_root);
        assert_eq!(
            fs::read_dir(&store.backups_root).unwrap().count(),
            BACKUP_RETENTION_COUNT
        );
        assert_eq!(
            store
                .conn
                .query_row("SELECT count(*) FROM events", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1000
        );
        println!("backup disk bytes: before logical={} allocated={}; recent logical={} allocated={}; aged logical={} allocated={}", before.0,before.1,recent.0,recent.1,aged.0,aged.1);
    }

    #[test]
    fn history_backup_reads_refuse_foreign_paths_or_expose_bytes_outside_the_store() {
        let directory = tempfile::tempdir().unwrap();
        let scope = scope_for(directory.path());
        let guard = guard_for(directory.path(), &scope);
        let mut store =
            SqliteHistoryStore::open(&directory.path().join("owned/events.sqlite3")).unwrap();
        let source = directory.path().join("user.txt");
        fs::write(&source, b"original user bytes").unwrap();
        let manifest = store
            .backup_paths(
                &guard,
                &EventId("safe".into()),
                std::slice::from_ref(&source),
            )
            .unwrap();
        assert_eq!(
            store
                .read_backup_bytes(&manifest.backup_dir, &manifest.entries[0].relative)
                .unwrap(),
            fs::read(&source).unwrap()
        );
        let foreign = directory.path().join("foreign");
        fs::create_dir(&foreign).unwrap();
        fs::write(foreign.join("secret.txt"), b"foreign bytes").unwrap();
        fs::write(foreign.join("manifest.json"), b"{\"entries\":{}}").unwrap();
        for backup_dir in ["../foreign", foreign.to_str().unwrap()] {
            assert!(
                store.read_manifest(backup_dir).is_err(),
                "foreign manifest was read: {backup_dir}"
            );
            assert!(store.read_backup_bytes(backup_dir, "secret.txt").is_err());
            assert!(store.read_backup_files(backup_dir, ".").is_err());
        }
        std::os::unix::fs::symlink(
            &foreign,
            store.backup_dir_for(&EventId("safe".into())).join("escape"),
        )
        .unwrap();
        std::os::unix::fs::symlink(
            foreign.join("secret.txt"),
            store
                .backup_dir_for(&EventId("safe".into()))
                .join("leaf-link"),
        )
        .unwrap();
        for relative in [
            "../../../foreign/secret.txt",
            "escape/secret.txt",
            "leaf-link",
        ] {
            assert!(
                store
                    .read_backup_bytes(&manifest.backup_dir, relative)
                    .is_err(),
                "foreign backup bytes were read through {relative}"
            );
        }
        assert!(store
            .read_backup_files(&manifest.backup_dir, "escape")
            .is_err());
        assert_eq!(
            fs::read(foreign.join("secret.txt")).unwrap(),
            b"foreign bytes"
        );
        let manifest_path = store
            .backup_dir_for(&EventId("safe".into()))
            .join("manifest.json");
        fs::remove_file(&manifest_path).unwrap();
        std::os::unix::fs::symlink(foreign.join("manifest.json"), &manifest_path).unwrap();
        assert!(
            store.read_manifest(&manifest.backup_dir).is_err(),
            "a manifest symlink exposes a foreign manifest"
        );
    }

    #[test]
    fn history_backup_ids_refuse_traversal_or_create_a_folder_outside_the_store() {
        let directory = tempfile::tempdir().unwrap();
        let scope = scope_for(directory.path());
        let guard = guard_for(directory.path(), &scope);
        let mut store =
            SqliteHistoryStore::open(&directory.path().join("owned/events.sqlite3")).unwrap();
        let source = directory.path().join("user.txt");
        fs::write(&source, b"keep").unwrap();
        store
            .backup_paths(
                &guard,
                &EventId("safe".into()),
                std::slice::from_ref(&source),
            )
            .unwrap();
        assert!(store
            .backup_paths(
                &guard,
                &EventId("../../foreign".into()),
                std::slice::from_ref(&source)
            )
            .is_err());
        assert!(!directory.path().join("foreign").exists());
        assert_eq!(fs::read(source).unwrap(), b"keep");
    }

    #[test]
    fn history_open_indexes_timestamp_pages_and_keeps_tie_order_or_sorts_the_whole_store() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("events.sqlite3");
        let store = SqliteHistoryStore::open(&path).unwrap();
        store
            .conn
            .execute("DROP INDEX IF EXISTS idx_events_ts", [])
            .unwrap();
        let fixtures = [
            ("old", "2026-01-01T00:00:00Z"),
            ("tied-first", "2026-02-01T00:00:00Z"),
            ("tied-last", "2026-02-01T00:00:00Z"),
        ];
        for (id, ts) in fixtures {
            store.conn.execute("INSERT INTO events (id,ts,kind,skill,payload,status) VALUES (?1,?2,'install','alpha','{}','done')", params![id, ts]).unwrap();
        }
        drop(store);
        let store = SqliteHistoryStore::open(&path).unwrap();
        let plan: Vec<String> = store
            .conn
            .prepare("EXPLAIN QUERY PLAN SELECT * FROM events ORDER BY ts DESC,rowid DESC LIMIT 2")
            .unwrap()
            .query_map([], |row| row.get(3))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(
            plan.iter().any(|line| line.contains("idx_events_ts")),
            "timestamp index missing: {plan:?}"
        );
        assert!(
            !plan.iter().any(|line| line.contains("TEMP B-TREE")),
            "page still sorts the store: {plan:?}"
        );
        let rows = store
            .list(&EventFilter {
                skill: None,
                limit: 2,
                after: None,
            })
            .unwrap();
        assert_eq!(
            rows.iter().map(|row| row.id.0.as_str()).collect::<Vec<_>>(),
            fixtures
                .iter()
                .rev()
                .take(2)
                .map(|(id, _)| *id)
                .collect::<Vec<_>>()
        );
        let tail = store
            .list(&EventFilter {
                skill: None,
                limit: 2,
                after: Some(rows[1].id.clone()),
            })
            .unwrap();
        assert_eq!(
            tail.iter().map(|row| row.id.0.as_str()).collect::<Vec<_>>(),
            vec![fixtures[0].0]
        );
    }

    std::thread_local! {
        static PATCH_WAIT: std::cell::RefCell<Option<(std::sync::mpsc::Sender<()>,std::sync::mpsc::Receiver<()>)>> = const { std::cell::RefCell::new(None) };
    }

    fn wait_for_competing_patch(_: i32) -> bool {
        PATCH_WAIT.with(|slot| match slot.borrow_mut().take() {
            Some((waiting, release)) => waiting.send(()).is_ok() && release.recv().is_ok(),
            None => false,
        })
    }

    #[test]
    fn concurrent_history_patches_keep_both_payload_and_inverse_keys_or_lose_a_committed_edit() {
        for column in [JsonColumn::Payload, JsonColumn::Inverse] {
            let tmp = tempfile::tempdir().unwrap();
            let scope = scope_for(tmp.path());
            let guard = guard_for(tmp.path(), &scope);
            let path = tmp.path().join("events.sqlite3");
            let mut store = SqliteHistoryStore::open(&path).unwrap();
            let id = EventId("patch-target".into());
            store
                .record(
                    &guard,
                    &id,
                    &draft(
                        EventKind::Install,
                        "alpha",
                        serde_json::json!({"base":true}),
                        Some(serde_json::json!({"base":true})),
                    ),
                )
                .unwrap();
            let mut first = Connection::open(&path).unwrap();
            let (ready_tx, ready_rx) = std::sync::mpsc::channel();
            let (start_tx, start_rx) = std::sync::mpsc::channel();
            let (waiting_tx, waiting_rx) = std::sync::mpsc::channel();
            let (release_tx, release_rx) = std::sync::mpsc::channel();
            let worker = std::thread::spawn(move || {
                store
                    .conn
                    .busy_handler(Some(wait_for_competing_patch))
                    .unwrap();
                PATCH_WAIT.with(|slot| slot.replace(Some((waiting_tx, release_rx))));
                ready_tx.send(()).unwrap();
                start_rx.recv().unwrap();
                let result = match column {
                    JsonColumn::Payload => {
                        store.patch_payload(&guard, &id, serde_json::json!({"second":true}))
                    }
                    JsonColumn::Inverse => {
                        store.patch_inverse(&guard, &id, serde_json::json!({"second":true}))
                    }
                };
                result.unwrap();
                let row = store.get(&id).unwrap().unwrap();
                match column {
                    JsonColumn::Payload => row.payload,
                    JsonColumn::Inverse => row.inverse.unwrap(),
                }
            });
            ready_rx.recv().unwrap();
            let transaction = first
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            start_tx.send(()).unwrap();
            waiting_rx.recv().unwrap();
            let sql = match column {
                JsonColumn::Payload => "UPDATE events SET payload = '{\"base\":true,\"first\":true}' WHERE id = 'patch-target'",
                JsonColumn::Inverse => "UPDATE events SET inverse = '{\"base\":true,\"first\":true}' WHERE id = 'patch-target'",
            };
            transaction.execute(sql, []).unwrap();
            transaction.commit().unwrap();
            release_tx.send(()).unwrap();
            assert_eq!(
                worker.join().unwrap(),
                serde_json::json!({"base":true,"first":true,"second":true})
            );
        }
    }

    #[test]
    fn read_if_exists_returns_none_for_a_missing_file_and_never_creates_it() {
        let tmp = tempfile::tempdir().unwrap();
        let scope = scope_for(tmp.path());
        let db_path = tmp.path().join("history").join("events.sqlite3");
        let opener = SqliteHistoryOpener::new(&db_path);

        let store = opener.open(&scope, HistoryAccess::ReadIfExists).unwrap();
        assert!(store.is_none());
        assert!(!db_path.exists());
        assert!(!tmp.path().join("history").exists());
    }

    #[test]
    fn events_read_preserves_unversioned_schema_or_migrates_history() {
        let tmp = tempfile::tempdir().unwrap();
        let scope = scope_for(tmp.path());
        let folder = tmp.path().join("history");
        fs::create_dir(&folder).unwrap();
        let path = folder.join("events.sqlite3");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(SCHEMA_SQL).unwrap();
        conn.execute_batch("DROP TABLE materialized_disabled; DROP TABLE materialized_roots;")
            .unwrap();
        drop(conn);
        let before = fs::read(&path).unwrap();
        let store = SqliteHistoryOpener::new(&path)
            .open(&scope, HistoryAccess::ReadIfExists)
            .unwrap()
            .unwrap();
        assert!(store
            .list(&EventFilter {
                skill: None,
                limit: 10,
                after: None
            })
            .unwrap()
            .is_empty());
        assert!(!folder.join("schema_version").exists());
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(fs::read_dir(&folder).unwrap().count(), 1);
        drop(store);
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn read_write_creates_the_directory_and_schema() {
        let tmp = tempfile::tempdir().unwrap();
        let scope = scope_for(tmp.path());
        let db_path = tmp.path().join("history").join("events.sqlite3");
        let opener = SqliteHistoryOpener::new(&db_path);

        let store = opener.open(&scope, HistoryAccess::ReadWrite).unwrap();
        assert!(store.is_some());
        assert!(db_path.exists());

        // Reopening with ReadIfExists now finds the file the write created.
        let store = opener.open(&scope, HistoryAccess::ReadIfExists).unwrap();
        assert!(store.is_some());
    }

    #[test]
    fn record_then_list_round_trips_payload_and_inverse() {
        let tmp = tempfile::tempdir().unwrap();
        let scope = scope_for(tmp.path());
        let guard = guard_for(tmp.path(), &scope);
        let db_path = tmp.path().join("history").join("events.sqlite3");
        let mut store = SqliteHistoryStore::open(&db_path).unwrap();

        let id = EventId::from_ulid(ulid::Ulid::generate());
        let inverse =
            serde_json::json!({"op": "recreate_symlink", "pre_fingerprint": "sha256:abc"});
        store
            .record(
                &guard,
                &id,
                &draft(
                    EventKind::Install,
                    "alpha",
                    serde_json::json!({"n": 1}),
                    Some(inverse.clone()),
                ),
            )
            .unwrap();
        store.finish(&guard, &id, EventStatus::Done, None).unwrap();

        let rows = store
            .list(&EventFilter {
                skill: None,
                limit: 10,
                after: None,
            })
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, id);
        assert_eq!(rows[0].kind, "install");
        assert_eq!(rows[0].skill, SkillName("alpha".to_string()));
        assert_eq!(rows[0].payload, serde_json::json!({"n": 1}));
        assert_eq!(rows[0].inverse, Some(inverse));
        assert_eq!(rows[0].status, EventStatus::Done);

        let fetched = store.get(&id).unwrap().unwrap();
        assert_eq!(fetched.id, id);
    }

    #[test]
    fn after_pages_backwards_through_older_rows() {
        let tmp = tempfile::tempdir().unwrap();
        let scope = scope_for(tmp.path());
        let guard = guard_for(tmp.path(), &scope);
        let db_path = tmp.path().join("history").join("events.sqlite3");
        let mut store = SqliteHistoryStore::open(&db_path).unwrap();

        let mut ids = Vec::new();
        for i in 0..3 {
            let id = EventId::from_ulid(ulid::Ulid::generate());
            store
                .record(
                    &guard,
                    &id,
                    &draft(
                        EventKind::Install,
                        "alpha",
                        serde_json::json!({"n": i}),
                        None,
                    ),
                )
                .unwrap();
            store.finish(&guard, &id, EventStatus::Done, None).unwrap();
            ids.push(id);
        }
        // Newest first: ids[2], ids[1], ids[0].
        let first_page = store
            .list(&EventFilter {
                skill: None,
                limit: 1,
                after: None,
            })
            .unwrap();
        assert_eq!(first_page.len(), 1);
        assert_eq!(first_page[0].id, ids[2]);

        let second_page = store
            .list(&EventFilter {
                skill: None,
                limit: 10,
                after: Some(ids[2].clone()),
            })
            .unwrap();
        assert_eq!(
            second_page.iter().map(|r| r.id.clone()).collect::<Vec<_>>(),
            vec![ids[1].clone(), ids[0].clone()]
        );
    }

    #[test]
    fn claim_revert_twice_returns_false_the_second_time() {
        let tmp = tempfile::tempdir().unwrap();
        let scope = scope_for(tmp.path());
        let guard = guard_for(tmp.path(), &scope);
        let db_path = tmp.path().join("history").join("events.sqlite3");
        let mut store = SqliteHistoryStore::open(&db_path).unwrap();

        let target = EventId::from_ulid(ulid::Ulid::generate());
        store
            .record(
                &guard,
                &target,
                &draft(EventKind::Remove, "alpha", serde_json::json!({}), None),
            )
            .unwrap();
        store
            .finish(&guard, &target, EventStatus::Done, None)
            .unwrap();

        let first_restore = EventId::from_ulid(ulid::Ulid::generate());
        let second_restore = EventId::from_ulid(ulid::Ulid::generate());
        assert!(store.claim_revert(&guard, &target, &first_restore).unwrap());
        assert!(!store
            .claim_revert(&guard, &target, &second_restore)
            .unwrap());

        let row = store.get(&target).unwrap().unwrap();
        assert_eq!(row.reverted_by, Some(first_restore));
    }

    #[test]
    fn release_revert_clears_the_claim_it_holds() {
        let tmp = tempfile::tempdir().unwrap();
        let scope = scope_for(tmp.path());
        let guard = guard_for(tmp.path(), &scope);
        let db_path = tmp.path().join("history").join("events.sqlite3");
        let mut store = SqliteHistoryStore::open(&db_path).unwrap();

        let target = EventId::from_ulid(ulid::Ulid::generate());
        store
            .record(
                &guard,
                &target,
                &draft(EventKind::Remove, "alpha", serde_json::json!({}), None),
            )
            .unwrap();
        store
            .finish(&guard, &target, EventStatus::Done, None)
            .unwrap();

        let restore = EventId::from_ulid(ulid::Ulid::generate());
        assert!(store.claim_revert(&guard, &target, &restore).unwrap());
        assert!(store.release_revert(&guard, &target, &restore).unwrap());

        let row = store.get(&target).unwrap().unwrap();
        assert_eq!(row.reverted_by, None);
    }

    #[test]
    fn release_revert_returns_false_when_another_restore_holds_the_claim() {
        let tmp = tempfile::tempdir().unwrap();
        let scope = scope_for(tmp.path());
        let guard = guard_for(tmp.path(), &scope);
        let db_path = tmp.path().join("history").join("events.sqlite3");
        let mut store = SqliteHistoryStore::open(&db_path).unwrap();

        let target = EventId::from_ulid(ulid::Ulid::generate());
        store
            .record(
                &guard,
                &target,
                &draft(EventKind::Remove, "alpha", serde_json::json!({}), None),
            )
            .unwrap();
        store
            .finish(&guard, &target, EventStatus::Done, None)
            .unwrap();

        let holder = EventId::from_ulid(ulid::Ulid::generate());
        let impostor = EventId::from_ulid(ulid::Ulid::generate());
        assert!(store.claim_revert(&guard, &target, &holder).unwrap());
        assert!(!store.release_revert(&guard, &target, &impostor).unwrap());

        let row = store.get(&target).unwrap().unwrap();
        assert_eq!(row.reverted_by, Some(holder));
    }

    /// Flow: an older desktop build wrote `harness = "Claude Code"` on a
    /// conversion and on its Undo, then crashed mid-Undo, so startup left the
    /// restore row `interrupted` with its claim held. Expect the next
    /// mutation session's recovery to release the claim and succeed.
    /// Failure: recovery decodes the label as an agent id, errors, and every
    /// later Install, Update, Remove, or Park fails with it.
    #[test]
    fn recovery_releases_a_claim_that_an_old_desktop_restore_holds_or_every_mutation_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let scope = scope_for(tmp.path());
        let guard = guard_for(tmp.path(), &scope);
        let db_path = tmp.path().join("history").join("events.sqlite3");
        let mut store = SqliteHistoryStore::open(&db_path).unwrap();
        store
            .conn
            .execute_batch(
                "INSERT INTO events (id, ts, kind, skill, harness, payload, status, reverted_by)
                 VALUES ('convert1', '2026-09-09T00:00:00Z', 'convert_to_shared', 'alpha',
                         'Claude Code', '{}', 'done', 'undo1');
                 INSERT INTO events (id, ts, kind, skill, harness, payload, status)
                 VALUES ('undo1', '2026-09-09T00:01:00Z', 'restore', 'alpha', 'Claude Code',
                         '{\"target_event\":\"convert1\"}', 'interrupted');",
            )
            .unwrap();

        skill_studio_core::events::recover_interrupted(
            &guard,
            &mut store,
            &crate::fs::RealFs,
            &crate::sink::NoopSink,
        )
        .unwrap();

        let reverted_by: Option<String> = store
            .conn
            .query_row(
                "SELECT reverted_by FROM events WHERE id = 'convert1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(reverted_by, None);
    }

    #[test]
    fn a_row_with_an_unknown_kind_still_loads_and_is_not_restorable() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("history").join("events.sqlite3");
        let store = SqliteHistoryStore::open(&db_path).unwrap();
        store
            .conn
            .execute(
                "INSERT INTO events (id, ts, kind, skill, payload, inverse, status)
                 VALUES ('future1', '2026-09-09T00:00:00Z', 'future_kind', 'x', '{}', '{}', 'done')",
                [],
            )
            .unwrap();

        let row = store.get(&EventId("future1".to_string())).unwrap().unwrap();
        assert_eq!(row.kind(), None);
        assert_eq!(
            row.restore_capability(),
            skill_studio_core::dto::RestoreCapability::UnknownKind
        );
    }

    #[test]
    fn an_existing_desktop_schema_database_opens_without_migration() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("history").join("events.sqlite3");
        fs::create_dir_all(db_path.parent().unwrap()).unwrap();

        // Copied verbatim from the desktop's `event_store::open`.
        let legacy = Connection::open(&db_path).unwrap();
        legacy
            .execute_batch(
                "CREATE TABLE events (
                    id          TEXT PRIMARY KEY,
                    ts          TEXT NOT NULL,
                    kind        TEXT NOT NULL,
                    skill       TEXT NOT NULL,
                    harness     TEXT,
                    scope       TEXT,
                    project_path TEXT,
                    payload     TEXT NOT NULL,
                    inverse     TEXT,
                    backup_dir  TEXT,
                    status      TEXT NOT NULL,
                    reverted_by TEXT REFERENCES events(id),
                    restorable  INTEGER NOT NULL DEFAULT 1
                );
                CREATE INDEX idx_events_skill ON events(skill, ts DESC);

                CREATE TABLE materialized_roots (
                    root_path   TEXT PRIMARY KEY,
                    harness     TEXT NOT NULL,
                    shared_root TEXT NOT NULL,
                    created_by  TEXT REFERENCES events(id)
                );
                CREATE TABLE materialized_disabled (
                    root_path   TEXT NOT NULL REFERENCES materialized_roots(root_path),
                    skill       TEXT NOT NULL,
                    PRIMARY KEY (root_path, skill)
                );
                INSERT INTO events (id, ts, kind, skill, payload, status)
                VALUES ('legacy', '2026-01-01T00:00:00Z', 'install', 'old', '{}', 'done');",
            )
            .unwrap();
        drop(legacy);

        let store = SqliteHistoryStore::open(&db_path).unwrap();
        let rows = store
            .list(&EventFilter {
                skill: None,
                limit: 10,
                after: None,
            })
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, EventId("legacy".to_string()));
        assert_eq!(rows[0].kind, "install");
    }

    #[test]
    fn a_manifest_written_by_the_desktops_code_path_is_read_correctly_including_an_absent_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("backups").join("evt1");
        fs::create_dir_all(&dir).unwrap();
        // Byte-for-byte what the desktop's `EventStore::backup_paths` would
        // write for two paths, one present (index 0) and one absent.
        fs::write(
            dir.join("manifest.json"),
            r#"{
  "entries": {
    "/skills/alpha": {
      "relative_path": "0-alpha",
      "fingerprint": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85"
    },
    "/skills/gone": {
      "relative_path": "",
      "fingerprint": "absent"
    }
  }
}"#,
        )
        .unwrap();

        let manifest = read_manifest(&dir).unwrap();
        assert_eq!(manifest.entries.len(), 2);
        let present = &manifest.entries["/skills/alpha"];
        assert_eq!(present.relative_path, "0-alpha");
        assert_eq!(
            present.fingerprint,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85"
        );
        let absent = &manifest.entries["/skills/gone"];
        assert_eq!(absent.relative_path, "");
        assert_eq!(absent.fingerprint, "absent");
    }

    #[test]
    fn a_manifest_this_adapter_writes_parses_into_the_desktops_struct_shape() {
        let tmp = tempfile::tempdir().unwrap();
        let scope = scope_for(tmp.path());
        let guard = guard_for(tmp.path(), &scope);
        let db_path = tmp.path().join("history").join("events.sqlite3");
        let mut store = SqliteHistoryStore::open(&db_path).unwrap();

        let present = tmp.path().join("skills").join("alpha");
        fs::create_dir_all(&present).unwrap();
        fs::write(present.join("SKILL.md"), b"hello").unwrap();
        let absent = tmp.path().join("skills").join("gone");

        let id = EventId::from_ulid(ulid::Ulid::generate());
        store
            .backup_paths(&guard, &id, &[present.clone(), absent.clone()])
            .unwrap();

        // Re-parse the manifest.json this adapter just wrote using a local
        // copy of the desktop's exact struct shape (field names, map keyed
        // by lossy absolute path), independent of this module's own reader.
        #[derive(Debug, Default, Deserialize)]
        struct DesktopShapeManifest {
            entries: std::collections::HashMap<String, DesktopShapeEntry>,
        }
        #[derive(Debug, Deserialize)]
        struct DesktopShapeEntry {
            relative_path: String,
            fingerprint: String,
        }

        let manifest_path = tmp
            .path()
            .join("history")
            .join("backups")
            .join(&id.0)
            .join("manifest.json");
        let bytes = fs::read(&manifest_path).unwrap();
        let parsed: DesktopShapeManifest = serde_json::from_slice(&bytes).unwrap();

        let present_key = present.to_string_lossy().into_owned();
        let absent_key = absent.to_string_lossy().into_owned();
        assert_eq!(parsed.entries[&present_key].relative_path, "0-alpha");
        assert_ne!(parsed.entries[&present_key].fingerprint, "absent");
        assert_eq!(parsed.entries[&absent_key].relative_path, "");
        assert_eq!(parsed.entries[&absent_key].fingerprint, "absent");
    }

    #[test]
    fn pending_lists_only_rows_still_pending_or_drops_them_all() {
        let tmp = tempfile::tempdir().unwrap();
        let scope = scope_for(tmp.path());
        let guard = guard_for(tmp.path(), &scope);
        let db_path = tmp.path().join("history").join("events.sqlite3");
        let mut store = SqliteHistoryStore::open(&db_path).unwrap();

        let finished = EventId::from_ulid(ulid::Ulid::generate());
        store
            .record(
                &guard,
                &finished,
                &draft(EventKind::Install, "alpha", serde_json::json!({}), None),
            )
            .unwrap();
        store
            .finish(&guard, &finished, EventStatus::Done, None)
            .unwrap();

        let still_pending = EventId::from_ulid(ulid::Ulid::generate());
        store
            .record(
                &guard,
                &still_pending,
                &draft(EventKind::Install, "beta", serde_json::json!({}), None),
            )
            .unwrap();

        let rows = store.pending().unwrap();
        assert_eq!(
            rows.iter().map(|r| r.id.clone()).collect::<Vec<_>>(),
            vec![still_pending],
            "pending() must list only the row still pending, not the finished one and not \
             an empty list"
        );
    }

    #[test]
    fn read_manifest_marks_an_absent_path_with_no_fingerprint_and_a_backed_up_dir_as_a_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let scope = scope_for(tmp.path());
        let guard = guard_for(tmp.path(), &scope);
        let db_path = tmp.path().join("history").join("events.sqlite3");
        let mut store = SqliteHistoryStore::open(&db_path).unwrap();

        let present_dir = tmp.path().join("skills").join("alpha");
        fs::create_dir_all(&present_dir).unwrap();
        fs::write(present_dir.join("SKILL.md"), b"hello").unwrap();
        let absent = tmp.path().join("skills").join("gone");

        let id = EventId::from_ulid(ulid::Ulid::generate());
        let written = store
            .backup_paths(&guard, &id, &[present_dir.clone(), absent.clone()])
            .unwrap();

        let manifest = store.read_manifest(&written.backup_dir).unwrap();
        let present_entry = manifest
            .entries
            .iter()
            .find(|e| e.original == present_dir)
            .expect("the backed-up directory must round-trip");
        let absent_entry = manifest
            .entries
            .iter()
            .find(|e| e.original == absent)
            .expect("the absent path must round-trip");

        assert!(
            present_entry.fingerprint.is_some(),
            "a path that existed at backup time must keep a fingerprint, or this test proves \
             nothing about reading the on-disk `\"absent\"` marker as `None`"
        );
        assert!(
            present_entry.is_dir,
            "a backed-up directory's copy is itself a directory on disk, or this test proves \
             nothing about reading that shape back as `is_dir`"
        );
        assert!(
            absent_entry.fingerprint.is_none(),
            "an absent path's on-disk `\"absent\"` marker must read back as no fingerprint, \
             not as a real one"
        );
        assert!(
            !absent_entry.is_dir,
            "an absent path has no relative copy on disk and must never read as a directory"
        );
    }

    #[test]
    fn backup_over_the_size_cap_is_refused_before_any_copy_or_fills_the_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let scope = scope_for(tmp.path());
        let guard = guard_for(tmp.path(), &scope);
        let db_path = tmp.path().join("history").join("events.sqlite3");
        let mut store = SqliteHistoryStore::open(&db_path).unwrap();

        let skill = tmp.path().join("skills").join("big");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), b"small").unwrap();
        fs::File::create(skill.join("blob.bin"))
            .unwrap()
            .set_len(MAX_BACKUP_BYTES + 1)
            .unwrap();

        let id = EventId::from_ulid(ulid::Ulid::generate());
        let error = store
            .backup_paths(&guard, &id, std::slice::from_ref(&skill))
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidRequest);
        assert!(
            !store.backup_dir_for(&id).exists(),
            "a refused backup must not leave a partial copy behind"
        );
    }

    #[test]
    fn read_backup_bytes_returns_the_copys_real_bytes_or_an_empty_or_placeholder_buffer() {
        let tmp = tempfile::tempdir().unwrap();
        let scope = scope_for(tmp.path());
        let guard = guard_for(tmp.path(), &scope);
        let db_path = tmp.path().join("history").join("events.sqlite3");
        let mut store = SqliteHistoryStore::open(&db_path).unwrap();

        let present = tmp.path().join("skills").join("SKILL.md");
        fs::create_dir_all(present.parent().unwrap()).unwrap();
        fs::write(&present, b"the real backed-up bytes").unwrap();

        let id = EventId::from_ulid(ulid::Ulid::generate());
        let written = store
            .backup_paths(&guard, &id, std::slice::from_ref(&present))
            .unwrap();
        let entry = &written.entries[0];

        let bytes = store
            .read_backup_bytes(&written.backup_dir, &entry.relative)
            .unwrap();
        assert_eq!(
            bytes, b"the real backed-up bytes",
            "read_backup_bytes must return the copy's real bytes, not an empty or \
             placeholder buffer"
        );
    }

    #[test]
    fn read_backup_files_walks_every_file_under_a_backed_up_dir_or_returns_none_of_them() {
        let tmp = tempfile::tempdir().unwrap();
        let scope = scope_for(tmp.path());
        let guard = guard_for(tmp.path(), &scope);
        let db_path = tmp.path().join("history").join("events.sqlite3");
        let mut store = SqliteHistoryStore::open(&db_path).unwrap();

        let present_dir = tmp.path().join("skills").join("alpha");
        fs::create_dir_all(present_dir.join("nested")).unwrap();
        fs::write(present_dir.join("SKILL.md"), b"top").unwrap();
        fs::write(present_dir.join("nested").join("more.md"), b"nested").unwrap();

        let id = EventId::from_ulid(ulid::Ulid::generate());
        let written = store
            .backup_paths(&guard, &id, std::slice::from_ref(&present_dir))
            .unwrap();
        let entry = &written.entries[0];

        let mut files = store
            .read_backup_files(&written.backup_dir, &entry.relative)
            .unwrap();
        files.sort_by(|a, b| a.relative.cmp(&b.relative));
        let names: Vec<_> = files.iter().map(|f| f.relative.clone()).collect();
        assert_eq!(
            names,
            vec![
                PathBuf::from("SKILL.md"),
                PathBuf::from("nested").join("more.md"),
            ],
            "read_backup_files must walk every file under the backed-up directory, not return \
             none of them"
        );
        assert_eq!(
            files
                .iter()
                .find(|f| f.relative == Path::new("SKILL.md"))
                .unwrap()
                .bytes,
            b"top",
            "each entry's bytes must be the real file contents, not a placeholder"
        );
    }
}
