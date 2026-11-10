use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::suite::FixtureFile;

const MAX_FILES: usize = 2000;
const MAX_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SnapshotFile {
    pub path: String,
    pub bytes: Vec<u8>,
    pub executable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Snapshot {
    pub hash: String,
    pub files: Vec<SnapshotFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Artifact {
    pub path: String,
    pub sha256: String,
    pub bytes: usize,
    pub text: Option<String>,
    pub change: String,
}

pub fn hash_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

impl Snapshot {
    pub fn read(root: &Path, workspace: bool) -> Result<Self, String> {
        let root = root
            .canonicalize()
            .map_err(|e| format!("Could not open {}: {e}", root.display()))?;
        let mut files = Vec::new();
        let mut total = 0;
        collect(&root, &root, workspace, &mut files, &mut total)?;
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let bytes = serde_json::to_vec(&files).map_err(|e| e.to_string())?;
        Ok(Self {
            hash: hash_bytes(&bytes),
            files,
        })
    }

    pub fn write(&self, root: &Path) -> Result<(), String> {
        fs::create_dir_all(root).map_err(|e| e.to_string())?;
        for file in &self.files {
            let path = root.join(&file.path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::write(&path, &file.bytes).map_err(|e| e.to_string())?;
            #[cfg(unix)]
            if file.executable {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }

    pub fn skill_name(&self) -> Result<String, String> {
        let skill = self
            .files
            .iter()
            .find(|f| f.path == "SKILL.md")
            .ok_or("The skill folder needs SKILL.md")?;
        let text = std::str::from_utf8(&skill.bytes).map_err(|e| e.to_string())?;
        let mut lines = text.lines();
        if lines.next() != Some("---") {
            return Err("SKILL.md needs YAML frontmatter with a name".into());
        }
        let yaml = lines
            .take_while(|line| *line != "---")
            .collect::<Vec<_>>()
            .join("\n");
        let metadata: serde_yaml::Value =
            serde_yaml::from_str(&yaml).map_err(|e| format!("Invalid skill frontmatter: {e}"))?;
        let name = metadata["name"]
            .as_str()
            .ok_or("Skill frontmatter needs a name")?;
        crate::suite::validate_id(name)?;
        Ok(name.into())
    }

    pub fn artifacts(&self, fixtures: &[FixtureFile]) -> Vec<Artifact> {
        let mut artifacts: Vec<_> = self
            .files
            .iter()
            .map(|file| {
                let original = fixtures.iter().find(|f| f.path == file.path);
                Artifact {
                    path: file.path.clone(),
                    sha256: hash_bytes(&file.bytes),
                    bytes: file.bytes.len(),
                    text: if file.bytes.len() <= 256 * 1024 {
                        String::from_utf8(file.bytes.clone()).ok()
                    } else {
                        None
                    },
                    change: match original {
                        Some(original) if original.content.as_bytes() == file.bytes => "unchanged",
                        Some(_) => "changed",
                        None => "created",
                    }
                    .into(),
                }
            })
            .collect();
        for file in fixtures {
            if !self.files.iter().any(|f| f.path == file.path) {
                artifacts.push(Artifact {
                    path: file.path.clone(),
                    sha256: String::new(),
                    bytes: 0,
                    text: None,
                    change: "deleted".into(),
                });
            }
        }
        artifacts
    }
}

fn collect(
    root: &Path,
    folder: &Path,
    workspace: bool,
    files: &mut Vec<SnapshotFile>,
    total: &mut usize,
) -> Result<(), String> {
    for entry in fs::read_dir(folder).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let relative = path.strip_prefix(root).map_err(|e| e.to_string())?;
        let name = relative
            .to_str()
            .ok_or("Snapshot paths must be valid UTF-8")?
            .to_string();
        if folder == root
            && (entry.file_name() == ".git"
                || (workspace
                    && [".agents", ".claude", ".pi", ".opencode", ".codex"]
                        .iter()
                        .any(|s| entry.file_name() == *s)))
        {
            continue;
        }
        let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "Cannot freeze a symbolic link: {name}. Use regular files in a test snapshot."
            ));
        }
        if metadata.is_dir() {
            if relative.components().count() > 24 {
                return Err(format!("Snapshot folder is too deep: {name}"));
            }
            collect(root, &path, workspace, files, total)?;
        } else if metadata.is_file() {
            if metadata.len() > (MAX_BYTES - *total) as u64 || files.len() >= MAX_FILES {
                return Err("Snapshot exceeds 32 MiB or 2000 files".into());
            }
            let mut bytes = Vec::new();
            fs::File::open(&path)
                .map_err(|e| e.to_string())?
                .take((MAX_BYTES - *total + 1) as u64)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            *total += bytes.len();
            if *total > MAX_BYTES {
                return Err("Snapshot exceeds 32 MiB or 2000 files".into());
            }
            #[cfg(unix)]
            let executable = {
                use std::os::unix::fs::PermissionsExt;
                metadata.permissions().mode() & 0o111 != 0
            };
            #[cfg(not(unix))]
            let executable = false;
            files.push(SnapshotFile {
                path: name,
                bytes,
                executable,
            });
        } else {
            return Err(format!("Cannot freeze a special file: {name}"));
        }
    }
    Ok(())
}

pub fn same_skill(left: &str, right: &str) -> bool {
    resolved_path(skill_root(left)) == resolved_path(skill_root(right))
}

pub fn resolved_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

pub fn skill_root(path: &str) -> &Path {
    let path = Path::new(path);
    if path.file_name().is_some_and(|name| name == "SKILL.md") {
        path.parent().unwrap_or(path)
    } else {
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshots_are_content_addressed_and_do_not_follow_artifact_links() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a"), "one").unwrap();
        let first = Snapshot::read(dir.path(), false).unwrap();
        fs::write(dir.path().join("a"), "two").unwrap();
        assert_ne!(first.hash, Snapshot::read(dir.path(), false).unwrap().hash);
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/etc/passwd", dir.path().join("outside")).unwrap();
            assert!(Snapshot::read(dir.path(), true)
                .unwrap_err()
                .contains("symbolic link"));
        }
    }
}
