use skill_studio_core::ports::{DirEntryFacts, ExclusiveGuard, FileFacts, ScopeFs, ScopedPath};
use skill_studio_host::RealFs;
use std::{
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
pub struct EditOnRead {
    pub path: PathBuf,
    pub backups: PathBuf,
    pub bytes: Vec<u8>,
    pub reads: AtomicUsize,
    pub after_v2: Option<PathBuf>,
    pub crash_after: Option<&'static str>,
}
impl ScopeFs for EditOnRead {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        RealFs::new().canonicalize(path)
    }
    fn symlink_metadata(&self, path: &Path) -> io::Result<FileFacts> {
        RealFs::new().symlink_metadata(path)
    }
    fn read_link(&self, path: &Path) -> io::Result<PathBuf> {
        RealFs::new().read_link(path)
    }
    fn ancestor_holds(&self, start: &Path, name: &str) -> io::Result<bool> {
        RealFs::new().ancestor_holds(start, name)
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntryFacts>> {
        RealFs::new().read_dir(path)
    }
    fn read_capped(&self, path: &Path, max_bytes: u64) -> io::Result<Vec<u8>> {
        RealFs::new().read_capped(path, max_bytes)
    }
    fn read_prefix(&self, path: &Path, limit: u64) -> io::Result<(Vec<u8>, bool)> {
        if self.after_v2.as_ref().is_some_and(|gate| {
            std::fs::read_to_string(gate).is_ok_and(|text| text.contains("Body at v2"))
        }) && self.reads.fetch_add(1, Ordering::SeqCst) == 0
        {
            std::fs::write(&self.path, &self.bytes)?;
        }
        if self.crash_after.is_none()
            && self.after_v2.is_none()
            && path == self.path
            && std::fs::read_dir(&self.backups)?.count() >= 2
            && self.reads.fetch_add(1, Ordering::SeqCst) == 0
        {
            std::fs::write(path, &self.bytes)?;
        }
        RealFs::new().read_prefix(path, limit)
    }
    fn write_atomic(
        &self,
        guard: &ExclusiveGuard,
        path: &ScopedPath,
        bytes: &[u8],
    ) -> io::Result<()> {
        RealFs::new().write_atomic(guard, path, bytes)?;
        if self.crash_after == Some("lock") && bytes.windows(7).any(|b| b == b"tree-v2") {
            std::process::exit(86);
        }
        Ok(())
    }
    fn rename(&self, guard: &ExclusiveGuard, from: &ScopedPath, to: &ScopedPath) -> io::Result<()> {
        RealFs::new().rename(guard, from, to)
    }
    fn remove_file(&self, guard: &ExclusiveGuard, path: &ScopedPath) -> io::Result<()> {
        RealFs::new().remove_file(guard, path)
    }
    fn create_dir_all(&self, guard: &ExclusiveGuard, path: &ScopedPath) -> io::Result<()> {
        RealFs::new().create_dir_all(guard, path)
    }
    fn symlink(
        &self,
        guard: &ExclusiveGuard,
        target: &ScopedPath,
        link: &ScopedPath,
    ) -> io::Result<()> {
        RealFs::new().symlink(guard, target, link)
    }
    fn symlink_relative(
        &self,
        guard: &ExclusiveGuard,
        target: &ScopedPath,
        relative_target: &Path,
        link: &ScopedPath,
    ) -> io::Result<()> {
        RealFs::new().symlink_relative(guard, target, relative_target, link)
    }
    fn fsops_device_inode(&self, path: &Path) -> io::Result<(u64, u64)> {
        RealFs::new().fsops_device_inode(path)
    }
    fn fsops_fsync_file(&self, path: &Path) -> io::Result<()> {
        RealFs::new().fsops_fsync_file(path)
    }
    fn fsops_fsync_dir(&self, path: &Path) -> io::Result<()> {
        RealFs::new().fsops_fsync_dir(path)?;
        if self.crash_after == Some("swap")
            && std::fs::read(&self.path).is_ok_and(|b| b == self.bytes)
        {
            std::process::exit(86);
        }
        Ok(())
    }
    fn fsops_create_dir(&self, path: &Path) -> io::Result<()> {
        RealFs::new().fsops_create_dir(path)
    }
    fn fsops_write_new_file(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        RealFs::new().fsops_write_new_file(path, bytes)
    }
    fn fsops_write_new_file_with_mode(
        &self,
        path: &Path,
        bytes: &[u8],
        mode: u32,
    ) -> io::Result<()> {
        RealFs::new().fsops_write_new_file_with_mode(path, bytes, mode)
    }
    fn fsops_rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        RealFs::new().fsops_rename(from, to)
    }
    fn fsops_symlink(&self, target: &Path, link: &Path) -> io::Result<()> {
        RealFs::new().fsops_symlink(target, link)
    }
    fn fsops_remove_dir(&self, path: &Path) -> io::Result<()> {
        RealFs::new().fsops_remove_dir(path)
    }
    fn fsops_remove_file(&self, path: &Path) -> io::Result<()> {
        RealFs::new().fsops_remove_file(path)
    }
    fn fsops_exchange(&self, a: &Path, b: &Path) -> io::Result<()> {
        RealFs::new().fsops_exchange(a, b)
    }
}
