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

/// Reading a listing and previewing another budget leave settings unchanged.
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

/// Resolve the owning file from native settings. The webview never supplies a file to open.
#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri owns the IPC arguments on its async executor.
pub fn open_listing_setting(
    key: String,
    project_path: Option<std::path::PathBuf>,
) -> Result<(), String> {
    let rt = super::core_runtime::build_runtime_write()?;
    let file = listing_visibility::listing_setting_source(&rt, project_path, &key)
        .map_err(|err| err.to_string())?;
    if !file.is_file() {
        return Err(format!(
            "No configuration file exists at {} yet.",
            file.display()
        ));
    }
    super::commands::open_in_finder_or_editor(&file.to_string_lossy(), "editor")
}

#[tauri::command(async)]
#[allow(clippy::needless_pass_by_value)] // Tauri owns the IPC arguments on its async executor.
pub fn save_listing_setting(
    key: String,
    project_path: Option<std::path::PathBuf>,
    value: Option<String>,
    revision: String,
) -> Result<(), String> {
    let rt = super::core_runtime::build_runtime_write()?;
    skill_studio_core::listing_setting_edit::save(
        &rt,
        &OpContext::uncancellable(CorrelationId(ulid::Ulid::generate().to_string())),
        project_path,
        &key,
        value.as_deref(),
        &revision,
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}
