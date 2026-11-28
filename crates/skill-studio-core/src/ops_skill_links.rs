//! The `skill_links` operation: links between installed skills, resolved for
//! each agent that has a copy of the linking skill
//! (`docs/spec-skill-links.md` sections 3 and 4).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dto::{DeploymentDto, InstalledSkillDto, ScanRequest};
use crate::error::CoreError;
use crate::harness::{deployment_visible_to, HarnessFacts};
use crate::identity::{AgentId, RootKind, RootScope};
use crate::listing_cost::disables_reader;
use crate::ops::Operation;
use crate::ports::{FileKind, OpContext, Runtime, ScopeFs, ScopedReads};
use crate::skill_links::{detect_links, LinkEvidence, LinkStrength, SkillLink, SkillText};

const MAX_FILE_BYTES: u64 = 256 * 1024;
const MAX_FILES: usize = 200;
const MAX_DEPTH: usize = 4;

/// What to report.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct SkillLinksRequest {
    /// Only links from or to this skill.
    pub skill: Option<String>,
    /// Only this agent's view.
    pub agent: Option<AgentId>,
    /// Only Needs and Hands off links that do not work for some agent.
    pub missing_only: bool,
}

/// Whether a link works for one agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LinkState {
    /// The agent can open the path or run the skill by name.
    Ok,
    /// No copy of the target anywhere in the scope.
    Missing,
    /// Not found by the name the link uses, but a copy installed under a
    /// prefixed name (`matt-grilling` for `grilling`) is.
    Renamed,
    /// The agent has a copy, but it is parked or turned off for it.
    Off,
    /// A copy exists only where this agent does not read it.
    Elsewhere,
}

/// One agent's view of a link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AgentLinkState {
    /// The agent with a working copy of the linking skill.
    pub agent: AgentId,
    /// Whether the link works for it.
    pub state: LinkState,
}

/// A link with its state for each agent that loads the linking skill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ResolvedLink {
    /// The skill whose text names the other.
    pub from: String,
    /// The skill it names.
    pub to: String,
    /// How much `from` depends on `to`.
    pub strength: LinkStrength,
    /// For a path link, the path as written.
    pub path: Option<String>,
    /// The line behind the link.
    #[schemars(extend("x-untrusted" = true))]
    pub evidence: LinkEvidence,
    /// One entry per agent that loads a copy of `from` holding this link.
    pub agents: Vec<AgentLinkState>,
}

/// Links between installed skills.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SkillLinksReport {
    /// Links, sorted by `from` then `to`.
    pub links: Vec<ResolvedLink>,
    /// Skills that list most of their pack. Their list entries never alert.
    pub indexes: Vec<String>,
    /// Needs and Hands off links that do not work, counted once per agent.
    pub broken: usize,
    /// Skills whose folder could not be read: their links are not known,
    /// which is not the same as having none.
    pub not_known: Vec<String>,
}

