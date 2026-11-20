//! Estimated model-visible skill listing cost, before budget reduction.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dto::{DeploymentDto, Inventory};
use crate::error::{CoreError, ErrorCode};
use crate::harness::DisabledBy;
use crate::identity::{AgentId, RootKind, RootScope};
use crate::ports::Runtime;

/// One skill's contribution to the listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListingSkillCost {
    /// Model-visible name.
    #[schemars(extend("x-untrusted" = true))]
    pub name: String,
    /// UTF-16 characters (UTF-8 bytes for Codex), divided by four and rounded up.
    pub estimated_tokens_chars_div_4: u64,
}

/// Listing cost for one harness in one session scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListingCost {
    /// Harness id.
    pub harness: AgentId,
    /// Project sessions include global skills. Identity rules depend on the harness.
    pub scope: RootScope,
    /// Number of model-visible skill entries.
    pub listed_skills: usize,
    /// UTF-16 characters (UTF-8 bytes for Codex), divided by four and rounded up.
    pub estimated_tokens_chars_div_4: u64,
    /// Budget in estimated tokens, or null when not verified.
    pub limit_tokens: Option<u64>,
    /// Setting file path, default, or reason the limit is unknown.
    pub limit_source: String,
    /// Context size used for a fractional budget.
    pub context_window_tokens: Option<u64>,
    /// Whether a verified fallback was used instead of a session context size.
    pub uses_default_context_window: bool,
    /// `over_limit` or `near_limit` at 80 percent, otherwise null.
    pub warning: Option<String>,
    /// The five largest entries, with names as a stable tie break.
    pub top_skills: Vec<ListingSkillCost>,
}

fn read(rt: &Runtime, path: &Path) -> Result<Option<Vec<u8>>, CoreError> {
    match rt.ports.fs.read_capped(path, 2 * 1024 * 1024) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(CoreError::new(ErrorCode::Io, "Could not read listing settings").at(path)),
    }
}

fn json(rt: &Runtime, path: &Path) -> Result<serde_json::Value, CoreError> {
    read(rt, path)?.map_or(Ok(serde_json::Value::Null), |bytes| {
        serde_json::from_slice(&bytes).map_err(|_| {
            CoreError::new(
                ErrorCode::InvalidRequest,
                "Invalid JSON in listing settings",
            )
            .at(path)
        })
    })
}

fn toml(rt: &Runtime, path: &Path) -> Result<toml::Value, CoreError> {
    read(rt, path)?.map_or_else(
        || Ok(toml::Value::Table(Default::default())),
        |bytes| {
            std::str::from_utf8(&bytes)
                .ok()
                .and_then(|text| toml::from_str(text).ok())
                .ok_or_else(|| {
                    CoreError::new(
                        ErrorCode::InvalidRequest,
                        "Invalid TOML in listing settings",
                    )
                    .at(path)
                })
        },
    )
}

fn codex_settings(
    rt: &Runtime,
    scope: &RootScope,
) -> Result<Vec<(std::path::PathBuf, toml::Value)>, CoreError> {
    let user_path = rt.scope.codex_home.join("config.toml");
    let user = toml(rt, &user_path)?;
    let mut layers = Vec::new();
    if let RootScope::Project(project) = scope {
        let markers: Vec<&str> = user
            .get("project_root_markers")
            .and_then(toml::Value::as_array)
            .map_or_else(
                || vec![".git"],
                |values| values.iter().filter_map(toml::Value::as_str).collect(),
            );
        let root = project
            .0
            .ancestors()
            .find(|dir| {
                markers
                    .iter()
                    .any(|marker| rt.ports.fs.symlink_metadata(&dir.join(marker)).is_ok())
            })
            .unwrap_or(&project.0);
        let dirs: Vec<_> = project
            .0
            .ancestors()
            .take_while(|dir| dir.starts_with(root))
            .collect();
        let trust = |dir: &Path| {
            let canonical = rt
                .ports
                .fs
                .canonicalize(dir)
                .unwrap_or_else(|_| dir.to_path_buf());
            let level = [dir, canonical.as_path()].into_iter().find_map(|key| {
                user.get("projects")?
                    .get(key.to_str()?)?
                    .get("trust_level")?
                    .as_str()
            });
            level
        };
        for dir in dirs.iter().rev() {
            if trust(dir).or_else(|| trust(root)) != Some("trusted") {
                continue;
            }
            let path = dir.join(".codex/config.toml");
            if path == user_path {
                continue;
            }
            layers.push((path.clone(), toml(rt, &path)?));
        }
    }
    layers.insert(0, (user_path, user));
    Ok(layers)
}

