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
//! A stamp cannot see everything: a folder swapped for another and back
//! while its facts were read (A, then B, then A again) leaves the stamp the
//! same, and some network filesystems report times that do not change. So a
//! kept result is used for at most [`MAX_AGE`]; after that the folder is
//! read in full again. A wall clock set back also ends the use of every
//! result kept at a later time. This bounds any wrong result the stamp
//! misses.
//!
//! Only a long-lived host gains from this (the desktop app). The host puts
//! one memo in [`crate::ports::Ports::scan_memo`]; `None` scans as before.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use chrono::{DateTime, Utc};

use crate::error::CoreError;
use crate::ops::CachedSkillRead;
use crate::ports::{
    DirEntryFacts, DirHandle, ExclusiveGuard, FileFacts, FileKind, OpContext, OpenedFile, ScopeFs,
    ScopedPath,
};

/// How recent a change must be for [`folder_stamp`] to refuse a stamp.
const RACY_WINDOW: chrono::Duration = chrono::Duration::seconds(2);

/// How long a kept result is used before the folder is read again.
const MAX_AGE: chrono::Duration = chrono::Duration::minutes(10);

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
    kept_at: DateTime<Utc>,
    read: CachedSkillRead,
}

impl ScanMemo {
    /// An empty memo.
    pub fn new() -> Self {
        Self::default()
    }

    /// The kept result for `dir` read as skill `name`, when its stamp is
    /// `stamp` and it was kept less than [`MAX_AGE`] before `now`, and not
    /// after `now` (a clock set back).
    pub(crate) fn get(
        &self,
        dir: &Path,
        name: &str,
        stamp: &[u8],
        now: DateTime<Utc>,
    ) -> Option<CachedSkillRead> {
        let folders = self.folders.lock().ok()?;
        let kept = folders.get(dir)?;
        let age = now - kept.kept_at;
        (kept.name == name
            && kept.stamp == stamp
            && age >= chrono::Duration::zero()
            && age < MAX_AGE)
            .then(|| kept.read.clone())
    }

    pub(crate) fn put(
        &self,
        dir: &Path,
        name: &str,
        stamp: Vec<u8>,
        kept_at: DateTime<Utc>,
        read: CachedSkillRead,
    ) {
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
                kept_at,
                read,
            },
        );
    }
}

/// The stamp of skill folder `dir` at `now`, or `None` when the folder
/// cannot be kept: an entry has no ctime or inode (a test filesystem), an
/// entry changed less than [`RACY_WINDOW`] before `now`, a folder could not
/// be listed, a link target cannot be read, or the folder has more than
/// [`MAX_STAMP_ENTRIES`] entries.
///
/// Entries are listed in name order and never followed into a linked
/// folder, as the facts walk does. A link adds the facts of its target too,
/// so a link that now points at other content changes the stamp.
///
/// The caller keeps a result only when every call behind it succeeded
/// ([`FailureWatch`]).
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

    /// Adds the facts of what `path` points at. A link whose target cannot
    /// be read (missing, or outside the scope) refuses the stamp: a read
    /// through the link can still see bytes whose changes no stamp shows.
    fn push_target(&mut self, fs: &dyn ScopeFs, path: &Path) -> Option<()> {
        let target = fs
            .canonicalize(path)
            .and_then(|target| fs.symlink_metadata(&target))
            .ok()?;
        self.push(Path::new(""), &target)
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

/// A [`ScopeFs`] that notes when any call through it fails. A skill folder
/// that the stamp could read in full has no missing entry, so a failed call
/// while its facts are read means the result is not the folder's true
/// result, and the memo must not keep it.
pub(crate) struct FailureWatch<'a> {
    inner: &'a dyn ScopeFs,
    failed: AtomicBool,
}

impl<'a> FailureWatch<'a> {
    pub(crate) fn new(inner: &'a dyn ScopeFs) -> Self {
        Self {
            inner,
            failed: AtomicBool::new(false),
        }
    }

    pub(crate) fn failed(&self) -> bool {
        self.failed.load(Ordering::SeqCst)
    }

    fn note<T>(&self, result: std::io::Result<T>) -> std::io::Result<T> {
        if result.is_err() {
            self.failed.store(true, Ordering::SeqCst);
        }
        result
    }
}

