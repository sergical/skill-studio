//! `pack_export`: copies installed skills into a new folder laid out as a
//! pack - `skills/<name>/` per skill and a `README.md` with the install
//! lines - so the user can push it to git and install it with
//! `npx skills add <owner>/<repo>` (design note on #615).
//!
//! The folder is the user's: the op writes no file that `npx skills`,
//! dotagents or an agent owns, no lock or registry row, and no journal
//! event, since it changes nothing Skill Studio manages and removing the
//! folder undoes it. It is the one op that writes outside the managed roots,
//! and only into the folder the request names: that folder must be new or
//! empty, and every file is created with create-new, so nothing is ever
//! overwritten.
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dto::{DeploymentDto, Inventory};
use crate::error::{CoreError, ErrorCode, ErrorEntry};
use crate::identity::{RootKind, RootScope};
use crate::ops::{OpStatus, Operation, Outcome};
use crate::ports::{FileKind, OpContext, Runtime, ScopeFs};

/// Largest single file the export copies.
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

/// Most files, and most bytes, one export copies, so a huge or looping
/// skill folder fails instead of filling memory.
const MAX_FILES: usize = 10_000;
const MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;

/// What `source` names in the README when the request gives none.
const SOURCE_PLACEHOLDER: &str = "<owner>/<repo>";

/// Input for `pack_export`.
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct PackExportRequest {
    /// Absolute folder to create the pack in. It must not exist yet, or be
    /// an empty folder.
    pub dir: PathBuf,
    /// Installed skills to put in the pack.
    pub skills: Vec<String>,
    /// Where the pack will live, as `owner/repo` or a git URL, for the
    /// README's install lines. Absent: the lines say `<owner>/<repo>`.
    #[serde(default)]
    pub source: Option<String>,
    /// Take each skill's copy from this project, as the other pack ops
    /// take `project_path`. Absent: the home.
    #[serde(default)]
    pub project_path: Option<PathBuf>,
    /// Return the plan and write nothing.
    #[serde(default)]
    pub dry_run: bool,
}

/// One skill in the pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PackExportSkill {
    /// Skill name, and its folder under `skills/`.
    #[schemars(extend("x-untrusted" = true))]
    pub skill: String,
    /// The installed copy its files come from.
    #[schemars(extend("x-untrusted" = true))]
    pub from: PathBuf,
    /// How many files it holds.
    pub files: usize,
}

/// Result of `pack_export`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PackExportReport {
    /// The call wrote nothing.
    pub dry_run: bool,
    /// The pack folder.
    #[schemars(extend("x-untrusted" = true))]
    pub dir: PathBuf,
    /// One entry per skill, in request order.
    pub skills: Vec<PackExportSkill>,
    /// Every file written, or to write on a dry run, relative to `dir`.
    #[schemars(extend("x-untrusted" = true))]
    pub files: Vec<PathBuf>,
    /// The command that installs every skill of the pack once it is pushed.
    #[schemars(extend("x-untrusted" = true))]
    pub install_command: String,
    /// One line for a person or agent.
    pub summary: String,
}

impl Outcome for PackExportReport {
    fn status(&self) -> OpStatus {
        OpStatus::Ok
    }

    fn errors(&self) -> Vec<ErrorEntry> {
        Vec::new()
    }
}

/// One file to write: its path relative to the pack folder, its bytes, and
/// its permission bits.
struct PlannedFile {
    relative: PathBuf,
    bytes: Vec<u8>,
    mode: u32,
}

/// Copies the named installed skills into `req.dir` as a pack.
pub fn pack_export(
    rt: &Runtime,
    ctx: &OpContext,
    req: &PackExportRequest,
) -> Result<PackExportReport, CoreError> {
    rt.run(Operation::PackExport, ctx, || {
        let fs = rt.ports.fs.as_ref();
        check_request(fs, req)?;
        let scope = match &req.project_path {
            Some(path) => crate::pack::selected_scopes(rt, Some(path))?.remove(0).0,
            None => RootScope::Global,
        };
        let inventory = crate::pack::scan(rt, ctx)?;
        let mut skills = Vec::new();
        let mut planned = Vec::new();
        for name in &req.skills {
            ctx.checkpoint()?;
            let copy = live_copy(&inventory, name, &scope)?;
            let before = planned.len();
            collect(
                fs,
                &copy.path,
                &Path::new("skills").join(name),
                &mut planned,
            )?;
            skills.push(PackExportSkill {
                skill: name.clone(),
                from: copy.path.clone(),
                files: planned.len() - before,
            });
        }
        let source = req.source.as_deref().unwrap_or(SOURCE_PLACEHOLDER);
        let install_command = install_line(source, &req.skills);
        planned.push(PlannedFile {
            relative: PathBuf::from("README.md"),
            bytes: readme(source, &req.skills, &install_command).into_bytes(),
            mode: 0o644,
        });
        if !req.dry_run {
            write_pack(fs, &req.dir, &planned)?;
        }
        let count = skills.len();
        let summary = format!(
            "{} {count} skill{} and a README to {}.",
            if req.dry_run { "Would write" } else { "Wrote" },
            if count == 1 { "" } else { "s" },
            req.dir.display()
        );
        Ok(PackExportReport {
            dry_run: req.dry_run,
            dir: req.dir.clone(),
            skills,
            files: planned.into_iter().map(|file| file.relative).collect(),
            install_command,
            summary,
        })
    })
}

