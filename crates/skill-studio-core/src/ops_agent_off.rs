//! `turn_off_for_agent`: turns a skill in the shared folder off for one
//! agent. The shared folder cannot hide a skill from one reader, so the op
//! splits it (a real copy for every agent that reads the folder) and then
//! parks the chosen agent's copy.
//!
//! Both steps run in the split's one session, under one lease. The split's
//! journal row is the row Activity shows, marked `agent_off` from the start;
//! the park step adds a `park` row of its own (the shared
//! `ops::park_found_copy`), so `unpark` brings the copy back like any other
//! parked copy. One undo (`restore_event`) of the split row puts the shared
//! folder back and removes the per-agent copies and the parked copy.

use std::path::{Path, PathBuf};

use crate::dto::{
    AgentOffCheck, AgentOffOutcome, AgentOffRefusal, AgentOffRequest, DeploymentDto,
    InstalledSkillDto, ParkCheckRequest, ScanRequest, SplitRequest,
};
use crate::error::{CoreError, ErrorCode};
use crate::harness::{RootRole, ScopeLevel};
use crate::identity::{
    AgentId, BackingRelationship, EventId, LifecycleOwnerKind, RootKind, RootScope, SkillName,
    PARKED_ROOT_RELATIVE,
};
use crate::ops::{self, Operation, Outcome, ParkedCopy};
use crate::ops_split::{is_whole_folder_link, split_body, split_target_root, AgentOffHook};
use crate::ports::{MutationSession, OpContext, Runtime};

/// The harnesses `split` can write a copy for, in the split dialog's order.
const SPLIT_HARNESSES: [&str; 6] = [
    AgentId::CLAUDE_CODE,
    AgentId::CODEX,
    AgentId::OPEN_CODE,
    AgentId::PI,
    AgentId::CURSOR,
    AgentId::GROK_BUILD,
];

impl Outcome for AgentOffOutcome {
    fn event_id(&self) -> Option<crate::identity::EventId> {
        Some(self.event_id.clone())
    }
}

/// Turns the skill in the shared folder off for `req.agent`.
///
/// Preconditions, all checked before the first write ([`plan`]): the
/// deployment is the live Universal folder; it is not a plugin copy and not
/// managed by dotagents (whose `install` would put the shared folder back);
/// `agent` reads the shared folder in that scope; no reader's skills folder
/// is a whole-folder link into it; `agent` reads no other folder the split
/// would put a copy in (Cursor, `OpenCode`, and Grok Build also read the
/// Claude Code folder, Cursor the Codex folder); no other reader has the
/// skill off in its own settings; and nothing is parked yet for `agent`'s
/// copy of this skill.
///
/// Sequence, in one session: `split` for every agent that reads the folder,
/// then park `agent`'s new copy. If the park step fails, the split is rolled
/// back inside the session, so a failure leaves the shared folder as it was
/// and writes no undoable row.
pub fn turn_off_for_agent(
    rt: &Runtime,
    ctx: &OpContext,
    req: &AgentOffRequest,
) -> Result<AgentOffOutcome, CoreError> {
    rt.run(Operation::TurnOffForAgent, ctx, || {
        turn_off_for_agent_body(rt, ctx, req)
    })
}

/// What to know before the user confirms: the refusal `turn_off_for_agent`
/// would give, and whether git tracks the shared folder. Reads only.
pub fn turn_off_check(
    rt: &Runtime,
    ctx: &OpContext,
    req: &AgentOffRequest,
) -> Result<AgentOffCheck, CoreError> {
    let refusal = plan(rt, ctx, req)?.err();
    let park = ops::park_check(
        rt,
        ctx,
        &ParkCheckRequest {
            deployment_id: req.deployment_id.clone(),
        },
    )?;
    Ok(AgentOffCheck {
        refusal,
        git_tracked: park.git_tracked,
        project: park.project,
    })
}

struct Plan {
    skill: SkillName,
    harnesses: Vec<AgentId>,
    agent_copy: PathBuf,
}

