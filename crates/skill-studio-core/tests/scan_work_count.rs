// Integration test binaries aren't covered by the lib crate's
// `cfg_attr(test, allow(...))`: this file compiles as its own crate, so
// the same allow needs to be declared here too.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Given a 400-skill estate across the harness roots with 20% linked from
//! the shared root, when the app rescans, the scan reads each `SKILL.md`
//! once and lists each directory once; fails if either count grows with
//! links or harnesses. With a scan memo, a rescan reads only the
//! `SKILL.md` files that changed.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use skill_studio_core::bench_estate::estate;
use skill_studio_core::dto::ScanRequest;
use skill_studio_core::harness::{HarnessCatalog, RootRole, ScopeLevel};
use skill_studio_core::identity::{MOVE_ASIDE_DIR_NAME, PARKED_ROOT_RELATIVE};
use skill_studio_core::ops::scan;
use skill_studio_core::ports::{
    DirEntryFacts, ExclusiveGuard, FileFacts, Ports, Runtime, ScopeFs, ScopedPath,
};
use skill_studio_core::scan_memo::ScanMemo;
use skill_studio_core::scope::{ProjectSelection, RuntimeScope};
use skill_studio_core::testing::golden::{ctx, materialized_ports, unique_temp_dir};
use skill_studio_core::testing::FakeClock;

use skill_studio_host::{FileLease, RealFs};

/// Wraps another [`ScopeFs`], counting every call per method so a test can
/// assert on the number of filesystem operations a scan performed, not just
/// its result. `skill_md_reads` counts only `read_capped`/`read_prefix`
/// calls whose path ends in `SKILL.md`. `read_dir` calls are recorded in
/// full, not just counted, so a test can assert no path was listed twice.
#[derive(Default)]
struct CountingFs {
    inner: Option<Arc<dyn ScopeFs>>,
    canonicalize: AtomicU64,
    symlink_metadata: AtomicU64,
    read_link: AtomicU64,
    read_dir_calls: Mutex<Vec<PathBuf>>,
    ancestor_holds: AtomicU64,
    read_capped: AtomicU64,
    read_prefix: AtomicU64,
    write_atomic: AtomicU64,
    rename: AtomicU64,
    skill_md_reads: AtomicU64,
    /// One call to fail, as a call does when the process has too many open
    /// files.
    fail: Mutex<Option<FailCall>>,
}

/// Fails the call to `method` on `path` after `skip` calls to it succeeded.
struct FailCall {
    method: &'static str,
    path: PathBuf,
    skip: u32,
}

impl CountingFs {
    fn wrap(inner: Arc<dyn ScopeFs>) -> Self {
        CountingFs {
            inner: Some(inner),
            ..Default::default()
        }
    }

    fn inner(&self) -> &dyn ScopeFs {
        self.inner.as_deref().expect("CountingFs::wrap sets inner")
    }

    fn count(counter: &AtomicU64) {
        counter.fetch_add(1, Ordering::SeqCst);
    }

    fn get(counter: &AtomicU64) -> u64 {
        counter.load(Ordering::SeqCst)
    }

    fn fail_call(&self, method: &'static str, path: &Path) -> std::io::Result<()> {
        let mut fail = self.fail.lock().expect("fail lock");
        let Some(call) = fail.as_mut() else {
            return Ok(());
        };
        if call.method != method || call.path != path {
            return Ok(());
        }
        if call.skip > 0 {
            call.skip -= 1;
            return Ok(());
        }
        *fail = None;
        Err(std::io::Error::other("too many open files"))
    }
}

