// ============================================================================
// Skills Module - skill_trust_policy
// Explicit trust for a later dotagents Add Skill retry. Confirmation records
// a normalized repository identity through this seam; Add Skill never trusts
// arbitrary CLI stderr, and never records trust without the user clicking
// "Trust repository and retry".
// ============================================================================

use std::path::Path;

use super::skill_dto::{ParsedSkillSource, ParsedSkillSourceKind};
use super::skill_fork_registry::{read_fork_registry, write_fork_registry_locked};
use super::write_lease::WriteLeaseGuard;

/// Unique prefix so an untrusted-source error greps back to this module.
pub const UNTRUSTED_DOTAGENTS_SOURCE_PREFIX: &str = "Untrusted dotagents source";

/// User-safe copy shown when a source is not yet trusted.
pub const UNTRUSTED_DOTAGENTS_SOURCE_MESSAGE: &str =
    "This repository is not trusted. Confirm Trust repository and retry to install it.";

/// Normalize a GitHub `owner/repo` or `git:<url>` identity for trust lookup.
/// Local sources are never gated. Returns `None` when the source has no
/// repository identity to trust.
pub fn normalize_dotagents_source_identity(source: &ParsedSkillSource) -> Option<String> {
    match source.kind {
        ParsedSkillSourceKind::Github => source
            .repo
            .as_deref()
            .map(str::trim)
            .filter(|repo| !repo.is_empty())
            .map(|repo| repo.trim_end_matches(".git").to_ascii_lowercase()),
        ParsedSkillSourceKind::Git => source.url.as_deref().map(normalize_git_url_identity),
        ParsedSkillSourceKind::Local => None,
    }
}

fn normalize_git_url_identity(url: &str) -> String {
    url.trim()
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .to_ascii_lowercase()
}

/// Record explicit trust for `identity` without a held write lease - a test
/// fixture writer. Rejects empty or mismatched values so confirmation cannot
/// trust arbitrary error text.
#[cfg(test)]
pub fn record_trusted_dotagents_source(home: &Path, identity: &str) -> Result<String, String> {
    let normalized = normalize_confirmation_identity(identity)?;
    super::skill_fork_registry::update_fork_registry(home, |registry| {
        registry
            .trusted_dotagents_sources
            .insert(normalized.clone());
    })?;
    Ok(normalized)
}

/// Record explicit trust for `identity`, for a caller that already holds the
/// per-root write lease. Rejects empty or mismatched values so confirmation
/// cannot trust arbitrary error text.
pub fn record_trusted_dotagents_source_locked(
    guard: &WriteLeaseGuard,
    home: &Path,
    identity: &str,
) -> Result<String, String> {
    let normalized = normalize_confirmation_identity(identity)?;
    let mut registry = read_fork_registry(home)?;
    registry
        .trusted_dotagents_sources
        .insert(normalized.clone());
    write_fork_registry_locked(guard, home, &mut registry)?;
    Ok(normalized)
}

/// Reject empty, whitespace, or multi-line confirmation identities.
pub fn normalize_confirmation_identity(identity: &str) -> Result<String, String> {
    let trimmed = identity.trim();
    if trimmed.is_empty() || trimmed.contains('\n') || trimmed.contains('\r') {
        return Err("Trust confirmation needs a repository identity".to_string());
    }
    if trimmed.contains("://") {
        return Ok(normalize_git_url_identity(trimmed));
    }
    Ok(trimmed.trim_end_matches(".git").to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn github(repo: &str) -> ParsedSkillSource {
        ParsedSkillSource {
            kind: ParsedSkillSourceKind::Github,
            repo: Some(repo.to_string()),
            path: None,
            git_ref: None,
            skill_name: None,
            url: None,
            local_path: None,
        }
    }

    #[test]
    fn github_identity_is_lowercase_owner_repo() {
        assert_eq!(
            normalize_dotagents_source_identity(&github("KentCDodds/KCD-Skills.git")).as_deref(),
            Some("kentcdodds/kcd-skills")
        );
    }

    #[test]
    fn confirmation_rejects_empty_and_multiline_text() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(record_trusted_dotagents_source(tmp.path(), "  ").is_err());
        assert!(record_trusted_dotagents_source(tmp.path(), "a\nb").is_err());
    }

    #[test]
    fn local_sources_are_not_gated() {
        let source = ParsedSkillSource {
            kind: ParsedSkillSourceKind::Local,
            repo: None,
            path: None,
            git_ref: None,
            skill_name: Some("find-bugs".to_string()),
            url: None,
            local_path: Some("/tmp/skill".to_string()),
        };
        assert_eq!(normalize_dotagents_source_identity(&source), None);
    }
}
