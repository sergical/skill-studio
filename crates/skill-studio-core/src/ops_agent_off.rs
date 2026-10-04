//! `turn_off_for_agent`: turns a skill in the shared folder off for one
//! agent. The shared folder cannot hide a skill from one reader, so the op
//! splits it (a real copy for every agent that reads the folder) and then
//! parks the chosen agent's copy.
//!
//! The split's journal row is the one row Activity shows. The park step
//! writes no row of its own: it extends the split row's inverse, so one undo
//! (`restore_event`) puts the shared folder back and removes the per-agent
//! copies, the parked copy, and its `.origin` note.

use std::path::{Path, PathBuf};

use crate::dto::{
    AgentOffCheck, AgentOffOutcome, AgentOffRefusal, AgentOffRequest, DeploymentDto,
    InstalledSkillDto, ParkCheckRequest, RestoreRequest, ScanRequest, SplitRequest,
};
use crate::error::{CoreError, ErrorCode};
use crate::harness::{RootRole, ScopeLevel};
use crate::identity::{
    AgentId, BackingRelationship, LifecycleOwnerKind, RootKind, RootRef, RootScope, SkillName,
    PARKED_ROOT_RELATIVE,
};
use crate::ops::{self, Operation, Outcome};
use crate::ops_split::{refuse_whole_folder_link, split_target_root};
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
/// is a whole-folder link into it; and nothing is parked yet for `agent`'s
/// copy of this skill.
///
/// Sequence: `split` for every agent that reads the folder (its own lease
/// and journal row), then the park step for `agent`'s new copy. If the park
/// step fails, the split is undone through `restore_event`, so a failure
/// leaves the shared folder as it was.
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
    let split = ops::split(
        rt,
        ctx,
        &SplitRequest {
            deployment_id: req.deployment_id.clone(),
            harnesses: plan.harnesses.clone(),
        },
    )?;
    let parked_path = match park_agent_copy(rt, ctx, &plan, &split.event_id, &req.agent) {
        Ok(path) => path,
        Err(error) => return Err(undo_split(rt, ctx, &split.event_id, error)),
    };
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

/// Undoes the split after a failed park step and returns the error to
/// report: the park error, plus a note when the undo failed too.
fn undo_split(
    rt: &Runtime,
    ctx: &OpContext,
    split_event: &crate::identity::EventId,
    park_error: CoreError,
) -> CoreError {
    let undo = ops::restore_event(
        rt,
        ctx,
        &RestoreRequest {
            event_id: split_event.clone(),
            force: false,
        },
    );
    match undo {
        Ok(_) => park_error,
        Err(undo_error) => CoreError::new(
            park_error.code,
            format!(
                "{}; undoing the split failed too ({}), so use Undo on the split in Activity",
                park_error.message, undo_error.message
            ),
        ),
    }
}

/// The agents that read the shared folder at `deployment`'s scope, in split
/// order: harnesses whose catalog reads the Universal root, plus harnesses
/// that reach it through a link. A harness with a real copy of its own is
/// not a reader.
fn readers(rt: &Runtime, skill: &InstalledSkillDto, deployment: &DeploymentDto) -> Vec<AgentId> {
    let level = match deployment.root.scope {
        RootScope::Global => ScopeLevel::Global,
        RootScope::Project(_) => ScopeLevel::Project,
    };
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
    let agent_label = rt.ports.catalog.get(&req.agent).map_or_else(
        || req.agent.as_str().to_string(),
        |f| f.display_name.clone(),
    );

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
        if let Err(error) = refuse_whole_folder_link(fs, harness, &root, &canonical_universal_root)
        {
            return Ok(Err(refusal(error.message, true)));
        }
    }

    let agent_root =
        split_target_root(rt, &deployment.root.scope, &req.agent).ok_or_else(|| {
            CoreError::new(
                ErrorCode::InvalidRequest,
                format!("`{}` has no skills folder to copy into", req.agent.as_str()),
            )
        })?;
    let agent_copy = agent_root.join(&skill.name.0);
    let parked = parked_place(rt, &deployment.root.scope, &req.agent, &skill.name)?;
    if fs.symlink_metadata(&parked.dir).is_ok() || parked.slot_name_taken {
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
        agent_copy,
    }))
}

/// Where a plain park of `agent`'s copy would put it.
struct ParkedPlace {
    origin: RootRef,
    parked_root: PathBuf,
    project_key: Option<String>,
    slot_dir: PathBuf,
    dir: PathBuf,
    /// An old flat parked copy whose folder name is this slot's name: moving
    /// into the slot would put the new copy inside it.
    slot_name_taken: bool,
}

fn parked_place(
    rt: &Runtime,
    scope: &RootScope,
    agent: &AgentId,
    skill: &SkillName,
) -> Result<ParkedPlace, CoreError> {
    let fs = rt.ports.fs.as_ref();
    let origin = RootRef {
        scope: scope.clone(),
        kind: RootKind::Harness(agent.clone()),
    };
    let parked_root = rt.scope.home.lexical.join(PARKED_ROOT_RELATIVE);
    let project_key = match scope {
        RootScope::Global => None,
        RootScope::Project(project) => Some(crate::park_layout::project_key(
            &fs.canonicalize(&project.0)
                .unwrap_or_else(|_| project.0.clone()),
        )),
    };
    let slot_dir =
        crate::park_layout::parked_slot_dir(&parked_root, &origin, project_key.as_deref())
            .ok_or_else(|| {
                CoreError::new(
                    ErrorCode::Unsupported,
                    format!("{}'s copy cannot be parked", agent.as_str()),
                )
            })?;
    let slot_name_taken = slot_dir
        .strip_prefix(&parked_root)
        .ok()
        .and_then(|relative| relative.components().next())
        .is_some_and(|top_level| {
            fs.symlink_metadata(&parked_root.join(top_level).join("SKILL.md"))
                .is_ok()
        });
    let dir = slot_dir.join(&skill.0);
    Ok(ParkedPlace {
        origin,
        parked_root,
        project_key,
        slot_dir,
        dir,
        slot_name_taken,
    })
}

