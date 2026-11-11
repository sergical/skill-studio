//! `TreeHash`: the git tree SHA-1 of a skill folder.
//!
//! This is the SHA GitHub shows for a tree and the `skillFolderHash`
//! `npx skills` writes into `~/.agents/.skill-lock.json`
//! ([`crate::lock_file::InstalledSkillEntry::skill_folder_hash`]). It exists
//! only to answer "does the installed copy match the tree the lock file or
//! GitHub records", so a currency check can avoid a network call.
//!
//! Distinct from [`crate::ops::skill_content_hash`]: that one is a sha256
//! over path/byte pairs, invented for the app's own change detection and
//! meaningless outside it. `TreeHash` reimplements git's own object model -
//! `blob <len>\0<bytes>` and `tree <len>\0<entries>`, both sha1'd - purely
//! against [`ScopeFs`], with no `git` binary involved.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use sha1::{Digest, Sha1};

use crate::error::CoreError;
use crate::ports::{FileKind, ScopeFs};

/// Largest single file this will read into a blob. A file over the cap is a
/// read error, not a silent truncation: a truncated read would still
/// produce a hex string that looks like a valid tree hash but does not
/// match git's, which defeats the point of comparing against git or the
/// lock file.
const MAX_BLOB_BYTES: u64 = 256 * 1024 * 1024;

/// Git mode string for a regular file, non-executable.
const MODE_FILE: &[u8] = b"100644";
/// Git mode string for a regular file with any executable bit set.
const MODE_EXEC: &[u8] = b"100755";
/// Git mode string for a symlink (the blob holds the link target text).
const MODE_SYMLINK: &[u8] = b"120000";
/// Git mode string for a subtree.
const MODE_TREE: &[u8] = b"40000";

/// A skill tree hash and the repository metadata it excluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeHashEvidence {
    /// Git tree SHA-1 of the skill files.
    pub hash: String,
    /// Skipped `.git` directories, relative to the skill folder and sorted.
    pub excluded_git_directories: Vec<PathBuf>,
}

/// Hashes a skill and names the repository metadata excluded from the hash.
pub fn tree_hash_with_evidence(
    fs: &dyn ScopeFs,
    dir: &Path,
) -> Result<TreeHashEvidence, CoreError> {
    tree_hash_with(fs, dir, false)
}

/// The git tree SHA-1 of `dir`, hex-encoded - the same value `git
/// write-tree` would produce for `dir` added to a fresh, empty index.
/// Repository metadata directories named `.git` are excluded at every depth.
/// [`tree_hash_with_evidence`] lists the excluded directories.
///
/// A directory with no trackable entry (no files, symlinks, or non-empty
/// subdirectories, recursively) hashes the same way git tracks it: as the
/// well-known empty tree, `4b825dc642cb6eb9a060e54bf8d69288fbee4904`.
pub fn tree_hash(fs: &dyn ScopeFs, dir: &Path) -> Result<String, CoreError> {
    tree_hash_with_evidence(fs, dir).map(|evidence| evidence.hash)
}

/// [`tree_hash`] without the files the `skills` CLI never installs and the
/// OS or Python litter into a skill folder (see [`is_install_junk`]), so
/// that `.DS_Store` or `__pycache__` does not make an untouched install
/// look edited.
pub fn tree_hash_ignoring_junk(fs: &dyn ScopeFs, dir: &Path) -> Result<String, CoreError> {
    tree_hash_with(fs, dir, true).map(|evidence| evidence.hash)
}

/// Names `npx skills` leaves out of an install (`EXCLUDE_DIRS` in the CLI:
/// `.git`, `__pycache__`, `__pypackages__`) plus Finder's `.DS_Store`.
/// `metadata.json` is also on the CLI's exclude list but is deliberately
/// not skipped here: the GitHub tree hash includes it, and a user may edit
/// one that exists locally.
pub fn is_install_junk(name: &str, kind: FileKind) -> bool {
    match kind {
        FileKind::Dir => matches!(name, ".git" | "__pycache__" | "__pypackages__"),
        _ => name == ".DS_Store",
    }
}

