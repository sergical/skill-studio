//! Pack actions: add, update, remove, or switch the source of the skills of
//! one pack in one call.
//! Each call plans first and returns only the plan on a dry run. A real run
//! gives each step to the per-skill op (`install`, `update_all`, `remove`),
//! so each skill gets its own journal event and undo, and the owner tool
//! (`npx skills`, dotagents) writes its own files.
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dto::{
    InstallLinkMode, InstallMethod, InstallOutcome, InstallRequest, Inventory, RemoveRequest,
    UpdateRequest,
};
use crate::error::{CoreError, ErrorCode, ErrorEntry};
use crate::identity::{
    AgentId, BackingRelationship, DeploymentId, EventId, RootKind, RootScope, SkillDestination,
    SkillName,
};
use crate::install_targets::UNIVERSAL_TARGET;
use crate::ops::{OpStatus, Operation, Outcome};
use crate::pack::{self, git_url, Pack, PackKind, PackRemote, PackSkill, RemoteError};
use crate::ports::{OpContext, Runtime};

/// Input for `pack_add`.
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct PackAddRequest {
    /// Git source as `npx skills add` takes it: `owner/repo` or a git URL.
    pub source: String,
    /// Skills to install. Empty: every skill of the pack that is not
    /// installed yet, which needs git access to list them.
    #[serde(default)]
    pub skills: Vec<String>,
    /// Branch, tag, or commit to pin the pack to.
    #[serde(default)]
    pub ref_pin: Option<String>,
    /// Agents to link, in the `skills` CLI's `--agent` spelling. Every skill
    /// also gets the shared `.agents/skills` copy that update and remove use.
    /// Empty: the shared copy only.
    #[serde(default)]
    pub harnesses: Vec<AgentId>,
    /// Install into this tracked project. Absent: the home.
    #[serde(default)]
    pub project_path: Option<PathBuf>,
    /// Return the plan and change nothing.
    #[serde(default)]
    pub dry_run: bool,
}

/// Input for `pack_update`. Give `source` or `all`, not both.
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct PackUpdateRequest {
    /// Pack source as `pack_list` reports it.
    #[serde(default)]
    pub source: Option<String>,
    /// Update every installed pack.
    #[serde(default)]
    pub all: bool,
    /// Only this tracked project. Absent: the home and every tracked project.
    #[serde(default)]
    pub project_path: Option<PathBuf>,
    /// Return the plan and change nothing.
    #[serde(default)]
    pub dry_run: bool,
}

/// Input for `pack_remove`.
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct PackRemoveRequest {
    /// Pack source as `pack_list` reports it.
    pub source: String,
    /// Only this tracked project. Absent: the home and every tracked project.
    #[serde(default)]
    pub project_path: Option<PathBuf>,
    /// Return the plan and change nothing.
    #[serde(default)]
    pub dry_run: bool,
}

/// Input for `pack_switch_source`.
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct PackSwitchSourceRequest {
    /// Pack source as `pack_list` reports it.
    pub source: String,
    /// Git source to install the skills from instead, usually your fork of
    /// the pack: `owner/repo` or a git URL, with `#ref` to pin a branch or tag.
    pub to: String,
    /// Only these skills of the pack. Empty: every skill of the pack.
    #[serde(default)]
    pub skills: Vec<String>,
    /// Only this tracked project. Absent: the home and every tracked project.
    #[serde(default)]
    pub project_path: Option<PathBuf>,
    /// Return the plan and change nothing.
    #[serde(default)]
    pub dry_run: bool,
}

/// What one step does to one skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PackStepAction {
    /// Install the skill.
    Install,
    /// Update the skill from its source.
    Update,
    /// Remove the skill.
    Remove,
    /// Install the skill from another source over its current copy.
    SwitchSource,
}

