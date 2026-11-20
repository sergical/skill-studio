#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Real-disk tests for `pack_export`: installed skills become a new folder
//! laid out as a pack, and nothing is ever overwritten.

use std::cell::Cell;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use skill_studio_core::dto::ListEventsRequest;
use skill_studio_core::harness::HarnessCatalog;
use skill_studio_core::pack_export::PackExportRequest;
use skill_studio_core::ports::{
    DirEntryFacts, DirHandle, ExclusiveGuard, FileKind, Ports, Runtime, ScopeFs, ScopedPath,
};
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
        scan_memo: None,
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
    let foxtrot = home.join(".agents/skills/foxtrot");
    write(&foxtrot.join("SKILL.md"), &skill_md("foxtrot"));
    std::os::unix::fs::symlink("../../../outside.md", foxtrot.join("notes.md")).unwrap();
    std::fs::create_dir_all(home.join(".claude/skills")).unwrap();
    std::os::unix::fs::symlink(home.join("gone/hotel"), home.join(".claude/skills/hotel")).unwrap();
    let india = home.join(".agents/skills/india");
    write(&india.join("SKILL.md"), &skill_md("india"));
    std::os::unix::fs::symlink("SKILL.md/", india.join("notes")).unwrap();
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
        (export(&out, &["foxtrot"], false), "leaves its folder"),
        (export(&out, &["hotel"], false), "broken link"),
        (export(&out, &["india"], false), "names nothing"),
    ];
    for (req, expected) in cases {
        let err = ops::pack_export(&rt, &ctx(), &req).unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidRequest, "{expected}: {err:?}");
        assert!(err.message.contains(expected), "{expected}: {err:?}");
        assert!(!out.exists(), "{expected}: refused after writing");
    }
    std::fs::remove_dir_all(&home).ok();
}

/// A link that stays inside its skill is copied as the file or folder it
/// names. Fails when the export refuses or drops it.
#[test]
fn pack_export_copies_what_a_link_inside_the_skill_names() {
    let home = home("pack_export_inner_link");
    let skill = home.join(".agents/skills/golf");
    write(&skill.join("SKILL.md"), &skill_md("golf"));
    write(&skill.join("docs/guide.md"), "guide\n");
    std::os::unix::fs::symlink("docs/guide.md", skill.join("guide.md")).unwrap();
    std::os::unix::fs::symlink("docs", skill.join("more")).unwrap();
    let rt = runtime(&home);
    let out = home.join("pack");

    let report = ops::pack_export(&rt, &ctx(), &export(&out, &["golf"], false)).unwrap();

    for copy in ["guide.md", "more/guide.md", "docs/guide.md"] {
        let path = out.join("skills/golf").join(copy);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "guide\n", "{copy}");
        assert!(!path.is_symlink(), "{copy} is a link in the pack");
    }
    assert_eq!(report.skills[0].files, 4, "{:?}", report.files);
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

/// Replaces `from` with a link to `to`, as another process could.
fn swap_for_link(from: &Path, to: &Path) {
    std::fs::rename(from, from.with_extension("aside")).unwrap();
    std::os::unix::fs::symlink(to, from).unwrap();
}

