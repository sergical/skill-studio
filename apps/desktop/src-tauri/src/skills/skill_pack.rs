// ============================================================================
// Skills Module - skill_pack
// The Packs page's commands: thin adapters over `skill_studio_core::ops`'s
// `pack_list`, `pack_add`, `pack_update` and `pack_remove`, the same calls the
// CLI's `pack` subcommands and the MCP `pack_*` tools make. The core plans
// each action (a dry run returns only the plan) and hands each step to the
// owner tool, so the app writes no lock or manifest file itself.
// ============================================================================

use skill_studio_core::error::CoreError;
use skill_studio_core::identity::CorrelationId;
use skill_studio_core::ops::{self, Operation, Outcome, ResultEnvelope};
use skill_studio_core::pack::{PackListRequest, PackRemote, PackReport};
use skill_studio_core::pack_action::{
    PackActionReport, PackAddRequest, PackRemoveRequest, PackStepState, PackUpdateRequest,
};
use skill_studio_core::ports::{OpContext, Runtime};

fn run_pack_op<T: Outcome>(
    operation: Operation,
    git: bool,
    call: impl FnOnce(&Runtime, &OpContext, Option<&dyn PackRemote>) -> Result<T, CoreError>,
) -> Result<T, String> {
    let rt = super::core_runtime::build_runtime_write()?;
    run_pack_op_on(&rt, operation, git, call)
}

fn run_pack_op_on<T: Outcome>(
    rt: &Runtime,
    operation: Operation,
    git: bool,
    call: impl FnOnce(&Runtime, &OpContext, Option<&dyn PackRemote>) -> Result<T, CoreError>,
) -> Result<T, String> {
    let ctx = OpContext::uncancellable(CorrelationId(ulid::Ulid::generate().to_string()));
    let remote = rt
        .ports
        .spawner
        .clone()
        .filter(|_| git)
        .map(skill_studio_host::GitPackRemote::new);
    let result = call(rt, &ctx, remote.as_ref().map(|r| r as &dyn PackRemote));
    super::core_runtime::to_command_result(ResultEnvelope::from_result(
        operation, &rt.scope, &ctx, result,
    ))
}

/// The skills a real run changed, for the targeted snapshot rebuild.
fn changed_skills(report: &PackActionReport) -> Vec<String> {
    let mut names: Vec<String> = report
        .steps
        .iter()
        .filter(|step| matches!(step.state, PackStepState::Done { .. }))
        .map(|step| step.skill.clone())
        .collect();
    names.sort();
    names.dedup();
    names
}

async fn pack_action(
    app: tauri::AppHandle,
    trace: crate::timing_log::IpcTrace,
    command: &'static str,
    run: impl FnOnce() -> Result<PackActionReport, String> + Send + 'static,
) -> Result<PackActionReport, String> {
    let state_app = app.clone();
    crate::timing_log::time_command_blocking(&app, trace, command, move || {
        let report = run()?;
        if !report.dry_run {
            super::skill_park::emit_snapshot_for_names(
                &state_app,
                command,
                changed_skills(&report),
            );
        }
        Ok(report)
    })
    .await
}

/// Every installed pack. `remote` asks each git source for behind and new skills.
#[tauri::command]
pub async fn pack_list(
    trace: crate::timing_log::IpcTrace,
    request: PackListRequest,
    app: tauri::AppHandle,
) -> Result<PackReport, String> {
    crate::timing_log::time_command_blocking(&app, trace, "pack_list", move || {
        run_pack_op(Operation::PackList, request.remote, |rt, ctx, remote| {
            ops::pack_list(rt, ctx, &request, remote)
        })
    })
    .await
}

/// Plans (`dry_run`) or runs `npx skills add` for the skills of one git pack.
/// With no skill names, git lists the pack's skills.
#[tauri::command]
pub async fn pack_add(
    trace: crate::timing_log::IpcTrace,
    request: PackAddRequest,
    app: tauri::AppHandle,
) -> Result<PackActionReport, String> {
    pack_action(app, trace, "pack_add", move || {
        run_pack_op(
            Operation::PackAdd,
            request.skills.is_empty(),
            |rt, ctx, remote| ops::pack_add(rt, ctx, &request, remote),
        )
    })
    .await
}

/// Plans (`dry_run`) or runs the update of one pack, or of every pack.
#[tauri::command]
pub async fn pack_update(
    trace: crate::timing_log::IpcTrace,
    request: PackUpdateRequest,
    app: tauri::AppHandle,
) -> Result<PackActionReport, String> {
    pack_action(app, trace, "pack_update", move || {
        run_pack_op(Operation::PackUpdate, false, |rt, ctx, _| {
            ops::pack_update(rt, ctx, &request)
        })
    })
    .await
}

/// Plans (`dry_run`) or runs the removal of one pack.
#[tauri::command]
pub async fn pack_remove(
    trace: crate::timing_log::IpcTrace,
    request: PackRemoveRequest,
    app: tauri::AppHandle,
) -> Result<PackActionReport, String> {
    pack_action(app, trace, "pack_remove", move || {
        run_pack_op(Operation::PackRemove, false, |rt, ctx, _| {
            ops::pack_remove(rt, ctx, &request)
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use skill_studio_core::install_tools::NODE_MISSING;

    /// The text Packs -> Add pack shows on a Mac without Node.js, through the
    /// same runtime and error conversion as the `pack_add` command. Before
    /// the fix, the plan listed the skills and Confirm failed each one with
    /// "No such file or directory (os error 2)". Fails if the dry run plans.
    #[test]
    fn add_pack_without_node_shows_the_install_node_sentence_or_plans_steps_that_fail() {
        let home = tempfile::tempdir().expect("temp home");
        let _opencode_home = crate::skills::test_support::OpencodeHomeGuard::new(home.path());
        let rt = super::super::core_runtime::build_runtime_write_at_with_search_dirs(
            home.path(),
            &home.path().join(".skill-studio"),
            vec!["/usr/bin".into(), "/bin".into()],
        )
        .expect("desktop runtime");
        let request = PackAddRequest {
            source: "acme/team-skills".to_string(),
            dry_run: true,
            ..PackAddRequest::default()
        };

        let shown = run_pack_op_on(&rt, Operation::PackAdd, true, |rt, ctx, remote| {
            ops::pack_add(rt, ctx, &request, remote)
        })
        .expect_err("a plan without Node");

        assert_eq!(shown, NODE_MISSING);
    }
}
