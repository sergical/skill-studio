// Integration test binaries aren't covered by the lib crate's
// `cfg_attr(test, allow(...))`: this file compiles as its own crate, so
// the same allow needs to be declared here too.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Real-disk integration tests for the Codex adapter: `ops::park` and
//! `ops::unpark` leaving Codex's config alone, and `CODEX_HOME` support. Like `park_and_unpark.rs`, these run against
//! `skill-studio-host`'s real adapters rather than the in-memory `FixtureFs`.

use std::path::Path;
use std::sync::{Arc, Mutex};

use skill_studio_core::discovery_sources::DiscoverySources;
use skill_studio_core::dto::{ParkRequest, UnparkRequest};
use skill_studio_core::harness::HarnessCatalog;
use skill_studio_core::identity::RootKind;
use skill_studio_core::ops;
use skill_studio_core::ports::{Ports, Runtime};
use skill_studio_core::scope::RuntimeScope;
use skill_studio_core::testing::golden::{ctx, unique_temp_dir};
use skill_studio_core::testing::{FakeClock, FakeIds, RecordingSink};

use skill_studio_host::{FileLease, RealFs, SkillInvocationIndex, SqliteHistoryOpener};

const UNIVERSAL_ROOT_RELATIVE: &str = ".agents/skills";
const PARKED_ROOT_RELATIVE: &str = ".agents/skills-parked";

/// Serializes the one test below that mutates the process-wide `CODEX_HOME`
/// env var - `skill_studio_host::codex_home` is the one place allowed to
/// read it, and cargo runs every `#[test]` in this binary on shared threads.
static CODEX_HOME_ENV_LOCK: Mutex<()> = Mutex::new(());

fn runtime_for(home: &Path, codex_home: Option<&Path>) -> Runtime {
    let history_root = home.join(".history");
    let db_path = history_root.join("events.sqlite3");
    let mut scope = RuntimeScope::fixture(home);
    if let Some(codex_home) = codex_home {
        scope = scope.with_codex_home(codex_home);
    }
    let ports = Ports {
        fs: Arc::new(RealFs::new()),
        clock: Arc::new(FakeClock::at(0)),
        ids: Arc::new(FakeIds::default()),
        leases: Arc::new(FileLease::new(home.join(".leases"))),
        history: Arc::new(SqliteHistoryOpener::new(db_path)),
        sink: Arc::new(RecordingSink::default()),
        spawner: None,
        discovery: None,
        tools: None,
        catalog: Arc::new(HarnessCatalog::builtin()),

        telemetry: std::sync::Arc::new(skill_studio_core::ports::NoopTelemetry),

        scan_memo: None,
    };
    Runtime::new(&scope, ports).unwrap()
}

/// `parking_a_skill_with_a_user_written_codex_disable_row_leaves_config_toml_byte_identical_or_names_the_rewritten_row`:
/// the user's own `[[skills.config]] enabled = false` row names the skill's
/// `SKILL.md`. Park moves the folder and nothing else: Codex's config is the
/// user's, so a rewrite of the row (the old carry) is the failure.
#[test]
fn parking_a_skill_with_a_user_written_codex_disable_row_leaves_config_toml_byte_identical_or_names_the_rewritten_row(
) {
    let home = unique_temp_dir("codex_park_leaves_config");
    let dir = home.join(UNIVERSAL_ROOT_RELATIVE).join("gamma");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        b"---\nname: gamma\ndescription: a parkable skill\n---\nBody.\n",
    )
    .unwrap();
    let config_path = home.join(".codex").join("config.toml");
    std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
    let original = format!(
        "# a user comment\nmodel = \"o3\"\n\n[[skills.config]]\npath = \"{}\"\nenabled = false\n",
        dir.join("SKILL.md").display()
    );
    std::fs::write(&config_path, &original).unwrap();
    let rt = runtime_for(&home, None);

    let inventory = ops::scan(&rt, &ctx(), &Default::default()).unwrap();
    let deployment_id = inventory
        .skills
        .iter()
        .find(|s| s.name.0 == "gamma")
        .unwrap()
        .deployments
        .iter()
        .find(|d| d.root.kind == RootKind::Universal)
        .unwrap()
        .id
        .clone();
    let outcome = ops::park(&rt, &ctx(), &ParkRequest { deployment_id }).unwrap();
    assert_eq!(
        outcome.parked_path,
        home.join(PARKED_ROOT_RELATIVE)
            .join("universal")
            .join("gamma")
    );
    assert_eq!(
        std::fs::read_to_string(&config_path).unwrap(),
        original,
        "park rewrote the user's config.toml"
    );

    let inventory = ops::scan(&rt, &ctx(), &Default::default()).unwrap();
    let parked_id = inventory
        .skills
        .iter()
        .find(|s| s.name.0 == "gamma")
        .unwrap()
        .deployments
        .iter()
        .find(|d| d.root.kind == RootKind::Parked)
        .unwrap()
        .id
        .clone();
    ops::unpark(
        &rt,
        &ctx(),
        &UnparkRequest {
            deployment_id: parked_id,
        },
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(&config_path).unwrap(),
        original,
        "unpark rewrote the user's config.toml"
    );

    std::fs::remove_dir_all(&home).ok();
}

