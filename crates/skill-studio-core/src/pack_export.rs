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
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dto::{DeploymentDto, Inventory};
use crate::error::{CoreError, ErrorCode, ErrorEntry};
use crate::identity::{RootKind, RootScope};
use crate::ops::{OpStatus, Operation, Outcome};
use crate::ports::{DirHandle, FileKind, OpContext, Runtime, ScopeFs};

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
        let target = check_request(fs, req)?;
        let scope = match &req.project_path {
            Some(path) => crate::pack::selected_scopes(rt, Some(path))?.remove(0).0,
            None => RootScope::Global,
        };
        let inventory = crate::pack::scan(rt, ctx)?;
        let mut skills = Vec::new();
        let mut planned = Vec::new();
        let mut walk = Walk {
            open: Vec::new(),
            planned: &mut planned,
            total: 0,
        };
        for name in &req.skills {
            ctx.checkpoint()?;
            let copy = live_copy(&inventory, name, &scope)?;
            let before = walk.planned.len();
            collect(fs, &copy.path, &Path::new("skills").join(name), &mut walk)?;
            skills.push(PackExportSkill {
                skill: name.clone(),
                from: copy.path.clone(),
                files: walk.planned.len() - before,
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
            target.write(&planned)?;
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
/// that holds anything. Returns the pack folder's held parent, which
/// [`PackTarget::write`] checks again and writes through.
fn check_request(fs: &dyn ScopeFs, req: &PackExportRequest) -> Result<PackTarget, CoreError> {
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
    PackTarget::open(fs, &req.dir)
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

/// Opens `path` through the port's handles ([`ScopeFs::fsops_open_dir`]).
fn open_dir(fs: &dyn ScopeFs, path: &Path) -> Result<Box<dyn DirHandle>, CoreError> {
    match fs.fsops_open_dir(path) {
        Ok(Some(handle)) => Ok(handle),
        Ok(None) => Err(CoreError::new(
            ErrorCode::Unsupported,
            "this file system cannot hold folders open, so the export cannot read safely",
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotADirectory => Err(changed(path)),
        Err(e) => Err(CoreError::io(path, e)),
    }
}

/// A folder or file that turned into something else between the moment the
/// export saw it and the moment it opened it.
fn changed(path: &Path) -> CoreError {
    CoreError::new(
        ErrorCode::InvalidRequest,
        format!(
            "{} changed while the export read it; run the export again",
            path.display()
        ),
    )
    .at(path)
}

fn refuse(path: &Path, message: &str) -> CoreError {
    CoreError::new(ErrorCode::InvalidRequest, message.to_string()).at(path)
}

/// A folder held open, shared by the walk and the link resolver.
type Held = Rc<dyn DirHandle>;

/// Most links one entry may pass through, so a chain of links that loops
/// fails instead of running forever.
const MAX_LINK_HOPS: usize = 40;

const NOT_A_FILE: &str = "the skill holds something that is not a file or a folder";
const LEAVES: &str = "a link in the skill leaves its folder; the pack would not hold what it names";

/// Adds every file under `folder` to `planned`, at `into/<relative path>`.
/// The folder itself may be a link (a linked install): its resolved folder
/// is the boundary. The walk holds each folder open and reads every entry
/// through the folder that holds it, never by path, so what it checks is
/// what it reads: an entry that turns into a link after the listing fails
/// the open instead of redirecting it. A link inside the skill is followed
/// through the folders already held ([`follow`]) and is refused when it
/// leaves the skill or loops. `.git` is left out.
fn collect(
    fs: &dyn ScopeFs,
    folder: &Path,
    into: &Path,
    walk: &mut Walk<'_>,
) -> Result<(), CoreError> {
    let resolved = match fs.canonicalize(folder) {
        Ok(resolved) => resolved,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(refuse(
                folder,
                "the skill is a broken link; fix or remove it before you pack it",
            ))
        }
        Err(e) => return Err(CoreError::io(folder, e)),
    };
    let root: Held = Rc::from(open_dir(fs, &resolved)?);
    let identity = root.identity().map_err(|e| CoreError::io(&resolved, e))?;
    walk.open = vec![identity];
    walk.dir(&[root], &resolved, into)
}

/// What an entry of the skill holds once opened.
enum Reached {
    /// A folder, and the held folders from the skill folder down to it.
    Dir(Vec<Held>),
    /// A regular file's bytes and permission bits.
    File(Vec<u8>, u32),
}

/// Follows the link target `target`, written in the last folder of `chain`
/// (the held folders from the skill folder down), one name at a time
/// through held folders: `..` steps back to the held parent, a name opens
/// without following a link, and a link on the way is followed the same
/// way. Nothing is resolved by path, so a folder swapped for a link after
/// it was opened cannot redirect the result. `link` names the entry, for
/// messages.
fn follow(
    chain: &[Held],
    target: &Path,
    link: &Path,
    hops: &mut usize,
) -> Result<Reached, CoreError> {
    *hops += 1;
    if *hops > MAX_LINK_HOPS {
        return Err(refuse(
            link,
            "a link in the skill loops back into its own folder",
        ));
    }
    let names_nothing = || refuse(link, "a link in the skill names nothing");
    let io = |e: std::io::Error| match e.kind() {
        std::io::ErrorKind::NotADirectory | std::io::ErrorKind::InvalidInput => changed(link),
        std::io::ErrorKind::NotFound => names_nothing(),
        _ => CoreError::io(link, e),
    };
    let mut chain = chain.to_vec();
    let names: Vec<Component<'_>> = target.components().collect();
    // `Path::components` drops a trailing `/` and `/.`, but the file system
    // then needs a folder: `run.sh/` names nothing when `run.sh` is a file.
    let raw = target.as_os_str().as_encoded_bytes();
    let wants_folder = raw.ends_with(b"/") || raw.ends_with(b"/.");
    for (index, component) in names.iter().enumerate() {
        let last = index + 1 == names.len();
        match component {
            Component::RootDir | Component::Prefix(_) => return Err(refuse(link, LEAVES)),
            Component::CurDir => {}
            Component::ParentDir => {
                if chain.len() == 1 {
                    return Err(refuse(link, LEAVES));
                }
                chain.pop();
            }
            Component::Normal(name) => {
                let name = name.to_str().ok_or_else(|| refuse(link, NOT_A_FILE))?;
                // The chain never empties: `..` stops at the skill folder.
                let Some(here) = chain.last().cloned() else {
                    return Err(refuse(link, LEAVES));
                };
                match here.kind(name).map_err(io)? {
                    FileKind::Dir => chain.push(Rc::from(here.open_dir(name).map_err(io)?)),
                    FileKind::File if last && !wants_folder => {
                        let (bytes, mode) = here.read_file(name, MAX_FILE_BYTES).map_err(io)?;
                        return Ok(Reached::File(bytes, mode));
                    }
                    FileKind::Symlink => {
                        let next = here.read_link(name).map_err(io)?;
                        match follow(&chain, &next, link, hops)? {
                            Reached::Dir(reached) => chain = reached,
                            file @ Reached::File(..) if last && !wants_folder => return Ok(file),
                            Reached::File(..) => return Err(names_nothing()),
                        }
                    }
                    FileKind::File => return Err(names_nothing()),
                    FileKind::Other => return Err(refuse(link, NOT_A_FILE)),
                }
            }
        }
    }
    Ok(Reached::Dir(chain))
}

/// Walk state shared across the skills of one export.
struct Walk<'a> {
    /// Device and inode of each folder on the current path, to refuse a link
    /// back into one.
    open: Vec<(u64, u64)>,
    planned: &'a mut Vec<PlannedFile>,
    /// Bytes in `planned`.
    total: u64,
}

impl Walk<'_> {
    /// Walks the last folder of `chain`, the held folders from the skill
    /// folder down; `shown` is its path, for messages only.
    fn dir(&mut self, chain: &[Held], shown: &Path, into: &Path) -> Result<(), CoreError> {
        let Some(dir) = chain.last() else {
            return Ok(());
        };
        let mut entries = dir.entries().map_err(|e| CoreError::io(shown, e))?;
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        for entry in entries {
            if entry.name == ".git" {
                continue;
            }
            let path = shown.join(&entry.name);
            // An error here means the entry is no longer the kind the
            // listing gave: a folder or file became a link, or the reverse.
            let stale = |e: std::io::Error| match e.kind() {
                std::io::ErrorKind::NotADirectory | std::io::ErrorKind::InvalidInput => {
                    changed(&path)
                }
                _ => CoreError::io(&path, e),
            };
            let reached = match entry.kind {
                FileKind::Dir => {
                    let child: Held = Rc::from(dir.open_dir(&entry.name).map_err(stale)?);
                    let mut deeper = chain.to_vec();
                    deeper.push(child);
                    Reached::Dir(deeper)
                }
                FileKind::File => {
                    let (bytes, mode) =
                        dir.read_file(&entry.name, MAX_FILE_BYTES).map_err(stale)?;
                    Reached::File(bytes, mode)
                }
                FileKind::Symlink => {
                    let target = dir.read_link(&entry.name).map_err(stale)?;
                    follow(chain, &target, &path, &mut 0)?
                }
                FileKind::Other => return Err(refuse(&path, NOT_A_FILE)),
            };
            let target = into.join(&entry.name);
            match reached {
                Reached::Dir(deeper) => {
                    let Some(held) = deeper.last() else {
                        return Err(refuse(&path, LEAVES));
                    };
                    let identity = held.identity().map_err(|e| CoreError::io(&path, e))?;
                    if self.open.contains(&identity) {
                        return Err(refuse(
                            &path,
                            "a link in the skill loops back into its own folder",
                        ));
                    }
                    self.open.push(identity);
                    self.dir(&deeper, &path, &target)?;
                    self.open.pop();
                }
                Reached::File(bytes, mode) => self.file(&path, target, bytes, mode)?,
            }
        }
        Ok(())
    }

    fn file(
        &mut self,
        path: &Path,
        relative: PathBuf,
        bytes: Vec<u8>,
        mode: u32,
    ) -> Result<(), CoreError> {
        if self.planned.len() >= MAX_FILES {
            return Err(refuse(path, "the pack would hold more than 10,000 files"));
        }
        self.total += bytes.len() as u64;
        if self.total > MAX_TOTAL_BYTES {
            return Err(refuse(path, "the pack would hold more than 256 MB"));
        }
        self.planned.push(PlannedFile {
            relative,
            bytes,
            mode: if mode & 0o111 != 0 { 0o755 } else { 0o644 },
        });
        Ok(())
    }
}

/// Where the pack goes: the pack folder's parent, held open from the first
/// check to the last write, and the folder's name in it. The user's path is
/// resolved once, so a parent swapped for a link later cannot move the
/// write.
struct PackTarget {
    parent: Box<dyn DirHandle>,
    name: String,
    /// The path the request named, for messages.
    dir: PathBuf,
}

/// The pack folder, held open.
struct PackFolder {
    folder: Box<dyn DirHandle>,
    /// This export created it.
    created: bool,
}

impl PackTarget {
    /// Opens the parent of `dir` by its link-free path and checks the
    /// folder: absent, or an empty folder.
    fn open(fs: &dyn ScopeFs, dir: &Path) -> Result<PackTarget, CoreError> {
        let (Some(parent), Some(name)) = (dir.parent(), dir.file_name().and_then(|n| n.to_str()))
        else {
            return Err(refuse(
                dir,
                "give the pack folder a name inside a parent folder",
            ));
        };
        let parent = match fs.canonicalize(parent) {
            Ok(parent) => parent,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(refuse(
                    parent,
                    "the pack folder's parent folder does not exist",
                ))
            }
            Err(e) => return Err(CoreError::io(parent, e)),
        };
        let target = PackTarget {
            parent: open_dir(fs, &parent)?,
            name: name.to_string(),
            dir: dir.to_path_buf(),
        };
        target.folder(false)?;
        Ok(target)
    }

    /// The pack folder, held open and empty, created when `create` and it
    /// does not exist; `None` when it does not exist and not `create`.
    fn folder(&self, create: bool) -> Result<Option<PackFolder>, CoreError> {
        let invalid = |message: &str| refuse(&self.dir, message);
        let io = |e| CoreError::io(&self.dir, e);
        let (folder, created) = match self.parent.open_dir(&self.name) {
            Ok(folder) => (folder, false),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && !create => return Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                (self.parent.create_dir(&self.name).map_err(io)?, true)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotADirectory => {
                return Err(invalid(
                    "the pack folder path is a file or a link, not a folder",
                ))
            }
            Err(e) => return Err(io(e)),
        };
        if !folder.entries().map_err(io)?.is_empty() {
            return Err(invalid(
                "the pack folder is not empty; choose a new or empty folder",
            ));
        }
        Ok(Some(PackFolder { folder, created }))
    }

    /// Writes the pack into the folder [`Self::folder`] holds, so no write
    /// can leave it; every folder and file is created with create-new. On a
    /// failure the error lists what was written and what to remove: the
    /// folder when this export created it, else only the listed files.
    fn write(&self, planned: &[PlannedFile]) -> Result<(), CoreError> {
        let dir = &self.dir;
        let Some(PackFolder {
            folder: root,
            created,
        }) = self.folder(true)?
        else {
            return Err(changed(dir));
        };
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
            CoreError::io(dir.join(path), e).with_detail(format!("the export stopped here; {undo}"))
        };
        // Folders this export created, by path relative to `dir`, held open.
        let mut made: BTreeMap<PathBuf, Box<dyn DirHandle>> = BTreeMap::new();
        for file in planned {
            let mut at = PathBuf::new();
            let parts: Vec<&std::ffi::OsStr> = file.relative.iter().collect();
            let Some((leaf, folders)) = parts.split_last() else {
                continue;
            };
            for folder in folders {
                let parent = at.clone();
                at.push(folder);
                if made.contains_key(&at) {
                    continue;
                }
                let holder = made.get(&parent).map_or(root.as_ref(), AsRef::as_ref);
                let opened = holder
                    .create_dir(&folder.to_string_lossy())
                    .map_err(|e| stopped(&at, e, &written))?;
                written.push(at.clone());
                made.insert(at.clone(), opened);
            }
            let wrote = made
                .get(&at)
                .map_or(root.as_ref(), AsRef::as_ref)
                .write_new_file(&leaf.to_string_lossy(), &file.bytes, file.mode);
            // Only `AlreadyExists` proves the create made nothing; after any
            // other failure a partial file may be there, so it is listed.
            if !matches!(&wrote, Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists) {
                written.push(file.relative.clone());
            }
            wrote.map_err(|e| stopped(&file.relative, e, &written))?;
        }
        Ok(())
    }
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
