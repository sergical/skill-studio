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
