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
