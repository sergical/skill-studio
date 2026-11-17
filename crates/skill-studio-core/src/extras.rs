//! Names and source paths of non-skill agent configuration. No config values leave this module.
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ports::{OpContext, Runtime, ScopeFs, ScopedReads};
use crate::{ops::Operation, CoreError};

/// Keys of `OpenCode`'s V2 `mcp` object that are not server names
/// (<https://opencode.ai/config.json>, `properties.mcp.properties`).
const OPENCODE_V2_MCP_KEYS: [&str; 2] = ["servers", "timeout"];

/// Empty input. The runtime scope selects the home and projects.
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct ExtrasRequest {}

/// One declaration, or one file that could not be read.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ExtraEntry {
    /// Agent wire name.
    pub harness: String,
    /// `global` or `project`.
    pub scope: String,
    /// Project root for a project declaration.
    #[schemars(extend("x-untrusted" = true))]
    pub project: Option<PathBuf>,
    /// `hook`, `mcp_server`, `instruction`, or `config` for an unreadable file.
    pub kind: String,
    /// Event, server, or instruction filename. Absent on unreadable files.
    #[schemars(extend("x-untrusted" = true))]
    pub name: Option<String>,
    /// File that declares this item.
    #[schemars(extend("x-untrusted" = true))]
    pub path: PathBuf,
    /// Handler count for hooks. One for other declarations, zero for unreadable files.
    pub count: usize,
    /// `found` or `unreadable`. Parser errors are never exposed.
    pub status: String,
}

/// Flat list of declarations. Trust and enable settings are not evaluated.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ExtrasReport {
    /// Declarations in stable harness, scope, path, kind, and name order.
    pub entries: Vec<ExtraEntry>,
}