fn tree_hash_with(
    fs: &dyn ScopeFs,
    dir: &Path,
    skip_junk: bool,
) -> Result<TreeHashEvidence, CoreError> {
    let mut excluded_git_directories = Vec::new();
    let sha = hash_dir(
        fs,
        dir,
        skip_junk,
        Path::new(""),
        &mut excluded_git_directories,
    )?
    .unwrap_or_else(|| hash_object(b"tree", &[]));
    let mut hex = String::with_capacity(sha.len() * 2);
    for byte in sha {
        write!(hex, "{byte:02x}").ok();
    }
    excluded_git_directories.sort();
    Ok(TreeHashEvidence {
        hash: hex,
        excluded_git_directories,
    })
}

/// One sorted tree entry, formatted git's way: `<mode> <name>\0<20-byte sha>`.
struct Entry {
    /// Comparison key for git's sort order: the name, with a trailing `/`
    /// appended for directories, so `"foo"` (a file) sorts before `"foo/"`
    /// (the directory `foo`) even though `"foo"` alone is a lexical prefix
    /// of `"foo-bar"` while `"foo/"` is not.
    sort_key: Vec<u8>,
    mode: &'static [u8],
    name: Vec<u8>,
    sha: [u8; 20],
}

/// Hashes `dir`'s contents into a git tree object, or `None` when `dir`
/// contributes nothing trackable - the same case in which git leaves the
/// directory out of its parent tree entirely, since git never records an
/// empty directory.
fn hash_dir(
    fs: &dyn ScopeFs,
    dir: &Path,
    skip_junk: bool,
    relative: &Path,
    excluded_git_directories: &mut Vec<PathBuf>,
) -> Result<Option<[u8; 20]>, CoreError> {
    let listing = fs.read_dir(dir).map_err(|e| CoreError::io(dir, e))?;
    let mut entries = Vec::new();
    for item in listing {
        if item.kind == FileKind::Dir && item.name == ".git" {
            excluded_git_directories.push(relative.join(&item.name));
            continue;
        }
        if skip_junk && is_install_junk(&item.name, item.kind) {
            continue;
        }
        let path = dir.join(&item.name);
        match item.kind {
            FileKind::Dir => {
                if let Some(sha) = hash_dir(
                    fs,
                    &path,
                    skip_junk,
                    &relative.join(&item.name),
                    excluded_git_directories,
                )? {
                    entries.push(Entry {
                        sort_key: sort_key(&item.name, true),
                        mode: MODE_TREE,
                        name: item.name.into_bytes(),
                        sha,
                    });
                }
            }
            FileKind::Symlink => {
                let target = fs.read_link(&path).map_err(|e| CoreError::io(&path, e))?;
                let bytes = target.to_string_lossy().into_owned().into_bytes();
                entries.push(Entry {
                    sort_key: sort_key(&item.name, false),
                    mode: MODE_SYMLINK,
                    name: item.name.into_bytes(),
                    sha: hash_object(b"blob", &bytes),
                });
            }
            FileKind::File => {
                let facts = fs
                    .symlink_metadata(&path)
                    .map_err(|e| CoreError::io(&path, e))?;
                let bytes = fs
                    .read_capped(&path, MAX_BLOB_BYTES)
                    .map_err(|e| CoreError::io(&path, e))?;
                let mode = if facts.mode.is_some_and(|m| m & 0o111 != 0) {
                    MODE_EXEC
                } else {
                    MODE_FILE
                };
                entries.push(Entry {
                    sort_key: sort_key(&item.name, false),
                    mode,
                    name: item.name.into_bytes(),
                    sha: hash_object(b"blob", &bytes),
                });
            }
            // Sockets, devices, and the rest: git tracks none of these
            // either, since none can be `git add`ed.
            FileKind::Other => {}
        }
    }
    if entries.is_empty() {
        return Ok(None);
    }
    entries.sort_by(|a, b| a.sort_key.cmp(&b.sort_key));
    let mut content = Vec::new();
    for entry in &entries {
        content.extend_from_slice(entry.mode);
        content.push(b' ');
        content.extend_from_slice(&entry.name);
        content.push(0);
        content.extend_from_slice(&entry.sha);
    }
    Ok(Some(hash_object(b"tree", &content)))
}

