use std::collections::HashSet;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::protocol::HarnessId;

pub const SUITE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TestSuite {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub skill_path: String,
    pub cases: Vec<TestCase>,
    pub variants: Vec<TestVariant>,
    pub repetitions: u32,
    pub timeout_seconds: u64,
    pub judge: Option<JudgeConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TestCase {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub mode: TestMode,
    pub files: Vec<FixtureFile>,
    pub checks: Vec<Check>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TestMode {
    Discovery,
    Explicit,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FixtureFile {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TestVariant {
    pub id: String,
    pub name: String,
    pub client: HarnessId,
    pub model: Option<String>,
    pub skill: SkillVariant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SkillVariant {
    Candidate,
    WithoutSkill,
    Baseline { path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JudgeConfig {
    pub client: HarnessId,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub id: String,
    pub name: String,
    pub assertion: Assertion,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Assertion {
    OutputContains {
        text: String,
    },
    OutputExcludes {
        text: String,
    },
    OutputJson {
        pointer: String,
        equals: Value,
    },
    FileExists {
        path: String,
    },
    FileContains {
        path: String,
        text: String,
    },
    Command {
        program: String,
        args: Vec<String>,
        expected_exit: i32,
    },
    ToolCalled {
        name: String,
    },
    SkillLoaded {
        expected: bool,
    },
    Rubric {
        criteria: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Passed,
    Failed,
    NeedsReview,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CheckResult {
    pub check_id: String,
    pub name: String,
    pub status: CheckStatus,
    pub reason: String,
    pub evidence: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationOutcome {
    Passed,
    Failed,
    NeedsReview,
    CheckError,
    NotEvaluated,
}

pub fn evaluation_outcome(checks: &[CheckResult]) -> EvaluationOutcome {
    if checks.is_empty() {
        EvaluationOutcome::NotEvaluated
    } else if checks.iter().any(|c| c.status == CheckStatus::Error) {
        EvaluationOutcome::CheckError
    } else if checks.iter().any(|c| c.status == CheckStatus::Failed) {
        EvaluationOutcome::Failed
    } else if checks.iter().any(|c| c.status == CheckStatus::NeedsReview) {
        EvaluationOutcome::NeedsReview
    } else {
        EvaluationOutcome::Passed
    }
}

impl TestSuite {
    pub fn trial_count(&self) -> usize {
        self.cases.len() * self.variants.len() * self.repetitions as usize
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SUITE_VERSION {
            return Err(format!(
                "Suite version {} is not supported; expected {SUITE_VERSION}",
                self.schema_version
            ));
        }
        validate_id(&self.id)?;
        require_text(&self.name, "Suite name")?;
        require_text(&self.skill_path, "Skill path")?;
        if self.cases.is_empty() || self.cases.len() > 100 {
            return Err("A suite needs 1–100 cases".into());
        }
        if self.variants.is_empty() || self.variants.len() > 16 {
            return Err("A suite needs 1–16 variants".into());
        }
        if !(1..=20).contains(&self.repetitions) {
            return Err("Repetitions must be between 1 and 20".into());
        }
        if !(15..=3600).contains(&self.timeout_seconds) {
            return Err("The client timeout must be between 15 and 3600 seconds".into());
        }
        if self.trial_count() > 2000 {
            return Err("A suite can contain at most 2000 client runs".into());
        }
        unique_ids(self.cases.iter().map(|c| c.id.as_str()), "case")?;
        unique_ids(self.variants.iter().map(|v| v.id.as_str()), "variant")?;
        if let Some(judge) = &self.judge {
            if !matches!(judge.client, HarnessId::ClaudeCode | HarnessId::Pi) {
                return Err("Choose Claude Code or pi for the separate judge".into());
            }
            if let Some(model) = &judge.model {
                require_text(model, "Judge model")?;
            }
        }
        for variant in &self.variants {
            if variant.skill == SkillVariant::WithoutSkill && variant.client != HarnessId::Pi {
                return Err("No-skill controls are supported only by pi; choose a previous skill version for other clients".into());
            }
            require_text(&variant.name, "Variant name")?;
            if let Some(model) = &variant.model {
                require_text(model, "Model")?;
            }
            if let SkillVariant::Baseline { path } = &variant.skill {
                require_text(path, "Baseline skill path")?;
            }
        }
        for case in &self.cases {
            require_text(&case.name, "Case name")?;
            require_text(&case.prompt, "Case prompt")?;
            if case.prompt.len() > 128 * 1024 {
                return Err(format!("Case {} has a prompt larger than 128 KiB", case.id));
            }
            if case.checks.is_empty() || case.checks.len() > 100 {
                return Err(format!("Case {} needs 1–100 expected results", case.id));
            }
            unique_ids(case.checks.iter().map(|c| c.id.as_str()), "check")?;
            let mut files = HashSet::new();
            let mut bytes = 0;
            for file in &case.files {
                validate_relative_path(&file.path)?;
                if !files.insert(file.path.as_str()) {
                    return Err(format!(
                        "Case {} repeats fixture path {}",
                        case.id, file.path
                    ));
                }
                bytes += file.content.len();
            }
            if bytes > 8 * 1024 * 1024 || files.len() > 200 {
                return Err(format!(
                    "Case {} exceeds 200 files or 8 MiB of fixture data",
                    case.id
                ));
            }
            for check in &case.checks {
                require_text(&check.name, "Expected result name")?;
                match &check.assertion {
                    Assertion::FileExists { path } | Assertion::FileContains { path, .. } => {
                        validate_relative_path(path)?;
                    }
                    Assertion::Command { program, .. } => require_text(program, "Check command")?,
                    Assertion::Rubric { criteria } => require_text(criteria, "Review criteria")?,
                    Assertion::ToolCalled { name } => require_text(name, "Tool name")?,
                    Assertion::OutputContains { text } | Assertion::OutputExcludes { text } => {
                        require_text(text, "Expected text")?;
                    }
                    Assertion::OutputJson { pointer, .. } => {
                        if !pointer.is_empty() && !pointer.starts_with('/') {
                            return Err("A JSON pointer must be empty or start with '/'".into());
                        }
                    }
                    Assertion::SkillLoaded { .. } => {}
                }
                if let Assertion::FileContains { text, .. } = &check.assertion {
                    require_text(text, "Expected file text")?;
                }
            }
        }
        Ok(())
    }
}

pub fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 80
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!(
            "Invalid id {id:?}; use 1–80 letters, digits, '-' or '_'"
        ));
    }
    Ok(())
}

pub fn validate_relative_path(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.contains('\\')
        || value.contains('\0')
        || value.split('/').any(|part| part.is_empty() || part == ".")
        || Path::new(value)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!(
            "{value:?} must be a relative file path without parent segments"
        ));
    }
    let first = value.split('/').next().unwrap_or_default();
    if [
        ".git",
        ".agents",
        ".claude",
        ".codex",
        ".pi",
        ".opencode",
        ".skill-studio",
    ]
    .contains(&first)
        || ["opencode.json", "opencode.jsonc"].contains(&value)
    {
        return Err(format!("{value:?} is reserved for the test workspace"));
    }
    Ok(())
}

fn require_text(value: &str, label: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.contains('\0') {
        Err(format!("{label} must contain text and no null bytes"))
    } else {
        Ok(())
    }
}

fn unique_ids<'a>(ids: impl Iterator<Item = &'a str>, kind: &str) -> Result<(), String> {
    let mut seen = HashSet::new();
    for id in ids {
        validate_id(id)?;
        if !seen.insert(id) {
            return Err(format!("Duplicate {kind} id: {id}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn suite() -> TestSuite {
        serde_json::from_value(json!({
            "schema_version": 1, "id": "example", "name": "Report writer",
            "skill_path": "/fixtures/report-writer", "repetitions": 2, "timeout_seconds": 120,
            "judge": null,
            "variants": [{"id":"candidate", "name":"Candidate", "client":"codex", "model":null,
                "skill":{"kind":"candidate"}}],
            "cases": [{"id":"report", "name":"Write report", "prompt":"Write the report",
                "mode":"discovery", "files":[{"path":"input.txt", "content":"Facts"}],
                "checks":[{"id":"exists", "name":"Report exists", "assertion":{"kind":"file_exists", "path":"report.md"}}]}]
        })).unwrap()
    }

    #[test]
    fn a_saved_suite_round_trips_without_losing_inputs_or_the_planned_run_count() {
        let suite = suite();
        suite.validate().unwrap();
        let stored = serde_json::to_string(&suite).unwrap();
        let restored: TestSuite = serde_json::from_str(&stored).unwrap();
        assert_eq!(restored.trial_count(), 2);
        assert_eq!(restored.cases[0].files[0].content, "Facts");
        assert_eq!(serde_json::to_string(&restored).unwrap(), stored);
    }

    #[test]
    fn importing_a_typo_or_future_version_cannot_silently_change_the_test() {
        let mut value = serde_json::to_value(suite()).unwrap();
        value["repetitons"] = json!(5);
        assert!(serde_json::from_value::<TestSuite>(value).is_err());
        let mut future = suite();
        future.schema_version = 2;
        assert!(future.validate().unwrap_err().contains("not supported"));
    }

    #[test]
    fn fixtures_cannot_escape_or_replace_client_configuration() {
        for path in [
            "../outside",
            "/tmp/outside",
            "a/../../outside",
            "a\\..\\b",
            ".git/config",
            ".codex/config.toml",
            "opencode.json",
        ] {
            let mut suite = suite();
            suite.cases[0].files[0].path = path.into();
            assert!(suite.validate().is_err(), "accepted fixture {path}");
        }
    }

    #[test]
    fn duplicate_check_ids_and_empty_expectations_are_refused_before_execution() {
        let mut suite = suite();
        let duplicate = suite.cases[0].checks[0].clone();
        suite.cases[0].checks.push(duplicate);
        assert!(suite.validate().unwrap_err().contains("Duplicate check"));
        suite.cases[0].checks.clear();
        assert!(suite.validate().unwrap_err().contains("expected results"));
    }

    #[test]
    fn a_missing_evaluation_or_failed_evaluator_never_counts_as_a_pass() {
        assert_eq!(evaluation_outcome(&[]), EvaluationOutcome::NotEvaluated);
        let check = |status| CheckResult {
            check_id: "rubric".into(),
            name: "Accurate".into(),
            status,
            reason: String::new(),
            evidence: String::new(),
        };
        assert_eq!(
            evaluation_outcome(&[check(CheckStatus::NeedsReview)]),
            EvaluationOutcome::NeedsReview
        );
        assert_eq!(
            evaluation_outcome(&[check(CheckStatus::Passed), check(CheckStatus::Error)]),
            EvaluationOutcome::CheckError
        );
        assert_eq!(
            evaluation_outcome(&[check(CheckStatus::Passed), check(CheckStatus::Failed)]),
            EvaluationOutcome::Failed
        );
    }
}
