#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use skill_studio_core::harness::HarnessCatalog;
use skill_studio_core::pack::{
    skills_cli_folder_hash, Pack, PackKind, PackListRequest, PackRemote, PackReport,
    PackShowRequest, RemoteError, RemotePack, RemoteState, SKILLS_CLI_STATE_ROOT,
};
use skill_studio_core::pack_action::PackRemoveRequest;
use skill_studio_core::ports::{CancelToken, Ports, Runtime};
use skill_studio_core::scope::ProjectSelection;
use skill_studio_core::testing::golden::{ctx, scope_for};
use skill_studio_core::testing::{
    FakeClock, FakeIds, FakeLease, FixtureBuilder, NoHistory, PanicOnSpawn,
};
use skill_studio_core::{ops, ErrorCode};

const SKILL: &[u8] = b"---\nname: s\ndescription: A test skill.\n---\nBody.\n";
const WRONG_TREE: &str = "0000000000000000000000000000000000000000";

struct PanicRemote;

impl PackRemote for PanicRemote {
    fn fetch(
        &self,
        _: &str,
        _: Option<&str>,
        _: &dyn CancelToken,
    ) -> Result<RemotePack, RemoteError> {
        panic!("pack_list without remote must not reach the network");
    }
}

fn skill(fixture: FixtureBuilder, folder: &str) -> FixtureBuilder {
    fixture
        .dir(folder)
        .file(&format!("{folder}/SKILL.md"), SKILL)
}

fn clean_hash() -> String {
    let fs = skill(FixtureBuilder::new(), "/x").build_fs();
    skills_cli_folder_hash(&fs, Path::new("/x")).unwrap()
}

fn runtime() -> Runtime {
    let mut fixture = FixtureBuilder::new();
    for folder in [
        "/home/.agents/skills/tracked",
        "/home/proj/.agents/skills/pinned",
        "/home/proj/.agents/skills/from-npm",
        "/home/proj/.agents/skills/over-ssh",
        "/home/proj/.agents/skills/dotty",
        "/home/proj/.agents/skills/adopted",
        "/home/proj/.agents/skills/local-one",
        "/home/proj/.agents/plugins/tools/skills/hammer",
        "/home/proj/.agents/plugins/local-tools/skills/wrench",
        "/home/.claude/plugins/cache/vendor-1/plugin-1/1.0.0/skills/plugged",
    ] {
        fixture = skill(fixture, folder);
    }
    let project_lock = format!(
        r#"{{"version":1,"skills":{{
            "pinned":{{"source":"owner/pinned","sourceType":"github","ref":"v1.2","skillPath":"skills/pinned/SKILL.md","computedHash":"{clean}"}},
            "from-npm":{{"source":"node_modules/@acme/skills","sourceType":"node_modules","via":"@acme/skills","computedHash":"{clean}"}},
            "over-ssh":{{"source":"git@github.com:Owner/repo.git","sourceType":"git","sourceUrl":"git@github.com:Owner/repo.git","skillPath":"skills/over-ssh/SKILL.md","computedHash":"{clean}"}},
            "local-one":{{"source":"vendor/local-one","sourceType":"local","computedHash":"{clean}"}}
        }}}}"#,
        clean = clean_hash()
    );
    let fixture = fixture
        .file(
            "/home/.local/state/skills/.skill-lock.json",
            format!(r#"{{"version":3,"skills":{{"tracked":{{"source":"owner/repo","sourceType":"github","sourceUrl":"https://github.com/owner/repo.git","skillPath":"skills/tracked/SKILL.md","skillFolderHash":"{WRONG_TREE}","installedAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z"}}}}}}"#).as_bytes(),
        )
        .file(
            "/home/.agents/.skill-lock.json",
            br#"{"version":3,"skills":{"tracked":{"source":"stale/legacy","sourceType":"github","sourceUrl":"","skillFolderHash":"x","installedAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z"}}}"#,
        )
        .file("/home/proj/skills-lock.json", project_lock.as_bytes())
        .file(
            "/home/proj/agents.toml",
            b"[[skills]]\nname = \"*\"\nsource = \"dot/pack\"\nref = \"v2\"\n\n[[plugins]]\nname = \"tools\"\nsource = \"dot/tools\"\ntargets = [\"claude-code\"]\n\n[[plugins]]\nname = \"local-tools\"\nsource = \"path:../local-tools\"\n",
        )
        .file(
            "/home/proj/agents.lock",
            b"[skills.dotty]\nsource = \"dot/pack\"\nresolved_url = \"https://gitlab.example.com/dot/pack.git\"\nresolved_path = \"skills/dotty\"\n\n[skills.adopted]\nsource = \"path:skills/adopted\"\n",
        )
        .file(
            "/home/.claude/plugins/cache/vendor-1/plugin-1/1.0.0/.claude-plugin/plugin.json",
            br#"{"name":"plugin-1","version":"1.0.0"}"#,
        );
    let ports = Ports {
        fs: Arc::new(fixture.build_fs()),
        clock: Arc::new(FakeClock::at(0)),
        ids: Arc::new(FakeIds::default()),
        leases: Arc::new(FakeLease::default()),
        history: Arc::new(NoHistory),
        sink: Arc::new(skill_studio_core::testing::RecordingSink::default()),
        spawner: Some(Arc::new(PanicOnSpawn)),
        discovery: None,
        tools: None,
        catalog: Arc::new(HarnessCatalog::builtin()),
        telemetry: Arc::new(skill_studio_core::ports::NoopTelemetry),
        scan_memo: None,
    };
    let mut scope = scope_for("pack-list", Path::new("/home"));
    scope.projects = ProjectSelection::Explicit {
        paths: vec![PathBuf::from("/home/proj")],
    };
    scope.agent_home_roots.insert(
        SKILLS_CLI_STATE_ROOT.into(),
        PathBuf::from("/home/.local/state/skills"),
    );
    Runtime::new(&scope, ports).unwrap()
}

