#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Real-disk tests for `pack_export`: installed skills become a new folder
//! laid out as a pack, and nothing is ever overwritten.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use skill_studio_core::dto::ListEventsRequest;
use skill_studio_core::harness::HarnessCatalog;
use skill_studio_core::pack_export::PackExportRequest;
use skill_studio_core::ports::{Ports, Runtime};
use skill_studio_core::scope::RuntimeScope;
use skill_studio_core::testing::golden::{ctx, unique_temp_dir};
use skill_studio_core::testing::{FakeClock, FakeIds, PanicOnSpawn, RecordingSink};
use skill_studio_core::{ops, ErrorCode};
use skill_studio_host::{FileLease, RealFs, SqliteHistoryOpener};

fn runtime(home: &Path) -> Runtime {
    let ports = Ports {
        fs: Arc::new(RealFs::new()),
        clock: Arc::new(FakeClock::at(0)),
        ids: Arc::new(FakeIds::default()),
        leases: Arc::new(FileLease::new(home.join(".leases"))),
        history: Arc::new(SqliteHistoryOpener::new(
            home.join(".history/events.sqlite3"),
        )),
        sink: Arc::new(RecordingSink::default()),
        spawner: Some(Arc::new(PanicOnSpawn)),
        discovery: None,
        tools: None,
        catalog: Arc::new(HarnessCatalog::builtin()),
        telemetry: Arc::new(skill_studio_core::ports::NoopTelemetry),
    };
    Runtime::new(&RuntimeScope::fixture(home), ports).unwrap()
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn skill_md(name: &str) -> String {
    format!("---\nname: {name}\ndescription: The {name} skill.\n---\nBody of {name}.\n")
}

/// `alpha` in the shared folder with a nested executable script; `bravo`
/// only in Claude Code's own folder.
fn home(label: &str) -> PathBuf {
    let home = unique_temp_dir(label);
    let alpha = home.join(".agents/skills/alpha");
    write(&alpha.join("SKILL.md"), &skill_md("alpha"));
    write(&alpha.join("scripts/run.sh"), "#!/bin/sh\necho run\n");
    std::fs::set_permissions(
        alpha.join("scripts/run.sh"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    write(
        &home.join(".claude/skills/bravo/SKILL.md"),
        &skill_md("bravo"),
    );
    home
}

fn export(dir: &Path, skills: &[&str], dry_run: bool) -> PackExportRequest {
    PackExportRequest {
        dir: dir.to_path_buf(),
        skills: skills.iter().map(|s| (*s).to_string()).collect(),
        source: Some("me/my-skills".to_string()),
        dry_run,
        ..PackExportRequest::default()
    }
}

#[test]
fn pack_export_copies_each_skill_and_a_readme_into_a_new_folder_and_records_no_event() {
    let home = home("pack_export_real");
    let rt = runtime(&home);
    let out = home.join("out/my-skills");
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();

    let report = ops::pack_export(&rt, &ctx(), &export(&out, &["alpha", "bravo"], false)).unwrap();

    assert_eq!(
        report.files,
        [
            "skills/alpha/SKILL.md",
            "skills/alpha/scripts/run.sh",
            "skills/bravo/SKILL.md",
            "README.md",
        ]
        .map(PathBuf::from)
    );
    assert_eq!(
        report.install_command,
        "npx skills add me/my-skills --skill alpha --skill bravo"
    );
    assert_eq!(
        std::fs::read_to_string(out.join("skills/bravo/SKILL.md")).unwrap(),
        skill_md("bravo")
    );
    let script = out.join("skills/alpha/scripts/run.sh");
    assert_eq!(
        std::fs::read_to_string(&script).unwrap(),
        "#!/bin/sh\necho run\n"
    );
    assert_eq!(
        std::fs::metadata(&script).unwrap().permissions().mode() & 0o777,
        0o755
    );
    let readme = std::fs::read_to_string(out.join("README.md")).unwrap();
    assert!(readme.contains(&report.install_command), "{readme}");
    assert!(
        readme.contains("npx skills add me/my-skills --skill bravo\n"),
        "{readme}"
    );
    assert_eq!(report.skills[1].from, home.join(".claude/skills/bravo"));
    // The export changes nothing Skill Studio manages: no journal event, no
    // change to the installed copies.
    let events = ops::list_events(&rt, &ctx(), &ListEventsRequest::default()).unwrap();
    assert!(events.is_empty(), "{events:#?}");
    assert!(home.join(".agents/skills/alpha/SKILL.md").exists());
    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn pack_export_dry_run_plans_every_file_and_writes_nothing() {
    let home = home("pack_export_dry");
    let rt = runtime(&home);
    let out = home.join("new-pack");

    let report = ops::pack_export(&rt, &ctx(), &export(&out, &["alpha"], true)).unwrap();

    assert!(report.dry_run);
    assert_eq!(report.files.len(), 3, "{:?}", report.files);
    assert!(!out.exists(), "a dry run must not create the folder");
    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn pack_export_refuses_a_folder_with_files_and_leaves_them_as_they_were() {
    let home = home("pack_export_not_empty");
    let rt = runtime(&home);
    let out = home.join("busy");
    write(&out.join("README.md"), "mine\n");

    let err = ops::pack_export(&rt, &ctx(), &export(&out, &["alpha"], false)).unwrap_err();

    assert_eq!(err.code, ErrorCode::InvalidRequest);
    assert!(err.message.contains("not empty"), "{err:?}");
    assert_eq!(
        std::fs::read_to_string(out.join("README.md")).unwrap(),
        "mine\n"
    );
    assert!(!out.join("skills").exists());
    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn pack_export_refuses_bad_requests_before_it_writes_anything() {
    let home = home("pack_export_refusals");
    write(
        &home.join(".agents/skills-parked/charlie/SKILL.md"),
        &skill_md("charlie"),
    );
    let outside = home.join("outside.md");
    write(&outside, "secret\n");
    let delta = home.join(".agents/skills/delta");
    write(&delta.join("SKILL.md"), &skill_md("delta"));
    std::os::unix::fs::symlink(&outside, delta.join("notes.md")).unwrap();
    let rt = runtime(&home);
    let out = home.join("pack");

    let cases: Vec<(PackExportRequest, &str)> = vec![
        (export(&out, &[], false), "at least one skill"),
        (export(&out, &["alpha", "alpha"], false), "named twice"),
        (
            export(&out, &["../alpha"], false),
            "not a skill folder name",
        ),
        (
            export(Path::new("relative/pack"), &["alpha"], false),
            "absolute path",
        ),
        (
            export(&out, &["nobody"], false),
            "no installed skill is named nobody",
        ),
        (export(&out, &["charlie"], false), "parked"),
        (
            export(&out, &["alpha", "delta"], false),
            "leaves its folder",
        ),
    ];
    for (req, expected) in cases {
        let err = ops::pack_export(&rt, &ctx(), &req).unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidRequest, "{expected}: {err:?}");
        assert!(err.message.contains(expected), "{expected}: {err:?}");
        assert!(!out.exists(), "{expected}: refused after writing");
    }
    std::fs::remove_dir_all(&home).ok();
}

/// A link inside a skill back into its own folder would make the walk run
/// forever. Fails when the export follows it instead of refusing.
#[test]
fn pack_export_refuses_a_link_that_loops_back_into_the_skill() {
    let home = home("pack_export_loop");
    let skill = home.join(".agents/skills/epsilon");
    write(&skill.join("SKILL.md"), &skill_md("epsilon"));
    std::os::unix::fs::symlink(".", skill.join("loop")).unwrap();
    let rt = runtime(&home);

    let err =
        ops::pack_export(&rt, &ctx(), &export(&home.join("pack"), &["epsilon"], true)).unwrap_err();

    assert_eq!(err.code, ErrorCode::InvalidRequest);
    assert!(err.message.contains("loops back"), "{err:?}");
    std::fs::remove_dir_all(&home).ok();
}

/// The install lines are pasted into a shell, so a name or source with a
/// space or a shell character is quoted. Fails when `my skill;id` reaches
/// the README unquoted.
#[test]
fn pack_export_quotes_each_install_argument_for_the_shell() {
    let home = home("pack_export_quote");
    write(
        &home.join(".agents/skills/my skill;id/SKILL.md"),
        &skill_md("my skill;id"),
    );
    let rt = runtime(&home);
    let out = home.join("pack");

    let report = ops::pack_export(&rt, &ctx(), &export(&out, &["my skill;id"], false)).unwrap();

    assert_eq!(
        report.install_command,
        "npx skills add me/my-skills --skill 'my skill;id'"
    );
    let readme = std::fs::read_to_string(out.join("README.md")).unwrap();
    assert!(!readme.contains("--skill my skill;id"), "{readme}");
    std::fs::remove_dir_all(&home).ok();
}