/// Where one step is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PackStepState {
    /// Dry run: the step would run.
    Planned,
    /// The step ran. `restore_event` with this event undoes it.
    Done {
        /// The per-skill journal event.
        event_id: EventId,
    },
    /// The step does not run.
    Skipped {
        /// Why, in one sentence.
        reason: String,
    },
    /// The step ran and failed.
    Failed {
        /// Stable code of the failure.
        code: ErrorCode,
        /// The per-skill op's message, with any child process output.
        #[schemars(extend("x-untrusted" = true))]
        reason: String,
    },
}

/// One skill in a pack action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PackStep {
    /// Pack source.
    #[schemars(extend("x-untrusted" = true))]
    pub source: String,
    /// Skill name.
    #[schemars(extend("x-untrusted" = true))]
    pub skill: String,
    /// Global or one project.
    pub scope: RootScope,
    /// What the step does.
    pub action: PackStepAction,
    /// Where the step is.
    pub state: PackStepState,
}

/// Result of `pack_add`, `pack_update`, and `pack_remove`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PackActionReport {
    /// The call changed nothing.
    pub dry_run: bool,
    /// One step per skill.
    pub steps: Vec<PackStep>,
    /// One line for a person or agent.
    pub summary: String,
}

impl Outcome for PackActionReport {
    fn status(&self) -> OpStatus {
        if self.errors().is_empty() {
            OpStatus::Ok
        } else {
            OpStatus::Partial
        }
    }

    fn errors(&self) -> Vec<ErrorEntry> {
        self.steps
            .iter()
            .filter_map(|step| match &step.state {
                PackStepState::Failed { code, reason } => Some(ErrorEntry {
                    code: *code,
                    message: "A pack step failed.".into(),
                    path: None,
                    detail: Some(format!("{}: {reason}", step.skill)),
                }),
                _ => None,
            })
            .collect()
    }
}

/// Installs the skills of one git pack with `npx skills add`.
pub fn pack_add(
    rt: &Runtime,
    ctx: &OpContext,
    req: &PackAddRequest,
    remote: Option<&dyn PackRemote>,
) -> Result<PackActionReport, CoreError> {
    rt.run(Operation::PackAdd, ctx, || {
        crate::install_tools::require_node(rt)?;
        let scope = match &req.project_path {
            Some(path) => pack::selected_scopes(rt, Some(path))?.remove(0).0,
            None => RootScope::Global,
        };
        let names = if req.skills.is_empty() {
            upstream_skills(req, remote, ctx)?
        } else {
            req.skills.iter().cloned().collect()
        };
        let inventory = pack::scan(rt, ctx)?;
        let mut steps: Vec<PackStep> = names
            .into_iter()
            .map(|name| {
                let installed = inventory.skills.iter().any(|skill| {
                    skill.name.0 == name && skill.deployments.iter().any(|d| d.root.scope == scope)
                });
                PackStep {
                    source: req.source.clone(),
                    state: planned_or_skipped(
                        installed.then(|| "already installed here".to_string()),
                    ),
                    skill: name,
                    scope: scope.clone(),
                    action: PackStepAction::Install,
                }
            })
            .collect();
        let harnesses: Vec<AgentId> = std::iter::once(AgentId::from(UNIVERSAL_TARGET))
            .chain(req.harnesses.iter().cloned())
            .collect();
        let source = match &req.ref_pin {
            Some(git_ref) => format!("{}#{git_ref}", req.source),
            None => req.source.clone(),
        };
        if !req.dry_run {
            run_each(ctx, &mut steps, |_, step| {
                let outcome = crate::ops::install(
                    rt,
                    ctx,
                    &InstallRequest {
                        skill: SkillName(step.skill.clone()),
                        method: InstallMethod::SkillsSh,
                        scope: step.scope.clone(),
                        harnesses: harnesses.clone(),
                        link_mode: InstallLinkMode::Link,
                        destination: SkillDestination::Universal,
                        files: Vec::new(),
                        source: Some(source.clone()),
                        trust_identity: None,
                        trust_confirmed: false,
                        save_as_preference: false,
                    },
                )?;
                match outcome {
                    InstallOutcome::Installed { event_id, .. } => Ok(event_id),
                    InstallOutcome::NeedsTrust { identity } => Err(CoreError::new(
                        ErrorCode::InvalidRequest,
                        format!("{identity} is not trusted yet"),
                    )),
                }
            });
        }
        Ok(action_report(req.dry_run, PackStepAction::Install, steps))
    })
}

