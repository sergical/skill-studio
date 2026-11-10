use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::files::{same_skill, Artifact, Snapshot};
use crate::process::ProcessResult;
use crate::suite::{validate_id, CheckResult, EvaluationOutcome, TestSuite};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Queued,
    Running,
    Completed,
    Cancelled,
    Interrupted,
    Failed,
}

impl RunState {
    pub fn active(self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }

    pub fn can_transition(self, next: Self) -> bool {
        self == next
            || matches!(
                (self, next),
                (
                    Self::Queued,
                    Self::Running | Self::Cancelled | Self::Interrupted | Self::Failed
                ) | (
                    Self::Running,
                    Self::Completed | Self::Cancelled | Self::Interrupted | Self::Failed
                )
            )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TrialState {
    Queued,
    Running,
    Checking,
    Finished,
    Cancelled,
    Interrupted,
}

impl TrialState {
    pub fn can_transition(self, next: Self) -> bool {
        self == next
            || matches!(
                (self, next),
                (
                    Self::Queued,
                    Self::Running | Self::Cancelled | Self::Interrupted
                ) | (
                    Self::Running,
                    Self::Checking | Self::Finished | Self::Cancelled | Self::Interrupted
                ) | (
                    Self::Checking,
                    Self::Finished | Self::Cancelled | Self::Interrupted
                )
            )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HumanReview {
    pub at: DateTime<Utc>,
    pub accepted: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Trial {
    pub id: String,
    pub case_id: String,
    pub variant_id: String,
    pub repetition: u32,
    pub state: TrialState,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub client_version: Option<String>,
    pub client_binary: Option<String>,
    pub isolation: String,
    pub skill_hash: Option<String>,
    pub result: Option<ProcessResult>,
    pub judge: Option<ProcessResult>,
    pub checks: Vec<CheckResult>,
    pub outcome: EvaluationOutcome,
    pub artifacts: Vec<Artifact>,
    pub reviews: Vec<HumanReview>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunRecord {
    pub schema_version: u32,
    pub id: String,
    pub suite: TestSuite,
    pub input_hash: String,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub state: RunState,
    pub trials: Vec<Trial>,
    pub error: Option<String>,
}

impl RunRecord {
    pub fn transition(&mut self, next: RunState) -> Result<(), String> {
        if !self.state.can_transition(next) {
            return Err(format!(
                "Invalid run transition: {:?} to {next:?}",
                self.state
            ));
        }
        self.state = next;
        Ok(())
    }

    pub fn summary(&self) -> RunSummary {
        RunSummary {
            id: self.id.clone(),
            suite_id: self.suite.id.clone(),
            name: self.suite.name.clone(),
            skill_path: self.suite.skill_path.clone(),
            created_at: self.created_at.to_rfc3339(),
            state: self.state,
            total: self.trials.len(),
            passed: self
                .trials
                .iter()
                .filter(|t| t.outcome == EvaluationOutcome::Passed)
                .count(),
            failed: self
                .trials
                .iter()
                .filter(|t| t.outcome == EvaluationOutcome::Failed)
                .count(),
            needs_review: self
                .trials
                .iter()
                .filter(|t| {
                    matches!(
                        t.outcome,
                        EvaluationOutcome::NeedsReview | EvaluationOutcome::CheckError
                    )
                })
                .count(),
            not_evaluated: self
                .trials
                .iter()
                .filter(|t| t.outcome == EvaluationOutcome::NotEvaluated)
                .count(),
        }
    }
}

impl Trial {
    pub fn transition(&mut self, next: TrialState) -> Result<(), String> {
        if !self.state.can_transition(next) {
            return Err(format!(
                "Invalid trial transition: {:?} to {next:?}",
                self.state
            ));
        }
        self.state = next;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunSummary {
    pub id: String,
    pub suite_id: String,
    pub name: String,
    pub skill_path: String,
    pub created_at: String,
    pub state: RunState,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub needs_review: usize,
    pub not_evaluated: usize,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FrozenInputs {
    pub suite: TestSuite,
    pub skills: HashMap<String, Snapshot>,
}

#[derive(Clone)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn open(root: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(root.join("runs")).map_err(|e| e.to_string())?;
        fs::create_dir_all(root.join("suites")).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
        let store = Self { root };
        for summary in store.summaries()? {
            if summary.state.active() {
                if let Ok(_lock) = store.lock(&summary.id) {
                    let mut run = store.run(&summary.id)?;
                    if !run.state.active() {
                        store.save_run(&run)?;
                        continue;
                    }
                    crate::recovery::stop_interrupted_processes(&store.run_dir(&run.id)?)?;
                    run.transition(RunState::Interrupted)?;
                    run.finished_at = Some(Utc::now());
                    run.error = Some("The app or runner stopped before this run finished. Completed trial evidence is retained. Start a new run to try again.".into());
                    for trial in &mut run.trials {
                        if matches!(
                            trial.state,
                            TrialState::Queued | TrialState::Running | TrialState::Checking
                        ) {
                            trial.transition(TrialState::Interrupted)?;
                            trial.outcome = EvaluationOutcome::NotEvaluated;
                            trial.finished_at = run.finished_at;
                        }
                    }
                    store.save_run(&run)?;
                }
            }
        }
        Ok(store)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn run_dir(&self, id: &str) -> Result<PathBuf, String> {
        validate_id(id)?;
        Ok(self.root.join("runs").join(id))
    }

    pub fn lock(&self, id: &str) -> Result<File, String> {
        let dir = self.run_dir(id)?;
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(dir.join("active.lock"))
            .map_err(|e| e.to_string())?;
        file.try_lock_exclusive()
            .map_err(|_| "This run is already active in another process".to_string())?;
        Ok(file)
    }

    pub fn save_suite(&self, suite: &TestSuite) -> Result<(), String> {
        suite.validate()?;
        atomic_json(
            &self.root.join("suites").join(format!("{}.json", suite.id)),
            suite,
        )
    }

    pub fn suites(&self) -> Result<Vec<TestSuite>, String> {
        read_directory(&self.root.join("suites"))
    }

    pub fn suites_for_skill(&self, path: &str) -> Result<Vec<TestSuite>, String> {
        Ok(self
            .suites()?
            .into_iter()
            .filter(|suite| same_skill(&suite.skill_path, path))
            .collect())
    }

    pub fn runs_for_skill(&self, path: &str) -> Result<Vec<RunSummary>, String> {
        Ok(self
            .summaries()?
            .into_iter()
            .filter(|run| same_skill(&run.skill_path, path))
            .collect())
    }

    pub fn save_run(&self, run: &RunRecord) -> Result<(), String> {
        let directory = self.run_dir(&run.id)?;
        atomic_json(&directory.join("result.json"), run)?;
        atomic_json(&directory.join("summary.json"), &run.summary())
    }

    pub fn run(&self, id: &str) -> Result<RunRecord, String> {
        let run: RunRecord = read_json(&self.run_dir(id)?.join("result.json"))?;
        if run.schema_version != 1 {
            return Err(format!(
                "Saved run version {} is not supported",
                run.schema_version
            ));
        }
        Ok(run)
    }

    pub fn summaries(&self) -> Result<Vec<RunSummary>, String> {
        let mut result = Vec::new();
        for entry in fs::read_dir(self.root.join("runs")).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let summary = entry.path().join("summary.json");
            let record = entry.path().join("result.json");
            if summary.is_file() {
                result.push(read_json::<RunSummary>(&summary)?);
            } else if record.is_file() {
                result.push(read_json::<RunRecord>(&record)?.summary());
            }
        }
        result.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(result)
    }

    pub fn review(
        &self,
        run_id: &str,
        trial_id: &str,
        accepted: bool,
        reason: String,
    ) -> Result<RunRecord, String> {
        if reason.trim().is_empty() || reason.len() > 16_384 {
            return Err("Give a review reason of 1–16384 characters".into());
        }
        let _lock = self.lock(run_id)?;
        let mut run = self.run(run_id)?;
        if run.state.active() {
            return Err("Wait for the run to finish before reviewing it".into());
        }
        let trial = run
            .trials
            .iter_mut()
            .find(|trial| trial.id == trial_id)
            .ok_or("Trial not found")?;
        trial.reviews.push(HumanReview {
            at: Utc::now(),
            accepted,
            reason,
        });
        self.save_run(&run)?;
        Ok(run)
    }
}

pub fn atomic_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let parent = path.parent().ok_or("Data path has no parent")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    serde_json::to_writer_pretty(&mut temp, value).map_err(|e| e.to_string())?;
    temp.flush().map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(path).map_err(|e| e.to_string())?;
    File::open(parent)
        .and_then(|dir| dir.sync_all())
        .map_err(|e| e.to_string())
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let bytes = fs::read(path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|e| format!("Could not read saved data {}: {e}", path.display()))
}

fn read_directory<T: DeserializeOwned>(path: &Path) -> Result<Vec<T>, String> {
    let mut result = Vec::new();
    for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.path().extension().is_some_and(|ext| ext == "json") {
            result.push(read_json(&entry.path())?);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_and_trial_transitions_do_not_restart_settled_work() {
        let runs = [
            RunState::Queued,
            RunState::Running,
            RunState::Completed,
            RunState::Cancelled,
            RunState::Interrupted,
            RunState::Failed,
        ];
        let allowed = [
            [true, true, false, true, true, true],
            [false, true, true, true, true, true],
            [false, false, true, false, false, false],
            [false, false, false, true, false, false],
            [false, false, false, false, true, false],
            [false, false, false, false, false, true],
        ];
        for (i, from) in runs.into_iter().enumerate() {
            for (j, to) in runs.into_iter().enumerate() {
                assert_eq!(from.can_transition(to), allowed[i][j], "{from:?} to {to:?}");
            }
        }
        let trials = [
            TrialState::Queued,
            TrialState::Running,
            TrialState::Checking,
            TrialState::Finished,
            TrialState::Cancelled,
            TrialState::Interrupted,
        ];
        let allowed = [
            [true, true, false, false, true, true],
            [false, true, true, true, true, true],
            [false, false, true, true, true, true],
            [false, false, false, true, false, false],
            [false, false, false, false, true, false],
            [false, false, false, false, false, true],
        ];
        for (i, from) in trials.into_iter().enumerate() {
            for (j, to) in trials.into_iter().enumerate() {
                assert_eq!(from.can_transition(to), allowed[i][j], "{from:?} to {to:?}");
            }
        }
    }
}