impl ScopeFs for FailureWatch<'_> {
    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        self.note(self.inner.canonicalize(path))
    }
    fn symlink_metadata(&self, path: &Path) -> std::io::Result<FileFacts> {
        self.note(self.inner.symlink_metadata(path))
    }
    fn read_link(&self, path: &Path) -> std::io::Result<PathBuf> {
        self.note(self.inner.read_link(path))
    }
    fn read_dir(&self, path: &Path) -> std::io::Result<Vec<DirEntryFacts>> {
        self.note(self.inner.read_dir(path))
    }
    fn visit_dir(&self, path: &Path, visit: &mut dyn FnMut(DirEntryFacts)) -> std::io::Result<()> {
        self.note(self.inner.visit_dir(path, visit))
    }
    fn ancestor_holds(&self, start: &Path, name: &str) -> std::io::Result<bool> {
        self.note(self.inner.ancestor_holds(start, name))
    }
    fn read_capped(&self, path: &Path, max_bytes: u64) -> std::io::Result<Vec<u8>> {
        self.note(self.inner.read_capped(path, max_bytes))
    }
    fn open_file(&self, path: &Path) -> std::io::Result<OpenedFile> {
        self.note(self.inner.open_file(path))
    }
    fn read_prefix(&self, path: &Path, limit: u64) -> std::io::Result<(Vec<u8>, bool)> {
        self.note(self.inner.read_prefix(path, limit))
    }
    fn write_atomic(
        &self,
        guard: &ExclusiveGuard,
        path: &ScopedPath,
        bytes: &[u8],
    ) -> std::io::Result<()> {
        self.note(self.inner.write_atomic(guard, path, bytes))
    }
    fn rename(
        &self,
        guard: &ExclusiveGuard,
        from: &ScopedPath,
        to: &ScopedPath,
    ) -> std::io::Result<()> {
        self.note(self.inner.rename(guard, from, to))
    }
    fn remove_file(&self, guard: &ExclusiveGuard, path: &ScopedPath) -> std::io::Result<()> {
        self.note(self.inner.remove_file(guard, path))
    }
    fn create_dir_all(&self, guard: &ExclusiveGuard, path: &ScopedPath) -> std::io::Result<()> {
        self.note(self.inner.create_dir_all(guard, path))
    }
    fn symlink(
        &self,
        guard: &ExclusiveGuard,
        target: &ScopedPath,
        link: &ScopedPath,
    ) -> std::io::Result<()> {
        self.note(self.inner.symlink(guard, target, link))
    }
    fn symlink_relative(
        &self,
        guard: &ExclusiveGuard,
        target: &ScopedPath,
        relative_target: &Path,
        link: &ScopedPath,
    ) -> std::io::Result<()> {
        self.note(
            self.inner
                .symlink_relative(guard, target, relative_target, link),
        )
    }
    fn fsops_device_inode(&self, path: &Path) -> std::io::Result<(u64, u64)> {
        self.note(self.inner.fsops_device_inode(path))
    }
    fn fsops_fsync_file(&self, path: &Path) -> std::io::Result<()> {
        self.note(self.inner.fsops_fsync_file(path))
    }
    fn fsops_fsync_dir(&self, path: &Path) -> std::io::Result<()> {
        self.note(self.inner.fsops_fsync_dir(path))
    }
    fn fsops_create_dir(&self, path: &Path) -> std::io::Result<()> {
        self.note(self.inner.fsops_create_dir(path))
    }
    fn fsops_write_new_file(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        self.note(self.inner.fsops_write_new_file(path, bytes))
    }
    fn fsops_write_new_file_with_mode(
        &self,
        path: &Path,
        bytes: &[u8],
        mode: u32,
    ) -> std::io::Result<()> {
        self.note(self.inner.fsops_write_new_file_with_mode(path, bytes, mode))
    }
    fn fsops_rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        self.note(self.inner.fsops_rename(from, to))
    }
    fn fsops_symlink(&self, target: &Path, link: &Path) -> std::io::Result<()> {
        self.note(self.inner.fsops_symlink(target, link))
    }
    fn fsops_remove_dir(&self, path: &Path) -> std::io::Result<()> {
        self.note(self.inner.fsops_remove_dir(path))
    }
    fn fsops_remove_file(&self, path: &Path) -> std::io::Result<()> {
        self.note(self.inner.fsops_remove_file(path))
    }
    fn fsops_exchange(&self, a: &Path, b: &Path) -> std::io::Result<()> {
        self.note(self.inner.fsops_exchange(a, b))
    }
    fn fsops_bind_root(&self, root: &Path) -> std::io::Result<Option<Box<dyn ScopeFs + '_>>> {
        self.note(self.inner.fsops_bind_root(root))
    }
    fn fsops_open_dir(&self, path: &Path) -> std::io::Result<Option<Box<dyn DirHandle>>> {
        self.note(self.inner.fsops_open_dir(path))
    }
}