fn refusal(reason: impl Into<String>, off_everywhere: bool) -> AgentOffRefusal {
    AgentOffRefusal {
        reason: reason.into(),
        off_everywhere,
    }
}

fn turn_off_for_agent_body(
    rt: &Runtime,
    ctx: &OpContext,
    req: &AgentOffRequest,
) -> Result<AgentOffOutcome, CoreError> {
    ctx.checkpoint()?;
    let plan = match plan(rt, ctx, req)? {
        Ok(plan) => plan,
        Err(refused) => {
            return Err(CoreError::new(ErrorCode::Unsupported, refused.reason));
        }
    };
    let park =
        |rt: &Runtime, ctx: &OpContext, session: &mut MutationSession, split_event: &EventId| {
            park_agent_copy(rt, ctx, session, split_event, &plan, &req.agent)
        };
    let hook = AgentOffHook {
        agent: &req.agent,
        park: &park,
    };
    let (split, parked_path) = split_body(
        rt,
        ctx,
        &SplitRequest {
            deployment_id: req.deployment_id.clone(),
            harnesses: plan.harnesses.clone(),
        },
        Some(&hook),
    )?;
    let parked_path = parked_path.ok_or_else(|| {
        CoreError::new(ErrorCode::Io, "the split finished without parking a copy")
    })?;
    Ok(AgentOffOutcome {
        event_id: split.event_id,
        deployment_id: split.deployment_id,
        skill: split.skill,
        agent: req.agent.clone(),
        copies: split.copies,
        parked_path,
        update_note: split.update_note,
    })
}

fn harness_label(rt: &Runtime, id: &AgentId) -> String {
    rt.ports
        .catalog
        .get(id)
        .map_or_else(|| id.as_str().to_string(), |f| f.display_name.clone())
}

