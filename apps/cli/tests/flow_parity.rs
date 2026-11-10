#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/headless.rs"]
mod headless;

use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Clone, Copy)]
enum Surface {
    Cli,
    Mcp,
}

struct Fixture {
    home: tempfile::TempDir,
    surface: Surface,
}

impl Fixture {
    fn new(surface: Surface) -> Self {
        Self {
            home: tempfile::tempdir().unwrap(),
            surface,
        }
    }
    fn md(name: &str, body: &str) -> String {
        format!("---\nname: {name}\ndescription: a parity fixture\n---\n{body}\n")
    }
    fn source(&self, name: &str, body: &str) -> PathBuf {
        let dir = self.home.path().join("sources").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), Self::md(name, body)).unwrap();
        dir
    }
    fn call(&self, args: &[&str], tool: &str, operation: &str, input: &Value) -> Value {
        let env = [("PATH".to_string(), "/usr/bin:/bin".to_string())];
        let result = match self.surface {
            Surface::Cli if operation == "scan" => headless::cli(self.home.path(), args),
            Surface::Cli => headless::cli_with_env(self.home.path(), args, &env),
            Surface::Mcp => {
                let rpc = if operation == "scan" {
                    headless::mcp(self.home.path(), tool, input)
                } else {
                    headless::mcp_with_env(self.home.path(), tool, input, &env)
                };
                assert!(rpc["error"].is_null(), "{tool}: {rpc}");
                assert!(
                    rpc["result"]["structuredContent"].is_object(),
                    "{tool}: {rpc}"
                );
                rpc["result"]["structuredContent"].clone()
            }
        };
        assert!(
            matches!(result["status"].as_str(), Some("ok" | "issues_found")),
            "{operation}: {result}"
        );
        assert_eq!(result["operation"], operation, "{result}");
        assert!(result["errors"].as_array().unwrap().is_empty(), "{result}");
        assert!(!result["data"].is_null(), "{result}");
        result
    }
    fn install(&self, name: &str) -> Value {
        let source = self.source(name, "Original");
        self.call(&["add", source.to_str().unwrap(), "--method", "copy", "--name", name], "add", "install",
            &json!({"skill":name,"method":"copy","scope":{"scope":"global"},"files":[{"relative_path":"SKILL.md","contents":Self::md(name,"Original").as_bytes()}]}))
    }
    fn path(&self, name: &str) -> PathBuf {
        self.home
            .path()
            .join(".agents/skills")
            .join(name)
            .join("SKILL.md")
    }
    fn deployment(&self, kind: &str) -> String {
        let scan = self.call(&["scan"], "scan", "scan", &json!({}));
        scan["data"]["skills"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == "gamma")
            .unwrap()["deployments"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["root"]["kind"]["kind"] == kind)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    }
    fn park(&self) {
        let id = self.deployment("universal");
        self.call(
            &["park", "--id", &id],
            "park",
            "park",
            &json!({"deployment_id":id}),
        );
        assert!(!self.path("gamma").exists());
    }
}