fn list(project_path: Option<&str>) -> PackReport {
    ops::pack_list(
        &runtime(),
        &ctx(),
        &PackListRequest {
            project_path: project_path.map(PathBuf::from),
            remote: false,
        },
        Some(&PanicRemote),
    )
    .unwrap()
}

fn pack<'a>(report: &'a PackReport, source: &str) -> &'a Pack {
    report
        .packs
        .iter()
        .find(|p| p.source == source)
        .unwrap_or_else(|| panic!("no pack {source} in {:#?}", report.packs))
}

#[test]
fn pack_list_reads_the_xdg_global_lock_and_flags_an_edit_or_shows_a_stale_lock() {
    let report = list(None);
    let tracked = pack(&report, "owner/repo");
    assert_eq!(tracked.kind, PackKind::SkillsSh);
    assert_eq!(tracked.skills[0].edited, Some(true));
    assert_eq!(tracked.edited_count, 1);
    assert!(report.packs.iter().all(|p| p.source != "stale/legacy"));
}

#[test]
fn pack_list_reads_ref_and_via_from_the_project_lock_or_loses_the_pin_and_package() {
    let report = list(None);
    let pinned = pack(&report, "owner/pinned");
    assert_eq!(pinned.ref_pin.as_deref(), Some("v1.2"));
    assert_eq!(pinned.skills[0].edited, Some(false));
    let npm = pack(&report, "@acme/skills");
    assert_eq!(npm.kind, PackKind::Npm);
    assert_eq!(npm.git_url, None);
    assert_eq!(npm.skills[0].name, "from-npm");
}

#[test]
fn pack_list_reads_dotagents_wildcard_and_plugins_or_misses_dotagents_packs() {
    let report = list(None);
    let dotty = pack(&report, "dot/pack");
    assert_eq!(dotty.kind, PackKind::Dotagents);
    assert_eq!(dotty.ref_pin.as_deref(), Some("v2"));
    assert_eq!(
        dotty.git_url.as_deref(),
        Some("https://gitlab.example.com/dot/pack.git")
    );
    let tools = pack(&report, "dot/tools");
    assert_eq!(tools.skills[0].name, "hammer");
    assert_eq!(tools.skills[0].agents, ["claude-code"]);
}

