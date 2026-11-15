#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/git_pack.rs"]
mod git_pack;

use std::path::Path;
use std::process::Command;

use git_pack::{run, skill_body, GitPack};
use serde_json::{json, Value};
use skill_studio_core::skills_cli_agents::SKILLS_CLI_PACKAGE;

struct Contract {
    root: tempfile::TempDir,
    home: std::path::PathBuf,
    pack: GitPack,
}

impl Contract {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        std::fs::create_dir(&home).unwrap();
        let pack = GitPack::new(root.path());
        Self { root, home, pack }
    }

    fn cli(&self, cwd: &Path, args: &[&str]) -> Vec<u8> {
        run(Command::new("npx")
            .args(["-y", SKILLS_CLI_PACKAGE])
            .args(args)
            .current_dir(cwd)
            .env("HOME", &self.home)
            .env_remove("XDG_STATE_HOME")
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("XDG_CACHE_HOME")
            .env_remove("CLAUDE_CONFIG_DIR")
            .env_remove("CODEX_HOME")
            .env("DISABLE_TELEMETRY", "1")
            .env("DO_NOT_TRACK", "1")
            // Update uses host-agent detection, independent of add --agent.
            .env("AI_AGENT", "claude")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null"))
    }

    fn global(&self, args: &[&str]) -> Vec<u8> {
        self.cli(&self.home, args)
    }

    fn lock(&self) -> Value {
        read_json(&self.home.join(".agents/.skill-lock.json"))
    }
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn assert_skill(root: &Path, name: &str, version: &str) {
    assert_eq!(
        std::fs::read_to_string(root.join(".claude/skills").join(name).join("SKILL.md")).unwrap(),
        skill_body(name, version)
    );
}

#[test]
#[ignore = "requires npm registry access to fetch the pinned skills CLI"]
fn skills_cli_contract_global_pack() {
    let c = Contract::new();
    // W1: listing is read-only; -l does not support JSON.
    c.global(&["add", &c.pack.url, "-l"]);
    assert!(!c.home.join(".agents/skills").exists());
    assert!(!c.home.join(".agents/.skill-lock.json").exists());
    // W2 and W3. One selected agent defaults to copies in skills 1.8.1.
    c.global(&[
        "add",
        &c.pack.url,
        "--skill",
        "a",
        "--skill",
        "b",
        "-g",
        "-a",
        "claude-code",
        "-y",
        "--json",
    ]);
    let lock = c.lock();
    assert_eq!(lock["skills"].as_object().unwrap().len(), 2);
    let listed: Value = serde_json::from_slice(&c.global(&["ls", "-g", "--json"])).unwrap();
    for name in ["a", "b"] {
        assert_skill(&c.home, name, "v1");
        assert!(!c.home.join(".claude/skills").join(name).is_symlink());
        assert_eq!(lock["skills"][name]["sourceType"], "git");
        assert_eq!(lock["skills"][name]["source"], c.pack.url);
        let row = listed
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == name)
            .unwrap();
        assert_eq!(row["source"], lock["skills"][name]["source"]);
        assert_eq!(row["sourceType"], "git");
    }
    // W4 and W10: unchanged local edits survive, new upstream skills stay absent.
    let b = c.home.join(".claude/skills/b/SKILL.md");
    std::fs::write(&b, skill_body("b", "local")).unwrap();
    c.pack.skill("a", "v2");
    c.pack.skill("d", "v2");
    c.pack.publish("v2");
    c.global(&["update", "-g", "-y"]);
    assert_skill(&c.home, "a", "v2");
    assert!(c.home.join(".claude/skills/a").is_symlink());
    assert_skill(&c.home, "b", "local");
    assert_eq!(c.lock()["skills"]["b"], lock["skills"]["b"]);
    assert_ne!(
        c.lock()["skills"]["a"]["skillFolderHash"],
        lock["skills"]["a"]["skillFolderHash"]
    );
    assert!(c.lock()["skills"].get("d").is_none());
    assert!(!c.home.join(".agents/skills/d").exists());
    assert!(!c.home.join(".claude/skills/d").exists());
    // W5: an upstream change overwrites the local edit. Fork protects edits.
    c.pack.skill("b", "v3");
    c.pack.publish("v3");
    c.global(&["update", "b", "-g", "-y"]);
    assert_skill(&c.home, "b", "v3");
    assert!(c.home.join(".claude/skills/b").is_symlink());
    assert_ne!(
        c.lock()["skills"]["b"]["skillFolderHash"],
        lock["skills"]["b"]["skillFolderHash"]
    );
    // W6 and W7.
    c.global(&[
        "add",
        &format!("{}#v1", c.pack.url),
        "--skill",
        "c",
        "-g",
        "-a",
        "claude-code",
        "-y",
    ]);
    assert_skill(&c.home, "c", "v1");
    assert_eq!(c.lock()["skills"]["c"]["ref"], "v1");
    c.global(&["remove", "a", "b", "c", "-g", "-y"]);
    assert!(c.lock()["skills"].as_object().unwrap().is_empty());
    for name in ["a", "b", "c"] {
        for dir in [".agents/skills", ".claude/skills"] {
            assert!(std::fs::symlink_metadata(c.home.join(dir).join(name)).is_err());
        }
    }
}