/// `codex_honours_codex_home_for_config_and_rollouts_or_names_the_path_read_from_the_default`:
/// with `CODEX_HOME` pointed somewhere other than `<home>/.codex`, both the
/// disable-config reader and the rollout reader follow it - neither one
/// falls back to reading under the default path.
#[test]
fn codex_honours_codex_home_for_config_and_rollouts_or_names_the_path_read_from_the_default() {
    let _guard = CODEX_HOME_ENV_LOCK.lock().unwrap();
    let home = unique_temp_dir("codex_home_override");
    std::fs::create_dir_all(&home).unwrap();
    // Nested under `home` (as a real `CODEX_HOME` override normally is,
    // e.g. `~/.codex2`) rather than an unrelated directory: `confine`
    // requires every path it confines, including `codex_home` itself, to
    // have a parent already inside the scope, and `home` is that scope's
    // only unconditional root.
    let custom_codex_home = home.join("custom-codex-home");
    std::fs::create_dir_all(&custom_codex_home).unwrap();
    std::env::set_var("CODEX_HOME", &custom_codex_home);

    // Config: the disable rows are read from the override, not `<home>/.codex`.
    let skill_md = home
        .join(UNIVERSAL_ROOT_RELATIVE)
        .join("gamma")
        .join("SKILL.md");
    std::fs::write(
        custom_codex_home.join("config.toml"),
        format!(
            "[[skills.config]]\npath = \"{}\"\nenabled = false\n",
            skill_md.display()
        ),
    )
    .unwrap();
    let fs = RealFs::new();
    let codex_home = skill_studio_host::codex_home(&home);
    assert!(
        ops::codex_disabled_skill_md_paths(&fs, &codex_home)
            .contains(&ops::codex_path_form(&fs, &skill_md)),
        "the path read from the default: the disable row under CODEX_HOME was not read"
    );

    // Rollouts: the reader must find a session file under the override.
    let session_dir = custom_codex_home.join("sessions").join("2026/09/17");
    std::fs::create_dir_all(&session_dir).unwrap();
    std::fs::write(
        session_dir.join("a.jsonl"),
        "{\"type\":\"session_meta\",\"payload\":{\"id\":\"sess-a\",\"cwd\":\"/proj-a\"}}\n",
    )
    .unwrap();
    let mut index = SkillInvocationIndex::default();
    let report = index.refresh(&home, &DiscoverySources::default());
    assert!(
        report.bytes_read > 0,
        "the path read from the default: no bytes were read from CODEX_HOME/sessions"
    );

    std::env::remove_var("CODEX_HOME");
    std::fs::remove_dir_all(&home).ok();
    std::fs::remove_dir_all(&custom_codex_home).ok();
}