#[test]
fn pack_list_shows_a_plugin_cache_with_the_agent_update_command_or_hides_plugins() {
    let report = list(None);
    let plugin = report
        .packs
        .iter()
        .find(|p| p.kind == PackKind::Plugin)
        .unwrap();
    assert_eq!(plugin.ref_pin.as_deref(), Some("1.0.0"));
    assert_eq!(plugin.skills[0].name, "plugged");
    assert!(
        plugin.update_commands[0].starts_with("claude plugin update "),
        "{:?}",
        plugin.update_commands
    );
}

#[test]
fn pack_list_without_remote_runs_no_process_and_no_network_or_panics() {
    let report = list(None);
    assert!(report
        .packs
        .iter()
        .all(|p| p.remote == RemoteState::NotChecked && p.new_upstream.is_empty()));
    assert!(
        report.summary.contains("upstream not checked"),
        "{}",
        report.summary
    );
}

#[test]
fn pack_list_with_a_project_path_leaves_out_global_packs_or_mixes_scopes() {
    let report = list(Some("/home/proj"));
    assert!(report.packs.iter().all(|p| p.source != "owner/repo"));
    pack(&report, "owner/pinned");
}

#[test]
fn pack_show_finds_one_source_or_names_the_missing_pack() {
    let rt = runtime();
    let shown = ops::pack_show(
        &rt,
        &ctx(),
        &PackShowRequest {
            source: "owner/pinned".into(),
            ..PackShowRequest::default()
        },
        None,
    )
    .unwrap();
    assert_eq!(shown.packs.len(), 1);
    let missing = ops::pack_show(
        &rt,
        &ctx(),
        &PackShowRequest {
            source: "nobody/none".into(),
            ..PackShowRequest::default()
        },
        None,
    )
    .unwrap_err();
    assert_eq!(missing.code, ErrorCode::AmbiguousTarget);
    assert!(
        missing.message.contains("nobody/none"),
        "{}",
        missing.message
    );
}

#[test]
fn pack_list_joins_two_spellings_of_one_repo_into_one_pack_or_checks_it_twice() {
    let report = list(None);
    let repo = pack(&report, "owner/repo");
    let names: Vec<_> = repo.skills.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["tracked", "over-ssh"], "{repo:#?}");
    assert_eq!(repo.scopes.len(), 2, "{repo:#?}");
    assert!(report.packs.iter().all(|p| !p.source.starts_with("git@")));

    let shown = ops::pack_show(
        &runtime(),
        &ctx(),
        &PackShowRequest {
            source: "https://github.com/OWNER/repo.git/".into(),
            ..PackShowRequest::default()
        },
        None,
    )
    .unwrap();
    assert_eq!(shown.packs.len(), 1);
    assert_eq!(shown.packs[0].skills.len(), 2);
}

/// A dotagents `path:` skill or plugin and a skills.sh `local` install come
/// from a folder on this machine and have no upstream: they are not packs, so the list
/// leaves them out and Remove finds no pack to delete them through.
#[test]
fn pack_list_leaves_out_local_folder_sources_or_offers_update_and_remove_for_them() {
    let report = list(None);
    let sources: Vec<_> = report.packs.iter().map(|p| p.source.as_str()).collect();
    assert!(
        report.packs.iter().all(|p| p
            .skills
            .iter()
            .all(|s| !["adopted", "local-one", "wrench"].contains(&s.name.as_str()))),
        "{sources:?}"
    );
    for source in [
        "path:skills/adopted",
        "vendor/local-one",
        "path:../local-tools",
    ] {
        let err = ops::pack_remove(
            &runtime(),
            &ctx(),
            &PackRemoveRequest {
                source: source.into(),
                project_path: None,
                dry_run: true,
            },
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::AmbiguousTarget, "{source}: {err:?}");
    }
}
