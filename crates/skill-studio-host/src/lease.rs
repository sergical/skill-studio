//! [`LeaseProvider`] over advisory file locks.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use skill_studio_core::error::{CoreError, ErrorCode, LeaseBusy};
use skill_studio_core::identity::sha256_hex;
use skill_studio_core::ports::{LeaseHandle, LeaseKey, LeaseMode, LeaseProvider};

/// How long to sleep between two lock attempts while waiting for a lease.
const RETRY_INTERVAL: Duration = Duration::from_millis(20);

/// Upper bound on how long a takeover of a lease whose recorded holder is
/// gone may take. The OS releases the advisory lock as part of tearing down
/// the dead process's file descriptors; this window only bridges the small
/// gap between that teardown and our next lock attempt, independent of the
/// caller's own `wait` budget (which may be zero).
const STALE_TAKEOVER_TIMEOUT: Duration = Duration::from_millis(500);
const STALE_RETRY_INTERVAL: Duration = Duration::from_millis(5);

/// `LeaseProvider` backed by one persistent lock file per canonical root.
/// Older clients can hold open descriptors, so these files must never be removed.
pub struct FileLease {
    lease_root: PathBuf,
}

impl FileLease {
    /// Builds a provider whose lock files live under `lease_root`.
    ///
    /// `lease_root` never sits inside a scope home or project; the adapter
    /// wiring is responsible for choosing an app-data directory.
    pub fn new(lease_root: PathBuf) -> Self {
        FileLease { lease_root }
    }

    fn lock_path(&self, key: &LeaseKey) -> PathBuf {
        let hex = sha256_hex(key.canonical_root.to_string_lossy().as_bytes());
        self.lease_root.join(format!("{hex}.lock"))
    }
}

/// Keys and open, locked files held for the lifetime of the handle.
///
/// Invariant: dropping the handle drops every `File`, which releases each
/// advisory lock; no explicit unlock call is needed.
struct FileLeaseHandle {
    keys: Vec<LeaseKey>,
    mode: LeaseMode,
    /// Kept only so the advisory locks release when this handle drops.
    _files: Vec<File>,
}

impl LeaseHandle for FileLeaseHandle {
    fn keys(&self) -> &[LeaseKey] {
        &self.keys
    }

    fn mode(&self) -> LeaseMode {
        self.mode
    }
}

fn try_lock(file: &File, mode: LeaseMode) -> Result<bool, io::Error> {
    let result = match mode {
        LeaseMode::Shared => file.try_lock_shared(),
        LeaseMode::Exclusive => file.try_lock(),
    };
    match result {
        Ok(()) => Ok(true),
        Err(fs::TryLockError::WouldBlock) => Ok(false),
        Err(fs::TryLockError::Error(e)) => Err(e),
    }
}

/// Records this process as the holder: pid and the wall-clock time of
/// acquisition, so a later `Busy` error can name both. Written only for
/// `LeaseMode::Exclusive`; a shared lease has no single holder to name.
///
/// Best effort: a write failure here only means a later reader sees no
/// holder info, never a wrong one, so the caller ignores its result.
fn write_holder(file: &File) -> io::Result<()> {
    let pid = std::process::id();
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let mut file = file;
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    write!(file, "{pid}|{now_ms}")?;
    file.sync_all()
}

/// Reads the holder an earlier `write_holder` recorded, if any and if
/// parseable. Returns the pid and how long ago it acquired the lease.
fn read_holder(path: &Path) -> Option<(u32, Duration)> {
    let content = fs::read_to_string(path).ok()?;
    let (pid_str, ts_str) = content.trim().split_once('|')?;
    let pid: u32 = pid_str.parse().ok()?;
    let recorded_ms: u128 = ts_str.parse().ok()?;
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis();
    let age_ms = now_ms.saturating_sub(recorded_ms);
    Some((
        pid,
        Duration::from_millis(u64::try_from(age_ms).unwrap_or(u64::MAX)),
    ))
}

