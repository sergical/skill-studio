#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::process::Command;
#[allow(dead_code)]
#[path = "support/headless.rs"]
mod headless;

#[test]
fn doctor_listing_cost_warns_and_names_top_skills_or_counts_manual_and_parked_skills() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    for (root, name, policy) in [
        (".claude/skills", "large", ""),
        (
            ".claude/skills",
            "manual",
            "disable-model-invocation: true\n",
        ),
        (".agents/skills-parked", "parked", ""),
        (
            ".pi/agent/skills",
            "pi-manual",
            "disable-model-invocation: true\n",
        ),
        (
            ".cursor/skills",
            "cursor-manual",
            "disable-model-invocation: true\n",
        ),
        (".codex/skills", "codex-manual", ""),
        (".codex/skills", "codex-listed", ""),
        (
            ".config/opencode/skills",
            "opencode-manual",
            "metadata:\n  opencode/autoinvoke: false\n",
        ),
    ] {
        let path = home.join(root).join(name);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            path.join("SKILL.md"),
            format!(
                "---\nname: {name}\ndescription: {}\n{policy}---\n",
                "x".repeat(1000)
            ),
        )
        .unwrap();
        if name == "codex-manual" {
            std::fs::create_dir_all(path.join("agents")).unwrap();
            std::fs::write(
                path.join("agents/openai.yaml"),
                "policy:\n  allow_implicit_invocation: false\n",
            )
            .unwrap();
        }
    }
    let run = || {
        let out = Command::new(env!("CARGO_BIN_EXE_skill-studio"))
            .args(["doctor", "--fixture", home.to_str().unwrap(), "--json"])
            .env("HOME", home)
            .env("CODEX_HOME", home.join(".codex"))
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("SKILL_STUDIO_TELEMETRY", "0")
            .output()
            .unwrap();
        serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["data"].clone()
    };
    std::fs::write(
        home.join(".claude/settings.json"),
        r#"{"skillListingBudgetFraction":0.0001}"#,
    )
    .unwrap();
    let report = run();
    let rows = report["listing_cost"]
        .as_array()
        .expect("doctor must report listing cost");
    let claude = rows.iter().find(|r| r["harness"] == "claude-code").unwrap();
    assert_eq!(claude["listed_skills"], 1);
    assert_eq!(claude["limit_tokens"], 20);
    assert_eq!(claude["top_skills"][0]["name"], "large");
    assert_eq!(claude["warning"], "over_limit");
    assert!(claude["limit_source"]
        .as_str()
        .unwrap()
        .ends_with(".claude/settings.json"));
    for harness in ["open-code", "pi"] {
        assert_eq!(
            rows.iter().find(|r| r["harness"] == harness).unwrap()["listed_skills"],
            0
        );
    }
    let codex = rows.iter().find(|r| r["harness"] == "codex").unwrap();
    assert_eq!(codex["listed_skills"], 1);
    assert_eq!(codex["limit_tokens"], 2000);
    std::fs::write(
        home.join(".codex/config.toml"),
        "model_context_window = 100000\n[skills]\nmax_context_tokens = 30\n",
    )
    .unwrap();
    std::fs::write(
        home.join(".claude/settings.json"),
        r#"{"skillListingBudgetFraction":0.0015}"#,
    )
    .unwrap();
    let override_report = run();
    let overrides = override_report["listing_cost"].as_array().unwrap();
    let codex = overrides.iter().find(|r| r["harness"] == "codex").unwrap();
    assert_eq!(codex["limit_tokens"], 30);
    assert_eq!(codex["warning"], "over_limit");
    assert_eq!(
        overrides
            .iter()
            .find(|r| r["harness"] == "claude-code")
            .unwrap()["warning"],
        "near_limit"
    );
    std::fs::write(
        home.join(".claude/settings.json"),
        r#"{"skillListingBudgetFraction":0.1}"#,
    )
    .unwrap();
    let report = run();
    let claude = report["listing_cost"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["harness"] == "claude-code")
        .unwrap();
    assert_eq!(claude["limit_tokens"], 20000);
    assert!(claude["warning"].is_null());
    for n in 0..6 {
        let path = home.join(format!(".claude/skills/small-{n}"));
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            path.join("SKILL.md"),
            format!("---\nname: small-{n}\ndescription: short\n---\n"),
        )
        .unwrap();
    }
    let report = run();
    let rows = report["listing_cost"].as_array().unwrap();
    let claude = rows.iter().find(|r| r["harness"] == "claude-code").unwrap();
    assert_eq!(claude["listed_skills"], 7);
    assert_eq!(claude["top_skills"].as_array().unwrap().len(), 5);
    assert_eq!(claude["top_skills"][0]["name"], "large");
    let cursor = rows.iter().find(|r| r["harness"] == "cursor").unwrap();
    assert_eq!(cursor["listed_skills"], 9);
    assert!(cursor["limit_tokens"].is_null());
}