/// Updates every skill of one pack, or of every pack, through its owner tool.
/// Skips skills with local edits, which an update would overwrite.
pub fn pack_update(
    rt: &Runtime,
    ctx: &OpContext,
    req: &PackUpdateRequest,
) -> Result<PackActionReport, CoreError> {
    rt.run(Operation::PackUpdate, ctx, || {
        let source = match (&req.source, req.all) {
            (Some(source), false) => Some(source.as_str()),
            (None, true) => None,
            _ => {
                return Err(CoreError::new(
                    ErrorCode::InvalidRequest,
                    "give one pack source or all, not both or neither",
                ))
            }
        };
        let (report, _) = pack::build(rt, ctx, req.project_path.as_deref(), source, false, None)?;
        if let (Some(source), true) = (source, report.packs.is_empty()) {
            return Err(pack::no_pack(source));
        }
        let mut steps = Vec::new();
        let mut methods = Vec::new();
        for pack in &report.packs {
            for skill in &pack.skills {
                let skip = match pack.kind {
                    PackKind::Plugin => Some(format!(
                        "a plugin; the agent updates it with {}",
                        pack.update_commands.join(" or ")
                    )),
                    PackKind::Npm => Some(format!(
                        "comes from the npm package {}; update the package, then run npx skills sync",
                        pack.source
                    )),
                    PackKind::SkillsSh | PackKind::Dotagents => edited_reason(skill),
                };
                steps.push(step(pack, skill, PackStepAction::Update, skip));
                methods.push(match pack.kind {
                    PackKind::Dotagents => InstallMethod::Dotagents,
                    _ => InstallMethod::SkillsSh,
                });
            }
        }
        require_node_for(rt, &steps)?;
        if !req.dry_run {
            update_by_scope(rt, ctx, &mut steps, &methods);
        }
        Ok(action_report(req.dry_run, PackStepAction::Update, steps))
    })
}

/// Removes every skill of one pack through its owner tool. Skips skills with
/// local edits: the owner tool deletes the folder, so they would be lost.
pub fn pack_remove(
    rt: &Runtime,
    ctx: &OpContext,
    req: &PackRemoveRequest,
) -> Result<PackActionReport, CoreError> {
    rt.run(Operation::PackRemove, ctx, || {
        let (report, inventory) = pack::build(
            rt,
            ctx,
            req.project_path.as_deref(),
            Some(&req.source),
            false,
            None,
        )?;
        if report.packs.is_empty() {
            return Err(pack::no_pack(&req.source));
        }
        let mut steps = Vec::new();
        let mut targets = Vec::new();
        for pack in &report.packs {
            for skill in &pack.skills {
                let target = removable(&inventory, skill);
                let skip = match (pack.kind, &target) {
                    (PackKind::Plugin, _) => Some("a plugin; remove it in the agent".to_string()),
                    (PackKind::Npm, _) => Some(format!(
                        "comes from the npm package {}; remove the package, then run npx skills sync",
                        pack.source
                    )),
                    (_, None) => Some("no copy in .agents/skills to remove".to_string()),
                    (_, Some(_)) => edited_reason(skill),
                };
                steps.push(step(pack, skill, PackStepAction::Remove, skip));
                targets.push(target);
            }
        }
        require_node_for(rt, &steps)?;
        if !req.dry_run {
            run_each(ctx, &mut steps, |index, _| {
                let deployment_id = targets[index].clone().ok_or_else(|| {
                    CoreError::new(ErrorCode::Incomplete, "planned remove has no target")
                })?;
                crate::ops::remove(rt, ctx, &RemoveRequest { deployment_id })
                    .map(|outcome| outcome.event_id)
            });
        }
        Ok(action_report(req.dry_run, PackStepAction::Remove, steps))
    })
}

