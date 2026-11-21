#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]
#[path = "support/headless.rs"]
mod headless;
use serde_json::json;

fn write(root: &std::path::Path, path: &str, body: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn home() -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    let skill = "---\nname: s\ndescription: A test skill.\n---\nBody.\n";
    write(home.path(), ".agents/skills/tracked/SKILL.md", skill);
    write(
        home.path(),
        ".agents/.skill-lock.json",
        r#"{"version":3,"skills":{"tracked":{"source":"owner/repo","sourceType":"github","sourceUrl":"https://github.com/owner/repo.git","skillFolderHash":"0000000000000000000000000000000000000000","installedAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z"}}}"#,
    );
    write(home.path(), "project/.agents/skills/pinned/SKILL.md", skill);
    write(
        home.path(),
        "project/skills-lock.json",
        r#"{"version":1,"skills":{"pinned":{"source":"owner/pinned","sourceType":"github","ref":"v1.2"}}}"#,
    );
    home
}

#[test]
fn pack_list_gives_the_same_packs_on_cli_and_mcp_or_the_surfaces_drift() {
    let home = home();
    let project = home.path().join("project");
    let project = project.to_str().unwrap();

    let cli = headless::cli(home.path(), &["pack", "list", "--project-path", project]);
    let mcp = headless::mcp(
        home.path(),
        "pack_list",
        &json!({ "project_path": project }),
    );
    let mcp = &mcp["result"]["structuredContent"];

    assert_eq!(cli["status"], "ok", "{cli}");
    assert_eq!(cli["data"]["packs"], mcp["data"]["packs"]);
    let packs = cli["data"]["packs"].as_array().unwrap();
    assert_eq!(packs.len(), 1, "{cli}");
    assert_eq!(packs[0]["source"], "owner/pinned");
    assert_eq!(packs[0]["ref_pin"], "v1.2");
    assert_eq!(packs[0]["remote"]["state"], "not_checked");
}

