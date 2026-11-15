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
    let env = vec![("SKILL_STUDIO_PROJECT".to_string(), project.to_string())];
    let mcp = headless::mcp_with_env(
        home.path(),
        "pack_list",
        &json!({ "project_path": project }),
        &env,
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