fn write(home: &std::path::Path, path: &str, text: &str) {
    let path = home.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn skill(home: &std::path::Path, path: &str) {
    write(
        home,
        &format!("{path}/SKILL.md"),
        "---\nname: review\ndescription: Review code\n---\n",
    );
}

fn report(home: &std::path::Path, command: &str, project: bool) -> serde_json::Value {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_skill-studio"));
    cmd.args([command, "--fixture", home.to_str().unwrap(), "--json"])
        .env("HOME", home)
        .env("CODEX_HOME", home.join(".codex"))
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("SKILL_STUDIO_TELEMETRY", "0");
    if project {
        cmd.arg("--project").arg(home.join("project"));
    }
    let out = cmd.output().unwrap();
    let data = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["data"].clone();
    if command == "doctor" {
        let mut env = vec![(
            "SKILL_STUDIO_FIXTURE".to_string(),
            home.to_str().unwrap().to_string(),
        )];
        if project {
            env.push((
                "SKILL_STUDIO_PROJECT".to_string(),
                home.join("project").to_str().unwrap().to_string(),
            ));
        }
        let mcp = headless::mcp_with_env(home, "doctor", &serde_json::json!({}), &env);
        assert_eq!(
            data["listing_cost"],
            mcp["result"]["structuredContent"]["data"]["listing_cost"]
        );
    }
    data
}

fn row(data: &serde_json::Value, harness: &str, project: bool) -> serde_json::Value {
    data["listing_cost"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["harness"] == harness && (r["scope"]["scope"] == "project") == project)
        .unwrap()
        .clone()
}

#[test]
fn doctor_excludes_disabled_claude_plugins_instead_of_charging_their_listing() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    write(
        home,
        ".claude/plugins/cache/mp/plug/1/.claude-plugin/plugin.json",
        r#"{"name":"plug"}"#,
    );
    skill(home, ".claude/plugins/cache/mp/plug/1/skills/review");
    write(
        home,
        ".claude/settings.json",
        r#"{"enabledPlugins":{"plug@mp":false}}"#,
    );
    assert_eq!(
        row(&report(home, "doctor", false), "claude-code", false)["listed_skills"],
        0
    );
}

#[test]
fn doctor_keeps_same_name_files_in_codex_and_claude_instead_of_replacing_global_entries() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    for path in [
        ".agents/skills/review",
        "project/.agents/skills/review",
        ".claude/skills/review",
        "project/.claude/skills/review",
    ] {
        skill(home, path);
    }
    let data = report(home, "doctor", true);
    for harness in ["codex", "claude-code"] {
        assert_eq!(row(&data, harness, true)["listed_skills"], 2, "{harness}");
        assert_eq!(
            row(&data, harness, true)["top_skills"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }
}

#[test]
fn doctor_and_scan_apply_codex_path_disable_to_universal_skills_instead_of_only_codex_roots() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    skill(home, ".agents/skills/review");
    write(
        home,
        ".codex/config.toml",
        &format!(
            "[[skills.config]]\npath = {:?}\nenabled = false\n",
            home.join(".agents/skills/review/SKILL.md")
        ),
    );
    let data = report(home, "scan", false);
    assert!(data["skills"][0]["deployments"][0]["disabled_readers"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("codex")));
    let data = report(home, "doctor", false);
    assert_eq!(row(&data, "codex", false)["listed_skills"], 0);
    assert_eq!(row(&data, "pi", false)["listed_skills"], 1);
}

#[test]
fn doctor_excludes_missing_and_empty_codex_descriptions_instead_of_counting_rejected_files() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    for (name, description) in [
        ("missing", ""),
        ("empty", "description: ''\n"),
        ("blank", "description: '   '\n"),
    ] {
        write(
            home,
            &format!(".agents/skills/{name}/SKILL.md"),
            &format!("---\nname: {name}\n{description}---\n"),
        );
    }
    skill(home, ".agents/skills/review");
    assert_eq!(
        row(&report(home, "doctor", false), "codex", false)["listed_skills"],
        1
    );
}

