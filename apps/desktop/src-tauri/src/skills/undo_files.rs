use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use skill_studio_host::{copy_confined_entry, open_confined_child, open_confined_dir, ConfinedDir};

use super::agents::skill_roots;
use super::event_store::allocate_id;
use super::skill_refresh::effective_project_paths;

/// The files an Undo can change, bound before any of its filesystem writes.
pub(super) struct UndoFiles {
    paths: BTreeMap<PathBuf, UndoPath>,
}

impl UndoFiles {
    pub(super) fn bind<'a>(
        home: &Path,
        app_data: &Path,
        paths: impl IntoIterator<Item = &'a Path>,
    ) -> Result<Self, String> {
        let paths: Vec<_> = paths.into_iter().collect();
        let mut roots = skill_roots(home, &[]);
        if paths
            .iter()
            .any(|path| !roots.iter().any(|root| path.starts_with(&root.path)))
        {
            roots = skill_roots(home, &effective_project_paths(home)?);
        }
        let mut bound = BTreeMap::new();
        for path in paths {
            let physical = roots
                .iter()
                .find_map(|root| {
                    let anchor = root.project_path.as_deref().unwrap_or(home);
                    approved_path(path, &root.path, anchor)
                })
                .or_else(|| approved_path(path, &app_data.join("backups"), app_data))
                .or_else(|| {
                    approved_path(
                        path,
                        &skill_studio_core::scope::fork_storage_root(app_data),
                        app_data,
                    )
                })
                .ok_or_else(|| {
                    format!(
                        "Undo refused: {} is outside the backup and skill roots",
                        path.display()
                    )
                })?;
            let entry = UndoPath::bind(&physical)
                .map_err(|error| format!("Undo refused: {}: {error}", path.display()))?;
            bound.insert(path.to_path_buf(), entry);
        }
        Ok(Self { paths: bound })
    }

    pub(super) fn get(&self, path: &Path) -> Result<&UndoPath, String> {
        self.paths
            .get(path)
            .ok_or_else(|| format!("Undo refused: {} has no bound directory", path.display()))
    }
}

// Resolve only the configured scope anchor, never the skill or backup root.
// Recorded paths may use either spelling (for instance /var or /private/var).
fn approved_path(path: &Path, root: &Path, anchor: &Path) -> Option<PathBuf> {
    let physical_root = root
        .strip_prefix(anchor)
        .ok()
        .and_then(|relative| anchor.canonicalize().ok().map(|base| base.join(relative)));
    if path.starts_with(root) {
        Some(
            physical_root
                .as_deref()
                .unwrap_or(root)
                .join(path.strip_prefix(root).ok()?),
        )
    } else if path.starts_with(physical_root.as_ref()?) {
        Some(path.to_path_buf())
    } else {
        None
    }
}

/// An entry relative to a held directory. Missing parents are created only
/// through that handle, with every new component opened without following links.
pub(super) struct UndoPath {
    directory: ConfinedDir,
    missing: PathBuf,
    name: OsString,
}

impl UndoPath {
    pub(super) fn bind(path: &Path) -> io::Result<Self> {
        let name = path
            .file_name()
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "path has no file name"))?
            .to_owned();
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "path has no parent"))?;
        let mut ancestor = parent;
        let directory = loop {
            match open_confined_dir(ancestor) {
                Ok(directory) => break directory,
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    ancestor = ancestor.parent().ok_or(error)?;
                }
                Err(error) => return Err(error),
            }
        };
        let missing = parent
            .strip_prefix(ancestor)
            .map_err(io::Error::other)?
            .to_path_buf();
        if missing.iter().any(|part| part == "..") {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "expected a plain absolute path",
            ));
        }
        Ok(Self {
            directory,
            missing,
            name,
        })
    }

    pub(super) fn parent(&self, create: bool) -> io::Result<ConfinedDir> {
        let mut directory = self.directory.try_clone()?;
        for name in &self.missing {
            directory = match open_confined_child(&directory, name) {
                Err(error) if create && error.kind() == ErrorKind::NotFound => {
                    match directory.create_dir(name) {
                        Err(error) if error.kind() != ErrorKind::AlreadyExists => {
                            return Err(error)
                        }
                        _ => {}
                    }
                    open_confined_child(&directory, name)?
                }
                result => result?,
            };
        }
        Ok(directory)
    }

    pub(super) fn backup_to(&self, dir: &ConfinedDir, name: &str) -> io::Result<()> {
        copy_confined_entry(&self.parent(false)?, &self.name, dir, name.as_ref())
    }

    pub(super) fn restore_from(&self, source: &ConfinedDir, name: Option<&str>) -> io::Result<()> {
        if let Some(name) = name {
            source.symlink_metadata(name)?;
            let parent = self.parent(true)?;
            remove(&parent, &self.name, false)?;
            copy_confined_entry(source, name.as_ref(), &parent, &self.name)
        } else {
            self.remove(false)
        }
    }

    pub(super) fn move_to(&self, to: &Self) -> io::Result<()> {
        let source = self.parent(false)?;
        source.symlink_metadata(&self.name)?;
        let destination = to.parent(true)?;
        remove(&destination, &to.name, false)?;
        source.rename(&self.name, &destination, &to.name)
    }

    pub(super) fn remove(&self, symlinks_only: bool) -> io::Result<()> {
        match self.parent(false) {
            Ok(parent) => remove(&parent, &self.name, symlinks_only),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    pub(super) fn symlink(&self, target: &Path, replace: bool) -> io::Result<()> {
        let parent = self.parent(true)?;
        if !replace {
            return parent.symlink_contents(target, &self.name);
        }
        let staged = format!(".skill-studio-restore-{}", allocate_id());
        parent.symlink_contents(target, &staged)?;
        let result = remove(&parent, &self.name, false)
            .and_then(|()| parent.rename(&staged, &parent, &self.name));
        if result.is_err() {
            let _ = parent.remove_file(&staged);
        }
        result
    }
}

fn remove(directory: &ConfinedDir, name: &OsStr, symlinks_only: bool) -> io::Result<()> {
    match directory.symlink_metadata(name) {
        Ok(meta) if meta.is_symlink() => directory.remove_file(name),
        Ok(_) if symlinks_only => Ok(()),
        Ok(meta) if meta.is_dir() => directory.remove_dir_all(name),
        Ok(_) => directory.remove_file(name),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
