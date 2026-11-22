// ============================================================================
// Skills Module - skill_listing
// A thin adapter over `skill_studio_core::listing_visibility::listing`: which
// skills each agent lists for the model with a description, with a name only,
// or not at all, at the current budget or a budget the owner tries.
// ============================================================================

use skill_studio_core::identity::CorrelationId;
use skill_studio_core::listing_visibility::{self, ListingReport, ListingRequest};
use skill_studio_core::ops::Operation;
use skill_studio_core::ports::OpContext;

/// Reads only. Skill Studio never writes an agent's settings file, so the
/// report carries the setting text and file for the owner to change.
#[tauri::command]
pub async fn skill_listing(
    trace: crate::timing_log::IpcTrace,
    request: ListingRequest,
    app: tauri::AppHandle,
) -> Result<ListingReport, String> {
    crate::timing_log::time_core_command_blocking(&app, trace, "skill_listing", move || {
        let rt = super::core_runtime::build_runtime_write()?;
        let ctx = OpContext::uncancellable(CorrelationId(ulid::Ulid::generate().to_string()));
        let result = listing_visibility::listing(&rt, &ctx, &request);
        super::core_runtime::finish_core_command(Operation::Listing, &rt, &ctx, result)
    })
    .await
}
