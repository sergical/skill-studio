//! Moving a skill folder for park and unpark: a rename, or a verified copy
//! when the parked root is on another volume.

use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{CoreError, ErrorCode};
use crate::ports::{confine, FileKind, MutationSession, Runtime, ScopeFs};

/// Largest single file the cross-volume copy reads into memory.
const COPY_FILE_MAX_BYTES: u64 = 256 * 1024 * 1024;

/// `EXDEV`: the same number on macOS and Linux.
const EXDEV: i32 = 18;

/// One entry under a folder being copied, relative to that folder.
#[derive(Debug)]
struct TreeEntry {
    relative: PathBuf,
    kind: FileKind,
    mode: Option<u32>,
    hash: Option<Vec<u8>>,
}

/// True when `err` is a rename refused because source and destination are on
/// different volumes.
pub(crate) fn crosses_devices(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::CrossesDevices || err.raw_os_error() == Some(EXDEV)
}

pub(crate) struct MoveFailure {
    pub error: CoreError,
    pub destination_published: bool,
}

impl From<MoveFailure> for CoreError {
    fn from(failure: MoveFailure) -> Self {
        failure.error
    }
}

/// Moves the folder `from` to `to`. A rename when both sit on one volume.
/// Across volumes: copies and flushes the folder, then quarantines the source
/// on its own volume before comparing its identity, entries, modes and hashes.
/// Drift keeps both trees; the quarantine is never swept as stale trash.
pub(crate) fn move_dir(
    rt: &Runtime,
    session: &MutationSession,
    from: &Path,
    to: &Path,
) -> Result<(), MoveFailure> {
    let mut destination_published = false;
    let result = (|| {
        let fs = rt.ports.fs.as_ref();
        for dir in [from.parent(), to.parent()].into_iter().flatten() {
            sweep_trash(rt, session, dir);
        }
        let scoped_from = confine(&rt.scope, fs, from)?;
        let scoped_to = confine(&rt.scope, fs, to)?;
        match fs.rename(&session.guard, &scoped_from, &scoped_to) {
            Ok(()) => {
                destination_published = true;
                Ok(())
            }
            Err(e) if crosses_devices(&e) => {
                copy_verify_remove(rt, session, from, to, &mut destination_published)
            }
            Err(e) => Err(CoreError::io(from, e)),
        }
    })();
    result.map_err(|error| MoveFailure {
        error,
        destination_published,
    })
}

/// First half of a delete: renames the folder `dir` to a hidden
/// `.park-trash-*` sibling and returns it, so an agent never sees a
/// half-deleted skill. A failed rename leaves the folder as it was. Finish
/// with `remove_trash`; a delete that never runs leaves only the hidden
/// sibling, which a later move or discard sweeps.
pub(crate) fn move_to_trash(
    rt: &Runtime,
    session: &MutationSession,
    dir: &Path,
) -> Result<PathBuf, CoreError> {
    let fs = rt.ports.fs.as_ref();
    let parent = dir.parent().unwrap_or(dir);
    sweep_trash(rt, session, parent);
    let trash = parent.join(format!("{TRASH_PREFIX}{}", crate::fsops::unique_suffix()));
    let scoped_dir = confine(&rt.scope, fs, dir)?;
    let scoped_trash = confine(&rt.scope, fs, &trash)?;
    fs.rename(&session.guard, &scoped_dir, &scoped_trash)
        .map_err(|e| CoreError::io(dir, e))?;
    Ok(trash)
}

