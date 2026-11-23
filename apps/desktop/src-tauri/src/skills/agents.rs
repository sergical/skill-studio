//! Desktop skill roots resolved from the core harness catalog.

use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use skill_studio_core::harness::CatalogAgent as AgentId;
use skill_studio_core::harness::BUILTIN_HARNESSES;

/// Agent target with paths resolved
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentTarget {
    pub id: AgentId,
    pub name: String,
    pub project_path: String,
    pub global_path: String,
}

// ============================================================================
// Skill Roots - where skills live on disk
// ============================================================================

/// One directory an agent loads skills from. Label is the display name
/// ("Claude Code", "`OpenCode`", "shared" for `~/.agents/skills`). Global vs.
/// project scope is implied by `project_path`.
#[derive(Debug, Clone)]
pub struct SkillRoot {
    pub label: String,
    pub project_path: Option<PathBuf>,
    pub path: PathBuf,
}

/// Every global and project skill root for the first-class agents plus the
/// shared root: each first-class agent's own directory (from `AgentId`'s
/// path methods, the single source of truth), `OpenCode`'s older singular
/// `skill/` directory (kept as a fallback alongside `skills/`), and the
/// shared `.agents/skills` root that Codex, `OpenCode`, pi, Cursor and Grok
/// Build all read.
pub fn skill_roots(home: &Path, project_paths: &[PathBuf]) -> Vec<SkillRoot> {
    let mut roots = Vec::new();

    for row in BUILTIN_HARNESSES {
        let id = row.agent;
        roots.push(SkillRoot {
            label: id.display_name().to_string(),
            project_path: None,
            path: skill_studio_host::agent_path(home, id.global_path()),
        });
    }
    roots.push(SkillRoot {
        label: AgentId::OpenCode.display_name().to_string(),
        project_path: None,
        path: home.join(".config/opencode/skill"),
    });
    roots.push(SkillRoot {
        label: "shared".to_string(),
        project_path: None,
        path: home.join(".agents/skills"),
    });
    // Parked (globally disabled) skills - see skill_park.rs. Global only:
    // parking only ever touches the shared `.agents/skills` root.
    roots.push(SkillRoot {
        label: "parked".to_string(),
        project_path: None,
        path: home.join(".agents/skills-parked"),
    });

    for project in project_paths {
        for row in BUILTIN_HARNESSES {
            let id = row.agent;
            roots.push(SkillRoot {
                label: id.display_name().to_string(),
                project_path: Some(project.clone()),
                path: project.join(id.project_path()),
            });
        }
        roots.push(SkillRoot {
            label: AgentId::OpenCode.display_name().to_string(),
            project_path: Some(project.clone()),
            path: project.join(".opencode/skill"),
        });
        roots.push(SkillRoot {
            label: "shared".to_string(),
            project_path: Some(project.clone()),
            path: project.join(".agents/skills"),
        });
    }

    roots
}
