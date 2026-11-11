// ============================================================================
// Skills Module - SKILL.md Frontmatter
// Parsing and validation of SKILL.md frontmatter against the agentskills.io
// spec.
// ============================================================================

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

/// SKILL.md frontmatter, per the agentskills.io spec. Unknown keys are
/// ignored by default serde behavior, so agent-specific extensions in a
/// skill's frontmatter don't break parsing.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillFrontmatter {
    /// Required by spec: 1-64 chars, lowercase a-z0-9 and hyphens, must
    /// match the parent directory name.
    pub name: Option<String>,
    /// Required by spec: 1-1024 chars.
    pub description: Option<String>,
    pub license: Option<String>,
    /// Optional, spec caps this at 500 chars.
    pub compatibility: Option<String>,
    /// Tolerate non-string metadata values (numbers, lists, nested maps).
    pub metadata: Option<HashMap<String, serde_yaml::Value>>,
    #[serde(rename = "allowed-tools")]
    pub allowed_tools: Option<String>,
    /// Claude Code / pi extension: restricts this skill to explicit
    /// invocation only (`/name`, `/skill:name`), hiding it from the model's
    /// own auto-invoke matching. See `InvocationPolicy`.
    #[serde(rename = "disable-model-invocation")]
    pub disable_model_invocation: Option<bool>,
    /// Claude Code / pi extension: `false` restricts this skill to model
    /// auto-invoke only, hiding it from explicit `/name` invocation. See
    /// `InvocationPolicy`.
    #[serde(rename = "user-invocable")]
    pub user_invocable: Option<bool>,
}

/// Outcome of parsing the complete fenced YAML block at the start of SKILL.md.
#[derive(Debug, Clone)]
pub enum FrontmatterParseResult {
    Absent,
    Valid(SkillFrontmatter),
    Invalid(FrontmatterParseError),
}

