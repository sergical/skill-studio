#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::process::Command;

#[test]
fn doctor_cli_warns_for_external_links_and_errors_for_broken_links_or_reports_a_healthy_scope() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let external = dir.path().join("external");
    let root = home.join(".claude/skills");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&external).unwrap();
    std::fs::write(
        external.join("SKILL.md"),
        "---\nname: dev-skill\ndescription: fixture\n---\n",
    )
    .unwrap();
    let link = root.join("dev-skill");
    std::os::unix::fs::symlink(&external, &link).unwrap();
    let run = |json: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_skill-studio"));
        command.args(["doctor", "--fixture", home.to_str().unwrap()]);
        if json {
            command.arg("--json");
        }
        command
            .env("HOME", &home)
            .env("CODEX_HOME", home.join(".codex"))
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("XDG_DATA_HOME", home.join(".local/share"))
            .env("SKILL_STUDIO_TELEMETRY", "0")
            .output()
            .unwrap()
    };
    for expected in ["warning", "error"] {
        let output = run(true);
        let human = run(false);
        let human = String::from_utf8(human.stdout).unwrap();
        assert!(
            human
                .lines()
                .any(|line| line.starts_with(&format!("{expected}:"))),
            "{human}"
        );
        let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let links: Vec<_> = envelope["data"]["violations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["invariant"] == "link_resolves_in_root")
            .collect();
        assert_eq!(links.len(), 1, "{envelope}");
        assert_eq!(links[0]["severity"], expected);
        assert!(std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        std::fs::remove_file(external.join("SKILL.md")).ok();
        std::fs::remove_dir(&external).ok();
    }
}
