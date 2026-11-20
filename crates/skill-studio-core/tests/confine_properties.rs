// Integration test binaries aren't covered by the lib crate's
// `cfg_attr(test, allow(...))`: this file compiles as its own crate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Property tests for `ports::confine` and `ports::confine_write_through`,
//! the scope check every core write goes through.
//!
//! The paths are random walks under the home through plain folders, `..`,
//! links that stay inside the home, and links (absolute and relative) that
//! leave it. The oracle is the filesystem itself: a path is confined exactly
//! when it has no `..` and its parent's canonical form lies under the home.

use std::path::{Component, Path, PathBuf};

use proptest::prelude::*;
use skill_studio_core::ports::{confine, confine_write_through, FileKind, ScopeFs};
use skill_studio_core::scope::{NormalizedScope, RuntimeScope};
use skill_studio_core::testing::{FixtureBuilder, FixtureFs};

const HOME: &str = "/home";

fn fixture() -> FixtureFs {
    FixtureBuilder::new()
        .dir("/home/a/b")
        .file("/home/a/b/f.txt", b"in")
        .file("/outside/x/secret.txt", b"out")
        // Leaves the home: absolute and relative.
        .alias("/home/esc", "/outside")
        .alias("/home/a/rel", "../../outside/x")
        .alias("/home/a/b/file-out", "/outside/x/secret.txt")
        // Stays inside the home.
        .alias("/home/a/inner", "/home/a/b")
        .alias("/home/a/b/up", "../..")
        .alias("/home/a/b/file-in", "f.txt")
        // Points nowhere.
        .alias("/home/a/dangling", "/home/missing")
        .build_fs()
}

fn segment() -> impl Strategy<Value = &'static str> {
    prop::sample::select(vec![
        "a", "b", "esc", "rel", "inner", "up", "dangling", "file-in", "file-out", "f.txt", "x",
        "new", "..", ".",
    ])
}

fn path_under_home() -> impl Strategy<Value = PathBuf> {
    prop::collection::vec(segment(), 1..6).prop_map(|parts| {
        let mut path = PathBuf::from(HOME);
        for part in parts {
            path.push(part);
        }
        path
    })
}

fn has_parent_dir(path: &Path) -> bool {
    path.components().any(|c| c == Component::ParentDir)
}

/// What `confine` must decide, worked out from the filesystem alone.
fn confined_by_oracle(fs: &FixtureFs, path: &Path) -> bool {
    !has_parent_dir(path)
        && path
            .parent()
            .and_then(|parent| fs.canonicalize(parent).ok())
            .is_some_and(|canonical| canonical.starts_with(HOME))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn confine_accepts_exactly_the_paths_whose_real_parent_is_in_the_home(
        path in path_under_home(),
    ) {
        let fs = fixture();
        let scope = NormalizedScope::normalize(&RuntimeScope::fixture(HOME), &fs).unwrap();
        let result = confine(&scope, &fs, &path);
        prop_assert_eq!(
            result.is_ok(),
            confined_by_oracle(&fs, &path),
            "confine({}) = {:?}",
            path.display(),
            result
        );
        if let Ok(scoped) = result {
            prop_assert_eq!(scoped.root(), Path::new(HOME));
            prop_assert!(!has_parent_dir(scoped.in_root()));
            // The place a real adapter writes: root + in_root, through the
            // parent's canonical form, never through a link.
            let canonical_parent = fs.canonicalize(path.parent().unwrap()).unwrap();
            let expected = match path.file_name() {
                Some(leaf) => canonical_parent.join(leaf),
                None => canonical_parent,
            };
            prop_assert_eq!(scoped.root().join(scoped.in_root()), expected);
        }
    }

    #[test]
    fn confine_write_through_never_returns_a_target_outside_the_home(
        path in path_under_home(),
    ) {
        let fs = fixture();
        let scope = NormalizedScope::normalize(&RuntimeScope::fixture(HOME), &fs).unwrap();
        let leaf_is_link = fs
            .symlink_metadata(&path)
            .is_ok_and(|facts| facts.kind == FileKind::Symlink);
        if let Ok(scoped) = confine_write_through(&scope, &fs, &path) {
            let written = scoped.root().join(scoped.in_root());
            prop_assert!(written.starts_with(HOME), "{} escaped", written.display());
            prop_assert!(!has_parent_dir(&written));
            if leaf_is_link {
                // The write goes to the file the link names, so the link survives.
                prop_assert_eq!(scoped.as_path(), fs.canonicalize(&path).unwrap());
            }
        } else {
            let target = if leaf_is_link {
                fs.canonicalize(&path).ok()
            } else {
                Some(path.clone())
            };
            prop_assert!(
                target.is_none_or(|target| !confined_by_oracle(&fs, &target)),
                "refused {} although its target is confinable",
                path.display()
            );
        }
    }
}
