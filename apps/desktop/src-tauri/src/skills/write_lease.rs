// ============================================================================
// Skill Studio - write_lease
// Replaces the desktop's old process-wide mutation mutex (fork, park,
// unpark, harness-disable, add, and frontmatter-repair writes all used to
// serialize on it). `WriteLease` is a `FileLease` keyed to the root
// the write touches (`home` for every one of today's call sites), so a
// concurrent CLI or MCP write on the same root serializes with the desktop
// too - a global in-process mutex never could.
// ============================================================================

use std::path::Path;
use std::time::Duration;

use skill_studio_core::ports::{ExclusiveGuard, LeaseKey, LeaseMode, LeaseProvider};
use skill_studio_host::FileLease;

use super::core_runtime;

thread_local! {
    /// Leases over the home the current thread holds, write or scan. The
    /// guards are `!Send` (they own a `Box<dyn LeaseHandle>`), so each drops
    /// on the thread that took it and this count stays exact.
    static HELD_ON_THIS_THREAD: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Whether the calling thread holds a lease over the home, write or scan. A
/// scan on such a thread must not lease again: advisory locks do not nest
/// within one process, so its shared lease would wait out the thread's own.
pub fn current_thread_holds_home_lease() -> bool {
    HELD_ON_THIS_THREAD.with(|held| held.get() > 0)
}

/// How long a rebuild waits for a write to finish before it gives up. Matches
/// the core scan's own read budget.
const SCAN_READ_WAIT: Duration = Duration::from_mins(1);

/// Holds a shared lease for a rebuild until dropped. The handle is `None`
/// when the thread already held a lease over the home, which covers the read.
///
/// Lock order: a rebuild takes this before `rebuild_lock`, the same order a
/// write command follows (its exclusive lease first, then the rebuild). The
/// other order lets a rebuild hold `rebuild_lock` while it waits for a lease
/// that a write holds, while that write waits for `rebuild_lock`.
pub struct ScanLeaseGuard {
    handle: Option<Box<dyn skill_studio_core::ports::LeaseHandle>>,
    _folder_guard: skill_studio_host::DataFolderGuard,
}

impl Drop for ScanLeaseGuard {
    fn drop(&mut self) {
        if self.handle.is_some() {
            HELD_ON_THIS_THREAD.with(|held| held.set(held.get().saturating_sub(1)));
        }
    }
}

/// Longest a write waits for a background scan's shared lease to end. A scan
/// holds it only while it reads, well under a second; this bound lets one
/// finish instead of failing the write with "in progress".
const SCAN_WAIT: Duration = Duration::from_secs(3);

/// Holds the write lease until dropped, releasing it.
pub struct WriteLeaseGuard(ExclusiveGuard);

impl Drop for WriteLeaseGuard {
    fn drop(&mut self) {
        HELD_ON_THIS_THREAD.with(|held| held.set(held.get().saturating_sub(1)));
    }
}

impl WriteLeaseGuard {
    /// The proof-of-lease token for a nested write to the same root - e.g.
    /// `write_fork_registry_locked`, which would otherwise need to take a
    /// second, conflicting lease. Advisory locks don't nest within one
    /// process, so anything writing the registry while this guard is held
    /// must go through it instead of acquiring its own lease.
    pub fn as_exclusive_guard(&self) -> &ExclusiveGuard {
        &self.0
    }
}

/// One lease per root, backed by a `FileLease` rooted at the same
/// `<data_root>/leases` directory `build_runtime_write` uses for park and
/// unpark.
pub struct WriteLease {
    lease: FileLease,
    /// How long `begin_mutation` waits for a holder to release.
    wait: Duration,
    /// How long `acquire_scan_lease` waits for a writer to release.
    scan_wait: Duration,
}

impl Default for WriteLease {
    fn default() -> Self {
        WriteLease {
            lease: FileLease::new(super::skill_fork_registry::registry_lease_root()),
            wait: SCAN_WAIT,
            scan_wait: SCAN_READ_WAIT,
        }
    }
}

impl WriteLease {
    /// Builds a lease provider rooted at `lease_root` instead of the real
    /// machine's data directory, so a test can hold a lease without
    /// touching it.
    pub fn with_lease_root(lease_root: std::path::PathBuf) -> Self {
        WriteLease {
            lease: FileLease::new(lease_root),
            wait: Duration::ZERO,
            scan_wait: Duration::ZERO,
        }
    }

    /// A lease rooted at `<data_root>/leases`, the directory the core writers
    /// for that data root take, so a desktop mutation and a CLI or MCP write
    /// over the same data root exclude each other. The default provider
    /// resolves to the machine's own data directory, which is not the data
    /// root a caller passed - a fixture run or a test passes its own.
    pub fn for_data_root(data_root: &Path) -> Self {
        WriteLease {
            lease: FileLease::new(data_root.join("leases")),
            wait: SCAN_WAIT,
            scan_wait: SCAN_READ_WAIT,
        }
    }

    /// [`Self::with_lease_root`], waiting up to `wait` for a holder to release
    /// like the real provider does.
    #[cfg(test)]
    pub fn with_lease_root_and_wait(lease_root: std::path::PathBuf, wait: Duration) -> Self {
        WriteLease {
            lease: FileLease::new(lease_root),
            wait,
            scan_wait: wait,
        }
    }

    /// Acquires a shared lease over `home` and `projects` for a rebuild, so a
    /// write and the scan exclude each other while scans do not block each
    /// other. Takes no lease when this thread already holds one over the home.
    pub fn acquire_scan_lease(
        &self,
        home: &Path,
        projects: &[std::path::PathBuf],
    ) -> Result<ScanLeaseGuard, String> {
        let history = if std::env::var_os("SKILL_STUDIO_FIXTURE").is_some() {
            home.join(".history")
        } else {
            core_runtime::data_root().join("history")
        };
        let guard = skill_studio_host::guard_read_data_folder(&history)?;
        if current_thread_holds_home_lease() {
            return Ok(ScanLeaseGuard {
                handle: None,
                _folder_guard: guard,
            });
        }
        let mut keys: Vec<LeaseKey> = std::iter::once(home)
            .chain(projects.iter().map(std::path::PathBuf::as_path))
            .map(|root| LeaseKey {
                canonical_root: root.canonicalize().unwrap_or_else(|_| root.to_path_buf()),
            })
            .collect();
        keys.sort();
        keys.dedup();
        #[cfg(test)]
        scan_wait_probe::notify(&home.canonicalize().unwrap_or_else(|_| home.to_path_buf()));
        let handle = self
            .lease
            .acquire(&keys, LeaseMode::Shared, self.scan_wait)
            .map_err(|e| e.message)?;
        HELD_ON_THIS_THREAD.with(|held| held.set(held.get() + 1));
        Ok(ScanLeaseGuard {
            handle: Some(handle),
            _folder_guard: guard,
        })
    }

    /// Waits for every trusted scope root before startup recovery runs.
    pub(crate) fn acquire_for_startup(&self, keys: &[LeaseKey]) -> Result<WriteLeaseGuard, String> {
        self.lease
            .acquire(keys, LeaseMode::Exclusive, Duration::MAX)
            .map(|handle| {
                HELD_ON_THIS_THREAD.with(|held| held.set(held.get() + 1));
                WriteLeaseGuard(ExclusiveGuard::from_handle(handle))
            })
            .map_err(|error| error.to_string())
    }

    /// The desktop's one mutation entry point: takes the exclusive write
    /// lease on `root` and returns the guard the mutation runs under. Call it
    /// before the first read of the state a mutation changes and hold the
    /// guard until its last write and its history row are finished, so every
    /// step between the check and the write stays under one lease. Anything
    /// that writes a skill path takes its proof from here instead of
    /// acquiring a lease of its own.
    ///
    /// Waits a few seconds at most for a background scan to finish. `Err`
    /// mirrors the old mutation mutex's message shape when another writer
    /// already holds the lease, naming its pid and how long it has held it
    /// when the lease reports one; any other failure (e.g. the lease
    /// directory isn't writable) passes the underlying error text through
    /// instead of flattening it to the same "in progress" message, which
    /// would send a caller looking for a concurrent writer that doesn't
    /// exist.
    pub fn begin_mutation(&self, root: &Path) -> Result<WriteLeaseGuard, String> {
        let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let key = LeaseKey { canonical_root };
        self.lease
            .acquire(&[key], LeaseMode::Exclusive, self.wait)
            .map(|handle| {
                HELD_ON_THIS_THREAD.with(|held| held.set(held.get() + 1));
                WriteLeaseGuard(ExclusiveGuard::from_handle(handle))
            })
            .map_err(|e| match e.busy {
                Some(busy) => format!(
                    "Another write is in progress (pid {}, held for {:?})",
                    busy.pid, busy.age
                ),
                None => e.message,
            })
    }
}

/// Whether another holder, such as a core writer, could not take the
/// exclusive lease over `root` right now.
#[cfg(test)]
pub(crate) fn exclusive_lease_is_held(lease_root: &Path, root: &Path) -> bool {
    FileLease::new(lease_root.to_path_buf())
        .acquire(
            &[LeaseKey {
                canonical_root: root.canonicalize().unwrap_or_else(|_| root.to_path_buf()),
            }],
            LeaseMode::Exclusive,
            Duration::ZERO,
        )
        .is_err()
}

/// Lets a test learn that a rebuild thread has reached the point just before
/// it waits for the scan lease, which is the moment its lock order shows.
#[cfg(test)]
pub(crate) mod scan_wait_probe {
    use std::path::{Path, PathBuf};
    use std::sync::mpsc::Sender;
    use std::sync::Mutex;

    static PROBE: Mutex<Option<(PathBuf, Sender<()>)>> = Mutex::new(None);

    /// Sends one message when a rebuild over `home` is about to wait for its
    /// lease. Rebuilds over other homes, in parallel tests, are ignored.
    pub(crate) fn watch(home: PathBuf, sender: Sender<()>) {
        *PROBE.lock().unwrap() = Some((home, sender));
    }

    pub(super) fn notify(home: &Path) {
        if let Some((watched, sender)) = PROBE.lock().unwrap().as_ref() {
            if watched == home {
                let _ = sender.send(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_recovery_holds_home_and_project_leases_or_reverses_a_project_under_only_home() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path().join("home");
        let project = directory.path().join("project");
        std::fs::create_dir(&home).unwrap();
        std::fs::create_dir(&project).unwrap();
        let keys = vec![
            LeaseKey {
                canonical_root: home.canonicalize().unwrap(),
            },
            LeaseKey {
                canonical_root: project.canonicalize().unwrap(),
            },
        ];
        let root = directory.path().join("leases");
        let lease = WriteLease::with_lease_root(root.clone());
        let guard = lease.acquire_for_startup(&keys).unwrap();
        let competitor = FileLease::new(root);
        for key in &keys {
            let error = competitor
                .acquire(
                    std::slice::from_ref(key),
                    LeaseMode::Exclusive,
                    Duration::ZERO,
                )
                .err()
                .expect("startup must hold this root");
            assert_eq!(error.code, skill_studio_core::error::ErrorCode::ScopeBusy);
        }
        drop(guard);
        for key in keys {
            competitor
                .acquire(&[key], LeaseMode::Exclusive, Duration::ZERO)
                .unwrap();
        }
    }

    #[test]
    fn a_second_acquire_on_the_same_root_is_refused_until_the_first_drops_and_another_root_is_never_blocked(
    ) {
        let dir = tempfile::tempdir().unwrap();
        let lease_root = dir.path().join("leases");
        let root = dir.path().join("root");
        let other = dir.path().join("other");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&other).unwrap();

        let lease = WriteLease::with_lease_root(lease_root);
        let first = lease.begin_mutation(&root).unwrap();
        let second = lease.begin_mutation(&root);
        assert!(second.err().unwrap().contains("Another write"));
        // The adapter picks the key: two roots must not share one.
        assert!(lease.begin_mutation(&other).is_ok());
        drop(first);
        assert!(lease.begin_mutation(&root).is_ok());
    }

    /// A command holds `WriteLease` on `home` for its whole run (fork, add,
    /// pack, harness) and, inside that, writes the fork registry.
    /// Advisory locks don't nest within one process, so a registry write
    /// that takes its own second exclusive lease over the same root reports
    /// the caller's own lease as busy instead of writing - see
    /// `write_fork_registry_locked`.
    #[test]
    fn a_command_holding_the_root_lease_can_write_the_fork_registry_or_names_the_self_deadlock() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let _lease_root = super::super::skill_fork_registry::registry_lease_root_for_test(
            home.join(".test-leases"),
        );
        std::fs::create_dir_all(&home).unwrap();

        let write_lease = WriteLease::default();
        let guard = write_lease.begin_mutation(&home).unwrap();

        let mut registry = super::super::skill_fork_registry::ForkRegistry::default();
        let result = super::super::skill_fork_registry::write_fork_registry_locked(
            &guard,
            &home,
            &mut registry,
        );
        assert!(
            result.is_ok(),
            "writing the fork registry while the caller holds the root's write lease must \
             not need a second lease on the same root: {result:?}"
        );
    }
}
