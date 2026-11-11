#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::process::Command;

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
