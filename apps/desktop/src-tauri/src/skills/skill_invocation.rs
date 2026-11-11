// ============================================================================
// Skill Studio - Desktop invocation adapter
// Validates snapshot targets and reconciles after shared core writes.
// ============================================================================

use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tauri::Manager;

use super::commands::canonicalize_skill_md;
use super::frontmatter::InvocationPolicy;
use super::skill_deployment::parse_deployment_id;
use super::skill_dto::{BulkTargetResult, Deployment};
use super::skill_refresh::{self, SkillRefreshState, SkillSnapshot};

/// Resolves one invocation edit to its exact lexical deployment before the
/// requested `SKILL.md` is canonicalized. This prevents separate deployment
/// paths that resolve to one directory from losing their harness identity.
fn exact_snapshot_invocation_deployment<'a>(
    snapshot: &'a SkillSnapshot,
    name: &str,
    requested_skill_md: &Path,
) -> Result<&'a Deployment, String> {
    if requested_skill_md
        .file_name()
        .and_then(|file| file.to_str())
        != Some("SKILL.md")
    {
        return Err(format!(
            "Invocation target is stale: {} is not a SKILL.md path",
            requested_skill_md.display()
        ));
    }
    let requested_dir = requested_skill_md.parent().ok_or_else(|| {
        format!(
            "Invocation target is stale: {} has no deployment directory",
            requested_skill_md.display()
        )
    })?;
    let mut matching = snapshot.skills.iter().flat_map(|skill| {
        skill
            .deployments
            .iter()
            .filter(move |deployment| Path::new(&deployment.path) == requested_dir)
            .map(move |deployment| (skill, deployment))
    });
    let (skill, deployment) = matching.next().ok_or_else(|| {
        format!(
            "Invocation target is stale: {} is not an exact deployment in the current snapshot",
            requested_skill_md.display()
        )
    })?;
    if matching.next().is_some() {
        return Err(format!(
            "Invocation target is ambiguous: {} matches more than one copy",
            requested_skill_md.display()
        ));
    }
    if skill.name != name {
        return Err(format!(
            "Invocation target is stale: {} belongs to {}, not {name}",
            requested_skill_md.display(),
            skill.name
        ));
    }

    let parsed = parse_deployment_id(&deployment.id).ok_or_else(|| {
        format!(
            "Invocation target is stale: deployment {} has an invalid identity",
            deployment.id
        )
    })?;
    let codex_identity_mismatch = (parsed.slot == "codex") != (deployment.agent == "Codex");
    if parsed.name != skill.name
        || parsed.scope != deployment.scope
        || parsed.destination != deployment.destination
        || parsed.project_path != deployment.project_path
        || parsed.lexical_path != Path::new(&deployment.path)
        || codex_identity_mismatch
    {
        return Err(format!(
            "Invocation target is stale: copy {} no longer matches its snapshot identity",
            deployment.id
        ));
    }

    Ok(deployment)
}

/// Validates `path` against `snapshot` the way every invocation write must,
/// then writes it. Shared by the single and the batch command so both refuse
/// the same stale, plugin-owned and non-SKILL.md targets.
fn write_invocation_target(
    snapshot: &SkillSnapshot,
    name: &str,
    path: &str,
    policy: InvocationPolicy,
) -> Result<(), String> {
    let path_buf = PathBuf::from(path);
    let deployment = exact_snapshot_invocation_deployment(snapshot, name, &path_buf)?;
    if deployment.plugin.is_some() {
        return Err("Skill is managed by a plugin and cannot be edited here".to_string());
    }
    canonicalize_skill_md(&path_buf, path)?;
    let rt = super::core_runtime::build_runtime_write()?;
    let inventory = skill_studio_core::ops::scan(
        &rt,
        &skill_studio_core::OpContext::uncancellable(skill_studio_core::identity::CorrelationId(
            "invocation-target".into(),
        )),
        &skill_studio_core::dto::ScanRequest {
            skills: vec![skill_studio_core::identity::SkillName(name.into())],
            timings: false,
        },
    )
    .map_err(|e| e.message)?;
    let id = inventory
        .skills
        .iter()
        .flat_map(|s| &s.deployments)
        .find(|d| d.path == Path::new(&deployment.path))
        .map(|d| d.id.clone())
        .ok_or("Invocation target is stale")?;
    skill_studio_core::ops_invocation::invocation(
        &rt,
        &skill_studio_core::OpContext::uncancellable(skill_studio_core::identity::CorrelationId(
            "desktop-invocation".into(),
        )),
        &skill_studio_core::ops_invocation::InvocationRequest {
            skill: Some(skill_studio_core::identity::SkillName(name.into())),
            policy: Some(policy),
            deployment_id: Some(id),
        },
    )
    .map(|_| ())
    .map_err(|e| e.message)
}

