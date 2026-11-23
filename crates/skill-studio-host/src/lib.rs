// unwrap/expect are fine in test code; production code must use ?
// or an explicit error.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
#![allow(clippy::print_stdout, clippy::print_stderr)]

//! Real-world adapters for `skill-studio-core`.
//!
//! This crate holds no policy. Every type here implements one port trait
//! from `skill_studio_core::ports` over the actual operating system: real
//! files, the wall clock, fresh ids, advisory file locks, and a notice
//! sink. Its dependencies are `std`, `skill-studio-core`, and a small set of
//! parsing and platform crates (`ulid`, `chrono`, `serde_json`, `toml`,
//! `rusqlite`); no `tokio`, no `tauri`. Adapters that need an async runtime
//! or Tauri state wrap these types rather than reimplement them.
//!
//! [`default_ports`] wires the common case: real filesystem, real clock,
//! monotonic ULIDs, file-lock leases, no history store yet, and discarded
//! notices. A caller that needs a process spawner, project discovery, or a
//! `PATH` lookup sets those fields on the returned `Ports` itself.

#![deny(missing_docs)]
// `deny`, not `forbid`, so `fs::macos_exchange` can locally `#[allow(unsafe_code)]`
// for `renamex_np` (atomic path exchange, no safe `std` wrapper - see its
// module doc for what the `unsafe` block promises).
#![deny(unsafe_code)]

/// Tests that fork a child which pauses before exec, and tests that check a
/// lock is free after its holder drops, run one at a time. A fork copies
/// every open file of the process, other tests' lock files included, until
/// the child calls exec, so a paused child can hold another test's lock.
#[cfg(test)]
pub(crate) fn fork_test_guard() -> std::sync::MutexGuard<'static, ()> {
    static FORKING_TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());
    FORKING_TESTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

mod builder;
mod clock;
mod data_folder_version;
mod discovery;
mod fs;
mod gh_currency;
mod gh_unavailable;
mod harness_detect;
mod history;
mod ids;
mod lease;
mod opencode_db;
mod opencode_guidance;
mod pack_remote;
mod probe_cache;
mod sink;
mod skill_uses;
#[cfg(feature = "telemetry")]
pub mod telemetry;
#[cfg(all(test, unix))]
mod test_scripts;
mod tools;
mod usage_report;

pub use builder::{
    default_ports, default_ports_for_scope, default_ports_with_discovery,
    default_ports_with_history,
};
/// Directory capability used by the host filesystem's confined operations.
pub use cap_std::fs::Dir as ConfinedDir;
pub use clock::SystemClock;
pub use data_folder_version::{
    check_compatible, guard_read_data_folder, lock_data_folders, migrate,
    newer_data_folder_message, read_version, with_data_folder, with_data_folders, DataFolderGuard,
    Fs as DataFolderFs, MigrationError, NewerDataFolder, RealFs as RealDataFolderFs,
    CURRENT_DATA_VERSION,
};
pub use discovery::{
    agent_home, agent_path, codex_home, discover_skill_projects, discovery_harnesses,
    opencode_config_dir, opencode_config_dir_under, resolve_agent_homes, HostProjectDiscovery,
};
pub use fs::{
    copy_confined_entry, open_confined_child, open_confined_dir, write_confined_file, RealFs,
};
pub use gh_currency::{
    parse_tree_response, GhCommitLookup, GhPluginManifestLookup, GhSourceTreeLookup,
};
pub use gh_unavailable::NoGhLookup;
pub use harness_detect::{child_path, resolve_program, spawn_retrying_busy, RealProcessSpawner};
pub use history::{
    hash_entry, reimport_event_as_legacy, GuardedNoHistoryOpener, NoHistoryOpener,
    SqliteHistoryOpener,
};
pub use ids::UlidIds;
pub use lease::FileLease;
pub use opencode_db::opencode_databases;
pub use pack_remote::GitPackRemote;
pub use sink::NoopSink;
pub use skill_uses::{
    has_usage_reader, is_skill_use_change, is_skill_use_change_with_databases,
    skill_use_watch_paths, skill_use_watch_paths_with_databases, SkillInvocationIndex,
    SkillUseRefreshReport, SkillUseWatchPath,
};
pub use tools::{terminal_editor_command, LoginShellToolLookup, PathToolLookup};
pub use usage_report::{
    cli_usage_cache_path, desktop_usage_cache_path, parse_usage_time, usage_report, SkillUsage,
    SkillUsageRow, UsageFilter, UsageReport, DEFAULT_USAGE_DAYS,
};

mod review;
pub use review::review_due_report;

pub use usage_report::library_report;