fn copy_verify_remove(
    rt: &Runtime,
    session: &MutationSession,
    from: &Path,
    to: &Path,
    destination_published: &mut bool,
) -> Result<(), CoreError> {
    let fs = rt.ports.fs.as_ref();
    // A link would be followed by the walk below, and the delete would then
    // reach the files it points at. A relative link would also break at `to`.
    let facts = fs
        .symlink_metadata(from)
        .map_err(|e| CoreError::io(from, e))?;
    if facts.kind != FileKind::Dir {
        return Err(CoreError::new(
            ErrorCode::Unsupported,
            "this skill is a link, so it cannot move to another disk",
        )
        .at(from));
    }
    let source_binding = fs
        .fsops_device_inode(from)
        .map_err(|e| CoreError::io(from, e))?;
    let mut tree = walk(fs, from).map_err(|e| CoreError::io(from, e))?;
    if let Some(entry) = tree
        .iter()
        .find(|e| !matches!(e.kind, FileKind::Dir | FileKind::File))
    {
        return Err(CoreError::new(
            ErrorCode::Unsupported,
            "this folder holds a link or special file, so it cannot move to another disk",
        )
        .at(from.join(&entry.relative)));
    }

    let parent = to.parent().unwrap_or(to);
    confine(&rt.scope, fs, parent)?;
    let bytes = tree
        .iter()
        .filter(|entry| entry.kind == FileKind::File)
        .try_fold(0u64, |sum, entry| {
            fs.symlink_metadata(&from.join(&entry.relative))
                .map(|facts| sum.saturating_add(facts.len))
        })
        .map_err(|e| CoreError::io(from, e))?;
    let stage = loop {
        let stage = parent.join(format!(".park-stage-{}", ulid::Ulid::generate()));
        session.store.record_move_copy(
            &session.guard,
            from,
            to,
            &stage,
            "stage_before_create",
            bytes,
        )?;
        confine(&rt.scope, fs, &stage)?;
        match fs.fsops_create_dir(&stage) {
            Ok(()) => break stage,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(CoreError::io(&stage, e)),
        }
    };
    fs.fsops_fsync_dir(parent)
        .map_err(|e| CoreError::io(parent, e))?;
    let preserves_modes = fs
        .fsops_preserves_modes(&stage)
        .map_err(|e| CoreError::io(parent, e))?;
    let stage_binding = fs
        .fsops_device_inode(&stage)
        .map_err(|e| CoreError::io(&stage, e))?;
    let copied = copy_tree(rt, from, &stage, &mut tree, preserves_modes, facts.mode)
        .and_then(|()| {
            verify_copy(fs, &stage, &tree, preserves_modes, facts.mode)
                .map_err(|e| CoreError::io(to, e))
        })
        .and_then(|()| flush_tree(fs, &stage, &tree).map_err(|e| CoreError::io(to, e)));
    if let Err(e) = copied {
        return Err(CoreError::new(
            e.code,
            format!(
                "{}; incomplete staging copy retained at {}",
                e.message,
                stage.display()
            ),
        )
        .at(&stage));
    }
    if fs.fsops_device_inode(&stage).ok() != Some(stage_binding)
        || verify_copy(fs, &stage, &tree, preserves_modes, facts.mode).is_err()
    {
        return Err(CoreError::new(
            ErrorCode::DriftConflict,
            format!(
                "the staging copy changed during sync; source and staging copy kept at {} and {}",
                from.display(),
                stage.display()
            ),
        )
        .at(&stage));
    }
    let scoped_stage = confine(&rt.scope, fs, &stage)?;
    let scoped_to = confine(&rt.scope, fs, to)?;
    fs.rename(&session.guard, &scoped_stage, &scoped_to)
        .map_err(|e| {
            CoreError::new(
                ErrorCode::Io,
                format!(
                    "publish failed; staging copy retained at {}: {e}",
                    stage.display()
                ),
            )
            .at(&stage)
        })?;
    *destination_published = true;
    flush_tree(fs, to, &tree).map_err(|e| CoreError::io(to, e))?;

    // Conflict copies must never enter the trash sweeper's namespace.
    let trash = from.parent().unwrap_or(from).join(format!(
        ".park-quarantine-{}",
        crate::fsops::unique_suffix()
    ));
    session.store.record_move_copy(
        &session.guard,
        from,
        to,
        &trash,
        "retain_before_rename",
        bytes,
    )?;
    let source_parent = from.parent().unwrap_or(from);
    fs.fsops_fsync_dir(source_parent)
        .map_err(|e| CoreError::io(source_parent, e))?;
    let scoped_from = confine(&rt.scope, fs, from)?;
    let scoped_trash = confine(&rt.scope, fs, &trash)?;
    if let Err(e) = fs.rename(&session.guard, &scoped_from, &scoped_trash) {
        if fs.fsops_device_inode(from).ok() == Some(source_binding)
            && verify_copy(fs, from, &tree, true, facts.mode).is_ok()
        {
            return Err(CoreError::io(from, e));
        }
        return Err(CoreError::new(
            ErrorCode::DriftConflict,
            format!("the source changed before quarantine; both trees were kept: {e}"),
        )
        .at(from));
    }
    fs.fsops_fsync_dir(source_parent)
        .map_err(|e| CoreError::io(&trash, e))?;
    let rechecked = (|| {
        if fs.symlink_metadata(&trash)?.kind != FileKind::Dir
            || fs.fsops_device_inode(&trash)? != source_binding
        {
            return Err(io::Error::other("the source folder was replaced"));
        }
        verify_copy(fs, &trash, &tree, true, facts.mode)?;
        match fs.symlink_metadata(from) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            _ => Err(io::Error::other("the source path was recreated")),
        }
    })();
    if let Err(e) = rechecked {
        return Err(CoreError::new(
            ErrorCode::DriftConflict,
            format!(
                "the source changed during sync; retained at {}: {e}",
                trash.display()
            ),
        )
        .at(&trash));
    }

    // Open handles can still write after any hash pass. The journal names
    // this backup, which must never enter the trash sweeper's namespace.
    Ok(())
}

