#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::process::Command;

fn cli_bin() -> std::path::PathBuf {
    std::env::var_os("SKILL_STUDIO_TEST_BINARY").map_or_else(
        || env!("CARGO_BIN_EXE_skill-studio").into(),
        std::path::PathBuf::from,
    )
}

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
        let mut command = Command::new(cli_bin());
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

#[test]
fn doctor_live_home_reports_old_orphan_fork_and_preserves_active_base_or_skips_data_root() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let forks = home.join(".skill-studio/forks");
    for name in ["orphan", "active"] {
        std::fs::create_dir_all(forks.join(name).join("base")).unwrap();
        std::fs::write(
            forks.join(name).join("base/SKILL.md"),
            format!("---\nname: {name}\ndescription: fixture\n---\n"),
        )
        .unwrap();
    }
    std::fs::create_dir_all(home.join(".agents")).unwrap();
    std::fs::write(
        home.join(".agents/skill-studio.json"),
        serde_json::to_vec(&serde_json::json!({
            "forks": {"active": {}},
            "retained": [
                {"base_dir": forks.join("orphan/base"), "created_at": "2000-01-01T00:00:00Z"},
                {"base_dir": forks.join("active/base"), "created_at": "2000-01-01T00:00:00Z"}
            ]
        }))
        .unwrap(),
    )
    .unwrap();
    let output = Command::new(cli_bin())
        .args(["doctor", "--home", home.to_str().unwrap(), "--json"])
        .env("HOME", home)
        .env("SKILL_STUDIO_TELEMETRY", "0")
        .output()
        .unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = envelope["data"]["violations"].as_array().unwrap();
    assert!(
        rows.iter()
            .any(|v| v["path"] == forks.join("orphan/base").to_str().unwrap()
                && v["detail"].as_str().unwrap().contains("older than")),
        "{envelope}"
    );
    assert!(
        !rows
            .iter()
            .any(|v| v["path"] == forks.join("active/base").to_str().unwrap()),
        "{envelope}"
    );
}

fn complete_tree(
    root: &std::path::Path,
) -> std::collections::BTreeMap<std::path::PathBuf, (u64, Vec<u8>)> {
    fn walk(
        root: &std::path::Path,
        path: &std::path::Path,
        out: &mut std::collections::BTreeMap<std::path::PathBuf, (u64, Vec<u8>)>,
    ) {
        let facts = std::fs::symlink_metadata(path).unwrap();
        let bytes = if facts.is_file() {
            std::fs::read(path).unwrap()
        } else if facts.file_type().is_symlink() {
            std::fs::read_link(path)
                .unwrap()
                .as_os_str()
                .as_encoded_bytes()
                .to_vec()
        } else {
            Vec::new()
        };
        out.insert(
            path.strip_prefix(root).unwrap().to_path_buf(),
            (facts.len(), bytes),
        );
        if facts.is_dir() {
            for entry in std::fs::read_dir(path).unwrap() {
                walk(root, &entry.unwrap().path(), out);
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(root, root, &mut out);
    out
}

#[test]
fn doctor_cli_preserves_empty_and_populated_trees_or_creates_lease_files() {
    for populated in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir(&home).unwrap();
        if populated {
            std::fs::create_dir_all(home.join(".agents/skills/alpha")).unwrap();
            std::fs::write(
                home.join(".agents/skills/alpha/SKILL.md"),
                "---\nname: alpha\ndescription: fixture\n---\n",
            )
            .unwrap();
            std::fs::create_dir_all(home.join(".history/leases")).unwrap();
            std::fs::write(
                home.join(".history/leases/sentinel"),
                "retained lease bytes",
            )
            .unwrap();
        }
        let before = complete_tree(dir.path());
        let output = Command::new(cli_bin())
            .args(["doctor", "--fixture", home.to_str().unwrap(), "--json"])
            .env("HOME", &home)
            .env("SKILL_STUDIO_TELEMETRY", "0")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(before, complete_tree(dir.path()));
    }
}
