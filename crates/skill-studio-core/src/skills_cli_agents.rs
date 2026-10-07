//! The agent skills folders skills CLI 1.7.0 clears on `npx skills remove <name>`.
//!
//! The CLI runs `rm -rf <agent skills folder>/<name>` for every agent it knows, installed or
//! not. `ops::remove` backs up each of these before the CLI runs, so Undo can bring back a
//! folder or link Skill Studio does not scan.

use std::path::{Path, PathBuf};

use crate::identity::RootScope;

/// From skills CLI 1.7.0 `dist/cli.mjs`, the `agents` table. Folders the CLI reads from an env
/// var (see [`cli_env_pins`]) are listed at their default location: `ops::remove` pins those
/// vars when it runs the CLI, so the CLI deletes where this table says.
///
/// Each row is (agent, global folder relative to the home, project folder relative to the
/// project). A global folder of `None` means the CLI has none for the agent and falls back to
/// the project folder under its working folder, the home. Agent names are the CLI's own keys.
const AGENTS: &[(&str, Option<&str>, &str)] = &[
    (
        "aider-desk",
        Some(".aider-desk/skills"),
        ".aider-desk/skills",
    ),
    ("amp", Some(".config/agents/skills"), ".agents/skills"),
    (
        "antigravity",
        Some(".gemini/antigravity/skills"),
        ".agents/skills",
    ),
    (
        "antigravity-cli",
        Some(".gemini/antigravity-cli/skills"),
        ".agents/skills",
    ),
    ("astrbot", Some(".astrbot/data/skills"), "data/skills"),
    (
        "autohand-code",
        Some(".autohand/skills"),
        ".autohand/skills",
    ),
    ("augment", Some(".augment/skills"), ".augment/skills"),
    ("bob", Some(".bob/skills"), ".bob/skills"),
    ("claude-code", Some(".claude/skills"), ".claude/skills"),
    ("openclaw", Some(".openclaw/skills"), "skills"),
    ("cline", Some(".agents/skills"), ".agents/skills"),
    (
        "codearts-agent",
        Some(".codeartsdoer/skills"),
        ".codeartsdoer/skills",
    ),
    ("codebuddy", Some(".codebuddy/skills"), ".codebuddy/skills"),
    ("codemaker", Some(".codemaker/skills"), ".codemaker/skills"),
    (
        "codestudio",
        Some(".codestudio/skills"),
        ".codestudio/skills",
    ),
    ("codex", Some(".codex/skills"), ".agents/skills"),
    (
        "command-code",
        Some(".commandcode/skills"),
        ".commandcode/skills",
    ),
    ("continue", Some(".continue/skills"), ".continue/skills"),
    ("cortex", Some(".snowflake/cortex/skills"), ".cortex/skills"),
    ("crush", Some(".config/crush/skills"), ".crush/skills"),
    ("cursor", Some(".cursor/skills"), ".agents/skills"),
    (
        "deepagents",
        Some(".deepagents/agent/skills"),
        ".agents/skills",
    ),
    ("devin", Some(".config/devin/skills"), ".devin/skills"),
    ("dexto", Some(".agents/skills"), ".agents/skills"),
    ("droid", Some(".factory/skills"), ".agents/skills"),
    ("eve", None, "agent/skills"),
    ("firebender", Some(".firebender/skills"), ".agents/skills"),
    ("forgecode", Some(".forge/skills"), ".forge/skills"),
    ("fx", Some(".fx/skills"), ".fx/skills"),
    ("gemini-cli", Some(".gemini/skills"), ".agents/skills"),
    ("github-copilot", Some(".copilot/skills"), ".agents/skills"),
    ("goose", Some(".config/goose/skills"), ".goose/skills"),
    ("grok", Some(".grok/skills"), ".grok/skills"),
    ("hermes-agent", Some(".hermes/skills"), ".hermes/skills"),
    (
        "inference-sh",
        Some(".inferencesh/skills"),
        ".inferencesh/skills",
    ),
    ("jazz", Some(".jazz/skills"), ".jazz/skills"),
    ("junie", Some(".junie/skills"), ".junie/skills"),
    ("iflow-cli", Some(".iflow/skills"), ".iflow/skills"),
    ("kilo", Some(".kilo/skills"), ".agents/skills"),
    (
        "kimchi",
        Some(".config/kimchi/harness/skills"),
        ".kimchi/skills",
    ),
    ("kimi-code-cli", Some(".agents/skills"), ".agents/skills"),
    ("kiro-cli", Some(".kiro/skills"), ".kiro/skills"),
    ("kode", Some(".kode/skills"), ".kode/skills"),
    ("lingma", Some(".lingma/skills"), ".lingma/skills"),
    ("loaf", Some(".agents/skills"), ".agents/skills"),
    ("mcpjam", Some(".mcpjam/skills"), ".mcpjam/skills"),
    ("minimax-code", Some(".minimax/skills"), ".minimax/skills"),
    ("mistral-vibe", Some(".vibe/skills"), ".vibe/skills"),
    ("moxby", Some(".moxby/skills"), ".moxby/skills"),
    ("mux", Some(".mux/skills"), ".mux/skills"),
    (
        "opencode",
        Some(".config/opencode/skills"),
        ".agents/skills",
    ),
    ("openhands", Some(".openhands/skills"), ".openhands/skills"),
    ("ona", Some(".ona/skills"), ".ona/skills"),
    ("pi", Some(".pi/agent/skills"), ".pi/skills"),
    (
        "posit-assistant",
        Some(".posit/assistant/skills"),
        ".posit/assistant/skills",
    ),
    ("qoder", Some(".qoder/skills"), ".qoder/skills"),
    ("qoder-cn", Some(".qoder-cn/skills"), ".qoder/skills"),
    ("qwen-code", Some(".qwen/skills"), ".qwen/skills"),
    ("replit", Some(".config/agents/skills"), ".agents/skills"),
    ("reasonix", Some(".reasonix/skills"), ".reasonix/skills"),
    ("rovodev", Some(".rovodev/skills"), ".rovodev/skills"),
    ("roo", Some(".roo/skills"), ".roo/skills"),
    ("sarvam-code", Some(".agents/skills"), ".agents/skills"),
    (
        "tabnine-cli",
        Some(".tabnine/agent/skills"),
        ".tabnine/agent/skills",
    ),
    ("terramind", Some(".terramind/skills"), ".terramind/skills"),
    ("tinycloud", Some(".tinycloud/skills"), ".tinycloud/skills"),
    ("trae", Some(".trae/skills"), ".trae/skills"),
    ("trae-cn", Some(".trae-cn/skills"), ".trae/skills"),
    ("warp", Some(".agents/skills"), ".agents/skills"),
    (
        "windsurf",
        Some(".codeium/windsurf/skills"),
        ".windsurf/skills",
    ),
    ("zed", Some(".agents/skills"), ".agents/skills"),
    ("zcode", Some(".zcode/skills"), ".zcode/skills"),
    ("zencoder", Some(".zencoder/skills"), ".zencoder/skills"),
    ("zenflow", Some(".zencoder/skills"), ".zencoder/skills"),
    ("neovate", Some(".neovate/skills"), ".neovate/skills"),
    ("pochi", Some(".pochi/skills"), ".pochi/skills"),
    ("promptscript", None, ".agents/skills"),
    ("adal", Some(".adal/skills"), ".adal/skills"),
    ("universal", Some(".config/agents/skills"), ".agents/skills"),
    // The CLI picks one OpenClaw home by which of these exists.
    ("openclaw", Some(".clawdbot/skills"), "skills"),
    ("openclaw", Some(".moltbot/skills"), "skills"),
];

