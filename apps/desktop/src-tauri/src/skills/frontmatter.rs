// ============================================================================
// Skills Module - SKILL.md Frontmatter
// Re-exports the core crate's SKILL.md frontmatter reader, which owns the
// parsing and its tests.
// ============================================================================

/// The three invocation-control states a skill's frontmatter can express -
/// see `docs/agent-skill-conventions.md`'s "Invocation control" section.
pub use skill_studio_core::frontmatter::InvocationPolicy;

/// Reads the two raw invocation keys into an `InvocationPolicy` - used by
/// `skill_refresh::build_snapshot`, which only has
/// `InstalledSkill.frontmatter_fields`'s stringified form to work from.
pub use skill_studio_core::frontmatter::invocation_policy_from;

/// Every top-level frontmatter key, stringified.
pub use skill_studio_core::frontmatter::frontmatter_fields;