/// Refuses a request that could write outside its folder or over a file:
/// no skills, a repeated or path-like name, a relative folder, or a folder
/// that holds anything.
fn check_request(fs: &dyn ScopeFs, req: &PackExportRequest) -> Result<(), CoreError> {
    let invalid = |message: String| CoreError::new(ErrorCode::InvalidRequest, message);
    if req.skills.is_empty() {
        return Err(invalid("name at least one skill to put in the pack".into()));
    }
    let mut seen = BTreeSet::new();
    for name in &req.skills {
        let plain = !name.is_empty()
            && name != "."
            && name != ".."
            && !name.contains(['/', '\\'])
            && !name.starts_with('.');
        if !plain {
            return Err(invalid(format!("{name:?} is not a skill folder name")));
        }
        if !seen.insert(name) {
            return Err(invalid(format!("{name} is named twice")));
        }
    }
    if !req.dir.is_absolute()
        || req
            .dir
            .components()
            .any(|c| c == std::path::Component::ParentDir)
    {
        return Err(
            invalid("give the pack folder as an absolute path without `..`".into()).at(&req.dir),
        );
    }
    match fs.symlink_metadata(&req.dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let parent = req.dir.parent().unwrap_or(&req.dir);
            match fs.symlink_metadata(parent) {
                Ok(facts) if facts.kind == FileKind::Dir => Ok(()),
                _ => {
                    Err(invalid("the pack folder's parent folder does not exist".into()).at(parent))
                }
            }
        }
        Err(e) => Err(CoreError::io(&req.dir, e)),
        Ok(facts) if facts.kind != FileKind::Dir => Err(invalid(
            "the pack folder path is a file or a link, not a folder".into(),
        )
        .at(&req.dir)),
        Ok(_) => {
            let entries = fs
                .read_dir(&req.dir)
                .map_err(|e| CoreError::io(&req.dir, e))?;
            if entries.is_empty() {
                Ok(())
            } else {
                Err(
                    invalid("the pack folder is not empty; choose a new or empty folder".into())
                        .at(&req.dir),
                )
            }
        }
    }
}

/// The copy of `name` to export from `scope`: the shared `.agents/skills`
/// copy first, else an agent's own copy. A parked, plugin-only or missing
/// skill is refused with the reason.
fn live_copy<'a>(
    inventory: &'a Inventory,
    name: &str,
    scope: &RootScope,
) -> Result<&'a DeploymentDto, CoreError> {
    let refuse = |message: String| CoreError::new(ErrorCode::InvalidRequest, message);
    let skill = inventory
        .skills
        .iter()
        .find(|skill| skill.name.0 == name)
        .ok_or_else(|| refuse(format!("no installed skill is named {name}")))?;
    let here: Vec<&DeploymentDto> = skill
        .deployments
        .iter()
        .filter(|d| &d.root.scope == scope)
        .collect();
    let pick = |want: fn(&RootKind) -> bool| here.iter().copied().find(|d| want(&d.root.kind));
    pick(|kind| matches!(kind, RootKind::Universal))
        .or_else(|| pick(|kind| matches!(kind, RootKind::Harness(_) | RootKind::Legacy(_))))
        .ok_or_else(|| {
            if here.iter().any(|d| matches!(d.root.kind, RootKind::Parked)) {
                refuse(format!("{name} is parked; unpark it first"))
            } else if skill
                .deployments
                .iter()
                .any(|d| matches!(d.root.kind, RootKind::PluginCache(_)))
            {
                refuse(format!(
                    "{name} ships with a plugin; it is not yours to export"
                ))
            } else {
                refuse(format!("{name} has no copy in this scope"))
            }
        })
}

