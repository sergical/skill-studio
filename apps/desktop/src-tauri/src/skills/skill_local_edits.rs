// ============================================================================
// Skills Module - skill_local_edits
// On-demand check behind the "Update will replace your edits" warning: does
// the installed skills.sh folder an update would replace still match the
// `skillFolderHash` `npx skills` recorded? Run per Update click, never during
// the scan, because it hashes every file in each folder.
//
// Only a Global skills.sh deployment is comparable. A project install's
// `skills-lock.json` records a different `computedHash`, and the dotagents
// `agents.lock` records a commit (`resolved_commit`), not a folder hash, so
// those report `checked: false` and the UI updates without a warning.
// ============================================================================

use std::path::Path;

use skill_studio_core::lock_file::{self, LocalEdits, SkillLockFile};
use skill_studio_core::ports::ScopeFs;
use tauri::Manager;

use super::skill_dto::{Deployment, LifecycleTarget, LocalEditsDto};
use super::skill_lifecycle;
use super::skill_ownership::LifecycleOwnerKind;
use super::skill_refresh::SkillRefreshState;

const NOT_CHECKED: LocalEditsDto = LocalEditsDto {
    edited: false,
    checked: false,
};

fn folder_local_edits(
    fs: &dyn ScopeFs,
    lock: Option<&SkillLockFile>,
    skill_name: &str,
    path: &Path,
) -> LocalEditsDto {
    let Some(lock) = lock else {
        return NOT_CHECKED;
    };
    match lock_file::local_edits(fs, lock, skill_name, path) {
        LocalEdits::Unedited => LocalEditsDto {
            edited: false,
            checked: true,
        },
        LocalEdits::Edited => LocalEditsDto {
            edited: true,
            checked: true,
        },
        LocalEdits::Unknown => {
            eprintln!("[local-edits] {skill_name}: no comparable lock hash");
            NOT_CHECKED
        }
    }
}

fn update_local_edits(
    fs: &dyn ScopeFs,
    lock: Option<&SkillLockFile>,
    skill_name: &str,
    deployment: &Deployment,
    deployments: &[Deployment],
) -> LocalEditsDto {
    if deployment.owner_kind != LifecycleOwnerKind::SkillsSh || deployment.scope != "global" {
        return NOT_CHECKED;
    }
    let mut verdict = folder_local_edits(fs, lock, skill_name, Path::new(&deployment.path));
    for copy in deployments {
        if copy.path == deployment.path
            || copy.scope != "global"
            || copy.is_symlink
            || copy.plugin.is_some()
        {
            continue;
        }
        let same_owner = deployment
            .owner_id
            .as_deref()
            .is_some_and(|owner| copy.owner_id.as_deref() == Some(owner));
        // A copy-mode edit loses its inferred owner (and reads as in-repo under a Git folder), but the skills CLI can still replace that folder.
        let unclaimed_agent_copy = copy.owner_id.is_none()
            && matches!(
                copy.owner_kind,
                LifecycleOwnerKind::Manual | LifecycleOwnerKind::InRepo
            )
            && copy.destination == super::skill_deployment::SkillDestination::PerHarness;
        if !same_owner && !unclaimed_agent_copy {
            continue;
        }
        let checked = folder_local_edits(fs, lock, skill_name, Path::new(&copy.path));
        verdict.edited |= checked.edited;
        verdict.checked |= checked.checked;
    }
    verdict
}

