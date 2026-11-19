// Integration test binaries aren't covered by the lib crate's
// `cfg_attr(test, allow(...))`: this file compiles as its own crate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Old-vs-new migration test for `skill-studio.json` `copies` rows. Today
//! `install` and `split` write `"destination": "per_harness"`; older
//! registries and the desktop wrote `"per-harness"`. Both ownership
//! (`ops::scan` classifying the copy as `Copy`) and the split-copy update
//! must keep reading the old spelling, or every old copy loses its owner
//! and its Update.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use skill_studio_core::dto::{InstallFile, ScanRequest, SplitRequest};
use skill_studio_core::harness::HarnessCatalog;
use skill_studio_core::identity::{AgentId, LifecycleOwnerKind, RootKind, RootScope, SkillName};
use skill_studio_core::ops;
use skill_studio_core::ports::{Ports, Runtime};
use skill_studio_core::scope::RuntimeScope;
use skill_studio_core::testing::golden::{ctx, unique_temp_dir};
use skill_studio_core::testing::{FakeClock, RecordingSink};
use skill_studio_core::SplitCopiesUpdate;
use skill_studio_host::{FileLease, RealFs, SqliteHistoryOpener, UlidIds};

const COPY_DIRS: [&str; 2] = [".claude/skills", ".codex/skills"];

fn skill_md(revision: &str) -> Vec<u8> {
    format!("---\nname: gamma\ndescription: a skill to split\n---\nBody at {revision}.\n")
        .into_bytes()
}

fn runtime(home: &Path) -> Runtime {
    let ports = Ports {
        fs: Arc::new(RealFs::new()),
        clock: Arc::new(FakeClock::at(0)),
        ids: Arc::new(UlidIds::new()),
        leases: Arc::new(FileLease::new(home.join(".leases"))),
        history: Arc::new(SqliteHistoryOpener::new(
            home.join(".history").join("events.sqlite3"),
        )),
        sink: Arc::new(RecordingSink::default()),
        spawner: None,
        discovery: None,
        tools: None,
        catalog: Arc::new(HarnessCatalog::builtin()),
        telemetry: Arc::new(skill_studio_core::ports::NoopTelemetry),
    };
    Runtime::new(&RuntimeScope::fixture(home), ports).unwrap()
}

/// A home whose Universal skill `gamma` was split into Claude Code and Codex
/// copies, with the registry rows then rewritten to the old spelling.
fn home_with_old_spelling_rows() -> PathBuf {
    let home = unique_temp_dir("legacy_registry_per_harness");
    let dir = home.join(".agents/skills/gamma");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("SKILL.md"), skill_md("v1")).unwrap();
    std::fs::write(
        home.join(".agents/.skill-lock.json"),
        serde_json::json!({
            "version": 3,
            "skills": {"gamma": {
                "source": "acme/skills", "sourceType": "github",
                "sourceUrl": "https://github.com/acme/skills",
                "skillFolderHash": "tree-v1",
                "installedAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-02T00:00:00Z",
            }},
        })
        .to_string(),
    )
    .unwrap();
    let rt = runtime(&home);
    let id = ops::scan(&rt, &ctx(), &ScanRequest::default())
        .unwrap()
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
    ops::split(
        &rt,
        &ctx(),
        &SplitRequest {
            deployment_id: id,
            harnesses: ["claude-code", "codex"]
                .iter()
                .map(|h| AgentId::parse(h).unwrap())
                .collect(),
        },
    )
    .unwrap();

    let registry = home.join(".agents/skill-studio.json");
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&registry).unwrap()).unwrap();
    let rows = doc["copies"].as_object_mut().unwrap();
    assert_eq!(rows.len(), COPY_DIRS.len());
    for row in rows.values_mut() {
        assert_eq!(row["destination"], "per_harness", "today's spelling");
        row["destination"] = serde_json::json!("per-harness");
    }
    std::fs::write(&registry, doc.to_string()).unwrap();
    home
}

#[test]
fn copies_recorded_with_the_old_per_harness_spelling_keep_their_owner_and_their_update() {
    let home = home_with_old_spelling_rows();
    let rt = runtime(&home);

    let inventory = ops::scan(&rt, &ctx(), &ScanRequest::default()).unwrap();
    let gamma = inventory
        .skills
        .iter()
        .find(|s| s.name.0 == "gamma")
        .unwrap();
    for dir in COPY_DIRS {
        let path = home.join(dir).join("gamma");
        let deployment = gamma
            .deployments
            .iter()
            .find(|d| d.path == path)
            .unwrap_or_else(|| panic!("no deployment at {}", path.display()));
        assert_eq!(
            deployment.owner_kind,
            LifecycleOwnerKind::Copy,
            "{dir}: an old-spelling row must still own its copy"
        );
    }

    let outcome = ops::update_split_copies(
        &rt,
        &ctx(),
        &SplitCopiesUpdate {
            skill: SkillName("gamma".to_string()),
            scope: RootScope::Global,
            files: vec![InstallFile {
                relative_path: PathBuf::from("SKILL.md"),
                contents: skill_md("v2"),
                mode: None,
            }],
            lock_folder_hash: Some("tree-v2".to_string()),
        },
    )
    .unwrap();
    assert_eq!(outcome.updated.len(), COPY_DIRS.len(), "{outcome:?}");
    for dir in COPY_DIRS {
        let text = std::fs::read_to_string(home.join(dir).join("gamma/SKILL.md")).unwrap();
        assert!(text.contains("Body at v2"), "{dir} was not updated");
    }
    std::fs::remove_dir_all(&home).ok();
}