impl ScopeFs for CountingFs {
    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        Self::count(&self.canonicalize);
        self.fail_call("canonicalize", path)?;
        self.inner().canonicalize(path)
    }
    fn symlink_metadata(&self, path: &Path) -> std::io::Result<FileFacts> {
        Self::count(&self.symlink_metadata);
        self.fail_call("symlink_metadata", path)?;
        self.inner().symlink_metadata(path)
    }
    fn read_link(&self, path: &Path) -> std::io::Result<PathBuf> {
        Self::count(&self.read_link);
        self.inner().read_link(path)
    }
    fn read_dir(&self, path: &Path) -> std::io::Result<Vec<DirEntryFacts>> {
        self.read_dir_calls
            .lock()
            .expect("read_dir_calls lock")
            .push(path.to_path_buf());
        self.inner().read_dir(path)
    }
    fn ancestor_holds(&self, start: &Path, name: &str) -> std::io::Result<bool> {
        Self::count(&self.ancestor_holds);
        self.inner().ancestor_holds(start, name)
    }
    fn read_capped(&self, path: &Path, max_bytes: u64) -> std::io::Result<Vec<u8>> {
        Self::count(&self.read_capped);
        self.fail_call("read_capped", path)?;
        if path.ends_with("SKILL.md") {
            Self::count(&self.skill_md_reads);
        }
        self.inner().read_capped(path, max_bytes)
    }
    fn read_prefix(&self, path: &Path, limit: u64) -> std::io::Result<(Vec<u8>, bool)> {
        Self::count(&self.read_prefix);
        if path.ends_with("SKILL.md") {
            Self::count(&self.skill_md_reads);
        }
        self.inner().read_prefix(path, limit)
    }
    fn write_atomic(
        &self,
        guard: &ExclusiveGuard,
        path: &ScopedPath,
        bytes: &[u8],
    ) -> std::io::Result<()> {
        Self::count(&self.write_atomic);
        self.inner().write_atomic(guard, path, bytes)
    }
    fn rename(
        &self,
        guard: &ExclusiveGuard,
        from: &ScopedPath,
        to: &ScopedPath,
    ) -> std::io::Result<()> {
        Self::count(&self.rename);
        self.inner().rename(guard, from, to)
    }
    fn remove_file(&self, guard: &ExclusiveGuard, path: &ScopedPath) -> std::io::Result<()> {
        self.inner().remove_file(guard, path)
    }
    fn create_dir_all(&self, guard: &ExclusiveGuard, path: &ScopedPath) -> std::io::Result<()> {
        self.inner().create_dir_all(guard, path)
    }
    fn symlink(
        &self,
        guard: &ExclusiveGuard,
        target: &ScopedPath,
        link: &ScopedPath,
    ) -> std::io::Result<()> {
        self.inner().symlink(guard, target, link)
    }
    fn fsops_device_inode(&self, path: &Path) -> std::io::Result<(u64, u64)> {
        self.inner().fsops_device_inode(path)
    }
    fn fsops_fsync_file(&self, path: &Path) -> std::io::Result<()> {
        self.inner().fsops_fsync_file(path)
    }
    fn fsops_fsync_dir(&self, path: &Path) -> std::io::Result<()> {
        self.inner().fsops_fsync_dir(path)
    }
    fn fsops_create_dir(&self, path: &Path) -> std::io::Result<()> {
        self.inner().fsops_create_dir(path)
    }
    fn fsops_write_new_file(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        self.inner().fsops_write_new_file(path, bytes)
    }
    fn fsops_write_new_file_with_mode(
        &self,
        path: &Path,
        bytes: &[u8],
        mode: u32,
    ) -> std::io::Result<()> {
        self.inner()
            .fsops_write_new_file_with_mode(path, bytes, mode)
    }
    fn fsops_rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        self.inner().fsops_rename(from, to)
    }
    fn fsops_symlink(&self, target: &Path, link: &Path) -> std::io::Result<()> {
        self.inner().fsops_symlink(target, link)
    }
    fn fsops_remove_dir(&self, path: &Path) -> std::io::Result<()> {
        self.inner().fsops_remove_dir(path)
    }
    fn fsops_remove_file(&self, path: &Path) -> std::io::Result<()> {
        self.inner().fsops_remove_file(path)
    }
    fn fsops_exchange(&self, a: &Path, b: &Path) -> std::io::Result<()> {
        self.inner().fsops_exchange(a, b)
    }
    fn fsops_bind_root(&self, _root: &Path) -> std::io::Result<Option<Box<dyn ScopeFs + '_>>> {
        Ok(None)
    }
}

/// A fixture-rooted scope tracking `project_dirs` as explicit projects, the
/// same shape `benches/scan.rs`'s `scope_for` uses.
fn scope_for(home: &Path, project_dirs: &[PathBuf]) -> RuntimeScope {
    let mut scope = RuntimeScope::fixture(home);
    scope.projects = ProjectSelection::Explicit {
        paths: project_dirs.iter().map(|d| home.join(d)).collect(),
    };
    scope
}