/// Reads every installed skill's Markdown, finds its links, and resolves
/// each for every agent that loads the linking skill. Writes nothing.
pub fn skill_links(
    rt: &Runtime,
    ctx: &OpContext,
    req: &SkillLinksRequest,
) -> Result<SkillLinksReport, CoreError> {
    rt.run(Operation::SkillLinks, ctx, || {
        let inventory = crate::ops::scan(rt, ctx, &ScanRequest::default())?;
        let fs = ScopedReads::new(rt.ports.fs.as_ref(), &rt.scope);
        // Every skill the scan saw, parked ones included, so a target that is
        // parked or gone from one root is still recognized by name.
        let names: BTreeSet<String> = inventory.skills.iter().map(|s| s.name.0.clone()).collect();
        let mut not_known = BTreeSet::new();
        let mut texts: BTreeMap<PathBuf, SkillText> = BTreeMap::new();
        let mut primary: Vec<SkillText> = Vec::new();
        for skill in &inventory.skills {
            ctx.checkpoint()?;
            for copy in skill.deployments.iter().filter(|d| readable(d)) {
                let real = fs
                    .canonicalize(&copy.path)
                    .unwrap_or_else(|_| copy.path.clone());
                if texts.contains_key(&real) {
                    continue;
                }
                let Some((files, complete)) = read_markdown(&fs, &copy.path) else {
                    not_known.insert(skill.name.0.clone());
                    continue;
                };
                if !complete {
                    not_known.insert(skill.name.0.clone());
                }
                let text = SkillText {
                    name: skill.name.0.clone(),
                    folder: folder_name(&copy.path),
                    pack: pack_of(skill),
                    files,
                };
                if !primary.iter().any(|t| t.name == text.name) {
                    primary.push(text.clone());
                }
                texts.insert(real, text);
            }
        }
        let detected = detect_links(&primary, &names);
        // Links in one copy: the copy an agent loads can differ from the first
        // one read, so each distinct copy is judged in its own text.
        let mut by_copy: BTreeMap<PathBuf, Vec<SkillLink>> = BTreeMap::new();
        let mut links_in = |real: &PathBuf| -> Vec<SkillLink> {
            by_copy
                .entry(real.clone())
                .or_insert_with(|| {
                    let Some(text) = texts.get(real) else {
                        return Vec::new();
                    };
                    let first = primary.iter().find(|t| t.name == text.name);
                    if first.is_some_and(|t| t.files == text.files) {
                        return detected
                            .links
                            .iter()
                            .filter(|l| l.from == text.name)
                            .cloned()
                            .collect();
                    }
                    let set: Vec<SkillText> = primary
                        .iter()
                        .map(|t| {
                            if t.name == text.name {
                                text.clone()
                            } else {
                                t.clone()
                            }
                        })
                        .collect();
                    detect_links(&set, &names)
                        .links
                        .into_iter()
                        .filter(|l| l.from == text.name)
                        .collect()
                })
                .clone()
        };
        let agents: Vec<&HarnessFacts> = rt
            .ports
            .catalog
            .facts
            .iter()
            .filter(|facts| req.agent.as_ref().is_none_or(|agent| agent == &facts.id))
            .collect();
        let mut grouped: BTreeMap<(String, String), ResolvedLink> = BTreeMap::new();
        for from in &inventory.skills {
            for facts in &agents {
                let Some(copy) = from.deployments.iter().find(|d| loads(d, facts)) else {
                    continue;
                };
                let real = fs
                    .canonicalize(&copy.path)
                    .unwrap_or_else(|_| copy.path.clone());
                for link in links_in(&real) {
                    let wanted = req
                        .skill
                        .as_ref()
                        .is_none_or(|s| *s == link.from || *s == link.to);
                    if !wanted {
                        continue;
                    }
                    let state = resolve(&fs, copy, &link, &inventory.skills, facts);
                    grouped
                        .entry((link.from.clone(), link.to.clone()))
                        .or_insert_with(|| ResolvedLink {
                            from: link.from.clone(),
                            to: link.to.clone(),
                            strength: link.strength,
                            path: link.path.clone(),
                            evidence: link.evidence.clone(),
                            agents: Vec::new(),
                        })
                        .agents
                        .push(AgentLinkState {
                            agent: facts.id.clone(),
                            state,
                        });
                }
            }
        }
        let mut report = SkillLinksReport {
            indexes: detected.indexes,
            not_known: not_known.into_iter().collect(),
            ..SkillLinksReport::default()
        };
        for link in grouped.into_values() {
            let strong = matches!(link.strength, LinkStrength::Needs | LinkStrength::HandsOff);
            let broken = link
                .agents
                .iter()
                .filter(|a| a.state != LinkState::Ok)
                .count();
            if strong {
                report.broken += broken;
            }
            if !req.missing_only || (strong && broken > 0) {
                report.links.push(link);
            }
        }
        Ok(report)
    })
}

/// The pack a skill came from, for the index rule. Skills with no known
/// source share no pack: grouping every manual install together would let
/// unrelated skills hide an index.
fn pack_of(skill: &InstalledSkillDto) -> Option<String> {
    (!skill.source.is_empty() && skill.source != "manual").then(|| skill.source.clone())
}

fn readable(d: &DeploymentDto) -> bool {
    !d.symlink_is_broken && d.root.kind != RootKind::Parked
}