/// Whether `pid` still names a live process. `sysinfo`, not `libc`'s
/// `kill(pid, 0)`, because this crate forbids unsafe code; it refreshes only
/// the one process, so the check stays cheap.
fn pid_alive(pid: u32) -> Result<bool, sysinfo::Error> {
    let mut system = sysinfo::System::new()?;
    system.refresh_processes(
        sysinfo::ProcessesToUpdate::Some(&[sysinfo::Pid::from_u32(pid)]),
        true,
    );
    Ok(system.process(sysinfo::Pid::from_u32(pid)).is_some())
}

impl LeaseProvider for FileLease {
    fn acquire(
        &self,
        keys: &[LeaseKey],
        mode: LeaseMode,
        wait: Duration,
    ) -> Result<Box<dyn LeaseHandle>, CoreError> {
        let mut sorted_keys = keys.to_vec();
        sorted_keys.sort();

        fs::create_dir_all(&self.lease_root).map_err(|e| CoreError::io(&self.lease_root, e))?;

        let deadline = Instant::now().checked_add(wait);
        let mut files = Vec::with_capacity(sorted_keys.len());
        for key in &sorted_keys {
            let path = self.lock_path(key);
            let file = OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(&path)
                .map_err(|e| CoreError::io(&path, e))?;
            let stale_deadline = Instant::now() + STALE_TAKEOVER_TIMEOUT;
            let mut attempted = false;
            loop {
                match try_lock(&file, mode) {
                    Ok(true) => {
                        // No exclusive holder can coexist with a shared one,
                        // so any record left by an earlier writer is stale.
                        // Left in place, it would send a later exclusive
                        // waiter into the dead-holder takeover past its wait.
                        let _ = match mode {
                            LeaseMode::Exclusive => write_holder(&file),
                            LeaseMode::Shared => file.set_len(0),
                        };
                        break;
                    }
                    Ok(false) => {
                        let holder = read_holder(&path);
                        let holder_alive =
                            holder.is_none_or(|(pid, _)| pid_alive(pid).unwrap_or(true));
                        if !holder_alive && Instant::now() < stale_deadline {
                            // The recorded holder is dead; the OS releases
                            // its advisory lock as part of exiting, usually
                            // before we even observe `WouldBlock`. Bridge
                            // the rare remaining gap instead of reporting a
                            // holder that is already gone.
                            std::thread::sleep(STALE_RETRY_INTERVAL);
                            continue;
                        }
                        // A zero (or already-elapsed) `wait` collapses
                        // `deadline` to "now", which would otherwise turn a
                        // single spurious `WouldBlock` - the OS can report
                        // one for a moment right after another fd on this
                        // process closes and releases the same lock under
                        // heavy concurrent load - into a false "busy". Always
                        // re-check once before trusting the first read.
                        if deadline.is_some_and(|deadline| Instant::now() >= deadline) && attempted
                        {
                            let mut err = CoreError::new(
                                ErrorCode::ScopeBusy,
                                format!(
                                    "another process holds the lease on {}",
                                    key.canonical_root.display()
                                ),
                            )
                            .at(&path);
                            if let Some((pid, age)) = holder {
                                err = err.with_busy(LeaseBusy { pid, age });
                            }
                            return Err(err);
                        }
                        attempted = true;
                        std::thread::sleep(RETRY_INTERVAL);
                    }
                    Err(e) => return Err(CoreError::io(&path, e)),
                }
            }
            files.push(file);
        }

        Ok(Box::new(FileLeaseHandle {
            keys: sorted_keys,
            mode,
            _files: files,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEGACY_DIRECTORY_LOCK: &str = ".lease-directory";

    fn key(name: &str) -> LeaseKey {
        LeaseKey {
            canonical_root: PathBuf::from(format!("/tmp/skill-studio-host-test/{name}")),
        }
    }

    #[test]
    fn two_shared_leases_over_the_same_key_both_succeed() {
        let dir = tempfile::tempdir().unwrap();
        let lease = FileLease::new(dir.path().to_path_buf());
        let keys = [key("root-a")];
        let first = lease
            .acquire(&keys, LeaseMode::Shared, Duration::from_millis(100))
            .unwrap();
        let second = lease
            .acquire(&keys, LeaseMode::Shared, Duration::from_millis(100))
            .unwrap();
        assert_eq!(first.mode(), LeaseMode::Shared);
        assert_eq!(second.mode(), LeaseMode::Shared);
    }

    #[test]
    fn a_shared_lease_blocks_an_exclusive_lease_until_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let lease_root = dir.path().to_path_buf();
        let keys = vec![key("root-b")];

        let lease = FileLease::new(lease_root.clone());
        let shared = lease
            .acquire(&keys, LeaseMode::Shared, Duration::from_millis(100))
            .unwrap();

        let keys_for_writer = keys.clone();
        let writer = std::thread::spawn(move || {
            let writer_lease = FileLease::new(lease_root);
            writer_lease.acquire(
                &keys_for_writer,
                LeaseMode::Exclusive,
                Duration::from_millis(50),
            )
        });
        let busy = writer.join().unwrap();
        assert!(matches!(
            busy,
            Err(e) if e.code == ErrorCode::ScopeBusy
        ));

        drop(shared);

        let lease = FileLease::new(dir.path().to_path_buf());
        let exclusive = lease
            .acquire(&keys, LeaseMode::Exclusive, Duration::from_millis(500))
            .unwrap();
        assert_eq!(exclusive.mode(), LeaseMode::Exclusive);
    }

    #[test]
    fn exclusive_lease_releases_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let keys = vec![key("root-c")];
        let lease = FileLease::new(dir.path().to_path_buf());

        let first = lease
            .acquire(&keys, LeaseMode::Exclusive, Duration::from_millis(100))
            .unwrap();
        drop(first);

        let second = lease
            .acquire(&keys, LeaseMode::Exclusive, Duration::from_millis(100))
            .unwrap();
        assert_eq!(second.keys(), keys.as_slice());
    }

    /// Given the current process's own pid, when [`pid_alive`] checks it,
    /// then it reports alive; on failure the panic names the pid the check
    /// missed.
    #[test]
    fn pid_alive_reports_the_current_process_alive_or_names_the_pid_it_missed() {
        assert!(
            pid_alive(std::process::id()).unwrap(),
            "the current process must report alive, or this test proves nothing about \
             detecting a live pid"
        );
    }

    /// Given a child process that has already exited, when [`pid_alive`]
    /// checks its (now stale) pid, then it reports dead; on failure the
    /// panic names the exited pid still treated as holding the lease.
    #[test]
    fn pid_alive_reports_an_exited_process_dead_or_treats_it_as_still_holding_the_lease() {
        let mut child = std::process::Command::new("true")
            .spawn()
            .expect("spawn a short-lived child");
        let pid = child.id();
        child.wait().expect("wait for the child to exit");

        assert!(
            !pid_alive(pid).unwrap(),
            "an exited process's pid ({pid}) must report dead, or this test proves nothing \
             about detecting a stale lease holder"
        );
    }

    /// Writes a fake holder record naming `pid`, the same shape
    /// [`write_holder`] leaves but for a pid this test controls rather than
    /// the current process.
    fn write_fake_holder(file: &File, pid: u32) {
        let mut file = file;
        file.set_len(0).expect("truncate the lock file");
        file.seek(SeekFrom::Start(0)).expect("seek to the start");
        write!(file, "{pid}|0").expect("write the fake holder record");
        file.sync_all().expect("flush the fake holder record");
    }

    #[test]
    fn a_shared_lease_clears_a_dead_writers_record_or_an_exclusive_waiter_outlasts_its_wait() {
        let dir = tempfile::tempdir().unwrap();
        let lease = FileLease::new(dir.path().to_path_buf());
        let keys = vec![key("shared-over-dead-writer")];
        let path = lease.lock_path(&keys[0]);
        fs::create_dir_all(dir.path()).unwrap();
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        write_fake_holder(&file, u32::MAX);
        drop(file);

        let _reader = lease
            .acquire(&keys, LeaseMode::Shared, Duration::from_millis(100))
            .unwrap();

        assert_eq!(
            read_holder(&path),
            None,
            "a live shared reader left the dead writer's record, so an exclusive waiter retries \
             the stale takeover for {STALE_TAKEOVER_TIMEOUT:?} whatever its own wait"
        );
        let busy = lease
            .acquire(&keys, LeaseMode::Exclusive, Duration::ZERO)
            .err()
            .expect("the shared reader still holds the root");
        assert_eq!(busy.code, ErrorCode::ScopeBusy);
    }

    /// Given another `FileLease` holds an exclusive lease, when this
    /// process asks for the same key with a generous wait, then it succeeds
    /// once the holder drops - not right away, and not because the caller
    /// gave up too soon; on failure the panic names the error the wait
    /// budget produced instead of a lease.
    #[test]
    fn acquire_waits_out_a_short_lived_holder_within_its_own_deadline_or_gives_up_too_soon() {
        let dir = tempfile::tempdir().unwrap();
        let lease_root = dir.path().to_path_buf();
        let keys = vec![key("short-lived-holder")];

        let holder_lease = FileLease::new(lease_root.clone());
        let held = holder_lease
            .acquire(&keys, LeaseMode::Exclusive, Duration::from_millis(100))
            .expect("the first holder must acquire cleanly");
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(80));
            drop(held);
        });

