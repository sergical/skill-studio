// ============================================================================
// Skills Module - skill_split
// "Split" replaces a Universal skill folder with one real copy per chosen
// harness. Thin adapters over `skill_studio_core::ops::split` (the same op
// the CLI `split` command and the MCP `split` tool call) and over
// `ops_split::split_target_root`, which the dialog uses to show the folders
// a split will write before the user confirms.
// ============================================================================

use std::path::PathBuf;

use skill_studio_core::dto::{
    AgentOffCheck, AgentOffOutcome, AgentOffRequest, SplitCopy, SplitOutcome, SplitRequest,
};
use skill_studio_core::identity::{AgentId, CorrelationId, DeploymentId, ProjectRef, RootScope};
use skill_studio_core::ops::{self, Operation, ResultEnvelope};
use skill_studio_core::ports::OpContext;

use super::skill_dto::LifecycleTarget;
use super::skill_park::{emit_snapshot_for_names, skill_names_for_deployments};

fn parse_harnesses(harnesses: &[String]) -> Result<Vec<AgentId>, String> {
    harnesses
        .iter()
        .map(|raw| AgentId::parse_harness(raw).map_err(|e| e.message))
        .collect()
}

#[tauri::command]
pub async fn split_skill(
    target: LifecycleTarget,
    harnesses: Vec<String>,
    app: tauri::AppHandle,
) -> Result<SplitOutcome, String> {
    let state_app = app.clone();
    crate::timing_log::time_command_blocking(&app, "split_skill", move || {
        let raw = target
            .deployment_id
            .as_deref()
            .ok_or("Split requires a copy id")?;
        let deployment_id = DeploymentId::parse(raw).map_err(|e| e.message)?;
        let harnesses = parse_harnesses(&harnesses)?;
        let names = skill_names_for_deployments([&deployment_id]);
        let rt = super::core_runtime::build_runtime_write()?;
        let ctx = OpContext::uncancellable(CorrelationId(ulid::Ulid::new().to_string()));
        let result = ops::split(
            &rt,
            &ctx,
            &SplitRequest {
                deployment_id,
                harnesses,
            },
        );
        let envelope = ResultEnvelope::from_result(Operation::Split, &rt.scope, &ctx, result);
        let outcome = super::core_runtime::to_command_result(envelope)?;
        emit_snapshot_for_names(&state_app, "split_skill", names);
        Ok(outcome)
    })
    .await
}

/// The folders `split_skill` would write for `harnesses`, in order. Reads
/// nothing on disk: it resolves the same paths the op itself uses, so the
/// dialog shows `CODEX_HOME` and the `OpenCode` config root as they are.
#[tauri::command]
pub async fn split_skill_targets(
    skill_name: String,
    project_path: Option<String>,
    harnesses: Vec<String>,
    app: tauri::AppHandle,
) -> Result<Vec<SplitCopy>, String> {
    crate::timing_log::time_command_blocking(&app, "split_skill_targets", move || {
        let harnesses = parse_harnesses(&harnesses)?;
        let rt = super::core_runtime::build_runtime_write()?;
        let scope = match project_path {
            Some(path) => RootScope::Project(ProjectRef(PathBuf::from(path))),
            None => RootScope::Global,
        };
        harnesses
            .into_iter()
            .map(|harness| {
                let root = skill_studio_core::ops_split::split_target_root(&rt, &scope, &harness)
                    .ok_or_else(|| {
                    format!("Split cannot write a copy for {}", harness.as_str())
                })?;
                Ok(SplitCopy {
                    harness,
                    path: root.join(&skill_name),
                })
            })
            .collect()
    })
    .await
}

fn agent_off_request(target: &LifecycleTarget, agent: &str) -> Result<AgentOffRequest, String> {
    let raw = target
        .deployment_id
        .as_deref()
        .ok_or("Turn off for one agent requires a copy id")?;
    Ok(AgentOffRequest {
        deployment_id: DeploymentId::parse(raw).map_err(|e| e.message)?,
        agent: AgentId::parse_harness(agent).map_err(|e| e.message)?,
    })
}

/// Give every agent under the shared folder its own copy, then park
/// `agent`'s copy: one journal event, one undo.
#[tauri::command]
pub async fn turn_off_for_agent(
    target: LifecycleTarget,
    agent: String,
    app: tauri::AppHandle,
) -> Result<AgentOffOutcome, String> {
    let state_app = app.clone();
    crate::timing_log::time_command_blocking(&app, "turn_off_for_agent", move || {
        let request = agent_off_request(&target, &agent)?;
        let names = skill_names_for_deployments([&request.deployment_id]);
        let rt = super::core_runtime::build_runtime_write()?;
        let ctx = OpContext::uncancellable(CorrelationId(ulid::Ulid::new().to_string()));
        let result = skill_studio_core::ops_agent_off::turn_off_for_agent(&rt, &ctx, &request);
        let envelope =
            ResultEnvelope::from_result(Operation::TurnOffForAgent, &rt.scope, &ctx, result);
        let outcome = super::core_runtime::to_command_result(envelope)?;
        emit_snapshot_for_names(&state_app, "turn_off_for_agent", names);
        Ok(outcome)
    })
    .await
}

/// What the confirm shows before `turn_off_for_agent`: the reason it would
/// refuse, and the git warning. Writes nothing.
#[tauri::command]
pub async fn turn_off_check(
    target: LifecycleTarget,
    agent: String,
    app: tauri::AppHandle,
) -> Result<AgentOffCheck, String> {
    crate::timing_log::time_command_blocking(&app, "turn_off_check", move || {
        let request = agent_off_request(&target, &agent)?;
        let rt = super::core_runtime::build_runtime_write()?;
        let ctx = OpContext::uncancellable(CorrelationId(ulid::Ulid::new().to_string()));
        skill_studio_core::ops_agent_off::turn_off_check(&rt, &ctx, &request).map_err(|e| e.message)
    })
    .await
}