fn sort_key(name: &str, is_dir: bool) -> Vec<u8> {
    let mut key = name.as_bytes().to_vec();
    if is_dir {
        key.push(b'/');
    }
    key
}

/// `sha1("<kind> <len(content)>\0" ++ content)` - git's object hash for
/// either a `blob` or a `tree`.
fn hash_object(kind: &[u8], content: &[u8]) -> [u8; 20] {
    let mut hasher = Sha1::new();
    hasher.update(kind);
    hasher.update(b" ");
    hasher.update(content.len().to_string().as_bytes());
    hasher.update(b"\0");
    hasher.update(content);
    hasher.finalize().into()
}

// The git-parity, lock-file, and mtime tests named in the ticket
// (`tree_hash_matches_the_skills_cli_lock_file_...`,
// `tree_hash_matches_a_git_tree_sha_computed_by_git_...`,
// `tree_hash_ignores_file_order_and_mtimes_...`) live in
// `tests/tree_hash_git_parity.rs` instead of here: they need
// `skill-studio-host`'s real `RealFs`, and this crate's own unit tests run
// inside the same compilation as `skill-studio-host`'s dependency on this
// crate, which would build two distinct, non-interchangeable copies of
// `ScopeFs`/`FileKind` (see `crate::testing::golden`'s doc comment for the
// same cycle). An external integration test, compiled as its own crate
// against `skill-studio-host` and this crate as ordinary dependencies, has
// no such cycle.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FixtureBuilder;
    use std::path::Path;

    #[test]
    fn hashing_a_skill_ignores_nested_git_writes_or_reports_repository_metadata_as_an_edit() {
        for git_dir in ["/skill/.git", "/skill/scripts/.git"] {
            let fs = FixtureBuilder::new()
                .dir(git_dir)
                .file("/skill/SKILL.md", b"skill body")
                .build_fs();
            let before = tree_hash(&fs, Path::new("/skill")).unwrap();
            fs.fsops_write_new_file(&Path::new(git_dir).join("FETCH_HEAD"), b"new commit")
                .unwrap();
            let after = tree_hash_with_evidence(&fs, Path::new("/skill")).unwrap();
            assert_eq!(
                after.hash, before,
                "repository metadata at {git_dir} must not count as a skill edit"
            );
            assert_eq!(
                after.excluded_git_directories,
                vec![PathBuf::from(git_dir.strip_prefix("/skill/").unwrap())],
                "the hash evidence must identify the excluded repository directory"
            );
            fs.fsops_write_new_file(Path::new("/skill/notes.md"), b"skill edit")
                .unwrap();
            assert_ne!(
                tree_hash(&fs, Path::new("/skill")).unwrap(),
                before,
                "skill files must still affect the hash"
            );
        }
    }

    #[test]
    fn tree_hash_of_an_empty_folder_is_gits_well_known_empty_tree_sha_or_names_the_mismatch() {
        let fs = FixtureBuilder::new().dir("/skill").build_fs();
        let actual = tree_hash(&fs, Path::new("/skill")).expect("hash an empty folder");
        assert_eq!(
            actual, "4b825dc642cb6eb9a060e54bf8d69288fbee4904",
            "an empty directory must hash as git's well-known empty tree"
        );
    }

    /// An untracked empty directory does not survive a fresh `git clone` -
    /// git records no tree for it at all - so this exercises the same
    /// omission with an in-memory fixture instead of relying on
    /// `fixtures/tree-hash/skills/empty-dir/nothing-here` surviving a
    /// clone of this repository.
    #[test]
    fn tree_hash_omits_an_empty_subdirectory_from_its_parent_or_names_the_leftover_entry() {
        let with_empty_sibling = FixtureBuilder::new()
            .dir("/skill/nothing-here")
            .file("/skill/keepme/file.txt", b"kept\n")
            .build_fs();
        let without_empty_sibling = FixtureBuilder::new()
            .file("/skill/keepme/file.txt", b"kept\n")
            .build_fs();
        let with_hash = tree_hash(&with_empty_sibling, Path::new("/skill")).unwrap();
        let without_hash = tree_hash(&without_empty_sibling, Path::new("/skill")).unwrap();
        assert_eq!(
            with_hash, without_hash,
            "an empty subdirectory must not appear as a tree entry in its parent"
        );
    }
}