fn manual_only(rt: &Runtime, deployment: &DeploymentDto, harness: &str) -> Result<bool, CoreError> {
    let fm = deployment.frontmatter.as_ref();
    match harness {
        AgentId::CLAUDE_CODE | AgentId::PI | AgentId::CURSOR => Ok(match fm {
            Some(f) => f.disable_model_invocation == Some(true),
            // YAML that does not parse strictly (a duplicate key) still
            // carries the field; Claude Code honors it.
            None => deployment
                .frontmatter_fields
                .get("disable-model-invocation")
                .is_some_and(|v| v.trim() == "true"),
        }),
        AgentId::OPEN_CODE => Ok(fm
            .and_then(|f| f.metadata.as_ref())
            .and_then(|m| m.get("opencode/autoinvoke"))
            .is_some_and(|v| match v {
                crate::yaml::Value::Bool(b) => !b,
                crate::yaml::Value::String(s) => s == "false",
                _ => false,
            })),
        AgentId::CODEX => {
            let path = deployment.path.join("agents/openai.yaml");
            let Some(bytes) = read(rt, &path)? else {
                return Ok(false);
            };
            let value: serde_json::Value = crate::yaml::from_str(&String::from_utf8_lossy(&bytes))
                .map_err(|_| {
                    CoreError::new(ErrorCode::InvalidRequest, "Invalid skill invocation policy")
                        .at(&path)
                })?;
            Ok(value
                .get("policy")
                .and_then(|p| p.get("allow_implicit_invocation"))
                .and_then(serde_json::Value::as_bool)
                == Some(false))
        }
        _ => Ok(false),
    }
}

/// Claude Code settings that shape the skill listing, merged from user,
/// project, and local settings in that order.
#[derive(Debug, Default)]
pub(crate) struct ClaudeListingSettings {
    /// `skillListingBudgetFraction` and the file that set it.
    pub fraction: Option<(f64, PathBuf)>,
    /// `skillListingMaxDescChars`, default 1536.
    pub max_desc_chars: usize,
    /// `enabledPlugins` switches, keyed `plugin@marketplace`.
    pub enabled_plugins: BTreeMap<String, bool>,
    /// `model`, as written (an alias such as `opus` or a model id).
    pub model: Option<String>,
    /// `skillOverrides` values by skill name.
    pub overrides: BTreeMap<String, String>,
    /// The user settings file.
    pub user_path: PathBuf,
    /// Plugin installs from `plugins/installed_plugins.json`, keyed
    /// `plugin@marketplace`: `None` for a user install, else the project.
    pub plugin_installs: BTreeMap<String, Vec<Option<PathBuf>>>,
    /// The project these settings are for.
    pub project: Option<PathBuf>,
}

impl ClaudeListingSettings {
    /// True when Claude Code loads this plugin in this session scope.
    fn plugin_off(&self, plugin: &crate::dto::PluginSourceDto) -> Option<&'static str> {
        let key = format!("{}@{}", plugin.plugin, plugin.marketplace);
        if self.enabled_plugins.get(&key).copied().or(plugin.enabled) == Some(false) {
            return Some("plugin turned off");
        }
        let installs = self.plugin_installs.get(&key)?;
        let here = installs
            .iter()
            .any(|p| p.is_none() || p.as_deref() == self.project.as_deref());
        (!here).then_some("plugin installed for another project")
    }
}

