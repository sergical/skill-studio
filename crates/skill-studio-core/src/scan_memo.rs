//! Skill folder facts kept from one scan to the next.
//!
//! A scan reads each `SKILL.md`, hashes each file of each skill folder, and
//! counts tokens. On a home with 529 skills this took about 1 s of CPU per
//! scan (measured 2026-11-19), and almost no folder changes between two
//! scans. A [`ScanMemo`] keeps each folder's result with a stamp: the
//! `lstat` facts (kind, size, mtime, ctime, inode) of each entry in the
//! folder, and of each link target. The next scan takes the stamp again,
//! with metadata calls only, and uses the kept result when the stamp is the
//! same.
//!
//! A write to a file always sets its ctime to the current time, and user
//! space cannot set ctime back, so a changed file always changes the stamp.
//! A folder with an entry that changed less than [`RACY_WINDOW`] ago is not
//! kept: a write in the same clock tick as the stamp could otherwise hide
//! (git's "racily clean" rule).
//!
//! Only a long-lived host gains from this (the desktop app). The host puts
//! one memo in [`crate::ports::Ports::scan_memo`]; `None` scans as before.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Utc};

use crate::error::CoreError;
use crate::ops::CachedSkillRead;
use crate::ports::{FileFacts, FileKind, OpContext, ScopeFs};

/// How recent a change must be for [`folder_stamp`] to refuse a stamp.
const RACY_WINDOW: chrono::Duration = chrono::Duration::seconds(2);

/// Entries one stamp may hold. A larger folder is not kept: the scan reads
/// it in full each time, as it did before the memo.
const MAX_STAMP_ENTRIES: usize = 4_096;

/// Folders one memo may hold. When it is full, it is emptied and fills
/// again on the next scan. Each folder is a few KB.
const MAX_FOLDERS: usize = 20_000;

/// The facts of each skill folder from earlier scans, keyed on the folder
/// path. See the module docs.
#[derive(Default)]
pub struct ScanMemo {
    folders: Mutex<HashMap<PathBuf, KeptFolder>>,
}

struct KeptFolder {
    name: String,
    stamp: Vec<u8>,
    read: CachedSkillRead,
}

impl ScanMemo {
    /// An empty memo.
    pub fn new() -> Self {
        Self::default()
    }

    /// The kept result for `dir` read as skill `name`, when its stamp is
    /// `stamp`.
    pub(crate) fn get(&self, dir: &Path, name: &str, stamp: &[u8]) -> Option<CachedSkillRead> {
        let folders = self.folders.lock().ok()?;
        let kept = folders.get(dir)?;
        (kept.name == name && kept.stamp == stamp).then(|| kept.read.clone())
    }

    pub(crate) fn put(&self, dir: &Path, name: &str, stamp: Vec<u8>, read: CachedSkillRead) {
        let Ok(mut folders) = self.folders.lock() else {
            return;
        };
        if folders.len() >= MAX_FOLDERS && !folders.contains_key(dir) {
            folders.clear();
        }
        folders.insert(
            dir.to_path_buf(),
            KeptFolder {
                name: name.to_owned(),
                stamp,
                read,
            },
        );
    }
}

/// The stamp of skill folder `dir` at `now`, or `None` when the folder
/// cannot be kept: an entry has no ctime or inode (a test filesystem), an
/// entry changed less than [`RACY_WINDOW`] before `now`, a folder could not
/// be listed, or the folder has more than [`MAX_STAMP_ENTRIES`] entries.
///
/// Entries are listed in name order and never followed into a linked
/// folder, as the facts walk does. A link adds the facts of its target too,
/// so a link that now points at other content changes the stamp.
pub(crate) fn folder_stamp(
    fs: &dyn ScopeFs,
    ctx: &OpContext,
    dir: &Path,
    now: DateTime<Utc>,
) -> Result<Option<Vec<u8>>, CoreError> {
    let mut stamp = Stamp {
        bytes: Vec::new(),
        entries: 0,
        changed_before: now - RACY_WINDOW,
    };
    let Ok(own) = fs.symlink_metadata(dir) else {
        return Ok(None);
    };
    let linked = own.kind == FileKind::Symlink;
    if stamp.push(Path::new(""), &own).is_none() || (linked && stamp.push_target(fs, dir).is_none())
    {
        return Ok(None);
    }
    Ok(stamp_dir(fs, ctx, dir, dir, &mut stamp)?.map(|()| stamp.bytes))
}

struct Stamp {
    bytes: Vec<u8>,
    entries: usize,
    changed_before: DateTime<Utc>,
}

impl Stamp {
    fn push(&mut self, rel: &Path, facts: &FileFacts) -> Option<()> {
        let modified = facts.modified?;
        let changed = facts.changed?;
        let inode = facts.inode?;
        if modified.max(changed) >= self.changed_before {
            return None;
        }
        self.entries += 1;
        if self.entries > MAX_STAMP_ENTRIES {
            return None;
        }
        let rel = rel.to_string_lossy();
        self.bytes
            .extend_from_slice(&(rel.len() as u64).to_le_bytes());
        self.bytes.extend_from_slice(rel.as_bytes());
        self.bytes.push(match facts.kind {
            FileKind::File => 0,
            FileKind::Dir => 1,
            FileKind::Symlink => 2,
            FileKind::Other => 3,
        });
        self.bytes.extend_from_slice(&facts.len.to_le_bytes());
        for time in [modified, changed] {
            self.bytes
                .extend_from_slice(&time.timestamp_nanos_opt()?.to_le_bytes());
        }
        self.bytes.extend_from_slice(&inode.to_le_bytes());
        Some(())
    }

    /// Adds the facts of what `path` points at, or a marker when it points
    /// at nothing.
    fn push_target(&mut self, fs: &dyn ScopeFs, path: &Path) -> Option<()> {
        if let Ok(target) = fs
            .canonicalize(path)
            .and_then(|target| fs.symlink_metadata(&target))
        {
            return self.push(Path::new(""), &target);
        }
        self.bytes.push(0xff);
        Some(())
    }
}

fn stamp_dir(
    fs: &dyn ScopeFs,
    ctx: &OpContext,
    root: &Path,
    dir: &Path,
    stamp: &mut Stamp,
) -> Result<Option<()>, CoreError> {
    let Ok(mut entries) = fs.read_dir(dir) else {
        return Ok(None);
    };
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    for entry in entries {
        ctx.checkpoint()?;
        let path = dir.join(&entry.name);
        let Ok(facts) = fs.symlink_metadata(&path) else {
            return Ok(None);
        };
        let rel = path.strip_prefix(root).unwrap_or(&path);
        if stamp.push(rel, &facts).is_none() {
            return Ok(None);
        }
        let more = match facts.kind {
            FileKind::Symlink => stamp.push_target(fs, &path),
            FileKind::Dir => stamp_dir(fs, ctx, root, &path, stamp)?,
            FileKind::File | FileKind::Other => Some(()),
        };
        if more.is_none() {
            return Ok(None);
        }
    }
    Ok(Some(()))
}
