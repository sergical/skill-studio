use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chrono::Utc;

use crate::checks::{apply_judgment, evaluate, judge_prompt};
use crate::clients::{command, inspect, isolation_note, ClientRequest};
use crate::files::{hash_bytes, skill_root, Snapshot};
use crate::process::{run_client, Cancellation, ExecutionState, ProcessResult};
use crate::protocol::HarnessId;
use crate::store::{
    atomic_json, read_json, FrozenInputs, RunRecord, RunState, Store, Trial, TrialState,
};
use crate::suite::{
    evaluation_outcome, Assertion, EvaluationOutcome, SkillVariant, TestMode, TestSuite,
};

pub struct Lab {
    pub store: Store,
    active: Mutex<HashMap<String, Arc<Cancellation>>>,
}

pub struct PreparedRun {
    pub id: String,
    lock: File,
    cancellation: Arc<Cancellation>,
    inputs: FrozenInputs,
}

impl Lab {
    pub fn open(root: PathBuf) -> Result<Self, String> {
        Ok(Self {
            store: Store::open(root)?,
            active: Mutex::default(),
        })
    }

    pub fn prepare(&self, suite: TestSuite) -> Result<PreparedRun, String> {
        suite.validate()?;
        let mut active = self
            .active
            .lock()
            .map_err(|_| "Run registry is unavailable")?;
        if active.len() >= 2 {
            return Err("Two suites are already running. Wait for one to finish.".into());
        }
        let mut skills = HashMap::new();
        for variant in &suite.variants {
            let path = match &variant.skill {
                SkillVariant::Candidate => Some(suite.skill_path.as_str()),
                SkillVariant::Baseline { path } => Some(path.as_str()),
                SkillVariant::WithoutSkill => None,
            };
            if let Some(path) = path {
                let snapshot = Snapshot::read(skill_root(path), false)?;
                snapshot.skill_name()?;
                skills.insert(variant.id.clone(), snapshot);
            }
        }
        let input_hash = hash_bytes(&serde_json::to_vec(&suite.cases).map_err(|e| e.to_string())?);
        let mut trials = Vec::new();
        for repetition in 1..=suite.repetitions {
            for case in &suite.cases {
                for variant in &suite.variants {
                    trials.push(Trial {
                        id: format!("trial-{}", trials.len() + 1),
                        case_id: case.id.clone(),
                        variant_id: variant.id.clone(),
                        repetition,
                        state: TrialState::Queued,
                        started_at: None,
                        finished_at: None,
                        client_version: None,
                        client_binary: None,
                        isolation: isolation_note(variant.client).into(),
                        skill_hash: skills.get(&variant.id).map(|s| s.hash.clone()),
                        result: None,
                        judge: None,
                        checks: vec![],
                        outcome: EvaluationOutcome::NotEvaluated,
                        artifacts: vec![],
                        reviews: vec![],
                        error: None,
                    });
                }
            }
        }
        let id = uuid::Uuid::new_v4().to_string();
        let lock = self.store.lock(&id)?;
        let inputs = FrozenInputs {
            suite: suite.clone(),
            skills,
        };
        let cancellation = Arc::new(Cancellation::default());
        atomic_json(&self.store.run_dir(&id)?.join("inputs.json"), &inputs)?;
        self.store.save_suite(&suite)?;
        self.store.save_run(&RunRecord {
            schema_version: 1,
            id: id.clone(),
            suite,
            input_hash,
            created_at: Utc::now(),
            finished_at: None,
            state: RunState::Queued,
            trials,
            error: None,
        })?;
        active.insert(id.clone(), cancellation.clone());
        Ok(PreparedRun {
            id,
            lock,
            cancellation,
            inputs,
        })
    }

    pub fn cancel(&self, id: &str) -> Result<(), String> {
        let active = self
            .active
            .lock()
            .map_err(|_| "Run registry is unavailable")?;
        if let Some(cancellation) = active.get(id) {
            cancellation.cancel();
        } else if self.store.run(id)?.state.active() {
            return Err("This run belongs to another process. Stop it there.".into());
        }
        Ok(())
    }

