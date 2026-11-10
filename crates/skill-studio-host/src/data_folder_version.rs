//! The app data folder's `schema_version` marker and its forward migration
//! (`docs/action-map/plan.md` unit 6.3).
//!
//! The data folder (Tauri's `app_data_dir`) holds the event store, the
//! timing log, and backups; a folder with no marker file is version 0
//! ("pre-versioned" - the layout every folder had before this unit
//! shipped). [`migrate`] runs the table in [`STEPS`] one entry at a time,
//! in order, and writes the marker only after every step's own files are
//! durably in place - so a crash mid-migration always leaves the marker
//! reading the version it started at, never a version whose files are
//! only partly written.
//!
//! [`Fs`] is this module's own read/write seam, not `skill-studio-core`'s
//! `ScopeFs`: that port's `confine` keys every path to a `RuntimeScope`'s
//! home and project roots, and the app data folder isn't one of those -
//! inventing a scope root for one well-known file would add confinement
//! machinery with no safety benefit here. `skill-studio-host` is allowed to
//! touch `std::fs` directly (only the core crate is not), so [`RealFs`]
//! does.

use std::fmt;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// The data folder version this build writes and expects. A folder read at
/// a version higher than this was written by a newer app; see
/// [`check_compatible`].
pub const CURRENT_DATA_VERSION: u32 = 1;

const VERSION_FILE_NAME: &str = "schema_version";

/// Read/write seam [`read_version`] and [`migrate`] run through, so a test
/// can substitute a fixture or an injected failure without touching real
/// disk.
pub trait Fs {
    /// Reads `path` whole. `Ok(None)` when nothing is there.
    fn read(&self, path: &Path) -> io::Result<Option<Vec<u8>>>;
    /// Writes `bytes` to `path` through a temp file, fsync, and a rename,
    /// so a crash never leaves a partial file at `path`.
    fn write_durable(&self, path: &Path, bytes: &[u8]) -> io::Result<()>;
}

/// [`Fs`] over the real filesystem.
#[derive(Debug, Default, Clone, Copy)]
pub struct RealFs;

impl RealFs {
    /// Builds a new adapter. Holds no state; every call goes straight to the OS.
    pub fn new() -> Self {
        RealFs
    }
}

/// Counter mixed into temp file names so concurrent writers in one process
/// never collide - same trick as `fs::RealFs::write_atomic`.
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

impl Fs for RealFs {
    fn read(&self, path: &Path) -> io::Result<Option<Vec<u8>>> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn write_durable(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let dir = path
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
        std::fs::create_dir_all(dir)?;
        let file_name = path
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?
            .to_string_lossy();
        let counter = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp_path = dir.join(format!(".{file_name}.tmp-{}-{counter}", std::process::id()));
        {
            use std::io::Write;
            let mut file = std::fs::File::create(&tmp_path)?;
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        std::fs::rename(&tmp_path, path)?;
        // The rename is durable once the directory's own entry is flushed,
        // not just the file's contents (`fsops.rs`'s write_file does the
        // same after its rename).
        if let Ok(dir_handle) = std::fs::File::open(dir) {
            let _ = dir_handle.sync_all();
        }
        Ok(())
    }
}

/// Keeps compatible folders locked until the protected store is dropped.
#[derive(Debug)]
pub struct DataFolderGuard {
    _locks: Vec<std::fs::File>,
}

fn folder_identity(folder: &Path) -> io::Result<std::path::PathBuf> {
    match folder.canonicalize() {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let absolute = std::path::absolute(folder)?;
            let parent = absolute.parent().ok_or(error)?;
            Ok(
                folder_identity(parent)?.join(absolute.file_name().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "folder has no name")
                })?),
            )
        }
        Err(error) => Err(error),
    }
}

fn private_coordination_root(base: &Path, uid: u32) -> io::Result<std::path::PathBuf> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};

    let root = base.join(format!("skill-studio-data-locks-{uid}"));
    match std::fs::DirBuilder::new().mode(0o700).create(&root) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let metadata = std::fs::symlink_metadata(&root)?;
    if !metadata.is_dir()
        || metadata.uid() != uid
        || metadata.permissions().mode() & 0o7777 != 0o700
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "Coordination folder {} must be owned by user {uid} with mode 0700",
                root.display()
            ),
        ));
    }
    Ok(root)
}

