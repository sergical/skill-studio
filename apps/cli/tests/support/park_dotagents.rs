use std::os::unix::fs::PermissionsExt;
use std::path::Path;

pub fn setup(home: &Path) -> String {
    let agents = home.join(".agents");
    std::fs::create_dir_all(agents.join("skills/foo")).unwrap();
    std::fs::create_dir_all(home.join("bin")).unwrap();
    std::fs::write(
        agents.join("skills/foo/SKILL.md"),
        "---\nname: foo\ndescription: Fixture skill\n---\nFixture\n",
    )
    .unwrap();
    std::fs::write(agents.join("agents.toml"),
        "[[skills]]\nname = \"foo\"\nsource = \"owner/foo\"\n\n[[skills]]\nname = \"*\"\nsource = \"other/repo\"\nexclude = []\n").unwrap();
    std::fs::write(
        agents.join("agents.lock"),
        "[skills.foo]\nsource = \"owner/foo\"\n",
    )
    .unwrap();
    std::fs::write(agents.join(".gitignore"), "/skills/foo\n!/skills/foo/\n").unwrap();
    let script = home.join("bin/dotagents");
    std::fs::write(
        &script,
        r#"#!/bin/sh
echo called > "$HOME/remove-called"
exit 79
"#,
    )
    .unwrap();
    std::fs::set_permissions(script, std::fs::Permissions::from_mode(0o755)).unwrap();
    home.join("bin").display().to_string()
}

pub fn config_bytes(home: &Path) -> Vec<Vec<u8>> {
    ["agents.toml", "agents.lock", ".gitignore"]
        .iter()
        .map(|file| std::fs::read(home.join(".agents").join(file)).unwrap())
        .collect()
}

pub fn make_lock_read_only(home: &Path) {
    let dir = home.join("readonly");
    std::fs::create_dir(&dir).unwrap();
    let lock = home.join(".agents/agents.lock");
    std::fs::rename(&lock, dir.join("agents.lock")).unwrap();
    std::os::unix::fs::symlink(dir.join("agents.lock"), lock).unwrap();
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o555)).unwrap();
}

pub fn restore_lock_permissions(home: &Path) {
    std::fs::set_permissions(
        home.join("readonly"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
}

pub fn layouts() -> Vec<String> {
    let table: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../../crates/skill-studio-core/tests/fixtures/park-dotagents-layouts.json"
    ))
    .unwrap();
    let mut variants = Vec::new();
    for row in table {
        for crlf in [false, true] {
            for final_newline in [false, true] {
                let mut text = row["manifest"]
                    .as_str()
                    .unwrap()
                    .trim_end_matches('\n')
                    .to_string();
                if final_newline {
                    text.push('\n');
                }
                if crlf {
                    text = text.replace('\n', "\r\n");
                }
                variants.push(text);
            }
        }
    }
    variants
}

pub fn write_layout(home: &Path, text: &str) {
    let source = if text.contains("owner/pack") {
        "owner/pack"
    } else {
        "owner/foo"
    };
    std::fs::write(home.join(".agents/agents.toml"), text).unwrap();
    std::fs::write(
        home.join(".agents/agents.lock"),
        format!("[skills.foo]\nsource = \"{source}\"\n"),
    )
    .unwrap();
}
