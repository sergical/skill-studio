// Tauri extracts owned request values and State handles for command calls.
#![allow(clippy::needless_pass_by_value)]

use std::sync::Arc;

use skill_studio_eval::clients::{inspect_clients, ClientInfo};
use skill_studio_eval::engine::Lab;
use skill_studio_eval::store::RunRecord;
pub use skill_studio_eval::store::RunSummary;
use skill_studio_eval::suite::TestSuite;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

pub struct LabState(Result<Arc<Lab>, String>);

impl LabState {
    pub fn open(app: &AppHandle) -> Self {
        Self(
            app.path()
                .app_data_dir()
                .map_err(|e| e.to_string())
                .and_then(|root| Lab::open(root.join("skill-lab")))
                .map(Arc::new),
        )
    }

    fn lab(&self) -> Result<Arc<Lab>, String> {
        self.0.clone()
    }
}

#[tauri::command]
pub async fn lab_clients() -> Vec<ClientInfo> {
    inspect_clients().await
}

#[tauri::command]
pub async fn lab_suites(
    state: State<'_, LabState>,
    skill_path: String,
) -> Result<Vec<TestSuite>, String> {
    state.lab()?.store.suites_for_skill(&skill_path)
}

#[tauri::command]
pub async fn lab_save_suite(state: State<'_, LabState>, suite: TestSuite) -> Result<(), String> {
    state.lab()?.store.save_suite(&suite)
}

#[tauri::command]
pub async fn lab_import_suite(json: String) -> Result<TestSuite, String> {
    if json.len() > 16 * 1024 * 1024 {
        return Err("Suite import exceeds 16 MiB".into());
    }
    let suite: TestSuite =
        serde_json::from_str(&json).map_err(|e| format!("Could not import this suite: {e}"))?;
    suite.validate()?;
    Ok(suite)
}

#[tauri::command]
pub async fn lab_start(state: State<'_, LabState>, suite: TestSuite) -> Result<String, String> {
    let lab = state.lab()?;
    let prepare_lab = lab.clone();
    let prepared = tauri::async_runtime::spawn_blocking(move || prepare_lab.prepare(suite))
        .await
        .map_err(|e| e.to_string())??;
    let id = prepared.id.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = lab.execute(prepared).await {
            eprintln!("Skill test failed: {error}");
        }
    });
    Ok(id)
}

#[tauri::command]
pub async fn lab_cancel(state: State<'_, LabState>, id: String) -> Result<(), String> {
    state.lab()?.cancel(&id)
}

#[tauri::command]
pub async fn lab_runs(
    state: State<'_, LabState>,
    skill_path: String,
) -> Result<Vec<RunSummary>, String> {
    state.lab()?.store.runs_for_skill(&skill_path)
}

#[tauri::command]
pub async fn lab_run(state: State<'_, LabState>, id: String) -> Result<RunRecord, String> {
    state.lab()?.store.run(&id)
}

#[tauri::command]
pub async fn lab_review(
    state: State<'_, LabState>,
    id: String,
    trial_id: String,
    accepted: bool,
    reason: String,
) -> Result<RunRecord, String> {
    state.lab()?.store.review(&id, &trial_id, accepted, reason)
}

#[tauri::command]
pub async fn lab_export(
    state: State<'_, LabState>,
    id: String,
    suite_only: bool,
) -> Result<String, String> {
    let lab = state.lab()?;
    if suite_only {
        serde_json::to_string_pretty(&lab.rerun_suite(&id)?).map_err(|e| e.to_string())
    } else {
        lab.export(&id)
    }
}

#[tauri::command]
pub async fn lab_delete_run(state: State<'_, LabState>, id: String) -> Result<(), String> {
    let lab = state.lab()?;
    let _lock = lab.store.lock(&id)?;
    if lab.store.run(&id)?.state.active() {
        return Err("Stop the run before deleting its evidence".into());
    }
    std::fs::remove_dir_all(lab.store.run_dir(&id)?).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn lab_save_export(app: AppHandle, name: String, json: String) -> Result<bool, String> {
    if json.len() > 128 * 1024 * 1024 {
        return Err(
            "Export exceeds 128 MiB. Open the saved run folder to inspect the full evidence."
                .into(),
        );
    }
    let value: serde_json::Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let Some(file) = app
            .dialog()
            .file()
            .add_filter("JSON", &["json"])
            .set_file_name(name)
            .blocking_save_file()
        else {
            return Ok(false);
        };
        let path = file.into_path().map_err(|e| e.to_string())?;
        skill_studio_eval::store::atomic_json(&path, &value)?;
        Ok(true)
    })
    .await
    .map_err(|e| e.to_string())?
}
