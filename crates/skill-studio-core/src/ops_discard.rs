//! `ops::discard`: deletes one real copy the person chose to give up, for the
//! "parked copy left behind" fix. A skill can end up live and parked at the
//! same origin (a hand `mv`, `dotagents install`, `npx skills add`); "Keep
//! live" discards the parked copy and "Keep parked" discards the live one.
//!
//! `remove` cannot do either: it takes only a Universal copy Skill Studio
//! owns, and refuses a parked copy and an agent's own folder. Here the person
//! has already confirmed which of two copies to drop, so ownership is not
//! asked. Like `remove`, the journal row and an archival `backup_paths` copy
//! come before the first write, and the row's `inverse` restores the tree, so
//! Activity can undo the delete. The row reuses `EventKind::Remove`.

use std::path::PathBuf;

use crate::dto::{DiscardOutcome, DiscardRequest};
use crate::error::{CoreError, ErrorCode};
use crate::events::{EventDraft, EventKind, EventStatus};
use crate::identity::{BackingRelationship, RootKind, RootScope};
use crate::ops::Operation;
use crate::ports::{MutationSession, OpContext, Runtime};

/// Deletes one real copy: a parked copy, or a live copy in the Universal
/// folder or an agent's own folder, at global or project scope.
///
/// Preconditions: exclusive lease; the deployment must resolve exactly once
/// and hold its own bytes. A plugin copy and a link are refused. Every
/// per-skill link into the folder is removed with it, as `park` does.
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
    let is_agent_symlink = deployment.is_symlink && deployment.root.kind != RootKind::Universal;
    if deployment.backing == BackingRelationship::LinkedTo || is_agent_symlink {
        return refuse("a link cannot be deleted here; delete the real folder it points to");
    }
    if !matches!(
        deployment.root.kind,
        RootKind::Universal | RootKind::Harness(_) | RootKind::Parked
    ) {
        return refuse("only a Universal, agent, or parked folder can be deleted");
    }

    let skill = crate::ops::resolve_skill(&session.fresh, &deployment.id)?.clone();
    let fs = rt.ports.fs.as_ref();
    let links: Vec<PathBuf> = crate::ops::find_all_links(&skill, &deployment.path, fs)
        .into_iter()
        .map(|d| d.path.clone())
        .collect();
    let link_targets: Vec<(PathBuf, PathBuf)> = links
        .iter()
        .filter_map(|link| fs.read_link(link).ok().map(|target| (link.clone(), target)))
        .collect();

    let id = rt.ports.ids.next_event_id();
    let manifest =
        session
            .store
            .backup_paths(&session.guard, &id, std::slice::from_ref(&deployment.path))?;
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

    let write_result = (|| -> Result<(), CoreError> {
        for link in &links {
            let scoped = crate::ports::confine(&rt.scope, fs, link)?;
            fs.remove_file(&session.guard, &scoped)
                .map_err(|e| CoreError::io(link, e))?;
        }
        // Confine first: the tree walk below deletes without a scoped path.
        crate::ports::confine(&rt.scope, fs, &deployment.path)?;
        crate::ops_remove::remove_tree_best_effort(fs, &deployment.path);
        if fs.symlink_metadata(&deployment.path).is_ok() {
            return Err(
                CoreError::new(ErrorCode::Io, "the folder could not be fully deleted")
                    .at(&deployment.path),
            );
        }
        // A parked agent copy keeps a note of where it came from beside it.
        if deployment.root.kind == RootKind::Parked {
            if let Some(slot_dir) = deployment.path.parent() {
                let marker = slot_dir
                    .join(crate::park_layout::COPY_ORIGIN_DIR)
                    .join(&skill.name.0);
                if let Ok(scoped) = crate::ports::confine(&rt.scope, fs, &marker) {
                    let _ = fs.remove_file(&session.guard, &scoped);
                }
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