    pub async fn execute(&self, prepared: PreparedRun) -> Result<RunRecord, String> {
        let mut result = self.execute_inner(&prepared).await;
        self.active
            .lock()
            .map_err(|_| "Run registry is unavailable")?
            .remove(&prepared.id);
        if let Err(error) = &result {
            if let Ok(mut run) = self.store.run(&prepared.id) {
                if run.state.active() {
                    run.transition(RunState::Failed)?;
                }
                run.finished_at = Some(Utc::now());
                run.error = Some(error.clone());
                for trial in &mut run.trials {
                    if matches!(
                        trial.state,
                        TrialState::Running | TrialState::Checking | TrialState::Queued
                    ) {
                        trial.transition(TrialState::Interrupted)?;
                        trial.outcome = EvaluationOutcome::NotEvaluated;
                    }
                }
                if let Err(save_error) = self.store.save_run(&run) {
                    result = Err(format!("{error}. Could not save the failure: {save_error}"));
                }
            }
        }
        drop(prepared.lock);
        result
    }

    async fn execute_inner(&self, prepared: &PreparedRun) -> Result<RunRecord, String> {
        let mut run = self.store.run(&prepared.id)?;
        run.transition(RunState::Running)?;
        self.store.save_run(&run)?;
        for index in 0..run.trials.len() {
            if prepared.cancellation.is_cancelled() {
                break;
            }
            run.trials[index].transition(TrialState::Running)?;
            run.trials[index].started_at = Some(Utc::now());
            self.store.save_run(&run)?;
            let result = self.execute_trial(&mut run, index, prepared).await;
            if let Err(error) = result {
                run.trials[index].error = Some(error.clone());
                if run.trials[index].result.is_none() {
                    run.trials[index].result = Some(ProcessResult::failed(error));
                }
            }
            let trial = &mut run.trials[index];
            if prepared.cancellation.is_cancelled() {
                trial.transition(TrialState::Cancelled)?;
                trial.outcome = EvaluationOutcome::NotEvaluated;
            } else {
                trial.transition(TrialState::Finished)?;
                trial.outcome = if trial
                    .result
                    .as_ref()
                    .is_some_and(|r| r.state == ExecutionState::Completed)
                {
                    if trial.error.is_some() {
                        EvaluationOutcome::CheckError
                    } else {
                        evaluation_outcome(&trial.checks)
                    }
                } else {
                    EvaluationOutcome::NotEvaluated
                };
            }
            trial.finished_at = Some(Utc::now());
            self.store.save_run(&run)?;
        }
        run.transition(if prepared.cancellation.is_cancelled() {
            RunState::Cancelled
        } else {
            RunState::Completed
        })?;
        run.finished_at = Some(Utc::now());
        for trial in &mut run.trials {
            if trial.state == TrialState::Queued {
                trial.transition(TrialState::Cancelled)?;
                trial.finished_at = run.finished_at;
            }
        }
        self.store.save_run(&run)?;
        Ok(run)
    }

