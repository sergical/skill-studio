//! Estimated model-visible skill listing cost, before budget reduction.
use std::collections::BTreeMap;
use std::path::Path;

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
        AgentId::CLAUDE_CODE | AgentId::PI | AgentId::CURSOR => {
            Ok(fm.and_then(|f| f.disable_model_invocation) == Some(true))
        }
        AgentId::OPEN_CODE => Ok(fm
            .and_then(|f| f.metadata.as_ref())
            .and_then(|m| m.get("opencode/autoinvoke"))
            .is_some_and(|v| v.as_bool() == Some(false) || v.as_str() == Some("false"))),
        AgentId::CODEX => {
            let path = deployment.path.join("agents/openai.yaml");
            let Some(bytes) = read(rt, &path)? else {
                return Ok(false);
            };
            let value: serde_yaml::Value = serde_yaml::from_slice(&bytes).map_err(|_| {
                CoreError::new(ErrorCode::InvalidRequest, "Invalid skill invocation policy")
                    .at(&path)
            })?;
            Ok(value
                .get("policy")
                .and_then(|p| p.get("allow_implicit_invocation"))
                .and_then(serde_yaml::Value::as_bool)
                == Some(false))
        }
        _ => Ok(false),
    }
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
            let mut enabled_plugins = BTreeMap::new();
            if harness == AgentId::CLAUDE_CODE {
                let mut fraction = 0.01;
                cap = 1536;
                source = "Claude Code 2.1.319 default (1%)".into();
                let mut paths = vec![rt
                    .scope
                    .global_root_path(Path::new(".claude/settings.json"))];
                if let RootScope::Project(project) = &scope {
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
                        enabled_plugins.extend(plugins.iter().filter_map(|(name, value)| {
                            value.as_bool().map(|enabled| (name.clone(), enabled))
                        }));
                    }
                    if let Some(value) = settings.get("skillListingBudgetFraction") {
                        fraction = value
                            .as_f64()
                            .filter(|v| *v > 0.0 && *v <= 1.0)
                            .ok_or_else(|| {
                                CoreError::new(
                                    ErrorCode::InvalidRequest,
                                    "Invalid skillListingBudgetFraction",
                                )
                                .at(&path)
                            })?;
                        source = path.display().to_string();
                    }
                    if let Some(value) = settings.get("skillListingMaxDescChars") {
                        cap = value
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
                context = Some(200_000);
                fallback = true;
                limit = Some((200_000.0 * fraction).floor().max(1.0) as u64);
            } else if harness == AgentId::CODEX {
                let mut window_setting = None;
                let mut max_setting = None;
                let mut path = rt.scope.codex_home.join("config.toml");
                let mut window_path = path.clone();
                for (layer_path, layer) in codex_settings(rt, &scope)? {
                    if let Some(window) = layer.get("model_context_window") {
                        window_setting = Some(window.clone());
                        window_path.clone_from(&layer_path);
                    }
                    if let Some(max) = layer
                        .get("skills")
                        .and_then(|skills| skills.get("max_context_tokens"))
                    {
                        max_setting = Some(max.clone());
                        path = layer_path;
                    }
                }
                cap = 1024;
                source = "Codex default: 8000 characters when session context is unknown".into();
                limit = Some(2000);
                fallback = true;
                if let Some(window) = window_setting
                    .as_ref()
                    .and_then(toml::Value::as_integer)
                    .and_then(|v| u64::try_from(v).ok())
                    .filter(|v| *v > 0)
                {
                    context = Some(window);
                    fallback = false;
                    limit = Some(window.saturating_mul(2).saturating_div(100).max(1));
                    source = format!("{} (2% of model_context_window)", window_path.display());
                }
                if let Some(value) = max_setting.as_ref() {
                    limit = Some(
                        value
                            .as_integer()
                            .and_then(|v| u64::try_from(v).ok())
                            .filter(|v| *v > 0)
                            .map(|v| v.min(10_000))
                            .ok_or_else(|| {
                                CoreError::new(
                                    ErrorCode::InvalidRequest,
                                    "Invalid skills.max_context_tokens",
                                )
                                .at(&path)
                            })?,
                    );
                    source = path.display().to_string();
                    fallback = false;
                }
            }
            let mut entries = BTreeMap::new();
            for selected_scope in std::iter::once(&RootScope::Global)
                .chain((scope != RootScope::Global).then_some(&scope))
            {
                for skill in &inventory.skills {
                    for deployment in &skill.deployments {
                        if &deployment.root.scope != selected_scope
                            || deployment.root.kind == RootKind::Parked
                            || deployment.studio_disabled
                            || deployment
                                .disabled_by
                                .is_some_and(|by| disables_reader(by, &facts.id))
                            || deployment.disabled_readers.contains(&facts.id)
                            || deployment.plugin.as_ref().is_some_and(|plugin| {
                                if harness == AgentId::CLAUDE_CODE {
                                    enabled_plugins
                                        .get(&format!("{}@{}", plugin.plugin, plugin.marketplace))
                                        .copied()
                                        .or(plugin.enabled)
                                        == Some(false)
                                } else {
                                    plugin.enabled == Some(false)
                                }
                            })
                            || deployment.symlink_is_broken
                        {
                            continue;
                        }
                        let visible = deployment.harness.as_ref() == Some(&facts.id)
                            || deployment.root.kind == RootKind::Universal
                                && facts.reads_universal_root.is_yes()
                            || harness == AgentId::CURSOR
                                && matches!(&deployment.root.kind, RootKind::Harness(id) if id.as_str() == AgentId::CLAUDE_CODE || id.as_str() == AgentId::CODEX);
                        if !visible {
                            continue;
                        }
                        let name = deployment
                            .frontmatter
                            .as_ref()
                            .and_then(|f| f.name.as_deref())
                            .unwrap_or(&skill.name.0)
                            .to_string();
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
                        if manual_only(rt, deployment, harness)? {
                            entries.remove(&key);
                            continue;
                        }
                        let description = deployment
                            .frontmatter
                            .as_ref()
                            .and_then(|f| f.description.as_deref())
                            .unwrap_or("");
                        if harness == AgentId::CODEX && description.trim().is_empty() {
                            continue;
                        }
                        let description: String = description.chars().take(cap).collect();
                        let chars = if harness == AgentId::CODEX {
                            name.len()
                                + description.len()
                                + deployment.path.join("SKILL.md").to_string_lossy().len()
                                + 15
                        } else {
                            name.encode_utf16().count() + description.encode_utf16().count() + 5
                        };
                        entries.insert(key, (name, chars as u64));
                    }
                }
            }
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
