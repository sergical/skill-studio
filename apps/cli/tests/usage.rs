// Integration test binaries aren't covered by the lib crate's
// `cfg_attr(test, allow(...))`: this file compiles as its own crate, so
// the same allow needs to be declared here too.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! `usage` counts each installed skill's uses from agent session history,
//! lists the never-used ones first, and ends with a summary line.

#[allow(dead_code)]
#[path = "support/test_home.rs"]
mod test_home;

use std::path::{Path, PathBuf};
use std::process::Output;

fn write_skill(dir: &Path, name: &str) {
    let skill_dir = dir.join(name);
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: A test skill.\n---\nBody.\n"),
    )
    .unwrap();
}

fn days_ago(days: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::days(days)).to_rfc3339()
}

/// One Claude Code transcript line where the model called the Skill tool.
fn claude_skill_line(skill: &str, at: &str) -> String {
    format!(
        r#"{{"type":"assistant","timestamp":"{at}","cwd":"/work","message":{{"content":[{{"type":"tool_use","name":"Skill","input":{{"skill":"{skill}"}}}}]}}}}"#
    )
}

/// A temp home where `gamma` was used 3 days ago and `idle` 45 days ago.
fn home() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().canonicalize().unwrap();
    write_skill(&home.join(".agents/skills"), "gamma");
    write_skill(&home.join(".claude/skills"), "idle");
    let transcripts = home.join(".claude/projects/-work");
    std::fs::create_dir_all(&transcripts).unwrap();
    let lines = [
        claude_skill_line("gamma", &days_ago(3)),
        claude_skill_line("idle", &days_ago(45)),
    ];
    std::fs::write(
        transcripts.join("session.jsonl"),
        format!("{}\n", lines.join("\n")),
    )
    .unwrap();
    (dir, home)
}

fn run(home: &Path, args: &[&str]) -> Output {
    test_home::command(env!("CARGO_BIN_EXE_skill-studio"), home)
        .arg("usage")
        .args(args)
        .arg("--home")
        .arg(home)
        .env("HOME", home)
        .env("SKILL_STUDIO_TELEMETRY", "0")
        .output()
        .expect("run skill-studio usage")
}

/// Flow: `usage --json` over a home with one recent and one old use.
/// Expectation: `idle` first and in `unused`; `gamma` with one recent use
/// by Claude Code.
/// A failure means the report counts uses wrong or hides unused skills.
#[test]
fn usage_json_lists_unused_skills_first_with_counts_per_agent() {
    let (_dir, home) = home();
    let output = run(&home, &["--json"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["operation"], "skill_usage", "{json}");
    let data = &json["data"];
    assert_eq!(data["days"], 30);
    assert_eq!(data["unused"], serde_json::json!(["idle"]), "{data}");
    let rows = data["rows"].as_array().unwrap();
    assert_eq!(rows[0]["skill"], "idle", "{data}");
    assert_eq!(rows[0]["recent_uses"], 0);
    assert_eq!(rows[0]["total_uses"], 1);
    assert_eq!(rows[1]["skill"], "gamma", "{data}");
    assert_eq!(rows[1]["recent_uses"], 1);
    assert!(rows.iter().all(|row| row.get("sessions").is_none()));
    assert_eq!(rows[1]["agents"], serde_json::json!({"claude-code": 1}));
}

/// Flow: `usage` as text, with the default window and with `--days 60`.
/// Expectation: the unused skill comes before the used one, and the
/// summary line counts unused skills for the window asked for.
/// A failure means the text report and the window flag disagree.
#[test]
fn usage_text_ends_with_a_summary_for_the_window() {
    let (_dir, home) = home();
    let stdout = String::from_utf8(run(&home, &[]).stdout).unwrap();
    assert!(!stdout.contains("sessions"), "{stdout}");
    let idle = stdout.find("idle").unwrap_or_else(|| panic!("{stdout}"));
    let gamma = stdout.find("gamma").unwrap_or_else(|| panic!("{stdout}"));
    assert!(idle < gamma, "unused skills should come first:\n{stdout}");
    assert!(
        stdout.contains("1 of 2 skills not used in 30 days"),
        "{stdout}"
    );

    let stdout = String::from_utf8(run(&home, &["--days", "60"]).stdout).unwrap();
    assert!(
        stdout.contains("0 of 2 skills not used in 60 days"),
        "{stdout}"
    );
}

/// Flow: CLI usage selects a harness and project from two recorded projects.
/// Expect only matching uses and a complete report. Failure: filters are ignored.
#[test]
fn usage_filters_history_by_harness_and_project() {
    let (_dir, home) = home();
    let project = home.join("work");
    std::fs::create_dir_all(&project).unwrap();
    let matching = claude_skill_line("gamma", "2026-09-29T12:00:00Z")
        .replace("/work", project.to_str().unwrap());
    std::fs::write(
        home.join(".claude/projects/-work/other.jsonl"),
        format!("{matching}\n"),
    )
    .unwrap();
    let output = run(
        &home,
        &[
            "--json",
            "--harness",
            "claude-code",
            "--project",
            project.to_str().unwrap(),
        ],
    );
    assert!(
        !output.stdout.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let gamma = json["data"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["skill"] == "gamma")
        .unwrap();
    assert_eq!(gamma["total_uses"], 1, "{json}");
    let output = run(&home, &["--json", "--harness", "codex"]);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        json["data"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["total_uses"] == 0),
        "{json}"
    );
    let output = run(&home, &[]);
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        !text.contains("Partial report") && output.status.success(),
        "{text}"
    );
}