/// Every directory a correct scan must call `read_dir` on for this estate, by
/// canonical identity, derived from the catalog's own root declarations and
/// a plain (uncounted) walk of the materialized fixture: each harness root,
/// tracked-project root, and parked/move-aside root that actually exists on
/// disk, plus each directory entry those roots hold, resolved to its
/// canonical path so a symlinked skill's one real directory appears once,
/// however many roots link it. A scan's actual `read_dir` calls must be
/// canonicalized the same way before comparing against this set, since a
/// deduplicated symlinked skill is read through whichever root's literal
/// entry path the scan reaches it by first, not through its canonical path.
fn expected_dirs(home: &Path, project_dirs: &[PathBuf]) -> BTreeSet<PathBuf> {
    let catalog = HarnessCatalog::builtin();
    let mut candidate_roots: BTreeSet<PathBuf> = BTreeSet::new();
    candidate_roots.insert(home.join(PARKED_ROOT_RELATIVE));
    for facts in &catalog.facts {
        for root in &facts.roots {
            let tracked = matches!(
                root.role,
                RootRole::Own | RootRole::Universal | RootRole::Legacy | RootRole::PluginCache
            );
            if !tracked {
                continue;
            }
            match root.level {
                ScopeLevel::Global => {
                    candidate_roots.insert(home.join(&root.relative_path));
                }
                ScopeLevel::Project => {
                    for project in project_dirs {
                        candidate_roots.insert(home.join(project).join(&root.relative_path));
                    }
                }
            }
        }
    }

    // A root only gets `read_dir`'d when it (or its move-aside sibling)
    // really exists; `Path::exists` follows a symlinked root the same way
    // `read_dir` would, and reports false for a broken one the way a
    // resolved-not-found root would never be listed either.
    let mut existing_roots: Vec<PathBuf> = Vec::new();
    for root in &candidate_roots {
        if root.exists() {
            existing_roots.push(root.clone());
        }
        let move_aside = root.join(MOVE_ASIDE_DIR_NAME);
        if move_aside.exists() {
            existing_roots.push(move_aside);
        }
    }

    let mut expected = BTreeSet::new();
    for root in &existing_roots {
        expected.insert(root.canonicalize().unwrap_or_else(|_| root.clone()));
        let Ok(entries) = root.read_dir() else {
            continue;
        };
        for entry in entries.flatten() {
            let is_dir = entry
                .file_type()
                .is_ok_and(|t| t.is_dir() || t.is_symlink());
            if !is_dir {
                continue;
            }
            // A broken link resolves to neither a file nor a directory;
            // the scan never lists it, so it is not added here either.
            if let Ok(canonical) = entry.path().canonicalize() {
                if canonical.is_dir() {
                    expected.insert(canonical);
                }
            }
        }
    }
    expected
}

/// Given a 400-skill estate (63% global, 20% universal/linked, four
/// harnesses), when `ops::scan` runs once, it reads each skill's `SKILL.md`
/// exactly once (a linked skill included, since it has exactly one real
/// directory) and calls `read_dir` on exactly the set of directories the
/// fixture itself holds, each exactly once.
#[test]
fn scan_reads_each_skill_md_once_and_lists_each_directory_once() {
    let generated = estate(400, 1);
    let dir = unique_temp_dir("scan-work-count");
    std::fs::create_dir_all(&dir).expect("create scan-work-count home");
    let home = dir.canonicalize().expect("canonicalize home");
    generated
        .builder
        .materialize(&home)
        .expect("materialize 400-skill estate");

    let counting_fs = Arc::new(CountingFs::wrap(Arc::new(RealFs::new())));
    let ports: Ports = materialized_ports(
        counting_fs.clone(),
        Arc::new(FileLease::new(home.join(".leases"))),
    );
    let scope = scope_for(&home, &generated.project_dirs);
    let rt = Runtime::new(&scope, ports).expect("runtime");

    scan(&rt, &ctx(), &ScanRequest::default()).expect("scan");

    let skill_md_reads = CountingFs::get(&counting_fs.skill_md_reads);
    assert_eq!(
        skill_md_reads, generated.stats.skill_count as u64,
        "expected one SKILL.md read per distinct skill folder ({}); a linked \
         skill must be read once, not once per link",
        generated.stats.skill_count
    );

    let read_dir_calls = counting_fs
        .read_dir_calls
        .lock()
        .expect("read_dir_calls lock")
        .clone();

    let mut seen: BTreeSet<&PathBuf> = BTreeSet::new();
    for path in &read_dir_calls {
        assert!(
            seen.insert(path),
            "expected every directory to be listed once; {} was listed twice",
            path.display()
        );
    }

    // Compare by canonical identity: a deduplicated symlinked skill is
    // listed through whichever root's literal entry path the scan reaches
    // it by first, which `expected_dirs` cannot predict, but both sides
    // must still resolve to the same real directories.
    let read_dir_paths: BTreeSet<PathBuf> = read_dir_calls
        .iter()
        .map(|path| path.canonicalize().unwrap_or_else(|_| path.clone()))
        .collect();

    let expected = expected_dirs(&home, &generated.project_dirs);
    if read_dir_paths != expected {
        let missing: Vec<&PathBuf> = expected.difference(&read_dir_paths).collect();
        let extra: Vec<&PathBuf> = read_dir_paths.difference(&expected).collect();
        panic!(
            "scan's read_dir calls don't match the fixture's directories: \
             missing (expected but not listed) = {missing:#?}, \
             extra (listed but not expected) = {extra:#?}"
        );
    }

    std::fs::remove_dir_all(&dir).ok();
}

