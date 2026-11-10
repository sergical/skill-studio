use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

pub struct Fixture {
    _home: tempfile::TempDir,
    _project: tempfile::TempDir,
    pub home: PathBuf,
    pub project: PathBuf,
}

pub fn fixture() -> Option<Fixture> {
    let home_dir = tempfile::tempdir().unwrap();
    let home = home_dir.path().canonicalize().unwrap();
    let volume = std::env::var_os("SKILL_STUDIO_TEST_VOLUME")
        .map(PathBuf::from)
        .or_else(|| cfg!(target_os = "linux").then(|| PathBuf::from("/dev/shm")))?;
    let facts = std::fs::metadata(&volume).ok()?;
    if facts.dev() == std::fs::metadata(&home).unwrap().dev() {
        return None;
    }
    let project_dir = tempfile::Builder::new()
        .prefix("park-volume-")
        .tempdir_in(volume)
        .unwrap();
    let project = project_dir.path().canonicalize().unwrap();
    std::fs::create_dir(project.join(".git")).unwrap();
    let root = project.join(".codex/skills/volume-copy");
    std::fs::create_dir_all(root.join("scripts")).unwrap();
    std::fs::write(
        root.join("SKILL.md"),
        "---\nname: volume-copy\ndescription: Volume fixture\n---\nOriginal body.\n",
    )
    .unwrap();
    std::fs::write(root.join("reference.md"), "Original reference bytes.\n").unwrap();
    std::fs::write(root.join("scripts/run.sh"), "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o750)).unwrap();
    std::fs::set_permissions(root.join("scripts"), std::fs::Permissions::from_mode(0o500)).unwrap();
    std::fs::set_permissions(
        root.join("scripts/run.sh"),
        std::fs::Permissions::from_mode(0o751),
    )
    .unwrap();
    Some(Fixture {
        _home: home_dir,
        _project: project_dir,
        home,
        project,
    })
}

pub fn bytes(root: &Path) -> Vec<Vec<u8>> {
    ["SKILL.md", "reference.md", "scripts/run.sh"]
        .iter()
        .map(|path| std::fs::read(root.join(path)).unwrap())
        .collect()
}

pub fn modes(root: &Path) -> Vec<u32> {
    [
        root.to_path_buf(),
        root.join("scripts"),
        root.join("scripts/run.sh"),
    ]
    .iter()
    .map(|path| std::fs::metadata(path).unwrap().permissions().mode() & 0o7777)
    .collect()
}