#[test]
#[ignore = "requires npm registry access to fetch the pinned skills CLI"]
fn skills_cli_contract_project_restore() {
    let c = Contract::new();
    let project = c.root.path().join("project");
    run(Command::new("git")
        .args(["init", "-b", "main"])
        .arg(&project));
    c.cli(
        &project,
        &[
            "add",
            &format!("{}#v1", c.pack.url),
            "--skill",
            "a",
            "--skill",
            "c",
            "-a",
            "claude-code",
            "-y",
        ],
    );
    let lock = read_json(&project.join("skills-lock.json"));
    assert_eq!(lock["version"], 2);
    for name in ["a", "c"] {
        assert_skill(&project, name, "v1");
        assert_eq!(lock["skills"][name]["ref"], "v1");
    }
    run(Command::new("git")
        .current_dir(&project)
        .args(["add", "skills-lock.json"]));
    run(Command::new("git").current_dir(&project).args([
        "-c",
        "user.name=Contract",
        "-c",
        "user.email=contract@example.invalid",
        "commit",
        "-m",
        "lock",
    ]));
    let clone = c.root.path().join("clone");
    run(Command::new("git").arg("clone").arg(&project).arg(&clone));
    c.pack.skill("a", "v2");
    c.pack.publish("v2");
    c.cli(&clone, &["install", "--dry-run", "-a", "claude-code"]);
    assert_eq!(read_json(&clone.join("skills-lock.json")), lock);
    assert!(!clone.join(".agents").exists());
    assert!(!clone.join(".claude").exists());
    c.cli(&clone, &["install", "-a", "claude-code"]);
    assert_eq!(read_json(&clone.join("skills-lock.json")), lock);
    for name in ["a", "c"] {
        assert_skill(&clone, name, "v1");
        assert!(clone.join(".claude/skills").join(name).is_symlink());
    }
}

#[test]
#[ignore = "requires npm registry access to fetch the pinned skills CLI"]
fn skills_cli_contract_npm_sync() {
    let c = Contract::new();
    let project = c.root.path().join("project");
    let pkg = c.root.path().join("pkg");
    std::fs::create_dir(&project).unwrap();
    std::fs::create_dir_all(pkg.join("skills/epsilon")).unwrap();
    std::fs::write(
        pkg.join("skills/epsilon/SKILL.md"),
        skill_body("epsilon", "v1"),
    )
    .unwrap();
    std::fs::write(pkg.join("package.json"), json!({"name":"contract-pack", "version":"1.0.0", "skills":[{"source":c.pack.url,"skills":["b"],"ref":"v1"}]}).to_string()).unwrap();
    std::fs::write(project.join("package.json"), json!({"name":"contract-project", "private":true, "dependencies":{"contract-pack":"file:../pkg"}}).to_string()).unwrap();
    run(Command::new("npm")
        .current_dir(&project)
        .args([
            "install",
            "--offline",
            "--ignore-scripts",
            "--no-audit",
            "--no-fund",
        ])
        .env("HOME", &c.home)
        .env("DISABLE_TELEMETRY", "1")
        .env("DO_NOT_TRACK", "1"));
    c.cli(
        &project,
        &["sync", "-a", "claude-code", "-y", "--no-frozen-lockfile"],
    );
    let lock = read_json(&project.join("skills-lock.json"));
    assert_eq!(lock["skills"]["b"]["via"], "contract-pack");
    assert_eq!(lock["skills"]["b"]["ref"], "v1");
    assert_eq!(lock["skills"]["epsilon"]["sourceType"], "node_modules");
    assert_skill(&project, "b", "v1");
    assert_eq!(
        std::fs::read_to_string(project.join(".claude/skills/epsilon/SKILL.md")).unwrap(),
        skill_body("epsilon", "v1")
    );
}

#[test]
fn git_pack_fixture_shallow_clone_and_tag() {
    let root = tempfile::tempdir().unwrap();
    let pack = GitPack::new(root.path());
    let clone = root.path().join("clone");
    run(Command::new("git")
        .args(["clone", "--depth", "1", "--branch", "v1"])
        .arg(&pack.url)
        .arg(&clone));
    assert_eq!(
        std::fs::read_to_string(clone.join("skills/a/SKILL.md")).unwrap(),
        skill_body("a", "v1")
    );
    pack.skill("a", "v2");
    pack.publish("v2");
    run(Command::new("git")
        .current_dir(&clone)
        .args(["fetch", "--depth", "1", "origin", "tag", "v2"]));
    run(Command::new("git")
        .current_dir(&clone)
        .args(["checkout", "v2"]));
    assert_eq!(
        std::fs::read_to_string(clone.join("skills/a/SKILL.md")).unwrap(),
        skill_body("a", "v2")
    );
}