fn exercise(flow: &str, surface: Surface) {
    let f = Fixture::new(surface);
    let installed = f.install("gamma");
    assert_eq!(
        std::fs::read_to_string(f.path("gamma")).unwrap(),
        Fixture::md("gamma", "Original")
    );
    match flow {
        "Install" => {
            assert_eq!(installed["data"]["skill"], "gamma");
        }
        "First launch harness detect" => {
            let result = f.call(&["harnesses"], "harnesses", "harnesses", &json!({}));
            assert!(result["data"]["harnesses"]
                .as_array()
                .unwrap()
                .iter()
                .any(|h| h["id"] == "codex"));
        }
        "Scan" => {
            assert!(!f.deployment("universal").is_empty());
        }
        "Activity" => {
            let result = f.call(&["events"], "list_events", "list_events", &json!({}));
            assert!(result["data"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["kind"] == "install"));
        }
        "Outdated check" => {
            let result = f.call(
                &["outdated", "--skill", "gamma"],
                "outdated",
                "outdated",
                &json!({"skills":["gamma"]}),
            );
            assert!(result["data"].get("gamma").is_some(), "{result}");
        }
        "Update one" | "Update all" => {
            let mut skills = vec!["gamma"];
            if flow == "Update all" {
                f.install("delta");
                skills.push("delta");
            }
            let source = f.source("gamma", "Updated");
            let mut args = vec![
                "update",
                "--method",
                "copy",
                "--source-dir",
                source.to_str().unwrap(),
            ];
            for name in &skills {
                args.extend(["--skill", name]);
            }
            let requests:Vec<_>=skills.iter().map(|name|json!({"skill":name,"method":"copy","scope":{"scope":"global"},"files":[{"relative_path":"SKILL.md","contents":Fixture::md("gamma","Updated").as_bytes()}]})).collect();
            if flow == "Update all" {
                f.call(
                    &args,
                    "update_all",
                    "update_all",
                    &json!({"requests":requests}),
                );
            } else {
                f.call(&args, "update", "update", &requests[0]);
            }
            for name in skills {
                assert!(std::fs::read_to_string(f.path(name))
                    .unwrap()
                    .contains("Updated"));
            }
        }
        "Park / unpark" | "Park for every agent" => {
            f.park();
            let id = f.deployment("parked");
            f.call(
                &["unpark", "--id", &id],
                "unpark",
                "unpark",
                &json!({"deployment_id":id}),
            );
            assert_eq!(
                std::fs::read_to_string(f.path("gamma")).unwrap(),
                Fixture::md("gamma", "Original")
            );
        }
        "Turn off for one agent" => {
            let id = f.deployment("universal");
            let check = f.call(
                &["turn-off", "--id", &id, "--agent", "codex", "--check"],
                "turn_off_check",
                "turn_off_for_agent",
                &json!({"deployment_id":id,"agent":"codex"}),
            );
            assert!(check["data"]["refusal"].is_null(), "{check}");
            let result = f.call(
                &["turn-off", "--id", &id, "--agent", "codex", "--confirm"],
                "turn_off_for_agent",
                "turn_off_for_agent",
                &json!({"deployment_id":id,"agent":"codex","confirmed":true}),
            );
            assert_eq!(result["data"]["agent"], "codex");
            assert!(!f.path("gamma").exists());
            assert!(
                PathBuf::from(result["data"]["parked_path"].as_str().unwrap())
                    .join("SKILL.md")
                    .exists()
            );
            assert!(f.home.path().join(".cursor/skills/gamma/SKILL.md").exists());
        }
        "Split" => {
            let id = f.deployment("universal");
            f.call(
                &[
                    "split",
                    "--deployment-id",
                    &id,
                    "--harness",
                    "claude-code",
                    "--harness",
                    "codex",
                ],
                "split",
                "split",
                &json!({"deployment_id":id,"harnesses":["claude-code","codex"]}),
            );
            assert!(!f.path("gamma").exists());
            for root in [".claude/skills", ".codex/skills"] {
                assert!(f.home.path().join(root).join("gamma/SKILL.md").exists());
            }
        }
        "Remove + undo" | "Undo from Activity" => {
            let id = f.deployment("universal");
            let removed = f.call(
                &["remove", "--id", &id],
                "remove",
                "remove",
                &json!({"deployment_id":id}),
            );
            assert!(!f.path("gamma").exists());
            let event = removed["event_id"].as_str().unwrap();
            f.call(
                &["restore", "--event-id", event],
                "restore_event",
                "restore_event",
                &json!({"event_id":event,"force":false}),
            );
            assert_eq!(
                std::fs::read_to_string(f.path("gamma")).unwrap(),
                Fixture::md("gamma", "Original")
            );
        }
        "Fix" | "Frontmatter repair" => {
            std::fs::write(f.home.path().join(".agents/skill-studio.json"), "{}").unwrap();
            let malformed = "---\nname: gamma\ndescription: Use this: when broken\n---\nBody\n";
            std::fs::write(f.path("gamma"), malformed).unwrap();
            if flow == "Fix" {
                f.call(
                    &["fix", "--skill", "gamma"],
                    "fix",
                    "fix_skill",
                    &json!({"skill":"gamma"}),
                );
            } else {
                let id = f.deployment("universal");
                let preview = f.call(
                    &["preview-repair", "--deployment-id", &id],
                    "preview_frontmatter_repair",
                    "preview_frontmatter_repair",
                    &json!({"deployment_id":id}),
                );
                let file = f.home.path().join("preview.json");
                std::fs::write(&file, serde_json::to_vec(&preview["data"]).unwrap()).unwrap();
                f.call(
                    &["apply-repair", "--preview-json", file.to_str().unwrap()],
                    "apply_frontmatter_repair",
                    "apply_frontmatter_repair",
                    &json!({"preview":preview["data"],"mode":"apply-fix"}),
                );
            }
            let fixed = std::fs::read_to_string(f.path("gamma")).unwrap();
            assert_ne!(fixed, malformed);
            assert!(!fixed.contains("description: Use this:"));
        }
        _ => panic!("flow has no executable scenario: {flow}"),
    }
}

struct Coverage {
    flow: &'static str,
    cli: &'static str,
    mcp: &'static str,
    cli_test: fn(),
    mcp_test: fn(),
}