fn coordination_lock(folder: &Path) -> io::Result<std::fs::File> {
    // CLI and desktop must use the same lock inode even when TMPDIR differs.
    let root = private_coordination_root(Path::new("/tmp"), rustix::process::geteuid().as_raw())?;
    let key = skill_studio_core::identity::sha256_hex(folder.as_os_str().as_encoded_bytes());
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join(key))
}

fn lock_error(error: std::fs::TryLockError) -> String {
    match error {
        std::fs::TryLockError::WouldBlock => {
            "Close the other Skill Studio process and try again. It holds the data folder lock."
                .to_owned()
        }
        std::fs::TryLockError::Error(error) => error.to_string(),
    }
}

/// Holds a compatible folder without creating or migrating its contents.
pub fn guard_read_data_folder(folder: &Path) -> Result<DataFolderGuard, String> {
    let folder = folder_identity(folder).map_err(|error| error.to_string())?;
    let validate = || {
        let version = read_version(&RealFs, &folder).map_err(|error| error.to_string())?;
        check_compatible(version, CURRENT_DATA_VERSION).map_err(newer_data_folder_message)
    };
    validate()?;
    let lock = coordination_lock(&folder).map_err(|error| error.to_string())?;
    lock.try_lock_shared().map_err(lock_error)?;
    validate()?;
    Ok(DataFolderGuard { _locks: vec![lock] })
}

/// Validates all folders, migrates under exclusive locks, then keeps shared locks.
pub fn lock_data_folders(folders: &[&Path]) -> Result<DataFolderGuard, String> {
    lock_data_folders_before_lock(folders, || {})
}

fn lock_data_folders_before_lock(
    folders: &[&Path],
    before_lock: impl FnOnce(),
) -> Result<DataFolderGuard, String> {
    let validate = |folder: &Path| {
        let version = read_version(&RealFs, folder).map_err(|error| {
            format!(
                "Could not read the data folder version at {}: {error}",
                folder.display()
            )
        })?;
        check_compatible(version, CURRENT_DATA_VERSION).map_err(newer_data_folder_message)?;
        Ok::<u32, String>(version)
    };
    let mut folders = folders
        .iter()
        .map(|folder| folder_identity(folder))
        .collect::<io::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    folders.sort_unstable();
    folders.dedup();
    for folder in &folders {
        validate(folder)?;
    }
    before_lock();
    let locks = folders
        .iter()
        .map(|folder| coordination_lock(folder))
        .collect::<io::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    for lock in &locks {
        lock.try_lock_shared().map_err(lock_error)?;
    }
    let versions = folders
        .iter()
        .map(|folder| validate(folder))
        .collect::<Result<Vec<_>, _>>()?;
    if versions
        .iter()
        .any(|version| *version < CURRENT_DATA_VERSION)
    {
        // Concurrent lock upgrades can deadlock while either process holds a shared lock.
        for lock in &locks {
            lock.unlock().map_err(|error| error.to_string())?;
        }
        for (lock, version) in locks.iter().zip(&versions) {
            if *version < CURRENT_DATA_VERSION {
                lock.try_lock().map_err(lock_error)?;
            } else {
                lock.try_lock_shared().map_err(lock_error)?;
            }
        }
        let versions = folders
            .iter()
            .map(|folder| validate(folder))
            .collect::<Result<Vec<_>, _>>()?;
        for (folder, version) in folders.iter().zip(versions) {
            if version < CURRENT_DATA_VERSION {
                migrate(&RealFs, folder, version, CURRENT_DATA_VERSION)
                    .map_err(|error| format!("Migrating {}: {error}", folder.display()))?;
            }
        }
        for lock in &locks {
            lock.try_lock_shared().map_err(lock_error)?;
        }
    }
    for folder in &folders {
        validate(folder)?;
    }
    Ok(DataFolderGuard { _locks: locks })
}

/// Runs a transient operation while the folder stays protected.
pub fn with_data_folder<T>(
    folder: &Path,
    open: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    with_data_folders(&[folder], open)
}

