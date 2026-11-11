use std::path::{Path, PathBuf};

pub fn skill(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        "---\nname: foo\ndescription: Display fixture\n---\nBody.\n",
    )
    .unwrap();
}

pub fn project(home: &Path, owner: &str) -> PathBuf {
    let project = home.join("project");
    std::fs::create_dir_all(project.join(".git")).unwrap();
    skill(&project.join(".agents/skills/foo"));
    if owner != "in_repo" {
        let name = if owner == "dotagents" { "foo" } else { "*" };
        std::fs::write(
            project.join("agents.toml"),
            format!("version = 1\n\n[[skills]]\nname = \"{name}\"\nsource = \"owner/pack\"\n"),
        )
        .unwrap();
        std::fs::write(
            project.join("agents.lock"),
            "version = 1\n\n[skills.foo]\nsource = \"owner/pack\"\n",
        )
        .unwrap();
    }
    project
}

/// A `dotagents` that does what `dotagents --project remove foo -y` does to
/// the fixture, so Park never reaches the real `npx` package.
#[allow(dead_code)]
pub fn fake_dotagents(home: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = home.join("fake-bin");
    std::fs::create_dir_all(&bin).unwrap();
    let script = bin.join("dotagents");
    std::fs::write(
        &script,
        "#!/bin/sh\nif /usr/bin/grep -q 'name = \"\\*\"' agents.toml; then\n  printf 'version = 1\\n\\n[[skills]]\\nname = \"*\"\\nsource = \"owner/pack\"\\nexclude = [\"foo\"]\\n' > agents.toml\nelse\n  printf 'version = 1\\n' > agents.toml\nfi\nprintf 'version = 1\\n' > agents.lock\n",
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    script
}

#[allow(dead_code)]
pub fn path_with_fake_dotagents(home: &Path) -> String {
    let script = fake_dotagents(home);
    let mut paths = vec![script.parent().unwrap().to_path_buf()];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    std::env::join_paths(paths).unwrap().into_string().unwrap()
}
