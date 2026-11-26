// ============================================================================
// Skills Module - skill_create
// The Add skill sheet's Paste tab: a new skill made from pasted text, such as
// a SKILL.md copied from Slack or a gist. `check_pasted_skill` builds the
// file and runs the same validator the scanner uses, so the sheet shows the
// name, description and problems before anything is written.
// `create_pasted_skill` builds the same file and writes it through the core
// Copy install (`ops::install`), so the new skill is a Global Universal Copy
// that Skill Studio owns: recorded in the copies registry, undoable, and
// refused when anything already sits at its folder.
// ============================================================================

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use skill_studio_core::dto::{InstallFile, InstallLinkMode, InstallMethod, InstallRequest};
use skill_studio_core::error::ErrorCode;
use skill_studio_core::frontmatter::{
    is_valid_skill_name, parse_frontmatter, FrontmatterParseResult,
};
use skill_studio_core::identity::{CorrelationId, RootScope, SkillDestination, SkillName};
use skill_studio_core::ops::{self, Operation};
use skill_studio_core::ports::{OpContext, Runtime};

use tauri::Manager;

use super::skill_refresh::{self, SkillRefreshState};
use crate::timing_log::CommandError;

use super::agents::AgentId;
use super::skill_dto::AddSkillResult;
use super::skill_fork_registry::AddMethod;
use super::skill_install::{core_harness, finish_install, InstallAdapterOutcome};

/// What the user pasted. `name` and `description` fill the frontmatter only
/// when `text` has none of its own.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct PastedSkillDraft {
    pub text: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// The SKILL.md a draft becomes, and what the validator says about it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PastedSkillCheck {
    /// `false` when the pasted text has no `---` frontmatter, so the sheet
    /// asks for a name and a description.
    pub has_frontmatter: bool,
    pub name: Option<String>,
    pub description: Option<String>,
    /// `validate_skill`'s violations, the same strings a scanned skill shows.
    pub problems: Vec<String>,
    /// `false` when the file has no usable folder name, no description, or
    /// frontmatter that does not parse.
    pub can_save: bool,
}

/// `create_pasted_skill`'s request: the draft plus the agents the sheet
/// would tick for a Global Universal install (`defaultSharedInstallAgents`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreatePastedSkillRequest {
    pub draft: PastedSkillDraft,
    #[serde(default)]
    pub agents: Vec<AgentId>,
    /// Link or copy into each chosen harness folder that is not the shared
    /// folder. Ignored when the choice writes one folder only.
    #[serde(default)]
    pub link_mode: InstallLinkMode,
}

#[derive(Serialize)]
struct ComposedFrontmatter<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
}

