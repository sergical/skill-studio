//! `ops::discard`: deletes one real copy the person chose to give up, for the
//! "parked copy left behind" fix. A skill can end up live and parked at the
//! same origin (a hand `mv`, `dotagents install`, `npx skills add`); "Keep
//! live" discards the parked copy and "Keep parked" discards the live one.
//!
//! `remove` cannot do either: it takes only a Universal copy Skill Studio
//! owns, and refuses a parked copy and an agent's own folder. Here the person
//! has already confirmed which of two copies to drop, so ownership is mostly
//! not asked. A copy an installer owns (`npx skills`, dotagents) is refused:
//! the installer keeps its own record, and only it can drop that cleanly.
//! Like `remove`, the journal row and an archival `backup_paths` copy come
//! before the first write, and the row's `inverse` restores the tree, so
//! Activity can undo the delete. The row reuses `EventKind::Remove`.

use std::path::PathBuf;

use crate::dto::{DiscardOutcome, DiscardRequest};
use crate::error::{CoreError, ErrorCode};
use crate::events::{EventDraft, EventKind, EventStatus};
use crate::identity::{BackingRelationship, LifecycleOwnerKind, RootKind, RootScope};
use crate::ops::Operation;
use crate::ports::{FileKind, MutationSession, OpContext, Runtime, ScopeFs};

/// Deletes one real copy: a parked copy, or a live copy in the Universal
/// folder or an agent's own folder, at global or project scope.
///
/// Preconditions: exclusive lease; the deployment must resolve exactly once
/// and hold its own bytes. A plugin copy, a link (in any root), and a live
/// copy an installer owns are refused. The copy to keep must still be there,
/// as a real folder at the matching origin. Every per-skill link into the
/// folder is removed with it, as `park` does.
pub fn discard(
    rt: &Runtime,
    ctx: &OpContext,
    req: &DiscardRequest,
) -> Result<DiscardOutcome, CoreError> {
    rt.run(Operation::Remove, ctx, || discard_body(rt, ctx, req))
}

fn discard_body(
    rt: &Runtime,
    ctx: &OpContext,
    req: &DiscardRequest,
) -> Result<DiscardOutcome, CoreError> {
    ctx.checkpoint()?;
    let session = MutationSession::begin_for_deployment(rt, ctx, &req.deployment_id);
    ctx.take_timing();
    let mut session = session?;

    let deployment = session.resolve_exact(&req.deployment_id)?.clone();
    let refuse =
        |message: &str| Err(CoreError::new(ErrorCode::Unsupported, message).at(&deployment.path));
    if deployment.plugin.is_some() || matches!(deployment.root.kind, RootKind::PluginCache(_)) {
        return refuse("a plugin copy cannot be deleted; turn it off with /plugin in the agent");
    }
    if deployment.is_symlink || deployment.backing == BackingRelationship::LinkedTo {
        return refuse("a link cannot be deleted here; delete the real folder it points to");
    }
    if !matches!(
        deployment.root.kind,
        RootKind::Universal | RootKind::Harness(_) | RootKind::Parked
    ) {
        return refuse("only a Universal, agent, or parked folder can be deleted");
    }
    // An allowlist: any ledger kind not named here, today's or a future one,
    // belongs to its installer.
    if deployment.root.kind != RootKind::Parked
        && !matches!(
            deployment.owner_kind,
            LifecycleOwnerKind::Manual
                | LifecycleOwnerKind::InRepo
                | LifecycleOwnerKind::Copy
                | LifecycleOwnerKind::Fork
        )
    {
        return refuse(MANAGED_COPY_MESSAGE);
    }
    let skill = crate::ops::resolve_skill(&session.fresh, &deployment.id)?.clone();
    let fs = rt.ports.fs.as_ref();
    check_kept_copy(
        &session,
        fs,
        &deployment,
        &skill.name,
        &req.keep_deployment_id,
    )?;
    let links: Vec<PathBuf> = crate::ops::find_all_links(&skill, &deployment.path, fs)
        .into_iter()
        .map(|d| d.path.clone())
        .collect();
    let link_targets: Vec<(PathBuf, PathBuf)> = links
        .iter()
        .filter_map(|link| fs.read_link(link).ok().map(|target| (link.clone(), target)))
        .collect();

    let id = rt.ports.ids.next_event_id();
    // A parked agent copy keeps a note of where it came from beside it. It
    // goes in the backup, so undo brings the note back with the folder.
    let origin_note = origin_note_path(&deployment, &skill.name.0);
    let mut backup_targets = vec![deployment.path.clone()];
    backup_targets.extend(
        origin_note
            .iter()
            .filter(|note| fs.symlink_metadata(note).is_ok())
            .cloned(),
    );
    let manifest = session
        .store
        .backup_paths(&session.guard, &id, &backup_targets)?;
    let pre_fingerprint = manifest
        .entries
        .first()
        .and_then(|e| e.fingerprint.as_ref());
    let inverse = crate::events::restore_backup_inverse_with_links(
        &deployment.path,
        pre_fingerprint,
        None,
        &link_targets,
    );
    let project_path = match &deployment.root.scope {
        RootScope::Global => None,
        RootScope::Project(project) => Some(project.0.clone()),
    };
    let draft = EventDraft {
        kind: EventKind::Remove,
        skill: skill.name.clone(),
        harness: None,
        scope: Some(crate::ops::scope_label(&deployment.root.scope).to_string()),
        project_path,
        payload: serde_json::json!({
            "deployment_id": deployment.id.as_str(),
            "from": deployment.path,
            "to": serde_json::Value::Null,
            "discarded": true,
            "links": links,
        }),
        inverse: Some(inverse),
        backup_dir: Some(manifest.backup_dir),
    };
    session.store.record(&session.guard, &id, &draft)?;

    // The folder moves first: if the rename fails nothing has changed, so the
    // links stay. A registry row is left alone on purpose: a parked copy
    // returns to the slot that row describes.
    let write_result = (|| -> Result<(), CoreError> {
        let trash = crate::park_move::move_to_trash(rt, &session, &deployment.path)?;
        for link in &links {
            let scoped = crate::ports::confine(&rt.scope, fs, link)?;
            fs.remove_file(&session.guard, &scoped)
                .map_err(|e| CoreError::io(link, e))?;
        }
        crate::park_move::remove_trash(rt, &session, &trash);
        if let Some(note) = &origin_note {
            if let Ok(scoped) = crate::ports::confine(&rt.scope, fs, note) {
                let _ = fs.remove_file(&session.guard, &scoped);
            }
        }
        Ok(())
    })();
    if let Err(e) = write_result {
        let _ = session
            .store
            .finish(&session.guard, &id, EventStatus::Failed, None);
        return Err(e);
    }
    session
        .store
        .finish(&session.guard, &id, EventStatus::Done, None)?;
    session.finish(rt, ctx);
    Ok(DiscardOutcome {
        event_id: id,
        deployment_id: deployment.id,
    })
}

