//! Names and source paths of non-skill agent configuration. No config values leave this module.
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ports::{OpContext, Runtime, ScopeFs, ScopedReads};
use crate::{ops::Operation, CoreError};

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
    pub project: Option<PathBuf>,
    /// `hook`, `mcp_server`, `instruction`, or `config` for an unreadable file.
    pub kind: String,
    /// Event, server, or instruction filename. Absent on unreadable files.
    pub name: Option<String>,
    /// File that declares this item.
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
                    "claude-code" => root.join(".claude"),
                    "codex" if project.is_none() => rt.scope.codex_home.clone(),
                    "codex" => root.join(".codex"),
                    "opencode" if project.is_none() => rt
                        .scope
                        .opencode_config_root
                        .clone()
                        .unwrap_or_else(|| home.join(".config/opencode")),
                    "opencode" => root.join(".opencode"),
                    "pi" if project.is_none() => root.join(".pi/agent"),
                    "pi" => root.join(".pi"),
                    "cursor" => root.join(".cursor"),
                    _ => root.join(".grok"),
                };
                match harness {
                    "claude-code" => {
                        for file in ["settings.json", "settings.local.json"] {
                            scan.config(&agent_dir.join(file), "", true);
                        }
                        if project.is_none() {
                            scan.config(&home.join(".claude.json"), "mcpServers", false);
                        } else {
                            scan.claude_project(&home.join(".claude.json"));
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
                    "grok-build" => &["AGENTS.md", "AGENT.md"],
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
                    scan.instruction(&home.join(".claude/CLAUDE.md"));
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
                } else if path.extension().is_some_and(|e| e == "jsonc") {
                    jsonc_parser::parse_to_serde_value(text, &Default::default())
                        .ok()
                        .flatten()
                } else {
                    serde_json::from_str(text).ok()
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
        if let Some(servers) = value.get(mcp_key).and_then(Value::as_object) {
            for name in servers.keys() {
                self.push(path, "mcp_server", Some(name), 1, "found");
            }
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