/// Prefix of the hidden folder a cross-volume move renames its source to.
const TRASH_PREFIX: &str = ".park-trash-";

/// True for exactly the names `unique_suffix` makes after the prefix
/// (`<digits>-<digits>`), so a user's own `.park-trash-notes` is never swept.
fn is_trash_name(name: &str) -> bool {
    name.strip_prefix(TRASH_PREFIX)
        .and_then(|rest| rest.split_once('-'))
        .is_some_and(|(pid, n)| {
            !pid.is_empty()
                && !n.is_empty()
                && pid.bytes().all(|b| b.is_ascii_digit())
                && n.bytes().all(|b| b.is_ascii_digit())
        })
}

/// Most stale trash folders one sweep removes, so a sweep stays cheap.
const SWEEP_LIMIT: usize = 16;

/// Deletes a trash folder best effort. Every `SKILL.md` (any case) goes first,
/// so a delete that stops partway never leaves a folder an agent could load
/// as a skill.
pub(crate) fn remove_trash(rt: &Runtime, session: &MutationSession, trash: &Path) {
    let fs = rt.ports.fs.as_ref();
    // Listed earlier, so it may have changed to a link since.
    if !fs
        .symlink_metadata(trash)
        .is_ok_and(|facts| facts.kind == FileKind::Dir)
    {
        return;
    }
    if let Ok(tree) = walk(fs, trash) {
        for entry in tree.iter().filter(|e| {
            e.kind == FileKind::File
                && e.relative
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("SKILL.md"))
        }) {
            let path = trash.join(&entry.relative);
            if confine(&rt.scope, fs, &path).is_ok() {
                let _ = fs.fsops_remove_file(&path);
            }
        }
    }
    let _ = remove_tree(rt, session, trash);
}

/// Removes stale `.park-trash-<digits>-<digits>` folders left directly under
/// `dir` by an earlier move whose delete failed. Only real folders with that
/// exact name shape are touched; a link is never followed. Best effort.
fn sweep_trash(rt: &Runtime, session: &MutationSession, dir: &Path) {
    let fs = rt.ports.fs.as_ref();
    let Ok(entries) = fs.read_dir(dir) else {
        return;
    };
    for entry in entries
        .iter()
        .filter(|e| e.kind == FileKind::Dir && is_trash_name(&e.name))
        .take(SWEEP_LIMIT)
    {
        remove_trash(rt, session, &dir.join(&entry.name));
    }
}

/// Flushes every file and directory of the copy at `to` to disk, so the
/// source is not renamed away while the copy is still only in memory.
fn flush_tree(fs: &dyn ScopeFs, to: &Path, tree: &[TreeEntry]) -> io::Result<()> {
    for entry in tree.iter().rev().filter(|e| e.kind == FileKind::Dir) {
        fs.fsops_fsync_dir(&to.join(&entry.relative))?;
    }
    fs.fsops_fsync_dir(to)?;
    if let Some(parent) = to.parent() {
        fs.fsops_fsync_dir(parent)?;
    }
    for entry in tree.iter().filter(|e| e.kind == FileKind::File) {
        fs.fsops_full_sync_file(&to.join(&entry.relative))?;
    }
    Ok(())
}

/// Every entry under `root`, directories before their contents.
fn walk(fs: &dyn ScopeFs, root: &Path) -> io::Result<Vec<TreeEntry>> {
    let mut out = Vec::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(relative_dir) = pending.pop() {
        let mut entries = fs.read_dir(&root.join(&relative_dir))?;
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        for entry in entries {
            let relative = relative_dir.join(&entry.name);
            let facts = fs.symlink_metadata(&root.join(&relative))?;
            if entry.kind == FileKind::Dir {
                pending.push(relative.clone());
            }
            out.push(TreeEntry {
                relative,
                kind: entry.kind,
                mode: facts.mode,
                hash: None,
            });
        }
    }
    out.sort_by(|a, b| a.relative.cmp(&b.relative));
    Ok(out)
}

