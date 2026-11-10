//! Successful version probes, invalidated by executable identity.
use serde::{Deserialize, Serialize};
use skill_studio_core::ports::ProcessOutput;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const MAX_BYTES: u64 = 1024 * 1024;
const MAX_ENTRIES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ProbeKey {
    path: PathBuf,
    target: PathBuf,
    size: u64,
    device: u64,
    inode: u64,
    changed_secs: i64,
    changed_nanos: i64,
    effective_path: OsString,
    modified_secs: u64,
    modified_nanos: u32,
}
impl ProbeKey {
    pub(crate) fn is_current(&self) -> bool {
        Self::read_with_path(&self.path, self.effective_path.clone()).as_ref() == Some(self)
    }
    #[cfg(test)]
    pub(crate) fn read(path: &Path) -> Option<Self> {
        Self::read_with_path(path, OsString::new())
    }
    pub(crate) fn read_with_path(path: &Path, effective_path: OsString) -> Option<Self> {
        let path = std::path::absolute(path).ok()?;
        let target = std::fs::canonicalize(&path).ok()?;
        let facts = std::fs::metadata(&target).ok()?;
        if !facts.is_file() || !is_native_executable(&target)? {
            return None;
        }
        let modified = facts.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
        Some(Self {
            path,
            target,
            size: facts.len(),
            device: facts.dev(),
            inode: facts.ino(),
            changed_secs: facts.ctime(),
            changed_nanos: facts.ctime_nsec(),
            effective_path,
            modified_secs: modified.as_secs(),
            modified_nanos: modified.subsec_nanos(),
        })
    }
}
fn is_native_executable(path: &Path) -> Option<bool> {
    let mut magic = [0; 4];
    let mut file = std::fs::File::open(path).ok()?;
    if file.read_exact(&mut magic).is_err() {
        return Some(false);
    }
    Some(matches!(
        magic,
        [0x7f, b'E', b'L', b'F']
            | [0xfe, 0xed, 0xfa, 0xce | 0xcf]
            | [0xce | 0xcf, 0xfa, 0xed, 0xfe]
            | [0xca, 0xfe, 0xba, 0xbe | 0xbf]
            | [0xbe | 0xbf, 0xba, 0xfe, 0xca]
    ))
}