fn read_snapshot(state: &SkillRefreshState, path: &str) -> Result<SkillSnapshot, String> {
    state
        .snapshot
        .read()
        .map_err(|error| format!("Snapshot lock poisoned: {error}"))?
        .clone()
        .ok_or_else(|| format!("Invocation target is stale: {path} is not an installed skill"))
}

/// One SKILL.md a batch invocation change should write.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InvocationTarget {
    pub name: String,
    pub path: String,
}

/// Writes every target against one `snapshot`, then hands the names that were
/// written to `reconcile` exactly once. A failing target is reported on its
/// own result and does not stop the rest; with no successful write there is
/// nothing to reconcile.
fn apply_invocation_targets(
    snapshot: &SkillSnapshot,
    targets: &[InvocationTarget],
    policy: InvocationPolicy,
    reconcile: impl FnOnce(Vec<String>) -> Result<(), String>,
) -> Vec<BulkTargetResult> {
    let mut written: Vec<String> = Vec::new();
    let results = BulkTargetResult::collect(targets, |target| {
        write_invocation_target(snapshot, &target.name, &target.path, policy)?;
        written.push(target.name.clone());
        Ok(())
    });
    if !written.is_empty() {
        if let Err(error) = reconcile(written) {
            eprintln!("[set_skills_invocation] targeted snapshot reconciliation failed: {error}");
        }
    }
    results
}