/// Parks `agent`'s fresh copy exactly as `ops::park` would, but writes no
/// journal row: the split row's inverse learns about the parked copy and its
/// `.origin` note before the move, so the one undo removes them.
fn park_agent_copy(
    rt: &Runtime,
    ctx: &OpContext,
    plan: &Plan,
    split_event: &crate::identity::EventId,
    agent: &AgentId,
) -> Result<PathBuf, CoreError> {
    let mut session = MutationSession::begin_for(rt, ctx, std::slice::from_ref(&plan.skill))?;
    let result = park_in_session(rt, &mut session, plan, split_event, agent);
    session.finish(rt, ctx);
    result
}

fn park_in_session(
    rt: &Runtime,
    session: &mut MutationSession,
    plan: &Plan,
    split_event: &crate::identity::EventId,
    agent: &AgentId,
) -> Result<PathBuf, CoreError> {
    let fs = rt.ports.fs.as_ref();
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
    let place = parked_place(rt, &copy.root.scope, agent, &plan.skill)?;
    if fs.symlink_metadata(&place.dir).is_ok() {
        return Err(CoreError::new(
            ErrorCode::InvalidRequest,
            "a parked copy from this folder already exists for this skill",
        )
        .at(&place.dir));
    }
    let origin_root_relative = ops::origin_root_relative(rt, &place.origin, &copy.path);
    let copy_fingerprint = crate::events::fingerprint_path(fs, &copy.path)?;

    let mut created_dirs: Vec<PathBuf> = Vec::new();
    let mut written_files: Vec<PathBuf> = Vec::new();
    let result = (|| -> Result<(), CoreError> {
        let parent = place.dir.parent().unwrap_or(&place.dir).to_path_buf();
        created_dirs.extend(ops::ensure_dir_all_tracked(rt, session, fs, &parent)?);
        if let (RootScope::Project(project), Some(key)) = (&place.origin.scope, &place.project_key)
        {
            let key_dir = place
                .parked_root
                .join(crate::park_layout::PARKED_PROJECTS_DIR)
                .join(key);
            let marker = key_dir.join(crate::park_layout::PROJECT_ORIGIN_MARKER);
            write_marker(
                rt,
                session,
                &marker,
                &project.0.to_string_lossy(),
                created_dirs.contains(&key_dir),
                &mut written_files,
            )?;
        }
        let mut note = None;
        if let Some(relative) = &origin_root_relative {
            let marker_dir = place.slot_dir.join(crate::park_layout::COPY_ORIGIN_DIR);
            created_dirs.extend(ops::ensure_dir_all_tracked(rt, session, fs, &marker_dir)?);
            let marker = marker_dir.join(&plan.skill.0);
            write_marker(rt, session, &marker, relative, true, &mut written_files)?;
            note = Some(marker);
        }
        record_parked_copy(
            session,
            fs,
            split_event,
            agent,
            &place.dir,
            copy_fingerprint.as_ref(),
            note.as_deref(),
        )?;
        crate::park_move::move_dir(rt, session, &copy.path, &place.dir)
    })();
    if let Err(error) = result {
        ops::remove_park_scaffolding(rt, session, &written_files, &created_dirs);
        return Err(error);
    }
    Ok(place.dir)
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

/// `undo_on_failure` is false for a marker other parked copies share.
fn write_marker(
    rt: &Runtime,
    session: &MutationSession,
    marker: &Path,
    text: &str,
    undo_on_failure: bool,
    written_files: &mut Vec<PathBuf>,
) -> Result<(), CoreError> {
    let fs = rt.ports.fs.as_ref();
    let scoped = crate::ports::confine(&rt.scope, fs, marker)?;
    fs.write_atomic(&session.guard, &scoped, text.as_bytes())
        .map_err(|e| CoreError::io(marker, e))?;
    if undo_on_failure {
        written_files.push(marker.to_path_buf());
    }
    Ok(())
}

/// Adds the parked copy and its `.origin` note to the split row's
/// `remove_copies`, so `restore_event` takes them down with the per-agent
/// copies. The parked copy has the bytes of the copy being moved, so its
/// fingerprint is known before the move.
fn record_parked_copy(
    session: &mut MutationSession,
    fs: &dyn crate::ports::ScopeFs,
    split_event: &crate::identity::EventId,
    agent: &AgentId,
    parked_dir: &Path,
    copy_fingerprint: Option<&crate::identity::Fingerprint>,
    note: Option<&Path>,
) -> Result<(), CoreError> {
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
    if let Some(fingerprint) = copy_fingerprint {
        copies.push((parked_dir.to_path_buf(), fingerprint.clone()));
    }
    if let Some(note) = note {
        if let Some(fingerprint) = crate::events::fingerprint_path(fs, note)? {
            copies.push((note.to_path_buf(), fingerprint));
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
            "agent_off": { "agent": agent.as_str(), "parked_to": parked_dir },
        }),
    )
}