/// A path-free YAML frontmatter error located in the full SKILL.md file.
#[derive(Debug, Clone)]
pub struct FrontmatterParseError {
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl FrontmatterParseResult {
    pub fn as_frontmatter(&self) -> Option<&SkillFrontmatter> {
        match self {
            Self::Valid(frontmatter) => Some(frontmatter),
            Self::Absent | Self::Invalid(_) => None,
        }
    }
}

/// The three invocation-control states a skill's frontmatter can express -
/// see `docs/agent-skill-conventions.md`'s "Invocation control" section.
pub use skill_studio_core::frontmatter::InvocationPolicy;

/// Reads `disable-model-invocation` and `user-invocable` off `frontmatter`
/// into an `InvocationPolicy`, plus a spec-violation note when both keys are
/// set (they contradict each other) - `user-invocable: false` is dropped in
/// that case and `UserOnly` wins, since a skill unusable by either a human or
/// the model would be pointless.
pub fn invocation_policy(frontmatter: Option<&SkillFrontmatter>) -> (InvocationPolicy, bool) {
    invocation_policy_from(
        frontmatter.and_then(|f| f.disable_model_invocation),
        frontmatter.and_then(|f| f.user_invocable),
    )
}

/// `invocation_policy`, taking the two raw key values directly rather than a
/// `SkillFrontmatter` - used by `skill_refresh::build_snapshot`, which only
/// has `InstalledSkill.frontmatter_fields`'s stringified form to work from.
pub use skill_studio_core::frontmatter::invocation_policy_from;

/// Parse a complete SKILL.md frontmatter fence without losing YAML errors.
pub fn parse_frontmatter(content: &str) -> FrontmatterParseResult {
    let mut lines = content.lines();
    if lines.next().map(str::trim) != Some("---") {
        return FrontmatterParseResult::Absent;
    }
    let mut yaml_block = String::new();
    for line in lines {
        if line.trim() == "---" {
            return match serde_yaml::from_str(&yaml_block) {
                Ok(frontmatter) => FrontmatterParseResult::Valid(frontmatter),
                Err(error) => {
                    let location = error.location();
                    let line = location.as_ref().map_or(1, |location| location.line() + 1);
                    let column = location.as_ref().map_or(1, serde_yaml::Location::column);
                    let rendered = error.to_string();
                    let message = rendered
                        .rsplit_once(" at line ")
                        .map_or(rendered.as_str(), |(message, _)| message)
                        .to_string();
                    FrontmatterParseResult::Invalid(FrontmatterParseError {
                        line,
                        column,
                        message,
                    })
                }
            };
        }
        yaml_block.push_str(line);
        yaml_block.push('\n');
    }
    FrontmatterParseResult::Invalid(FrontmatterParseError {
        line: content.lines().count().saturating_add(1),
        column: 1,
        message: "unterminated YAML frontmatter".to_string(),
    })
}

/// Every top-level frontmatter key, stringified, for the dashboard to show
/// agent-specific extensions the typed `SkillFrontmatter` doesn't model.
/// Malformed or missing frontmatter yields an empty map.
pub fn frontmatter_fields(content: &str) -> BTreeMap<String, String> {
    let mut lines = content.lines();
    if lines.next().map(str::trim) != Some("---") {
        return BTreeMap::new();
    }
    let mut yaml_block = String::new();
    let mut has_closing_fence = false;
    for line in lines {
        if line.trim() == "---" {
            has_closing_fence = true;
            break;
        }
        yaml_block.push_str(line);
        yaml_block.push('\n');
    }
    if !has_closing_fence {
        return BTreeMap::new();
    }
    let Ok(serde_yaml::Value::Mapping(map)) = serde_yaml::from_str(&yaml_block) else {
        return BTreeMap::new();
    };
    map.into_iter()
        .filter_map(|(k, v)| {
            let key = k.as_str()?.to_string();
            let value = match v {
                serde_yaml::Value::String(s) => s,
                other => serde_yaml::to_string(&other)
                    .unwrap_or_default()
                    .trim()
                    .to_string(),
            };
            Some((key, value))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_frontmatter() -> SkillFrontmatter {
        SkillFrontmatter {
            name: Some("write-tests".to_string()),
            description: Some("Writes tests for the current change.".to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn frontmatter_fields_captures_every_top_level_key() {
        let content = "---\nname: write-tests\ndescription: Writes tests.\ncustom-field: some-value\n---\nBody.";
        let fields = frontmatter_fields(content);
        assert_eq!(fields.get("name").map(String::as_str), Some("write-tests"));
        assert_eq!(
            fields.get("custom-field").map(String::as_str),
            Some("some-value")
        );
    }

    #[test]
    fn frontmatter_fields_rejects_body_fields_after_an_unclosed_fence() {
        let content = "---\nname: write-tests\ndescription: Writes tests.\ndisable-model-invocation: true\nRole: test author\nGoal: write tests for new features\n";

        assert!(frontmatter_fields(content).is_empty());
    }

    #[test]
    fn frontmatter_fields_parses_valid_crlf_frontmatter() {
        let content = "---\r\nname: write-tests\r\ndescription: Writes tests.\r\n---\r\nBody.";
        let fields = frontmatter_fields(content);

        assert_eq!(fields.get("name").map(String::as_str), Some("write-tests"));
        assert_eq!(
            fields.get("description").map(String::as_str),
            Some("Writes tests.")
        );
    }

    #[test]
    fn frontmatter_fields_empty_without_frontmatter() {
        assert!(frontmatter_fields("no frontmatter here").is_empty());
    }

    #[test]
    fn invocation_policy_defaults_to_both() {
        let (policy, conflict) = invocation_policy(Some(&valid_frontmatter()));
        assert_eq!(policy, InvocationPolicy::Both);
        assert!(!conflict);
    }

    #[test]
    fn invocation_policy_disable_model_invocation_is_user_only() {
        let mut fm = valid_frontmatter();
        fm.disable_model_invocation = Some(true);
        let (policy, conflict) = invocation_policy(Some(&fm));
        assert_eq!(policy, InvocationPolicy::UserOnly);
        assert!(!conflict);
    }

    #[test]
    fn invocation_policy_user_invocable_false_is_model_only() {
        let mut fm = valid_frontmatter();
        fm.user_invocable = Some(false);
        let (policy, conflict) = invocation_policy(Some(&fm));
        assert_eq!(policy, InvocationPolicy::ModelOnly);
        assert!(!conflict);
    }

    #[test]
    fn invocation_policy_conflicting_keys_favors_user_only_and_flags_conflict() {
        let mut fm = valid_frontmatter();
        fm.disable_model_invocation = Some(true);
        fm.user_invocable = Some(false);
        let (policy, conflict) = invocation_policy(Some(&fm));
        assert_eq!(policy, InvocationPolicy::UserOnly);
        assert!(conflict);
    }
}
