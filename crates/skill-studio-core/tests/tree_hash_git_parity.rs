// Integration test binaries aren't covered by the lib crate's
// `cfg_attr(test, allow(...))`: this file compiles as its own crate, so
// the same allow needs to be declared here too.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Proves [`skill_studio_core::tree_hash::tree_hash`] against values git
//! itself produced, never against the module's own output.
//!
//! `fixtures/tree-hash/skills/*` and `fixtures/tree-hash/skill-lock.json`
//! were captured by copying each skill folder into its own scratch
//! directory and running `git init`, `git add -A`, `git write-tree` there;
//! the resulting SHAs went straight into the lock file fixture and into the
//! literal in the second test below. Regenerating them means re-running
//! that git sequence, never copying this module's output back into the
//! fixtures.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use proptest::prelude::*;

use skill_studio_core::lock_file::SkillLockFile;
use skill_studio_core::tree_hash::tree_hash;

use skill_studio_host::RealFs;

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tree-hash")
}

#[test]
fn tree_hash_matches_the_skills_cli_lock_file_for_nine_fixture_skills_or_names_the_skill_with_a_mismatched_hash(
) {
    let base = fixtures_dir();
    let lock_bytes = fs::read(base.join("skill-lock.json")).expect("read fixture skill-lock.json");
    let lock: SkillLockFile =
        serde_json::from_slice(&lock_bytes).expect("parse fixture skill-lock.json");
    assert_eq!(
        lock.skills.len(),
        9,
        "fixture lock file must name nine skills, named the count instead"
    );

    let fs_adapter = RealFs::new();
    let mut mismatches = Vec::new();
    for (name, entry) in &lock.skills {
        let dir = base.join("skills").join(name);
        let actual = tree_hash(&fs_adapter, &dir).unwrap_or_else(|e| panic!("{name}: {e}"));
        if actual != entry.skill_folder_hash {
            mismatches.push(format!(
                "{name}: lock file has {}, computed {actual}",
                entry.skill_folder_hash
            ));
        }
    }
    assert!(mismatches.is_empty(), "mismatched skills: {mismatches:?}");
}

#[test]
fn tree_hash_matches_a_git_tree_sha_computed_by_git_for_the_same_folder_or_names_the_differing_byte(
) {
    // `nested`'s SHA below is `git write-tree`'s own output for a fresh
    // copy of `fixtures/tree-hash/skills/nested` - the same value recorded
    // in `skill-lock.json`, repeated here as a literal so this test does
    // not depend on that file parsing correctly.
    let dir = fixtures_dir().join("skills").join("nested");
    let fs_adapter = RealFs::new();
    let actual = tree_hash(&fs_adapter, &dir).expect("hash the nested fixture");
    assert_eq!(
        actual, "ad489fbfda3ca76c014a29ee02f1893069eccd35",
        "byte-for-byte mismatch against git's own write-tree output for `nested`"
    );
}

/// A generated folder tree. Names are chosen to hit git's sort rule
/// (`foo` < `foo-bar` < `foo.txt` < `foo/` < `foo0`), and the tree mixes
/// plain files, executable files, links and empty folders (git drops
/// folders with no files).
#[derive(Debug, Clone)]
enum Node {
    File {
        bytes: Vec<u8>,
        executable: bool,
        days_old: u64,
    },
    Link(String),
    Dir(BTreeMap<String, Node>),
}

fn node() -> impl Strategy<Value = Node> {
    let name = prop::sample::select(vec![
        "foo", "foo-bar", "foo.txt", "foo0", "a", "b b", "SKILL.md",
    ]);
    let leaf = prop_oneof![
        (
            prop::collection::vec(any::<u8>(), 0..64),
            any::<bool>(),
            0u64..400
        )
            .prop_map(|(bytes, executable, days_old)| Node::File {
                bytes,
                executable,
                days_old
            }),
        prop::sample::select(vec!["foo", "../outside", "a/b b", "missing"])
            .prop_map(|target| Node::Link(target.to_string())),
    ];
    leaf.prop_recursive(3, 24, 5, move |inner| {
        prop::collection::btree_map(name.clone().prop_map(str::to_string), inner, 0..5)
            .prop_map(Node::Dir)
    })
}

/// Writes `tree` under `dir`, entries in reverse name order, each file with
/// its own backdated mtime: git trees are sorted by name and carry no mtime.
fn write_tree(dir: &Path, tree: &BTreeMap<String, Node>) {
    for (name, node) in tree.iter().rev() {
        let path = dir.join(name);
        match node {
            Node::File {
                bytes,
                executable,
                days_old,
            } => {
                fs::write(&path, bytes).expect("write fixture file");
                let mode = if *executable { 0o755 } else { 0o644 };
                fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
                let stamp = std::time::SystemTime::now()
                    - std::time::Duration::from_secs(days_old * 86_400);
                filetime::set_file_mtime(&path, filetime::FileTime::from_system_time(stamp))
                    .expect("backdate mtime");
            }
            Node::Link(target) => std::os::unix::fs::symlink(target, &path).unwrap(),
            Node::Dir(children) => {
                fs::create_dir(&path).unwrap();
                write_tree(&path, children);
            }
        }
    }
}

/// `git add -A && git write-tree` in `dir`, isolated from the user's and
/// the system's git config.
fn git_write_tree(dir: &Path) -> String {
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .args(["-c", "core.autocrlf=false", "-c", "core.filemode=true"])
            .args(["-c", "core.symlinks=true", "-c", "init.defaultBranch=main"])
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("HOME", dir)
            .output()
            .expect("run git");
        assert!(output.status.success(), "git {args:?}: {output:?}");
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    };
    git(&["init", "-q", "--template="]);
    git(&["add", "-A"]);
    git(&["write-tree"])
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// For any generated tree, `tree_hash` equals the SHA git itself writes
    /// for it. Order, mtimes and the `.git` folder git init creates must not
    /// move the hash; any content, name, mode or link change moves git's SHA
    /// and so must move this one.
    #[test]
    fn tree_hash_equals_git_write_tree_for_any_tree(tree in prop::collection::btree_map(
        prop::sample::select(vec!["foo", "foo-bar", "foo.txt", "foo0", "SKILL.md"]).prop_map(str::to_string),
        node(),
        1..6,
    )) {
        let dir = tempfile::tempdir().expect("tempdir");
        write_tree(dir.path(), &tree);
        let ours = tree_hash(&RealFs::new(), dir.path()).unwrap();
        let theirs = git_write_tree(dir.path());
        prop_assert_eq!(ours, theirs, "tree: {:#?}", tree);
    }
}
