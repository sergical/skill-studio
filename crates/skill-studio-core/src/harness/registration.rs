//! Registration of built-in harnesses and legacy desktop installation targets.
use super::{HarnessAdapter, HarnessFacts};
use crate::identity::AgentId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
/// Destination for a Universal install. A per-harness copy always uses its own folder.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum UniversalInstall {
    Shared,
    Own,
    OwnExceptSkillsCli,
}

/// Implemented listing preview for a discovered skill.
#[derive(Clone, Copy)]
pub(crate) enum ListingRenderer {
    Discovery,
    Claude,
    Codex,
}

/// Implemented reader for a listing captured by a saved session.
#[derive(Clone, Copy)]
pub(crate) enum SessionListingReader {
    UsesOnly,
    Claude,
    Codex,
}

/// Vendor evidence for a specific release or source revision. This describes the
/// client, independently of the preview and recording readers shipped by Skill Studio.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisibilityReference {
    /// Release or source revision on which these observations were verified.
    pub revision: &'static str,
    /// How the client exposes skills to users or the model.
    pub exposure: &'static str,
    /// Saved listing format, when verified for this revision.
    pub saved_listing: Option<&'static str>,
    /// Sources for the exposure and recording observations.
    pub sources: &'static [&'static str],
}

/// Connects a desktop identity to its core facts and executable operations.
pub struct HarnessRegistration {
    /// Desktop wire identity.
    pub agent: CatalogAgent,
    /// Version-scoped evidence from the client implementation.
    pub visibility: &'static [VisibilityReference],
    pub(super) facts: fn() -> HarnessFacts,
    pub(super) adapter: &'static dyn HarnessAdapter,
    pub(crate) universal_install: Option<UniversalInstall>,
    pub(crate) project_marker: Option<&'static str>,
    pub(crate) renderer: ListingRenderer,
    pub(crate) session_reader: SessionListingReader,
}

