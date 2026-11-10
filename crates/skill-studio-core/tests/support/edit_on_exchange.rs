use skill_studio_core::ports::{DirEntryFacts, ExclusiveGuard, FileFacts, ScopeFs, ScopedPath};
use skill_studio_host::RealFs;
use std::{
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
pub struct EditOnExchange {
    pub path: PathBuf,
    pub trigger: PathBuf,
    pub bytes: Vec<u8>,
    pub reads: AtomicUsize,
    pub writer: Option<std::sync::Mutex<std::fs::File>>,
    pub crash_after_capture: bool,
    pub writer_on_publication: bool,
    pub crash_after_publication: bool,
}
impl EditOnExchange {
    fn write_descriptor(&self) -> io::Result<()> {
        if let Some(writer) = &self.writer {
            use std::io::{Seek, Write};
            let mut writer = writer.lock().unwrap();
            writer.rewind()?;
            writer.set_len(0)?;
            writer.write_all(&self.bytes)?;
            writer.sync_all()?;
        }
        Ok(())
    }
}
impl ScopeFs for EditOnExchange {
    fn fsops_hard_link(&self, from: &Path, to: &Path) -> io::Result<()> {
        if self.writer_on_publication && from.extension().is_some_and(|ext| ext == "restored") {
            self.write_descriptor()?;
        }
        RealFs::new().fsops_hard_link(from, to)?;
        assert!(
            !(self.crash_after_publication
                && from.extension().is_some_and(|ext| ext == "restored")),
            "interrupted after lock publication"
        );
        Ok(())
    }

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
        RealFs::new().read_prefix(path, limit)
    }
    fn write_atomic(
        &self,
        guard: &ExclusiveGuard,
        path: &ScopedPath,
        bytes: &[u8],
    ) -> io::Result<()> {
        RealFs::new().write_atomic(guard, path, bytes)
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
        if !self.writer_on_publication && path.extension().is_some_and(|ext| ext == "restored") {
            self.write_descriptor()?;
        }
        RealFs::new().fsops_fsync_file(path)
    }
    fn fsops_fsync_dir(&self, path: &Path) -> io::Result<()> {
        RealFs::new().fsops_fsync_dir(path)
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
        RealFs::new().fsops_rename(from, to)?;
        assert!(
            !(from == self.path && self.crash_after_capture),
            "interrupted after lock capture"
        );
        if from == self.path
            && self.trigger == self.path
            && self.reads.fetch_add(1, Ordering::SeqCst) == 0
        {
            std::fs::write(&self.path, &self.bytes)?;
        }
        Ok(())
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
        if (a == self.trigger || b == self.trigger)
            && self.reads.fetch_add(1, Ordering::SeqCst) == 0
        {
            std::fs::write(&self.path, &self.bytes)?;
        }
        RealFs::new().fsops_exchange(a, b)
    }
}
