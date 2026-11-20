// Integration test binaries aren't covered by the lib crate's
// `cfg_attr(test, allow(...))`: this file compiles as its own crate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Property tests for the deployment and owner ids. Ids select which copy
//! Park, Remove and Update act on, and the desktop and the core each build
//! them. Both builders claim byte-for-byte parity; these tests check it, and
//! check that parsing an id gives back what built it, for project paths and
//! entries with `/`, `%`, a literal `%2F`, spaces and unicode.

use std::path::{Path, PathBuf};

use proptest::prelude::*;
use skill_studio_core::identity::SkillDestination as CoreDestination;
use skill_studio_core::lock_file::SkillLockFile;
use skill_studio_core::testing::wire_ids;
use skill_studio_lib::skills::skill_deployment::{
    deployment_id, parse_deployment_id, SkillDestination,
};
use skill_studio_lib::skills::skill_dto::InstallScope;
use skill_studio_lib::skills::skill_ownership::{owner_id_for, parse_owner_id, OwnershipLedgers};

/// An agentskills.io skill name: lowercase words joined by single hyphens.
fn skill_name() -> impl Strategy<Value = String> {
    "[a-z0-9]{1,8}(-[a-z0-9]{1,8}){0,3}"
}

/// One path segment, biased toward the characters the id encoding escapes.
fn segment() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z0-9 ._é漢-]{1,8}",
        Just("%2F".to_string()),
        Just("%25".to_string()),
        Just("%".to_string()),
        Just("a%2Fb%25c".to_string()),
        Just("-".to_string()),
    ]
}

/// An absolute path. Project paths are always absolute in the app, which
/// also keeps them apart from the `-` that marks "no project".
fn absolute_path() -> impl Strategy<Value = String> {
    prop::collection::vec(segment(), 1..5).prop_map(|parts| format!("/{}", parts.join("/")))
}

fn destination() -> impl Strategy<Value = (SkillDestination, CoreDestination)> {
    prop_oneof![
        Just((SkillDestination::Universal, CoreDestination::Universal)),
        Just((SkillDestination::PerHarness, CoreDestination::PerHarness)),
    ]
}

fn slot() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("universal".to_string()),
        Just("claude-code".to_string()),
        Just("codex".to_string()),
        Just("opencode".to_string()),
        Just("pi".to_string()),
    ]
}

fn ledger(scope: InstallScope, project_path: Option<&str>) -> OwnershipLedgers {
    OwnershipLedgers {
        agents_dir: PathBuf::from("/home/.agents"),
        scope,
        project_path: project_path.map(PathBuf::from),
        lock: SkillLockFile {
            version: 3,
            skills: Default::default(),
        },
        dotagents: Vec::new(),
        lock_failed: false,
        ledger_failed: false,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn a_deployment_id_parses_back_to_its_inputs_and_the_core_builds_the_same_bytes(
        name in skill_name(),
        scope in prop_oneof![Just("global"), Just("project")],
        (destination, core_destination) in destination(),
        slot in slot(),
        project in prop::option::of(absolute_path()),
        entry in absolute_path(),
    ) {
        let id = deployment_id(
            &name,
            scope,
            destination,
            &slot,
            project.as_deref(),
            Path::new(&entry),
        );
        let core = wire_ids::deployment_id(
            &name,
            scope,
            core_destination,
            &slot,
            project.as_deref(),
            Path::new(&entry),
        );
        prop_assert_eq!(&id, core.as_str());

        let parsed = parse_deployment_id(&id).expect("a built id parses");
        prop_assert_eq!(parsed.scope, scope);
        prop_assert_eq!(parsed.slot, slot);
        prop_assert_eq!(parsed.destination, destination);
        prop_assert_eq!(&parsed.name, &name);
        prop_assert_eq!(parsed.project_path, project);
        prop_assert_eq!(parsed.lexical_path, PathBuf::from(&entry));
        prop_assert_eq!(core.skill_name().map(|n| n.0), Some(name));
    }

    #[test]
    fn an_owner_id_parses_back_to_its_ledger_and_the_core_builds_the_same_bytes(
        name in skill_name(),
        project in prop::option::of(absolute_path()),
        global in any::<bool>(),
    ) {
        let (scope, scope_label) = if global {
            (InstallScope::Global, "global")
        } else {
            (InstallScope::Project, "project")
        };
        let id = owner_id_for(&ledger(scope, project.as_deref()), &name);
        let core = wire_ids::owner_id(scope_label, project.as_deref(), &name);
        prop_assert_eq!(&id, core.as_str());

        let parsed = parse_owner_id(&id).expect("a built owner id parses");
        prop_assert_eq!(parsed.scope, scope);
        prop_assert_eq!(&parsed.name, &name);
        let expected_project = if global { None } else { project };
        prop_assert_eq!(parsed.project_path, expected_project);
    }
}