    async fn execute_trial(
        &self,
        run: &mut RunRecord,
        index: usize,
        prepared: &PreparedRun,
    ) -> Result<(), String> {
        let case = prepared
            .inputs
            .suite
            .cases
            .iter()
            .find(|c| c.id == run.trials[index].case_id)
            .ok_or("Case was not saved")?;
        let variant = prepared
            .inputs
            .suite
            .variants
            .iter()
            .find(|v| v.id == run.trials[index].variant_id)
            .ok_or("Variant was not saved")?;
        let snapshot = prepared.inputs.skills.get(&variant.id);
        let directory = self.store.run_dir(&run.id)?.join(&run.trials[index].id);
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let workspace = tempfile::Builder::new()
            .prefix("workspace-")
            .tempdir_in(&directory)
            .map_err(|e| e.to_string())?;
        for file in &case.files {
            let path = workspace.path().join(&file.path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(path, &file.content).map_err(|e| e.to_string())?;
        }
        let skill_name = snapshot
            .map(Snapshot::skill_name)
            .transpose()?
            .unwrap_or_else(|| "no-skill".into());
        let skill_dir = snapshot.map(|_| {
            workspace
                .path()
                .join(match variant.client {
                    HarnessId::ClaudeCode => ".claude/skills",
                    HarnessId::Pi => ".pi/skills",
                    _ => ".agents/skills",
                })
                .join(&skill_name)
        });
        if let (Some(snapshot), Some(path)) = (snapshot, &skill_dir) {
            snapshot.write(path)?;
        }
        let prompt = if case.mode == TestMode::Explicit {
            if let Some(path) = &skill_dir {
                format!(
                    "Read and use the skill at {} for this task.\n\n{}",
                    path.join("SKILL.md").display(),
                    case.prompt
                )
            } else {
                case.prompt.clone()
            }
        } else {
            case.prompt.clone()
        };
        let (binary, version) = inspect(variant.client).await?;
        run.trials[index].client_binary = Some(binary.to_string_lossy().into());
        run.trials[index].client_version = Some(version);
        self.store.save_run(run)?;
        let spec = command(&ClientRequest {
            client: variant.client,
            binary: &binary,
            model: variant.model.as_deref(),
            workspace: workspace.path(),
            skill: skill_dir.as_deref(),
            prompt: &prompt,
            timeout_seconds: prepared.inputs.suite.timeout_seconds,
            judge: false,
        })?;
        atomic_json(
            &directory.join("command.json"),
            &serde_json::json!({"program":spec.program,"args":spec.args,"cwd":spec.cwd,"timeout_seconds":spec.timeout_seconds}),
        )?;
        let result = run_client(
            &spec,
            variant.client,
            &skill_name,
            skill_dir
                .as_ref()
                .map(|path| path.join("SKILL.md"))
                .as_deref(),
            &directory.join("client"),
            prepared.cancellation.clone(),
        )
        .await;
        run.trials[index].result = Some(result.clone());
        run.trials[index].transition(TrialState::Checking)?;
        self.store.save_run(run)?;
        let artifacts = Snapshot::read(workspace.path(), true)?;
        artifacts.write(&directory.join("artifacts"))?;
        run.trials[index].artifacts = artifacts.artifacts(&case.files);
        if result.state != ExecutionState::Completed {
            return Ok(());
        }
        run.trials[index].checks = evaluate(
            &case.checks,
            &result,
            &artifacts,
            workspace.path(),
            &directory.join("checks"),
            prepared.cancellation.clone(),
        )
        .await;
        self.store.save_run(run)?;
        if case
            .checks
            .iter()
            .any(|check| matches!(check.assertion, Assertion::Rubric { .. }))
            && !prepared.cancellation.is_cancelled()
        {
            if let Some(judge) = &prepared.inputs.suite.judge {
                let grade = self
                    .judge(
                        &result,
                        &artifacts,
                        &case.checks,
                        judge,
                        &directory,
                        prepared,
                    )
                    .await;
                match grade {
                    Ok(judgment) => {
                        let failure = if judgment.state == ExecutionState::Completed {
                            apply_judgment(
                                &case.checks,
                                &judgment.output,
                                &mut run.trials[index].checks,
                            )
                            .err()
                        } else {
                            Some(
                                judgment
                                    .error
                                    .clone()
                                    .unwrap_or_else(|| "Judge did not complete".into()),
                            )
                        };
                        if let Some(error) = failure {
                            mark_review_error(&mut run.trials[index], &case.checks, &error);
                        }
                        run.trials[index].judge = Some(judgment);
                    }
                    Err(error) => mark_review_error(&mut run.trials[index], &case.checks, &error),
                }
            }
        }
        Ok(())
    }

    async fn judge(
        &self,
        result: &ProcessResult,
        snapshot: &Snapshot,
        checks: &[crate::suite::Check],
        config: &crate::suite::JudgeConfig,
        directory: &std::path::Path,
        prepared: &PreparedRun,
    ) -> Result<ProcessResult, String> {
        let prompt = judge_prompt(checks, result, snapshot)?;
        let (binary, version) = inspect(config.client).await?;
        let workspace = tempfile::Builder::new()
            .prefix("judge-")
            .tempdir_in(directory)
            .map_err(|e| e.to_string())?;
        let spec = command(&ClientRequest {
            client: config.client,
            binary: &binary,
            model: config.model.as_deref(),
            workspace: workspace.path(),
            skill: None,
            prompt: &prompt,
            timeout_seconds: prepared.inputs.suite.timeout_seconds,
            judge: true,
        })?;
        atomic_json(
            &directory.join("judge-command.json"),
            &serde_json::json!({"program":spec.program,"version":version,"args":spec.args}),
        )?;
        Ok(run_client(
            &spec,
            config.client,
            "no-skill",
            None,
            &directory.join("judge"),
            prepared.cancellation.clone(),
        )
        .await)
    }

    pub fn export(&self, id: &str) -> Result<String, String> {
        let run = self.store.run(id)?;
        serde_json::to_string_pretty(&run).map_err(|e| e.to_string())
    }

    pub fn rerun_suite(&self, id: &str) -> Result<TestSuite, String> {
        let inputs: FrozenInputs = read_json(&self.store.run_dir(id)?.join("inputs.json"))?;
        Ok(inputs.suite)
    }
}

fn mark_review_error(trial: &mut Trial, checks: &[crate::suite::Check], error: &str) {
    for result in &mut trial.checks {
        if checks
            .iter()
            .any(|c| c.id == result.check_id && matches!(c.assertion, Assertion::Rubric { .. }))
        {
            result.status = crate::suite::CheckStatus::NeedsReview;
            result.reason = format!("Automatic review could not finish: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture(root: &std::path::Path) -> TestSuite {
        let skill = root.join("skill");
        std::fs::create_dir(&skill).unwrap();
        std::fs::write(
            skill.join("SKILL.md"),
            "---\nname: report\ndescription: Write a report\n---\nOriginal instructions",
        )
        .unwrap();
        serde_json::from_value(json!({
            "schema_version":1,"id":"suite","name":"Report tests","skill_path":skill,
            "repetitions":1,"timeout_seconds":30,"judge":null,
            "variants":[{"id":"candidate","name":"Candidate","client":"pi","model":null,"skill":{"kind":"candidate"}}],
            "cases":[{"id":"report","name":"Report","prompt":"Write report.md","mode":"discovery","files":[],
                "checks":[{"id":"file","name":"Report exists","assertion":{"kind":"file_exists","path":"report.md"}}]}]
        })).unwrap()
    }

    #[test]
    fn inputs_are_frozen_and_only_unlocked_unfinished_runs_are_recovered() {
        let root = tempfile::tempdir().unwrap();
        let suite = fixture(root.path());
        let lab = Lab::open(root.path().join("data")).unwrap();
        let prepared = lab.prepare(suite.clone()).unwrap();
        let id = prepared.id.clone();
        std::fs::write(
            skill_root(&suite.skill_path).join("SKILL.md"),
            "Changed instructions",
        )
        .unwrap();
        let saved: FrozenInputs =
            read_json(&lab.store.run_dir(&id).unwrap().join("inputs.json")).unwrap();
        assert!(
            String::from_utf8(saved.skills["candidate"].files[0].bytes.clone())
                .unwrap()
                .contains("Original instructions")
        );
        let concurrent = Store::open(root.path().join("data")).unwrap();
        assert_eq!(concurrent.run(&id).unwrap().state, RunState::Queued);
        drop(prepared);
        let recovered = Store::open(root.path().join("data")).unwrap();
        let run = recovered.run(&id).unwrap();
        assert_eq!(run.state, RunState::Interrupted);
        assert_eq!(run.trials[0].outcome, EvaluationOutcome::NotEvaluated);
        assert_eq!(run.trials[0].state, TrialState::Interrupted);
    }

    #[test]
    fn recovery_repairs_a_stale_summary_without_interrupting_completed_evidence() {
        let root = tempfile::tempdir().unwrap();
        let lab = Lab::open(root.path().join("data")).unwrap();
        let prepared = lab.prepare(fixture(root.path())).unwrap();
        let id = prepared.id.clone();
        let mut run = lab.store.run(&id).unwrap();
        run.transition(RunState::Running).unwrap();
        run.transition(RunState::Completed).unwrap();
        run.trials[0].transition(TrialState::Running).unwrap();
        run.trials[0].transition(TrialState::Finished).unwrap();
        run.trials[0].outcome = EvaluationOutcome::Passed;
        atomic_json(&lab.store.run_dir(&id).unwrap().join("result.json"), &run).unwrap();
        drop(prepared);
        let recovered = Store::open(root.path().join("data")).unwrap();
        assert_eq!(recovered.run(&id).unwrap().state, RunState::Completed);
        let summary = &recovered.summaries().unwrap()[0];
        assert_eq!(summary.state, RunState::Completed);
        assert_eq!(summary.passed, 1);
    }

    #[test]
    fn desktop_history_matches_cli_folder_paths_even_after_other_skills_have_many_runs() {
        let root = tempfile::tempdir().unwrap();
        let lab = Lab::open(root.path().join("data")).unwrap();
        let suite = fixture(root.path());
        let prepared = lab.prepare(suite.clone()).unwrap();
        let file = PathBuf::from(&suite.skill_path).join("SKILL.md");
        assert_eq!(
            lab.store
                .suites_for_skill(&file.to_string_lossy())
                .unwrap()
                .len(),
            1
        );
        let mut other = lab.store.run(&prepared.id).unwrap();
        other.suite.skill_path = root.path().join("other-skill").to_string_lossy().into();
        other.state = RunState::Completed;
        for index in 0..101 {
            other.id = format!("other-{index}");
            lab.store.save_run(&other).unwrap();
        }
        assert_eq!(lab.store.summaries().unwrap().len(), 102);
        let history = lab.store.runs_for_skill(&file.to_string_lossy()).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].id, prepared.id);
        #[cfg(unix)]
        {
            let alias = root.path().join("skill-alias");
            std::os::unix::fs::symlink(&suite.skill_path, &alias).unwrap();
            assert_eq!(
                lab.store
                    .runs_for_skill(&alias.join("SKILL.md").to_string_lossy())
                    .unwrap()[0]
                    .id,
                prepared.id
            );
        }
    }

    #[test]
    fn a_human_review_keeps_the_machine_result_and_survives_reopening() {
        let root = tempfile::tempdir().unwrap();
        let lab = Lab::open(root.path().join("data")).unwrap();
        let prepared = lab.prepare(fixture(root.path())).unwrap();
        let id = prepared.id.clone();
        let mut run = lab.store.run(&id).unwrap();
        run.state = RunState::Completed;
        run.trials[0].state = TrialState::Finished;
        run.trials[0].outcome = EvaluationOutcome::Failed;
        lab.store.save_run(&run).unwrap();
        drop(prepared);
        lab.store
            .review(
                &id,
                "trial-1",
                true,
                "The file uses an agreed alternative name".into(),
            )
            .unwrap();
        let stored = Store::open(root.path().join("data"))
            .unwrap()
            .run(&id)
            .unwrap();
        assert_eq!(stored.trials[0].outcome, EvaluationOutcome::Failed);
        assert!(stored.trials[0].reviews[0].accepted);
    }
}
