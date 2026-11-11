/// A [`skill_studio_core::skill_update_check::SourceTreeLookup`],
/// [`skill_studio_core::skill_update_check::CommitLookup`], and
/// [`skill_studio_core::skill_update_check::PluginManifestLookup`] all in
/// one: when `gh` is not on `PATH`, every lookup a currency check makes
/// fails, which `ops::outdated` already turns into `Currency::Unknown` per
/// skill rather than a hard error - matching the desktop's own fallback for
/// an unresolved `gh` binary.
pub struct NoGhLookup;

impl skill_studio_core::skill_update_check::SourceTreeLookup for NoGhLookup {
    fn tree_shas_at_head(
        &self,
        _repo: &str,
    ) -> Result<std::collections::HashMap<String, String>, skill_studio_core::CoreError> {
        Err(skill_studio_core::CoreError::new(
            skill_studio_core::ErrorCode::Unsupported,
            "gh is not on PATH",
        ))
    }
}

impl skill_studio_core::skill_update_check::CommitLookup for NoGhLookup {
    fn latest_commit(
        &self,
        _repo: &str,
        _path: &str,
    ) -> Result<
        Option<skill_studio_core::skill_update_check::CommitInfo>,
        skill_studio_core::CoreError,
    > {
        Err(skill_studio_core::CoreError::new(
            skill_studio_core::ErrorCode::Unsupported,
            "gh is not on PATH",
        ))
    }
}

impl skill_studio_core::skill_update_check::PluginManifestLookup for NoGhLookup {
    fn marketplace_version(
        &self,
        _marketplace: &str,
        _plugin: &str,
    ) -> Result<Option<String>, skill_studio_core::CoreError> {
        Ok(None)
    }
}