#[test]
fn usage_cached_run_keeps_cache_file_instead_of_rewriting_unchanged_history() {
    let (_dir, home) = home();
    let run_cached = || {
        test_home::command(env!("CARGO_BIN_EXE_skill-studio"), &home)
            .args(["usage", "--json"])
            .output()
            .unwrap()
    };
    assert!(run_cached().status.success());
    fn find_cache(dir: &Path) -> Option<PathBuf> {
        for entry in std::fs::read_dir(dir).ok()?.flatten() {
            let path = entry.path();
            if path.file_name().unwrap() == "cli-skill-uses.json" {
                return Some(path);
            }
            if path.is_dir() {
                if let Some(found) = find_cache(&path) {
                    return Some(found);
                }
            }
        }
        None
    }
    let cache = find_cache(&home).expect("CLI cache in temporary home");
    let old_time = std::fs::FileTimes::new().set_modified(std::time::SystemTime::UNIX_EPOCH);
    std::fs::File::options()
        .write(true)
        .open(&cache)
        .unwrap()
        .set_times(old_time)
        .unwrap();
    assert!(run_cached().status.success());
    assert_eq!(
        std::fs::metadata(&cache).unwrap().modified().unwrap(),
        std::time::SystemTime::UNIX_EPOCH
    );
}

