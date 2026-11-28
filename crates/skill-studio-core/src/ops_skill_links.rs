//! The `skill_links` operation: links between installed skills, resolved for
//! each agent that has a copy of the linking skill
//! (`docs/spec-skill-links.md` sections 3 and 4).

use std::collections::BTreeSet;
use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dto::{DeploymentDto, InstalledSkillDto, ScanRequest};
use crate::error::CoreError;
use crate::harness::{deployment_visible_to, HarnessFacts};
use crate::identity::{AgentId, RootKind, RootScope};
use crate::ops::Operation;
use crate::ports::{FileKind, OpContext, Runtime, ScopeFs, ScopedReads};
use crate::skill_links::{detect_links, LinkEvidence, LinkStrength, SkillText};

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
    /// One entry per agent that loads `from`.
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
        let mut texts = Vec::new();
        let mut not_known = Vec::new();
        for skill in &inventory.skills {
            ctx.checkpoint()?;
            let Some(copy) = skill.deployments.iter().find(|d| readable(d)) else {
                continue;
            };
            match read_markdown(&fs, &copy.path) {
                Some(files) => texts.push(SkillText {
                    name: skill.name.to_string(),
                    folder: folder_name(&copy.path),
                    pack: Some(skill.source.clone()),
                    files,
                }),
                None => not_known.push(skill.name.to_string()),
            }
        }
        let detected = detect_links(&texts, &BTreeSet::new());
        let agents: Vec<&HarnessFacts> = rt
            .ports
            .catalog
            .facts
            .iter()
            .filter(|facts| req.agent.as_ref().is_none_or(|agent| agent == &facts.id))
            .collect();
        let mut report = SkillLinksReport {
            indexes: detected.indexes,
            not_known,
            ..SkillLinksReport::default()
        };
        for link in detected.links {
            let wanted = req
                .skill
                .as_ref()
                .is_none_or(|s| *s == link.from || *s == link.to);
            let Some(from) = inventory.skills.iter().find(|s| s.name.0 == link.from) else {
                continue;
            };
            if !wanted {
                continue;
            }
            let target = inventory.skills.iter().find(|s| s.name.0 == link.to);
            let mut states = Vec::new();
            for facts in &agents {
                let Some(copy) = from.deployments.iter().find(|d| loads(d, facts)) else {
                    continue;
                };
                let state = resolve(
                    &fs,
                    copy,
                    &link.evidence,
                    link.path.as_deref(),
                    target,
                    facts,
                );
                states.push(AgentLinkState {
                    agent: facts.id.clone(),
                    state,
                });
            }
            let strong = matches!(link.strength, LinkStrength::Needs | LinkStrength::HandsOff);
            let broken = states.iter().filter(|s| s.state != LinkState::Ok).count();
            if strong {
                report.broken += broken;
            }
            if req.missing_only && (!strong || broken == 0) {
                continue;
            }
            report.links.push(ResolvedLink {
                from: link.from,
                to: link.to,
                strength: link.strength,
                path: link.path,
                evidence: link.evidence,
                agents: states,
            });
        }
        Ok(report)
    })
}

fn readable(d: &DeploymentDto) -> bool {
    !d.symlink_is_broken && d.root.kind != RootKind::Parked
}

/// The agent loads this copy: it reads its root, and the copy is neither
/// parked nor turned off for it.
fn loads(d: &DeploymentDto, facts: &HarnessFacts) -> bool {
    readable(d) && deployment_visible_to(d, facts) && !off_for(d, facts)
}

fn off_for(d: &DeploymentDto, facts: &HarnessFacts) -> bool {
    d.disabled_by.is_some() || d.disabled_readers.contains(&facts.id)
}

fn same_scope(target: &RootScope, from: &RootScope) -> bool {
    *target == RootScope::Global || target == from
}

fn resolve(
    fs: &dyn ScopeFs,
    from: &DeploymentDto,
    evidence: &LinkEvidence,
    path: Option<&str>,
    target: Option<&InstalledSkillDto>,
    facts: &HarnessFacts,
) -> LinkState {
    if let Some(path) = path {
        // The operating system resolves `..` after following a linked skill
        // folder, so the path is joined to the real folder, as an agent's
        // file read would be.
        let opened = fs.canonicalize(&from.path).ok().and_then(|real| {
            let dir = Path::new(&evidence.file)
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default();
            fs.canonicalize(&real.join(dir).join(path)).ok()
        });
        if opened.is_some() {
            return LinkState::Ok;
        }
    }
    let Some(target) = target else {
        return LinkState::Missing;
    };
    let scoped = |d: &&DeploymentDto| same_scope(&d.root.scope, &from.root.scope);
    if path.is_none()
        && target
            .deployments
            .iter()
            .filter(scoped)
            .any(|d| loads(d, facts))
    {
        return LinkState::Ok;
    }
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

/// The `.md` files of one skill folder, as `(path with /, text)`. `None`
/// when the folder itself cannot be read.
fn read_markdown(fs: &dyn ScopeFs, root: &Path) -> Option<Vec<(String, String)>> {
    let mut out = Vec::new();
    let mut stack = vec![(root.to_path_buf(), String::new(), 0usize)];
    let mut first = true;
    while let Some((dir, rel, depth)) = stack.pop() {
        let entries = match fs.read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) if first => return None,
            Err(_) => continue,
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
                FileKind::File if is_markdown(&entry.name) && out.len() < MAX_FILES => {
                    if let Ok(bytes) = fs.read_capped(&path, MAX_FILE_BYTES) {
                        out.push((name, String::from_utf8_lossy(&bytes).into_owned()));
                    }
                }
                _ => {}
            }
        }
    }
    out.sort();
    Some(out)
}

fn is_markdown(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
}