/// Moves the skills of one skills.sh pack to another git source, such as
/// the user's fork. Each skill runs `npx skills add <to> --skill <name>`
/// over its copy, which moves its lock row to `to`; undo one with its
/// event. The old source is recorded as the pack's original, so a remote
/// `pack_show` of `to` compares the two. Skips skills with local edits,
/// which the new copy would overwrite.
pub fn pack_switch_source(
    rt: &Runtime,
    ctx: &OpContext,
    req: &PackSwitchSourceRequest,
) -> Result<PackActionReport, CoreError> {
    rt.run(Operation::PackSwitchSource, ctx, || {
        if git_url(&req.to).is_none() {
            return Err(CoreError::new(
                ErrorCode::InvalidRequest,
                format!("give the new source {} as owner/repo or a git URL", req.to),
            ));
        }
        let identity = |source: &str| pack::source_identity(PackKind::SkillsSh, source);
        if identity(&req.to) == identity(&req.source) {
            return Err(CoreError::new(
                ErrorCode::InvalidRequest,
                format!("the pack already comes from {}", req.to),
            ));
        }
        let (report, inventory) = pack::build(
            rt,
            ctx,
            req.project_path.as_deref(),
            Some(&req.source),
            false,
            None,
        )?;
        if report.packs.is_empty() {
            return Err(pack::no_pack(&req.source));
        }
        let unknown: Vec<&str> = req
            .skills
            .iter()
            .filter(|name| {
                !report
                    .packs
                    .iter()
                    .any(|pack| pack.skills.iter().any(|skill| &&skill.name == name))
            })
            .map(String::as_str)
            .collect();
        if !unknown.is_empty() {
            return Err(CoreError::new(
                ErrorCode::InvalidRequest,
                format!(
                    "{} has no installed skill named {}",
                    req.source,
                    unknown.join(", ")
                ),
            ));
        }
        let mut steps = Vec::new();
        for pack in &report.packs {
            for skill in &pack.skills {
                if !req.skills.is_empty() && !req.skills.contains(&skill.name) {
                    continue;
                }
                let skip = match pack.kind {
                    PackKind::SkillsSh if removable(&inventory, skill).is_none() => {
                        Some("no copy in .agents/skills to switch".to_string())
                    }
                    PackKind::SkillsSh => edited_reason(skill),
                    PackKind::Dotagents => Some(
                        "installed by dotagents, which keeps the source in agents.toml; change it there"
                            .to_string(),
                    ),
                    PackKind::Plugin => Some("a plugin; the agent picks its source".to_string()),
                    PackKind::Npm => Some(format!(
                        "comes from the npm package {}; change the package instead",
                        pack.source
                    )),
                };
                steps.push(step(pack, skill, PackStepAction::SwitchSource, skip));
            }
        }
        require_node_for(rt, &steps)?;
        let planned = steps.iter().any(|s| s.state == PackStepState::Planned);
        let mut switched = None;
        if !req.dry_run && planned {
            // The link goes down first: if it cannot be written, nothing moved.
            let from = &report.packs[0].source;
            let (before, link) = crate::pack_original::record_switch(rt, from, &req.to)?;
            run_each(ctx, &mut steps, |_, step| {
                let request = UpdateRequest {
                    skill: SkillName(step.skill.clone()),
                    method: InstallMethod::SkillsSh,
                    scope: step.scope.clone(),
                    files: Vec::new(),
                    source: Some(step.source.clone()),
                    ref_pin: None,
                };
                crate::ops_update::update_from_source(rt, ctx, &request, &req.to)
                    .map(|outcome| outcome.event_id)
            });
            if steps
                .iter()
                .any(|s| matches!(s.state, PackStepState::Done { .. }))
            {
                switched = Some((from.clone(), link));
            } else {
                crate::pack_original::restore_links(rt, before)?;
            }
        }
        let mut report = action_report(req.dry_run, PackStepAction::SwitchSource, steps);
        report.summary = match switched {
            Some((from, crate::pack_original::Switched::Linked)) => format!(
                "{} {from} stays recorded as the original; pack show {} --remote compares the two.",
                report.summary, req.to
            ),
            Some((_, crate::pack_original::Switched::BackToOriginal)) => format!(
                "{} {} was the original of this pack, so no original is recorded now.",
                report.summary, req.to
            ),
            None => report.summary,
        };
        Ok(report)
    })
}

