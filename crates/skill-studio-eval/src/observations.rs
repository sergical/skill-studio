use std::collections::HashSet;
use std::path::Path;

use serde_json::Value;

use crate::files::resolved_path;
use crate::protocol::{HarnessId, SkillLoaded};

#[derive(Default)]
pub struct LoadingEvidence {
    pending: HashSet<String>,
    pub loaded: SkillLoaded,
}

impl LoadingEvidence {
    pub fn observe(&mut self, client: HarnessId, line: &str, skill_file: &Path, workspace: &Path) {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return;
        };
        let target = |path: &Value| {
            path.as_str().is_some_and(|path| {
                resolved_path(&workspace.join(path)) == resolved_path(skill_file)
            })
        };
        let Some(skill_dir) = skill_file.parent() else {
            return;
        };
        let name = skill_dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let loaded_directory = |text: &str| {
            text.lines().any(|line| {
                line.strip_prefix("Base directory for this skill: ")
                    .is_some_and(|path| resolved_path(Path::new(path)) == resolved_path(skill_dir))
            })
        };
        match client {
            HarnessId::ClaudeCode => {
                if value["type"] == "assistant" {
                    if let Some(blocks) = value["message"]["content"].as_array() {
                        for block in blocks {
                            if block["type"] == "tool_use"
                                && block["name"] == "Read"
                                && target(&block["input"]["file_path"])
                            {
                                if let Some(id) = block["id"].as_str() {
                                    self.pending.insert(id.into());
                                }
                            }
                        }
                    }
                }
                if value["type"] == "user" {
                    if let Some(blocks) = value["message"]["content"].as_array() {
                        for block in blocks {
                            if block["type"] == "text"
                                && block["text"].as_str().is_some_and(loaded_directory)
                            {
                                self.loaded = SkillLoaded::Yes;
                            }
                            if block["type"] == "tool_result"
                                && block["is_error"].as_bool() != Some(true)
                                && block["tool_use_id"]
                                    .as_str()
                                    .is_some_and(|id| self.pending.remove(id))
                            {
                                self.loaded = SkillLoaded::Yes;
                            }
                        }
                    }
                }
            }
            HarnessId::Codex => {
                let item = &value["item"];
                let command = item["command"].as_str().unwrap_or_default();
                if value["type"] == "item.completed"
                    && item["type"] == "command_execution"
                    && item["exit_code"] == 0
                    && command_names_skill_file(command, skill_file, workspace)
                    && ["cat ", "sed ", "head "]
                        .iter()
                        .any(|read| command.contains(read))
                    && item["aggregated_output"]
                        .as_str()
                        .is_some_and(|output| output.contains(&format!("name: {name}")))
                {
                    self.loaded = SkillLoaded::Yes;
                }
            }
            HarnessId::Pi => {
                if value["type"] == "tool_execution_start"
                    && value["toolName"] == "read"
                    && target(&value["args"]["path"])
                {
                    if let Some(id) = value["toolCallId"].as_str() {
                        self.pending.insert(id.into());
                    }
                }
                if value["type"] == "tool_execution_end"
                    && value["isError"].as_bool() == Some(false)
                    && value["toolCallId"]
                        .as_str()
                        .is_some_and(|id| self.pending.remove(id))
                {
                    self.loaded = SkillLoaded::Yes;
                }
            }
            HarnessId::OpenCode => {
                let part = &value["part"];
                if part["type"] == "tool"
                    && part["state"]["status"] == "completed"
                    && ((part["tool"] == "read" && target(&part["state"]["input"]["path"]))
                        || (part["tool"] == "skill"
                            && part["state"]["input"]["name"].as_str() == Some(name)
                            && part["state"]["output"]
                                .as_str()
                                .is_some_and(loaded_directory)))
                {
                    self.loaded = SkillLoaded::Yes;
                }
            }
        }
    }
}