/// The agent loads this copy: it reads its root, and the copy is neither
/// parked nor turned off for it.
fn loads(d: &DeploymentDto, facts: &HarnessFacts) -> bool {
    readable(d) && deployment_visible_to(d, facts) && !off_for(d, facts)
}

/// Turned off for this agent: by a setting that agent owns, by Skill Studio
/// moving it aside, or as a disabled reader of a shared copy.
fn off_for(d: &DeploymentDto, facts: &HarnessFacts) -> bool {
    d.disabled_by
        .is_some_and(|by| disables_reader(by, &facts.id))
        || d.disabled_readers.contains(&facts.id)
}

fn same_scope(target: &RootScope, from: &RootScope) -> bool {
    *target == RootScope::Global || target == from
}

fn resolve(
    fs: &dyn ScopeFs,
    from: &DeploymentDto,
    link: &SkillLink,
    skills: &[InstalledSkillDto],
    facts: &HarnessFacts,
) -> LinkState {
    if let Some(path) = &link.path {
        // The operating system resolves `..` after following a linked skill
        // folder, so the path is joined to the real folder, as an agent's
        // file read would be.
        let opened = fs.canonicalize(&from.path).ok().and_then(|real| {
            let dir = Path::new(&link.evidence.file)
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default();
            fs.canonicalize(&real.join(dir).join(path)).ok()
        });
        if opened.is_some() {
            return LinkState::Ok;
        }
    }
    let scoped = |d: &&DeploymentDto| same_scope(&d.root.scope, &from.root.scope);
    let loaded = |skill: &InstalledSkillDto| {
        skill
            .deployments
            .iter()
            .filter(scoped)
            .any(|d| loads(d, facts))
    };
    let target = skills.iter().find(|s| s.name.0 == link.to);
    if link.path.is_none() {
        if target.is_some_and(loaded) {
            return LinkState::Ok;
        }
        let suffix = format!("-{}", link.to);
        if skills
            .iter()
            .any(|s| s.name.0.ends_with(&suffix) && loaded(s))
        {
            return LinkState::Renamed;
        }
    }
    let Some(target) = target.filter(|t| t.deployments.iter().any(|d| !d.symlink_is_broken)) else {
        return LinkState::Missing;
    };
    let held = target.deployments.iter().filter(scoped).any(|d| {
        d.root.kind == RootKind::Parked || (deployment_visible_to(d, facts) && off_for(d, facts))
    });
    if held {
        LinkState::Off
    } else if target.deployments.is_empty() {
        LinkState::Missing
    } else {
        LinkState::Elsewhere
    }
}

fn folder_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// The `.md` files of one skill folder, as `(path with /, text)`, and
/// whether every file was read in full. `None` when the folder itself cannot
/// be read.
fn read_markdown(fs: &dyn ScopeFs, root: &Path) -> Option<(Vec<(String, String)>, bool)> {
    let mut out = Vec::new();
    let mut complete = true;
    let mut stack = vec![(root.to_path_buf(), String::new(), 0usize)];
    let mut first = true;
    while let Some((dir, rel, depth)) = stack.pop() {
        let entries = match fs.read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) if first => return None,
            Err(_) => {
                complete = false;
                continue;
            }
        };
        first = false;
        for entry in entries {
            if entry.name.starts_with('.') || entry.name == "node_modules" {
                continue;
            }
            let path = dir.join(&entry.name);
            let name = if rel.is_empty() {
                entry.name.clone()
            } else {
                format!("{rel}/{}", entry.name)
            };
            match entry.kind {
                FileKind::Dir if depth < MAX_DEPTH => stack.push((path, name, depth + 1)),
                FileKind::Dir => complete = false,
                FileKind::File if is_markdown(&entry.name) => {
                    if out.len() >= MAX_FILES {
                        complete = false;
                        continue;
                    }
                    match fs.read_capped(&path, MAX_FILE_BYTES + 1) {
                        Ok(bytes) if bytes.len() as u64 <= MAX_FILE_BYTES => {
                            out.push((name, String::from_utf8_lossy(&bytes).into_owned()));
                        }
                        _ => complete = false,
                    }
                }
                _ => {}
            }
        }
    }
    out.sort();
    Some((out, complete))
}

fn is_markdown(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
}
