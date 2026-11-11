//! Retention policy shared by completed history and journal backups.

/// Keep at least this many completed backups for each skill target.
pub const BACKUP_RETENTION_COUNT: usize = 20;

/// Keep every completed backup younger than this age.
pub const BACKUP_RETENTION_AGE: chrono::Duration = chrono::Duration::days(30);

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dto::RestoreCapability;
use crate::events::{EventRecord, EventStatus};
use crate::identity::EventId;

/// Payload marker left after a completed event's backup expires.
pub const BACKUP_EXPIRED_PAYLOAD_KEY: &str = "backup_expired";

/// The reason shown when retention has expired an event's backup.
pub const BACKUP_EXPIRED_REASON: &str = "Backup expired. Undo is not available.";

/// Whether retention has durably made this event's backup unavailable.
pub fn backup_is_expired(payload: &serde_json::Value) -> bool {
    payload
        .get(BACKUP_EXPIRED_PAYLOAD_KEY)
        .and_then(serde_json::Value::as_bool)
        == Some(true)
}

/// The targets a lease-held retention pass may inspect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum BackupRetentionScope {
    /// Every target covered by the held lease, for startup.
    All,
    /// Only the target of this successful write.
    TargetOf(EventId),
}

/// History metadata needed to decide which backups remain protected.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BackupRetentionEntry {
    /// The history row, with display-only agent labels omitted if necessary.
    pub event: EventRecord,
    /// The original primary path recorded when the backup was made.
    pub target: Option<PathBuf>,
}

impl BackupRetentionEntry {
    /// The physical skill target, including legacy inverse layouts.
    pub fn target(&self) -> Option<PathBuf> {
        let path = self.target.clone().or_else(|| {
            let inverse = self.event.inverse.as_ref()?;
            let field = match inverse.get("op")?.as_str()? {
                "restore_backup" => "path",
                "recreate_symlink" | "remove_symlink" => "link",
                "move_back" => "to",
                "undistribute_from_shared" => "shared_dir",
                _ => return None,
            };
            inverse.get(field)?.as_str().map(PathBuf::from)
        })?;
        Some(skill_target(&path))
    }
}

/// Selects expired completed event backups. Input ties retain their history
/// insertion order, so callers supply timestamp-descending, rowid-descending rows.
/// Unknown legacy targets and unfinished rows remain protected.
pub fn expired_event_backups(
    entries: &[BackupRetentionEntry],
    now: DateTime<Utc>,
    scope: &BackupRetentionScope,
) -> Vec<EventId> {
    let target_filter = match scope {
        BackupRetentionScope::All => None,
        BackupRetentionScope::TargetOf(id) => {
            let Some(target) = entries
                .iter()
                .find(|entry| &entry.event.id == id)
                .and_then(BackupRetentionEntry::target)
            else {
                return Vec::new();
            };
            Some(target)
        }
    };
    let recovery_references: HashSet<EventId> = entries
        .iter()
        .filter(|entry| {
            entry.event.reverted_by.is_none()
                && matches!(
                    entry.event.status,
                    EventStatus::Pending | EventStatus::Interrupted
                )
        })
        .filter_map(|entry| {
            entry
                .event
                .payload
                .get("target_event")?
                .as_str()
                .map(|id| EventId(id.to_owned()))
        })
        .collect();
    let mut targets: BTreeMap<PathBuf, Vec<&EventRecord>> = BTreeMap::new();
    for entry in entries {
        let Some(target) = entry.target() else {
            continue;
        };
        if target_filter
            .as_ref()
            .is_some_and(|filter| filter != &target)
        {
            continue;
        }
        targets.entry(target).or_default().push(&entry.event);
    }
    let mut expired = Vec::new();
    for events in targets.values_mut() {
        events.sort_by_key(|event| std::cmp::Reverse(event.ts));
        let newest_undoable = events
            .iter()
            .find(|event| event.restore_capability() == RestoreCapability::Yes)
            .map(|event| &event.id);
        let mut completed = 0;
        for event in events.iter() {
            if event.backup_dir.is_none() {
                continue;
            }
            match event.status {
                EventStatus::Done | EventStatus::Failed => {}
                EventStatus::Pending | EventStatus::Interrupted => continue,
            }
            let keep_by_count = completed < BACKUP_RETENTION_COUNT;
            completed += 1;
            if keep_by_count
                || now.signed_duration_since(event.ts) < BACKUP_RETENTION_AGE
                || newest_undoable == Some(&event.id)
                || recovery_references.contains(&event.id)
            {
                continue;
            }
            expired.push(event.id.clone());
        }
    }
    expired
}

/// Groups a skill's frontmatter file with its containing folder.
pub(crate) fn skill_target(path: &Path) -> PathBuf {
    if path.file_name().is_some_and(|name| name == "SKILL.md") {
        path.parent().unwrap_or(path).to_path_buf()
    } else {
        path.to_path_buf()
    }
}
