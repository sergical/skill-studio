#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use skill_studio_core::dto::{Inventory, ScanRequest, ScanRoot};
use skill_studio_core::harness::HarnessCatalog;
use skill_studio_core::ops;
use skill_studio_core::ports::{Ports, Runtime};
use skill_studio_core::testing::golden::{ctx, scope_for};
use skill_studio_core::testing::{FakeClock, FakeIds, FakeLease, FixtureBuilder, NoHistory};

fn scan(request: &ScanRequest) -> Inventory {
    let mut fixture = FixtureBuilder::new();
    for (root, name) in [
        (".agents", "tracked"),
        (".agents", "hand-made"),
        (".agents", "dot"),
        (".codex", "agent-only"),
    ] {
        let folder = format!("/home/{root}/skills/{name}");
        fixture = fixture.dir(&folder).file(
            &format!("{folder}/SKILL.md"),
            format!("---\nname: {name}\ndescription: A test skill.\n---\nBody.\n").as_bytes(),
        );
    }
    fixture = fixture
        .file(
            "/home/.claude/plugins/cache/vendor-1/plugin-1/1.0.0/.claude-plugin/plugin.json",
            br#"{"name":"plugin-1","version":"1.0.0"}"#,
        )
        .file(
            "/home/.claude/plugins/cache/vendor-1/plugin-1/1.0.0/skills/plugged/SKILL.md",
            b"---\nname: plugged\ndescription: A plugin skill.\n---\nBody.\n",
        )
        .alias("/home/.agents/skills/linked-gone", "/home/elsewhere/gone");
    fixture = fixture.file("/home/.agents/.skill-lock.json", br#"{"version":3,"skills":{"linked-gone":{"source":"owner/linked","sourceType":"github","sourceUrl":"https://github.com/owner/linked","skillFolderHash":"hash","installedAt":"2024-01-01T00:00:00Z","updatedAt":"2024-01-01T00:00:00Z"},"tracked":{"source":"owner/repo","sourceType":"github","sourceUrl":"https://github.com/owner/repo","skillFolderHash":"hash","installedAt":"2024-01-01T00:00:00Z","updatedAt":"2024-01-01T00:00:00Z"},"missing":{"source":"owner/gone","sourceType":"github","sourceUrl":"https://github.com/owner/gone","skillFolderHash":"hash","installedAt":"2024-01-01T00:00:00Z","updatedAt":"2024-01-01T00:00:00Z"}}}"#)
        .file("/home/.agents/agents.lock", b"[skills.dot]\nsource = \"git:https://github.com/dot/repo.git\"\n[skills.dot-missing]\nsource = \"dot/gone\"\n");
    let ports = Ports {
        fs: Arc::new(fixture.build_fs()),
        clock: Arc::new(FakeClock::at(0)),
        ids: Arc::new(FakeIds::default()),
        leases: Arc::new(FakeLease::default()),
        history: Arc::new(NoHistory),
        sink: Arc::new(skill_studio_core::testing::RecordingSink::default()),
        spawner: None,
        discovery: None,
        tools: None,
        catalog: Arc::new(HarnessCatalog::builtin()),
        telemetry: Arc::new(skill_studio_core::ports::NoopTelemetry),
    };
    let runtime = Runtime::new(&scope_for("scan-source", Path::new("/home")), ports).unwrap();
    ops::scan(&runtime, &ctx(), request).unwrap()
}

#[test]
fn scan_reports_the_lock_repo_or_loses_the_installed_source() {
    let result = scan(&ScanRequest::default());
    let skill = result
        .skills
        .iter()
        .find(|s| s.name.0 == "tracked")
        .unwrap();
    assert_eq!(skill.source, "owner/repo");
}

#[test]
fn scan_reports_manual_for_a_hand_made_skill_or_invents_an_upstream() {
    let result = scan(&ScanRequest::default());
    let skill = result
        .skills
        .iter()
        .find(|s| s.name.0 == "hand-made")
        .unwrap();
    assert_eq!(skill.source, "manual");
}

#[test]
fn scan_reports_the_dotagents_repo_or_loses_dotagents_provenance() {
    let result = scan(&ScanRequest::default());
    let skill = result.skills.iter().find(|s| s.name.0 == "dot").unwrap();
    assert_eq!(skill.source, "dot/repo");
}

#[test]
fn scan_source_filter_returns_only_the_repo_or_includes_unrelated_skills() {
    let result = scan(&ScanRequest {
        source: Some("owner/repo".into()),
        ..Default::default()
    });
    assert_eq!(result.skills.len(), 1);
    assert_eq!(result.skills[0].name.0, "tracked");
    assert!(result.lock_only.is_empty());
}

#[test]
fn scan_library_filter_excludes_agent_folders_or_counts_agent_only_skills() {
    let result = scan(&ScanRequest {
        root: Some(ScanRoot::Library),
        ..Default::default()
    });
    assert_eq!(result.skills.len(), 4);
    assert!(result
        .skills
        .iter()
        .flat_map(|s| &s.deployments)
        .all(|d| d.path.parent() == Some(Path::new("/home/.agents/skills"))));
}

#[test]
fn scan_reports_missing_lock_folders_or_silently_loses_stale_entries() {
    let result = scan(&ScanRequest::default());
    let names: Vec<_> = result
        .lock_only
        .iter()
        .map(|entry| (entry.name.0.as_str(), entry.source.as_str()))
        .collect();
    assert_eq!(
        names,
        [("dot-missing", "dot/gone"), ("missing", "owner/gone")]
    );
}

#[test]
fn scan_combines_manual_and_library_filters_or_returns_an_agent_only_skill() {
    let result = scan(&ScanRequest {
        source: Some("manual".into()),
        root: Some(ScanRoot::Library),
        ..Default::default()
    });
    assert_eq!(result.skills.len(), 1);
    assert_eq!(result.skills[0].name.0, "hand-made");
}

#[test]
fn scan_reports_the_plugin_for_a_plugin_skill_or_calls_it_manual() {
    let result = scan(&ScanRequest::default());
    let skill = result
        .skills
        .iter()
        .find(|s| s.name.0 == "plugged")
        .unwrap();
    assert_eq!(skill.source, "plugin-1@vendor-1");

    let manual = scan(&ScanRequest {
        source: Some("manual".into()),
        ..Default::default()
    });
    assert!(manual.skills.iter().all(|s| s.name.0 != "plugged"));
}

#[test]
fn scan_lists_a_locked_broken_link_as_a_skill_or_loses_it_from_both_lists() {
    let result = scan(&ScanRequest::default());
    let skill = result
        .skills
        .iter()
        .find(|s| s.name.0 == "linked-gone")
        .unwrap();
    assert_eq!(skill.source, "owner/linked");
    assert!(result.lock_only.iter().all(|e| e.name.0 != "linked-gone"));
}