macro_rules! executable_coverage {
    ($($flow:literal => ($cli:ident, $mcp:ident)),* $(,)?) => {
        $(
            #[test]
            fn $cli() { exercise($flow, Surface::Cli); }
            #[test]
            fn $mcp() { exercise($flow, Surface::Mcp); }
        )*
        const COVERAGE: &[Coverage] = &[$(
            Coverage { flow: $flow, cli: stringify!($cli), mcp: stringify!($mcp), cli_test: $cli, mcp_test: $mcp },
        )*];
    };
}

executable_coverage! {
    "First launch harness detect" => (cli_harness_detection_reports_codex_or_names_the_missing_agent, mcp_harness_detection_reports_codex_or_names_the_missing_agent),
    "Scan" => (cli_scan_reports_shared_gamma_or_names_the_missing_deployment, mcp_scan_reports_shared_gamma_or_names_the_missing_deployment),
    "Activity" => (cli_activity_contains_install_or_names_the_missing_event, mcp_activity_contains_install_or_names_the_missing_event),
    "Outdated check" => (cli_outdated_reports_gamma_or_names_the_missing_result, mcp_outdated_reports_gamma_or_names_the_missing_result),
    "Install" => (cli_install_writes_gamma_or_names_the_missing_bytes, mcp_install_writes_gamma_or_names_the_missing_bytes),
    "Update one" => (cli_update_replaces_gamma_bytes_or_names_the_stale_copy, mcp_update_replaces_gamma_bytes_or_names_the_stale_copy),
    "Update all" => (cli_update_all_replaces_each_copy_or_names_the_stale_skill, mcp_update_all_replaces_each_copy_or_names_the_stale_skill),
    "Park / unpark" => (cli_park_unpark_moves_and_restores_gamma_or_names_the_wrong_path, mcp_park_unpark_moves_and_restores_gamma_or_names_the_wrong_path),
    "Park for every agent" => (cli_park_all_removes_and_restores_shared_bytes_or_names_the_wrong_path, mcp_park_all_removes_and_restores_shared_bytes_or_names_the_wrong_path),
    "Turn off for one agent" => (cli_agent_off_parks_codex_and_keeps_cursor_or_names_the_wrong_agent, mcp_agent_off_parks_codex_and_keeps_cursor_or_names_the_wrong_agent),
    "Split" => (cli_split_writes_claude_and_codex_copies_or_names_the_missing_copy, mcp_split_writes_claude_and_codex_copies_or_names_the_missing_copy),
    "Remove + undo" => (cli_remove_restore_recovers_gamma_bytes_or_names_the_missing_restore, mcp_remove_restore_recovers_gamma_bytes_or_names_the_missing_restore),
    "Fix" => (cli_fix_repairs_yaml_or_names_the_still_broken_header, mcp_fix_repairs_yaml_or_names_the_still_broken_header),
    "Undo from Activity" => (cli_activity_undo_restores_gamma_or_names_the_missing_bytes, mcp_activity_undo_restores_gamma_or_names_the_missing_bytes),
    "Frontmatter repair" => (cli_repair_applies_preview_or_names_the_unchanged_header, mcp_repair_applies_preview_or_names_the_unchanged_header),
}

#[test]
fn every_desktop_flow_has_named_executable_cli_and_mcp_coverage_or_an_explicit_gap() {
    let docs = include_str!("../../../docs/flows.md");
    let mut seen = std::collections::BTreeSet::new();
    for row in docs
        .split("## Timing")
        .next()
        .unwrap()
        .lines()
        .filter(|line| line.starts_with("| "))
        .skip(2)
    {
        let columns: Vec<_> = row.split('|').map(str::trim).collect();
        assert_eq!(
            columns.len(),
            11,
            "flow needs named executable coverage: {}",
            columns[1]
        );
        if columns[7].starts_with("Gap: ") {
            assert!(columns[7].len() > 35);
            assert_eq!(columns[5], "Unavailable");
            assert_eq!(columns[6], "Unavailable");
            assert_eq!(columns[8], "Unavailable");
            assert_eq!(columns[9], "Unavailable");
            continue;
        }
        let coverage = COVERAGE
            .iter()
            .find(|case| case.flow == columns[1])
            .unwrap_or_else(|| panic!("no executable scenarios for {}", columns[1]));
        assert_eq!(columns[8], coverage.cli);
        assert_eq!(columns[9], coverage.mcp);
        (coverage.cli_test)();
        (coverage.mcp_test)();
        assert!(seen.insert(coverage.flow), "duplicate flow");
    }
    assert_eq!(
        seen.len(),
        COVERAGE.len(),
        "an executable flow disappeared from the docs"
    );
}