fn copy_tree(
    rt: &Runtime,
    from: &Path,
    to: &Path,
    tree: &mut [TreeEntry],
    preserves_modes: bool,
    root_mode: Option<u32>,
) -> Result<(), CoreError> {
    let fs = rt.ports.fs.as_ref();
    confine(&rt.scope, fs, to)?;
    for entry in tree.iter_mut() {
        let source = from.join(&entry.relative);
        let dest = to.join(&entry.relative);
        // Confine each destination before the unscoped write below.
        confine(&rt.scope, fs, &dest)?;
        if entry.kind == FileKind::Dir {
            fs.fsops_create_dir(&dest)
                .map_err(|e| CoreError::io(&dest, e))?;
        } else {
            let bytes = fs
                .read_capped(&source, COPY_FILE_MAX_BYTES)
                .map_err(|e| CoreError::io(&source, e))?;
            let written = match entry.mode.filter(|_| preserves_modes) {
                Some(mode) => fs.fsops_write_new_file_with_mode(&dest, &bytes, mode & 0o7777),
                None => fs.fsops_write_new_file(&dest, &bytes),
            };
            written.map_err(|e| CoreError::io(&dest, e))?;
            entry.hash = Some(Sha256::digest(&bytes).to_vec());
        }
    }
    if preserves_modes {
        for entry in tree.iter().rev().filter(|e| e.kind == FileKind::Dir) {
            if let Some(mode) = entry.mode {
                let path = to.join(&entry.relative);
                confine(&rt.scope, fs, &path)?;
                fs.fsops_set_directory_mode(&path, mode & 0o7777)
                    .map_err(|e| CoreError::io(&path, e))?;
            }
        }
        if let Some(mode) = root_mode {
            confine(&rt.scope, fs, to)?;
            fs.fsops_set_directory_mode(to, mode & 0o7777)
                .map_err(|e| CoreError::io(to, e))?;
        }
    }
    Ok(())
}

/// Fails unless `to` holds the same entries as `tree`, and every file hashes
/// the same as the bytes captured during copying.
fn verify_copy(
    fs: &dyn ScopeFs,
    to: &Path,
    tree: &[TreeEntry],
    preserves_modes: bool,
    root_mode: Option<u32>,
) -> io::Result<()> {
    let same_mode = |copied: Option<u32>, original: Option<u32>| {
        !preserves_modes
            || original.is_none()
            || copied.map(|mode| mode & 0o7777) == original.map(|mode| mode & 0o7777)
    };
    if !same_mode(fs.symlink_metadata(to)?.mode, root_mode) {
        return Err(io::Error::other(
            "the copied root has different permission bits",
        ));
    }
    let copied = walk(fs, to)?;
    let same_shape = copied.len() == tree.len()
        && copied.iter().zip(tree).all(|(a, b)| {
            a.relative == b.relative && a.kind == b.kind && same_mode(a.mode, b.mode)
        });
    if !same_shape {
        return Err(io::Error::other(
            "the copy does not list the same files and modes",
        ));
    }
    for entry in tree.iter().filter(|e| e.kind == FileKind::File) {
        let bytes = fs.read_capped(&to.join(&entry.relative), COPY_FILE_MAX_BYTES)?;
        if entry.hash.as_ref() != Some(&Sha256::digest(bytes).to_vec()) {
            return Err(io::Error::other(format!(
                "{} differs in the copy",
                entry.relative.display()
            )));
        }
    }
    Ok(())
}

/// Deletes `root` and everything under it (files and directories only).
fn remove_tree(rt: &Runtime, session: &MutationSession, root: &Path) -> Result<(), CoreError> {
    let fs = rt.ports.fs.as_ref();
    confine(&rt.scope, fs, root)?;
    if let Ok(facts) = fs.symlink_metadata(root) {
        if let Some(mode) = facts.mode {
            let _ = fs.fsops_set_directory_mode(root, (mode & 0o7777) | 0o700);
        }
    }
    let tree = walk(fs, root).map_err(|e| CoreError::io(root, e))?;
    for entry in tree.iter().filter(|entry| entry.kind == FileKind::Dir) {
        if let Some(mode) = entry.mode {
            let path = root.join(&entry.relative);
            confine(&rt.scope, fs, &path)?;
            let _ = fs.fsops_set_directory_mode(&path, (mode & 0o7777) | 0o700);
        }
    }
    for entry in tree.iter().rev() {
        let path = root.join(&entry.relative);
        if entry.kind == FileKind::Dir {
            confine(&rt.scope, fs, &path)?;
            fs.fsops_remove_dir(&path)
                .map_err(|e| CoreError::io(&path, e))?;
        } else {
            let scoped = confine(&rt.scope, fs, &path)?;
            fs.remove_file(&session.guard, &scoped)
                .map_err(|e| CoreError::io(&path, e))?;
        }
    }
    confine(&rt.scope, fs, root)?;
    fs.fsops_remove_dir(root)
        .map_err(|e| CoreError::io(root, e))
}

pub(crate) fn retained_bytes(fs: &dyn ScopeFs, path: &Path) -> io::Result<u64> {
    if fs.symlink_metadata(path)?.kind != FileKind::Dir {
        return Err(io::Error::other("retained path is not a directory"));
    }
    walk(fs, path)?
        .iter()
        .filter(|entry| entry.kind == FileKind::File)
        .try_fold(0u64, |sum, entry| {
            fs.symlink_metadata(&path.join(&entry.relative))
                .map(|facts| sum.saturating_add(facts.len))
        })
}
