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

/// The verdict for the deployment an update would replace.
fn deployment_local_edits(
    fs: &dyn ScopeFs,
    lock: Option<&SkillLockFile>,
    skill_name: &str,
    deployment: &Deployment,
) -> LocalEditsDto {
    if deployment.owner_kind != LifecycleOwnerKind::SkillsSh || deployment.scope != "global" {
        return NOT_CHECKED;
    }
    let Some(lock) = lock else {
        return NOT_CHECKED;
    };
    match lock_file::local_edits(fs, lock, skill_name, Path::new(&deployment.path)) {
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
    let Some(owner_id) = deployment.owner_id.as_deref() else {
        return deployment_local_edits(fs, lock, skill_name, deployment);
    };
    let mut verdict = NOT_CHECKED;
    for copy in deployments
        .iter()
        .filter(|copy| copy.owner_id.as_deref() == Some(owner_id) && !copy.is_symlink)
    {
        let checked = deployment_local_edits(fs, lock, skill_name, copy);
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

    #[test]
    fn updating_an_owner_checks_its_copy_mode_folders_or_silently_overwrites_their_edits() {
        let home = tempfile::tempdir().unwrap();
        let canonical_path = home.path().join(".agents/skills/sample");
        let copy_path = home.path().join(".codex/skills/sample");
        for path in [&canonical_path, &copy_path] {
            fs::create_dir_all(path).unwrap();
            fs::write(path.join("SKILL.md"), b"original skill").unwrap();
        }
        let real_fs = skill_studio_host::RealFs::new();
        let hash = skill_studio_core::tree_hash::tree_hash_ignoring_junk(&real_fs, &canonical_path)
            .unwrap();
        let lock: SkillLockFile = serde_json::from_value(serde_json::json!({"version":3,"skills":{"sample":{"source":"owner/repo","sourceType":"github","sourceUrl":"https://github.com/owner/repo","skillFolderHash":hash,"installedAt":"2020-01-01T00:00:00Z","updatedAt":"2020-01-01T00:00:00Z"}}})).unwrap();
        let canonical = Deployment {
            path: canonical_path.to_string_lossy().into_owned(),
            scope: "global".to_string(),
            owner_kind: LifecycleOwnerKind::SkillsSh,
            owner_id: Some("owner:v1/global/sample".to_string()),
            ..Default::default()
        };
        let copy = Deployment {
            path: copy_path.to_string_lossy().into_owned(),
            ..canonical.clone()
        };
        fs::write(copy_path.join("SKILL.md"), b"my edit").unwrap();
        let verdict = update_local_edits(
            &real_fs,
            Some(&lock),
            "sample",
            &canonical,
            &[canonical.clone(), copy.clone()],
        );
        assert!(
            verdict.edited && verdict.checked,
            "the edited per-agent folder was not checked"
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