/// Runs a transient operation while all folders stay protected.
pub fn with_data_folders<T>(
    folders: &[&Path],
    open: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let _guard = lock_data_folders(folders)?;
    open()
}

#[cfg(test)]
fn with_data_folders_before_lock<T>(
    folders: &[&Path],
    before_lock: impl FnOnce(),
    open: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let _guard = lock_data_folders_before_lock(folders, before_lock)?;
    open()
}

/// One version migration step: turns a folder at version `from` into
/// `from + 1`. `run` does whatever that step needs beyond the marker
/// itself - [`migrate`] writes the marker only after every step through
/// `to` has returned `Ok`.
struct MigrationStep {
    from: u32,
    run: fn(&dyn Fs, &Path) -> io::Result<()>,
}

/// Every step from version 0 up, in order. `CURRENT_DATA_VERSION` steps
/// live here: version `v`'s step has `from == v - 1`.
const STEPS: &[MigrationStep] = &[MigrationStep {
    from: 0,
    run: migrate_v0_to_v1,
}];

/// v0 (no marker file) to v1: the marker is the only thing that changes.
/// Neither the event store nor the timing log needed a layout change to
/// carry a version number, so this step writes nothing of its own.
///
/// Returns `io::Result` to match [`MigrationStep::run`]'s signature, which
/// every other step needs; this one just never fails.
#[allow(clippy::unnecessary_wraps)]
fn migrate_v0_to_v1(_fs: &dyn Fs, _folder: &Path) -> io::Result<()> {
    Ok(())
}

/// Reads `folder`'s version marker. A folder with no marker file is
/// version 0.
pub fn read_version(fs: &dyn Fs, folder: &Path) -> io::Result<u32> {
    let path = folder.join(VERSION_FILE_NAME);
    match fs.read(&path)? {
        None => Ok(0),
        Some(bytes) => String::from_utf8_lossy(&bytes)
            .trim()
            .parse::<u32>()
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{} does not hold a decimal version: {e}", path.display()),
                )
            }),
    }
}

/// Names which part of a [`migrate`] call failed: one of [`STEPS`], or the
/// final marker write.
#[derive(Debug)]
pub enum MigrationError {
    /// The step that turns version `from` into `from + 1` failed before
    /// writing anything the marker could safely point past.
    Step {
        /// The version the failed step would have moved the folder from.
        from: u32,
        /// The underlying I/O failure.
        source: io::Error,
    },
    /// Every step through `to` succeeded, but writing the marker itself
    /// failed.
    VersionMarker {
        /// The version the marker would have been set to.
        to: u32,
        /// The underlying I/O failure.
        source: io::Error,
    },
}

impl fmt::Display for MigrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MigrationError::Step { from, source } => {
                write!(f, "migration step from version {from} failed: {source}")
            }
            MigrationError::VersionMarker { to, source } => {
                write!(f, "writing the version {to} marker failed: {source}")
            }
        }
    }
}

impl std::error::Error for MigrationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MigrationError::Step { source, .. } | MigrationError::VersionMarker { source, .. } => {
                Some(source)
            }
        }
    }
}

/// Runs every step from `from` to `to`, in order, then writes the marker
/// last. A crash before this returns `Ok` leaves the marker reading
/// `from` still - see the module doc. `migrate` calls this with [`STEPS`];
/// the seam exists so a test can substitute a table where every step - not
/// just `migrate_v0_to_v1` - performs a write of its own, which
/// `fail_write_after` needs to land mid-loop rather than always hitting the
/// marker write.
fn migrate_with(
    steps: &[MigrationStep],
    fs: &dyn Fs,
    folder: &Path,
    from: u32,
    to: u32,
) -> Result<(), MigrationError> {
    for step in steps.iter().filter(|s| s.from >= from && s.from < to) {
        (step.run)(fs, folder).map_err(|source| MigrationError::Step {
            from: step.from,
            source,
        })?;
    }
    let path = folder.join(VERSION_FILE_NAME);
    fs.write_durable(&path, format!("{to}\n").as_bytes())
        .map_err(|source| MigrationError::VersionMarker { to, source })
}

/// Runs every step from `from` to `to`, in order, then writes the marker
/// last. A crash before this returns `Ok` leaves the marker reading
/// `from` still - see the module doc.
pub fn migrate(fs: &dyn Fs, folder: &Path, from: u32, to: u32) -> Result<(), MigrationError> {
    migrate_with(STEPS, fs, folder, from, to)
}