/// What a refused installer-owned copy tells the person to do. The desktop
/// shows the same words for a disabled "Keep parked".
const MANAGED_COPY_MESSAGE: &str =
    "Remove this copy with its installer (npx skills remove / dotagents remove), then try again.";

/// The note beside a parked agent copy that records its origin, if this copy
/// is one.
fn origin_note_path(deployment: &crate::dto::DeploymentDto, name: &str) -> Option<PathBuf> {
    if deployment.root.kind != RootKind::Parked {
        return None;
    }
    Some(
        deployment
            .path
            .parent()?
            .join(crate::park_layout::COPY_ORIGIN_DIR)
            .join(name),
    )
}

/// Refuses unless the copy to keep still exists, as a real folder with a
/// `SKILL.md`, and pairs with `deployment` the way a left-behind pair does: one
/// is parked, and the parked copy's origin is the live copy's root.
fn check_kept_copy(
    session: &MutationSession,
    fs: &dyn ScopeFs,
    deployment: &crate::dto::DeploymentDto,
    skill_name: &crate::identity::SkillName,
    keep_id: &crate::identity::DeploymentId,
) -> Result<(), CoreError> {
    let refuse =
        |message: &str| Err(CoreError::new(ErrorCode::StaleProposal, message).at(&deployment.path));
    let Ok(keep) = session.resolve_exact(keep_id) else {
        return refuse("The copy you meant to keep is gone, so nothing was deleted.");
    };
    let is_real_folder = !keep.is_symlink
        && keep.backing != BackingRelationship::LinkedTo
        && fs
            .symlink_metadata(&keep.path)
            .is_ok_and(|facts| facts.kind == FileKind::Dir)
        && fs.read_dir(&keep.path).is_ok_and(|entries| {
            entries
                .iter()
                .any(|e| e.kind == FileKind::File && e.name.eq_ignore_ascii_case("SKILL.md"))
        });
    if !is_real_folder {
        return refuse(
            "The copy you meant to keep is no longer a real folder, so nothing was deleted.",
        );
    }
    let same_skill = crate::ops::resolve_skill(&session.fresh, keep_id)
        .is_ok_and(|kept| kept.name == *skill_name);
    if !same_skill {
        return refuse("The copy you meant to keep is a different skill, so nothing was deleted.");
    }
    let (live, parked) = if deployment.root.kind == RootKind::Parked {
        (keep, deployment)
    } else {
        (deployment, keep)
    };
    let paired = live.root.kind != RootKind::Parked
        && parked.root.kind == RootKind::Parked
        && parked.parked_origin.as_ref() == Some(&live.root);
    if !paired {
        return refuse("These two copies are no longer a parked copy and its live copy, so nothing was deleted.");
    }
    Ok(())
}