/// Adds every file under `folder` to `planned`, at `into/<relative path>`.
/// The folder itself may be a link (a linked install): its resolved folder
/// is the boundary. Files are read through a handle bound to that folder
/// ([`ScopeFs::fsops_read_capped`]), so a link to outside it, even one
/// swapped in after the checks here, fails the read. A link that leaves the
/// folder or loops back into a folder being walked is refused. `.git` is
/// left out.
fn collect(
    fs: &dyn ScopeFs,
    folder: &Path,
    into: &Path,
    planned: &mut Vec<PlannedFile>,
) -> Result<(), CoreError> {
    let root = fs
        .canonicalize(folder)
        .map_err(|e| CoreError::io(folder, e))?;
    let (bound, _) = bind_checked(fs, &root, "the skill folder")?;
    let reader = bound.as_deref().unwrap_or(fs);
    let mut walk = Walk {
        fs,
        reader,
        root: &root,
        open: vec![root.clone()],
        planned,
    };
    walk.dir(&root, into)
}

struct Walk<'a> {
    fs: &'a dyn ScopeFs,
    reader: &'a dyn ScopeFs,
    root: &'a Path,
    /// Resolved folders on the current path, to refuse a link back into one.
    open: Vec<PathBuf>,
    planned: &'a mut Vec<PlannedFile>,
}

impl Walk<'_> {
    fn refuse(path: &Path, message: &str) -> CoreError {
        CoreError::new(ErrorCode::InvalidRequest, message.to_string()).at(path)
    }

    fn dir(&mut self, dir: &Path, into: &Path) -> Result<(), CoreError> {
        let mut entries = self.fs.read_dir(dir).map_err(|e| CoreError::io(dir, e))?;
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        for entry in entries {
            if entry.name == ".git" {
                continue;
            }
            let path = dir.join(&entry.name);
            let target = if entry.kind == FileKind::Symlink {
                let resolved = self
                    .fs
                    .canonicalize(&path)
                    .map_err(|e| CoreError::io(&path, e))?;
                if !resolved.starts_with(self.root) {
                    return Err(Self::refuse(
                        &path,
                        "a link in the skill leaves its folder; the pack would not hold what it names",
                    ));
                }
                resolved
            } else {
                path.clone()
            };
            let facts = self
                .fs
                .symlink_metadata(&target)
                .map_err(|e| CoreError::io(&target, e))?;
            let relative = into.join(&entry.name);
            match facts.kind {
                FileKind::Dir => {
                    if self.open.contains(&target) {
                        return Err(Self::refuse(
                            &path,
                            "a link in the skill loops back into its own folder",
                        ));
                    }
                    self.open.push(target.clone());
                    self.dir(&target, &relative)?;
                    self.open.pop();
                }
                FileKind::File => self.file(&path, &target, relative, facts.mode)?,
                FileKind::Symlink | FileKind::Other => {
                    return Err(Self::refuse(
                        &path,
                        "the skill holds something that is not a file or a folder",
                    ));
                }
            }
        }
        Ok(())
    }

    fn file(
        &mut self,
        path: &Path,
        target: &Path,
        relative: PathBuf,
        mode: Option<u32>,
    ) -> Result<(), CoreError> {
        if self.planned.len() >= MAX_FILES {
            return Err(Self::refuse(
                path,
                "the pack would hold more than 10,000 files",
            ));
        }
        let bytes = self
            .reader
            .fsops_read_capped(target, MAX_FILE_BYTES)
            .map_err(|e| CoreError::io(path, e))?;
        let total: u64 = self
            .planned
            .iter()
            .map(|f| f.bytes.len() as u64)
            .sum::<u64>()
            + bytes.len() as u64;
        if total > MAX_TOTAL_BYTES {
            return Err(Self::refuse(path, "the pack would hold more than 256 MB"));
        }
        let executable = mode.is_some_and(|mode| mode & 0o111 != 0);
        self.planned.push(PlannedFile {
            relative,
            bytes,
            mode: if executable { 0o755 } else { 0o644 },
        });
        Ok(())
    }
}

/// Binds a handle to `dir` ([`ScopeFs::fsops_bind_root`]) and checks that
/// it holds the real folder the path named a moment before: a folder or an
/// ancestor swapped for a link between the check and the open would bind
/// the wrong folder. Returns the view and the folder's device and inode.
fn bind_checked<'a>(
    fs: &'a dyn ScopeFs,
    dir: &Path,
    what: &str,
) -> Result<(Option<Box<dyn ScopeFs + 'a>>, (u64, u64)), CoreError> {
    let changed = || Walk::refuse(dir, &format!("{what} changed while the export opened it"));
    if !fs
        .symlink_metadata(dir)
        .is_ok_and(|facts| facts.kind == FileKind::Dir)
    {
        return Err(changed());
    }
    let expected = fs.fsops_device_inode(dir).map_err(|e| CoreError::io(dir, e))?;
    let bound = fs.fsops_bind_root(dir).map_err(|e| CoreError::io(dir, e))?;
    if let Some(view) = &bound {
        let opened = view.fsops_root_identity().map_err(|e| CoreError::io(dir, e))?;
        if opened.is_some_and(|opened| opened != expected) {
            return Err(changed());
        }
    }
    Ok((bound, expected))
}