/// Flow: queries read a chmod-protected home. Expect answers and unchanged bytes.
/// Failure means a read path still creates a lease or cache.
#[cfg(unix)]
#[test]
fn readonly_queries_answer_without_writes_or_fail_on_chmod_home() {
    use std::os::unix::fs::PermissionsExt;
    fn tree(path: &Path, entries: &mut Vec<(PathBuf, Vec<u8>)>) {
        entries.push((
            path.to_path_buf(),
            if path.is_file() {
                std::fs::read(path).unwrap()
            } else {
                Vec::new()
            },
        ));
        if path.is_dir() {
            let mut children: Vec<_> = std::fs::read_dir(path)
                .unwrap()
                .map(|e| e.unwrap().path())
                .collect();
            children.sort();
            for child in children {
                tree(&child, entries);
            }
        }
    }
    let (_dir, home) = home();
    let warm = test_home::command(env!("CARGO_BIN_EXE_skill-studio"), &home)
        .args(["usage", "--json"])
        .output()
        .unwrap();
    assert!(warm.status.success());
    std::fs::write(
        home.join(".claude/projects/-work/session.jsonl"),
        claude_skill_line("gamma", &days_ago(1)),
    )
    .unwrap();
    let mut before = Vec::new();
    tree(&home, &mut before);
    for (path, _) in before.iter().rev() {
        std::fs::set_permissions(
            path,
            std::fs::Permissions::from_mode(if path.is_dir() { 0o555 } else { 0o444 }),
        )
        .unwrap();
    }
    let mut outputs: Vec<_> = ["scan", "usage", "invocation"]
        .iter()
        .map(|command| {
            test_home::command(env!("CARGO_BIN_EXE_skill-studio"), &home)
                .args([command, "--json", "--home"])
                .arg(&home)
                .output()
                .unwrap()
        })
        .collect();
    outputs.push(
        test_home::command(env!("CARGO_BIN_EXE_skill-studio"), &home)
            .args(["usage", "--json"])
            .output()
            .unwrap(),
    );
    let mut after = Vec::new();
    tree(&home, &mut after);
    for (path, _) in &before {
        std::fs::set_permissions(
            path,
            std::fs::Permissions::from_mode(if path.is_dir() { 0o755 } else { 0o644 }),
        )
        .unwrap();
    }
    for output in outputs {
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
    assert_eq!(before, after);
}

/// Flow: usage with --home and a foreign `CODEX_HOME`. Expect only selected-home uses.
/// Failure means the usage reader re-resolves an ambient home.
#[test]
fn explicit_home_usage_ignores_codex_home_or_counts_foreign_sessions() {
    let (_dir, home) = home();
    let foreign = tempfile::tempdir().unwrap();
    std::fs::create_dir(foreign.path().join("sessions")).unwrap();
    let line = serde_json::json!({"type":"response_item", "timestamp":days_ago(1), "payload":{"type":"function_call", "namespace":"skills", "name":"read", "arguments":"{\"package\":\"skill://root/gamma\"}"}});
    std::fs::write(
        foreign.path().join("sessions/foreign.jsonl"),
        format!("{line}\n{line}\n"),
    )
    .unwrap();
    std::fs::create_dir_all(home.join(".codex/sessions")).unwrap();
    std::fs::write(
        home.join(".codex/sessions/local.jsonl"),
        format!("{line}\n"),
    )
    .unwrap();
    let output = test_home::command(env!("CARGO_BIN_EXE_skill-studio"), &home)
        .args(["usage", "--json", "--harness", "codex", "--home"])
        .arg(&home)
        .env("CODEX_HOME", foreign.path())
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        json["data"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["total_uses"] == u64::from(r["skill"] == "gamma")),
        "{json}"
    );
}

#[test]
fn review_due_selected_home_counts_local_sessions_instead_of_ambient_codex_sessions() {
    let (_dir, home) = home();
    let foreign = tempfile::tempdir().unwrap();
    std::fs::create_dir(foreign.path().join("sessions")).unwrap();
    let line = serde_json::json!({"type":"response_item", "timestamp":days_ago(1), "payload":{"type":"function_call", "namespace":"skills", "name":"read", "arguments":"{\"package\":\"skill://root/gamma\"}"}});
    std::fs::write(
        foreign.path().join("sessions/foreign.jsonl"),
        format!(
            "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"foreign\"}}}}\n{line}\n{line}\n"
        ),
    )
    .unwrap();
    std::fs::write(
        foreign.path().join("sessions/another-foreign.jsonl"),
        format!("{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"foreign-two\"}}}}\n{line}\n"),
    )
    .unwrap();
    std::fs::create_dir_all(home.join(".codex/sessions")).unwrap();
    std::fs::write(
        home.join(".codex/sessions/local.jsonl"),
        format!("{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"local\"}}}}\n{line}\n"),
    )
    .unwrap();
    let set = test_home::command(env!("CARGO_BIN_EXE_skill-studio"), &home)
        .args(["review", "set", "gamma", "2000-01-01", "--home"])
        .arg(&home)
        .output()
        .unwrap();
    assert!(set.status.success());
    let due = |codex_home: Option<&Path>| {
        let mut command = test_home::command(env!("CARGO_BIN_EXE_skill-studio"), &home);
        command
            .args(["review", "due", "--json", "--home"])
            .arg(&home);
        if let Some(codex_home) = codex_home {
            command.env("CODEX_HOME", codex_home);
        }
        let output = command.output().unwrap();
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(json["data"]["rows"].as_array().unwrap().len(), 1, "{json}");
        json["data"]["rows"][0]["sessions"].as_u64().unwrap()
    };
    let local = due(None);
    assert!(local > 0, "the local Codex session must count");
    assert_eq!(
        due(Some(foreign.path())),
        local,
        "an ambient CODEX_HOME added foreign sessions to the selected home"
    );
}
