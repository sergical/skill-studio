// ============================================================================
// Skills Module - coordinated SKILL.md writes
// Every app-managed SKILL.md read-modify-write runs under the shared write
// lease, the same `FileLease` core writes such as `ops_invocation` take, and
// uses this module's atomic file-replacement primitive.
// ============================================================================

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use super::write_lease::WriteLeaseGuard;

/// Counter appended to the atomic-write temp filename, on top of the pid and
/// a timestamp, so two saves in the same nanosecond still get distinct files.
static ATOMIC_WRITE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Capability for reading and replacing SKILL.md while the caller holds the
/// write lease. Core writers and other Skill Studio processes take the same
/// lease; external editors do not.
pub(crate) struct SkillMdWriteTransaction<'lease> {
    _lease: &'lease WriteLeaseGuard,
}

// Every method here takes `&self` as a capability token proving the caller
// holds the write lease, not because the body reads it.
#[allow(clippy::unused_self)]
impl SkillMdWriteTransaction<'_> {
    /// Reads text that will be checked or rewritten before this transaction
    /// replaces the same SKILL.md.
    pub(crate) fn read_to_string(&self, path: &Path) -> Result<String, String> {
        fs::read_to_string(path).map_err(|error| {
            format!(
                "Failed to open {} during SKILL.md write transaction: {error}",
                path.display()
            )
        })
    }

    /// Reads bytes that will be checked before this transaction replaces the
    /// same SKILL.md.
    pub(crate) fn read(&self, path: &Path) -> Result<Vec<u8>, String> {
        fs::read(path).map_err(|error| {
            format!(
                "Failed to read {} during SKILL.md write transaction: {error}",
                path.display()
            )
        })
    }

    /// Atomically replaces SKILL.md bytes under the lease `self` borrows.
    /// Callers can safely use it after a read or drift check on `self`.
    pub(crate) fn replace_bytes(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        atomic_replace_skill_md(path, bytes)
    }

    /// Atomically replaces SKILL.md text under the lease `self` borrows.
    /// Callers can safely use it after a read or drift check on `self`.
    pub(crate) fn replace_text(&self, path: &Path, content: &str) -> Result<(), String> {
        self.replace_bytes(path, content.as_bytes())
    }
}

/// Starts one app-managed SKILL.md write transaction under `lease`. Take the
/// lease at the outermost point of the command and keep the returned
/// capability alive across every read or drift check and its replacement.
pub(crate) fn begin_skill_md_write_transaction(
    lease: &WriteLeaseGuard,
) -> SkillMdWriteTransaction<'_> {
    SkillMdWriteTransaction { _lease: lease }
}

/// Atomically replaces SKILL.md with no lease, for tests of the replace
/// primitive itself and for fixtures that stand in for an external editor.
#[cfg(test)]
pub(crate) fn write_skill_md(path: &Path, content: &str) -> Result<(), String> {
    write_skill_md_bytes(path, content.as_bytes())
}

/// Byte form of [`write_skill_md`].
#[cfg(test)]
pub(crate) fn write_skill_md_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    atomic_replace_skill_md(path, bytes)
}

/// Compares and atomically replaces SKILL.md under `lease`.
pub(crate) fn write_skill_md_compare_and_swap(
    lease: &WriteLeaseGuard,
    path: &Path,
    expected_content: &str,
    content: &str,
) -> Result<(), String> {
    let transaction = begin_skill_md_write_transaction(lease);
    compare_and_replace_skill_md(&transaction, path, expected_content, content, || {})
}

fn compare_and_replace_skill_md(
    transaction: &SkillMdWriteTransaction,
    path: &Path,
    expected_content: &str,
    content: &str,
    before_replace: impl FnOnce(),
) -> Result<(), String> {
    let current = transaction.read_to_string(path)?;
    if current != expected_content {
        return Err(
            "SKILL.md changed on disk since it was loaded. Reload the file and run the audit again."
                .to_string(),
        );
    }
    before_replace();
    transaction.replace_text(path, content)
}

#[cfg(test)]
pub(crate) fn write_skill_md_compare_and_swap_with(
    lease: &WriteLeaseGuard,
    path: &Path,
    expected_content: &str,
    content: &str,
    before_replace: impl FnOnce(),
) -> Result<(), String> {
    let transaction = begin_skill_md_write_transaction(lease);
    compare_and_replace_skill_md(
        &transaction,
        path,
        expected_content,
        content,
        before_replace,
    )
}

fn atomic_replace_skill_md(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("Failed to resolve parent directory of {}", path.display()))?;
    let permissions = fs::metadata(path)
        .map_err(|error| format!("Failed to stat {}: {error}", path.display()))?
        .permissions();
    let counter = ATOMIC_WRITE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let temp = parent.join(format!(
        ".SKILL.md.tmp-{}-{counter}-{nanos}",
        std::process::id()
    ));

    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|error| format!("Failed to create {}: {error}", temp.display()))?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("Failed to write {}: {error}", temp.display()))?;
        fs::set_permissions(&temp, permissions)
            .map_err(|error| format!("Failed to preserve SKILL.md permissions: {error}"))?;
        fs::rename(&temp, path)
            .map_err(|error| format!("Failed to save {}: {error}", path.display()))?;
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| format!("Failed to sync skill directory: {error}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}