/// Pasted text without leading blank lines, trailing blank lines or Windows
/// line endings, so a fence copied with a blank line above it still counts as
/// frontmatter. Indentation inside the text is kept: an indented first line is
/// a Markdown code block.
fn normalized_text(draft: &PastedSkillDraft) -> String {
    let text = draft.text.replace("\r\n", "\n");
    let mut lines: Vec<&str> = text.lines().skip_while(|l| l.trim().is_empty()).collect();
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

/// The SKILL.md text a draft writes. Text without a fence gets one built
/// from the name and description fields.
fn compose_skill_md(draft: &PastedSkillDraft) -> String {
    let text = normalized_text(draft);
    if !matches!(parse_frontmatter(&text), FrontmatterParseResult::Absent) {
        return format!("{text}\n");
    }
    fn non_empty(s: &str) -> Option<&str> {
        Some(s.trim()).filter(|t| !t.is_empty())
    }
    let frontmatter = ComposedFrontmatter {
        name: non_empty(&draft.name),
        description: non_empty(&draft.description),
    };
    let yaml = if frontmatter.name.is_none() && frontmatter.description.is_none() {
        String::new()
    } else {
        // Two optional strings always serialize.
        let yaml = skill_studio_core::yaml::to_string(&frontmatter).unwrap_or_default();
        if yaml.ends_with('\n') {
            yaml
        } else {
            format!("{yaml}\n")
        }
    };
    format!("---\n{yaml}---\n\n{text}\n")
}

fn check_composed(draft: &PastedSkillDraft, content: &str) -> PastedSkillCheck {
    let parsed = parse_frontmatter(content);
    let frontmatter = parsed.as_frontmatter();
    let name = frontmatter
        .and_then(|f| f.name.clone())
        .filter(|n| !n.is_empty());
    let description = frontmatter
        .and_then(|f| f.description.clone())
        .filter(|d| !d.trim().is_empty());
    // The folder is named after the skill, so `name` never differs from it.
    let problems = skill_studio_core::frontmatter_repair::validate_skill_md(
        name.as_deref().unwrap_or_default(),
        content,
        &parsed,
    );
    let can_save = !matches!(parsed, FrontmatterParseResult::Invalid(_))
        && name.as_deref().is_some_and(is_valid_skill_name)
        && description.is_some();
    PastedSkillCheck {
        has_frontmatter: !matches!(
            parse_frontmatter(&normalized_text(draft)),
            FrontmatterParseResult::Absent
        ),
        name,
        description,
        problems,
        can_save,
    }
}

/// Builds the file and validates it, without writing anything.
pub fn check_draft(draft: &PastedSkillDraft) -> PastedSkillCheck {
    check_composed(draft, &compose_skill_md(draft))
}

/// Writes the draft as a new Global Universal Copy, refusing a draft
/// `check_draft` would not save and any skill already at its folder.
pub(crate) fn create_pasted_skill_with(
    rt: &Runtime,
    request: &CreatePastedSkillRequest,
) -> Result<AddSkillResult, CommandError> {
    let content = compose_skill_md(&request.draft);
    let check = check_composed(&request.draft, &content);
    let (true, Some(name)) = (check.can_save, check.name) else {
        let reason = check
            .problems
            .first()
            .cloned()
            .unwrap_or_else(|| "the skill needs a name and a description".to_string());
        return Err(CommandError::with_code(
            ErrorCode::InvalidRequest,
            format!("Cannot save this skill: {reason}"),
        ));
    };
    let harnesses = std::iter::once(Ok(skill_studio_core::identity::AgentId::from(
        skill_studio_core::install_targets::UNIVERSAL_TARGET,
    )))
    .chain(request.agents.iter().copied().map(core_harness))
    .collect::<Result<Vec<_>, _>>()?;
    let install = InstallRequest {
        skill: SkillName(name),
        method: InstallMethod::Copy,
        scope: RootScope::Global,
        harnesses,
        link_mode: request.link_mode,
        destination: SkillDestination::Universal,
        files: vec![InstallFile {
            relative_path: PathBuf::from("SKILL.md"),
            contents: content.into_bytes(),
            mode: None,
        }],
        source: None,
        trust_identity: None,
        trust_confirmed: false,
        save_as_preference: false,
    };
    let ctx = OpContext::uncancellable(CorrelationId(ulid::Ulid::generate().to_string()));
    let result = ops::install(rt, &ctx, &install);
    let outcome = super::core_runtime::finish_core_command(Operation::Install, rt, &ctx, result)?;
    match finish_install(AddMethod::Copy, outcome) {
        InstallAdapterOutcome::Result(result) => Ok(result),
        InstallAdapterOutcome::NeedsTrust { identity } => Err(CommandError::with_code(
            ErrorCode::InvalidRequest,
            format!("a pasted skill has no source to trust ({identity})"),
        )),
    }
}

#[tauri::command]
pub async fn check_pasted_skill(draft: PastedSkillDraft) -> PastedSkillCheck {
    check_draft(&draft)
}

#[tauri::command]
pub async fn create_pasted_skill(
    trace: crate::timing_log::IpcTrace,
    request: CreatePastedSkillRequest,
    app: tauri::AppHandle,
) -> Result<AddSkillResult, String> {
    let reconcile_app = app.clone();
    let work = async move {
        let joined = crate::timing_log::spawn_blocking_in_span(move || {
            let rt = super::core_runtime::build_runtime_write()?;
            let result = create_pasted_skill_with(&rt, &request)?;
            // The sheet opens this skill's page next, so put it in the snapshot first, as Add by
            // source does; the folder watcher alone would leave the page on the old snapshot.
            let reconciled = reconcile_app.try_state::<SkillRefreshState>().map(|state| {
                skill_refresh::reconcile_skill_names_and_emit(
                    &reconcile_app,
                    state.inner(),
                    [result.name.clone()],
                    &[],
                )
            });
            if !matches!(reconciled, Some(Ok(()))) {
                skill_refresh::request_snapshot_rebuild(&reconcile_app);
            }
            Ok::<_, CommandError>(result)
        })
        .await;
        crate::timing_log::join_result_to_err("create_pasted_skill", joined)
    };
    crate::timing_log::time_command_async(&app, trace, "create_pasted_skill", work)
        .await
        .map_err(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use skill_studio_core::harness::HarnessCatalog;
    use skill_studio_core::ports::Ports;
    use skill_studio_core::RuntimeScope;
    use std::path::Path;
    use std::sync::Arc;

    fn test_runtime(home: &Path) -> Runtime {
        let catalog = Arc::new(HarnessCatalog::builtin());
        let scope = RuntimeScope::fixture(home.to_path_buf());
        let db_path = scope.history_root.join("events.sqlite3");
        let ports: Ports =
            skill_studio_host::default_ports_with_history(home.join("leases"), catalog, db_path);
        Runtime::new(&scope, ports).unwrap()
    }

    fn request(text: &str, name: &str, description: &str) -> CreatePastedSkillRequest {
        CreatePastedSkillRequest {
            draft: PastedSkillDraft {
                text: text.to_string(),
                name: name.to_string(),
                description: description.to_string(),
            },
            agents: vec![],
            link_mode: InstallLinkMode::Link,
        }
    }

    const PASTED: &str =
        "\n---\nname: write-tests\ndescription: Writes tests for the change.\n---\n\nRun the tests.\n";

    #[test]
    fn a_pasted_skill_md_becomes_a_new_global_copy_and_a_second_paste_of_that_name_is_refused() {
        let home = tempfile::tempdir().unwrap();
        let rt = test_runtime(home.path());

        let result = create_pasted_skill_with(&rt, &request(PASTED, "", "")).unwrap();

        assert_eq!(result.name, "write-tests");
        let folder = home.path().join(".agents/skills/write-tests");
        assert_eq!(
            std::fs::read_to_string(folder.join("SKILL.md")).unwrap(),
            PASTED.trim_start(),
        );
        // Skill Studio owns it: the copies registry names the folder.
        let registry =
            std::fs::read_to_string(home.path().join(".agents/skill-studio.json")).unwrap();
        assert!(registry.contains("\"copies\""), "{registry}");
        assert!(registry.contains("write-tests"), "{registry}");

        let edited = PASTED.replace("Run the tests.", "Something else.");
        let refused = create_pasted_skill_with(&rt, &request(&edited, "", "")).unwrap_err();
        assert!(
            String::from(refused).contains("already exists"),
            "a second paste must not overwrite the first"
        );
        assert_eq!(
            std::fs::read_to_string(folder.join("SKILL.md")).unwrap(),
            PASTED.trim_start(),
        );
    }

    #[test]
    fn a_pasted_unquoted_colon_gets_the_same_note_the_scanner_gives() {
        let check = check_draft(&PastedSkillDraft {
            text: "---\nname: testing\ndescription: Use when: testing\n---\n\nRun tests.".into(),
            name: String::new(),
            description: String::new(),
        });
        assert!(
            check
                .problems
                .iter()
                .any(|p| p.ends_with(skill_studio_core::frontmatter_repair::UNQUOTED_COLON_NOTE)),
            "{:?}",
            check.problems
        );
    }

    #[test]
    fn pasted_body_keeps_its_indentation_so_a_leading_code_block_stays_one() {
        let draft = PastedSkillDraft {
            text: "\n\n    npm test\n\nRun the tests first.  \n\n".to_string(),
            name: "run-tests".to_string(),
            description: "Use before a commit".to_string(),
        };
        let content = compose_skill_md(&draft);
        assert!(
            content.ends_with("\n    npm test\n\nRun the tests first.  \n"),
            "{content}"
        );
    }

    #[test]
    fn body_text_with_a_name_and_description_gets_frontmatter_the_validator_accepts() {
        let check = check_draft(&PastedSkillDraft {
            text: "Say banana: always.".to_string(),
            name: "say-banana".to_string(),
            description: "Use when: the user wants bananas".to_string(),
        });
        assert_eq!(
            check,
            PastedSkillCheck {
                has_frontmatter: false,
                name: Some("say-banana".to_string()),
                description: Some("Use when: the user wants bananas".to_string()),
                problems: vec![],
                can_save: true,
            }
        );
    }
}
