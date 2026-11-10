// ============================================================================
// Skills Module - data_folder_status
// Unit 6.3: the app data folder's `schema_version` marker, checked once at
// startup before anything else opens the folder. A folder older than this
// build is migrated forward in place; a folder newer than this build blocks
// the data layer and hands the frontend one message naming both versions.
// ============================================================================

use std::path::Path;
use std::sync::Mutex;

use tauri::Manager;

/// Startup verdict on the app data folder's version. `None` until
/// [`check_and_migrate`] runs in `lib.rs`'s `setup`; `Some` names the app
/// to update, and stays set for the life of the process - there is no way
/// to un-block a running process short of restarting it against an
/// updated app.
#[derive(Default)]
pub struct DataFolderStatusState(pub Mutex<Option<String>>);

/// Reads `app_data`'s version marker, migrates it forward in place when it
/// is older than [`skill_studio_host::CURRENT_DATA_VERSION`], and returns
/// the blocking message when it is newer. Called once, before
/// `lib.rs::open_event_store_at`, so a migration always lands before anything
/// else touches the folder, and a newer folder is never opened at all.
pub fn check_and_migrate(app_data: &Path) -> Option<String> {
    let fs = skill_studio_host::RealDataFolderFs::new();
    let folder_version = match skill_studio_host::read_version(&fs, app_data) {
        Ok(version) => version,
        Err(error) => {
            let message = format!(
                "Could not read the data folder's version marker at {}: {error}. \
                 The app cannot tell whether this folder is safe to open.",
                app_data.display()
            );
            eprintln!("[data_folder_version] {message}");
            return Some(message);
        }
    };

    if let Err(mismatch) =
        skill_studio_host::check_compatible(folder_version, skill_studio_host::CURRENT_DATA_VERSION)
    {
        return Some(skill_studio_host::newer_data_folder_message(mismatch));
    }

    if folder_version < skill_studio_host::CURRENT_DATA_VERSION {
        if let Err(error) = skill_studio_host::migrate(
            &fs,
            app_data,
            folder_version,
            skill_studio_host::CURRENT_DATA_VERSION,
        ) {
            let message = format!(
                "Migrating the data folder at {} failed: {error}. \
                 The folder is unchanged; restart the app to retry.",
                app_data.display()
            );
            eprintln!("[data_folder_version] {message}");
            return Some(message);
        }
    }

    None
}

/// Checks the folder before invoking the data-layer opener used by startup.
pub fn check_and_open<T>(
    app_data: &Path,
    open: impl FnOnce() -> Option<T>,
) -> (Option<String>, Option<T>) {
    let message = check_and_migrate(app_data);
    let store = if message.is_none() { open() } else { None };
    (message, store)
}

/// True when no [`DataFolderStatusState`] message is set for `app` - the
/// timing log ([`crate::timing_log::record_command`]) and the update-check
/// writer (`skill_update_check::check_now`) both consult this before
/// touching `app_data_dir`, so neither background job writes into a folder
/// [`check_and_migrate`] refused to open. Absent state (nothing has called
/// [`check_and_migrate`] yet) reads as writable, since setup runs that check
/// before managing any state a command could reach.
pub fn data_folder_writable(app: &tauri::AppHandle) -> bool {
    match app.try_state::<DataFolderStatusState>() {
        None => true,
        Some(state) => state.0.lock().is_ok_and(|guard| guard.is_none()),
    }
}

/// The blocking message [`check_and_migrate`] set at startup, if any - the
/// frontend calls this before rendering its normal chrome (`App.tsx`).
#[tauri::command]
// Tauri commands deserialize their arguments fresh per invocation, so
// `state` can't be borrowed from the caller - it must be owned.
#[allow(clippy::needless_pass_by_value)]
pub fn data_folder_status(state: tauri::State<'_, DataFolderStatusState>) -> Option<String> {
    state.0.lock().ok().and_then(|guard| guard.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_folder_startup_refuses_the_event_store_or_opens_an_incompatible_database() {
        let dir = tempfile::tempdir().unwrap();
        let app_data = dir.path().join("app");
        let data_root = dir.path().join("home");
        std::fs::create_dir(&app_data).unwrap();
        let current = skill_studio_host::CURRENT_DATA_VERSION;
        let newer = current.checked_add(1).unwrap();
        let marker = format!("{newer}\n");
        std::fs::write(app_data.join("schema_version"), &marker).unwrap();
        let mut opens = 0;
        let (message, store) = check_and_open(&app_data, || {
            opens += 1;
            crate::open_event_store_at(&app_data, &data_root)
        });
        assert_eq!(
            opens, 0,
            "incompatible startup reached the real database opener"
        );
        assert!(store.is_none());
        let message = message.expect("startup must explain the incompatible folder");
        assert!(message.contains(&newer.to_string()), "{message}");
        assert!(message.contains(&current.to_string()), "{message}");
        assert_eq!(
            std::fs::read_to_string(app_data.join("schema_version")).unwrap(),
            marker
        );
        assert_eq!(std::fs::read_dir(&app_data).unwrap().count(), 1);
        assert!(!data_root.exists());
    }

    #[test]
    fn compatible_folder_startup_opens_the_event_store_or_blocks_usable_data() {
        let dir = tempfile::tempdir().unwrap();
        let app_data = dir.path().join("app");
        let data_root = dir.path().join("home");
        let mut opens = 0;
        let (message, store) = check_and_open(&app_data, || {
            opens += 1;
            crate::open_event_store_at(&app_data, &data_root)
        });
        assert!(message.is_none(), "{message:?}");
        assert_eq!(opens, 1);
        assert!(
            store.is_some(),
            "compatible startup must open the real event store"
        );
    }

    /// A marker file that doesn't hold a decimal version blocks startup with
    /// a message naming the path and the read error.
    #[test]
    fn corrupt_version_marker_blocks_startup_or_names_the_read_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("schema_version"), b"not-a-number").expect("write marker");

        let message = check_and_migrate(dir.path()).expect("a corrupt marker must block startup");
        assert!(
            message.contains(&dir.path().display().to_string()),
            "message should name the folder path, got: {message}"
        );
        assert!(
            message.contains("version"),
            "message should describe the version-read failure, got: {message}"
        );
    }

    /// A migration that fails partway through blocks startup with a message
    /// naming the path and the write error, rather than silently leaving
    /// `check_and_migrate` reporting the folder as fine. A valid `0\n`
    /// marker keeps the read step passing - a directory at the marker path
    /// would fail the read step instead, exiting through the corrupt-marker
    /// branch above without ever reaching `migrate`. A read-only folder then
    /// makes the marker write's temp-file creation fail, standing in for a
    /// real disk-full or permission error without needing to inject one into
    /// `RealFs`.
    #[cfg(unix)]
    #[test]
    fn migration_failure_blocks_startup_or_names_the_write_error() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("schema_version"), b"0\n").expect("write marker");
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555))
            .expect("make folder read-only");

        let result = check_and_migrate(dir.path());

        // Restore permissions so the tempdir can be cleaned up regardless of
        // the assertion outcome.
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755))
            .expect("restore folder permissions");

        let message = result.expect("a failed migration must block startup");
        assert!(
            message.contains("Migrating"),
            "message should name the migration step, got: {message}"
        );
        assert!(
            message.contains(&dir.path().display().to_string()),
            "message should name the folder path, got: {message}"
        );
    }
}
