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
fn a_project_recording_does_not_add_its_local_skill_to_the_global_estimate() {
    let dir = tempfile::tempdir().unwrap();
    let canonical_home = dir.path().canonicalize().unwrap();
    let home = canonical_home.as_path();
    let project = home.join("release");
    skill(home, ".claude/skills", "summary", "Global description");
    skill(
        &project,
        ".claude/skills",
        "verify-packaging",
        "Current project description",
    );
    let project = project.canonicalize().unwrap();
    let slug: String = project
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let capture = serde_json::json!({
        "type": "attachment", "cwd": project, "attachment": {
            "type": "skill_listing", "isInitial": true,
            "names": ["summary", "verify-packaging", "bundled"],
            "content": "- summary: Global description\n- verify-packaging: Old wording\n- bundled: From the client"
        }
    });
    write(
        home,
        &format!(".claude/projects/{slug}/s.jsonl"),
        &capture.to_string(),
    );
    let project = project.to_str().unwrap();
    let global = row(&listing(home, &["--project", project]), "claude-code");
    let both = listing(home, &["--project", project, "--for-project", project]);
    assert_eq!(global["entries"], row(&both, "claude-code")["entries"]);
    assert_eq!(global["entries"].as_array().unwrap().len(), 2);
    assert_eq!(state(&global, "bundled"), "described");
    let local = both["listings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["harness"] == "claude-code" && r["scope"]["scope"] == "project")
        .unwrap();
    assert_eq!(state(local, "verify-packaging"), "described");
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
    // Manual-only now, but the recorded session still listed it.
    write(
        home,
        ".claude/skills/m/SKILL.md",
        "---\nname: m\ndescription: manual\ndisable-model-invocation: true\n---\n",
    );
    write(
        home,
        ".claude/cache/model-catalog/cc.json",
        r#"{"fetchedAt":1,"catalog":{"config":{"models":[{"id":"claude-opus-5-5","runtime":{"max_input_tokens":1000000}}],"provider_alias_targets":{"opus":{"default":"claude-opus-5-5"}}}}}"#,
    );
    // 1000000 tokens x 3 characters x 0.00006 = 180 characters.
    write(
        home,
        ".claude/settings.json",
        r#"{"model":"opus","skillListingBudgetFraction":0.00006,"skillOverrides":{"dataviz":"off"}}"#,
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
    let content = format!(
        "- a\\n- b: {text}\\n- c\\n- m\\n- dataviz: {}",
        "y".repeat(50)
    );
    write(
        home,
        ".claude/projects/-work/s.jsonl",
        &format!(
            "{}\n{}\n",
            format_args!(
                r#"{{"type":"attachment","timestamp":"2026-11-19T00:00:00Z","attachment":{{"type":"skill_listing","isInitial":true,"names":["a","b","c","m","dataviz"],"content":"{content}"}}}}"#
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
    assert_eq!(claude["evidence"], "preview");
    assert_eq!(row(&data, "cursor")["evidence"], "discovery");
    assert_eq!(claude["budget"], 180);
    assert_eq!(claude["context_window_tokens"], 1_000_000);
    assert_eq!(claude["uses_default_context_window"], false);
    assert_eq!(state(&claude, "b"), "described");
    assert_eq!(state(&claude, "a"), "name_only");
    assert_eq!(state(&claude, "c"), "name_only");
    // Turned off now, so the session's bundled `dataviz` is hidden, not listed.
    let dataviz: Vec<_> = claude["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["name"] == "dataviz")
        .collect();
    assert_eq!(dataviz.len(), 1);
    assert_eq!(dataviz[0]["scanned"], false);
    assert_eq!(dataviz[0]["state"], "hidden");
    assert_eq!(
        claude["entries"][1]["observed"], "described",
        "b, listed second, had a description in the session"
    );
    assert_eq!(claude["observed_session"]["model"], "claude-opus-5-5");
    let m: Vec<_> = claude["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["name"] == "m")
        .collect();
    assert_eq!(
        m.len(),
        1,
        "a hidden skill came back from the session: {m:?}"
    );
    assert_eq!(m[0]["state"], "hidden");
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

    let outside = dir.path().join("elsewhere.jsonl");
    std::fs::copy(home.join(".claude/projects/-work/s.jsonl"), &outside).unwrap();
    let mut cmd = test_home::command(env!("CARGO_BIN_EXE_skill-studio"), home);
    let out = cmd
        .args([
            "listing",
            "--fixture",
            home.to_str().unwrap(),
            "--json",
            "--claude-session",
        ])
        .arg(&outside)
        .env("HOME", home)
        .env("SKILL_STUDIO_TELEMETRY", "0")
        .output()
        .unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        envelope["status"], "error",
        "a transcript outside Claude's projects was read"
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
    // Codex quotes a value with `: ` in it and reads the YAML again.
    write(
        home,
        ".codex/skills/colon/SKILL.md",
        "---\nname: colon\ndescription: Triggers on: video\n---\n",
    );
    // A repeated key is invalid YAML that Codex does not repair.
    write(
        home,
        ".codex/skills/bad/SKILL.md",
        "---\nname: bad\ndescription: d\nmetadata:\n  a: 1\nmetadata:\n  b: 2\n---\n",
    );
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
    assert_ne!(
        state(&codex, "colon"),
        "hidden",
        "Codex repairs `: ` values"
    );
    assert_eq!(
        state(&codex, "bad"),
        "hidden",
        "Codex rejects a repeated key"
    );

    let tight = row(&listing(home, &["--codex-max-tokens", "1"]), "codex");
    assert_eq!(state(&tight, "x"), "omitted");
    assert_eq!(
        tight["setting"]["snippet"],
        "[skills]\nmax_context_tokens = 1"
    );
}

fn codex_rollout(home: &std::path::Path, file: &str, cwd: &str, block: &str) {
    let lines = [
        serde_json::json!({"type":"session_meta","payload":{"id":"s","cwd":cwd,"timestamp":"2026-11-20T00:00:00Z"}}).to_string(),
        serde_json::json!({"type":"turn_context","payload":{"model":"m"}}).to_string(),
        serde_json::json!({"type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":block}]}}).to_string(),
    ];
    write(
        home,
        &format!(".codex/sessions/2026/11/20/{file}"),
        &(lines.join("\n") + "\n"),
    );
}

#[test]
fn codex_listing_adds_system_skills_from_the_newest_session_in_catalog_order() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    skill(home, ".agents/skills", "a", &"d".repeat(40));
    // Enough skills that the order check covers more than two entries.
    for i in 0..30 {
        skill(home, ".agents/skills", &format!("z{i:02}"), "Filler");
    }
    // Codex system skills are not scanned; the session names them.
    skill(home, ".codex/skills/.system", "sys", "System skill");
    write(home, ".codex/config.toml", "model = \"m\"\n");
    write(
        home,
        ".codex/models_cache.json",
        r#"{"models":[{"slug":"m","context_window":1000000}]}"#,
    );
    let agents = home.join(".agents/skills");
    let system = home.join(".codex/skills/.system");
    let block = format!(
        "<skills_instructions>\n## Skills\n### Skill roots\n- `r0` = `{}`\n- `r1` = `{}`\n### Available skills\n- sys: System skill (file: r1/sys/SKILL.md)\n- a: {} (file: r0/a/SKILL.md)\n</skills_instructions>",
        agents.display(),
        system.display(),
        "d".repeat(40)
    );
    codex_rollout(home, "rollout-1.jsonl", home.to_str().unwrap(), &block);
    // A newer session in a temporary folder (an eval run) is not the reference.
    codex_rollout(
        home,
        "rollout-2.jsonl",
        "/tmp/eval-run",
        "<skills_instructions>\n### Available skills\n</skills_instructions>",
    );
    let later = std::time::SystemTime::now() + std::time::Duration::from_mins(1);
    std::fs::File::options()
        .write(true)
        .open(home.join(".codex/sessions/2026/11/20/rollout-2.jsonl"))
        .unwrap()
        .set_modified(later)
        .unwrap();

    let codex = row(&listing(home, &[]), "codex");
    let entries = codex["entries"].as_array().unwrap();
    let names: Vec<&str> = entries
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert_eq!(names[..2], ["sys", "a"], "system skills first: {codex}");
    assert_eq!(names.len(), 32, "{codex}");
    assert_eq!(entries[0]["scanned"], false);
    assert_eq!(entries[1]["observed_description_chars"], 40);
    assert_eq!(codex["thresholds"]["tier"], "all_described", "{codex}");
    // The roots table can cost 0 tokens: its header is shorter than the
    // absolute-path one (Codex subtracts the two), as on a short Linux path.
    assert!(codex["thresholds"]["roots_table_cost"].is_u64());
    assert_eq!(codex["thresholds"]["cap"], 10000);
    assert!(codex["observed_session"]["transcript"]
        .as_str()
        .unwrap()
        .ends_with("rollout-1.jsonl"));
}

#[test]
fn codex_session_comparison_reads_only_rollouts_in_this_homes_session_folders() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let other = tempfile::tempdir().unwrap();
    // Named like a rollout, in a folder named like Codex's, outside this home.
    let foreign = other.path().join(".codex/sessions/rollout-1.jsonl");
    std::fs::create_dir_all(foreign.parent().unwrap()).unwrap();
    std::fs::write(&foreign, "").unwrap();
    let mut cmd = test_home::command(env!("CARGO_BIN_EXE_skill-studio"), home);
    cmd.args(["listing", "--fixture", home.to_str().unwrap(), "--json"])
        .args(["--codex-session", foreign.to_str().unwrap()])
        .env("HOME", home)
        .env("CODEX_HOME", home.join(".codex"))
        .env("SKILL_STUDIO_TELEMETRY", "0");
    let out = cmd.output().unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(envelope["status"], "error", "{envelope}");
    assert!(envelope["errors"][0]["message"]
        .as_str()
        .unwrap()
        .contains("in this home"));
}

#[cfg(unix)]
#[test]
fn listing_and_sessions_read_a_large_home_under_the_desktop_file_budget() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    for index in 0..450 {
        skill(
            home,
            ".claude/skills",
            &format!("skill-{index}"),
            "A coding skill",
        );
    }
    // Fixture reads share existing locks but never create them.
    let leases = home.join(".skill-studio/leases");
    std::fs::create_dir_all(&leases).unwrap();
    let seed_lease = |root: &std::path::Path| {
        let root = root.canonicalize().unwrap();
        let key = skill_studio_core::identity::sha256_hex(root.to_string_lossy().as_bytes());
        std::fs::File::create(leases.join(format!("{key}.lock"))).unwrap();
    };
    seed_lease(home);

    let mut project_args = Vec::new();
    for index in 0..180 {
        let path = home.join(format!("projects/project-{index}"));
        std::fs::create_dir_all(&path).unwrap();
        seed_lease(&path);
        project_args.push("--project".into());
        project_args.push(path.into_os_string());
    }

    for report in ["listing", "sessions"] {
        // Finder's 256 descriptors less 80 already open in the desktop app.
        let output = test_home::command("/bin/sh", home)
            .args(["-c", "ulimit -n 176 && exec \"$@\"", "file-budget"])
            .args([env!("CARGO_BIN_EXE_skill-studio"), report, "--fixture"])
            .arg(home)
            .env("SKILL_STUDIO_FIXTURE", home)
            .args(project_args.iter())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{report} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