        let waiting_lease = FileLease::new(lease_root);
        let result = waiting_lease.acquire(&keys, LeaseMode::Exclusive, Duration::from_secs(2));
        releaser.join().expect("join the releasing thread");

        assert!(
            result.is_ok(),
            "a caller with a two-second wait must still get the lease once an 80ms holder \
             drops, or this test proves nothing about the wait budget being honoured; got \
             {:?}",
            result.as_ref().err()
        );
    }

    /// Given a lease file the OS still (briefly) locks but whose recorded
    /// holder pid has already exited - the exact shape a crash leaves
    /// behind, since the OS releases the advisory lock as part of tearing
    /// the dead process down - when `acquire` runs with *no* wait budget at
    /// all, then it still takes the lease over rather than reporting it
    /// busy: the bridge past that OS teardown gap does not depend on the
    /// caller's own wait; on failure the panic names the busy error
    /// returned instead.
    #[test]
    fn acquire_takes_over_a_dead_holders_lease_even_with_no_wait_budget_or_reports_it_busy() {
        let dir = tempfile::tempdir().unwrap();
        let lease = FileLease::new(dir.path().to_path_buf());
        let keys = vec![key("dead-holder-no-wait")];
        fs::create_dir_all(dir.path()).unwrap();
        let path = lease.lock_path(&keys[0]);

        let mut child = std::process::Command::new("true")
            .spawn()
            .expect("spawn a short-lived child");
        let dead_pid = child.id();
        child.wait().expect("wait for the child to exit");

        // Hold the OS lock ourselves - not through `FileLease::acquire`, so
        // the holder record below keeps naming the dead pid rather than
        // this test process.
        let holder_file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .expect("open the lock file");
        assert!(
            try_lock(&holder_file, LeaseMode::Exclusive).expect("lock the file ourselves"),
            "the test must hold the OS lock itself before simulating a stale holder"
        );
        write_fake_holder(&holder_file, dead_pid);

        let releaser = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(80));
            drop(holder_file);
        });

        let result = lease.acquire(&keys, LeaseMode::Exclusive, Duration::from_millis(0));
        releaser.join().expect("join the releasing thread");

        assert!(
            result.is_ok(),
            "a dead holder's lease must be taken over even with a zero wait budget, or this \
             test proves nothing about the stale-takeover bridge running independently of the \
             caller's own wait; got {:?}",
            result.as_ref().err()
        );
    }

    /// Given the same dead-holder shape as the test above, but the OS lock
    /// stays held well past the stale-takeover bridge's own timeout, when
    /// `acquire` runs, then it reports the lease busy once that timeout
    /// elapses rather than bridging forever regardless of how long the OS
    /// takes to finish releasing; on failure the panic names the lease
    /// `acquire` returned instead of the busy error.
    #[test]
    fn acquire_gives_up_the_stale_takeover_bridge_after_its_own_timeout_or_bridges_forever() {
        let dir = tempfile::tempdir().unwrap();
        let lease = FileLease::new(dir.path().to_path_buf());
        let keys = vec![key("dead-holder-past-bridge-timeout")];
        fs::create_dir_all(dir.path()).unwrap();
        let path = lease.lock_path(&keys[0]);

        let mut child = std::process::Command::new("true")
            .spawn()
            .expect("spawn a short-lived child");
        let dead_pid = child.id();
        child.wait().expect("wait for the child to exit");

        let holder_file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .expect("open the lock file");
        assert!(
            try_lock(&holder_file, LeaseMode::Exclusive).expect("lock the file ourselves"),
            "the test must hold the OS lock itself before simulating a stale holder"
        );
        write_fake_holder(&holder_file, dead_pid);

        // This thread holds the lock until `acquire` returns, so only the
        // bridge's own `STALE_TAKEOVER_TIMEOUT` (500ms) can end the wait. The
        // 5s watchdog turns a bridge that waits forever into a failure
        // instead of a hang.
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = lease.acquire(&keys, LeaseMode::Exclusive, Duration::from_millis(100));
            let _ = result_tx.send(result.map(|_| ()));
        });
        let result = result_rx.recv_timeout(Duration::from_secs(5));
        drop(holder_file);
        worker.join().expect("join the acquiring thread");
        let result = result.expect(
            "acquire kept bridging for 5s while the OS lock stayed held; the stale-takeover \
             bridge must give up once its own timeout elapses",
        );

        assert!(
            matches!(result, Err(ref e) if e.code == ErrorCode::ScopeBusy),
            "the stale-takeover bridge must give up once its own timeout elapses, not keep \
             bridging until the OS lock actually releases; got {:?}",
            result.as_ref().err()
        );
    }

    #[test]
    fn released_scan_and_write_leases_reuse_the_stable_root_file_instead_of_removing_it() {
        let dir = tempfile::tempdir().unwrap();
        let lease = FileLease::new(dir.path().to_path_buf());
        let keys = [key("released")];
        for mode in [LeaseMode::Shared, LeaseMode::Exclusive] {
            drop(lease.acquire(&keys, mode, Duration::ZERO).unwrap());
            assert!(
                lease.lock_path(&keys[0]).exists(),
                "release unlinked the stable root file"
            );
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), keys.len());
        }
    }

    #[test]
    fn shared_scans_keep_exclusion_and_the_last_release_preserves_the_stable_file() {
        let dir = tempfile::tempdir().unwrap();
        let lease = FileLease::new(dir.path().to_path_buf());
        let keys = [key("shared")];
        let first = lease
            .acquire(&keys, LeaseMode::Shared, Duration::ZERO)
            .unwrap();
        let second = lease
            .acquire(&keys, LeaseMode::Shared, Duration::ZERO)
            .unwrap();
        drop(first);
        assert!(lease
            .acquire(&keys, LeaseMode::Exclusive, Duration::ZERO)
            .is_err());
        drop(second);
        assert!(
            lease.lock_path(&keys[0]).exists(),
            "last release removed a legacy waiter's file"
        );
        assert!(lease
            .acquire(&keys, LeaseMode::Exclusive, Duration::ZERO)
            .is_ok());
    }

    #[test]
    fn an_unlocked_stale_root_file_is_reused_instead_of_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let lease = FileLease::new(dir.path().to_path_buf());
        let keys = [key("stale")];
        let path = lease.lock_path(&keys[0]);
        fs::write(&path, "4294967295|0").unwrap();
        drop(
            lease
                .acquire(&keys, LeaseMode::Exclusive, Duration::ZERO)
                .unwrap(),
        );
        assert!(
            path.exists(),
            "a stale holder record caused the lock file to be removed"
        );
    }

    #[test]
    fn contending_writers_keep_exclusion_and_reuse_the_same_stable_root_file() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Barrier};
        let dir = tempfile::tempdir().unwrap();
        let lease = Arc::new(FileLease::new(dir.path().to_path_buf()));
        let active = Arc::new(AtomicUsize::new(0));
        let start = Arc::new(Barrier::new(4));
        std::thread::scope(|scope| {
            for _ in 0..4 {
                let (lease, active, start) =
                    (Arc::clone(&lease), Arc::clone(&active), Arc::clone(&start));
                scope.spawn(move || {
                    start.wait();
                    for _ in 0..50 {
                        let held = lease
                            .acquire(
                                &[key("contended")],
                                LeaseMode::Exclusive,
                                Duration::from_secs(5),
                            )
                            .unwrap();
                        assert_eq!(
                            active.fetch_add(1, Ordering::SeqCst),
                            0,
                            "two writers entered the same root"
                        );
                        std::thread::yield_now();
                        assert_eq!(active.fetch_sub(1, Ordering::SeqCst), 1);
                        drop(held);
                    }
                });
            }
        });
        assert!(
            lease.lock_path(&key("contended")).exists(),
            "the stable root file was removed"
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn a_failed_multi_root_acquire_releases_earlier_locks_and_preserves_their_stable_files() {
        let dir = tempfile::tempdir().unwrap();
        let lease = FileLease::new(dir.path().to_path_buf());
        let first = key("a");
        let busy = key("b");
        let held = lease
            .acquire(
                std::slice::from_ref(&busy),
                LeaseMode::Exclusive,
                Duration::ZERO,
            )
            .unwrap();
        assert!(lease
            .acquire(&[first.clone(), busy], LeaseMode::Exclusive, Duration::ZERO)
            .is_err());
        assert!(
            lease.lock_path(&first).exists(),
            "partial acquisition removed the stable file"
        );
        assert!(lease
            .acquire(&[first], LeaseMode::Exclusive, Duration::ZERO)
            .is_ok());
        drop(held);
    }

    #[test]
    fn a_held_legacy_directory_lock_cannot_extend_the_root_acquisition_deadline() {
        let dir = tempfile::tempdir().unwrap();
        let lease = FileLease::new(dir.path().to_path_buf());
        let keys = [key("directory-blocked")];
        let held = lease
            .acquire(&keys, LeaseMode::Exclusive, Duration::from_secs(1))
            .unwrap();
        let directory = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.path().join(LEGACY_DIRECTORY_LOCK))
            .unwrap();
        directory.lock().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                let result = lease.acquire(&keys, LeaseMode::Exclusive, Duration::from_millis(50));
                tx.send(result.map(drop)).unwrap();
            });
            let outcome = rx.recv_timeout(Duration::from_secs(5));
            drop(directory);
            worker.join().unwrap();
            drop(held);
            assert!(
                matches!(outcome, Ok(Err(ref error)) if error.code == ErrorCode::ScopeBusy),
                "the directory lock ignored the acquisition deadline: {outcome:?}"
            );
        });
    }

    #[test]
    fn release_does_not_wait_for_a_held_directory_lock() {
        let dir = tempfile::tempdir().unwrap();
        let lease = FileLease::new(dir.path().to_path_buf());
        let (held_tx, held_rx) = std::sync::mpsc::channel();
        let (drop_tx, drop_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            let lease = &lease;
            let worker = scope.spawn(move || {
                let held = lease
                    .acquire(&[key("release")], LeaseMode::Exclusive, Duration::ZERO)
                    .unwrap();
                held_tx.send(()).unwrap();
                drop_rx.recv().unwrap();
                drop(held);
                done_tx.send(()).unwrap();
            });
            held_rx.recv().unwrap();
            let directory = OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(dir.path().join(LEGACY_DIRECTORY_LOCK))
                .unwrap();
            directory.lock().unwrap();
            drop_tx.send(()).unwrap();
            let released = done_rx.recv_timeout(Duration::from_secs(5)).is_ok();
            drop(directory);
            worker.join().unwrap();
            assert!(released, "release waited for the directory lock");
        });
    }
    #[test]
    fn a_held_legacy_directory_lock_does_not_block_a_free_root() {
        let dir = tempfile::tempdir().unwrap();
        let lease = FileLease::new(dir.path().to_path_buf());
        let directory = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.path().join(LEGACY_DIRECTORY_LOCK))
            .unwrap();
        directory.lock().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                tx.send(
                    lease
                        .acquire(
                            &[key("free-root")],
                            LeaseMode::Exclusive,
                            Duration::from_millis(50),
                        )
                        .map(drop),
                )
                .unwrap();
            });
            let acquired = rx.recv_timeout(Duration::from_secs(5));
            drop(directory);
            worker.join().unwrap();
            assert!(
                matches!(acquired, Ok(Ok(()))),
                "a legacy directory guard blocked an independent root: {acquired:?}"
            );
        });
    }
}