/// One verdict per target, in the order sent. A target that no longer
/// resolves reads as not checked: Update itself reports that refusal.
#[tauri::command]
pub async fn skill_local_edits(
    trace: crate::timing_log::IpcTrace,
    targets: Vec<LifecycleTarget>,
    app: tauri::AppHandle,
) -> Result<Vec<LocalEditsDto>, String> {
    let timing_app = app.clone();
    crate::timing_log::time_command_blocking(&timing_app, trace, "skill_local_edits", move || {
        let refresh_state = app.state::<SkillRefreshState>();
        let snapshot = skill_lifecycle::rebuild_fresh_lifecycle_snapshot(&app, &refresh_state)?;
        let home = dirs::home_dir().ok_or("Could not find home directory")?;
        let fs = skill_studio_host::RealFs::new();
        let lock = match lock_file::read_lock_file(&fs, &lock_file::lock_file_path(&home)) {
            Ok(lock) => Some(lock),
            Err(e) => {
                eprintln!("[local-edits] lock file unreadable: {e:?}");
                None
            }
        };
        Ok(targets
            .iter()
            .map(|target| {
                match skill_lifecycle::resolve_lifecycle_target(&snapshot, target, "Update") {
                    Ok((skill, deployment)) => update_local_edits(
                        &fs,
                        lock.as_ref(),
                        &skill.name,
                        &deployment,
                        &skill.deployments,
                    ),
                    Err(e) => {
                        eprintln!("[local-edits] target not resolved: {e}");
                        NOT_CHECKED
                    }
                }
            })
            .collect())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Flow: Update a skills.sh skill whose copy-mode Codex folder was edited,
    /// with and without a Git folder at `~/.codex`. Expectation: the verdict
    /// says edited either way. Failure: the in-repo copy is skipped and Update
    /// overwrites the edit with no warning.
    #[test]
    fn updating_an_owner_checks_its_copy_mode_folders_or_silently_overwrites_their_edits() {
        for codex_in_git in [false, true] {
            check_copy_mode_folder(codex_in_git);
        }
    }

    fn check_copy_mode_folder(codex_in_git: bool) {
        let home = tempfile::tempdir().unwrap();
        let canonical_path = home.path().join(".agents/skills/sample");
        let copy_path = home.path().join(".codex/skills/sample");
        for path in [&canonical_path, &copy_path] {
            fs::create_dir_all(path).unwrap();
            fs::write(path.join("SKILL.md"), b"original skill").unwrap();
        }
        if codex_in_git {
            fs::create_dir_all(home.path().join(".codex/.git")).unwrap();
        }
        let real_fs = skill_studio_host::RealFs::new();
        let hash = skill_studio_core::tree_hash::tree_hash_ignoring_junk(&real_fs, &canonical_path)
            .unwrap();
        let lock: SkillLockFile = serde_json::from_value(serde_json::json!({"version":3,"skills":{"sample":{"source":"owner/repo","sourceType":"github","sourceUrl":"https://github.com/owner/repo","skillFolderHash":hash,"installedAt":"2020-01-01T00:00:00Z","updatedAt":"2020-01-01T00:00:00Z"}}})).unwrap();
        fs::write(
            home.path().join(".agents/.skill-lock.json"),
            serde_json::to_vec(&lock).unwrap(),
        )
        .unwrap();
        fs::write(copy_path.join("SKILL.md"), b"my edit").unwrap();
        let ports = skill_studio_host::default_ports(
            home.path().join(".leases"),
            std::sync::Arc::new(skill_studio_core::harness::HarnessCatalog::builtin()),
        );
        let runtime = skill_studio_core::ports::Runtime::new(
            &skill_studio_core::scope::RuntimeScope::fixture(home.path()),
            ports,
        )
        .unwrap();
        let inventory = skill_studio_core::ops::scan(
            &runtime,
            &skill_studio_core::testing::golden::ctx(),
            &Default::default(),
        )
        .unwrap();
        let assembled =
            super::super::skill_assembly::assemble_installed_skills(&inventory.skills, &lock);
        let installed = assembled
            .iter()
            .find(|skill| skill.name == "sample")
            .unwrap();
        let canonical = installed
            .deployments
            .iter()
            .find(|copy| {
                copy.destination == super::super::skill_deployment::SkillDestination::Universal
            })
            .unwrap()
            .clone();
        let copy = installed
            .deployments
            .iter()
            .find(|copy| {
                copy.destination == super::super::skill_deployment::SkillDestination::PerHarness
            })
            .unwrap()
            .clone();
        assert_eq!(
            copy.owner_kind == LifecycleOwnerKind::InRepo,
            codex_in_git,
            "fixture: {:?}",
            copy.owner_kind
        );
        let verdict = update_local_edits(
            &real_fs,
            Some(&lock),
            "sample",
            &canonical,
            &[canonical.clone(), copy.clone()],
        );
        assert!(
            verdict.edited && verdict.checked,
            "the edited per-agent folder was not checked: {:?}, {:?}",
            copy.owner_kind,
            copy.owner_id
        );
        fs::write(copy_path.join("SKILL.md"), b"original skill").unwrap();
        let foreign = Deployment {
            owner_id: Some("another-owner".to_string()),
            path: home.path().join("foreign").to_string_lossy().into_owned(),
            ..copy.clone()
        };
        fs::create_dir_all(&foreign.path).unwrap();
        fs::write(Path::new(&foreign.path).join("SKILL.md"), b"foreign edit").unwrap();
        let linked_path = home.path().join("linked");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&foreign.path, &linked_path).unwrap();
        let linked = Deployment {
            path: linked_path.to_string_lossy().into_owned(),
            is_symlink: true,
            ..canonical.clone()
        };
        assert!(
            !update_local_edits(
                &real_fs,
                Some(&lock),
                "sample",
                &canonical,
                &[canonical.clone(), copy, foreign, linked]
            )
            .edited
        );
    }
}