/// The data folder is newer than this app build understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewerDataFolder {
    /// The version found on disk.
    pub folder_version: u32,
    /// The version this app build writes and understands
    /// ([`CURRENT_DATA_VERSION`] for a real run).
    pub app_version: u32,
}

/// Compares `folder_version` against `app_version`. `Err` only when the
/// folder is ahead of the app - the caller must not open its data layer
/// against a folder a future version of itself already migrated.
pub fn check_compatible(folder_version: u32, app_version: u32) -> Result<(), NewerDataFolder> {
    if folder_version > app_version {
        Err(NewerDataFolder {
            folder_version,
            app_version,
        })
    } else {
        Ok(())
    }
}

/// One line naming both versions, for the startup-blocking message the
/// desktop app shows when [`check_compatible`] refuses to open.
pub fn newer_data_folder_message(mismatch: NewerDataFolder) -> String {
    format!(
        "This data folder is version {} but this app understands up to version {}. Update the app to open it.",
        mismatch.folder_version, mismatch.app_version
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[test]
    fn newer_build_migration_refuses_while_older_desktop_holds_the_folder_or_hangs() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("a");
        let second = dir.path().join("b");
        std::fs::create_dir(&second).unwrap();
        let older = coordination_lock(&folder_identity(&second).unwrap()).unwrap();
        older.lock_shared().unwrap();
        let error = lock_data_folders(&[&first, &second]).unwrap_err();
        assert!(
            error.contains("Close the other Skill Studio process"),
            "{error}"
        );
        assert!(!first.exists());
        assert!(!second.join(VERSION_FILE_NAME).exists());
        let released = coordination_lock(&folder_identity(&first).unwrap()).unwrap();
        released.try_lock().unwrap();
        drop(released);
        drop(older);
        lock_data_folders(&[&first, &second]).unwrap();
    }

    #[test]
    fn startup_coordination_uses_private_user_folder_or_other_accounts_block_access() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("skill-studio-data-locks");
        std::fs::create_dir(&legacy).unwrap();
        std::fs::set_permissions(&legacy, std::fs::Permissions::from_mode(0o755)).unwrap();
        let uid = rustix::process::geteuid().as_raw();
        let root = private_coordination_root(dir.path(), uid).unwrap();
        let metadata = std::fs::metadata(&root).unwrap();
        assert_eq!(metadata.uid(), uid);
        assert_eq!(metadata.permissions().mode() & 0o7777, 0o700);
        assert_eq!(private_coordination_root(dir.path(), uid).unwrap(), root);
        assert_ne!(root, legacy);
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(private_coordination_root(dir.path(), uid).is_err());
        let other_uid = uid + 1;
        let other = dir
            .path()
            .join(format!("skill-studio-data-locks-{other_uid}"));
        std::fs::create_dir(&other).unwrap();
        std::fs::set_permissions(&other, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(private_coordination_root(dir.path(), other_uid).is_err());
        std::fs::remove_dir(&root).unwrap();
        std::os::unix::fs::symlink(&legacy, &root).unwrap();
        assert!(private_coordination_root(dir.path(), uid).is_err());
        assert_eq!(
            std::fs::metadata(&legacy).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }

    #[test]
    fn startup_symlink_alias_opens_once_or_deadlocks_on_its_own_lock() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let alias = dir.path().join("alias");
        std::os::unix::fs::symlink(&real, &alias).unwrap();
        with_data_folders(&[&real, &alias], || Ok(())).unwrap();
        assert_eq!(
            folder_identity(&dir.path().join("alias/missing")).unwrap(),
            folder_identity(&dir.path().join("real/missing")).unwrap()
        );
    }

    #[test]
    fn startup_racing_newer_history_leaves_missing_app_data_absent_or_creates_files_on_refusal() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("app");
        let history = dir.path().join("history");
        std::fs::create_dir(&history).unwrap();
        let error = with_data_folders_before_lock(
            &[&app, &history],
            || std::fs::write(history.join(VERSION_FILE_NAME), b"2\n").unwrap(),
            || Err::<(), _>("opener ran".to_owned()),
        )
        .unwrap_err();
        assert_eq!(
            error,
            newer_data_folder_message(NewerDataFolder {
                folder_version: 2,
                app_version: CURRENT_DATA_VERSION,
            })
        );
        assert!(!app.exists(), "refusal created the app-data folder");
        assert_eq!(std::fs::read_dir(history).unwrap().count(), 1);
    }

    #[test]
    fn startup_open_holds_folder_lock_through_migration_and_sqlite_or_allows_a_newer_writer() {
        let dir = tempfile::tempdir().unwrap();
        with_data_folder(dir.path(), || {
            assert_eq!(
                read_version(&RealFs, dir.path()).unwrap(),
                CURRENT_DATA_VERSION
            );
            let child = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "data_folder_version::tests::competing_process_cannot_lock_during_startup_open_or_migrations_can_race"])
                .env("SKILL_STUDIO_LOCK_PROBE", dir.path())
                .output().unwrap();
            assert!(child.status.success(), "{}", String::from_utf8_lossy(&child.stdout));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn competing_process_cannot_lock_during_startup_open_or_migrations_can_race() {
        let Some(folder) = std::env::var_os("SKILL_STUDIO_LOCK_PROBE") else {
            return;
        };
        let competing = coordination_lock(&folder_identity(Path::new(&folder)).unwrap()).unwrap();
        if std::env::var_os("SKILL_STUDIO_LOCK_AVAILABLE").is_some() {
            competing.try_lock().unwrap();
        } else {
            assert!(matches!(
                competing.try_lock(),
                Err(std::fs::TryLockError::WouldBlock)
            ));
        }
    }

    #[test]
    fn startup_waiting_on_newer_migration_refuses_without_lowering_marker_or_opens_stale_version_zero(
    ) {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().to_path_buf();
        let lock = coordination_lock(&folder_identity(&folder).unwrap()).unwrap();
        lock.lock().unwrap();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (continue_tx, continue_rx) = std::sync::mpsc::channel();
        let older = std::thread::spawn(move || {
            with_data_folders_before_lock(
                &[&folder],
                || {
                    ready_tx.send(()).unwrap();
                    continue_rx.recv().unwrap();
                },
                || Err::<(), _>("incompatible opener ran".to_owned()),
            )
        });
        ready_rx.recv().unwrap();
        let marker = dir.path().join("schema_version");
        std::fs::write(&marker, b"2\n").unwrap();
        let before = (
            std::fs::read(&marker).unwrap(),
            std::fs::metadata(&marker).unwrap().modified().unwrap(),
        );
        drop(lock);
        continue_tx.send(()).unwrap();
        let error = older.join().unwrap().unwrap_err();
        assert_eq!(
            error,
            newer_data_folder_message(NewerDataFolder {
                folder_version: 2,
                app_version: CURRENT_DATA_VERSION
            })
        );
        assert_eq!(
            before,
            (
                std::fs::read(&marker).unwrap(),
                std::fs::metadata(&marker).unwrap().modified().unwrap()
            )
        );
    }

    /// Extracts every run of ASCII digits in `text`, in order, as `u32`s -
    /// used to assert on the version numbers a message names without
    /// matching a digit that's really part of a larger number.
    fn digit_runs(text: &str) -> Vec<u32> {
        let mut numbers = Vec::new();
        let mut digits = String::new();
        for ch in text.chars() {
            if ch.is_ascii_digit() {
                digits.push(ch);
            } else if !digits.is_empty() {
                numbers.push(digits.parse().expect("digit run parses as u32"));
                digits.clear();
            }
        }
        if !digits.is_empty() {
            numbers.push(digits.parse().expect("digit run parses as u32"));
        }
        numbers
    }

    /// In-memory [`Fs`] fixture. `fail_write_after` counts down across
    /// every [`Fs::write_durable`] call the fixture sees (steps' own
    /// writes and the final marker write alike); the call it reaches zero
    /// on fails and nothing after it runs.
    #[derive(Default)]
    struct FakeFs {
        files: Mutex<HashMap<std::path::PathBuf, Vec<u8>>>,
        fail_write_after: Mutex<Option<u32>>,
    }

    impl FakeFs {
        fn with_files(files: &[(&str, &[u8])]) -> Self {
            let map = files
                .iter()
                .map(|(path, bytes)| (std::path::PathBuf::from(path), bytes.to_vec()))
                .collect();
            FakeFs {
                files: Mutex::new(map),
                fail_write_after: Mutex::new(None),
            }
        }

        fn fail_write_after(&self, n: u32) {
            *self.fail_write_after.lock().expect("lock") = Some(n);
        }

        fn get(&self, path: &Path) -> Option<Vec<u8>> {
            self.files.lock().expect("lock").get(path).cloned()
        }
    }

    impl Fs for FakeFs {
        fn read(&self, path: &Path) -> io::Result<Option<Vec<u8>>> {
            Ok(self.files.lock().expect("lock").get(path).cloned())
        }

        fn write_durable(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
            let mut fail_after = self.fail_write_after.lock().expect("lock");
            if let Some(remaining) = fail_after.as_mut() {
                if *remaining == 0 {
                    return Err(io::Error::other("injected write failure"));
                }
                *remaining -= 1;
            }
            drop(fail_after);
            self.files
                .lock()
                .expect("lock")
                .insert(path.to_path_buf(), bytes.to_vec());
            Ok(())
        }
    }

    const FOLDER: &str = "/data";
    const SENTINEL_PATH: &str = "/data/event_store.sqlite";
    const SENTINEL_BYTES: &[u8] = b"pretend event store bytes";

    fn fixture_at_version(version: u32) -> FakeFs {
        if version == 0 {
            FakeFs::with_files(&[(SENTINEL_PATH, SENTINEL_BYTES)])
        } else {
            FakeFs::with_files(&[
                (SENTINEL_PATH, SENTINEL_BYTES),
                ("/data/schema_version", format!("{version}\n").as_bytes()),
            ])
        }
    }

    /// `migrate` from every version below current reaches current, and
    /// leaves the folder's other files untouched. Fails production code
    /// that skips a step, stops early, or overwrites a file `STEPS` has
    /// no entry for.
    #[test]
    fn migration_from_each_earlier_schema_version_reaches_current_or_names_the_failed_step() {
        for from in 0..CURRENT_DATA_VERSION {
            let fs = fixture_at_version(from);
            let result = migrate(&fs, Path::new(FOLDER), from, CURRENT_DATA_VERSION);
            assert!(
                result.is_ok(),
                "migrating from version {from} failed: {result:?}"
            );

            let version = read_version(&fs, Path::new(FOLDER)).expect("read version");
            assert_eq!(
                version, CURRENT_DATA_VERSION,
                "migrating from version {from} left the marker at {version}, not {CURRENT_DATA_VERSION}"
            );
            assert_eq!(
                fs.get(Path::new(SENTINEL_PATH)).as_deref(),
                Some(SENTINEL_BYTES),
                "migrating from version {from} changed a file no step in STEPS names"
            );
        }
    }

    // Records which `TEST_STEPS` entries actually ran, so a test can tell
    // "skipped" from "ran but its write happened to succeed". Cleared at the
    // start of every test that reads it; each test owns its own thread under
    // the default test harness, so this thread-local never crosses tests.
    thread_local! {
        static STEP_LOG: std::cell::RefCell<Vec<u32>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    fn record_step(id: u32) {
        STEP_LOG.with(|log| log.borrow_mut().push(id));
    }

    fn test_step_1(fs: &dyn Fs, folder: &Path) -> io::Result<()> {
        record_step(1);
        fs.write_durable(&folder.join("step-1.marker"), b"step 1 ran")
    }

    fn test_step_2(fs: &dyn Fs, folder: &Path) -> io::Result<()> {
        record_step(2);
        fs.write_durable(&folder.join("step-2.marker"), b"step 2 ran")
    }

    /// A two-step table where - unlike [`STEPS`], whose only real step
    /// (`migrate_v0_to_v1`) writes nothing - every step performs its own
    /// `write_durable`. `fail_write_after` needs at least one such step to
    /// land mid-loop instead of always hitting the final marker write.
    const TEST_STEPS: &[MigrationStep] = &[
        MigrationStep {
            from: 0,
            run: test_step_1,
        },
        MigrationStep {
            from: 1,
            run: test_step_2,
        },
    ];

    /// A failure at any point during migration - a step's own write, or
    /// the final marker write - leaves the marker reading the pre-
    /// migration version and every other file byte-identical, and the
    /// error names the step (or the marker) that failed. Fails production
    /// code that writes the marker before every step has run, or that
    /// leaves a partial write on the disk when a step's write fails (that
    /// would be `write_durable` not going through temp+rename).
    #[test]
    fn migration_crash_after_any_step_leaves_folder_at_old_version_with_data_intact() {
        let step_count = TEST_STEPS.len() as u32;
        let total_writes = step_count + 1;

        for crash_after in 0..total_writes {
            let fs = fixture_at_version(0);
            fs.fail_write_after(crash_after);
            STEP_LOG.with(|log| log.borrow_mut().clear());

            let err = migrate_with(TEST_STEPS, &fs, Path::new(FOLDER), 0, 2)
                .expect_err("crash injected before the marker write must surface as Err");

            if crash_after < step_count {
                let expects_step =
                    matches!(err, MigrationError::Step { from, .. } if from == crash_after);
                assert!(
                    expects_step,
                    "crash after write #{crash_after} should name the step from version {crash_after}, got {err:?}"
                );
            } else {
                let expects_marker =
                    matches!(err, MigrationError::VersionMarker { to, .. } if to == 2);
                assert!(
                    expects_marker,
                    "crash after write #{crash_after} should name the marker write, got {err:?}"
                );
            }

            let version = read_version(&fs, Path::new(FOLDER)).expect("read version");
            assert_eq!(
                version, 0,
                "a crash after write #{crash_after} left the marker at {version}, not 0"
            );
            assert_eq!(
                fs.get(Path::new(SENTINEL_PATH)).as_deref(),
                Some(SENTINEL_BYTES),
                "a crash after write #{crash_after} changed the pre-migration sentinel file"
            );
        }
    }

    /// `migrate_with` skips every step whose `from` is below the folder's
    /// current version - fails production code that runs every step in the
    /// table regardless of `from`.
    #[test]
    fn migrate_with_test_steps_skips_steps_before_from_or_names_the_extra_step_that_ran() {
        let fs = fixture_at_version(1);
        STEP_LOG.with(|log| log.borrow_mut().clear());

        let result = migrate_with(TEST_STEPS, &fs, Path::new(FOLDER), 1, 2);
        assert!(
            result.is_ok(),
            "migrating from version 1 failed: {result:?}"
        );

        let ran = STEP_LOG.with(|log| log.borrow().clone());
        assert_eq!(
            ran,
            vec![2],
            "migrating from version 1 should run only step 2 (from=1), ran {ran:?} instead"
        );
    }

    /// The read-side compatibility check names both versions when the
    /// folder is ahead of the app, and is silent otherwise. Fails
    /// production code that returns `Ok` for a newer folder, or that
    /// drops either version number from the mismatch.
    #[test]
    fn newer_data_folder_is_reported_with_both_versions_or_names_the_missing_check() {
        let mismatch =
            check_compatible(CURRENT_DATA_VERSION + 1, CURRENT_DATA_VERSION).unwrap_err();
        assert_eq!(mismatch.folder_version, CURRENT_DATA_VERSION + 1);
        assert_eq!(mismatch.app_version, CURRENT_DATA_VERSION);

        let message = newer_data_folder_message(mismatch);
        // Parse out the numbers the message contains, in order, rather than
        // searching for the version numbers as substrings: a `.find()` on
        // e.g. "1" would also match inside "10", so a version pair like
        // (10, 1) could pass this assertion for the wrong reason.
        let numbers = digit_runs(&message);
        assert_eq!(
            numbers,
            vec![CURRENT_DATA_VERSION + 1, CURRENT_DATA_VERSION],
            "message should name the folder version ({}) before the app version ({}), got: {message}",
            CURRENT_DATA_VERSION + 1,
            CURRENT_DATA_VERSION
        );

        assert!(check_compatible(CURRENT_DATA_VERSION, CURRENT_DATA_VERSION).is_ok());
        assert!(check_compatible(0, CURRENT_DATA_VERSION).is_ok());
    }
}