/// "Codex", "Codex and Claude Code", "A, B and C".
fn join_labels(labels: &[String]) -> String {
    match labels {
        [] => String::new(),
        [only] => only.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

fn scope_level(scope: &RootScope) -> ScopeLevel {
    match scope {
        RootScope::Global => ScopeLevel::Global,
        RootScope::Project(_) => ScopeLevel::Project,
    }
}

/// The agents that read the shared folder at `deployment`'s scope, in split
/// order: harnesses whose catalog reads the Universal root, plus harnesses
/// that reach it through a link. A harness with a real copy of its own is
/// not a reader.
fn readers(rt: &Runtime, skill: &InstalledSkillDto, deployment: &DeploymentDto) -> Vec<AgentId> {
    let level = scope_level(&deployment.root.scope);
    let own_deployments = |harness: &AgentId| {
        skill
            .deployments
            .iter()
            .filter(|d| {
                d.root.scope == deployment.root.scope
                    && d.plugin.is_none()
                    && d.root.kind == RootKind::Harness(harness.clone())
            })
            .collect::<Vec<_>>()
    };
    SPLIT_HARNESSES
        .iter()
        .filter_map(|raw| AgentId::parse(raw).ok())
        .filter(|harness| {
            let reads_root = rt.ports.catalog.get(harness).is_some_and(|facts| {
                facts
                    .roots
                    .iter()
                    .any(|root| root.role == RootRole::Universal && root.level == level)
            });
            let own = own_deployments(harness);
            let reaches_by_link = |d: &&DeploymentDto| d.is_symlink || d.shared_via_whole_dir_link;
            let has_own_copy = own.iter().any(|d| !reaches_by_link(d));
            !has_own_copy && (reads_root || own.iter().any(reaches_by_link))
        })
        .collect()
}

/// The harnesses whose skills folder `agent` also reads, and where the
/// split would leave a copy of the skill: it writes one for every reader,
/// and a real copy already there stays. `agent` would still load that copy
/// after its own is parked.
fn cross_readers(
    rt: &Runtime,
    skill: &InstalledSkillDto,
    deployment: &DeploymentDto,
    agent: &AgentId,
    harnesses: &[AgentId],
) -> Vec<AgentId> {
    let scope = &deployment.root.scope;
    let level = scope_level(scope);
    let Some(facts) = rt.ports.catalog.get(agent) else {
        return Vec::new();
    };
    let cross_roots: Vec<PathBuf> = facts
        .roots
        .iter()
        .filter(|root| root.role == RootRole::CrossHarness && root.level == level)
        .map(|root| match scope {
            RootScope::Global => rt.scope.global_root_path(Path::new(&root.relative_path)),
            RootScope::Project(project) => project.0.join(&root.relative_path),
        })
        .collect();
    SPLIT_HARNESSES
        .iter()
        .filter_map(|raw| AgentId::parse(raw).ok())
        .filter(|other| other != agent)
        .filter(|other| {
            let Some(root) = split_target_root(rt, scope, other) else {
                return false;
            };
            cross_roots.contains(&root)
                && (harnesses.contains(other)
                    || skill.deployments.iter().any(|d| {
                        d.root.scope == *scope
                            && d.plugin.is_none()
                            && !d.is_symlink
                            && d.path.parent() == Some(root.as_path())
                    }))
        })
        .collect()
}

/// The readers other than `agent` that have the skill off in their own
/// settings: Codex by the path of its `SKILL.md`, `OpenCode` by name. The split
/// gives Codex a copy at a new path, which its path-based rule no longer
/// covers.
fn config_off_readers(
    rt: &Runtime,
    deployment: &DeploymentDto,
    skill: &SkillName,
    agent: &AgentId,
    harnesses: &[AgentId],
) -> Vec<AgentId> {
    let fs = rt.ports.fs.as_ref();
    harnesses
        .iter()
        .filter(|harness| *harness != agent)
        .filter(|harness| match harness.as_str() {
            AgentId::CODEX => {
                let skill_md = deployment.path.join("SKILL.md");
                ops::codex_disabled_skill_md_paths(fs, &rt.scope.codex_home)
                    .contains(&ops::codex_path_form(fs, &skill_md))
            }
            AgentId::OPEN_CODE => {
                let config_dir = rt
                    .scope
                    .opencode_config_root
                    .clone()
                    .unwrap_or_else(|| rt.scope.home.lexical.join(".config").join("opencode"));
                crate::opencode_config::read_skill_rules(fs, &config_dir).is_denied(&skill.0)
            }
            _ => false,
        })
        .cloned()
        .collect()
}

/// Checks every precondition without writing. The inner `Err` is a refusal
/// with a plain reason; the outer `Err` is a failure to look.
fn plan(
    rt: &Runtime,
    ctx: &OpContext,
    req: &AgentOffRequest,
) -> Result<Result<Plan, AgentOffRefusal>, CoreError> {
    let inventory = ops::scan(
        rt,
        ctx,
        &ScanRequest {
            skills: req.deployment_id.skill_name().into_iter().collect(),
            timings: false,
        },
    )?;
    let skill = ops::resolve_skill(&inventory, &req.deployment_id)?;
    let deployment = skill
        .deployments
        .iter()
        .find(|d| d.id == req.deployment_id)
        .ok_or_else(|| {
            CoreError::new(
                ErrorCode::AmbiguousTarget,
                format!("no copy matches {}", req.deployment_id.as_str()),
            )
        })?;
    let fs = rt.ports.fs.as_ref();
    let agent_label = harness_label(rt, &req.agent);

    if deployment.plugin.is_some() || deployment.owner_kind == LifecycleOwnerKind::Plugin {
        return Ok(Err(refusal(
            "a plugin copy cannot be turned off for one agent; turn it off with /plugin in the agent",
            false,
        )));
    }
    if deployment.root.kind != RootKind::Universal
        || deployment.backing != BackingRelationship::Canonical
        || deployment.is_symlink
    {
        return Ok(Err(refusal(
            "this is not a live copy in the shared folder, so there is nothing to turn off for one agent",
            false,
        )));
    }
    if matches!(
        deployment.owner_kind,
        LifecycleOwnerKind::Dotagents | LifecycleOwnerKind::WildcardDotagents
    ) {
        return Ok(Err(refusal(
            format!(
                "dotagents manages this skill, and `dotagents install` would undo a change for {agent_label} only. Use \"Off everywhere\" instead."
            ),
            true,
        )));
    }

    let harnesses = readers(rt, skill, deployment);
    if !harnesses.contains(&req.agent) {
        return Ok(Err(refusal(
            format!(
                "{agent_label} does not read the shared folder here, so there is nothing to turn off for it"
            ),
            false,
        )));
    }

    let universal_root = deployment.path.parent().ok_or_else(|| {
        CoreError::new(ErrorCode::Io, "a Universal folder copy path has no parent")
            .at(&deployment.path)
    })?;
    let canonical_universal_root = fs
        .canonicalize(universal_root)
        .map_err(|e| CoreError::io(universal_root, e))?;
    for harness in &harnesses {
        let Some(root) = split_target_root(rt, &deployment.root.scope, harness) else {
            continue;
        };
        if is_whole_folder_link(fs, &root, &canonical_universal_root) {
            return Ok(Err(refusal(
                format!(
                    "{} is a link to the shared folder, so {} has no folder of its own to hold a copy. Use \"Off everywhere\" instead.",
                    root.display(),
                    harness_label(rt, harness)
                ),
                true,
            )));
        }
    }

    let also_read = cross_readers(rt, skill, deployment, &req.agent, &harnesses);
    if !also_read.is_empty() {
        let folders: Vec<String> = also_read.iter().map(|id| harness_label(rt, id)).collect();
        return Ok(Err(refusal(
            format!(
                "{agent_label} also reads the {} {}, so it would still load {}. Use \"Off everywhere\" instead.",
                join_labels(&folders),
                if folders.len() == 1 { "folder" } else { "folders" },
                skill.name.0
            ),
            true,
        )));
    }

    let config_off = config_off_readers(rt, deployment, &skill.name, &req.agent, &harnesses);
    if !config_off.is_empty() {
        let labels: Vec<String> = config_off.iter().map(|id| harness_label(rt, id)).collect();
        return Ok(Err(refusal(
            format!(
                "{} already has {} off in its own settings, and a copy made for {agent_label} could turn it back on there. Use \"Off everywhere\" instead.",
                join_labels(&labels),
                skill.name.0
            ),
            true,
        )));
    }

    let agent_root =
        split_target_root(rt, &deployment.root.scope, &req.agent).ok_or_else(|| {
            CoreError::new(
                ErrorCode::InvalidRequest,
                format!("`{}` has no skills folder to copy into", req.agent.as_str()),
            )
        })?;
    if fs
        .symlink_metadata(&parked_dir(
            rt,
            &deployment.root.scope,
            &req.agent,
            &skill.name,
        )?)
        .is_ok()
    {
        return Ok(Err(refusal(
            format!(
                "a parked copy of this skill from {agent_label} already exists; turn it on or delete it first"
            ),
            false,
        )));
    }
    Ok(Ok(Plan {
        skill: skill.name.clone(),
        harnesses,
        agent_copy: agent_root.join(&skill.name.0),
    }))
}

/// Where a plain park of `agent`'s copy would put it.
fn parked_dir(
    rt: &Runtime,
    scope: &RootScope,
    agent: &AgentId,
    skill: &SkillName,
) -> Result<PathBuf, CoreError> {
    let origin = crate::identity::RootRef {
        scope: scope.clone(),
        kind: RootKind::Harness(agent.clone()),
    };
    let project_key = match scope {
        RootScope::Global => None,
        RootScope::Project(project) => Some(crate::park_layout::project_key(
            &rt.ports
                .fs
                .canonicalize(&project.0)
                .unwrap_or_else(|_| project.0.clone()),
        )),
    };
    let parked_root = rt.scope.home.lexical.join(PARKED_ROOT_RELATIVE);
    crate::park_layout::parked_slot_dir(&parked_root, &origin, project_key.as_deref())
        .map(|slot| slot.join(&skill.0))
        .ok_or_else(|| {
            CoreError::new(
                ErrorCode::Unsupported,
                format!("{}'s copy cannot be parked", agent.as_str()),
            )
        })
}

/// Parks `agent`'s fresh copy with the same steps `ops::park` runs, then
/// teaches the split row's inverse about the parked copy and its `.origin`
/// note, so the one undo removes them too.
///
/// The session's inventory predates the split, so it is scanned again to
/// see the new copy.
fn park_agent_copy(
    rt: &Runtime,
    ctx: &OpContext,
    session: &mut MutationSession,
    split_event: &EventId,
    plan: &Plan,
    agent: &AgentId,
) -> Result<PathBuf, CoreError> {
    session.fresh = ops::scan_inner(
        rt,
        ctx,
        &ScanRequest {
            skills: vec![plan.skill.clone()],
            timings: false,
        },
    )?;
    let copy = find_copy(rt, session, &plan.agent_copy).ok_or_else(|| {
        CoreError::new(
            ErrorCode::Io,
            format!(
                "the {} copy is not in the scan after the split",
                agent.as_str()
            ),
        )
        .at(&plan.agent_copy)
    })?;
    ops::refuse_unparkable(&copy)?;
    let skill = ops::resolve_skill(&session.fresh, &copy.id)?.clone();
    let parked = ops::park_found_copy(rt, ctx, session, &copy, &skill)?;
    record_parked_copy(rt, session, split_event, agent, &parked)?;
    Ok(parked.parked_dir)
}

fn find_copy(rt: &Runtime, session: &MutationSession, path: &Path) -> Option<DeploymentDto> {
    let fs = rt.ports.fs.as_ref();
    let canonical = fs.canonicalize(path).ok();
    session
        .fresh
        .skills
        .iter()
        .flat_map(|skill| skill.deployments.iter())
        .find(|d| {
            d.path == path
                || canonical.as_ref().is_some_and(|canonical| {
                    fs.canonicalize(&d.path).ok().as_ref() == Some(canonical)
                })
        })
        .cloned()
}

/// Adds the parked copy and its `.origin` note to the split row's
/// `remove_copies`, so `restore_event` takes them down with the per-agent
/// copies, and records where it went in the row's `agent_off` marker.
fn record_parked_copy(
    rt: &Runtime,
    session: &mut MutationSession,
    split_event: &EventId,
    agent: &AgentId,
    parked: &ParkedCopy,
) -> Result<(), CoreError> {
    let fs = rt.ports.fs.as_ref();
    let record = session
        .store
        .get(split_event)?
        .ok_or_else(|| CoreError::new(ErrorCode::Io, "the split event is missing"))?;
    let mut copies: Vec<(PathBuf, crate::identity::Fingerprint)> = record
        .inverse
        .as_ref()
        .map(crate::events::parse_restore_remove_copies)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(path, hex)| {
            crate::identity::Fingerprint::parse(&hex)
                .ok()
                .map(|fingerprint| (path, fingerprint))
        })
        .collect();
    if let Some(fingerprint) = crate::events::fingerprint_path(fs, &parked.parked_dir)? {
        copies.push((parked.parked_dir.clone(), fingerprint));
    }
    if let Some(note) = &parked.origin_note {
        if let Some(fingerprint) = crate::events::fingerprint_path(fs, note)? {
            copies.push((note.clone(), fingerprint));
        }
    }
    let patch = crate::events::with_remove_copies(serde_json::json!({}), &copies);
    session
        .store
        .patch_inverse(&session.guard, split_event, patch)?;
    session.store.patch_payload(
        &session.guard,
        split_event,
        serde_json::json!({
            "agent_off": { "agent": agent.as_str(), "parked_to": parked.parked_dir },
        }),
    )
}