macro_rules! register_agents {
    (harnesses { $( $(#[$alias:meta])* $agent:ident, $constant:ident, $id:literal, $label:literal, $project:literal, $global:literal =>
        ($facts:ident, $adapter:ident, $renderer:ident, $reader:ident $(, install($target:ident, $marker:expr))?)
        $(verified($revision:literal, $exposure:literal, $saved:expr, [$($source:literal),+]))*; )* }
     legacy { $( $legacy:ident, $legacy_id:literal, $legacy_label:literal, $legacy_dir:literal; )* }) => {
        /// Desktop wire enum, including historical install destinations.
        #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, JsonSchema)]
        #[schemars(rename = "AgentId")]
        pub enum CatalogAgent {
            $( $(#[$alias])* #[doc = concat!("`", $id, "`")] #[serde(rename = $id)] $agent, )*
            $( #[doc = concat!("`", $legacy_id, "`")] #[serde(rename = $legacy_id)] $legacy, )*
        }

        impl CatalogAgent {
            /// Stable wire name.
            pub const fn cli_name(self) -> &'static str {
                match self {
                    $(Self::$agent => $id,)*
                    $(Self::$legacy => $legacy_id,)*
                }
            }
            /// Display label used by scanned deployments.
            pub const fn display_name(self) -> &'static str {
                match self {
                    $(Self::$agent => $label,)*
                    $(Self::$legacy => $legacy_label,)*
                }
            }
            /// Skills folder relative to the project.
            pub const fn project_path(self) -> &'static str {
                match self {
                    $(Self::$agent => $project,)*
                    $(Self::$legacy => $legacy_dir,)*
                }
            }
            /// Default skills folder relative to the user's home.
            pub const fn global_path(self) -> &'static str {
                match self {
                    $(Self::$agent => $global,)*
                    $(Self::$legacy => $legacy_dir,)*
                }
            }
            /// All targets that can occur in a desktop deployment.
            pub fn all() -> Vec<Self> { vec![$(Self::$agent,)* $(Self::$legacy,)*] }
        }
        impl AgentId {
            $( #[doc = concat!("`", $id, "`")] pub const $constant: &'static str = $id; )*
        }

        /// First-class harnesses in detection and picker order.
        pub static BUILTIN_HARNESSES: &[HarnessRegistration] = &[$(HarnessRegistration {
            visibility: &[$(VisibilityReference { revision: $revision, exposure: $exposure, saved_listing: $saved, sources: &[$($source),+] },)*],
            agent: CatalogAgent::$agent, facts: super::$facts, adapter: &super::$adapter,
            universal_install: register_agents!(@install $($target)?), project_marker: register_agents!(@marker $($marker)?),
            renderer: ListingRenderer::$renderer, session_reader: SessionListingReader::$reader,
        },)*];

        /// Accepted install ids, including the shared-folder pseudo target.
        pub const INSTALL_TARGET_IDS: &[&str] = &["universal", $( $(register_agents!(@id $target $id),)? )*];
    };
    (@install $kind:ident) => { Some(UniversalInstall::$kind) };
    (@install) => { None };
    (@marker $marker:expr) => { $marker };
    (@marker) => { None };
    (@id $kind:ident $id:literal) => { $id };
}

impl HarnessRegistration {
    /// Evidence-backed discovery and operation facts for this harness.
    pub fn facts(&self) -> HarnessFacts {
        (self.facts)()
    }

    /// Whether this harness is a supported per-harness copy destination.
    pub const fn supports_install(&self) -> bool {
        self.universal_install.is_some()
    }
    /// Whether a native Universal install also targets the harness's own folder.
    pub const fn links_into_own_folder(&self) -> bool {
        matches!(
            self.universal_install,
            Some(UniversalInstall::Own | UniversalInstall::OwnExceptSkillsCli)
        )
    }
}

register_agents! {
    harnesses {
        ClaudeCode, CLAUDE_CODE, "claude-code", "Claude Code", ".claude/skills", ".claude/skills" =>
            (claude_code, ClaudeCodeAdapter, Claude, Claude, install(Own, None))
        verified("2.1.327", "Budgeted skill descriptions", Some("skill_listing attachment"),
            ["https://code.claude.com/docs/en/skills",
             "docs/agent-skill-conventions.md"]);
        Codex, CODEX, "codex", "Codex", ".codex/skills", ".codex/skills" =>
            (codex, CodexAdapter, Codex, Codex, install(Shared, None))
        verified("0.170.0", "Budgeted skill descriptions and paths", Some("skills_instructions block"),
            ["https://github.com/openai/codex/tree/rust-v0.170.0/codex-rs",
             "docs/agent-skill-conventions.md"]);
        #[serde(alias = "open-code")]
        OpenCode, OPEN_CODE, "opencode", "OpenCode", ".opencode/skills", ".config/opencode/skills" =>
            (open_code, OpenCodeAdapter, Discovery, UsesOnly, install(Shared, None))
        verified("2.3.2", "Registered skills and skill guidance", Some("Per-session skill guidance and instruction changes"),
            ["https://github.com/anomalyco/opencode/blob/v2.3.2/packages/core/src/session/instruction-state.ts"]);
        Pi, PI, "pi", "pi", ".pi/skills", ".pi/agent/skills" =>
            (pi, PiAdapter, Discovery, UsesOnly, install(OwnExceptSkillsCli, Some(".pi")))
        verified("1.6.0", "Base prompt lists model-eligible skills with descriptions and paths", None,
            ["https://github.com/earendil-works/pi/blob/v1.6.0/packages/coding-agent/src/core/skills.ts"]);
        Cursor, CURSOR, "cursor", "Cursor", ".cursor/skills", ".cursor/skills" =>
            (cursor, CursorAdapter, Discovery, UsesOnly, install(Shared, None))
        verified("2026.11.13", "Native skill picker and description-based model routing", None,
            ["https://cursor.com/docs/skills"]);
        GrokBuild, GROK_BUILD, "grok-build", "Grok Build", ".grok/skills", ".grok/skills" =>
            (grok_build, GrokBuildAdapter, Discovery, UsesOnly, install(Own, Some(".grok")))
        verified("2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8", "Template-specific skill listing and inspect output", Some("chat_history.jsonl can contain the listing"),
            ["https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-pager/docs/user-guide/08-skills.md",
             "https://github.com/xai-org/grok-build/blob/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-pager/docs/user-guide/17-sessions.md"]);
    }
    legacy {
        Cline, "cline", "Cline", ".cline/skills";
        Windsurf, "windsurf", "Windsurf", ".windsurf/skills";
        RooCode, "roo-code", "Roo Code", ".roo-code/skills";
        Amp, "amp", "Amp", ".amp/skills";
        Zed, "zed", "Zed", ".zed/skills";
        Void, "void", "Void", ".void/skills";
        Aider, "aider", "Aider", ".aider/skills";
        PearAi, "pear-ai", "Pear AI", ".pearai/skills";
        Continue, "continue", "Continue", ".continue/skills";
        Copilot, "copilot", "GitHub Copilot", ".copilot/skills";
        Supermaven, "supermaven", "Supermaven", ".supermaven/skills";
        Tabnine, "tabnine", "Tabnine", ".tabnine/skills";
        Sourcegraph, "sourcegraph", "Sourcegraph", ".sourcegraph/skills";
        Replit, "replit", "Replit", ".replit/skills";
        Bolt, "bolt", "Bolt", ".bolt/skills";
        V0, "v0", "v0", ".v0/skills";
        Lovable, "lovable", "Lovable", ".lovable/skills";
        Devin, "devin", "Devin", ".devin/skills";
        Goose, "goose", "Goose", ".goose/skills";
        Aide, "aide", "Aide", ".aide/skills";
        Trae, "trae", "Trae", ".trae/skills";
        Melty, "melty", "Melty", ".melty/skills";
        CodyAi, "cody-ai", "Cody AI", ".cody/skills";
        Blackbox, "blackbox", "Blackbox", ".blackbox/skills";
        Codeium, "codeium", "Codeium", ".codeium/skills";
        Qodo, "qodo", "Qodo", ".qodo/skills";
        Coderabbit, "coderabbit", "CodeRabbit", ".coderabbit/skills";
        Codium, "codium", "Codium", ".codium/skills";
        Sourcery, "sourcery", "Sourcery", ".sourcery/skills";
        AmazonQ, "amazon-q", "Amazon Q", ".amazonq/skills";
        GeminiCode, "gemini-code", "Gemini Code", ".gemini/skills";
        JetbrainsAi, "jetbrains-ai", "JetBrains AI", ".jetbrains-ai/skills";
        XcodeAi, "xcode-ai", "Xcode AI", ".xcode-ai/skills";
        Pieces, "pieces", "Pieces", ".pieces/skills";
        Mintlify, "mintlify", "Mintlify", ".mintlify/skills";
        Swimm, "swimm", "Swimm", ".swimm/skills";
        Sweep, "sweep", "Sweep", ".sweep/skills";
    }
}

/// Registration for a first-class wire id.
pub fn registered_harness(id: &str) -> Option<&'static HarnessRegistration> {
    BUILTIN_HARNESSES
        .iter()
        .find(|row| row.agent.cli_name() == id)
}