/// Reads config names and instruction paths without running hooks or connecting to servers.
pub fn extras(rt: &Runtime, ctx: &OpContext, _: &ExtrasRequest) -> Result<ExtrasReport, CoreError> {
    rt.run(Operation::Extras, ctx, || {
        let fs = ScopedReads::new(rt.ports.fs.as_ref(), &rt.scope);
        let home = &rt.scope.home.lexical;
        let mut entries = Vec::new();
        for project in
            std::iter::once(None).chain(rt.scope.projects.iter().map(|p| Some(p.lexical.as_path())))
        {
            ctx.checkpoint()?;
            let root = project.unwrap_or(home);
            for harness in [
                "claude-code",
                "codex",
                "opencode",
                "cursor",
                "pi",
                "grok-build",
            ] {
                let mut scan = Reader {
                    fs: &fs,
                    harness,
                    project,
                    entries: &mut entries,
                };
                let agent_dir = match harness {
                    "claude-code" if project.is_none() => {
                        rt.scope.global_root_path(Path::new(".claude"))
                    }
                    "claude-code" => root.join(".claude"),
                    "codex" if project.is_none() => rt.scope.codex_home.clone(),
                    "codex" => root.join(".codex"),
                    "opencode" if project.is_none() => rt
                        .scope
                        .opencode_config_root
                        .clone()
                        .unwrap_or_else(|| home.join(".config/opencode")),
                    "opencode" => root.join(".opencode"),
                    "pi" if project.is_none() => rt.scope.global_root_path(Path::new(".pi/agent")),
                    "pi" => root.join(".pi"),
                    "cursor" => root.join(".cursor"),
                    _ if project.is_none() => rt.scope.global_root_path(Path::new(".grok")),
                    _ => root.join(".grok"),
                };
                match harness {
                    "claude-code" => {
                        scan.config(&agent_dir.join("settings.json"), "", true);
                        if project.is_some() {
                            scan.config(&agent_dir.join("settings.local.json"), "", true);
                        }
                        if project.is_none() {
                            scan.config(
                                &rt.scope.global_root_path(Path::new(".claude.json")),
                                "mcpServers",
                                false,
                            );
                        } else {
                            scan.claude_project(
                                &rt.scope.global_root_path(Path::new(".claude.json")),
                            );
                            scan.config(&root.join(".mcp.json"), "mcpServers", false);
                        }
                    }
                    "codex" => {
                        scan.config(&agent_dir.join("config.toml"), "mcp_servers", true);
                        scan.config(&agent_dir.join("hooks.json"), "", true);
                    }
                    "opencode" => {
                        let config_root = if project.is_some() { root } else { &agent_dir };
                        for file in ["opencode.json", "opencode.jsonc"] {
                            scan.config(&config_root.join(file), "mcp", false);
                            if project.is_some() {
                                scan.config(&agent_dir.join(file), "mcp", false);
                            }
                        }
                    }
                    "cursor" => {
                        scan.config(&agent_dir.join("mcp.json"), "mcpServers", false);
                        scan.config(&agent_dir.join("hooks.json"), "", true);
                    }
                    "pi" => scan.config(&agent_dir.join("mcp.json"), "mcpServers", false),
                    _ => {
                        scan.config(&agent_dir.join("config.toml"), "mcp_servers", true);
                        scan.hook_dir(&agent_dir.join("hooks"));
                    }
                }
                let files: &[&str] = match harness {
                    "claude-code" if project.is_none() => &["CLAUDE.md"],
                    "claude-code" => &["CLAUDE.md", "CLAUDE.local.md"],
                    "codex" => &["AGENTS.md", "AGENTS.override.md"],
                    "opencode" if project.is_none() => &["AGENTS.md"],
                    "opencode" => &["AGENTS.md", "CLAUDE.md"],
                    "pi" => &["AGENTS.override.md", "AGENTS.md", "CLAUDE.md"],
                    "grok-build" => &["AGENTS.md", "AGENT.md", "CLAUDE.md", "CLAUDE.local.md"],
                    _ => &["AGENTS.md"],
                };
                if harness == "cursor" && project.is_none() {
                    continue;
                }
                for file in files {
                    scan.instruction(&if project.is_some() {
                        root.join(file)
                    } else {
                        agent_dir.join(file)
                    });
                }
                if harness == "opencode" && project.is_none() {
                    scan.instruction(&rt.scope.global_root_path(Path::new(".claude/CLAUDE.md")));
                }
                if harness == "pi" {
                    for file in ["SYSTEM.md", "APPEND_SYSTEM.md"] {
                        scan.instruction(&agent_dir.join(file));
                    }
                }
                if harness == "claude-code" && project.is_some() {
                    scan.instruction(&agent_dir.join("CLAUDE.md"));
                }
            }
        }
        entries.sort_by(|a, b| {
            (&a.harness, &a.project, &a.path, &a.kind, &a.name)
                .cmp(&(&b.harness, &b.project, &b.path, &b.kind, &b.name))
        });
        Ok(ExtrasReport { entries })
    })
}

struct Reader<'a> {
    fs: &'a dyn ScopeFs,
    harness: &'a str,
    project: Option<&'a Path>,
    entries: &'a mut Vec<ExtraEntry>,
}