#[cfg(unix)]
#[test]
fn pack_update_caps_the_total_plan_on_cli_and_mcp_before_running_owner_tools() {
    use std::os::unix::fs::PermissionsExt;

    let home = tempfile::tempdir().unwrap();
    let root = home.path();
    write(
        root,
        ".agents/skill-studio.json",
        &json!({"projects": {"added": [root.join("project")]}}).to_string(),
    );
    write(root, "bin/npx", "#!/bin/sh\n: > \"${0%/*}/owner-called\"\n");
    std::fs::set_permissions(root.join("bin/npx"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let env = [("PATH".to_string(), root.join("bin").display().to_string())];

    let skill = json!({
        "source": "owner/pack",
        "sourceType": "github",
        "sourceUrl": "https://github.com/owner/pack.git",
        "skillFolderHash": "unknown",
        "installedAt": "2026-01-01T00:00:00Z",
        "updatedAt": "2026-01-01T00:00:00Z",
    });
    let lock = |prefix: &str, path: &str, version: u32, count: usize| {
        let mut rows = serde_json::Map::new();
        for i in 0..count {
            let name = format!("skill-{i}");
            write(
                root,
                &format!("{prefix}.agents/skills/{name}/SKILL.md"),
                &format!("---\nname: {name}\ndescription: A test skill.\n---\nBody.\n"),
            );
            rows.insert(name, skill.clone());
        }
        write(
            root,
            path,
            &json!({"version": version, "skills": rows}).to_string(),
        );
    };
    lock("", ".agents/.skill-lock.json", 3, 50);
    lock("project/", "project/skills-lock.json", 1, 51);

    let update = |dry_run: bool| {
        let mut args = vec!["pack", "update", "--all"];
        if dry_run {
            args.push("--dry-run");
        }
        let cli = headless::cli_with_env(root, &args, &env);
        let mcp = headless::mcp_with_env(
            root,
            "pack_update",
            &json!({"all": true, "dry_run": dry_run}),
            &env,
        );
        (cli, mcp["result"]["structuredContent"].clone())
    };
    let (cli, mcp) = update(false);
    assert_eq!(
        [cli["errors"][0]["code"].as_str(), mcp["errors"][0]["code"].as_str()],
        [Some("invalid_request"); 2],
        "a 101-step plan must fail on CLI and MCP before updates (CLI status {}, MCP status {}, owner called {})",
        cli["status"], mcp["status"], root.join("bin/owner-called").exists(),
    );
    for result in [&cli, &mcp] {
        assert!(result["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("101 skills"));
    }
    assert!(!root.join("bin/owner-called").exists());

    lock("project/", "project/skills-lock.json", 1, 50);
    let (cli, mcp) = update(true);
    for result in [&cli, &mcp] {
        assert_eq!(result["status"], "ok", "{}", result["errors"]);
        let steps = result["data"]["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 100);
        assert!(steps.iter().all(|step| step["state"]["state"] == "planned"));
    }
    assert!(!root.join("bin/owner-called").exists());
}

#[test]
fn pack_show_reports_one_pack_and_names_a_missing_source_on_cli_and_mcp_or_fails_silently() {
    let home = home();

    let shown = headless::cli(home.path(), &["pack", "show", "owner/repo"]);
    let skill = &shown["data"]["packs"][0]["skills"][0];
    assert_eq!(skill["name"], "tracked", "{shown}");
    assert_eq!(skill["edited"], true, "{shown}");

    let cli = headless::cli(home.path(), &["pack", "show", "nobody/none"]);
    let mcp = headless::mcp(
        home.path(),
        "pack_show",
        &json!({ "source": "nobody/none" }),
    );
    let mcp = &mcp["result"]["structuredContent"];
    assert_eq!(cli["status"], "error", "{cli}");
    assert_eq!(cli["errors"], mcp["errors"]);
    assert!(
        cli["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("nobody/none"),
        "{cli}"
    );
}

#[test]
fn pack_dry_runs_give_the_same_plan_on_cli_and_mcp_and_change_nothing_or_the_surfaces_drift() {
    let home = home();
    let lock = std::fs::read(home.path().join(".agents/.skill-lock.json")).unwrap();
    let cases = [
        (
            vec![
                "pack",
                "add",
                "owner/repo",
                "--skill",
                "tracked",
                "--skill",
                "fresh",
                "--dry-run",
            ],
            "pack_add",
            json!({ "source": "owner/repo", "skills": ["tracked", "fresh"], "dry_run": true }),
            json!([["fresh", "planned"], ["tracked", "skipped"]]),
        ),
        (
            vec!["pack", "update", "owner/repo", "--dry-run"],
            "pack_update",
            json!({ "source": "owner/repo", "dry_run": true }),
            json!([["tracked", "skipped"]]),
        ),
        (
            vec!["pack", "remove", "owner/repo", "--dry-run"],
            "pack_remove",
            json!({ "source": "owner/repo", "dry_run": true }),
            json!([["tracked", "skipped"]]),
        ),
        (
            vec![
                "pack",
                "switch-source",
                "owner/repo",
                "me/fork",
                "--dry-run",
            ],
            "pack_switch_source",
            json!({ "source": "owner/repo", "to": "me/fork", "dry_run": true }),
            json!([["tracked", "skipped"]]),
        ),
    ];
    for (args, tool, arguments, expected) in cases {
        let cli = headless::cli(home.path(), &args);
        let mcp = headless::mcp(home.path(), tool, &arguments);
        let mcp = &mcp["result"]["structuredContent"];
        assert_eq!(cli["status"], "ok", "{tool}: {cli}");
        assert_eq!(cli["data"], mcp["data"], "{tool}");
        let states: Vec<_> = cli["data"]["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|step| json!([step["skill"], step["state"]["state"]]))
            .collect();
        assert_eq!(json!(states), expected, "{tool}: {cli}");
    }
    assert!(home.path().join(".agents/skills/tracked/SKILL.md").exists());
    assert!(!home.path().join(".agents/skills/fresh").exists());
    assert_eq!(
        std::fs::read(home.path().join(".agents/.skill-lock.json")).unwrap(),
        lock
    );
    assert!(!home.path().join(".agents/skill-studio.json").exists());
}

/// Flow: export `tracked` as a pack, as a dry run, on the CLI and MCP; then
/// for real on the CLI.
/// Expectation: both surfaces plan the same files; the dry run creates no
/// folder; the real run writes the skill and a README with the install line.
/// A failure means the surfaces drift, or a dry run writes.
#[test]
fn pack_export_gives_the_same_plan_on_cli_and_mcp_and_writes_only_for_real_or_the_surfaces_drift() {
    let home = home();
    let out = home.path().join("my-pack");
    let dir = out.to_str().unwrap();
    let cli = headless::cli(
        home.path(),
        &[
            "pack",
            "export",
            dir,
            "--skill",
            "tracked",
            "--source",
            "me/pack",
            "--dry-run",
        ],
    );
    let mcp = headless::mcp(
        home.path(),
        "pack_export",
        &json!({ "dir": dir, "skills": ["tracked"], "source": "me/pack", "dry_run": true }),
    );
    assert_eq!(cli["status"], "ok", "{cli}");
    assert_eq!(cli["data"], mcp["result"]["structuredContent"]["data"]);
    assert!(!out.exists());

    let cli = headless::cli(
        home.path(),
        &[
            "pack", "export", dir, "--skill", "tracked", "--source", "me/pack",
        ],
    );
    assert_eq!(cli["status"], "ok", "{cli}");
    assert!(out.join("skills/tracked/SKILL.md").exists());
    let readme = std::fs::read_to_string(out.join("README.md")).unwrap();
    assert!(
        readme.contains("npx skills add me/pack --skill tracked"),
        "{readme}"
    );
}

/// An oversized pack still returns complete skill rows and a way to read the rest.
#[test]
fn oversized_pack_skills_page_within_the_budget_on_cli_and_mcp() {
    use skill_studio_core::view::ANSWER_BYTES;

    let home = home();
    let names: Vec<_> = (0..320)
        .map(|i| format!("large-pack-skill-{i:03}"))
        .collect();
    let lock_path = home.path().join(".agents/.skill-lock.json");
    let mut lock: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&lock_path).unwrap()).unwrap();
    let template = lock["skills"]["tracked"].clone();
    for name in &names {
        write(
            home.path(),
            &format!(".agents/skills/{name}/SKILL.md"),
            &format!("---\nname: {name}\ndescription: A pack skill.\n---\n"),
        );
        let mut skill = template.clone();
        skill["source"] = json!("owner/zz-large");
        skill["sourceUrl"] = json!("https://github.com/owner/zz-large.git");
        lock["skills"][name] = skill;
    }
    std::fs::write(&lock_path, lock.to_string()).unwrap();

    for surface in ["cli", "mcp"] {
        let call = |show: bool, cursor: Option<&str>| {
            if surface == "cli" {
                let mut args = if show {
                    vec!["pack", "show", "owner/zz-large"]
                } else {
                    vec!["pack", "list"]
                };
                if let Some(cursor) = cursor {
                    args.extend(["--cursor", cursor]);
                }
                headless::cli(home.path(), &args)
            } else {
                let tool = if show { "pack_show" } else { "pack_list" };
                let mut args = if show {
                    json!({"source": "owner/zz-large"})
                } else {
                    json!({})
                };
                if let Some(cursor) = cursor {
                    args["cursor"] = json!(cursor);
                }
                headless::mcp(home.path(), tool, &args)["result"]["structuredContent"].clone()
            }
        };
        let size = |envelope: &serde_json::Value| {
            let bytes = envelope.to_string().len();
            assert!(
                bytes <= ANSWER_BYTES,
                "{surface} pack answer is {bytes} bytes, budget is {ANSWER_BYTES}"
            );
            assert_eq!(
                envelope["status"], "ok",
                "{surface}: {}",
                envelope["errors"]
            );
        };

        let mut answer = call(true, None);
        size(&answer);
        let first_page = &answer["page"]["also"][0];
        assert_eq!(first_page["list"], "/packs/0/skills", "{surface}");
        assert_eq!(first_page["total"], names.len(), "{surface}");
        assert!(first_page["next_cursor"].is_string(), "{surface}");
        let mut seen = Vec::new();
        for index in 0..names.len() {
            let page = if index == 0 {
                &answer["page"]["also"][0]
            } else {
                &answer["page"]
            };
            let list = page["list"].as_str().unwrap();
            let skills = answer["data"]
                .pointer(list)
                .and_then(serde_json::Value::as_array)
                .unwrap();
            assert_eq!(
                answer["data"]["packs"][0]["source"], "owner/zz-large",
                "{surface}"
            );
            assert_eq!(page["returned"], skills.len(), "{surface}");
            assert!(!skills.is_empty(), "{surface}");
            seen.extend(
                skills
                    .iter()
                    .map(|skill| skill["name"].as_str().unwrap().to_owned()),
            );
            let Some(cursor) = page["next_cursor"].as_str() else {
                break;
            };
            answer = call(true, Some(cursor));
            size(&answer);
        }
        assert_eq!(seen, names, "{surface}: skills were lost or repeated");

        let small = call(false, None);
        size(&small);
        assert_eq!(
            small["data"]["packs"][0]["source"], "owner/repo",
            "{surface}"
        );
        let large = call(false, Some(small["page"]["next_cursor"].as_str().unwrap()));
        size(&large);
        assert_eq!(
            large["data"]["packs"][0]["source"], "owner/zz-large",
            "{surface}"
        );
        let next = call(false, large["page"]["also"][0]["next_cursor"].as_str());
        size(&next);
        assert_eq!(next["page"]["list"], "/packs/0/skills", "{surface}");
    }
}