/// Given the same 400-skill estate and a [`ScanMemo`], when the app rescans
/// and nothing changed, the second scan reads no `SKILL.md` and returns the
/// same skills. When one `SKILL.md` gets new bytes with the same size and
/// mtime, the next scan reads that one file again (its ctime changed) and
/// shows the new description. Fails if the memo is not used, or if a write
/// that keeps size and mtime is hidden.
#[test]
fn a_rescan_with_a_scan_memo_reads_only_the_skill_md_that_changed() {
    let generated = estate(400, 1);
    let dir = unique_temp_dir("scan-memo");
    std::fs::create_dir_all(&dir).expect("create scan-memo home");
    let home = dir.canonicalize().expect("canonicalize home");
    generated
        .builder
        .materialize(&home)
        .expect("materialize 400-skill estate");

    let counting_fs = Arc::new(CountingFs::wrap(Arc::new(RealFs::new())));
    let rt = memo_runtime(&home, &generated.project_dirs, counting_fs.clone());

    let first = scan(&rt, &ctx(), &ScanRequest::default()).expect("first scan");
    let after_first = CountingFs::get(&counting_fs.skill_md_reads);
    assert_eq!(after_first, generated.stats.skill_count as u64);

    let second = scan(&rt, &ctx(), &ScanRequest::default()).expect("second scan");
    assert_eq!(
        CountingFs::get(&counting_fs.skill_md_reads),
        after_first,
        "a rescan of an unchanged estate must read no SKILL.md"
    );
    assert_eq!(skills(&second), skills(&first));

    let skill_md = std::fs::read_dir(home.join(".claude/skills"))
        .expect("list Claude skills")
        .flatten()
        .map(|entry| entry.path().join("SKILL.md"))
        .find(|path| path.symlink_metadata().is_ok_and(|m| m.is_file()))
        .expect("a Claude skill with its own SKILL.md");
    let before = std::fs::read_to_string(&skill_md).expect("read SKILL.md");
    let mtime = filetime::FileTime::from_last_modification_time(
        &skill_md.metadata().expect("SKILL.md metadata"),
    );
    let at = before.find("description: ").expect("a description line") + "description: ".len();
    let (old, new) = if before[at..].starts_with('Z') {
        ("Z", "Y")
    } else {
        (&before[at..=at], "Z")
    };
    let after = format!("{}{new}{}", &before[..at], &before[at + old.len()..]);
    assert_eq!(after.len(), before.len());
    std::fs::write(&skill_md, &after).expect("rewrite SKILL.md");
    filetime::set_file_mtime(&skill_md, mtime).expect("restore SKILL.md mtime");

    let third = scan(&rt, &ctx(), &ScanRequest::default()).expect("third scan");
    assert_eq!(
        CountingFs::get(&counting_fs.skill_md_reads),
        after_first + 1,
        "a rescan must read again only the SKILL.md that changed"
    );
    let new_description = after[at..].lines().next().expect("description").to_owned();
    assert!(
        third
            .skills
            .iter()
            .any(|skill| skill.description.as_deref() == Some(new_description.as_str())),
        "the rescan must show the new description {new_description:?}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// A runtime over `fs` with a [`ScanMemo`]. The fixture files were written
/// just now; a clock an hour ahead puts them outside the memo's racy window.
fn memo_runtime(home: &Path, project_dirs: &[PathBuf], fs: Arc<CountingFs>) -> Runtime {
    let an_hour_ahead = chrono::Utc::now().timestamp_millis() as u64 + 3_600_000;
    let ports = Ports {
        clock: Arc::new(FakeClock::at(an_hour_ahead)),
        scan_memo: Some(Arc::new(ScanMemo::new())),
        ..materialized_ports(fs, Arc::new(FileLease::new(home.join(".leases"))))
    };
    Runtime::new(&scope_for(home, project_dirs), ports).expect("runtime")
}

fn skills(inventory: &skill_studio_core::dto::Inventory) -> serde_json::Value {
    serde_json::to_value(&inventory.skills).expect("skills to JSON")
}

/// A small home: one Claude skill with a support file, a `spec.md` and a
/// link to the support file, and one whose folder holds a link to a file
/// that does not exist.
fn two_skill_home(name: &str) -> PathBuf {
    let dir = unique_temp_dir(name);
    let skills = dir.join(".claude/skills");
    for skill in ["notes", "linked"] {
        std::fs::create_dir_all(skills.join(skill)).expect("create skill folder");
        std::fs::write(
            skills.join(skill).join("SKILL.md"),
            format!("---\nname: {skill}\ndescription: The {skill} skill.\n---\nBody.\n"),
        )
        .expect("write SKILL.md");
    }
    std::fs::write(skills.join("notes/notes.md"), "Support text.\n").expect("write notes.md");
    std::fs::write(skills.join("notes/spec.md"), "Spec.\n").expect("write spec.md");
    std::os::unix::fs::symlink("notes.md", skills.join("notes/alias.md")).expect("link alias.md");
    std::os::unix::fs::symlink(dir.join("gone.md"), skills.join("linked/gone.md"))
        .expect("link to a missing file");
    dir.canonicalize().expect("canonicalize home")
}

/// Given a skill folder where one filesystem call fails once during a scan
/// with a [`ScanMemo`] (a file read, the `spec.md` check, a link lookup),
/// when the app rescans, the rescan reads that folder again and returns the
/// same skills as a scan in which no call failed. The stamp itself reads
/// each path first, so `skip` lets that call through and fails the facts
/// read. Fails if the memo keeps a result from a failed call.
#[test]
fn a_scan_memo_does_not_keep_facts_from_a_failed_call() {
    let home = two_skill_home("scan-memo-failed-call");
    let notes = home.join(".claude/skills/notes");
    let clean_rt = memo_runtime(
        &home,
        &[],
        Arc::new(CountingFs::wrap(Arc::new(RealFs::new()))),
    );
    let clean = skills(&scan(&clean_rt, &ctx(), &ScanRequest::default()).expect("clean scan"));

    for (method, path, skip) in [
        ("read_capped", notes.join("notes.md"), 0),
        ("symlink_metadata", notes.join("spec.md"), 1),
        ("canonicalize", notes.join("alias.md"), 1),
    ] {
        let counting_fs = Arc::new(CountingFs::wrap(Arc::new(RealFs::new())));
        let rt = memo_runtime(&home, &[], counting_fs.clone());
        *counting_fs.fail.lock().expect("fail lock") = Some(FailCall {
            method,
            path: path.clone(),
            skip,
        });
        let failed = scan(&rt, &ctx(), &ScanRequest::default()).expect("scan with a failed call");
        assert!(
            counting_fs.fail.lock().expect("fail lock").is_none(),
            "{method} on {} was not called {} times",
            path.display(),
            skip + 1
        );
        assert_ne!(
            skills(&failed),
            clean,
            "a failed {method} must change the facts"
        );
        let rescan = scan(&rt, &ctx(), &ScanRequest::default()).expect("rescan");
        assert_eq!(skills(&rescan), clean, "after a failed {method}");
    }

    std::fs::remove_dir_all(&home).ok();
}

/// Given a skill folder that holds a link to a missing file, when the app
/// rescans with a [`ScanMemo`], the rescan reads that `SKILL.md` again (a
/// link target the stamp cannot read is never kept), while the other skill
/// comes from the memo. Fails if such a folder is kept.
#[test]
fn a_scan_memo_does_not_keep_a_folder_with_an_unreadable_link_target() {
    let home = two_skill_home("scan-memo-dangling-link");
    let counting_fs = Arc::new(CountingFs::wrap(Arc::new(RealFs::new())));
    let rt = memo_runtime(&home, &[], counting_fs.clone());

    scan(&rt, &ctx(), &ScanRequest::default()).expect("first scan");
    assert_eq!(CountingFs::get(&counting_fs.skill_md_reads), 2);
    scan(&rt, &ctx(), &ScanRequest::default()).expect("rescan");
    assert_eq!(
        CountingFs::get(&counting_fs.skill_md_reads),
        3,
        "only the folder with the missing link target is read again"
    );

    std::fs::remove_dir_all(&home).ok();
}
