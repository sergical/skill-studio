#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]
#[path = "support/headless.rs"]
mod headless;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

fn real_skill(bucket: &str, name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/skill-studio-core/tests/fixtures/real-packs/mattpocock_skills/skills")
        .join(bucket)
        .join(name)
}

/// Installs a real skill the way skills.sh does: a copy in the shared
/// folder, and a per-skill link from Claude Code's folder when asked.
fn install(home: &Path, bucket: &str, name: &str, link_for_claude: bool) {
    let shared = home.join(".agents/skills").join(name);
    std::fs::create_dir_all(&shared).unwrap();
    for entry in std::fs::read_dir(real_skill(bucket, name)).unwrap() {
        let entry = entry.unwrap();
        if entry.path().is_file() {
            std::fs::copy(entry.path(), shared.join(entry.file_name())).unwrap();
        }
    }
    if link_for_claude {
        std::fs::create_dir_all(home.join(".claude/skills")).unwrap();
        std::os::unix::fs::symlink(&shared, home.join(".claude/skills").join(name)).unwrap();
    }
}

fn grill_me_to_grilling(home: &Path) -> (Value, Value) {
    let cli = headless::cli(home, &["links", "grill-me"]);
    let mcp = headless::mcp(home, "skill_links", &json!({"skill": "grill-me"}));
    let mcp = mcp["result"]["structuredContent"].clone();
    for field in ["schema_version", "operation", "status", "errors", "data"] {
        assert_eq!(
            cli[field], mcp[field],
            "{field} differs between CLI and MCP"
        );
    }
    let link = cli["data"]["links"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["from"] == "grill-me" && l["to"] == "grilling")
        .cloned()
        .unwrap();
    (cli, link)
}

