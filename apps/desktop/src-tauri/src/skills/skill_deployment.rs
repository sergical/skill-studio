// ============================================================================
// Skills Module - skill_deployment
// Stable identity for one on-disk skill deployment. Discovery still walks
// the filesystem; this module only names what it found: scope, destination
// (Universal vs Per harness), lexical path, owner, and backing relationship.
// Compatibility: scanner roots still use the label "shared"; that identifier
// is not a user-facing destination. Universal is the domain term.
// ============================================================================

use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::skill_dto::InstallScope;

/// Where a skill is installed relative to harness folders. Universal owns
/// `.agents/skills`; Per harness owns an independent copy in one harness dir
/// and never writes `.agents/skills`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SkillDestination {
    Universal,
    /// The core's `split` and `install` write the `per_harness` spelling.
    #[default]
    #[serde(alias = "per_harness")]
    PerHarness,
}

impl SkillDestination {
    /// Wire string used in deployment / owner ids.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Universal => "universal",
            Self::PerHarness => "per-harness",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "universal" => Some(Self::Universal),
            "per-harness" | "per_harness" => Some(Self::PerHarness),
            _ => None,
        }
    }
}

/// How this deployment relates to a Universal folder of the same skill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum BackingRelationship {
    /// This directory is the Universal `.agents/skills/<name>` folder.
    Canonical,
    /// A per-skill or whole-dir link into a Universal folder.
    LinkedTo { deployment_id: String },
    /// An independent harness copy that does not back or follow Universal.
    #[default]
    Independent,
}

/// Whether Skill Studio may mutate this deployment through an owner adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum DeploymentMutability {
    Mutable,
    #[default]
    ReadOnly,
}

/// Compatibility root label the scanner still emits for `.agents/skills`.
pub const UNIVERSAL_ROOT_LABEL: &str = "shared";

/// The Universal skills directory for `scope` under `home` or `project`.
pub fn universal_skills_dir(
    home: &Path,
    scope: InstallScope,
    project_path: Option<&Path>,
) -> PathBuf {
    match scope {
        InstallScope::Global => home.join(".agents").join("skills"),
        InstallScope::Project => project_path
            .unwrap_or_else(|| Path::new(""))
            .join(".agents")
            .join("skills"),
    }
}

/// The `.agents` directory that owns ledgers for a Universal skills root.
pub fn agents_root_for_skills_dir(skills_dir: &Path) -> Option<PathBuf> {
    let parent = skills_dir.parent()?;
    if parent.file_name().and_then(|n| n.to_str()) == Some(".agents") {
        Some(parent.to_path_buf())
    } else {
        None
    }
}

/// Percent-encode `/` and `%` so a project path can sit in a single id slot.
pub fn encode_id_path(path: &str) -> String {
    path.replace('%', "%25").replace('/', "%2F")
}

fn decode_id_path(encoded: &str) -> String {
    encoded.replace("%2F", "/").replace("%25", "%")
}

/// Stable id for one deployment. Format:
/// `dep:v1/{scope}/{slot}/{destination}/{name}/{project}/{lexical-entry}`
/// where `slot` is `universal` or a harness cli name, and `project` is `-`
/// for global / plugin / parked.
pub fn deployment_id(
    name: &str,
    scope: &str,
    destination: SkillDestination,
    slot: &str,
    project_path: Option<&str>,
    lexical_entry: &Path,
) -> String {
    let project = match project_path {
        Some(path) if !path.is_empty() => encode_id_path(path),
        _ => "-".to_string(),
    };
    format!(
        "dep:v1/{scope}/{slot}/{}/{name}/{project}/{}",
        destination.as_str(),
        encode_id_path(&lexical_entry.to_string_lossy())
    )
}

/// Parse a `dep:v1/...` id back into its slots. `None` when the string is
/// not a v1 deployment id.
pub fn parse_deployment_id(id: &str) -> Option<ParsedDeploymentId> {
    let rest = id.strip_prefix("dep:v1/")?;
    let mut parts = rest.splitn(6, '/');
    let scope = parts.next()?.to_string();
    let slot = parts.next()?.to_string();
    let destination = SkillDestination::parse(parts.next()?)?;
    let name = parts.next()?.to_string();
    let project = parts.next()?;
    let lexical_path = PathBuf::from(decode_id_path(parts.next()?));
    let project_path = if project == "-" {
        None
    } else {
        Some(decode_id_path(project))
    };
    Some(ParsedDeploymentId {
        scope,
        slot,
        destination,
        name,
        project_path,
        lexical_path,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDeploymentId {
    pub scope: String,
    pub slot: String,
    pub destination: SkillDestination,
    pub name: String,
    pub project_path: Option<String>,
    pub lexical_path: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn universal_id_round_trips_global() {
        let id = deployment_id(
            "find-bugs",
            "global",
            SkillDestination::Universal,
            "universal",
            None,
            Path::new("/home/.agents/skills/find-bugs"),
        );
        assert_eq!(
            id,
            "dep:v1/global/universal/universal/find-bugs/-/%2Fhome%2F.agents%2Fskills%2Ffind-bugs"
        );
        let parsed = parse_deployment_id(&id).unwrap();
        assert_eq!(parsed.name, "find-bugs");
        assert_eq!(parsed.destination, SkillDestination::Universal);
        assert_eq!(parsed.slot, "universal");
        assert!(parsed.project_path.is_none());
    }
}