#[test]
fn doctor_uses_trusted_project_codex_budget_instead_of_user_budget_or_untrusted_config() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    skill(home, "project/.agents/skills/review");
    write(
        home,
        "project/.codex/config.toml",
        "[skills]\nmax_context_tokens = 40\n",
    );
    let user = "[skills]\nmax_context_tokens = 20\n";
    write(home, ".codex/config.toml", user);
    assert_eq!(
        row(&report(home, "doctor", true), "codex", true)["limit_tokens"],
        20
    );
    write(
        home,
        ".codex/config.toml",
        &format!(
            "{user}\n[projects.{:?}]\ntrust_level = 'trusted'\n",
            home.join("project")
        ),
    );
    let data = report(home, "doctor", true);
    assert_eq!(row(&data, "codex", false)["limit_tokens"], 20);
    let project = row(&data, "codex", true);
    assert_eq!(project["limit_tokens"], 40);
    assert_eq!(
        project["limit_source"],
        home.join("project/.codex/config.toml").to_str().unwrap()
    );
}

#[test]
fn doctor_keeps_a_codex_disabled_skill_in_the_cursor_row_instead_of_hiding_it_from_every_reader() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    skill(home, ".codex/skills/review");
    write(
        home,
        ".codex/config.toml",
        &format!(
            "[[skills.config]]\npath = {:?}\nenabled = false\n",
            home.join(".codex/skills/review/SKILL.md")
        ),
    );
    let data = report(home, "doctor", false);
    assert_eq!(row(&data, "codex", false)["listed_skills"], 0);
    assert_eq!(row(&data, "cursor", false)["listed_skills"], 1);
}

#[test]
fn doctor_charges_codex_utf8_bytes_instead_of_utf16_units_for_non_ascii_descriptions() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    write(
        home,
        ".codex/skills/wide/SKILL.md",
        &format!("---\nname: wide\ndescription: {}\n---\n", "界".repeat(1000)),
    );
    write(
        home,
        ".codex/config.toml",
        "[skills]\nmax_context_tokens = 500\n",
    );
    let codex = row(&report(home, "doctor", false), "codex", false);
    assert!(codex["estimated_tokens_chars_div_4"].as_u64().unwrap() >= 750);
    assert_eq!(codex["warning"], "over_limit");
}

#[test]
fn doctor_and_scan_apply_codex_name_rules_in_order_instead_of_counting_disabled_skills() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    skill(home, ".agents/skills/review");
    skill(home, ".codex/skills/folder");
    for (rules, expected) in [
        ("[[skills.config]]\nname = ' review '\nenabled = false\n".to_string(), 0),
        (format!("[[skills.config]]\nname = 'review'\nenabled = false\n[[skills.config]]\npath = {:?}\nenabled = true\n", home.join(".codex/skills/folder/SKILL.md")), 1),
        (format!("[[skills.config]]\npath = {:?}\nenabled = true\n[[skills.config]]\nname = 'review'\nenabled = false\n", home.join(".codex/skills/folder/SKILL.md")), 0),
    ] {
        write(home, ".codex/config.toml", &rules);
        assert_eq!(row(&report(home, "doctor", false), "codex", false)["listed_skills"], expected);
        let scan = report(home, "scan", false);
        if expected == 0 {
            for deployment in scan["skills"][0]["deployments"].as_array().unwrap() {
                assert!(deployment["disabled_by"] == "codex-config" || deployment["disabled_readers"].as_array().unwrap().contains(&serde_json::json!("codex")));
            }
        }
    }
}

#[test]
fn doctor_merges_project_plugin_switches_instead_of_using_global_enablement() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    write(
        home,
        ".claude/plugins/cache/mp/plug/1/.claude-plugin/plugin.json",
        r#"{"name":"plug"}"#,
    );
    skill(home, ".claude/plugins/cache/mp/plug/1/skills/review");
    write(
        home,
        ".claude/settings.json",
        r#"{"enabledPlugins":{"plug@mp":true}}"#,
    );
    write(
        home,
        "project/.claude/settings.json",
        r#"{"enabledPlugins":{"plug@mp":false}}"#,
    );
    let data = report(home, "doctor", true);
    assert_eq!(row(&data, "claude-code", false)["listed_skills"], 1);
    assert_eq!(row(&data, "claude-code", true)["listed_skills"], 0);
    write(
        home,
        "project/.claude/settings.local.json",
        r#"{"enabledPlugins":{"plug@mp":true}}"#,
    );
    assert_eq!(
        row(&report(home, "doctor", true), "claude-code", true)["listed_skills"],
        1
    );
    write(
        home,
        ".claude/settings.json",
        r#"{"enabledPlugins":{"plug@mp":false}}"#,
    );
    assert_eq!(
        row(&report(home, "doctor", true), "claude-code", true)["listed_skills"],
        1
    );
}