/// Every planned update or remove step runs `npx`: plugin and npm packs are
/// already skipped. Fails once, before the plan is shown, when Node is missing.
fn require_node_for(rt: &Runtime, steps: &[PackStep]) -> Result<(), CoreError> {
    if steps
        .iter()
        .any(|step| step.state == PackStepState::Planned)
    {
        crate::install_tools::require_node(rt)?;
    }
    Ok(())
}

fn upstream_skills(
    req: &PackAddRequest,
    remote: Option<&dyn PackRemote>,
    ctx: &OpContext,
) -> Result<BTreeSet<String>, CoreError> {
    let remote = remote.ok_or_else(|| {
        CoreError::new(
            ErrorCode::Unsupported,
            "name the skills to add: this surface has no git access to list them",
        )
    })?;
    let url = git_url(&req.source).ok_or_else(|| {
        CoreError::new(
            ErrorCode::InvalidRequest,
            format!(
                "cannot list the skills of {}; name them, or give owner/repo or a git URL",
                req.source
            ),
        )
    })?;
    let upstream = remote
        .fetch(&url, req.ref_pin.as_deref(), ctx.cancel.as_ref())
        .map_err(|err| match err {
            RemoteError::GitMissing => CoreError::new(ErrorCode::Unsupported, err.text(&url)),
            RemoteError::NoAccess => CoreError::new(ErrorCode::Io, err.text(&req.source)),
            RemoteError::Failed(_) => CoreError::new(ErrorCode::Io, err.text(&url)),
        })?;
    if upstream.skills.is_empty() {
        return Err(CoreError::new(
            ErrorCode::InvalidRequest,
            format!("{} has no SKILL.md at {}", req.source, upstream.commit),
        ));
    }
    Ok(upstream
        .skills
        .into_iter()
        .map(|skill| skill.name)
        .collect())
}

fn removable(inventory: &Inventory, skill: &PackSkill) -> Option<DeploymentId> {
    inventory
        .skills
        .iter()
        .filter(|found| found.name.0 == skill.name)
        .flat_map(|found| &found.deployments)
        .find(|d| {
            d.root.scope == skill.scope
                && d.root.kind == RootKind::Universal
                && d.backing == BackingRelationship::Canonical
                && d.owner_kind.is_mutable()
        })
        .map(|d| d.id.clone())
}

fn edited_reason(skill: &PackSkill) -> Option<String> {
    (skill.edited == Some(true))
        .then(|| "has local edits, which the owner tool would overwrite or delete".to_string())
}

fn planned_or_skipped(skip: Option<String>) -> PackStepState {
    skip.map_or(PackStepState::Planned, |reason| PackStepState::Skipped {
        reason,
    })
}

fn step(pack: &Pack, skill: &PackSkill, action: PackStepAction, skip: Option<String>) -> PackStep {
    PackStep {
        source: pack.source.clone(),
        skill: skill.name.clone(),
        scope: skill.scope.clone(),
        action,
        state: planned_or_skipped(skip),
    }
}

/// Runs each planned step in order, with its index in `steps`. After a
/// cancel, the rest are skipped.
fn run_each(
    ctx: &OpContext,
    steps: &mut [PackStep],
    mut run: impl FnMut(usize, &PackStep) -> Result<EventId, CoreError>,
) {
    for (index, step) in steps.iter_mut().enumerate() {
        if step.state != PackStepState::Planned {
            continue;
        }
        step.state = match ctx.checkpoint().and_then(|()| run(index, step)) {
            Ok(event_id) => PackStepState::Done { event_id },
            Err(err) if err.code == ErrorCode::Cancelled => PackStepState::Skipped {
                reason: "cancelled".to_string(),
            },
            Err(err) => PackStepState::Failed {
                code: err.code,
                reason: err.text(),
            },
        };
    }
}

