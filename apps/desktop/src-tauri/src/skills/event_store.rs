// ============================================================================
// Skills Module - event_store
// Append-only log of every mutating operation Skill Studio performs
// (install/remove/park/harness-disable/etc.), plus the byte backups those
// mutations displace. Every mutating command follows the same five phases:
// allocate an id, back up anything about to be destroyed, record a
// `pending` row, perform the mutation, then `finish` the row `done` or
// `failed`. A crash leaves a `pending` row that `reconcile_at_startup`
// flips to `interrupted` on the next launch, so nothing silently vanishes.
//
// Restore semantics: each restorable event's `inverse` JSON is a tagged
// `InverseOp` carrying `pre_fingerprint` (the state the destination had
// *before* the original mutation - what restoring should bring back) and
// `post_fingerprint` (the state the mutation *left behind* - what the
// filesystem should still look like right before a restore runs). The
// drift guard in `restore` compares the destination's live fingerprint
// against `post_fingerprint`; a mismatch means something touched the path
// since the event, and restore refuses unless `force`. Restore is itself a
// mutation: before applying the inverse it backs up whatever currently sits
// at the destination (drifted or not) under its own event id and inserts a
// `restore` event with its own `RestoreBackup` inverse. Most restore events
// can themselves be restored. A caller can mark one non-restorable when
// recreating bytes cannot recreate the matching ownership metadata.
// ============================================================================

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use skill_studio_core::events::EventStatus as CoreEventStatus;
use skill_studio_host::{open_confined_child, write_confined_file, ConfinedDir};

use super::skill_md_write::{begin_skill_md_write_transaction, SkillMdWriteTransaction};
use super::undo_files::{canonical_undo_parent, UndoFiles, UndoPath};
use super::write_lease::WriteLeaseGuard;

const UNDO_LINKED_PARENTS: &str = "undo_linked_parents";

/// Opens (creating if absent) the event store DB at `db_path` and ensures
/// its schema exists.
pub fn open(db_path: &Path) -> Result<GuardedConnection, String> {
    let parent = db_path.parent().unwrap_or_else(|| Path::new("."));
    let guard = skill_studio_host::lock_data_folders(&[parent])?;
    Ok(GuardedConnection {
        conn: open_locked(db_path)?,
        _guard: guard,
    })
}

pub struct GuardedConnection {
    conn: Connection,
    _guard: skill_studio_host::DataFolderGuard,
}

impl std::ops::Deref for GuardedConnection {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        &self.conn
    }
}

impl std::ops::DerefMut for GuardedConnection {
    fn deref_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }
}

fn open_locked(db_path: &Path) -> Result<Connection, String> {
    let conn = Connection::open(db_path)
        .map_err(|e| format!("Failed to open {}: {e}", db_path.display()))?;
    // The desktop's `EventStore` and the core's `SqliteHistoryStore`
    // (`skill-studio-host/src/history.rs`) now open this same file from two
    // separate connections (one per process' worth of core `ops` calls, one
    // for the desktop's own direct writes). WAL lets both read concurrently,
    // but a writer still briefly locks the file; without a `busy_timeout`
    // the loser gets `SQLITE_BUSY` immediately instead of waiting its turn.
    // Set before `journal_mode = WAL` itself, since switching journal modes
    // is its own write that can hit a busy database - a CLI or MCP write in
    // flight at app launch could otherwise fail this whole open instead of
    // just waiting, leaving the app running its entire session with no
    // event store.
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| format!("Failed to set busy timeout: {e}"))?;
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| format!("Failed to set WAL mode: {e}"))?;
    // Earlier desktop builds claimed `reverted_by` before the
    // restore row existed, and an app that died in between left an id that
    // names no row. Foreign key enforcement on that column must stay off.
    conn.pragma_update(None, "foreign_keys", "OFF")
        .map_err(|e| format!("Failed to disable foreign keys: {e}"))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS events (
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
        );",
    )
    .map_err(|e| format!("Failed to create event store schema: {e}"))?;
    let has_restorable = conn
        .prepare("SELECT restorable FROM events LIMIT 0")
        .is_ok();
    if !has_restorable {
        conn.execute(
            "ALTER TABLE events ADD COLUMN restorable INTEGER NOT NULL DEFAULT 1",
            [],
        )
        .map_err(|e| format!("Failed to add events.restorable: {e}"))?;
    }
    let has_backup_dir = {
        let mut statement = conn
            .prepare("PRAGMA table_info(events)")
            .map_err(|e| format!("Failed to inspect event store schema: {e}"))?;
        let has_backup_dir = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| format!("Failed to query event store columns: {e}"))?
            .try_fold(false, |found, column| {
                column.map(|column| found || column == "backup_dir")
            })
            .map_err(|e| format!("Failed to read event store columns: {e}"))?;
        has_backup_dir
    };
    if !has_backup_dir {
        conn.execute("ALTER TABLE events ADD COLUMN backup_dir TEXT", [])
            .map_err(|e| format!("Failed to add events.backup_dir: {e}"))?;
    }
    Ok(conn)
}

#[cfg(test)]
type HistoryObserver = Box<dyn Fn(&str) + Send>;

/// Owns the event store connection plus the app data dir its backups live
/// under (`<app_data>/backups/<event-id>/`).
///
/// `journal` is `skill-studio-core`'s reference [`Journal`] implementation
/// (unit 1.2), rooted at `<app_data>/journal`: this struct is the host
/// implementation of that port, in place of a separate desktop-only
/// concept - see the `impl Journal for EventStore` below, which delegates
/// every method straight to `self.journal` rather than reimplementing
/// plan/step/backup storage on `SQLite`. `FsJournal` already gets the
/// manifest-before-plan durability order right and is exercised by
/// `skill-studio-core`'s own journal tests; a second, SQLite-backed
/// implementation here would only duplicate that logic, untested. Nothing
/// here reroutes the five-phase `events` table write path above through it
/// yet - that adoption is a later slice (see
/// `docs/flows.md`) - but `lib.rs`'s startup
/// reconciliation now calls `skill_studio_core::journal::reconcile`
/// against this store directly.
pub struct EventStore {
    pub conn: GuardedConnection,
    pub app_data: PathBuf,
    journal: skill_studio_core::journal::FsJournal,
    #[cfg(test)]
    pub(crate) observe_history: Option<HistoryObserver>,
}

impl EventStore {
    /// Opens `<app_data>/events.sqlite3`, creating `app_data` if needed.
    /// Kept for tests and anything that has never had a shared history
    /// database to migrate onto; the real app calls [`Self::open_with_db`]
    /// so its backups still live under `app_data` while the connection
    /// itself points at the core's shared history file.
    pub fn open(app_data: &Path) -> Result<Self, String> {
        Self::open_with_db(app_data, &app_data.join("events.sqlite3"))
    }

    /// Opens `db_path` as the event log, while still rooting backups and the
    /// journal under `app_data` - the split that lets the desktop keep its
    /// own backup/journal directories after its event *table* moved onto the
    /// core's shared `<data_root>/history/events.sqlite3` (`core_runtime::
    /// history_db_path`), so Activity and Undo see every core `ops` mutation
    /// alongside the desktop's own.
    pub fn open_with_db(app_data: &Path, db_path: &Path) -> Result<Self, String> {
        let parent = db_path.parent().unwrap_or_else(|| Path::new("."));
        let guard = skill_studio_host::lock_data_folders(&[app_data, parent])?;
        let conn = GuardedConnection {
            conn: open_locked(db_path)?,
            _guard: guard,
        };
        let journal = skill_studio_core::journal::FsJournal::new(
            app_data.join("journal"),
            std::sync::Arc::new(skill_studio_host::RealFs::new()),
        );
        Ok(Self {
            conn,
            app_data: app_data.to_path_buf(),
            journal,
            #[cfg(test)]
            observe_history: None,
        })
    }

