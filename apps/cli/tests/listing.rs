#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use headless::test_home;

#[allow(dead_code)]
#[path = "support/headless.rs"]
mod headless;

fn write(home: &std::path::Path, path: &str, text: &str) {
    let path = home.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn skill(home: &std::path::Path, root: &str, name: &str, description: &str) {
    write(
        home,
        &format!("{root}/{name}/SKILL.md"),
        &format!("---\nname: {name}\ndescription: {description}\n---\n"),
    );
}

fn listing(home: &std::path::Path, args: &[&str]) -> serde_json::Value {
    let mut cmd = test_home::command(env!("CARGO_BIN_EXE_skill-studio"), home);
    cmd.args(["listing", "--fixture", home.to_str().unwrap(), "--json"])
        .args(args)
        .env("HOME", home)
        .env("CODEX_HOME", home.join(".codex"))
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("SKILL_STUDIO_TELEMETRY", "0");
    let out = cmd.output().unwrap();
    let envelope = serde_json::from_slice::<serde_json::Value>(&out.stdout)
        .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&out.stderr)));
    envelope["data"].clone()
}

fn row(data: &serde_json::Value, harness: &str) -> serde_json::Value {
    data["listings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["harness"] == harness && r["scope"]["scope"] == "global")
        .unwrap()
        .clone()
}

fn state(row: &serde_json::Value, name: &str) -> String {
    row["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == name)
        .unwrap_or_else(|| panic!("{name} missing from {row}"))["state"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn claude_listing_keeps_descriptions_of_used_skills_at_the_catalog_window_instead_of_a_200k_default(
) {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let text = "x".repeat(100);
    for name in ["a", "b", "c"] {
        skill(home, ".claude/skills", name, &text);
    }
    write(
        home,
        ".claude/cache/model-catalog/cc.json",
        r#"{"fetchedAt":1,"catalog":{"config":{"models":[{"id":"claude-opus-5-5","runtime":{"max_input_tokens":1000000}}],"provider_alias_targets":{"opus":{"default":"claude-opus-5-5"}}}}}"#,
    );
    // 1000000 tokens x 3 characters x 0.00006 = 180 characters.
    write(
        home,
        ".claude/settings.json",
        r#"{"model":"opus","skillListingBudgetFraction":0.00006}"#,
    );
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    write(
        home,
        ".claude.json",
        &format!(r#"{{"skillUsage":{{"b":{{"usageCount":5,"lastUsedAt":{now}}}}}}}"#),
    );
    // A recorded session: the bundled `dataviz` keeps its description.
    let content = format!("- a\\n- b: {text}\\n- c\\n- dataviz: {}", "y".repeat(50));
    write(
        home,
        ".claude/projects/-work/s.jsonl",
        &format!(
            "{}\n{}\n",
            format_args!(
                r#"{{"type":"attachment","timestamp":"2026-11-19T00:00:00Z","attachment":{{"type":"skill_listing","isInitial":true,"names":["a","b","c","dataviz"],"content":"{content}"}}}}"#
            ),
            r#"{"type":"assistant","message":{"model":"claude-opus-5-5"}}"#
        ),
    );
    write(
        home,
        ".local/share/chezmoi/private_dot_claude/private_settings.json.tmpl",
        "{}",
    );

    let data = listing(home, &[]);
    let claude = row(&data, "claude-code");
    assert_eq!(claude["budget"], 180);
    assert_eq!(claude["context_window_tokens"], 1_000_000);
    assert_eq!(claude["uses_default_context_window"], false);
    assert_eq!(state(&claude, "b"), "described");
    assert_eq!(state(&claude, "a"), "name_only");
    assert_eq!(state(&claude, "c"), "name_only");
    let dataviz = claude["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "dataviz")
        .unwrap();
    assert_eq!(dataviz["scanned"], false);
    assert_eq!(dataviz["state"], "described");
    assert_eq!(
        claude["entries"][1]["observed"], "described",
        "b, listed second, had a description in the session"
    );
    assert_eq!(claude["observed_session"]["model"], "claude-opus-5-5");
    assert!(claude["setting"]["chezmoi_source"]
        .as_str()
        .unwrap()
        .ends_with("private_dot_claude/private_settings.json.tmpl"));

    let wider = row(&listing(home, &["--claude-fraction", "1"]), "claude-code");
    assert_eq!(state(&wider, "a"), "described");
    assert_eq!(
        wider["setting"]["snippet"],
        "\"skillListingBudgetFraction\": 1"
    );

    let mcp = headless::mcp_with_env(
        home,
        "listing",
        &serde_json::json!({}),
        &[(
            "SKILL_STUDIO_FIXTURE".to_string(),
            home.to_str().unwrap().to_string(),
        )],
    );
    assert_eq!(data, mcp["result"]["structuredContent"]["data"]);
}

#[test]
fn codex_listing_shortens_descriptions_then_omits_entries_instead_of_reporting_only_a_total() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    for name in ["x", "y"] {
        skill(home, ".codex/skills", name, &"d".repeat(400));
    }
    write(home, ".codex/config.toml", "model = \"m\"\n");
    write(
        home,
        ".codex/models_cache.json",
        r#"{"models":[{"slug":"m","context_window":10000}]}"#,
    );

    let codex = row(&listing(home, &[]), "codex");
    assert_eq!(codex["budget"], 200);
    assert_eq!(codex["budget_unit"], "tokens");
    assert_eq!(state(&codex, "x"), "truncated");
    assert_eq!(state(&codex, "y"), "truncated");

    let tight = row(&listing(home, &["--codex-max-tokens", "1"]), "codex");
    assert_eq!(state(&tight, "x"), "omitted");
    assert_eq!(
        tight["setting"]["snippet"],
        "[skills]\nmax_context_tokens = 1"
    );
}
