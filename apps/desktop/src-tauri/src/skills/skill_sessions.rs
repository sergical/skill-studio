// ============================================================================
// Skills Module - skill_sessions
// A thin adapter over `skill_studio_core::skill_sessions::skill_sessions`:
// for each agent's recent sessions, which skills were listed with a
// description, by name only, or not at all, which were used, and which were
// possibly missed.
// ============================================================================

use skill_studio_core::identity::CorrelationId;
use skill_studio_core::ops::Operation;
use skill_studio_core::ports::OpContext;
use skill_studio_core::skill_sessions::{self, SkillSessionsReport, SkillSessionsRequest};

/// Reads only: session transcripts and the current listing prediction.
#[tauri::command]
pub async fn skill_sessions(
    trace: crate::timing_log::IpcTrace,
    request: SkillSessionsRequest,
    app: tauri::AppHandle,
) -> Result<SkillSessionsReport, String> {
    crate::timing_log::time_core_command_blocking(&app, trace, "skill_sessions", move || {
        let rt = super::core_runtime::build_runtime_write()?;
        let ctx = OpContext::uncancellable(CorrelationId(ulid::Ulid::generate().to_string()));
        let result = skill_sessions::skill_sessions(&rt, &ctx, &request);
        super::core_runtime::finish_core_command(Operation::SkillSessions, &rt, &ctx, result)
    })
    .await
}