fn state(link: &Value, agent: &str) -> String {
    link["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["agent"] == agent)
        .map(|a| a["state"].as_str().unwrap().to_string())
        .unwrap()
}

#[test]
fn links_show_a_skill_grill_me_cannot_reach_per_agent_the_same_on_cli_and_mcp() {
    let home = tempfile::tempdir().unwrap();
    install(home.path(), "productivity", "grill-me", true);

    let (cli, link) = grill_me_to_grilling(home.path());
    assert_eq!(cli["operation"], "skill_links");
    assert_eq!(link["strength"], "hands_off");
    assert_eq!(
        link["evidence"]["text"],
        "Call the Skill tool with \"grilling\"."
    );
    assert_eq!(state(&link, "claude-code"), "missing");
    assert_eq!(state(&link, "codex"), "missing");

    // In the shared folder only: Codex reads it, Claude Code does not.
    install(home.path(), "productivity", "grilling", false);
    let (_, link) = grill_me_to_grilling(home.path());
    assert_eq!(state(&link, "claude-code"), "elsewhere");
    assert_eq!(state(&link, "codex"), "ok");
}

#[test]
fn a_prefixed_install_shows_as_renamed_and_an_agent_filter_drops_skills_it_cannot_load() {
    let home = tempfile::tempdir().unwrap();
    let shared = home.path().join(".agents/skills");
    // A port that prefixes every folder and name without rewriting the text.
    for (bucket, name) in [("productivity", "grill-me"), ("productivity", "grilling")] {
        install(home.path(), bucket, name, false);
        let prefixed = format!("cursor-team-kit-{name}");
        std::fs::rename(shared.join(name), shared.join(&prefixed)).unwrap();
        let skill_md = shared.join(&prefixed).join("SKILL.md");
        let text = std::fs::read_to_string(&skill_md).unwrap();
        std::fs::write(
            &skill_md,
            text.replacen(&format!("name: {name}"), &format!("name: {prefixed}"), 1),
        )
        .unwrap();
    }
    let query = || headless::cli(home.path(), &["links", "cursor-team-kit-grill-me"]);
    assert_eq!(state(&query()["data"]["links"][0], "codex"), "missing");
    let mut records = serde_json::Map::new();
    for name in ["grill-me", "grilling"] {
        records.insert(format!("cursor-team-kit-{name}"),json!({
            "source":"mattpocock/skills","sourceType":"github","sourceUrl":"https://github.com/mattpocock/skills.git",
            "skillPath":format!("skills/productivity/{name}/SKILL.md"),"skillFolderHash":"","installedAt":"","updatedAt":""
        }));
    }
    let lock = json!({"version":3,"skills":records});
    std::fs::write(
        home.path().join(".agents/.skill-lock.json"),
        serde_json::to_vec(&lock).unwrap(),
    )
    .unwrap();
    let cli = headless::cli(home.path(), &["links", "cursor-team-kit-grill-me"]);
    let links = cli["data"]["links"].as_array().unwrap();
    let link = links
        .iter()
        .find(|l| l["to"] == "grilling")
        .cloned()
        .unwrap();
    assert_eq!(state(&link, "codex"), "renamed");

    std::fs::create_dir_all(home.path().join(".claude/skills")).unwrap();
    std::fs::rename(
        shared.join("cursor-team-kit-grill-me"),
        home.path().join(".claude/skills/cursor-team-kit-grill-me"),
    )
    .unwrap();
    let claude_only = headless::cli(
        home.path(),
        &["links", "cursor-team-kit-grill-me", "--agent", "codex"],
    );
    assert!(claude_only["data"]["links"].as_array().unwrap().is_empty());
}

#[test]
fn an_uninstalled_pstack_target_keeps_its_line_and_only_a_record_identifies_it() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join(".agents");
    let dir = root.join("skills/poteto-mode");
    std::fs::create_dir_all(&dir).unwrap();
    let original = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/skill-studio-core/tests/fixtures/real-packs/cursor_plugins_pstack/skills/poteto-mode/SKILL.md");
    std::fs::copy(original, dir.join("SKILL.md")).unwrap();
    let read = || {
        headless::cli(home.path(), &["links", "no-comments", "--agent", "codex"])["data"].clone()
    };
    let by_source = |extra: &[&str]| {
        let mut args = vec!["links", "poteto-mode", "--agent", "codex"];
        args.extend(extra);
        headless::cli(home.path(), &args)["data"].clone()
    };
    assert_eq!(by_source(&[])["unverified"], json!([]));
    assert!(by_source(&["--unverified"])["unverified"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["to"] == "no-comments"));
    let untracked = read();
    assert!(untracked["links"].as_array().unwrap().is_empty());
    assert_eq!(untracked["unverified"][0]["evidence"]["line"], 24);
    std::fs::write(
        root.join("agents.toml"),
        "[[skills]]\nname='*'\nsource='cursor/plugins'\nexclude=['no-comments']\n",
    )
    .unwrap();
    let recorded = read();
    assert_eq!(recorded["links"][0]["strength"], "mentions");
    assert_eq!(recorded["links"][0]["agents"][0]["state"], "missing");
    assert_eq!(
        recorded["links"][0]["evidence"],
        untracked["unverified"][0]["evidence"]
    );
    assert_eq!(recorded["broken"], 0);
}

#[test]
fn a_global_break_counts_once_and_only_a_different_project_gets_its_own_state() {
    let home = tempfile::tempdir().unwrap();
    install(home.path(), "productivity", "grill-me", false);
    let project = home.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    let read = || {
        headless::cli(
            home.path(),
            &[
                "links",
                "grill-me",
                "--agent",
                "codex",
                "--project",
                project.to_str().unwrap(),
            ],
        )["data"]
            .clone()
    };
    let missing = read();
    assert_eq!(missing["broken"], 1);
    assert_eq!(missing["links"][0]["agents"].as_array().unwrap().len(), 1);
    install(&project, "productivity", "grilling", false);
    let data = read();
    let states = data["links"][0]["agents"].as_array().unwrap();
    assert_eq!(states.len(), 2);
    assert_eq!(states[0]["scope"]["scope"], "global");
    assert_eq!(states[0]["state"], "elsewhere");
    assert_eq!(states[1]["scope"]["scope"], "project");
    assert_eq!(states[1]["state"], "ok");
    assert_eq!(data["broken"], 1);
}