/// One `update_all` batch per scope: skill names are unique within a scope,
/// and `update_all` reports by name.
fn update_by_scope(
    rt: &Runtime,
    ctx: &OpContext,
    steps: &mut [PackStep],
    methods: &[InstallMethod],
) {
    let mut by_scope: BTreeMap<RootScope, Vec<usize>> = BTreeMap::new();
    for (index, step) in steps.iter().enumerate() {
        if step.state == PackStepState::Planned {
            by_scope.entry(step.scope.clone()).or_default().push(index);
        }
    }
    for indexes in by_scope.into_values() {
        let requests: Vec<UpdateRequest> = indexes
            .iter()
            .map(|&i| UpdateRequest {
                skill: SkillName(steps[i].skill.clone()),
                method: methods[i],
                scope: steps[i].scope.clone(),
                files: Vec::new(),
                source: Some(steps[i].source.clone()),
                ref_pin: None,
            })
            .collect();
        let outcome = match crate::ops::update_all_unbounded(rt, ctx, &requests, |_, _| {}) {
            Ok(outcome) => outcome,
            Err(err) => {
                for &i in &indexes {
                    steps[i].state = PackStepState::Failed {
                        code: err.code,
                        reason: err.text(),
                    };
                }
                continue;
            }
        };
        for &i in &indexes {
            let name = steps[i].skill.as_str();
            steps[i].state = if let Some(done) = outcome
                .items
                .iter()
                .find_map(|item| item.outcome.as_ref().filter(|o| o.skill.0 == name))
            {
                PackStepState::Done {
                    event_id: done.event_id.clone(),
                }
            } else if let Some(reason) = outcome.errors.get(name) {
                PackStepState::Failed {
                    code: ErrorCode::Io,
                    reason: reason.clone(),
                }
            } else {
                PackStepState::Skipped {
                    reason: "cancelled".to_string(),
                }
            };
        }
    }
}

fn action_report(dry_run: bool, action: PackStepAction, steps: Vec<PackStep>) -> PackActionReport {
    let count = |want: fn(&PackStepState) -> bool| steps.iter().filter(|s| want(&s.state)).count();
    let planned = count(|s| matches!(s, PackStepState::Planned));
    let done = count(|s| matches!(s, PackStepState::Done { .. }));
    let skipped = count(|s| matches!(s, PackStepState::Skipped { .. }));
    let failed = count(|s| matches!(s, PackStepState::Failed { .. }));
    let (verb, past) = match action {
        PackStepAction::Install => ("install", "Installed"),
        PackStepAction::Update => ("update", "Updated"),
        PackStepAction::Remove => ("remove", "Removed"),
        PackStepAction::SwitchSource => ("switch", "Switched"),
    };
    let summary = if dry_run {
        format!(
            "Would {verb} {}; {skipped} skipped.",
            pack::plural(planned, "skill")
        )
    } else {
        format!(
            "{past} {}; {skipped} skipped; {failed} failed.",
            pack::plural(done, "skill")
        )
    };
    PackActionReport {
        dry_run,
        steps,
        summary,
    }
}

#[cfg(test)]
mod tests {
    use crate::pack::git_url;

    #[test]
    fn git_url_expands_github_shorthand_and_keeps_urls_or_refuses_local_paths() {
        assert_eq!(
            git_url("owner/repo").as_deref(),
            Some("https://github.com/owner/repo.git")
        );
        assert_eq!(
            git_url("https://gitlab.example.com/a/b.git").as_deref(),
            Some("https://gitlab.example.com/a/b.git")
        );
        assert_eq!(
            git_url("git@github.com:o/r.git").as_deref(),
            Some("git@github.com:o/r.git")
        );
        assert_eq!(git_url("./local/pack"), None);
        assert_eq!(git_url("/abs/pack"), None);
        assert_eq!(git_url("a/b/c"), None);
    }
}
