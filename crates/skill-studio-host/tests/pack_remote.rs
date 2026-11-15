#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "../../skill-studio-core/tests/support/git_pack.rs"]
mod git_pack;

use std::path::Path;
use std::sync::Arc;

use git_pack::GitPack;
use skill_studio_core::harness::HarnessCatalog;
use skill_studio_core::identity::CorrelationId;
use skill_studio_core::pack::{Pack, PackListRequest, RemoteState};
use skill_studio_core::ports::{OpContext, Runtime};
use skill_studio_core::scope::ProjectSelection;
use skill_studio_core::tree_hash::tree_hash_ignoring_junk;
use skill_studio_core::{ops, RuntimeScope};
use skill_studio_host::{GitPackRemote, RealFs, RealProcessSpawner};

fn install(home: &Path, pack: &GitPack, names: &[&str]) {
    let mut rows = Vec::new();
    for name in names {
        let dir = home.join(".agents/skills").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), git_pack::skill_body(name, "v1")).unwrap();
        let hash = tree_hash_ignoring_junk(&RealFs::new(), &dir).unwrap();
        rows.push(format!(
            r#""{name}":{{"source":"owner/pack","sourceType":"git","sourceUrl":"{url}","skillPath":"skills/{name}/SKILL.md","skillFolderHash":"{hash}","installedAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z"}}"#,
            url = pack.url
        ));
    }
    std::fs::write(
        home.join(".agents/.skill-lock.json"),
        format!(r#"{{"version":3,"skills":{{{}}}}}"#, rows.join(",")),
    )
    .unwrap();
}

fn list(home: &Path, remote: bool) -> Pack {
    let mut scope = RuntimeScope::live(home.to_path_buf(), home.join("history"));
    scope.projects = ProjectSelection::Explicit { paths: vec![] };
    let mut ports =
        skill_studio_host::default_ports(home.join("leases"), Arc::new(HarnessCatalog::builtin()));
    let spawner = Arc::new(
        RealProcessSpawner::for_home(home, Vec::new()).with_isolated_home(home.to_path_buf()),
    );
    ports.spawner = Some(spawner.clone());
    let rt = Runtime::new(&scope, ports).unwrap();
    let git = GitPackRemote::new(spawner);
    let report = ops::pack_list(
        &rt,
        &OpContext::uncancellable(CorrelationId("pack-remote".into())),
        &PackListRequest {
            project_path: None,
            remote,
        },
        remote.then_some(&git as _),
    )
    .unwrap();
    report
        .packs
        .into_iter()
        .find(|p| p.source == "owner/pack")
        .unwrap()
}

#[test]
fn pack_list_remote_finds_a_changed_and_a_new_upstream_skill_or_hides_updates() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().canonicalize().unwrap().join("home");
    let pack = GitPack::new(&temp.path().join("git"));
    install(&home, &pack, &["a", "b", "c"]);
    pack.skill("b", "v2");
    pack.skill("d", "v2");
    pack.publish("v2");

    let checked = list(&home, true);

    assert!(
        matches!(checked.remote, RemoteState::Checked { .. }),
        "{:?}",
        checked.remote
    );
    let behind: Vec<_> = checked
        .skills
        .iter()
        .filter(|s| s.behind == Some(true))
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(behind, ["b"]);
    assert_eq!(checked.new_upstream, ["d"]);
    assert!(
        checked.summary.contains("1 behind upstream"),
        "{}",
        checked.summary
    );
}

#[test]
fn pack_list_offline_leaves_upstream_unchecked_or_touches_the_network() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().canonicalize().unwrap().join("home");
    let pack = GitPack::new(&temp.path().join("git"));
    install(&home, &pack, &["a"]);

    let offline = list(&home, false);

    assert_eq!(offline.remote, RemoteState::NotChecked);
    assert_eq!(offline.skills[0].behind, None);
    assert!(offline.new_upstream.is_empty());
}

#[test]
fn pack_list_remote_reports_an_unreachable_source_as_failed_or_drops_the_pack() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().canonicalize().unwrap().join("home");
    let pack = GitPack::new(&temp.path().join("git"));
    install(&home, &pack, &["a"]);
    let lock = home.join(".agents/.skill-lock.json");
    let text = std::fs::read_to_string(&lock).unwrap();
    std::fs::write(
        &lock,
        text.replace(&pack.url, "http://127.0.0.1:9/owner/gone.git"),
    )
    .unwrap();

    let failed = list(&home, true);

    assert!(
        matches!(failed.remote, RemoteState::Failed { .. }),
        "{:?}",
        failed.remote
    );
    assert_eq!(failed.skills.len(), 1);
}
