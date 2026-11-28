//! The `skill_links` operation: links between installed skills, resolved for
//! each agent that has a copy of the linking skill
//! (`docs/spec-skill-links.md` sections 3 and 4).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dto::{Completeness, DeploymentDto, InstalledSkillDto, ScanRequest};
use crate::error::CoreError;
use crate::harness::{deployment_visible_to, HarnessFacts};
use crate::identity::{AgentId, ProjectRef, RootKind, RootScope};
use crate::listing_cost::disables_reader;
use crate::ops::Operation;
use crate::pack::LinkRecords;
use crate::ports::{FileKind, OpContext, Runtime, ScopeFs, ScopedReads};
use crate::skill_links::{LinkEvidence, LinkStrength, LinkVocabulary, SkillLink, SkillText};

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
    /// The available evidence cannot establish which file the agent reads.
    NotKnown,
}

/// One agent's view of a link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AgentLinkState {
    /// The agent with a working copy of the linking skill.
    pub agent: AgentId,
    /// The working scope in which the agent uses the skill.
    pub scope: RootScope,
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
    /// Global and project contexts covered by this lookup.
    pub scopes: Vec<RootScope>,
    /// Links, sorted by `from` then `to`.
    pub links: Vec<ResolvedLink>,
    /// Literal command or bold name with no target record. Excluded from link and warning counts.
    pub unverified: Vec<ResolvedLink>,
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
        let records = crate::pack::link_records(rt, &fs)?;
        let mut names = records.names.clone();
        names.extend(inventory.skills.iter().map(|s| s.name.0.clone()));
        names.extend(inventory.lock_only.iter().map(|s| s.name.0.clone()));
        let mut not_known = BTreeSet::new();
        let mut texts = BTreeMap::new();
        for skill in &inventory.skills {
            ctx.checkpoint()?;
            for copy in skill.deployments.iter().filter(|d| readable(d)) {
                let real = fs
                    .canonicalize(&copy.path)
                    .unwrap_or_else(|_| copy.path.clone());
                let key = (skill.name.0.clone(), real.clone());
                if texts.contains_key(&key) {
                    continue;
                }
                if !rt.scope.contains(&real) {
                    not_known.insert(skill.name.0.clone());
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
                texts.insert(key, text);
            }
        }
        let vocabulary = LinkVocabulary::new(texts.values(), &names);
        let mut indexes = BTreeSet::new();
        let mut by_copy = BTreeMap::new();
        for (path, text) in &texts {
            let detected = vocabulary.detect(text);
            indexes.extend(detected.indexes);
            let refs = detected
                .links
                .into_iter()
                .map(|l| (false, l))
                .chain(detected.unverified.into_iter().map(|l| (true, l)))
                .collect::<Vec<_>>();
            by_copy.insert(path, refs);
        }
        let agents: Vec<&HarnessFacts> = rt
            .ports
            .catalog
            .facts
            .iter()
            .filter(|facts| req.agent.as_ref().is_none_or(|agent| agent == &facts.id))
            .collect();
        // One row per distinct way a copy names the target, so an agent whose
        // copy runs it is not hidden behind another copy that only mentions it.
        let scopes: Vec<_> = std::iter::once(RootScope::Global)
            .chain(
                inventory
                    .projects
                    .iter()
                    .map(|p| RootScope::Project(ProjectRef(p.clone()))),
            )
            .collect();
        let mut grouped = BTreeMap::new();
        for from in &inventory.skills {
            for facts in &agents {
                for scope in &scopes {
                    let copies: Vec<_> = from
                        .deployments
                        .iter()
                        .filter(|d| same_scope(&d.root.scope, scope) && loads(d, facts))
                        .collect();
                    let files: BTreeSet<_> = copies
                        .iter()
                        .map(|d| d.resolved_path.as_ref().unwrap_or(&d.path))
                        .collect();
                    for copy in &copies {
                        let real = copy.resolved_path.as_ref().unwrap_or(&copy.path);
                        let real = fs.canonicalize(real).unwrap_or_else(|_| real.clone());
                        for (unverified, link) in by_copy
                            .get(&(from.name.0.clone(), real))
                            .into_iter()
                            .flatten()
                        {
                            let wanted = req
                                .skill
                                .as_ref()
                                .is_none_or(|s| *s == link.from || *s == link.to);
                            if !wanted {
                                continue;
                            }
                            let state = if *unverified || files.len() > 1 {
                                LinkState::NotKnown
                            } else {
                                resolve(&fs, copy, link, &inventory.skills, facts, &records, scope)
                            };
                            let state = if inventory.completeness == Completeness::Partial
                                && state != LinkState::Ok
                            {
                                LinkState::NotKnown
                            } else {
                                state
                            };
                            let row = grouped
                                .entry((
                                    *unverified,
                                    link.from.clone(),
                                    link.to.clone(),
                                    link.strength,
                                    link.evidence.file.clone(),
                                    link.evidence.line,
                                    link.evidence.text.clone(),
                                    link.path.clone(),
                                ))
                                .or_insert_with(|| ResolvedLink {
                                    from: link.from.clone(),
                                    to: link.to.clone(),
                                    strength: link.strength,
                                    path: link.path.clone(),
                                    evidence: link.evidence.clone(),
                                    agents: Vec::new(),
                                });
                            if let Some(previous) = row
                                .agents
                                .iter_mut()
                                .find(|a| a.agent == facts.id && a.scope == *scope)
                            {
                                if previous.state != state {
                                    previous.state = LinkState::NotKnown;
                                }
                            } else {
                                row.agents.push(AgentLinkState {
                                    agent: facts.id.clone(),
                                    scope: scope.clone(),
                                    state,
                                });
                            }
                        }
                    }
                }
            }
        }
        let mut report = SkillLinksReport {
            scopes,
            indexes: indexes.into_iter().collect(),
            not_known: not_known.into_iter().collect(),
            ..SkillLinksReport::default()
        };
        for ((unverified, ..), link) in grouped {
            if unverified {
                if !req.missing_only {
                    report.unverified.push(link);
                }
                continue;
            }
            let strong = matches!(link.strength, LinkStrength::Needs | LinkStrength::HandsOff);
            let broken = link
                .agents
                .iter()
                .filter(|a| !matches!(a.state, LinkState::Ok | LinkState::NotKnown))
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
    records: &LinkRecords,
    scope: &RootScope,
) -> LinkState {
    if let Some(path) = &link.path {
        // Extraction admits only a sibling path relative to the linking file.
        let sibling = path.trim_start_matches("../");
        let lookup = |base: &Path| -> std::io::Result<Option<PathBuf>> {
            let Some(parent) = base.parent() else {
                return Ok(None);
            };
            let path = parent.join(sibling);
            match fs.canonicalize(&path) {
                Ok(file) => match fs.symlink_metadata(&file) {
                    Ok(meta) if meta.kind == FileKind::File => Ok(Some(file)),
                    Ok(_) => Ok(None),
                    Err(err) => Err(err),
                },
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(err) => Err(err),
            }
        };
        let physical = || fs.canonicalize(&from.path).and_then(|p| lookup(&p));
        let opened = match facts.id.as_str() {
            "claude-code" => lookup(&from.path),
            "codex" => physical(),
            _ => match (lookup(&from.path), physical()) {
                (Ok(left), Ok(right)) if left == right => Ok(left),
                _ => return LinkState::NotKnown,
            },
        };
        match opened {
            Ok(Some(_)) => return LinkState::Ok,
            Err(_) => return LinkState::NotKnown,
            Ok(None) => {}
        }
    }
    let scoped = |d: &&DeploymentDto| same_scope(&d.root.scope, scope);
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
        if records.renamed(
            &link.to,
            &link.from,
            &from.root.scope,
            scope,
            |name, scope| {
                skills.iter().any(|s| {
                    s.name.0 == name
                        && s.deployments
                            .iter()
                            .any(|d| &d.root.scope == scope && loads(d, facts))
                })
            },
        ) {
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