#[test]
fn a_listing_budget_can_be_saved_reset_and_protected_from_a_stale_edit() {
    use skill_studio_core::listing_setting_edit::save;
    use skill_studio_core::listing_visibility::{listing, ListingRequest};
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join(".codex/config.toml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "# keep this comment\nmodel = 'local-model'\n").unwrap();
    let rt = runtime_for(home.path(), Some(path.parent().unwrap()));
    let read_budget = || {
        listing(&rt, &ctx(), &ListingRequest::default())
            .unwrap()
            .listings
            .into_iter()
            .find(|r| r.harness.as_str() == "codex")
            .unwrap()
            .setting
            .unwrap()
    };
    let initial = read_budget().edit_revision.unwrap();
    save(
        &rt,
        &ctx(),
        None,
        "skills.max_context_tokens",
        Some("8000"),
        &initial,
    )
    .unwrap();
    let saved = read_budget();
    assert_eq!(saved.current.as_deref(), Some("8000"));
    assert!(save(
        &rt,
        &ctx(),
        None,
        "skills.max_context_tokens",
        Some("5000"),
        &initial
    )
    .is_err());
    save(
        &rt,
        &ctx(),
        None,
        "skills.max_context_tokens",
        None,
        &saved.edit_revision.unwrap(),
    )
    .unwrap();
    assert!(read_budget().current.is_none());
    assert!(std::fs::read_to_string(path)
        .unwrap()
        .contains("# keep this comment\nmodel = 'local-model'"));
}

#[test]
fn codex_preview_separates_its_ellipsis_from_the_original_description() {
    use skill_studio_core::listing_visibility::{listing, ListingRequest, ListingState};
    let home = tempfile::tempdir().unwrap();
    let folder = home.path().join(".agents/skills/check-preview");
    std::fs::create_dir_all(&folder).unwrap();
    let description = format!("{} original ending", "é".repeat(1030));
    std::fs::write(
        folder.join("SKILL.md"),
        format!("---\nname: check-preview\ndescription: {description}\n---\n\nReview changes.\n"),
    )
    .unwrap();
    let rt = runtime_for(home.path(), None);
    let report = listing(&rt, &ctx(), &ListingRequest::default()).unwrap();
    let codex = report
        .listings
        .iter()
        .find(|row| row.harness.as_str() == "codex")
        .unwrap();
    let entry = codex
        .entries
        .iter()
        .find(|e| e.name == "check-preview")
        .unwrap();
    assert_eq!(entry.state, ListingState::Truncated);
    let preview = entry.description_preview.as_ref().unwrap();
    let prefix = preview.shown.strip_suffix("...").unwrap();
    assert_eq!(preview.shown.chars().count(), 1024);
    assert_eq!(format!("{prefix}{}", preview.remainder), description);
}

#[test]
fn claude_settings_save_preserves_other_keys_and_activity_can_restore_the_original() {
    use skill_studio_core::dto::RestoreRequest;
    use skill_studio_core::listing_setting_edit::save;
    use skill_studio_core::listing_visibility::{listing, ListingRequest};
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join(".claude/settings.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let original = br#"{ "theme": "dark", "model": "sonnet" }"#;
    std::fs::write(&path, original).unwrap();
    let rt = runtime_for(home.path(), None);
    let revision = || {
        listing(&rt, &ctx(), &ListingRequest::default())
            .unwrap()
            .listings
            .into_iter()
            .find(|row| row.harness.as_str() == "claude-code")
            .unwrap()
            .setting
            .unwrap()
            .edit_revision
            .unwrap()
    };
    let first = revision();
    let id = save(
        &rt,
        &ctx(),
        None,
        "skillListingBudgetFraction",
        Some("0.025"),
        &first,
    )
    .unwrap()
    .unwrap();
    let after: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        after,
        serde_json::json!({"theme":"dark","model":"sonnet","skillListingBudgetFraction":0.025})
    );
    assert!(save(
        &rt,
        &ctx(),
        None,
        "skillListingBudgetFraction",
        None,
        &first
    )
    .is_err());
    ops::restore_event(
        &rt,
        &ctx(),
        &RestoreRequest {
            event_id: id,
            force: false,
        },
    )
    .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), original);
}