/// When [`Racing`] swaps `from` for a link to `to`.
enum When {
    /// Just before the export opens the folder `at`: after it resolved the
    /// path, before it holds the folder.
    BeforeOpen,
    /// Just after the export lists a folder under `at` that holds the entry
    /// named here: after it read each entry's kind, before it opens one.
    AfterListing(&'static str),
    /// Not a swap: writing the file named here under `at` creates it with
    /// half its bytes, then fails, as a full disk would.
    FailWriting(&'static str),
}

/// The real file system, with one swap made at an exact moment of the
/// export, so a race with another process is deterministic.
struct Racing {
    real: RealFs,
    at: PathBuf,
    from: PathBuf,
    to: PathBuf,
    when: When,
}

macro_rules! delegate {
    ($($name:ident(&self $(, $arg:ident: $ty:ty)*) -> $ret:ty;)*) => {
        $(fn $name(&self $(, $arg: $ty)*) -> $ret { self.real.$name($($arg),*) })*
    };
}

impl ScopeFs for Racing {
    delegate! {
        canonicalize(&self, path: &Path) -> std::io::Result<PathBuf>;
        symlink_metadata(&self, path: &Path) -> std::io::Result<skill_studio_core::ports::FileFacts>;
        read_link(&self, path: &Path) -> std::io::Result<PathBuf>;
        ancestor_holds(&self, start: &Path, name: &str) -> std::io::Result<bool>;
        read_dir(&self, path: &Path) -> std::io::Result<Vec<DirEntryFacts>>;
        read_capped(&self, path: &Path, max_bytes: u64) -> std::io::Result<Vec<u8>>;
        read_prefix(&self, path: &Path, limit: u64) -> std::io::Result<(Vec<u8>, bool)>;
        write_atomic(&self, guard: &ExclusiveGuard, path: &ScopedPath, bytes: &[u8]) -> std::io::Result<()>;
        rename(&self, guard: &ExclusiveGuard, from: &ScopedPath, to: &ScopedPath) -> std::io::Result<()>;
        remove_file(&self, guard: &ExclusiveGuard, path: &ScopedPath) -> std::io::Result<()>;
        create_dir_all(&self, guard: &ExclusiveGuard, path: &ScopedPath) -> std::io::Result<()>;
        symlink(&self, guard: &ExclusiveGuard, target: &ScopedPath, link: &ScopedPath) -> std::io::Result<()>;
        symlink_relative(&self, guard: &ExclusiveGuard, target: &ScopedPath, relative_target: &Path, link: &ScopedPath) -> std::io::Result<()>;
        fsops_device_inode(&self, path: &Path) -> std::io::Result<(u64, u64)>;
        fsops_fsync_file(&self, path: &Path) -> std::io::Result<()>;
        fsops_fsync_dir(&self, path: &Path) -> std::io::Result<()>;
        fsops_create_dir(&self, path: &Path) -> std::io::Result<()>;
        fsops_write_new_file(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()>;
        fsops_write_new_file_with_mode(&self, path: &Path, bytes: &[u8], mode: u32) -> std::io::Result<()>;
        fsops_rename(&self, from: &Path, to: &Path) -> std::io::Result<()>;
        fsops_symlink(&self, target: &Path, link: &Path) -> std::io::Result<()>;
        fsops_remove_dir(&self, path: &Path) -> std::io::Result<()>;
        fsops_remove_file(&self, path: &Path) -> std::io::Result<()>;
        fsops_exchange(&self, a: &Path, b: &Path) -> std::io::Result<()>;
        fsops_bind_root(&self, root: &Path) -> std::io::Result<Option<Box<dyn ScopeFs + '_>>>;
    }

    fn fsops_open_dir(&self, path: &Path) -> std::io::Result<Option<Box<dyn DirHandle>>> {
        if path != self.at {
            return self.real.fsops_open_dir(path);
        }
        match self.when {
            When::BeforeOpen => {
                swap_for_link(&self.from, &self.to);
                self.real.fsops_open_dir(path)
            }
            When::FailWriting(name) => Ok(self
                .real
                .fsops_open_dir(path)?
                .map(|inner| Box::new(FailWriting { inner, name }) as Box<dyn DirHandle>)),
            When::AfterListing(after) => Ok(self.real.fsops_open_dir(path)?.map(|inner| {
                Box::new(SwapAfterListing {
                    inner,
                    after,
                    swap: Rc::new(Cell::new(Some((self.from.clone(), self.to.clone())))),
                }) as Box<dyn DirHandle>
            })),
        }
    }
}

/// A held folder that makes [`Racing`]'s swap once a listing holds `after`,
/// and hands the same watch to every folder opened through it.
struct SwapAfterListing {
    inner: Box<dyn DirHandle>,
    after: &'static str,
    swap: Rc<Cell<Option<(PathBuf, PathBuf)>>>,
}

impl SwapAfterListing {
    fn wrap(&self, inner: Box<dyn DirHandle>) -> Box<dyn DirHandle> {
        Box::new(SwapAfterListing {
            inner,
            after: self.after,
            swap: Rc::clone(&self.swap),
        })
    }
}

impl DirHandle for SwapAfterListing {
    fn identity(&self) -> std::io::Result<(u64, u64)> {
        self.inner.identity()
    }
    fn entries(&self) -> std::io::Result<Vec<DirEntryFacts>> {
        let entries = self.inner.entries()?;
        if entries.iter().any(|entry| entry.name == self.after) {
            if let Some((from, to)) = self.swap.take() {
                swap_for_link(&from, &to);
            }
        }
        Ok(entries)
    }
    fn kind(&self, name: &str) -> std::io::Result<FileKind> {
        self.inner.kind(name)
    }
    fn open_dir(&self, name: &str) -> std::io::Result<Box<dyn DirHandle>> {
        Ok(self.wrap(self.inner.open_dir(name)?))
    }
    fn read_file(&self, name: &str, max_bytes: u64) -> std::io::Result<(Vec<u8>, u32)> {
        self.inner.read_file(name, max_bytes)
    }
    fn read_link(&self, name: &str) -> std::io::Result<PathBuf> {
        self.inner.read_link(name)
    }
    fn create_dir(&self, name: &str) -> std::io::Result<Box<dyn DirHandle>> {
        Ok(self.wrap(self.inner.create_dir(name)?))
    }
    fn write_new_file(&self, name: &str, bytes: &[u8], mode: u32) -> std::io::Result<()> {
        self.inner.write_new_file(name, bytes, mode)
    }
}

/// A held folder whose write of `name`, in it or in a folder made through
/// it, stops half way with an error.
struct FailWriting {
    inner: Box<dyn DirHandle>,
    name: &'static str,
}

impl DirHandle for FailWriting {
    fn identity(&self) -> std::io::Result<(u64, u64)> {
        self.inner.identity()
    }
    fn entries(&self) -> std::io::Result<Vec<DirEntryFacts>> {
        self.inner.entries()
    }
    fn kind(&self, name: &str) -> std::io::Result<FileKind> {
        self.inner.kind(name)
    }
    fn open_dir(&self, name: &str) -> std::io::Result<Box<dyn DirHandle>> {
        let inner = self.inner.open_dir(name)?;
        Ok(Box::new(FailWriting {
            inner,
            name: self.name,
        }))
    }
    fn read_file(&self, name: &str, max_bytes: u64) -> std::io::Result<(Vec<u8>, u32)> {
        self.inner.read_file(name, max_bytes)
    }
    fn read_link(&self, name: &str) -> std::io::Result<PathBuf> {
        self.inner.read_link(name)
    }
    fn create_dir(&self, name: &str) -> std::io::Result<Box<dyn DirHandle>> {
        let inner = self.inner.create_dir(name)?;
        Ok(Box::new(FailWriting {
            inner,
            name: self.name,
        }))
    }
    fn write_new_file(&self, name: &str, bytes: &[u8], mode: u32) -> std::io::Result<()> {
        if name != self.name {
            return self.inner.write_new_file(name, bytes, mode);
        }
        self.inner
            .write_new_file(name, &bytes[..bytes.len() / 2], mode)?;
        Err(std::io::Error::other("no space left on the disk"))
    }
}

fn runtime_with(home: &Path, fs: Racing) -> Runtime {
    let ports = Ports {
        fs: Arc::new(fs),
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
        scan_memo: None,
    };
    Runtime::new(&RuntimeScope::fixture(home), ports).unwrap()
}

/// The pack folder's parent turns into a link to an outside tree that
/// holds an empty folder of the same name, after the export resolved the
/// path and before it opened it. Fails when the export follows the link and
/// writes into the outside folder instead of refusing.
#[test]
fn pack_export_refuses_a_pack_folder_whose_parent_turns_into_a_link() {
    let home = home("pack_export_swap_dest");
    let parent = std::fs::canonicalize(&home).unwrap().join("out");
    let outside = home.join("elsewhere");
    std::fs::create_dir_all(parent.join("pack")).unwrap();
    std::fs::create_dir_all(outside.join("pack")).unwrap();
    let rt = runtime_with(
        &home,
        Racing {
            real: RealFs::new(),
            at: parent.clone(),
            from: parent.clone(),
            to: outside.clone(),
            when: When::BeforeOpen,
        },
    );

    let err = ops::pack_export(
        &rt,
        &ctx(),
        &export(&parent.join("pack"), &["alpha"], false),
    )
    .unwrap_err();

    assert!(
        err.message.contains("changed while the export read it"),
        "{err:?}"
    );
    assert_eq!(
        std::fs::read_dir(outside.join("pack")).unwrap().count(),
        0,
        "wrote outside the pack"
    );
    std::fs::remove_dir_all(&home).ok();
}

/// A skill folder's parent turns into a link to an outside tree that holds
/// a skill of the same name, after the export resolved the path and before
/// it opened it. Fails when the export reads the outside files instead of
/// refusing.
#[test]
fn pack_export_refuses_a_skill_folder_whose_parent_turns_into_a_link() {
    let home = home("pack_export_swap_source");
    let outside = home.join("elsewhere/skills");
    write(&outside.join("alpha/SKILL.md"), "outside bytes\n");
    let skills = std::fs::canonicalize(home.join(".agents/skills")).unwrap();
    let rt = runtime_with(
        &home,
        Racing {
            real: RealFs::new(),
            at: skills.join("alpha"),
            from: skills,
            to: outside,
            when: When::BeforeOpen,
        },
    );

    let err =
        ops::pack_export(&rt, &ctx(), &export(&home.join("pack"), &["alpha"], true)).unwrap_err();

    assert!(
        err.message.contains("changed while the export read it"),
        "{err:?}"
    );
    std::fs::remove_dir_all(&home).ok();
}

/// A folder and a file in a skill each turn into a link after the export
/// listed them as a folder and a file: the folder into a link to an empty
/// folder in the skill, the file into a link to a file outside it. Fails
/// when the export follows either link, so the pack silently misses the
/// folder's files or holds the outside bytes, instead of refusing.
#[test]
fn pack_export_refuses_an_entry_that_turns_into_a_link_after_the_listing() {
    for (entry, to) in [("scripts", "empty"), ("SKILL.md", "../../../outside.md")] {
        let home = home("pack_export_swap_entry");
        write(&home.join("outside.md"), "outside bytes\n");
        let skill = std::fs::canonicalize(home.join(".agents/skills/alpha")).unwrap();
        std::fs::create_dir_all(skill.join("empty")).unwrap();
        let rt = runtime_with(
            &home,
            Racing {
                real: RealFs::new(),
                at: skill.clone(),
                from: skill.join(entry),
                to: PathBuf::from(to),
                when: When::AfterListing(entry),
            },
        );

        let err = ops::pack_export(&rt, &ctx(), &export(&home.join("pack"), &["alpha"], true))
            .unwrap_err();

        assert!(
            err.message.contains("changed while the export read it"),
            "{entry}: {err:?}"
        );
        assert_eq!(
            err.path.as_deref(),
            Some(skill.join(entry).as_path()),
            "{entry}"
        );
        std::fs::remove_dir_all(&home).ok();
    }
}

/// The pack folder's parent turns into a link to an outside tree while the
/// export reads the skills, after it checked the folder. Fails when the
/// export resolves the path again and writes into the outside tree instead
/// of the folder it checked.
#[test]
fn pack_export_writes_into_the_folder_it_checked_when_the_parent_turns_into_a_link() {
    let home = home("pack_export_swap_dest_late");
    let parent = std::fs::canonicalize(&home).unwrap().join("out");
    let outside = home.join("elsewhere");
    std::fs::create_dir_all(&parent).unwrap();
    std::fs::create_dir_all(outside.join("pack")).unwrap();
    let rt = runtime_with(
        &home,
        Racing {
            real: RealFs::new(),
            at: std::fs::canonicalize(home.join(".agents/skills/alpha")).unwrap(),
            from: parent.clone(),
            to: outside.clone(),
            when: When::AfterListing("SKILL.md"),
        },
    );

    ops::pack_export(
        &rt,
        &ctx(),
        &export(&parent.join("pack"), &["alpha"], false),
    )
    .unwrap();

    assert_eq!(
        std::fs::read_dir(outside.join("pack")).unwrap().count(),
        0,
        "wrote outside the folder it checked"
    );
    assert!(parent
        .with_extension("aside")
        .join("pack/skills/alpha/SKILL.md")
        .is_file());
    std::fs::remove_dir_all(&home).ok();
}

/// A folder in a skill turns into a link to another folder of the skill
/// after the export listed it, and a link inside it is followed after that.
/// Fails when the export resolves the link by path from the skill folder,
/// through the swapped folder, and copies the other folder's file instead
/// of the one next to the link.
#[test]
fn pack_export_follows_a_link_through_the_folder_it_holds_not_the_path() {
    let home = home("pack_export_swap_link_parent");
    let skill = std::fs::canonicalize(home.join(".agents/skills/alpha")).unwrap();
    std::os::unix::fs::symlink("run.sh", skill.join("scripts/ref")).unwrap();
    write(&skill.join("zz-other/run.sh"), "other bytes\n");
    std::os::unix::fs::symlink("run.sh", skill.join("zz-other/ref")).unwrap();
    let rt = runtime_with(
        &home,
        Racing {
            real: RealFs::new(),
            at: skill.clone(),
            from: skill.join("scripts"),
            to: PathBuf::from("zz-other"),
            when: When::AfterListing("ref"),
        },
    );
    let out = home.join("pack");

    ops::pack_export(&rt, &ctx(), &export(&out, &["alpha"], false)).unwrap();

    assert!(skill.join("scripts").is_symlink(), "the race did not run");
    assert_eq!(
        std::fs::read_to_string(out.join("skills/alpha/scripts/ref")).unwrap(),
        "#!/bin/sh\necho run\n"
    );
    std::fs::remove_dir_all(&home).ok();
}

/// A write fails after it created its file, into a folder that existed and
/// was empty. Fails when the error's cleanup list leaves out that partial
/// file, so following it leaves the folder not empty.
#[test]
fn pack_export_lists_a_partial_file_in_the_cleanup_text() {
    let home = home("pack_export_partial");
    let parent = std::fs::canonicalize(&home).unwrap();
    std::fs::create_dir_all(parent.join("pack")).unwrap();
    let rt = runtime_with(
        &home,
        Racing {
            real: RealFs::new(),
            at: parent.clone(),
            from: PathBuf::new(),
            to: PathBuf::new(),
            when: When::FailWriting("README.md"),
        },
    );

    let err = ops::pack_export(
        &rt,
        &ctx(),
        &export(&parent.join("pack"), &["alpha"], false),
    )
    .unwrap_err();

    let detail = err.detail.unwrap_or_default();
    assert!(parent.join("pack/README.md").is_file(), "{detail}");
    assert!(detail.contains("README.md"), "{detail}");
    assert!(detail.contains("skills/alpha/SKILL.md"), "{detail}");
    std::fs::remove_dir_all(&home).ok();
}
