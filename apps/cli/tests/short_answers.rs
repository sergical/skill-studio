#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Short answers: one call gives an agent the answer, not the raw data to
//! filter. The CLI and the MCP server must return the same answer.

#[path = "support/headless.rs"]
mod headless;

use serde_json::json;
use std::path::Path;

fn write_skill(dir: &Path, name: &str, body: &str) {
    let skill = dir.join(name);
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: A pack skill.\n---\n{body}\n"),
    )
    .unwrap();
}

/// Flow: `invocation --mismatches` and the MCP `invocation_mismatches` tool
/// on a home where Claude Code reads a skill as manual and Codex as auto,
/// and a second skill is auto everywhere.
/// Expectation: both list only the first skill, with the agents on each side.
/// Failure means an agent must read every copy's mode to find one mismatch.
#[test]
fn invocation_mismatches_list_only_the_skills_that_disagree() {
    let home = tempfile::tempdir().unwrap();
    for root in [".claude/skills", ".codex/skills"] {
        write_skill(&home.path().join(root), "same", "Body.");
        write_skill(&home.path().join(root), "split", "Body.");
    }
    std::fs::write(
        home.path().join(".claude/skills/split/SKILL.md"),
        "---\nname: split\ndescription: A pack skill.\ndisable-model-invocation: true\n---\nBody.\n",
    )
    .unwrap();

    let cli = headless::cli(home.path(), &["invocation", "--mismatches"]);
    let mcp = headless::mcp(home.path(), "invocation_mismatches", &json!({}));

    assert_eq!(
        cli["data"], mcp["result"]["structuredContent"]["data"],
        "cli: {cli}\nmcp: {mcp}"
    );
    assert_eq!(
        cli["data"],
        json!({"skills": [{"skill": "split", "manual": ["claude-code"], "auto": ["codex"]}]}),
    );
}

fn claude_skill_line(skill: &str, at: &str) -> String {
    format!(
        r#"{{"type":"assistant","sessionId":"{at}","timestamp":"{at}","cwd":"/work","message":{{"content":[{{"type":"tool_use","name":"Skill","input":{{"skill":"{skill}"}}}}]}}}}"#
    )
}

/// Flow: `usage --since --until --top` and the MCP `skill_usage` tool with
/// the same `since`, `until` and `top`, on a home with uses in September
/// and October.
/// Expectation: both count only the September uses, keep the one most used
/// skill, and echo the window.
/// Failure means "my top skill last month" needs a date filter by hand, or
/// the two surfaces disagree on the window.
#[test]
fn usage_window_and_top_match_on_the_cli_and_mcp() {
    let home = tempfile::tempdir().unwrap();
    for name in ["often", "once", "later"] {
        write_skill(&home.path().join(".claude/skills"), name, "Body.");
    }
    let transcripts = home.path().join(".claude/projects/-work");
    std::fs::create_dir_all(&transcripts).unwrap();
    let lines = [
        claude_skill_line("often", "2026-09-03T10:00:00Z"),
        claude_skill_line("often", "2026-09-20T10:00:00Z"),
        claude_skill_line("once", "2026-09-10T10:00:00Z"),
        claude_skill_line("later", "2026-10-02T10:00:00Z"),
        claude_skill_line("later", "2026-10-03T10:00:00Z"),
        claude_skill_line("later", "2026-10-04T10:00:00Z"),
    ];
    std::fs::write(
        transcripts.join("session.jsonl"),
        format!("{}\n", lines.join("\n")),
    )
    .unwrap();

    let cli = headless::cli(
        home.path(),
        &[
            "usage",
            "--since",
            "2026-09-01",
            "--until",
            "2026-10-01",
            "--top",
            "1",
        ],
    );
    let mcp = headless::mcp(
        home.path(),
        "skill_usage",
        &json!({"since": "2026-09-01", "until": "2026-10-01", "top": 1}),
    );

    assert_eq!(
        cli["data"], mcp["result"]["structuredContent"]["data"],
        "cli: {cli}\nmcp: {mcp}"
    );
    let data = &cli["data"];
    assert_eq!(data["since"], "2026-09-01T00:00:00+00:00", "{cli}");
    assert_eq!(data["until"], "2026-10-01T00:00:00+00:00", "{cli}");
    let rows = data["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{cli}");
    assert_eq!(rows[0]["skill"], "often");
    assert_eq!(rows[0]["recent_uses"], 2);
}

/// Flow: `usage` and the MCP `skill_usage` tool with `since` after `until`,
/// and with `since` in the future.
/// Expectation: both refuse with `invalid_request` and no rows.
/// Failure means an empty window reports every skill as unused.
#[test]
fn an_empty_usage_window_is_refused_on_the_cli_and_mcp() {
    let home = tempfile::tempdir().unwrap();
    write_skill(&home.path().join(".claude/skills"), "often", "Body.");

    for (since, until) in [("2026-11-01", Some("2026-10-01")), ("2999-01-01", None)] {
        let mut args = vec!["usage", "--since", since];
        let mut params = json!({"since": since});
        if let Some(until) = until {
            args.extend(["--until", until]);
            params["until"] = json!(until);
        }
        let cli = headless::cli(home.path(), &args);
        let mcp = headless::mcp(home.path(), "skill_usage", &params);
        let mcp = &mcp["result"]["structuredContent"];

        for (surface, envelope) in [("cli", &cli), ("mcp", mcp)] {
            assert_eq!(
                envelope["errors"][0]["code"], "invalid_request",
                "{surface} {since}..{until:?}: {envelope}"
            );
            assert!(envelope["data"].is_null(), "{surface}: {envelope}");
        }
    }
}