/// The env vars skills CLI 1.7.0 reads agent skills folders from, each set to the folder this
/// table assumes: the default under `home`, and Codex's own directory as the adapter resolved
/// it (`codex_home`). `ops::remove` hands these to the CLI, replacing whatever the process
/// inherited, so a `VIBE_HOME` or `XDG_CONFIG_HOME` the user set cannot send the CLI's
/// `rm -rf` to a folder that was not backed up.
pub fn cli_env_pins(home: &Path, codex_home: &Path) -> Vec<(String, String)> {
    [
        ("XDG_CONFIG_HOME", home.join(".config")),
        ("CODEX_HOME", codex_home.to_path_buf()),
        ("CLAUDE_CONFIG_DIR", home.join(".claude")),
        ("VIBE_HOME", home.join(".vibe")),
        ("HERMES_HOME", home.join(".hermes")),
        ("AUTOHAND_HOME", home.join(".autohand")),
        ("GROK_HOME", home.join(".grok")),
    ]
    .into_iter()
    .map(|(name, folder)| (name.to_string(), folder.display().to_string()))
    .collect()
}

/// Every `<agent skills folder>/<name>` the CLI deletes for a removal in `scope`, without
/// duplicates. `name` must be the CLI's sanitized folder name. A global removal runs the CLI
/// in `home`, so an agent with no global folder resolves under `home`.
pub fn cli_removal_targets(
    home: &Path,
    codex_home: &Path,
    scope: &RootScope,
    name: &str,
) -> Vec<PathBuf> {
    let mut targets: Vec<PathBuf> = Vec::new();
    for &(agent, global, project_folder) in AGENTS {
        let folder = match scope {
            RootScope::Global if agent == "codex" => codex_home.join("skills"),
            RootScope::Global => home.join(global.unwrap_or(project_folder)),
            RootScope::Project(project) => project.0.join(project_folder),
        };
        let target = folder.join(name);
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    targets
}
