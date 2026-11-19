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
    /// Take each skill's copy from this tracked project. Absent: the home.
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
/// The folder itself may be a link (a linked install). Inside it, a link is
/// copied as the file or folder it names when that stays inside the skill
/// folder, and refused when it leaves it. `.git` is left out.
fn collect(
    fs: &dyn ScopeFs,
    folder: &Path,
    into: &Path,
    planned: &mut Vec<PlannedFile>,
) -> Result<(), CoreError> {
    let root = fs
        .canonicalize(folder)
        .map_err(|e| CoreError::io(folder, e))?;
    walk(fs, &root, &root, into, planned)
}

fn walk(
    fs: &dyn ScopeFs,
    root: &Path,
    dir: &Path,
    into: &Path,
    planned: &mut Vec<PlannedFile>,
) -> Result<(), CoreError> {
    let mut entries = fs.read_dir(dir).map_err(|e| CoreError::io(dir, e))?;
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    for entry in entries {
        if entry.name == ".git" {
            continue;
        }
        let path = dir.join(&entry.name);
        let target = if entry.kind == FileKind::Symlink {
            let resolved = fs
                .canonicalize(&path)
                .map_err(|e| CoreError::io(&path, e))?;
            if !resolved.starts_with(root) {
                return Err(CoreError::new(
                    ErrorCode::InvalidRequest,
                    "a link in the skill leaves its folder; the pack would not hold what it names",
                )
                .at(&path));
            }
            resolved
        } else {
            path.clone()
        };
        let facts = fs
            .symlink_metadata(&target)
            .map_err(|e| CoreError::io(&target, e))?;
        let relative = into.join(&entry.name);
        match facts.kind {
            FileKind::Dir => walk(fs, root, &target, &relative, planned)?,
            FileKind::File => {
                if facts.len > MAX_FILE_BYTES {
                    return Err(CoreError::new(
                        ErrorCode::InvalidRequest,
                        "a file in the skill is larger than 16 MB",
                    )
                    .at(&path));
                }
                let bytes = fs
                    .read_capped(&target, MAX_FILE_BYTES)
                    .map_err(|e| CoreError::io(&target, e))?;
                let executable = facts.mode.is_some_and(|mode| mode & 0o111 != 0);
                planned.push(PlannedFile {
                    relative,
                    bytes,
                    mode: if executable { 0o755 } else { 0o644 },
                });
            }
            FileKind::Symlink | FileKind::Other => {
                return Err(CoreError::new(
                    ErrorCode::InvalidRequest,
                    "the skill holds something that is not a file or a folder",
                )
                .at(&path));
            }
        }
    }
    Ok(())
}

/// Creates `dir` when it does not exist, then every folder and file, each
/// with create-new, so an existing file stops the export instead of being
/// overwritten.
fn write_pack(fs: &dyn ScopeFs, dir: &Path, planned: &[PlannedFile]) -> Result<(), CoreError> {
    let stopped = |path: &Path, e: std::io::Error| {
        CoreError::io(path, e).with_detail(format!(
            "the export stopped here; remove {} and export again",
            dir.display()
        ))
    };
    if fs.symlink_metadata(dir).is_err() {
        fs.fsops_create_dir(dir).map_err(|e| stopped(dir, e))?;
    }
    let mut made: BTreeSet<PathBuf> = BTreeSet::new();
    for file in planned {
        let path = dir.join(&file.relative);
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
                fs.fsops_create_dir(&folder)
                    .map_err(|e| stopped(&folder, e))?;
            }
        }
        fs.fsops_write_new_file_with_mode(&path, &file.bytes, file.mode)
            .map_err(|e| stopped(&path, e))?;
    }
    Ok(())
}

/// `npx skills add <source> --skill a --skill b`, for every skill at once.
fn install_line(source: &str, skills: &[String]) -> String {
    let mut line = format!("npx skills add {source}");
    for skill in skills {
        line.push_str(" --skill ");
        line.push_str(skill);
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
        let _ = writeln!(text, "npx skills add {source} --skill {skill}");
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