    /// One-time import of the desktop's pre-migration event log
    /// (`<app_data>/events.sqlite3`, from before the desktop and core shared
    /// one history file) into the database this store already has open.
    /// Skips rows whose id already exists (so a second run imports nothing
    /// new), runs as one transaction, and renames the legacy file to
    /// `events.sqlite3.migrated` only once every row has copied over -
    /// leaving it in place (for the next launch to retry) if anything
    /// failed. A missing or corrupt legacy file is not an error: this
    /// returns `Ok(0)` rather than block startup.
    pub fn import_legacy_events(&self) -> Result<usize, String> {
        let legacy_path = self.app_data.join("events.sqlite3");
        if !legacy_path.exists() {
            return Ok(0);
        }
        if let Some(current) = self.conn.path() {
            if Path::new(current) == legacy_path {
                // The connection this store already holds *is* the legacy
                // file (no shared history database configured) - nothing to
                // import from itself.
                return Ok(0);
            }
        }
        // Opening the legacy file first applies any pending schema
        // migrations (e.g. the `restorable`/`backup_dir` ALTER TABLEs) to it,
        // so the ATTACHed copy below has the same columns as the live table.
        // A corrupt legacy file fails here, before anything is attached or
        // touched, and is reported without blocking startup.
        drop(open(&legacy_path)?);
        let legacy_str = legacy_path.to_str().ok_or_else(|| {
            format!(
                "Non-UTF-8 legacy event store path: {}",
                legacy_path.display()
            )
        })?;
        self.conn
            .execute("ATTACH DATABASE ?1 AS legacy", params![legacy_str])
            .map_err(|e| format!("Failed to attach legacy event store: {e}"))?;
        let import_result = (|| -> Result<usize, String> {
            self.conn
                .execute_batch("BEGIN IMMEDIATE")
                .map_err(|e| format!("Failed to begin legacy import transaction: {e}"))?;
            let imported = self
                .conn
                .execute(
                    "INSERT OR IGNORE INTO events
                        (id, ts, kind, skill, harness, scope, project_path, payload,
                         inverse, backup_dir, status, reverted_by, restorable)
                     SELECT id, ts, kind, skill, harness, scope, project_path, payload,
                            inverse, backup_dir, status, reverted_by, restorable
                     FROM legacy.events",
                    [],
                )
                .map_err(|e| format!("Failed to import legacy events: {e}"))?;
            self.conn
                .execute(
                    "INSERT OR IGNORE INTO materialized_roots
                        (root_path, harness, shared_root, created_by)
                     SELECT root_path, harness, shared_root, created_by
                     FROM legacy.materialized_roots",
                    [],
                )
                .map_err(|e| format!("Failed to import legacy converted-folder records: {e}"))?;
            self.conn
                .execute(
                    "INSERT OR IGNORE INTO materialized_disabled (root_path, skill)
                     SELECT root_path, skill FROM legacy.materialized_disabled",
                    [],
                )
                .map_err(|e| format!("Failed to import legacy converted-folder switches: {e}"))?;
            self.conn
                .execute_batch("COMMIT")
                .map_err(|e| format!("Failed to commit legacy import transaction: {e}"))?;
            Ok(imported)
        })();
        if import_result.is_err() {
            let _ = self.conn.execute_batch("ROLLBACK");
        }
        let _ = self.conn.execute_batch("DETACH DATABASE legacy");
        let imported = import_result?;
        let migrated_path = self.app_data.join("events.sqlite3.migrated");
        fs::rename(&legacy_path, &migrated_path)
            .map_err(|e| format!("Failed to rename legacy event store: {e}"))?;
        // Best-effort: WAL/SHM sidecars only exist if the legacy connection
        // was left open mid-checkpoint. Their absence is not an error.
        for ext in ["-wal", "-shm"] {
            let mut sidecar = legacy_path.clone().into_os_string();
            sidecar.push(ext);
            let sidecar = PathBuf::from(sidecar);
            if sidecar.exists() {
                let mut migrated_sidecar = migrated_path.clone().into_os_string();
                migrated_sidecar.push(ext);
                let _ = fs::rename(&sidecar, PathBuf::from(migrated_sidecar));
            }
        }
        Ok(imported)
    }

    fn bind_backup(&self, id: &str) -> Result<UndoPath, String> {
        if Path::new(id).file_name() != Some(OsStr::new(id)) {
            return Err("Invalid backup event ID".into());
        }
        let app_data = self.app_data.canonicalize().map_err(|e| e.to_string())?;
        let path = app_data.join("backups").join(id);
        UndoPath::bind(&path).map_err(|e| format!("Could not bind backup {}: {e}", path.display()))
    }

    /// Copies each existing entry into its event backup, preserving its literal
    /// symlinks and recording the original path and fingerprint in the manifest.
    pub fn backup_paths(&self, id: &str, paths: &[PathBuf]) -> Result<BackupManifest, String> {
        Self::write_backup(id, paths, &self.bind_backup(id)?, |path, dir, name| {
            let parent = path.parent().ok_or("Backup path has no parent")?;
            let name_on_disk = path.file_name().ok_or("Backup path has no file name")?;
            let physical = parent
                .canonicalize()
                .map_err(|e| e.to_string())?
                .join(name_on_disk);
            UndoPath::bind(&physical)
                .and_then(|source| source.backup_to(dir, name))
                .map_err(|e| format!("Failed to back up {}: {e}", path.display()))
        })
    }