impl Reader<'_> {
    fn push(&mut self, path: &Path, kind: &str, name: Option<&str>, count: usize, status: &str) {
        self.entries.push(ExtraEntry {
            harness: self.harness.into(),
            scope: if self.project.is_some() {
                "project"
            } else {
                "global"
            }
            .into(),
            project: self.project.map(Path::to_path_buf),
            kind: kind.into(),
            name: name.map(str::to_owned),
            path: path.into(),
            count,
            status: status.into(),
        });
    }

    fn load(&mut self, path: &Path) -> Option<Value> {
        match self.fs.symlink_metadata(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
            _ => {}
        }
        let value = self
            .fs
            .read_capped(path, 8 * 1024 * 1024)
            .ok()
            .and_then(|bytes| {
                let text = std::str::from_utf8(&bytes).ok()?;
                if path.extension().is_some_and(|e| e == "toml") {
                    toml::from_str::<toml::Value>(text)
                        .ok()
                        .and_then(|v| serde_json::to_value(v).ok())
                } else {
                    jsonc_parser::parse_to_serde_value(text, &Default::default())
                        .ok()
                        .flatten()
                }
            });
        match value {
            Some(v) if v.is_object() => Some(v),
            _ => {
                self.push(path, "config", None, 0, "unreadable");
                None
            }
        }
    }

    fn config(&mut self, path: &Path, mcp_key: &str, hooks: bool) {
        if let Some(value) = self.load(path) {
            self.names(path, &value, mcp_key, hooks);
        }
    }

    fn names(&mut self, path: &Path, value: &Value, mcp_key: &str, hooks: bool) {
        let servers = value.get(mcp_key).and_then(Value::as_object);
        let mut names: BTreeSet<&str> = servers
            .into_iter()
            .flat_map(|m| m.keys())
            .map(String::as_str)
            .collect();
        if mcp_key == "mcp" {
            // OpenCode reads V1 `mcp.<name>` and V2 `mcp.servers.<name>` in one file;
            // `opencode mcp add` writes V2 next to V1 entries.
            names.retain(|name| !OPENCODE_V2_MCP_KEYS.contains(name));
            let v2 = servers
                .and_then(|m| m.get("servers"))
                .and_then(Value::as_object);
            names.extend(v2.into_iter().flat_map(|m| m.keys()).map(String::as_str));
        }
        for name in names {
            self.push(path, "mcp_server", Some(name), 1, "found");
        }
        if hooks {
            if let Some(events) = value.get("hooks").and_then(Value::as_object) {
                for (name, groups) in events {
                    if let Some(groups) = groups.as_array() {
                        let count = groups
                            .iter()
                            .map(|group| {
                                group
                                    .get("hooks")
                                    .and_then(Value::as_array)
                                    .map_or(1, Vec::len)
                            })
                            .sum();
                        self.push(path, "hook", Some(name), count, "found");
                    }
                }
            }
        }
    }

    fn claude_project(&mut self, path: &Path) {
        if let Some(value) = self.load(path) {
            if let Some(config) = self
                .project
                .and_then(Path::to_str)
                .and_then(|p| value.get("projects")?.get(p))
            {
                self.names(path, config, "mcpServers", false);
            }
        }
    }

    fn instruction(&mut self, path: &Path) {
        match self.fs.symlink_metadata(path) {
            Ok(facts) if facts.kind == crate::ports::FileKind::Dir => {}
            Ok(_) => self.push(
                path,
                "instruction",
                path.file_name().and_then(|n| n.to_str()),
                1,
                "found",
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => self.push(path, "instruction", None, 0, "unreadable"),
        }
    }

    fn hook_dir(&mut self, path: &Path) {
        match self.fs.read_dir(path) {
            Ok(files) => {
                for file in files {
                    let file = path.join(file.name);
                    if file.extension().is_some_and(|e| e == "json") {
                        self.config(&file, "", true);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => self.push(path, "config", None, 0, "unreadable"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FixtureBuilder;

    fn mcp_names(config: &Value, mcp_key: &str) -> Vec<String> {
        let fs = FixtureBuilder::new().build_fs();
        let mut entries = Vec::new();
        Reader {
            fs: &fs,
            harness: "opencode",
            project: None,
            entries: &mut entries,
        }
        .names(Path::new("opencode.json"), config, mcp_key, false);
        entries.into_iter().filter_map(|e| e.name).collect()
    }

    #[test]
    fn opencode_mcp_lists_v1_and_v2_servers_without_the_v2_keys() {
        let config = serde_json::json!({"mcp": {
            "polylane": {}, "blender": {}, "timeout": {},
            "servers": {"skill-studio": {}, "blender": {}}
        }});
        assert_eq!(
            mcp_names(&config, "mcp"),
            ["blender", "polylane", "skill-studio"]
        );
    }

    #[test]
    fn other_mcp_keys_keep_servers_and_timeout_as_server_names() {
        let config = serde_json::json!({"mcpServers": {
            "servers": {"nested": {}}, "timeout": {}
        }});
        assert_eq!(mcp_names(&config, "mcpServers"), ["servers", "timeout"]);
    }
}
