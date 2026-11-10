//! The current run and last result of the desktop's background work.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};
use tauri::Emitter;

pub const BACKGROUND_EVENT: &str = "skills://background-status";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundWorker {
    Scan,
    Invocations,
    UpdateCheck,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BackgroundTaskStatus {
    pub worker: BackgroundWorker,
    pub running: bool,
    pub last_started_at: Option<String>,
    pub last_finished_at: Option<String>,
    pub last_error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BackgroundStatus {
    pub revision: u64,
    pub tasks: Vec<BackgroundTaskStatus>,
}
struct TaskRecord {
    worker: BackgroundWorker,
    in_flight: u32,
    last_started_at: Option<String>,
    last_finished_at: Option<String>,
    last_error: Option<String>,
}
struct BackgroundMemory {
    revision: u64,
    tasks: Vec<TaskRecord>,
}
impl BackgroundMemory {
    fn snapshot(&self) -> BackgroundStatus {
        BackgroundStatus {
            revision: self.revision,
            tasks: self
                .tasks
                .iter()
                .map(|task| BackgroundTaskStatus {
                    worker: task.worker,
                    running: task.in_flight != 0,
                    last_started_at: task.last_started_at.clone(),
                    last_finished_at: task.last_finished_at.clone(),
                    last_error: task.last_error.clone(),
                })
                .collect(),
        }
    }
}
struct BackgroundState(Mutex<BackgroundMemory>);
impl Default for BackgroundState {
    fn default() -> Self {
        Self(Mutex::new(BackgroundMemory {
            revision: 0,
            tasks: [
                BackgroundWorker::Scan,
                BackgroundWorker::Invocations,
                BackgroundWorker::UpdateCheck,
            ]
            .into_iter()
            .map(|worker| TaskRecord {
                in_flight: 0,
                worker,
                last_started_at: None,
                last_finished_at: None,
                last_error: None,
            })
            .collect(),
        }))
    }
}
impl BackgroundState {
    fn snapshot(&self) -> BackgroundStatus {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .snapshot()
    }
    fn change(
        &self,
        worker: BackgroundWorker,
        update: impl FnOnce(&mut TaskRecord),
    ) -> BackgroundStatus {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(task) = state.tasks.iter_mut().find(|task| task.worker == worker) {
            update(task);
        }
        state.revision = state.revision.saturating_add(1);
        state.snapshot()
    }
    fn run<T>(
        &self,
        worker: BackgroundWorker,
        work: impl FnOnce() -> Result<T, String>,
        publish: impl Fn(&BackgroundStatus),
    ) -> Result<T, String> {
        publish(&self.change(worker, |task| {
            task.in_flight = task.in_flight.saturating_add(1);
            task.last_started_at = Some(chrono::Utc::now().to_rfc3339());
        }));
        let result = work();
        publish(&self.change(worker, |task| {
            task.in_flight = task.in_flight.saturating_sub(1);
            task.last_finished_at = Some(chrono::Utc::now().to_rfc3339());
            task.last_error = result.as_ref().err().cloned();
        }));
        result
    }
}
fn state() -> &'static BackgroundState {
    static STATE: OnceLock<BackgroundState> = OnceLock::new();
    STATE.get_or_init(BackgroundState::default)
}

pub(crate) fn run<T>(
    app: &tauri::AppHandle,
    worker: BackgroundWorker,
    work: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    state().run(worker, work, |status| {
        let _ = app.emit(BACKGROUND_EVENT, status);
    })
}
pub(crate) fn record_failure(app: &tauri::AppHandle, worker: BackgroundWorker, message: String) {
    let _: Result<(), String> = run(app, worker, || Err(message));
}

#[tauri::command]
pub async fn get_background_status(app: tauri::AppHandle) -> Result<BackgroundStatus, String> {
    crate::timing_log::time_command_blocking(&app, "get_background_status", || {
        Ok(state().snapshot())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlapping_runs_keep_the_worker_running_until_both_finish_or_hide_the_remaining_scan() {
        let state = BackgroundState::default();
        state
            .run(
                BackgroundWorker::Scan,
                || {
                    state.run(BackgroundWorker::Scan, || Ok(()), |_| {})?;
                    assert!(state.snapshot().tasks[0].running);
                    Ok(())
                },
                |_| {},
            )
            .unwrap();
        assert!(!state.snapshot().tasks[0].running);
    }
    #[test]
    fn background_failure_exposes_its_run_and_error_and_a_later_success_clears_it_or_leaves_stale_health(
    ) {
        let state = BackgroundState::default();
        let events = Mutex::new(Vec::new());
        let result: Result<(), String> = state.run(
            BackgroundWorker::Scan,
            || {
                assert!(state.snapshot().tasks[0].running);
                Err("fixture rebuild failed".into())
            },
            |status| events.lock().unwrap().push(status.clone()),
        );
        assert_eq!(result.unwrap_err(), "fixture rebuild failed");
        let failed = state.snapshot();
        assert!(!failed.tasks[0].running);
        assert!(failed.tasks[0].last_started_at.is_some());
        assert!(failed.tasks[0].last_finished_at.is_some());
        assert_eq!(
            failed.tasks[0].last_error.as_deref(),
            Some("fixture rebuild failed")
        );
        state
            .run(BackgroundWorker::Scan, || Ok(()), |_| {})
            .unwrap();
        assert!(state.snapshot().tasks[0].last_error.is_none());
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert!(events[0].revision < events[1].revision);
        assert!(events[0].tasks[0].running);
        assert!(!events[1].tasks[0].running);
    }
}