fn cache_path_confined(file: &Path) -> bool {
    let Some(parent) = file.parent() else {
        return false;
    };
    let Some(home) = parent.parent() else {
        return false;
    };
    let Ok(home) = home.canonicalize() else {
        return false;
    };
    [parent, file]
        .into_iter()
        .all(|path| match std::fs::symlink_metadata(path) {
            Ok(metadata) => {
                !metadata.file_type().is_symlink()
                    && path
                        .canonicalize()
                        .is_ok_and(|resolved| resolved.starts_with(&home))
            }
            Err(error) => error.kind() == std::io::ErrorKind::NotFound,
        })
}
#[derive(Serialize, Deserialize)]
struct Entry {
    key: ProbeKey,
    output: ProcessOutput,
}
#[derive(Serialize, Deserialize)]
struct Document {
    version: u32,
    entries: Vec<Entry>,
}
#[derive(Default)]
pub(crate) struct ProbeCache {
    file: Option<PathBuf>,
    entries: Vec<Entry>,
}
impl ProbeCache {
    pub(crate) fn load(file: PathBuf) -> Self {
        let mut bytes = Vec::new();
        let entries = cache_path_confined(&file)
            .then(|| std::fs::File::open(&file))
            .transpose()
            .ok()
            .flatten()
            .and_then(|input| {
                input.take(MAX_BYTES + 1).read_to_end(&mut bytes).ok()?;
                if bytes.len() as u64 > MAX_BYTES {
                    return None;
                }
                let document: Document = serde_json::from_slice(&bytes).ok()?;
                (document.version == 1 && document.entries.len() <= MAX_ENTRIES)
                    .then_some(document.entries)
            })
            .unwrap_or_default();
        Self {
            file: Some(file),
            entries,
        }
    }
    pub(crate) fn get(&self, key: &ProbeKey) -> Option<ProcessOutput> {
        self.entries
            .iter()
            .find(|entry| entry.key == *key && cacheable(&entry.output))
            .map(|entry| entry.output.clone())
    }
    pub(crate) fn put(&mut self, key: ProbeKey, output: &ProcessOutput) {
        if !cacheable(output) {
            return;
        }
        self.entries.retain(|entry| entry.key.path != key.path);
        self.entries.push(Entry {
            key,
            output: output.clone(),
        });
        if self.entries.len() > MAX_ENTRIES {
            self.entries.remove(0);
        }
        let Some(file) = &self.file else {
            return;
        };
        let Ok(bytes) = serde_json::to_vec(&Document {
            version: 1,
            entries: self
                .entries
                .iter()
                .map(|entry| Entry {
                    key: entry.key.clone(),
                    output: entry.output.clone(),
                })
                .collect(),
        }) else {
            return;
        };
        let _ = write_cache(file, &bytes);
    }
}
fn cacheable(output: &ProcessOutput) -> bool {
    !output.timed_out
        && output.status == Some(0)
        && output.stdout.len() + output.stderr.len() <= 8192
}
fn write_cache(file: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let parent = file
        .parent()
        .ok_or_else(|| std::io::Error::other("cache has no parent"))?;
    if !cache_path_confined(file) {
        return Ok(());
    }
    std::fs::create_dir_all(parent)?;
    if !cache_path_confined(file) {
        return Ok(());
    }
    let temp = parent.join(format!(".probe-cache-{}.tmp", ulid::Ulid::generate()));
    let result = (|| {
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)?;
        output.write_all(bytes)?;
        std::fs::rename(&temp, file)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn non_native_launchers_bypass_cache_while_symlinked_binaries_keep_keys_or_cache_obsolete_backends(
    ) {
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("program");
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&program, &link).unwrap();
        for header in [
            "#!/bin/sh",
            "#!/usr/bin/env -S bash",
            "#!/bin/zsh",
            "#!/bin/dash",
            "#!/usr/bin/python3",
            "#!/usr/bin/env node",
            "plain text",
        ] {
            std::fs::write(&program, format!("{header}\nexec backend --version\n")).unwrap();
            assert!(
                ProbeKey::read(&link).is_none(),
                "shell launcher received a cache key: {header}"
            );
        }
        std::fs::write(&program, b"\x7fELFnative").unwrap();
        assert!(
            ProbeKey::read(&link).is_some(),
            "symlinked program lost its cache key"
        );
    }

    #[test]
    fn caching_under_symlinked_home_entries_skips_writes_or_modifies_an_outside_file() {
        for symlink_directory in [true, false] {
            let home = tempfile::tempdir().unwrap();
            let outside = tempfile::tempdir().unwrap();
            let parent = home.path().join(".skill-studio");
            let file = parent.join("version-probes.json");
            let victim = outside.path().join("version-probes.json");
            std::fs::write(&victim, b"untouched").unwrap();
            if symlink_directory {
                std::os::unix::fs::symlink(outside.path(), &parent).unwrap();
            } else {
                std::fs::create_dir(&parent).unwrap();
                std::os::unix::fs::symlink(&victim, &file).unwrap();
            }
            let program = home.path().join("program");
            std::fs::write(&program, b"\x7fELFfixture").unwrap();
            ProbeCache::load(file.clone()).put(ProbeKey::read(&program).unwrap(), &output());
            assert_eq!(std::fs::read(&victim).unwrap(), b"untouched");
            assert!(
                std::fs::symlink_metadata(if symlink_directory { &parent } else { &file })
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
            assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 1);
        }
    }

    fn output() -> ProcessOutput {
        ProcessOutput {
            status: Some(0),
            stdout: "1.2.3".into(),
            stderr: String::new(),
            timed_out: false,
        }
    }
    #[test]
    fn replacing_the_same_path_with_equal_size_and_mtime_invalidates_the_probe_or_returns_stale_output(
    ) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("program");
        let replacement = dir.path().join("replacement");
        std::fs::write(&path, b"\x7fELFfirst").unwrap();
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        let before = ProbeKey::read(&path).unwrap();
        let mut cache = ProbeCache::default();
        cache.put(before.clone(), &output());
        std::fs::write(&replacement, b"\x7fELFother").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&replacement)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        std::fs::rename(&replacement, &path).unwrap();
        let after = ProbeKey::read(&path).unwrap();
        assert_eq!(before.size, after.size);
        assert_eq!(
            (before.modified_secs, before.modified_nanos),
            (after.modified_secs, after.modified_nanos)
        );
        assert!(
            cache.get(&after).is_none(),
            "same-path replacement reused stale output"
        );
    }

    #[test]
    fn retargeting_a_shim_with_equal_size_and_mtime_invalidates_the_probe_or_returns_the_old_binary_version(
    ) {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first");
        let second = dir.path().join("second");
        let shim = dir.path().join("shim");
        std::fs::write(&first, b"\x7fELFfirst").unwrap();
        std::fs::write(&second, b"\x7fELFother").unwrap();
        let modified = std::fs::metadata(&first).unwrap().modified().unwrap();
        std::fs::File::options()
            .write(true)
            .open(&second)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        std::os::unix::fs::symlink(&first, &shim).unwrap();
        let before = ProbeKey::read(&shim).unwrap();
        let mut cache = ProbeCache::default();
        cache.put(before.clone(), &output());
        std::fs::remove_file(&shim).unwrap();
        std::os::unix::fs::symlink(&second, &shim).unwrap();
        let after = ProbeKey::read(&shim).unwrap();
        assert_eq!(before.size, after.size);
        assert_eq!(
            (before.modified_secs, before.modified_nanos),
            (after.modified_secs, after.modified_nanos)
        );
        assert!(
            cache.get(&after).is_none(),
            "retargeted shim reused the old executable's output"
        );
    }

    #[test]
    fn version_cache_reopens_only_for_the_same_path_size_and_mtime_or_returns_stale_output() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("program");
        let alias = dir.path().join("alias");
        std::fs::write(&path, b"\x7fELFone").unwrap();
        let key = ProbeKey::read(&path).unwrap();
        let file = dir.path().join(".skill-studio/cache.json");
        let mut cache = ProbeCache::load(file.clone());
        cache.put(key.clone(), &output());
        let reopened = ProbeCache::load(file);
        assert_eq!(reopened.get(&key), Some(output()));
        std::fs::hard_link(&path, &alias).unwrap();
        assert!(
            reopened.get(&ProbeKey::read(&alias).unwrap()).is_none(),
            "different invocation paths must retain argv[0] semantics"
        );
        let times = std::fs::FileTimes::new().set_modified(
            UNIX_EPOCH + std::time::Duration::from_secs(key.modified_secs.saturating_sub(1)),
        );
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(times)
            .unwrap();
        let changed_time = ProbeKey::read(&path).unwrap();
        assert_eq!(changed_time.size, key.size);
        assert!(
            reopened.get(&changed_time).is_none(),
            "mtime change reused stale output"
        );
        std::fs::write(&path, b"\x7fELFlarger").unwrap();
        assert!(
            reopened.get(&ProbeKey::read(&path).unwrap()).is_none(),
            "size change reused stale output"
        );
    }
    #[test]
    fn failed_and_timed_out_probes_are_not_cached_or_turn_a_transient_failure_into_a_launch_result()
    {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("program");
        std::fs::write(&path, b"\x7fELFfixture").unwrap();
        let key = ProbeKey::read(&path).unwrap();
        for failed in [
            ProcessOutput {
                status: Some(1),
                ..output()
            },
            ProcessOutput {
                timed_out: true,
                ..output()
            },
        ] {
            let mut cache = ProbeCache::default();
            cache.put(key.clone(), &failed);
            assert!(cache.get(&key).is_none());
        }
    }
}