/// Writes the pack through a handle bound to `dir`, so no write can leave it,
/// even through a folder swapped for a link after the checks: creates `dir`
/// when it does not exist, checks again that it is a real, empty folder,
/// then creates every folder and file with create-new. On a failure the
/// error lists what was written and what to remove: the folder when this
/// export created it, else only the listed files.
fn write_pack(fs: &dyn ScopeFs, dir: &Path, planned: &[PlannedFile]) -> Result<(), CoreError> {
    let created = fs.symlink_metadata(dir).is_err();
    if created {
        fs.fsops_create_dir(dir)
            .map_err(|e| CoreError::io(dir, e))?;
    }
    let (bound, identity) = bind_checked(fs, dir, "the pack folder")?;
    let writer = bound.as_deref().unwrap_or(fs);
    // The listing is by path, so it counts only when the path still names
    // the folder the handle holds.
    let empty = fs
        .read_dir(dir)
        .map_err(|e| CoreError::io(dir, e))?
        .is_empty();
    let same = fs.fsops_device_inode(dir).ok() == Some(identity);
    if !same || !empty {
        return Err(Walk::refuse(
            dir,
            "the pack folder changed while the export read the skills; choose a new or empty folder",
        ));
    }
    let mut written: Vec<PathBuf> = Vec::new();
    let stopped = |path: &Path, e: std::io::Error, written: &[PathBuf]| {
        let undo = if created {
            format!("remove {}", dir.display())
        } else if written.is_empty() {
            "nothing was written".to_string()
        } else {
            format!(
                "remove what it wrote in {}: {}",
                dir.display(),
                written
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        CoreError::io(path, e).with_detail(format!("the export stopped here; {undo}"))
    };
    let mut made: BTreeSet<PathBuf> = BTreeSet::new();
    for file in planned {
        let mut parents: Vec<&Path> = file
            .relative
            .ancestors()
            .skip(1)
            .filter(|p| !p.as_os_str().is_empty())
            .collect();
        parents.reverse();
        for parent in parents {
            if made.insert(parent.to_path_buf()) {
                let folder = dir.join(parent);
                writer
                    .fsops_create_dir(&folder)
                    .map_err(|e| stopped(&folder, e, &written))?;
                written.push(parent.to_path_buf());
            }
        }
        let path = dir.join(&file.relative);
        writer
            .fsops_write_new_file_with_mode(&path, &file.bytes, file.mode)
            .map_err(|e| stopped(&path, e, &written))?;
        written.push(file.relative.clone());
    }
    Ok(())
}

/// `word` as one shell word: as is when it holds only safe characters, else
/// in single quotes. The install lines are meant to be pasted into a shell.
fn shell_word(word: &str) -> String {
    let safe = !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:@#+=,%".contains(c));
    if safe {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}

/// `npx skills add <source> --skill a --skill b`, for every skill at once.
fn install_line(source: &str, skills: &[String]) -> String {
    let mut line = format!("npx skills add {}", shell_word(source));
    for skill in skills {
        line.push_str(" --skill ");
        line.push_str(&shell_word(skill));
    }
    line
}

fn readme(source: &str, skills: &[String], install_command: &str) -> String {
    use std::fmt::Write as _;
    let mut text = format!(
        "# Skills\n\nAgent skills, one folder each under `skills/`.\n\n## Install\n\n\
         Every skill:\n\n```bash\n{install_command}\n```\n\nOne skill:\n\n```bash\n"
    );
    // Writing to a `String` cannot fail.
    for skill in skills {
        let _ = writeln!(
            text,
            "{}",
            install_line(source, std::slice::from_ref(skill))
        );
    }
    text.push_str("```\n");
    if source == SOURCE_PLACEHOLDER {
        let _ = writeln!(
            text,
            "\nReplace `{SOURCE_PLACEHOLDER}` with this repository once it is pushed."
        );
    }
    text.push_str("\n## Skills\n\n");
    for skill in skills {
        let _ = writeln!(text, "- [`{skill}`](skills/{skill}/SKILL.md)");
    }
    text
}
