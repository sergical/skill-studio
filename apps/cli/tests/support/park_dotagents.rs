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
/bin/cat > "$DOTAGENTS_HOME/agents.toml" <<'TOML'
[[skills]]
name = "*"
source = "other/repo"
exclude = ["foo"]
TOML
: > "$DOTAGENTS_HOME/agents.lock"
/bin/cat > "$DOTAGENTS_HOME/.gitignore" <<'IGNORE'
!/skills/foo/
IGNORE
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
