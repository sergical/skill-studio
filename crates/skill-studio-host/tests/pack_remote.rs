#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "../../skill-studio-core/tests/support/git_pack.rs"]
mod git_pack;

use std::path::Path;
use std::sync::Arc;

use git_pack::GitPack;
use skill_studio_core::harness::HarnessCatalog;
use skill_studio_core::identity::CorrelationId;
use skill_studio_core::pack::{skills_cli_folder_hash, Pack, PackListRequest, RemoteState};
use skill_studio_core::ports::{OpContext, Runtime};
use skill_studio_core::scope::ProjectSelection;
use skill_studio_core::{ops, RuntimeScope};
use skill_studio_host::{GitPackRemote, RealFs, RealProcessSpawner};

fn install(home: &Path, pack: &GitPack, names: &[&str]) {
    let mut rows = Vec::new();
    for name in names {
        let dir = home.join(".agents/skills").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), git_pack::skill_body(name, "v1")).unwrap();
        let hash = git_pack::run(
            std::process::Command::new("git")
                .current_dir(&pack.work)
                .args(["rev-parse", &format!("HEAD:skills/{name}")]),
        );
        let hash = String::from_utf8(hash).unwrap().trim().to_string();
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

#[test]
fn pack_list_remote_counts_committed_junk_like_github_or_reports_a_false_update() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().canonicalize().unwrap().join("home");
    let pack = GitPack::new(&temp.path().join("git"));
    let cache = pack.work.join("skills/a/__pycache__");
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(cache.join("x.pyc"), b"bytecode").unwrap();
    pack.publish("v1-junk");
    install(&home, &pack, &["a"]);

    let checked = list(&home, true);

    assert!(
        matches!(checked.remote, RemoteState::Checked { .. }),
        "{:?}",
        checked.remote
    );
    assert_eq!(checked.skills[0].behind, Some(false), "{checked:#?}");
}

#[test]
fn pack_list_remote_names_a_new_skill_by_its_frontmatter_or_offers_a_name_add_rejects() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().canonicalize().unwrap().join("home");
    let pack = GitPack::new(&temp.path().join("git"));
    install(&home, &pack, &["a"]);
    let dir = pack.work.join("skills/folder-name");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        "---\nname: actual-skill\ndescription: Named apart from its folder.\nallowed-tools: [Read, Bash]\n---\nv2\n",
    )
    .unwrap();
    pack.publish("v2");

    let checked = list(&home, true);

    assert!(
        checked.new_upstream.contains(&"actual-skill".to_string()),
        "{checked:#?}"
    );
    assert!(
        !checked.new_upstream.contains(&"folder-name".to_string()),
        "{checked:#?}"
    );
}

#[test]
fn folder_hash_skips_a_link_loop_like_the_skills_cli_or_drops_the_skill() {
    let temp = tempfile::tempdir().unwrap();
    let skill = temp.path().canonicalize().unwrap().join("s");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(skill.join("SKILL.md"), git_pack::skill_body("s", "v1")).unwrap();
    let plain = skills_cli_folder_hash(&RealFs::new(), &skill).unwrap();

    std::os::unix::fs::symlink(".", skill.join("loop")).unwrap();

    assert_eq!(
        skills_cli_folder_hash(&RealFs::new(), &skill).unwrap(),
        plain
    );
}

#[test]
fn folder_hash_ends_a_loop_between_two_sibling_links_or_drops_the_skill() {
    let temp = tempfile::tempdir().unwrap();
    let skill = temp.path().canonicalize().unwrap().join("s");
    for dir in ["a", "b"] {
        std::fs::create_dir_all(skill.join(dir)).unwrap();
        std::fs::write(skill.join(dir).join("note.md"), dir).unwrap();
    }
    std::fs::write(skill.join("SKILL.md"), git_pack::skill_body("s", "v1")).unwrap();
    std::os::unix::fs::symlink("../b", skill.join("a/next")).unwrap();
    std::os::unix::fs::symlink("../a", skill.join("b/next")).unwrap();

    assert!(skills_cli_folder_hash(&RealFs::new(), &skill).is_ok());
}

/// A spawner that cannot find its program, as on a Mac without git.
struct NoProgram;

impl skill_studio_core::ports::ProcessSpawner for NoProgram {
    fn run(
        &self,
        spec: &skill_studio_core::ports::ProcessSpec,
        _: &dyn skill_studio_core::ports::CancelToken,
    ) -> Result<skill_studio_core::ports::ProcessOutput, skill_studio_core::CoreError> {
        Err(skill_studio_core::CoreError::io(
            &spec.program,
            std::io::Error::from(std::io::ErrorKind::NotFound),
        ))
    }
}

/// Fails if a git that cannot start reaches the person as "No such file or
/// directory (os error 2)" instead of the missing-git sentence.
#[test]
fn git_that_cannot_start_is_reported_as_missing_git_or_as_raw_os_text() {
    use skill_studio_core::pack::{PackRemote, RemoteError};
    let remote = GitPackRemote::new(Arc::new(NoProgram));
    let err = remote
        .fetch(
            "https://github.com/owner/pack.git",
            None,
            &skill_studio_core::ports::NeverCancel,
        )
        .unwrap_err();
    assert_eq!(err, RemoteError::GitMissing);
}