    fn write_backup(
        id: &str,
        paths: &[PathBuf],
        backup: &UndoPath,
        copy: impl Fn(&Path, &ConfinedDir, &str) -> Result<(), String>,
    ) -> Result<BackupManifest, String> {
        let root = backup
            .parent(true)
            .map_err(|e| format!("Failed to open backups: {e}"))?;
        root.create_dir(id)
            .map_err(|e| format!("Failed to create backup {id}: {e}"))?;
        let dir = open_confined_child(&root, id.as_ref())
            .map_err(|e| format!("Failed to open backup {id}: {e}"))?;
        let mut manifest = BackupManifest::default();
        for (i, path) in paths.iter().enumerate() {
            let fingerprint = fingerprint_path(path);
            let relative_path = if fingerprint == "absent" {
                String::new()
            } else {
                let basename = path.file_name().map_or_else(
                    || format!("path-{i}"),
                    |name| name.to_string_lossy().into_owned(),
                );
                let relative = format!("{i}-{basename}");
                copy(path, &dir, &relative)?;
                relative
            };
            manifest.entries.insert(
                path.to_string_lossy().into_owned(),
                BackupEntry {
                    relative_path,
                    fingerprint,
                },
            );
        }
        let json = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| format!("Failed to serialize backup manifest: {e}"))?;
        write_confined_file(&dir, OsStr::new("manifest.json"), &json)
            .map_err(|e| format!("Failed to write backup {id} manifest: {e}"))?;
        Ok(manifest)
    }

    /// Inserts a `pending` row for `id`.
    pub fn record(&self, id: &str, draft: &EventDraft) -> Result<(), String> {
        self.record_with_bound_paths(id, draft, None)
    }

    fn record_with_bound_paths(
        &self,
        id: &str,
        draft: &EventDraft,
        files: Option<&UndoFiles>,
    ) -> Result<(), String> {
        #[cfg(test)]
        if let Some(observe) = &self.observe_history {
            observe("record");
        }
        let ts = Utc::now().to_rfc3339();
        let payload_json = serde_json::to_string(&draft.payload)
            .map_err(|e| format!("Failed to serialize payload: {e}"))?;
        let mut inverse_value = draft.inverse.clone();
        if let Some(json) = &mut inverse_value {
            if let Ok(inverse) = serde_json::from_value::<InverseOp>(json.clone()) {
                let parents: BTreeMap<_, _> = inverse
                    .touched_paths()
                    .into_iter()
                    .filter_map(|path| {
                        let parent = path.parent()?;
                        let physical = match files {
                            Some(files) => files.bound_parent(path)?.to_path_buf(),
                            None => canonical_undo_parent(parent).ok()?,
                        };
                        (physical != parent).then_some((path, physical))
                    })
                    .collect();
                if let Some(object) = json.as_object_mut() {
                    object.remove(UNDO_LINKED_PARENTS);
                    if !parents.is_empty() {
                        object.insert(
                            UNDO_LINKED_PARENTS.to_string(),
                            serde_json::to_value(parents)
                                .map_err(|e| format!("Failed to record Undo link parents: {e}"))?,
                        );
                    }
                }
            }
        }
        let inverse_json = inverse_value
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| format!("Failed to serialize inverse: {e}"))?;
        let harness = draft
            .harness
            .as_deref()
            .map(skill_studio_core::identity::AgentId::parse_harness)
            .transpose()
            .map_err(|error| error.to_string())?
            .map(|id| id.to_string());
        self.conn
            .execute(
                "INSERT INTO events
                    (id, ts, kind, skill, harness, scope, project_path, payload, inverse, backup_dir, status, reverted_by, restorable)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'pending', NULL, ?11)",
                params![
                    id,
                    ts,
                    draft.kind,
                    draft.skill,
                    harness,
                    draft.scope,
                    draft.project_path,
                    payload_json,
                    inverse_json,
                    draft.backup_dir,
                    draft.restorable,
                ],
            )
            .map_err(|e| format!("Failed to insert event {id}: {e}"))?;
        Ok(())
    }

    /// Settles `id` as `done` or `failed` under the shared row lifecycle in
    /// `skill_studio_core::events::EventStatus::settle`.
    pub fn finish(&self, id: &str, status: EventStatus) -> Result<(), String> {
        #[cfg(test)]
        if let Some(observe) = &self.observe_history {
            observe("finish");
        }
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| format!("Failed to start finishing event {id}: {e}"))?;
        let current: String = tx
            .query_row(
                "SELECT status FROM events WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| format!("Failed to read event {id}: {e}"))?
            .ok_or_else(|| format!("No event {id} to finish"))?;
        let settled = CoreEventStatus::parse(&current)
            .ok_or_else(|| format!("Event {id} has unknown status {current}"))?
            .settle(status.into())
            .map_err(|e| format!("Event {id}: {}", e.message))?;
        tx.execute(
            "UPDATE events SET status = ?1 WHERE id = ?2",
            params![settled.as_str(), id],
        )
        .map_err(|e| format!("Failed to update event {id}: {e}"))?;
        tx.commit()
            .map_err(|e| format!("Failed to finish event {id}: {e}"))
    }

    /// Runs the shared retention policy before the writer releases its lease.
    /// Cleanup errors leave durable expiry records for the startup retry.
    pub fn retain_backups(&self, guard: &skill_studio_core::ports::ExclusiveGuard) {
        let Some(db_path) = self.conn.path() else {
            return;
        };
        let opener = skill_studio_host::SqliteHistoryOpener::new(db_path)
            .with_backup_roots(vec![self.app_data.join("backups")]);
        if let Err(error) = opener.retain_backups(
            guard,
            chrono::Utc::now(),
            skill_studio_core::backup_retention::BackupRetentionScope::All,
        ) {
            eprintln!("[backup_retention] cleanup deferred: {error}");
        }
        if let Err(error) =
            skill_studio_core::journal::trim_backups(self, guard, chrono::Utc::now())
        {
            eprintln!("[backup_retention] plan cleanup deferred: {error}");
        }
    }

    /// Looks up one event row by id - used by callers (e.g.
    /// `restore_guard_for_explode`) that need to inspect an event before
    /// deciding whether to restore it.
    pub fn get(&self, id: &str) -> Result<Option<EventRow>, String> {
        self.get_event(id)
    }

    fn get_event(&self, id: &str) -> Result<Option<EventRow>, String> {
        #[cfg(test)]
        if let Some(observe) = &self.observe_history {
            observe("read");
        }
        self.conn
            .query_row("SELECT * FROM events WHERE id = ?1", params![id], row_from)
            .optional()
            .map_err(|e| format!("Failed to query event {id}: {e}"))
    }

    /// Lists events newest-first by `ts`, with `rowid` only as a tiebreaker
    /// for two ULIDs allocated in the same millisecond (which don't reliably
    /// sort). `rowid` alone is not enough: `import_legacy_events` appends
    /// imported rows at the end of the table regardless of their original
    /// `ts`, so a legacy row imported today would otherwise sort above
    /// events the core wrote just now.
    pub fn list(&self, limit: usize, skill: Option<&str>) -> Result<Vec<EventRow>, String> {
        let mut stmt = if skill.is_some() {
            self.conn.prepare(
                "SELECT * FROM events WHERE skill = ?1 ORDER BY ts DESC, rowid DESC LIMIT ?2",
            )
        } else {
            self.conn
                .prepare("SELECT * FROM events ORDER BY ts DESC, rowid DESC LIMIT ?1")
        }
        .map_err(|e| format!("Failed to prepare event list query: {e}"))?;

        // A caller-supplied event limit that overflows `i64` is a bug at the
        // call site, not a corrupt row; cap it instead of wrapping so the
        // query still runs with a saner (if too-generous) limit.
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let rows = if let Some(skill) = skill {
            stmt.query_map(params![skill, limit], row_from)
        } else {
            stmt.query_map(params![limit], row_from)
        }
        .map_err(|e| format!("Failed to list events: {e}"))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to read event row: {e}"))
    }

    /// Lists active events of one kind for dependency guards. Failed and
    /// already-restored events cannot own live filesystem state.
    pub(crate) fn active_events_of_kind(&self, kind: &str) -> Result<Vec<EventRow>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT * FROM events
                 WHERE kind = ?1 AND reverted_by IS NULL
                   AND status IN ('pending', 'done', 'interrupted')
                 ORDER BY rowid DESC",
            )
            .map_err(|e| format!("Failed to prepare active event query: {e}"))?;
        let rows = stmt
            .query_map(params![kind], row_from)
            .map_err(|e| format!("Failed to query active {kind} events: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to read active {kind} event: {e}"))
    }

    /// Lists every interrupted Make event and restore event that startup must
    /// retry. Recovery handlers discard restores that do not target Make.
    pub fn interrupted_independent_copy_events(&self) -> Result<Vec<EventRow>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT * FROM events
                 WHERE status = 'interrupted'
                   AND kind IN ('make_independent_copy', 'restore')
                 ORDER BY rowid ASC",
            )
            .map_err(|e| format!("Failed to prepare independent-copy recovery query: {e}"))?;
        let rows = stmt
            .query_map([], row_from)
            .map_err(|e| format!("Failed to query interrupted independent-copy events: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to read interrupted independent-copy event: {e}"))
    }

    /// Lists interrupted whole-root convert-and-disable intents in creation order.
    pub fn interrupted_convert_then_disable_events(&self) -> Result<Vec<EventRow>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT * FROM events
                 WHERE status = 'interrupted' AND kind = 'materialize_then_disable'
                 ORDER BY rowid ASC",
            )
            .map_err(|e| format!("Failed to prepare convert-and-disable recovery query: {e}"))?;
        let rows = stmt
            .query_map([], row_from)
            .map_err(|e| format!("Failed to query interrupted convert-and-disable events: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to read interrupted convert-and-disable event: {e}"))
    }

    /// Interrupted deterministic frontmatter repairs that startup can finish
    /// from their backend-generated, fingerprint-bound intent. Both the
    /// desktop's `apply_skill_frontmatter_repair` and the core's
    /// `ops::fix_skill` write `kind = 'repair_skill_frontmatter'`, but only
    /// the desktop's payload carries `proposed_content_fingerprint` - the
    /// core's `fix_skill` payload shape is not something this recovery loop
    /// (desktop-only, driven by `lib.rs`) knows how to parse, so a core row
    /// here would fail rather than recover.
    pub fn interrupted_frontmatter_repair_events(&self) -> Result<Vec<EventRow>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT * FROM events
                 WHERE status = 'interrupted' AND kind = 'repair_skill_frontmatter'
                   AND json_extract(payload, '$.proposed_content_fingerprint') IS NOT NULL
                 ORDER BY rowid ASC",
            )
            .map_err(|e| format!("Failed to prepare frontmatter repair recovery query: {e}"))?;
        let rows = stmt
            .query_map([], row_from)
            .map_err(|e| format!("Failed to query interrupted frontmatter repairs: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to read interrupted frontmatter repair: {e}"))
    }

    /// Flips every `pending` row to `interrupted` (a crash is the only way
    /// one survives a restart) and returns the flipped rows.
    pub fn reconcile_at_startup(&self) -> Result<Vec<EventRow>, String> {
        let ids: Vec<String> = {
            let mut stmt = self
                .conn
                .prepare("SELECT id FROM events WHERE status = 'pending'")
                .map_err(|e| format!("Failed to prepare pending query: {e}"))?;
            let mapped = stmt
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|e| format!("Failed to query pending events: {e}"))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("Failed to read pending id: {e}"))?;
            mapped
        };
        for id in &ids {
            self.conn
                .execute(
                    "UPDATE events SET status = 'interrupted' WHERE id = ?1",
                    params![id],
                )
                .map_err(|e| format!("Failed to interrupt event {id}: {e}"))?;
        }
        ids.iter()
            .map(|id| {
                self.get_event(id)?
                    .ok_or_else(|| format!("Event {id} vanished mid-reconcile"))
            })
            .collect()
    }

    /// Undoes event `target_id` under `lease`, the proof the caller took
    /// before it read this row. Returns the id of the `restore` event
    /// created to do it. See the module header for the drift-guard and
    /// restore-of-restore design.
    ///
    /// Holding the proof for this whole call is what puts the drift check and
    /// the write under one lease: `apply_restore` compares the destination
    /// against the inverse's post-fingerprint and then writes it, and no
    /// other Skill Studio writer can change the destination between the two.
    pub fn restore(
        &self,
        lease: &WriteLeaseGuard,
        home: &Path,
        target_id: &str,
        force: bool,
    ) -> Result<String, String> {
        self.restore_with_skill_md_transaction_observer(lease, home, target_id, force, |_| {})
    }

    fn restore_with_skill_md_transaction_observer(
        &self,
        lease: &WriteLeaseGuard,
        home: &Path,
        target_id: &str,
        force: bool,
        observe_transaction: impl FnOnce(Option<&SkillMdWriteTransaction<'_>>),
    ) -> Result<String, String> {
        let target = self
            .get_event(target_id)?
            .ok_or_else(|| format!("Event {target_id} not found"))?;
        if skill_studio_core::backup_retention::backup_is_expired(&target.payload) {
            return Err(skill_studio_core::backup_retention::BACKUP_EXPIRED_REASON.to_owned());
        }
        if target.reverted_by.is_some() {
            return Err(format!("Event {target_id} was already restored"));
        }
        if !target.restorable {
            return Err(format!("Event {target_id} is not restorable"));
        }
        let inverse_value = target
            .inverse
            .clone()
            .ok_or_else(|| format!("Event {target_id} has no inverse and cannot be restored"))?;
        let linked_parents = inverse_value
            .get(UNDO_LINKED_PARENTS)
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| format!("Failed to read Undo link parents: {e}"))?
            .unwrap_or_default();
        let inverse: InverseOp = serde_json::from_value(inverse_value)
            .map_err(|e| format!("Failed to parse inverse for {target_id}: {e}"))?;
        let files = UndoFiles::bind(
            home,
            &self.app_data,
            inverse.touched_paths(),
            &linked_parents,
        )?;
        let restore_id = allocate_id();
        let backup = self.bind_backup(&restore_id)?;
        let source = match &inverse {
            InverseOp::RestoreBackup { path, .. } => Some(backup_source(&target, &backup, path)?),
            InverseOp::UndistributeFromShared { shared_dir, .. } => {
                Some(backup_source(&target, &backup, shared_dir)?)
            }
            _ => None,
        };

        let skill_md_transaction =
            if inverse.destination().file_name() == Some(std::ffi::OsStr::new("SKILL.md")) {
                Some(begin_skill_md_write_transaction(lease))
            } else {
                None
            };
        observe_transaction(skill_md_transaction.as_ref());

        match self.apply_restore(
            &restore_id,
            &target,
            &inverse,
            force,
            &files,
            &backup,
            source.as_ref(),
        ) {
            Ok(()) => Ok(restore_id),
            Err(e) => {
                let _ = self.unclaim_event_restore(target_id, &restore_id);
                Err(e)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_restore(
        &self,
        restore_id: &str,
        target: &EventRow,
        inverse: &InverseOp,
        force: bool,
        files: &UndoFiles,
        backup: &UndoPath,
        source: Option<&BackupSource>,
    ) -> Result<(), String> {
        // `distribute_from_shared`'s inverse touches several paths (the
        // shared dir plus every copy it created), not the single destination
        // the generic flow below drift-checks and restores - see the module
        // header's "add a per-kind arm" note and `apply_restore_distribute`.
        if let InverseOp::UndistributeFromShared {
            shared_dir,
            copies,
            copy_fingerprints,
            symlinks,
            ..
        } = inverse
        {
            return self.apply_restore_distribute(
                restore_id,
                target,
                shared_dir,
                copies,
                copy_fingerprints,
                symlinks,
                force,
                files,
                backup,
                source.ok_or("Distribution restore has no bound source backup")?,
            );
        }

        let dest = inverse.destination().to_path_buf();
        let current_fp = fingerprint_path(&dest);
        if let Some(expected) = inverse.post_fingerprint() {
            if current_fp != *expected && !force {
                return Err(format!(
                    "{} has changed since the event that would be undone; use force to restore anyway (the current content will be backed up first)",
                    dest.display()
                ));
            }
        }

        // Phase 2: preserve whatever currently sits at the destination
        // (drifted or not) under the restore event's own backup dir.
        Self::backup_for_restore(restore_id, std::slice::from_ref(&dest), files, backup)?;
        let backup_dir = format!("backups/{restore_id}");

        // Phase 3: record the pending restore row, with a provisional
        // inverse (its post_fingerprint is patched in once we know the
        // state the restore itself leaves behind).
        let restore_inverse = InverseOp::RestoreBackup {
            path: dest.clone(),
            pre_fingerprint: current_fp,
            post_fingerprint: None,
        };
        self.record_with_bound_paths(
            restore_id,
            &EventDraft {
                kind: "restore".to_string(),
                skill: target.skill.clone(),
                harness: target.harness.clone(),
                scope: target.scope.clone(),
                project_path: target.project_path.clone(),
                payload: serde_json::json!({ "target_event": target.id }),
                inverse: Some(
                    serde_json::to_value(&restore_inverse)
                        .map_err(|e| format!("Failed to serialize restore inverse: {e}"))?,
                ),
                backup_dir: Some(backup_dir),
                restorable: true,
            },
            Some(files),
        )?;
        self.claim_recorded_restore(&target.id, restore_id)?;

        // Phase 4: apply the target event's inverse.
        match Self::apply_inverse_op(inverse, files, source) {
            Ok(()) => {
                let post_fp = fingerprint_path(&dest);
                self.patch_inverse_post_fingerprint(restore_id, &post_fp)?;
                self.finish(restore_id, EventStatus::Done)?;
                Ok(())
            }
            Err(e) => {
                self.fail_claimed_restore(&target.id, restore_id)?;
                Err(e)
            }
        }
    }

    /// Replaces an already-recorded event payload. Independent copy records
    /// intent first, then fills fingerprints, Copy ownership, and any
    /// whole-root conversion id once those values exist.
    pub(crate) fn patch_event_payload(&self, id: &str, payload: &Value) -> Result<(), String> {
        let json = serde_json::to_string(payload)
            .map_err(|e| format!("Failed to serialize payload for {id}: {e}"))?;
        self.conn
            .execute(
                "UPDATE events SET payload = ?1 WHERE id = ?2",
                params![json, id],
            )
            .map_err(|e| format!("Failed to patch payload for {id}: {e}"))?;
        Ok(())
    }

    /// Replaces an already-recorded inverse. Whole-root independent copies
    /// record intent before the per-skill link exists, then fill `RecreateSymlink`.
    pub(crate) fn patch_event_inverse(&self, id: &str, inverse: &Value) -> Result<(), String> {
        let json = serde_json::to_string(inverse)
            .map_err(|e| format!("Failed to serialize inverse for {id}: {e}"))?;
        self.conn
            .execute(
                "UPDATE events SET inverse = ?1 WHERE id = ?2",
                params![json, id],
            )
            .map_err(|e| format!("Failed to patch inverse for {id}: {e}"))?;
        Ok(())
    }

    /// Claims `target_id` for restore event `restore_id`. Zero rows means
    /// another restore already claimed it.
    pub(crate) fn claim_event_restore(
        &self,
        target_id: &str,
        restore_id: &str,
    ) -> Result<(), String> {
        #[cfg(test)]
        if let Some(observe) = &self.observe_history {
            observe("claim");
        }
        let claimed = self
            .conn
            .execute(
                "UPDATE events SET reverted_by = ?1 WHERE id = ?2 AND reverted_by IS NULL",
                params![restore_id, target_id],
            )
            .map_err(|e| format!("Failed to claim event {target_id}: {e}"))?;
        if claimed == 0 {
            return Err(format!("Event {target_id} was already restored"));
        }
        Ok(())
    }

    /// Claims `target_id` for the restore row that was just recorded. The row
    /// comes first, as in the core's `restore_event_body`: recovery releases a
    /// claim only through its `pending` or `interrupted` restore row, so a
    /// claim must never name a row that does not exist.
    fn claim_recorded_restore(&self, target_id: &str, restore_id: &str) -> Result<(), String> {
        if let Err(error) = self.claim_event_restore(target_id, restore_id) {
            self.finish(restore_id, EventStatus::Failed)?;
            return Err(error);
        }
        Ok(())
    }

    /// Settles a restore whose write failed. The claim is released first, as
    /// in the core: recovery does not release the claim of a `failed` row, so
    /// a `failed` row must never hold one.
    fn fail_claimed_restore(&self, target_id: &str, restore_id: &str) -> Result<(), String> {
        self.unclaim_event_restore(target_id, restore_id)?;
        self.finish(restore_id, EventStatus::Failed)
    }

    /// Clears a restore claim so an interrupted undo can be retried or
    /// abandoned without leaving the original event unrestorable.
    pub(crate) fn unclaim_event_restore(
        &self,
        target_id: &str,
        restore_id: &str,
    ) -> Result<(), String> {
        #[cfg(test)]
        if let Some(observe) = &self.observe_history {
            observe("unclaim");
        }
        self.conn
            .execute(
                "UPDATE events SET reverted_by = NULL WHERE id = ?1 AND reverted_by = ?2",
                params![target_id, restore_id],
            )
            .map_err(|e| format!("Failed to unclaim event {target_id}: {e}"))?;
        Ok(())
    }

    /// Patches an already-recorded event's `inverse.post_fingerprint` once
    /// the state its mutation left behind is known - the same pattern
    /// `apply_restore` uses for its own restore row. Exposed to
    /// `skill_materialize` so multi-step mutations (record pending, mutate,
    /// then learn the post-fingerprint) outside this module can do the same.
    pub(crate) fn patch_inverse_post_fingerprint(
        &self,
        id: &str,
        post_fp: &str,
    ) -> Result<(), String> {
        #[cfg(test)]
        if let Some(observe) = &self.observe_history {
            observe("patch");
        }
        let row = self
            .get_event(id)?
            .ok_or_else(|| format!("Event {id} vanished before its inverse could be patched"))?;
        let mut inverse = row
            .inverse
            .ok_or_else(|| format!("Event {id} has no inverse to patch"))?;
        if let Some(obj) = inverse.as_object_mut() {
            obj.insert(
                "post_fingerprint".to_string(),
                Value::String(post_fp.to_string()),
            );
        }
        let json = serde_json::to_string(&inverse)
            .map_err(|e| format!("Failed to serialize patched inverse: {e}"))?;
        self.conn
            .execute(
                "UPDATE events SET inverse = ?1 WHERE id = ?2",
                params![json, id],
            )
            .map_err(|e| format!("Failed to patch inverse for {id}: {e}"))?;
        Ok(())
    }

    fn backup_for_restore(
        id: &str,
        paths: &[PathBuf],
        files: &UndoFiles,
        backup: &UndoPath,
    ) -> Result<(), String> {
        Self::write_backup(id, paths, backup, |path, dir, name| {
            files
                .get(path)?
                .backup_to(dir, name)
                .map_err(|e| format!("Failed to back up {}: {e}", path.display()))
        })?;
        Ok(())
    }

    fn apply_inverse_op(
        op: &InverseOp,
        files: &UndoFiles,
        source: Option<&BackupSource>,
    ) -> Result<(), String> {
        let result = match op {
            InverseOp::RecreateSymlink { link, target, .. } => {
                files.get(link)?.symlink(target, true)
            }
            InverseOp::RemoveSymlink { link, .. } => files.get(link)?.remove(true),
            InverseOp::MoveBack { from, to, .. } => files.get(from)?.move_to(files.get(to)?),
            InverseOp::RestoreBackup { path, .. } => {
                return source
                    .ok_or("Restore has no bound source backup")?
                    .restore_to(files.get(path)?, path);
            }
            InverseOp::UndistributeFromShared { .. } => {
                return Err("Distribution restore requires its full set of paths".into())
            }
        };
        result.map_err(|e| format!("Failed to undo {}: {e}", op.destination().display()))
    }

    /// The `UndistributeFromShared` arm of `apply_restore`: refuses (without
    /// `force`) if any copy `distribute_from_shared` created has drifted from
    /// its fingerprint at distribution time, then backs up the shared dir and
    /// every copy (so this restore is itself restorable), puts the shared dir
    /// back from the original event's backup, deletes the copies, and
    /// recreates the symlinks that were removed.
    #[allow(clippy::too_many_arguments)]
    fn apply_restore_distribute(
        &self,
        restore_id: &str,
        target: &EventRow,
        shared_dir: &Path,
        copies: &[PathBuf],
        copy_fingerprints: &[String],
        symlinks: &[(PathBuf, PathBuf)],
        force: bool,
        files: &UndoFiles,
        backup: &UndoPath,
        source: &BackupSource,
    ) -> Result<(), String> {
        if !force {
            for (path, expected) in copies.iter().zip(copy_fingerprints) {
                let current = fingerprint_path(path);
                if current != *expected {
                    return Err(format!(
                        "{} has changed since the event that would be undone; use force to restore anyway (the current content will be backed up first)",
                        path.display()
                    ));
                }
            }
        }

        // Phase 2: preserve whatever currently sits at every path this
        // restore is about to touch.
        let mut backup_targets = vec![shared_dir.to_path_buf()];
        backup_targets.extend(copies.iter().cloned());
        Self::backup_for_restore(restore_id, &backup_targets, files, backup)?;
        let backup_dir = format!("backups/{restore_id}");

        // Phase 3: record the pending restore row. Its own inverse only
        // covers putting the shared dir back - see the module doc note on
        // `apply_restore_distribute` for why a restore-of-this-restore
        // doesn't also recreate the copies/symlinks.
        let restore_inverse = InverseOp::RestoreBackup {
            path: shared_dir.to_path_buf(),
            pre_fingerprint: fingerprint_path(shared_dir),
            post_fingerprint: None,
        };
        self.record_with_bound_paths(
            restore_id,
            &EventDraft {
                kind: "restore".to_string(),
                skill: target.skill.clone(),
                harness: target.harness.clone(),
                scope: target.scope.clone(),
                project_path: target.project_path.clone(),
                payload: serde_json::json!({ "target_event": target.id }),
                inverse: Some(
                    serde_json::to_value(&restore_inverse)
                        .map_err(|e| format!("Failed to serialize restore inverse: {e}"))?,
                ),
                backup_dir: Some(backup_dir),
                restorable: true,
            },
            Some(files),
        )?;
        self.claim_recorded_restore(&target.id, restore_id)?;

        // Phase 4: put the shared dir back, delete the copies, recreate the
        // removed symlinks.
        let apply: Result<(), String> = (|| {
            source.restore_to(files.get(shared_dir)?, shared_dir)?;
            for copy in copies {
                files
                    .get(copy)?
                    .remove(false)
                    .map_err(|e| format!("Failed to remove {}: {e}", copy.display()))?;
            }
            for (link, link_target) in symlinks {
                files
                    .get(link)?
                    .symlink(link_target, false)
                    .map_err(|e| format!("Failed to recreate {}: {e}", link.display()))?;
            }
            Ok(())
        })();

        match apply {
            Ok(()) => {
                let post_fp = fingerprint_path(shared_dir);
                self.patch_inverse_post_fingerprint(restore_id, &post_fp)?;
                self.finish(restore_id, EventStatus::Done)?;
                Ok(())
            }
            Err(e) => {
                self.fail_claimed_restore(&target.id, restore_id)?;
                Err(e)
            }
        }
    }

    /// Records `root` as a harness skills dir that Skill Studio converted
    /// to per-skill links mirroring `shared_root` (see the spec's
    /// `explode_shared_dir`). Idempotent: re-registering updates the row.
    pub fn register_materialized_root(
        &self,
        root: &Path,
        harness: &str,
        shared_root: &Path,
        created_by: &str,
    ) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO materialized_roots (root_path, harness, shared_root, created_by)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(root_path) DO UPDATE SET
                    harness = excluded.harness,
                    shared_root = excluded.shared_root,
                    created_by = excluded.created_by",
                params![
                    root.to_string_lossy(),
                    harness,
                    shared_root.to_string_lossy(),
                    created_by,
                ],
            )
            .map_err(|e| format!("Failed to register converted folder link: {e}"))?;
        Ok(())
    }

    pub fn unregister_materialized_root(&self, root: &Path) -> Result<(), String> {
        self.conn
            .execute(
                "DELETE FROM materialized_disabled WHERE root_path = ?1",
                params![root.to_string_lossy()],
            )
            .map_err(|e| format!("Failed to clear converted-folder switches: {e}"))?;
        self.conn
            .execute(
                "DELETE FROM materialized_roots WHERE root_path = ?1",
                params![root.to_string_lossy()],
            )
            .map_err(|e| format!("Failed to unregister converted folder link: {e}"))?;
        Ok(())
    }

    pub fn materialized_root(&self, root: &Path) -> Result<Option<MaterializedRoot>, String> {
        self.conn
            .query_row(
                "SELECT root_path, harness, shared_root, created_by FROM materialized_roots WHERE root_path = ?1",
                params![root.to_string_lossy()],
                |row| {
                    Ok(MaterializedRoot {
                        root_path: row.get(0)?,
                        harness: row.get(1)?,
                        shared_root: row.get(2)?,
                        created_by: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(|e| format!("Failed to query converted folder link: {e}"))
    }

    pub fn set_materialized_disabled(
        &self,
        root: &Path,
        skill: &str,
        disabled: bool,
    ) -> Result<(), String> {
        if disabled {
            self.conn
                .execute(
                    "INSERT OR IGNORE INTO materialized_disabled (root_path, skill) VALUES (?1, ?2)",
                    params![root.to_string_lossy(), skill],
                )
                .map_err(|e| format!("Failed to disable {skill}: {e}"))?;
        } else {
            self.conn
                .execute(
                    "DELETE FROM materialized_disabled WHERE root_path = ?1 AND skill = ?2",
                    params![root.to_string_lossy(), skill],
                )
                .map_err(|e| format!("Failed to re-enable {skill}: {e}"))?;
        }
        Ok(())
    }

    pub fn materialized_disabled(&self, root: &Path) -> Result<Vec<String>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT skill FROM materialized_disabled WHERE root_path = ?1")
            .map_err(|e| format!("Failed to prepare disabled query: {e}"))?;
        let mapped = stmt
            .query_map(params![root.to_string_lossy()], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|e| format!("Failed to query disabled skills: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to read disabled row: {e}"))?;
        Ok(mapped)
    }
}

/// `EventStore` is the host implementation of `skill-studio-core`'s
/// `Journal` port; every method just forwards to the `FsJournal` it already
/// owns (see the doc comment on the struct for why) and never touches
/// `self.conn` - the `rusqlite::Connection` this struct also owns is
/// untouched by this impl, which is why `Journal`'s `Send`-only bound (no
/// `Sync`) costs this struct nothing despite `Connection` itself being
/// `!Sync`.
impl skill_studio_core::ports::Journal for EventStore {
    fn begin(
        &self,
        guard: &skill_studio_core::ports::ExclusiveGuard,
        plan: &skill_studio_core::ports::PlanRecord,
    ) -> Result<(), skill_studio_core::error::CoreError> {
        skill_studio_core::ports::Journal::begin(&self.journal, guard, plan)
    }

    fn record_step(
        &self,
        guard: &skill_studio_core::ports::ExclusiveGuard,
        id: &skill_studio_core::identity::PlanId,
        step: skill_studio_core::ports::PlanStep,
    ) -> Result<(), skill_studio_core::error::CoreError> {
        skill_studio_core::ports::Journal::record_step(&self.journal, guard, id, step)
    }

    fn record_recovery_failure(
        &self,
        guard: &skill_studio_core::ports::ExclusiveGuard,
        id: &skill_studio_core::identity::PlanId,
        error: &str,
    ) -> Result<(), skill_studio_core::error::CoreError> {
        skill_studio_core::ports::Journal::record_recovery_failure(&self.journal, guard, id, error)
    }

    fn finish(
        &self,
        guard: &skill_studio_core::ports::ExclusiveGuard,
        id: &skill_studio_core::identity::PlanId,
        status: skill_studio_core::ports::PlanStatus,
    ) -> Result<(), skill_studio_core::error::CoreError> {
        skill_studio_core::ports::Journal::finish(&self.journal, guard, id, status)
    }

    fn all(
        &self,
    ) -> Result<Vec<skill_studio_core::ports::PlanRecord>, skill_studio_core::error::CoreError>
    {
        skill_studio_core::ports::Journal::all(&self.journal)
    }

    fn pending(
        &self,
    ) -> Result<Vec<skill_studio_core::ports::PlanRecord>, skill_studio_core::error::CoreError>
    {
        skill_studio_core::ports::Journal::pending(&self.journal)
    }

    fn remove_backup(
        &self,
        guard: &skill_studio_core::ports::ExclusiveGuard,
        id: &skill_studio_core::identity::PlanId,
        relative: &str,
    ) -> Result<(), skill_studio_core::error::CoreError> {
        skill_studio_core::ports::Journal::remove_backup(&self.journal, guard, id, relative)
    }

    fn write_backup(
        &self,
        guard: &skill_studio_core::ports::ExclusiveGuard,
        id: &skill_studio_core::identity::PlanId,
        relative: &str,
        bytes: &[u8],
    ) -> Result<(), skill_studio_core::error::CoreError> {
        skill_studio_core::ports::Journal::write_backup(&self.journal, guard, id, relative, bytes)
    }

    fn read_backup(
        &self,
        id: &skill_studio_core::identity::PlanId,
        relative: &str,
    ) -> Result<Vec<u8>, skill_studio_core::error::CoreError> {
        skill_studio_core::ports::Journal::read_backup(&self.journal, id, relative)
    }
}

fn row_from(row: &rusqlite::Row) -> rusqlite::Result<EventRow> {
    let payload_str: String = row.get("payload")?;
    let inverse_str: Option<String> = row.get("inverse")?;
    Ok(EventRow {
        id: row.get("id")?,
        ts: row.get("ts")?,
        kind: row.get("kind")?,
        skill: row.get("skill")?,
        harness: row.get("harness")?,
        scope: row.get("scope")?,
        project_path: row.get("project_path")?,
        payload: serde_json::from_str(&payload_str).unwrap_or(Value::Null),
        inverse: inverse_str.map(|s| serde_json::from_str(&s).unwrap_or(Value::Null)),
        backup_dir: row.get("backup_dir")?,
        status: row.get("status")?,
        reverted_by: row.get("reverted_by")?,
        restorable: row.get("restorable")?,
    })
}

/// A fresh, sortable-by-time event id. No DB access.
pub fn allocate_id() -> String {
    ulid::Ulid::generate().to_string()
}

/// Content fingerprint for drift detection and backup verification:
/// `"absent"` when nothing exists at `path` (checked via `symlink_metadata`
/// so a broken symlink still fingerprints as present), a SHA-256 of the
/// literal target string for a symlink, of the bytes for a file, and of the
/// sorted `(name, entry-fingerprint)` pairs for a directory (so a rename
/// inside a directory changes its fingerprint even if total bytes match).
pub fn fingerprint_path(path: &Path) -> String {
    fingerprint_path_checked(path)
        .ok()
        .flatten()
        .unwrap_or_else(|| "absent".to_string())
}

pub fn fingerprint_path_checked(path: &Path) -> std::io::Result<Option<String>> {
    match fs::symlink_metadata(path) {
        Ok(_) => hash_entry(path).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn hash_entry(path: &Path) -> std::io::Result<String> {
    skill_studio_host::hash_entry(path)
}

struct BackupSource {
    directory: ConfinedDir,
    entry: Option<String>,
}

impl BackupSource {
    fn restore_to(&self, destination: &UndoPath, path: &Path) -> Result<(), String> {
        destination
            .restore_from(&self.directory, self.entry.as_deref())
            .map_err(|e| format!("Failed to restore {} from backup: {e}", path.display()))
    }
}

fn backup_source(event: &EventRow, backup: &UndoPath, path: &Path) -> Result<BackupSource, String> {
    let backup_dir = event
        .backup_dir
        .as_deref()
        .ok_or("Restore has no source backup dir")?;
    let expected_dir = Path::new("backups").join(&event.id);
    if Path::new(backup_dir) != expected_dir
        || Path::new(&event.id).file_name() != Some(OsStr::new(&event.id))
    {
        return Err(format!(
            "Undo refused: invalid source backup directory {backup_dir}"
        ));
    }
    let directory = backup
        .parent(false)
        .and_then(|root| open_confined_child(&root, event.id.as_ref()))
        .map_err(|e| format!("Undo refused: could not open source backup {backup_dir}: {e}"))?;
    let data = directory
        .read("manifest.json")
        .map_err(|e| format!("Failed to read manifest in {backup_dir}: {e}"))?;
    let mut manifest: BackupManifest =
        serde_json::from_slice(&data).map_err(|e| format!("Failed to parse manifest: {e}"))?;
    let entry = manifest
        .entries
        .remove(path.to_string_lossy().as_ref())
        .ok_or_else(|| format!("No backup entry for {}", path.display()))?;
    let name = (entry.fingerprint != "absent").then_some(entry.relative_path);
    if let Some(name) = &name {
        if Path::new(name).file_name() != Some(OsStr::new(name)) {
            return Err(format!(
                "Undo refused: backup entry {name} is outside its event directory"
            ));
        }
        directory
            .symlink_metadata(name)
            .map_err(|e| format!("Failed to find backup entry {name}: {e}"))?;
    }
    Ok(BackupSource {
        directory,
        entry: name,
    })
}

/// Manifest written alongside a backup: original absolute path -> where its
/// bytes live inside the backup dir, plus its fingerprint at backup time.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct BackupManifest {
    pub entries: std::collections::BTreeMap<String, BackupEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupEntry {
    /// Path relative to the backup dir; empty when the original was absent.
    pub relative_path: String,
    pub fingerprint: String,
}

/// Forward-mutation data for a not-yet-written event row.
#[derive(Debug, Clone)]
pub struct EventDraft {
    pub kind: String,
    pub skill: String,
    pub harness: Option<String>,
    pub scope: Option<String>,
    pub project_path: Option<String>,
    pub payload: Value,
    pub inverse: Option<Value>,
    /// Relative to `app_data`, e.g. `"backups/<id>"`.
    pub backup_dir: Option<String>,
    pub restorable: bool,
}

#[derive(Clone, Copy)]
pub enum EventStatus {
    Done,
    Failed,
}

impl From<EventStatus> for CoreEventStatus {
    fn from(status: EventStatus) -> Self {
        match status {
            EventStatus::Done => CoreEventStatus::Done,
            EventStatus::Failed => CoreEventStatus::Failed,
        }
    }
}

/// One row of the `events` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRow {
    pub id: String,
    pub ts: String,
    pub kind: String,
    pub skill: String,
    pub harness: Option<String>,
    pub scope: Option<String>,
    pub project_path: Option<String>,
    pub payload: Value,
    pub inverse: Option<Value>,
    pub backup_dir: Option<String>,
    pub status: String,
    pub reverted_by: Option<String>,
    pub restorable: bool,
}

/// How to undo one event. `pre_fingerprint` is the destination's
/// fingerprint before the forward mutation ran (what restoring recreates);
/// `post_fingerprint` is the fingerprint the mutation left behind (what the
/// drift guard checks the live filesystem against before restoring). It is
/// `Option` because it's only known once the forward mutation completes -
/// `record` writes it as `None` and the caller patches it in after phase 4,
/// same as `restore` patches its own inverse in `patch_inverse_post_fingerprint`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum InverseOp {
    /// Undo of a deleted symlink: recreate `link` pointing at `target`.
    RecreateSymlink {
        link: PathBuf,
        target: PathBuf,
        pre_fingerprint: String,
        post_fingerprint: Option<String>,
    },
    /// Undo of a created symlink (`pre_fingerprint` is `"absent"`): remove
    /// `link`, but only if it is still a symlink.
    RemoveSymlink {
        link: PathBuf,
        pre_fingerprint: String,
        post_fingerprint: Option<String>,
    },
    /// Undo of a rename: move `from` back to `to`.
    MoveBack {
        from: PathBuf,
        to: PathBuf,
        pre_fingerprint: String,
        post_fingerprint: Option<String>,
    },
    /// Undo of any mutation that backed up the destination first: copy its
    /// bytes back out of the original event's backup dir.
    RestoreBackup {
        path: PathBuf,
        pre_fingerprint: String,
        post_fingerprint: Option<String>,
    },
    /// Undo of `skill_materialize::distribute_from_shared`: put `shared_dir`
    /// back from the event's backup, delete every path in `copies` (real
    /// directories the operation created), and recreate every `(link,
    /// target)` in `symlinks` (the per-skill symlinks it removed to make
    /// room for those copies). `copy_fingerprints` is parallel to `copies` -
    /// each copy's fingerprint right after distribution, for the drift guard
    /// `apply_restore_distribute` runs instead of the generic single-path
    /// check the other variants get.
    UndistributeFromShared {
        shared_dir: PathBuf,
        copies: Vec<PathBuf>,
        copy_fingerprints: Vec<String>,
        symlinks: Vec<(PathBuf, PathBuf)>,
        pre_fingerprint: String,
        post_fingerprint: Option<String>,
    },
}

impl InverseOp {
    fn destination(&self) -> &Path {
        match self {
            InverseOp::RecreateSymlink { link, .. } | InverseOp::RemoveSymlink { link, .. } => link,
            InverseOp::MoveBack { to, .. } => to,
            InverseOp::RestoreBackup { path, .. } => path,
            InverseOp::UndistributeFromShared { shared_dir, .. } => shared_dir,
        }
    }

    fn touched_paths(&self) -> Vec<&Path> {
        let mut paths = vec![self.destination()];
        match self {
            Self::MoveBack { from, .. } => paths.push(from),
            Self::UndistributeFromShared {
                copies, symlinks, ..
            } => {
                paths.extend(copies.iter().map(PathBuf::as_path));
                paths.extend(symlinks.iter().map(|(link, _)| link.as_path()));
            }
            _ => {}
        }
        paths
    }

    fn post_fingerprint(&self) -> Option<&String> {
        match self {
            InverseOp::RecreateSymlink {
                post_fingerprint, ..
            }
            | InverseOp::RemoveSymlink {
                post_fingerprint, ..
            }
            | InverseOp::MoveBack {
                post_fingerprint, ..
            }
            | InverseOp::RestoreBackup {
                post_fingerprint, ..
            }
            | InverseOp::UndistributeFromShared {
                post_fingerprint, ..
            } => post_fingerprint.as_ref(),
        }
    }
}

/// A harness skills dir Skill Studio converted to per-skill links mirroring
/// a shared root (see the spec's `explode_shared_dir` / Materialize).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterializedRoot {
    pub root_path: String,
    pub harness: String,
    pub shared_root: String,
    pub created_by: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::write_lease::{exclusive_lease_is_held, WriteLease};
    use std::os::unix::fs::symlink;

    #[test]
    fn desktop_history_open_indexes_timestamp_pages_and_keeps_tie_order_or_sorts_the_whole_store() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("events.sqlite3");
        let conn = open(&path).unwrap();
        conn.execute("DROP INDEX IF EXISTS idx_events_ts", [])
            .unwrap();
        let fixtures = [
            ("old", "2026-01-01T00:00:00Z"),
            ("tied-first", "2026-02-01T00:00:00Z"),
            ("tied-last", "2026-02-01T00:00:00Z"),
        ];
        for (id, ts) in fixtures {
            conn.execute("INSERT INTO events (id,ts,kind,skill,payload,status) VALUES (?1,?2,'install','alpha','{}','done')", params![id, ts]).unwrap();
        }
        drop(conn);
        let store = EventStore::open_with_db(tmp.path(), &path).unwrap();
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
        let rows = store.list(2, None).unwrap();
        assert_eq!(
            rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            fixtures
                .iter()
                .rev()
                .take(2)
                .map(|(id, _)| *id)
                .collect::<Vec<_>>()
        );
    }

    fn store(dir: &Path) -> EventStore {
        EventStore::open(&dir.join("app_data")).expect("open store")
    }

    fn mutation(dir: &Path) -> WriteLeaseGuard {
        WriteLease::with_lease_root(dir.join("leases"))
            .try_acquire(dir)
            .expect("take the write lease")
    }

    fn draft(
        kind: &str,
        skill: &str,
        payload: Value,
        inverse: Option<Value>,
        backup_dir: Option<String>,
    ) -> EventDraft {
        EventDraft {
            kind: kind.to_string(),
            skill: skill.to_string(),
            harness: Some("claude-code".to_string()),
            scope: Some("global".to_string()),
            project_path: None,
            payload,
            inverse,
            backup_dir,
            restorable: true,
        }
    }

    /// Given a plan recorded through `EventStore`'s `Journal` impl and left
    /// `Pending` (a simulated crash - the plan writer is dropped without
    /// `finish`), when `skill_studio_core::journal::reconcile` runs against
    /// `&store`, then the plan's row is left `Reversed`, not deleted; on
    /// failure the panic names the plan left open.
    #[test]
    fn desktop_event_store_reverses_a_pending_core_plan_at_startup_or_names_the_plan_left_open() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path());

        let root_path = tmp.path().join("skills_root");
        fs::create_dir_all(&root_path).unwrap();
        fs::write(root_path.join("target.txt"), b"hi").unwrap();
        let fs_port: std::sync::Arc<dyn skill_studio_core::ports::ScopeFs> =
            std::sync::Arc::new(skill_studio_host::RealFs::new());
        let root = skill_studio_core::fsops::Root::open(fs_port.as_ref(), root_path.clone())
            .expect("open root");

        let lease_dir = tmp.path().join("lease");
        fs::create_dir_all(&lease_dir).unwrap();
        let lease = skill_studio_host::FileLease::new(lease_dir);
        let handle = skill_studio_core::ports::LeaseProvider::acquire(
            &lease,
            &[skill_studio_core::ports::LeaseKey {
                canonical_root: root_path.canonicalize().unwrap(),
            }],
            skill_studio_core::ports::LeaseMode::Exclusive,
            std::time::Duration::from_secs(5),
        )
        .expect("acquire exclusive lease");
        let guard = skill_studio_core::ports::ExclusiveGuard::from_handle(handle);

        let plan = skill_studio_core::journal::PlanWriter::begin(
            &store,
            &guard,
            skill_studio_core::identity::PlanId("01PLANDESKTOPTEST000000001".into()),
            Utc::now(),
            "desktop reconcile test",
            root_path.clone(),
            Vec::new(),
        )
        .expect("begin plan");

        skill_studio_core::fsops::link(&root, &plan, Path::new("link"), Path::new("target.txt"))
            .expect("link");

        let id = plan.id().clone();
        drop(plan); // simulated crash: never call finish

        let report = skill_studio_core::journal::reconcile(&store, &guard, fs_port.as_ref())
            .expect("reconcile");
        assert!(
            report.reversed.contains(&id),
            "plan {id:?} must be reversed by startup reconciliation; report was {report:?}"
        );

        let record = skill_studio_core::ports::Journal::all(&store)
            .expect("read plans back")
            .into_iter()
            .find(|p| p.id == id)
            .expect("the plan begun above must still exist as a row, never deleted");
        assert_eq!(
            record.status,
            skill_studio_core::ports::PlanStatus::Reversed,
            "row must be left Reversed, naming the plan otherwise left open"
        );
    }

    /// Event history: a row settles once. Startup recovery may settle an
    /// `interrupted` row, and may rerun, but a `done` row must not turn
    /// `failed` later.
    /// Fails if a second `finish` overwrites a settled status (History would
    /// show a finished install as failed) or if recovery cannot settle an
    /// interrupted row.
    #[test]
    fn an_event_settles_once_and_recovery_settles_interrupted_rows_or_names_the_overwrite() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path());
        let status_of = |id: &str| store.get(id).unwrap().unwrap().status;

        let done = allocate_id();
        store
            .record(
                &done,
                &draft("install", "alpha", serde_json::json!({}), None, None),
            )
            .unwrap();
        store.finish(&done, EventStatus::Done).unwrap();
        store
            .finish(&done, EventStatus::Done)
            .expect("a rerun recovery settles the same status again");
        let second = store.finish(&done, EventStatus::Failed);
        assert!(second.is_err(), "a settled row accepted a second finish");
        assert_eq!(status_of(&done), "done");

        let interrupted = allocate_id();
        store
            .record(
                &interrupted,
                &draft("install", "beta", serde_json::json!({}), None, None),
            )
            .unwrap();
        store.reconcile_at_startup().unwrap();
        assert_eq!(status_of(&interrupted), "interrupted");
        store.finish(&interrupted, EventStatus::Failed).unwrap();
        assert_eq!(status_of(&interrupted), "failed");

        assert!(store.finish("no-such-event", EventStatus::Done).is_err());
    }

    #[test]
    fn record_list_roundtrip_preserves_fields_and_order() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path());

        let id1 = allocate_id();
        store
            .record(
                &id1,
                &draft("install", "alpha", serde_json::json!({"n": 1}), None, None),
            )
            .unwrap();
        store.finish(&id1, EventStatus::Done).unwrap();

        let id2 = allocate_id();
        store
            .record(
                &id2,
                &draft("remove", "alpha", serde_json::json!({"n": 2}), None, None),
            )
            .unwrap();
        store.finish(&id2, EventStatus::Done).unwrap();

        let rows = store.list(10, None).unwrap();
        assert_eq!(rows.len(), 2);
        // newest first
        assert_eq!(rows[0].id, id2);
        assert_eq!(rows[1].id, id1);
        assert_eq!(rows[1].kind, "install");
        assert_eq!(rows[1].skill, "alpha");
        assert_eq!(rows[1].payload, serde_json::json!({"n": 1}));
        assert_eq!(rows[1].status, "done");
        assert_eq!(rows[1].harness.as_deref(), Some("claude-code"));
        assert_eq!(rows[1].scope.as_deref(), Some("global"));

        let filtered = store.list(10, Some("alpha")).unwrap();
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn schema_migration_defaults_existing_events_to_restorable() {
        let tmp = tempfile::tempdir().unwrap();
        let app_data = tmp.path().join("app_data");
        fs::create_dir_all(&app_data).unwrap();
        let db_path = app_data.join("events.sqlite3");
        let connection = Connection::open(&db_path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE events (
                    id TEXT PRIMARY KEY, ts TEXT NOT NULL, kind TEXT NOT NULL,
                    skill TEXT NOT NULL, harness TEXT, scope TEXT, project_path TEXT,
                    payload TEXT NOT NULL, inverse TEXT, backup_dir TEXT,
                    status TEXT NOT NULL, reverted_by TEXT
                );
                INSERT INTO events VALUES
                    ('legacy', '2026-09-05T00:00:00Z', 'install', 'find-bugs', NULL,
                     NULL, NULL, '{}', NULL, NULL, 'done', NULL);",
            )
            .unwrap();
        drop(connection);

        let store = EventStore::open(&app_data).unwrap();
        assert!(store.get("legacy").unwrap().unwrap().restorable);
    }

    #[test]
    fn open_migrates_legacy_events_schema_without_backup_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let app_data = tmp.path().join("app_data");
        fs::create_dir_all(&app_data).unwrap();
        let db_path = app_data.join("events.sqlite3");
        let legacy = Connection::open(&db_path).unwrap();
        legacy
            .execute_batch(
                "CREATE TABLE events (
                    id TEXT PRIMARY KEY, ts TEXT NOT NULL, kind TEXT NOT NULL,
                    skill TEXT NOT NULL, harness TEXT, scope TEXT, project_path TEXT,
                    payload TEXT NOT NULL, inverse TEXT, status TEXT NOT NULL, reverted_by TEXT
                );
                INSERT INTO events (id, ts, kind, skill, payload, status)
                VALUES ('legacy', '2026-01-01T00:00:00Z', 'install', 'old', '{}', 'done');",
            )
            .unwrap();
        drop(legacy);

        let store = EventStore::open(&app_data).unwrap();
        let id = allocate_id();
        let backup_dir = format!("backups/{id}");
        store
            .record(
                &id,
                &draft(
                    "remove",
                    "new",
                    serde_json::json!({}),
                    None,
                    Some(backup_dir.clone()),
                ),
            )
            .unwrap();
        store.finish(&id, EventStatus::Done).unwrap();

        let rows = store.list(10, None).unwrap();
        assert_eq!(rows[0].backup_dir.as_deref(), Some(backup_dir.as_str()));
        assert_eq!(rows[1].id, "legacy");
        assert_eq!(rows[1].backup_dir, None);
    }

    #[test]
    fn backup_and_restore_a_skill_folder_is_byte_identical() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path());

        let skill_dir = tmp.path().join(".agents/skills").join("my-skill");
        fs::create_dir_all(skill_dir.join("nested")).unwrap();
        fs::write(skill_dir.join("SKILL.md"), b"---\nname: my-skill\n---\n").unwrap();
        fs::write(skill_dir.join("nested/file.txt"), b"hello").unwrap();
        symlink("nested/file.txt", skill_dir.join("link")).unwrap();

        let fp_before = fingerprint_path(&skill_dir);

        let id = allocate_id();
        store
            .backup_paths(&id, std::slice::from_ref(&skill_dir))
            .unwrap();
        fs::remove_dir_all(&skill_dir).unwrap();

        let inverse = InverseOp::RestoreBackup {
            path: skill_dir.clone(),
            pre_fingerprint: fp_before.clone(),
            post_fingerprint: Some("absent".to_string()),
        };
        store
            .record(
                &id,
                &draft(
                    "remove",
                    "my-skill",
                    serde_json::json!({}),
                    Some(serde_json::to_value(&inverse).unwrap()),
                    Some(format!("backups/{id}")),
                ),
            )
            .unwrap();
        store.finish(&id, EventStatus::Done).unwrap();

        assert_eq!(fingerprint_path(&skill_dir), "absent");

        store
            .restore(&mutation(tmp.path()), tmp.path(), &id, false)
            .unwrap();
        assert_eq!(fingerprint_path(&skill_dir), fp_before);
    }

    #[test]
    fn restore_of_reverted_event_fails_and_restore_of_restore_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path());

        let path = tmp.path().join(".agents/skills").join("beta");
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("SKILL.md"), b"beta").unwrap();
        let fp_before = fingerprint_path(&path);

        let id = allocate_id();
        store
            .backup_paths(&id, std::slice::from_ref(&path))
            .unwrap();
        fs::remove_dir_all(&path).unwrap();
        let inverse = InverseOp::RestoreBackup {
            path: path.clone(),
            pre_fingerprint: fp_before.clone(),
            post_fingerprint: Some("absent".to_string()),
        };
        store
            .record(
                &id,
                &draft(
                    "remove",
                    "beta",
                    serde_json::json!({}),
                    Some(serde_json::to_value(&inverse).unwrap()),
                    Some(format!("backups/{id}")),
                ),
            )
            .unwrap();
        store.finish(&id, EventStatus::Done).unwrap();

        let lease = mutation(tmp.path());
        let restore_id = store.restore(&lease, tmp.path(), &id, false).unwrap();
        assert_eq!(fingerprint_path(&path), fp_before);

        let err = store.restore(&lease, tmp.path(), &id, false).unwrap_err();
        assert!(err.contains("already restored"), "unexpected error: {err}");

        // Restore-of-restore: brings the path back to "absent", the state
        // right before the first restore ran.
        store
            .restore(&lease, tmp.path(), &restore_id, false)
            .unwrap();
        assert_eq!(fingerprint_path(&path), "absent");
    }

    #[test]
    fn repair_undo_and_redo_hold_skill_md_transaction_but_other_files_do_not() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path());
        let skill_md = tmp.path().join(".agents/skills/sample/SKILL.md");
        fs::create_dir_all(skill_md.parent().unwrap()).unwrap();
        fs::write(&skill_md, b"malformed frontmatter").unwrap();
        let malformed_fingerprint = fingerprint_path(&skill_md);

        let repair_id = allocate_id();
        store
            .backup_paths(&repair_id, std::slice::from_ref(&skill_md))
            .unwrap();
        fs::write(&skill_md, b"repaired frontmatter").unwrap();
        let repair_inverse = InverseOp::RestoreBackup {
            path: skill_md.clone(),
            pre_fingerprint: malformed_fingerprint,
            post_fingerprint: Some(fingerprint_path(&skill_md)),
        };
        store
            .record(
                &repair_id,
                &draft(
                    "repair_skill_frontmatter",
                    "sample",
                    serde_json::json!({}),
                    Some(serde_json::to_value(repair_inverse).unwrap()),
                    Some(format!("backups/{repair_id}")),
                ),
            )
            .unwrap();
        store.finish(&repair_id, EventStatus::Done).unwrap();
        let lease_root = tmp.path().join("leases");
        let lease = mutation(tmp.path());

        let undo_id = store
            .restore_with_skill_md_transaction_observer(
                &lease,
                tmp.path(),
                &repair_id,
                false,
                |transaction| {
                    assert!(transaction.is_some());
                    assert!(exclusive_lease_is_held(&lease_root, tmp.path()));
                },
            )
            .unwrap();
        assert_eq!(fs::read(&skill_md).unwrap(), b"malformed frontmatter");
        assert_eq!(
            store
                .get(&repair_id)
                .unwrap()
                .unwrap()
                .reverted_by
                .as_deref(),
            Some(undo_id.as_str())
        );

        let redo_id = store
            .restore_with_skill_md_transaction_observer(
                &lease,
                tmp.path(),
                &undo_id,
                false,
                |transaction| {
                    assert!(transaction.is_some());
                    assert!(exclusive_lease_is_held(&lease_root, tmp.path()));
                },
            )
            .unwrap();
        assert_eq!(fs::read(&skill_md).unwrap(), b"repaired frontmatter");
        assert_eq!(
            store.get(&undo_id).unwrap().unwrap().reverted_by.as_deref(),
            Some(redo_id.as_str())
        );
        assert_eq!(store.get(&redo_id).unwrap().unwrap().status, "done");

        let ordinary_file = tmp.path().join(".agents/skills/sample/notes.md");
        fs::write(&ordinary_file, b"before").unwrap();
        let ordinary_before_fingerprint = fingerprint_path(&ordinary_file);
        let ordinary_id = allocate_id();
        store
            .backup_paths(&ordinary_id, std::slice::from_ref(&ordinary_file))
            .unwrap();
        fs::write(&ordinary_file, b"after").unwrap();
        let ordinary_inverse = InverseOp::RestoreBackup {
            path: ordinary_file.clone(),
            pre_fingerprint: ordinary_before_fingerprint,
            post_fingerprint: Some(fingerprint_path(&ordinary_file)),
        };
        store
            .record(
                &ordinary_id,
                &draft(
                    "update_notes",
                    "sample",
                    serde_json::json!({}),
                    Some(serde_json::to_value(ordinary_inverse).unwrap()),
                    Some(format!("backups/{ordinary_id}")),
                ),
            )
            .unwrap();
        store.finish(&ordinary_id, EventStatus::Done).unwrap();

        store
            .restore_with_skill_md_transaction_observer(
                &lease,
                tmp.path(),
                &ordinary_id,
                false,
                |transaction| {
                    assert!(transaction.is_none());
                },
            )
            .unwrap();
        assert_eq!(fs::read(ordinary_file).unwrap(), b"before");
    }

    #[test]
    fn failed_event_keeps_status_and_backup_pending_flips_to_interrupted() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path());

        let path = tmp.path().join(".agents/skills").join("gamma");
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("SKILL.md"), b"gamma").unwrap();

        let failed_id = allocate_id();
        store
            .backup_paths(&failed_id, std::slice::from_ref(&path))
            .unwrap();
        store
            .record(
                &failed_id,
                &draft(
                    "remove",
                    "gamma",
                    serde_json::json!({}),
                    None,
                    Some(format!("backups/{failed_id}")),
                ),
            )
            .unwrap();
        store.finish(&failed_id, EventStatus::Failed).unwrap();

        let rows = store.list(10, Some("gamma")).unwrap();
        assert_eq!(rows[0].status, "failed");
        assert!(tmp
            .path()
            .join("app_data/backups")
            .join(&failed_id)
            .join("manifest.json")
            .exists());

        let pending_id = allocate_id();
        store
            .record(
                &pending_id,
                &draft("remove", "gamma", serde_json::json!({}), None, None),
            )
            .unwrap();

        let flipped = store.reconcile_at_startup().unwrap();
        assert_eq!(flipped.len(), 1);
        assert_eq!(flipped[0].id, pending_id);
        assert_eq!(flipped[0].status, "interrupted");
    }

    #[test]
    fn drift_guard_refuses_without_force_and_force_preserves_drifted_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store(tmp.path());

        let file = tmp
            .path()
            .join(".agents/skills")
            .join("delta")
            .join("SKILL.md");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, b"original").unwrap();
        let fp_original = fingerprint_path(&file);

        let id = allocate_id();
        store
            .backup_paths(&id, std::slice::from_ref(&file))
            .unwrap();
        fs::write(&file, b"post-event").unwrap();
        let fp_post_event = fingerprint_path(&file);
        let inverse = InverseOp::RestoreBackup {
            path: file.clone(),
            pre_fingerprint: fp_original.clone(),
            post_fingerprint: Some(fp_post_event),
        };
        store
            .record(
                &id,
                &draft(
                    "update",
                    "delta",
                    serde_json::json!({}),
                    Some(serde_json::to_value(&inverse).unwrap()),
                    Some(format!("backups/{id}")),
                ),
            )
            .unwrap();
        store.finish(&id, EventStatus::Done).unwrap();

        // Drift: someone edits the file after the event.
        fs::write(&file, b"drifted-by-user").unwrap();

        let lease = WriteLease::with_lease_root(tmp.path().join("leases"))
            .try_acquire(tmp.path())
            .unwrap();
        let err = store.restore(&lease, tmp.path(), &id, false).unwrap_err();
        assert!(
            err.contains(&file.display().to_string()),
            "error should name the path: {err}"
        );
        assert_eq!(fs::read(&file).unwrap(), b"drifted-by-user");

        let restore_id = store.restore(&lease, tmp.path(), &id, true).unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"original");

        // The drifted bytes must be recoverable from the restore event's backup.
        store
            .restore(&lease, tmp.path(), &restore_id, false)
            .unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"drifted-by-user");
    }
}