fn command_names_skill_file(command: &str, skill_file: &Path, workspace: &Path) -> bool {
    let Some(arguments) = shell_arguments(command) else {
        return false;
    };
    if arguments
        .iter()
        .any(|argument| resolved_path(&workspace.join(argument)) == resolved_path(skill_file))
    {
        return true;
    }
    let is_shell = arguments.first().is_some_and(|program| {
        matches!(
            Path::new(program)
                .file_name()
                .and_then(|name| name.to_str()),
            Some("sh" | "bash" | "zsh")
        )
    });
    if !is_shell {
        return false;
    }
    arguments.windows(2).any(|pair| {
        matches!(pair[0].as_str(), "-c" | "-lc")
            && shell_arguments(&pair[1]).is_some_and(|words| {
                words.iter().any(|argument| {
                    resolved_path(&workspace.join(argument)) == resolved_path(skill_file)
                })
            })
    })
}

fn shell_arguments(command: &str) -> Option<Vec<String>> {
    let mut separated = String::with_capacity(command.len());
    let mut quote = None;
    let mut escaped = false;
    for character in command.chars() {
        if escaped {
            separated.push(character);
            escaped = false;
            continue;
        }
        match character {
            '\\' if quote != Some('\'') => escaped = true,
            '\'' | '"' if quote == Some(character) => quote = None,
            '\'' | '"' if quote.is_none() => quote = Some(character),
            ';' | '|' | '&' if quote.is_none() => {
                separated.push(' ');
                separated.push(character);
                separated.push(' ');
                continue;
            }
            _ => {}
        }
        separated.push(character);
    }
    shlex::split(&separated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_read_preserves_quoted_and_escaped_paths_with_spaces() {
        let file = Path::new("/work/Application Support/.agents/skills/report/SKILL.md");
        let workspace = Path::new("/work/Application Support");
        let commands = [
            "cat '/work/Application Support/.agents/skills/report/SKILL.md'",
            "head -20 \"/work/Application Support/.agents/skills/report/SKILL.md\"",
            "cat /work/Application\\ Support/.agents/skills/report/SKILL.md",
            "/bin/zsh -lc 'cat \"/work/Application Support/.agents/skills/report/SKILL.md\"'",
            "cat '/work/Application Support/.agents/skills/report/SKILL.md'; echo done",
            "cat '/work/Application Support/.agents/skills/report/SKILL.md'|head -20",
            "/bin/zsh -lc 'cat \"/work/Application Support/.agents/skills/report/SKILL.md\"; echo done'",
        ];
        for command in commands {
            let mut evidence = LoadingEvidence::default();
            let line = serde_json::json!({"type":"item.completed","item":{
                "type":"command_execution", "command":command, "exit_code":0,
                "aggregated_output":"---\nname: report\n---"
            }});
            evidence.observe(HarnessId::Codex, &line.to_string(), file, workspace);
            assert_eq!(evidence.loaded, SkillLoaded::Yes, "{command}");
        }
    }

    #[test]
    fn quoted_shell_separators_remain_part_of_skill_paths() {
        let workspace = Path::new("/work");
        let file = Path::new("/work/a;b|c&d/report/SKILL.md");
        assert!(command_names_skill_file(
            "cat '/work/a;b|c&d/report/SKILL.md'; echo done",
            file,
            workspace
        ));
        assert!(!command_names_skill_file(
            "cat '/work/a;b|c&d/report/SKILL.md;other'",
            file,
            workspace
        ));
        assert!(!command_names_skill_file(
            "cat 'unterminated",
            file,
            workspace
        ));
    }

    #[test]
    fn a_requested_read_is_not_evidence_that_the_skill_was_loaded() {
        let mut evidence = LoadingEvidence::default();
        let file = Path::new("/work/.pi/skills/report/SKILL.md");
        let workspace = Path::new("/work");
        evidence.observe(HarnessId::Pi, r#"{"type":"tool_execution_start","toolName":"read","toolCallId":"call","args":{"path":"/work/.pi/skills/report/SKILL.md"}}"#, file, workspace);
        assert_eq!(evidence.loaded, SkillLoaded::Unknown);
        evidence.observe(
            HarnessId::Pi,
            r#"{"type":"tool_execution_end","toolCallId":"call","isError":true}"#,
            file,
            workspace,
        );
        assert_eq!(evidence.loaded, SkillLoaded::Unknown);
        evidence.observe(
            HarnessId::Pi,
            r#"{"type":"tool_execution_end","toolCallId":"call","isError":false}"#,
            file,
            workspace,
        );
        assert_eq!(evidence.loaded, SkillLoaded::Yes);
    }

    #[cfg(unix)]
    #[test]
    fn native_reads_through_a_workspace_alias_identify_the_same_frozen_skill() {
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("work");
        let skill = workspace.join(".pi/skills/report");
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(skill.join("SKILL.md"), "name: report").unwrap();
        let alias = root.path().join("alias");
        std::os::unix::fs::symlink(&workspace, &alias).unwrap();
        let expected = alias.join(".pi/skills/report/SKILL.md");
        let actual = skill.join("SKILL.md");
        let traces = [
            (
                HarnessId::Pi,
                vec![
                    serde_json::json!({"type":"tool_execution_start","toolName":"read","toolCallId":"call","args":{"path":actual}}),
                    serde_json::json!({"type":"tool_execution_end","toolCallId":"call","isError":false}),
                ],
            ),
            (
                HarnessId::ClaudeCode,
                vec![
                    serde_json::json!({"type":"user","message":{"content":[{"type":"text","text":format!("Base directory for this skill: {}\n\nInstructions",skill.display())}]}}),
                ],
            ),
            (
                HarnessId::OpenCode,
                vec![
                    serde_json::json!({"part":{"type":"tool","tool":"read","state":{"status":"completed","input":{"path":actual}}}}),
                ],
            ),
            (
                HarnessId::Codex,
                vec![
                    serde_json::json!({"type":"item.completed","item":{"type":"command_execution","command":format!("cat {}",actual.display()),"exit_code":0,"aggregated_output":"name: report"}}),
                ],
            ),
        ];
        for (client, lines) in traces {
            let mut evidence = LoadingEvidence::default();
            for line in lines {
                evidence.observe(client, &line.to_string(), &expected, &alias);
            }
            assert_eq!(evidence.loaded, SkillLoaded::Yes, "{client:?}");
        }
    }

    #[test]
    fn a_same_named_global_skill_does_not_prove_the_frozen_candidate_loaded() {
        let mut evidence = LoadingEvidence::default();
        let file = Path::new("/work/.pi/skills/report/SKILL.md");
        let workspace = Path::new("/work");
        evidence.observe(HarnessId::Pi, r#"{"type":"tool_execution_start","toolName":"read","toolCallId":"global","args":{"path":"/home/user/.pi/skills/report/SKILL.md"}}"#, file, workspace);
        evidence.observe(
            HarnessId::Pi,
            r#"{"type":"tool_execution_end","toolCallId":"global","isError":false}"#,
            file,
            workspace,
        );
        assert_eq!(evidence.loaded, SkillLoaded::Unknown);
        evidence.observe(HarnessId::Codex, r#"{"type":"item.completed","item":{"type":"command_execution","command":"cat /home/user/.pi/skills/report/SKILL.md","exit_code":0,"aggregated_output":"name: report"}}"#, file, workspace);
        assert_eq!(evidence.loaded, SkillLoaded::Unknown);
        evidence.observe(HarnessId::ClaudeCode, r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"skill","name":"Skill","input":{"skill":"report"}}]}}"#, file, workspace);
        evidence.observe(HarnessId::ClaudeCode, r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"skill","content":"Launching skill: report"}]}}"#, file, workspace);
        assert_eq!(evidence.loaded, SkillLoaded::Unknown);
    }
}