/// `set_skill_invocation` for many SKILL.md files at once. One reconcile at
/// the end replaces one per file: each reconcile takes `rebuild_lock`, so a
/// per-file call queued behind whatever rebuild the previous write triggered.
#[tauri::command]
pub async fn set_skills_invocation(
    trace: crate::timing_log::IpcTrace,
    targets: Vec<InvocationTarget>,
    policy: InvocationPolicy,
    app: tauri::AppHandle,
) -> Result<Vec<BulkTargetResult>, String> {
    let timing_app = app.clone();
    crate::timing_log::time_command_blocking(
        &timing_app,
        trace,
        "set_skills_invocation",
        move || {
            let refresh_state = app.state::<SkillRefreshState>();
            let start = std::time::Instant::now();
            let first_path = targets.first().map_or("", |target| target.path.as_str());
            let snapshot = read_snapshot(&refresh_state, first_path)?;
            let mut reconcile_ms = 0;
            let results = apply_invocation_targets(&snapshot, &targets, policy, |names| {
                let reconcile_start = std::time::Instant::now();
                let outcome =
                    skill_refresh::reconcile_skill_names_and_emit(&app, &refresh_state, names, &[]);
                reconcile_ms = reconcile_start.elapsed().as_millis();
                if outcome.is_err() {
                    refresh_state.mark_skills_dirty();
                }
                outcome
            });
            eprintln!(
                "skill refresh: batch invocation {} targets in {} ms (reconcile {reconcile_ms} ms)",
                targets.len(),
                start.elapsed().as_millis()
            );
            Ok(results)
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use skill_studio_core::invocation::rewrite_invocation_frontmatter;
    #[test]
    fn both_resets_existing_key_value() {
        let content =
            "---\nname: find-bugs\ndescription: test\ndisable-model-invocation: true\n---\nBody.";
        let updated = rewrite_invocation_frontmatter(content, InvocationPolicy::Both).unwrap();
        assert_eq!(
            updated,
            "---\nname: find-bugs\ndescription: test\ndisable-model-invocation: false\n---\nBody."
        );
    }
    #[test]
    fn user_only_inserts_key_after_description() {
        let content = "---\nname: find-bugs\ndescription: test\nlicense: MIT\n---\nBody.";
        let updated = rewrite_invocation_frontmatter(content, InvocationPolicy::UserOnly).unwrap();
        assert_eq!(
            updated,
            "---\nname: find-bugs\ndescription: test\ndisable-model-invocation: true\nlicense: MIT\n---\nBody."
        );
    }
    #[test]
    fn model_only_replaces_an_existing_conflicting_key() {
        let content =
            "---\nname: find-bugs\ndescription: test\ndisable-model-invocation: true\n---\nBody.\n";
        let updated = rewrite_invocation_frontmatter(content, InvocationPolicy::ModelOnly).unwrap();
        assert_eq!(
            updated,
            "---\nname: find-bugs\ndescription: test\nuser-invocable: false\ndisable-model-invocation: false\n---\nBody.\n"
        );
    }
    #[test]
    fn body_and_other_keys_are_byte_identical() {
        let content = "---\nname: find-bugs\ndescription: test\nmetadata:\n  foo: bar\n---\n# Heading\n\nSome body text.\n";
        let updated = rewrite_invocation_frontmatter(content, InvocationPolicy::UserOnly).unwrap();
        assert!(updated.contains("metadata:\n  foo: bar\n"));
        assert!(updated.ends_with("# Heading\n\nSome body text.\n"));
    }
    #[test]
    fn refuses_content_without_frontmatter() {
        let err = rewrite_invocation_frontmatter("no frontmatter here", InvocationPolicy::Both)
            .unwrap_err();
        assert!(err.contains("no frontmatter"));
    }
    #[test]
    fn inserts_after_a_literal_block_scalar_description() {
        let content = "---\nname: find-bugs\ndescription: |\n  Line one.\n  Line two.\nlicense: MIT\n---\nBody.\n";
        let updated = rewrite_invocation_frontmatter(content, InvocationPolicy::UserOnly).unwrap();
        assert_eq!(
            updated,
            "---\nname: find-bugs\ndescription: |\n  Line one.\n  Line two.\ndisable-model-invocation: true\nlicense: MIT\n---\nBody.\n"
        );
    }
    #[test]
    fn inserts_after_a_folded_block_scalar_description() {
        let content = "---\nname: find-bugs\ndescription: >\n  Folded text\n  continues here.\nlicense: MIT\n---\nBody.\n";
        let updated = rewrite_invocation_frontmatter(content, InvocationPolicy::UserOnly).unwrap();
        assert_eq!(
            updated,
            "---\nname: find-bugs\ndescription: >\n  Folded text\n  continues here.\ndisable-model-invocation: true\nlicense: MIT\n---\nBody.\n"
        );
    }
    #[test]
    fn nested_key_sharing_a_name_stays_untouched() {
        let content = "---\nname: find-bugs\ndescription: test\nmetadata:\n  user-invocable: false\n---\nBody.\n";
        let updated = rewrite_invocation_frontmatter(content, InvocationPolicy::UserOnly).unwrap();
        assert!(updated.contains("metadata:\n  user-invocable: false\n"));
        assert!(updated.contains("disable-model-invocation: true"));
    }
    #[test]
    fn crlf_document_keeps_crlf_outside_the_edited_span() {
        let content =
            "---\r\nname: find-bugs\r\ndescription: test\r\nlicense: MIT\r\n---\r\nBody.\r\n";
        let updated = rewrite_invocation_frontmatter(content, InvocationPolicy::UserOnly).unwrap();
        assert_eq!(
            updated,
            "---\r\nname: find-bugs\r\ndescription: test\r\ndisable-model-invocation: true\r\nlicense: MIT\r\n---\r\nBody.\r\n"
        );
    }
    #[test]
    fn missing_final_newline_is_preserved_alongside_an_insertion() {
        let content = "---\nname: find-bugs\ndescription: test\n---\nBody.";
        let updated = rewrite_invocation_frontmatter(content, InvocationPolicy::UserOnly).unwrap();
        assert_eq!(
            updated,
            "---\nname: find-bugs\ndescription: test\ndisable-model-invocation: true\n---\nBody."
        );
        assert!(!updated.ends_with('\n'));
    }
    #[test]
    fn both_resets_conflicting_key_values() {
        let content = "---\nname: find-bugs\ndescription: test\ndisable-model-invocation: true\nuser-invocable: false\n---\nBody.\n";
        let updated = rewrite_invocation_frontmatter(content, InvocationPolicy::Both).unwrap();
        assert!(updated.contains("disable-model-invocation: false"));
        assert!(updated.contains("user-invocable: true"));
    }
    #[test]
    fn applying_the_same_policy_twice_is_idempotent() {
        let content = "---\nname: find-bugs\ndescription: test\nlicense: MIT\n---\nBody.\n";
        let once = rewrite_invocation_frontmatter(content, InvocationPolicy::UserOnly).unwrap();
        let twice = rewrite_invocation_frontmatter(&once, InvocationPolicy::UserOnly).unwrap();
        assert_eq!(once, twice);
    }
}