fn claude_plugin_installs(rt: &Runtime) -> BTreeMap<String, Vec<Option<PathBuf>>> {
    let path = rt
        .scope
        .global_root_path(Path::new(".claude/plugins/installed_plugins.json"));
    let Ok(value) = json(rt, &path) else {
        return BTreeMap::new();
    };
    value
        .get("plugins")
        .and_then(serde_json::Value::as_object)
        .map(|plugins| {
            plugins
                .iter()
                .filter_map(|(key, installs)| {
                    let installs = installs
                        .as_array()?
                        .iter()
                        .map(
                            |i| match i.get("scope").and_then(serde_json::Value::as_str) {
                                Some("project" | "local") => i
                                    .get("projectPath")
                                    .and_then(serde_json::Value::as_str)
                                    .map(PathBuf::from)
                                    .or_else(|| Some(PathBuf::new())),
                                _ => None,
                            },
                        )
                        .collect();
                    Some((key.clone(), installs))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn claude_settings(
    rt: &Runtime,
    scope: &RootScope,
) -> Result<ClaudeListingSettings, CoreError> {
    let user_path = rt
        .scope
        .global_root_path(Path::new(".claude/settings.json"));
    let mut out = ClaudeListingSettings {
        max_desc_chars: 1536,
        user_path: user_path.clone(),
        plugin_installs: claude_plugin_installs(rt),
        project: match scope {
            RootScope::Project(project) => Some(project.0.clone()),
            RootScope::Global => None,
        },
        ..Default::default()
    };
    let mut paths = vec![user_path];
    if let RootScope::Project(project) = scope {
        paths.extend([
            project.0.join(".claude/settings.json"),
            project.0.join(".claude/settings.local.json"),
        ]);
    }
    for path in paths {
        let settings = json(rt, &path)?;
        if let Some(plugins) = settings
            .get("enabledPlugins")
            .and_then(serde_json::Value::as_object)
        {
            out.enabled_plugins
                .extend(plugins.iter().filter_map(|(name, value)| {
                    value.as_bool().map(|enabled| (name.clone(), enabled))
                }));
        }
        if let Some(overrides) = settings
            .get("skillOverrides")
            .and_then(serde_json::Value::as_object)
        {
            out.overrides.extend(
                overrides
                    .iter()
                    .filter_map(|(name, v)| v.as_str().map(|v| (name.clone(), v.to_string()))),
            );
        }
        if let Some(model) = settings.get("model").and_then(serde_json::Value::as_str) {
            out.model = Some(model.to_string());
        }
        if let Some(value) = settings.get("skillListingBudgetFraction") {
            let fraction = value
                .as_f64()
                .filter(|v| *v > 0.0 && *v <= 1.0)
                .ok_or_else(|| {
                    CoreError::new(
                        ErrorCode::InvalidRequest,
                        "Invalid skillListingBudgetFraction",
                    )
                    .at(&path)
                })?;
            out.fraction = Some((fraction, path.clone()));
        }
        if let Some(value) = settings.get("skillListingMaxDescChars") {
            out.max_desc_chars = value
                .as_u64()
                .filter(|v| *v > 0)
                .and_then(|v| usize::try_from(v).ok())
                .ok_or_else(|| {
                    CoreError::new(
                        ErrorCode::InvalidRequest,
                        "Invalid skillListingMaxDescChars",
                    )
                    .at(&path)
                })?;
        }
    }
    Ok(out)
}

/// Codex settings that shape the skill listing, from the user layer and
/// trusted project layers.
#[derive(Debug)]
pub(crate) struct CodexListingSettings {
    /// `model_context_window` and the file that set it.
    pub context_window: Option<(u64, PathBuf)>,
    /// `skills.max_context_tokens`, before the 10000 cap, and its file.
    pub max_context_tokens: Option<(u64, PathBuf)>,
    /// `model`.
    pub model: Option<String>,
    /// The user config file.
    pub user_path: PathBuf,
}

pub(crate) fn codex_listing_settings(
    rt: &Runtime,
    scope: &RootScope,
) -> Result<CodexListingSettings, CoreError> {
    let user_path = rt.scope.codex_home.join("config.toml");
    let mut out = CodexListingSettings {
        context_window: None,
        max_context_tokens: None,
        model: None,
        user_path,
    };
    for (path, layer) in codex_settings(rt, scope)? {
        if let Some(window) = layer.get("model_context_window") {
            // A non-positive window is ignored, like an absent one.
            out.context_window = window
                .as_integer()
                .and_then(|v| u64::try_from(v).ok())
                .filter(|v| *v > 0)
                .map(|v| (v, path.clone()));
        }
        if let Some(model) = layer.get("model").and_then(toml::Value::as_str) {
            out.model = Some(model.to_string());
        }
        if let Some(max) = layer
            .get("skills")
            .and_then(|skills| skills.get("max_context_tokens"))
        {
            let value = max
                .as_integer()
                .and_then(|v| u64::try_from(v).ok())
                .filter(|v| *v > 0)
                .ok_or_else(|| {
                    CoreError::new(
                        ErrorCode::InvalidRequest,
                        "Invalid skills.max_context_tokens",
                    )
                    .at(&path)
                })?;
            out.max_context_tokens = Some((value, path.clone()));
        }
    }
    Ok(out)
}

/// The fields Codex 0.170.0 reads from frontmatter (`codex-rs/skills/src/
/// parser.rs`), typed the same way: serde rejects a repeated known field
/// and ignores unknown ones; a scalar keeps its text.
#[derive(serde::Deserialize)]
struct CodexFrontmatter {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    metadata: CodexFrontmatterMetadata,
}

#[derive(serde::Deserialize, Default)]
struct CodexFrontmatterMetadata {
    #[serde(default, rename = "short-description")]
    #[allow(dead_code)]
    short_description: Option<String>,
}

/// Name and description the way Codex 0.170.0 reads a `SKILL.md`: typed
/// YAML; on an error, values with a `: ` in them are quoted line by line
/// and the YAML is read again; any other error rejects the skill. The name
/// (or `default_name`, the folder name) has at most 64 characters.
/// Whitespace runs become one space. `None` when Codex rejects the file.
pub(crate) fn codex_frontmatter(contents: &str, default_name: &str) -> Option<(String, String)> {
    let mut lines = contents.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    let mut body = Vec::new();
    let mut closed = false;
    for line in lines {
        if line.trim() == "---" {
            closed = true;
            break;
        }
        body.push(line);
    }
    if body.is_empty() || !closed {
        return None;
    }
    let parse = |text: &str| crate::yaml::from_str::<CodexFrontmatter>(text).ok();
    let parsed = parse(&body.join("\n")).or_else(|| parse(&codex_repair_scalars(&body)?))?;
    let one_line = |s: String| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let description = parsed.description.map(one_line).unwrap_or_default();
    let name = parsed
        .name
        .map(one_line)
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| default_name.to_string());
    (name.chars().count() <= 64).then_some((name, description))
}

/// `repair_frontmatter_scalar_fields`: quotes each plain `key: value` whose
/// value holds `: `. `None` when nothing changed.
fn codex_repair_scalars(lines: &[&str]) -> Option<String> {
    let mut changed = false;
    let mut block_indent: Option<usize> = None;
    let mut out = Vec::new();
    for line in lines {
        let indent = line.chars().take_while(|c| *c == ' ').count();
        if let Some(block) = block_indent {
            if line.trim().is_empty() || indent > block {
                out.push((*line).to_string());
                continue;
            }
            block_indent = None;
        }
        let Some((key, value)) = line.split_once(':') else {
            out.push((*line).to_string());
            continue;
        };
        if key.trim().is_empty() || !value.chars().next().is_none_or(char::is_whitespace) {
            out.push((*line).to_string());
            continue;
        }
        let trimmed = value.trim_start();
        let lead = &value[..value.len() - trimmed.len()];
        let mut scalar = trimmed;
        let mut comment = "";
        for (i, c) in trimmed.char_indices() {
            if c == '#'
                && (i == 0
                    || trimmed[..i]
                        .chars()
                        .next_back()
                        .is_some_and(char::is_whitespace))
            {
                let end = trimmed[..i].trim_end().len();
                scalar = &trimmed[..end];
                comment = &trimmed[end..];
                break;
            }
        }
        let scalar = scalar.trim_end();
        let Some(first) = scalar.chars().next() else {
            out.push((*line).to_string());
            continue;
        };
        if matches!(first, '|' | '>') {
            block_indent = Some(indent);
            out.push((*line).to_string());
            continue;
        }
        if matches!(first, '\'' | '"') {
            out.push((*line).to_string());
            continue;
        }
        let colon = scalar
            .char_indices()
            .any(|(i, c)| c == ':' && scalar[i + 1..].starts_with(char::is_whitespace));
        let flow_like = matches!(first, '[' | '{' | '@' | '`')
            && crate::yaml::from_str::<serde_json::Value>(scalar).is_err();
        if !colon && !flow_like {
            out.push((*line).to_string());
            continue;
        }
        out.push(format!(
            "{key}:{lead}'{}'{comment}",
            scalar.replace('\'', "''")
        ));
        changed = true;
    }
    changed.then(|| out.join("\n"))
}

/// One model-visible skill entry, or a skill the harness hides.
#[derive(Debug, Clone)]
pub(crate) struct ListedSkill {
    /// Frontmatter name, or the skill name.
    pub name: String,
    /// Plugin name for plugin-shipped deployments.
    pub plugin: Option<String>,
    /// Raw description, before any cap.
    pub description: String,
    /// Claude Code's `when_to_use` frontmatter text.
    pub when_to_use: Option<String>,
    /// Deployment folder.
    pub path: PathBuf,
    /// True for a project root, false for a global root.
    pub project_root: bool,
    /// Why the harness does not list it, or `None` when it does.
    pub hidden: Option<&'static str>,
    /// Manual-only in Claude Code (`disable-model-invocation`), read for
    /// other harnesses to flag one that still lists it.
    pub claude_manual: bool,
}

/// Every deployment `facts` reads in `scope`, keyed by the harness's
/// identity rule. Hidden skills stay in the map with a reason, so callers
/// that only count listed skills filter on `hidden.is_none()`.
pub(crate) fn listed_skills(
    rt: &Runtime,
    inventory: &Inventory,
    facts: &crate::harness::HarnessFacts,
    scope: &RootScope,
    claude: Option<&ClaudeListingSettings>,
) -> Result<BTreeMap<String, ListedSkill>, CoreError> {
    let harness = facts.id.as_str();
    let mut entries = BTreeMap::new();
    for selected_scope in
        std::iter::once(&RootScope::Global).chain((*scope != RootScope::Global).then_some(scope))
    {
        for skill in &inventory.skills {
            for deployment in &skill.deployments {
                if &deployment.root.scope != selected_scope || deployment.symlink_is_broken {
                    continue;
                }
                let visible = deployment.harness.as_ref() == Some(&facts.id)
                    || deployment.root.kind == RootKind::Universal
                        && facts.reads_universal_root.is_yes()
                    || harness == AgentId::CURSOR
                        && matches!(&deployment.root.kind, RootKind::Harness(id) if id.as_str() == AgentId::CLAUDE_CODE || id.as_str() == AgentId::CODEX);
                if !visible && deployment.root.kind != RootKind::Parked {
                    continue;
                }
                // Codex reads the file its own way; see `codex_frontmatter`.
                let codex = if harness == AgentId::CODEX {
                    Some(
                        read(rt, &deployment.path.join("SKILL.md"))
                            .ok()
                            .flatten()
                            .and_then(|b| {
                                let folder = deployment
                                    .path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().into_owned())
                                    .unwrap_or_default();
                                codex_frontmatter(&String::from_utf8_lossy(&b), &folder)
                            }),
                    )
                } else {
                    None
                };
                let field = |key: &str| match &deployment.frontmatter {
                    _ if codex.is_some() => {
                        codex
                            .as_ref()
                            .and_then(|c| c.as_ref())
                            .map(|(name, description)| match key {
                                "name" => name.clone(),
                                _ => description.clone(),
                            })
                    }
                    Some(f) => match key {
                        "name" => f.name.clone(),
                        _ => f.description.clone(),
                    },
                    // Frontmatter that is not valid YAML (a description
                    // with `: ` in it): every agent still reads its plain
                    // lines. Codex 0.170.0 lists such a skill (checked
                    // against recorded sessions on 2026-11-20).
                    None => deployment.frontmatter_fields.get(key).cloned(),
                };
                let name = field("name").unwrap_or_else(|| skill.name.0.clone());
                let key = if matches!(harness, AgentId::CODEX | AgentId::CLAUDE_CODE) {
                    rt.ports
                        .fs
                        .canonicalize(&deployment.path)
                        .unwrap_or_else(|_| deployment.path.clone())
                        .to_string_lossy()
                        .into_owned()
                } else {
                    name.clone()
                };
                let description = field("description").unwrap_or_default();
                let entry = ListedSkill {
                    name,
                    plugin: deployment.plugin.as_ref().map(|p| p.plugin.clone()),
                    description,
                    when_to_use: deployment
                        .frontmatter_fields
                        .get("when_to_use")
                        .filter(|v| !v.trim().is_empty())
                        .cloned(),
                    path: deployment.path.clone(),
                    project_root: *selected_scope != RootScope::Global,
                    hidden: None,
                    claude_manual: harness != AgentId::CLAUDE_CODE
                        && manual_only(rt, deployment, AgentId::CLAUDE_CODE)?,
                };
                let filtered = if deployment.root.kind == RootKind::Parked {
                    // A parked copy is listed only when the harness reads it.
                    if !parked_copy_was_read_by(deployment, &facts.id) {
                        continue;
                    }
                    Some("parked")
                } else if deployment.studio_disabled {
                    Some("off in Skill Studio")
                } else if deployment
                    .disabled_by
                    .is_some_and(|by| disables_reader(by, &facts.id))
                    || deployment.disabled_readers.contains(&facts.id)
                {
                    Some("off in agent settings")
                } else if let Some(plugin) = &deployment.plugin {
                    match claude {
                        Some(claude) if harness == AgentId::CLAUDE_CODE => {
                            claude.plugin_off(plugin)
                        }
                        _ => (plugin.enabled == Some(false)).then_some("plugin turned off"),
                    }
                } else {
                    None
                };
                if let Some(reason) = filtered {
                    entries.entry(key).or_insert(ListedSkill {
                        hidden: Some(reason),
                        ..entry
                    });
                    continue;
                }
                if manual_only(rt, deployment, harness)? {
                    entries.insert(
                        key,
                        ListedSkill {
                            hidden: Some("manual-only"),
                            ..entry
                        },
                    );
                    continue;
                }
                if codex.as_ref().is_some_and(Option::is_none) {
                    entries.entry(key).or_insert(ListedSkill {
                        hidden: Some(
                            "invalid frontmatter, Codex cannot load it (see skill-studio diagnose)",
                        ),
                        ..entry
                    });
                    continue;
                }
                if harness == AgentId::CODEX && entry.description.trim().is_empty() {
                    entries.entry(key).or_insert(ListedSkill {
                        hidden: Some("no description"),
                        ..entry
                    });
                    continue;
                }
                entries.insert(key, entry);
            }
        }
    }
    Ok(entries)
}

/// Parked deployments sit in Skill Studio's own root. They count for a
/// harness only to show that it lost them, so they belong to the harness
/// whose folder they came from, or to every reader of Universal.
fn parked_copy_was_read_by(deployment: &DeploymentDto, reader: &AgentId) -> bool {
    deployment
        .harness
        .as_ref()
        .is_none_or(|harness| harness == reader)
}

/// Claude Code's character budget for the listing, resolved the way the
/// installed binary does it (see docs/agent-skill-conventions.md).
#[derive(Debug, Clone)]
pub(crate) struct ClaudeBudget {
    /// Budget in characters.
    pub chars: u64,
    /// Context window in tokens.
    pub context_tokens: u64,
    /// Characters per token for the model: 4 for older models, else 3.
    pub chars_per_token: u64,
    /// Fraction that applied.
    pub fraction: f64,
    /// Where the fraction came from.
    pub fraction_source: String,
    /// Where the context window came from.
    pub context_source: String,
    /// True when no model or catalog entry was found.
    pub default_context: bool,
}

/// Models that Claude Code 2.1.327 (`Ky`) counts at 4 characters per token.
/// Newer models count at 3.
const CLAUDE_FOUR_CHAR_MODELS: [&str; 14] = [
    "claude-3-opus",
    "claude-3-sonnet",
    "claude-3-haiku",
    "claude-3-5-sonnet",
    "claude-3-5-haiku",
    "claude-3-7-sonnet",
    "claude-opus-4-0",
    "claude-opus-4-1",
    "claude-opus-4-5",
    "claude-opus-4-6",
    "claude-sonnet-4-0",
    "claude-sonnet-4-5",
    "claude-sonnet-4-6",
    "claude-haiku-4-5",
];

pub(crate) fn claude_chars_per_token(model: &str) -> u64 {
    let mut id = model.to_ascii_lowercase().replace(['.', '_'], "-");
    if let Some((head, tail)) = id.rsplit_once('-') {
        if tail.len() == 8 && tail.bytes().all(|b| b.is_ascii_digit()) {
            id = head.to_string();
        }
    }
    if CLAUDE_FOUR_CHAR_MODELS.contains(&id.as_str()) {
        4
    } else {
        3
    }
}

/// Resolves a model alias or id against Claude Code's cached model catalog
/// (`~/.claude/cache/model-catalog/*.json`) and returns the id and its
/// `runtime.max_input_tokens`.
pub(crate) fn claude_model_window(rt: &Runtime, model: &str) -> Option<(String, u64, PathBuf)> {
    let (model, one_million) = match model.strip_suffix("[1m]") {
        Some(base) => (base.trim().to_ascii_lowercase(), true),
        None => (model.trim().to_ascii_lowercase(), false),
    };
    let dir = rt
        .scope
        .global_root_path(Path::new(".claude/cache/model-catalog"));
    let mut newest: Option<(i64, serde_json::Value, PathBuf)> = None;
    for entry in rt.ports.fs.read_dir(&dir).ok()? {
        if !Path::new(&entry.name)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("json"))
        {
            continue;
        }
        let path = dir.join(&entry.name);
        let Ok(value) = json(rt, &path) else { continue };
        let fetched = value
            .get("fetchedAt")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        if newest.as_ref().is_none_or(|(at, ..)| fetched > *at) {
            newest = Some((fetched, value, path));
        }
    }
    let (_, catalog, path) = newest?;
    let config = catalog.get("catalog")?.get("config")?;
    let models = config.get("models")?.as_array()?;
    let base = if model == "opusplan" { "opus" } else { &model };
    let id = if models
        .iter()
        .any(|m| m.get("id").and_then(serde_json::Value::as_str) == Some(base))
    {
        base.to_string()
    } else {
        config
            .get("provider_alias_targets")?
            .get(base)?
            .get("default")?
            .as_str()?
            .to_string()
    };
    let window = models
        .iter()
        .find(|m| m.get("id").and_then(serde_json::Value::as_str) == Some(id.as_str()))?
        .get("runtime")?
        .get("max_input_tokens")?
        .as_u64()?;
    Some((id, if one_million { 1_000_000 } else { window }, path))
}

/// The cached value of the experiment Claude Code applies only when
/// `skillListingBudgetFraction` is not set (`tengu_typed_pebble`, capped at
/// 100000 characters).
fn claude_cached_experiment_floor(rt: &Runtime) -> Option<u64> {
    let config = json(rt, &rt.scope.global_root_path(Path::new(".claude.json"))).ok()?;
    config
        .get("cachedGrowthBookFeatures")?
        .get("tengu_typed_pebble")?
        .as_u64()
        .map(|v| v.min(100_000))
}

pub(crate) fn claude_budget(
    rt: &Runtime,
    settings: &ClaudeListingSettings,
    observed_model: Option<&str>,
    fraction_override: Option<f64>,
    context_override: Option<u64>,
) -> ClaudeBudget {
    let model = settings.model.as_deref().or(observed_model);
    let resolved = model.and_then(|m| claude_model_window(rt, m));
    let (context_tokens, context_source, default_context) = match (context_override, &resolved) {
        (Some(tokens), _) => (tokens, "chosen in Skill Studio".to_string(), false),
        (None, Some((id, window, path))) => (*window, format!("{id} in {}", path.display()), false),
        (None, None) => (
            200_000,
            "Claude Code default: 200000 tokens when the model is unknown".to_string(),
            true,
        ),
    };
    let model_id = resolved.map(|(id, ..)| id).or(model.map(str::to_string));
    let chars_per_token = model_id.as_deref().map_or(4, claude_chars_per_token);
    let (fraction, fraction_source, explicit) = match (fraction_override, &settings.fraction) {
        (Some(f), _) => (f, "chosen in Skill Studio".to_string(), true),
        (None, Some((f, path))) => (*f, path.display().to_string(), true),
        (None, None) => (0.01, "Claude Code default (1%)".to_string(), false),
    };
    let base = ((context_tokens * chars_per_token) as f64 * fraction).floor() as u64;
    let floor = if explicit {
        0
    } else {
        claude_cached_experiment_floor(rt).unwrap_or(0)
    };
    let fraction_source = if floor > base {
        format!("{fraction_source}; an account experiment raises it to {floor} characters")
    } else {
        fraction_source
    };
    ClaudeBudget {
        chars: base.max(floor).max(1),
        context_tokens,
        chars_per_token,
        fraction,
        fraction_source,
        context_source,
        default_context,
    }
}

/// Codex's budget for the listing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodexLimit {
    /// Estimated tokens, UTF-8 bytes divided by four.
    Tokens(u64),
    /// Characters, when no context window is known.
    Chars(u64),
}

#[derive(Debug, Clone)]
pub(crate) struct CodexBudget {
    pub limit: CodexLimit,
    pub context_tokens: Option<u64>,
    pub source: String,
    pub default_context: bool,
}

/// `context_window`, or `max_context_window`, of `model` in Codex's
/// `models_cache.json`.
pub(crate) fn codex_model_window(rt: &Runtime, model: &str) -> Option<(u64, PathBuf)> {
    let path = rt.scope.codex_home.join("models_cache.json");
    let cache = json(rt, &path).ok()?;
    let entry = cache
        .get("models")?
        .as_array()?
        .iter()
        .find(|m| m.get("slug").and_then(serde_json::Value::as_str) == Some(model))?;
    let window = entry
        .get("context_window")
        .and_then(serde_json::Value::as_u64)
        .or_else(|| {
            entry
                .get("max_context_window")
                .and_then(serde_json::Value::as_u64)
        })
        .filter(|v| *v > 0)?;
    Some((window, path))
}

/// `session_window` is the context window of the model a recorded session
/// ran with; it wins over the configured model, because an app can start
/// Codex with another model.
pub(crate) fn codex_budget(
    rt: &Runtime,
    settings: &CodexListingSettings,
    max_tokens_override: Option<u64>,
    session_window: Option<(u64, String)>,
) -> CodexBudget {
    let window = session_window
        .map(|(v, source)| (v, format!("2% of the {source}")))
        .or_else(|| {
            settings.context_window.as_ref().map(|(v, path)| {
                (
                    *v,
                    format!("{} (2% of model_context_window)", path.display()),
                )
            })
        })
        .or_else(|| {
            let model = settings.model.as_deref()?;
            codex_model_window(rt, model)
                .map(|(v, path)| (v, format!("2% of {model} context in {}", path.display())))
        });
    let mut budget = match &window {
        Some((tokens, source)) => CodexBudget {
            limit: CodexLimit::Tokens((tokens * 2 / 100).max(1)),
            context_tokens: Some(*tokens),
            source: source.clone(),
            default_context: false,
        },
        None => CodexBudget {
            limit: CodexLimit::Chars(8000),
            context_tokens: None,
            source: "Codex default: 8000 characters when session context is unknown".into(),
            default_context: true,
        },
    };
    let max = max_tokens_override
        .map(|v| (v, "chosen in Skill Studio".to_string()))
        .or_else(|| {
            settings
                .max_context_tokens
                .as_ref()
                .map(|(v, path)| (*v, path.display().to_string()))
        });
    if let Some((tokens, source)) = max {
        budget.limit = CodexLimit::Tokens(tokens.clamp(1, 10_000));
        budget.source = source;
        budget.default_context = false;
    }
    budget
}

/// Uses the scan's deployments, so parked roots never enter the listing.
pub(crate) fn listing_cost(
    rt: &Runtime,
    inventory: &Inventory,
) -> Result<Vec<ListingCost>, CoreError> {
    let mut scopes = vec![RootScope::Global];
    scopes.extend(
        rt.scope
            .projects
            .iter()
            .map(|p| RootScope::Project(crate::identity::ProjectRef(p.lexical.clone()))),
    );
    let mut rows = Vec::new();
    for scope in scopes {
        for facts in &rt.ports.catalog.facts {
            let harness = facts.id.as_str();
            let mut cap = usize::MAX;
            let mut limit = None;
            let mut source = "no known limit".to_string();
            let mut context = None;
            let mut fallback = false;
            let mut claude = None;
            if harness == AgentId::CLAUDE_CODE {
                let settings = claude_settings(rt, &scope)?;
                let budget = claude_budget(rt, &settings, None, None, None);
                cap = settings.max_desc_chars;
                source = if settings.fraction.is_some() {
                    budget.fraction_source
                } else {
                    "Claude Code 2.1.327 default (1%)".into()
                };
                context = Some(budget.context_tokens);
                fallback = budget.default_context;
                limit = Some(budget.chars.div_ceil(4).max(1));
                claude = Some(settings);
            } else if harness == AgentId::CODEX {
                let budget = codex_budget(rt, &codex_listing_settings(rt, &scope)?, None, None);
                cap = 1024;
                source = budget.source;
                context = budget.context_tokens;
                fallback = budget.default_context;
                limit = Some(match budget.limit {
                    CodexLimit::Tokens(tokens) => tokens,
                    CodexLimit::Chars(chars) => chars / 4,
                });
            }
            let entries: BTreeMap<_, _> =
                listed_skills(rt, inventory, facts, &scope, claude.as_ref())?
                    .into_values()
                    .filter(|e| e.hidden.is_none())
                    .map(|e| {
                        let description: String = e.description.chars().take(cap).collect();
                        let chars = if harness == AgentId::CODEX {
                            e.name.len()
                                + description.len()
                                + e.path.join("SKILL.md").to_string_lossy().len()
                                + 15
                        } else {
                            e.name.encode_utf16().count() + description.encode_utf16().count() + 5
                        };
                        (e.path.clone(), (e.name, chars as u64))
                    })
                    .collect();
            let total_chars: u64 = entries.values().map(|(_, chars)| chars).sum();
            let estimated = total_chars.div_ceil(4);
            let warning = limit.and_then(|max| {
                if estimated > max {
                    Some("over_limit".into())
                } else if estimated > 0 && estimated.saturating_mul(5) >= max.saturating_mul(4) {
                    Some("near_limit".into())
                } else {
                    None
                }
            });
            let count = entries.len();
            let mut top: Vec<_> = entries
                .into_values()
                .map(|(name, chars)| ListingSkillCost {
                    name,
                    estimated_tokens_chars_div_4: chars.div_ceil(4),
                })
                .collect();
            top.sort_by(|a, b| {
                b.estimated_tokens_chars_div_4
                    .cmp(&a.estimated_tokens_chars_div_4)
                    .then(a.name.cmp(&b.name))
            });
            top.truncate(5);
            rows.push(ListingCost {
                harness: facts.id.clone(),
                scope: scope.clone(),
                listed_skills: count,
                estimated_tokens_chars_div_4: estimated,
                limit_tokens: limit,
                limit_source: source,
                context_window_tokens: context,
                uses_default_context_window: fallback,
                warning,
                top_skills: top,
            });
        }
    }
    Ok(rows)
}

fn disables_reader(by: DisabledBy, reader: &AgentId) -> bool {
    let owner = match by {
        DisabledBy::StudioMoved => return true,
        DisabledBy::CodexConfig => AgentId::CODEX,
        DisabledBy::OpencodePermission => AgentId::OPEN_CODE,
        DisabledBy::ClaudeLinkRemoved | DisabledBy::ClaudeSkillOverrides => AgentId::CLAUDE_CODE,
        DisabledBy::PiSettings => AgentId::PI,
    };
    reader.as_str() == owner
}
