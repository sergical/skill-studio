//! Reads one pack's git source for a remote pack check: one `git ls-remote`
//! for the commit, then one shallow clone into a temp folder that is
//! removed after its skill folders are hashed. Git uses the user's own
//! credential helper or SSH keys; nothing is stored.
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use skill_studio_core::frontmatter::frontmatter_fields;
use skill_studio_core::pack::{
    skills_cli_folder_hash, PackRemote, RemoteError, RemotePack, RemoteSkill,
};
use skill_studio_core::ports::{CancelToken, ProcessSpawner, ProcessSpec};

use crate::RealFs;

const GIT_TIMEOUT_MS: u64 = 120_000;
const MAX_SKILL_DEPTH: usize = 6;

/// [`PackRemote`] over the `git` command line.
pub struct GitPackRemote {
    spawner: Arc<dyn ProcessSpawner>,
}

impl GitPackRemote {
    /// Runs git through `spawner`.
    pub fn new(spawner: Arc<dyn ProcessSpawner>) -> Self {
        Self { spawner }
    }

    fn git(&self, args: &[&str], cancel: &dyn CancelToken) -> Result<String, RemoteError> {
        let spec = ProcessSpec {
            program: "git".into(),
            args: args.iter().map(ToString::to_string).collect(),
            cwd: None,
            env: vec![("GIT_TERMINAL_PROMPT".into(), "0".into())],
            timeout_ms: GIT_TIMEOUT_MS,
        };
        let output = self.spawner.run(&spec, cancel).map_err(|e| {
            match e.source.as_ref().map(std::io::Error::kind) {
                Some(std::io::ErrorKind::NotFound) => RemoteError::GitMissing,
                _ => RemoteError::Failed(e.message),
            }
        })?;
        if output.timed_out {
            return Err(RemoteError::Failed(format!("git {} timed out", args[0])));
        }
        if output.status != Some(0) {
            return Err(
                RemoteError::from_git_stderr(&output.stderr).unwrap_or_else(|| {
                    let reason = output.stderr.trim().lines().last().unwrap_or("no message");
                    RemoteError::Failed(format!("git {} failed: {reason}", args[0]))
                }),
            );
        }
        Ok(output.stdout)
    }
}

impl PackRemote for GitPackRemote {
    fn fetch(
        &self,
        url: &str,
        git_ref: Option<&str>,
        cancel: &dyn CancelToken,
    ) -> Result<RemotePack, RemoteError> {
        let wanted = git_ref.unwrap_or("HEAD");
        if url.starts_with('-') || wanted.starts_with('-') {
            return Err(RemoteError::Failed(format!(
                "refusing git source {url} at {wanted}"
            )));
        }
        let listing = self.git(&["ls-remote", url, wanted], cancel)?;
        let commit = peeled_commit(&listing)
            .ok_or_else(|| RemoteError::Failed(format!("{url} has no ref {wanted}")))?;
        let dir =
            std::env::temp_dir().join(format!("skill-studio-pack-{}", ulid::Ulid::generate()));
        let dir_arg = dir.to_string_lossy();
        let mut args = vec!["clone", "--quiet", "--depth", "1", "--no-tags"];
        if let Some(git_ref) = git_ref {
            args.extend(["--branch", git_ref]);
        }
        args.extend(["--", url, &dir_arg]);
        let skills = self.git(&args, cancel).and_then(|_| {
            let trees = self.git(
                &[
                    "-C",
                    &dir_arg,
                    "ls-tree",
                    "-r",
                    "-d",
                    "-z",
                    "--full-tree",
                    "HEAD",
                ],
                cancel,
            )?;
            let root = self.git(&["-C", &dir_arg, "rev-parse", "HEAD^{tree}"], cancel)?;
            let trees = tree_hashes(&trees, root.trim());
            let mut skills = Vec::new();
            skill_folders(&dir, "", url_stem(url), 0, &trees, &mut skills);
            Ok(skills)
        });
        let _ = std::fs::remove_dir_all(&dir);
        Ok(RemotePack {
            commit,
            skills: skills?,
        })
    }
}

/// The commit a ref points at, preferring the peeled `^{}` line of an annotated tag.
fn peeled_commit(listing: &str) -> Option<String> {
    let lines: Vec<&str> = listing.lines().filter(|l| !l.trim().is_empty()).collect();
    lines
        .iter()
        .find(|l| l.ends_with("^{}"))
        .or_else(|| lines.first())
        .and_then(|l| l.split_whitespace().next())
        .map(String::from)
}

fn url_stem(url: &str) -> &str {
    let tail = url
        .trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()
        .unwrap_or(url);
    tail.strip_suffix(".git").unwrap_or(tail)
}

/// Each directory's git tree SHA by its `/`-separated path, `""` for the
/// root: the hash GitHub reports and the skills CLI records as
/// `skillFolderHash`, committed junk files included.
fn tree_hashes(ls_tree: &str, root: &str) -> HashMap<String, String> {
    let mut trees: HashMap<String, String> = ls_tree
        .split('\0')
        .filter_map(|line| {
            let (meta, path) = line.split_once('\t')?;
            Some((
                path.to_string(),
                meta.split_whitespace().nth(2)?.to_string(),
            ))
        })
        .collect();
    trees.insert(String::new(), root.to_string());
    trees
}

/// The skill name the skills CLI installs a folder under: its `SKILL.md`
/// frontmatter `name`, else the folder name.
fn skill_name(dir: &Path, folder: &str) -> String {
    std::fs::read_to_string(dir.join("SKILL.md"))
        .ok()
        .and_then(|text| frontmatter_fields(&text).remove("name"))
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| folder.to_string())
}

fn skill_folders(
    dir: &Path,
    relative: &str,
    folder: &str,
    depth: usize,
    trees: &HashMap<String, String>,
    out: &mut Vec<RemoteSkill>,
) {
    if dir.join("SKILL.md").is_file() {
        if let (Some(tree_hash), Ok(folder_hash)) = (
            trees.get(relative),
            skills_cli_folder_hash(&RealFs::new(), dir),
        ) {
            out.push(RemoteSkill {
                dir: relative.to_string(),
                name: skill_name(dir, folder),
                tree_hash: tree_hash.clone(),
                folder_hash,
            });
        }
        return;
    }
    if depth >= MAX_SKILL_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut children: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n != ".git" && n != "node_modules")
        .collect();
    children.sort();
    for child in children {
        let child_relative = if relative.is_empty() {
            child.clone()
        } else {
            format!("{relative}/{child}")
        };
        skill_folders(
            &dir.join(&child),
            &child_relative,
            &child,
            depth + 1,
            trees,
            out,
        );
    }
}
