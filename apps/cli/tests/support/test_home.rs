use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

pub struct TestHome(pub tempfile::TempDir);

impl Default for TestHome {
    fn default() -> Self {
        let home = Self(tempfile::tempdir().unwrap());
        assert_temporary_home(home.0.path());
        home
    }
}

impl TestHome {
    pub fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        command(program, self.0.path())
    }
}

pub fn command(program: impl AsRef<std::ffi::OsStr>, home: impl AsRef<Path>) -> Command {
    assert_temporary_home(home.as_ref());
    let mut command = Command::new(program);
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", home.as_ref())
        .env("SKILL_STUDIO_TELEMETRY", "0");
    command
}

pub fn os_user_home() -> &'static Path {
    static OS_HOME: OnceLock<PathBuf> = OnceLock::new();
    OS_HOME.get_or_init(|| {
        let output = Command::new("/bin/sh")
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .args(["-c", "if command -v getent >/dev/null 2>&1; then getent passwd \"$(id -u)\" | cut -d: -f6; else printf '%s\\n' ~; fi"])
            .output()
            .expect("resolve OS user home");
        assert!(output.status.success(), "resolve OS user home");
        let home = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        assert!(home.is_absolute(), "OS user home must be an absolute path");
        home
    })
}

pub fn assert_not_os_home(home: &Path, os_home: &Path) {
    let home = home.canonicalize().unwrap_or_else(|_| home.to_path_buf());
    let os_home = os_home
        .canonicalize()
        .unwrap_or_else(|_| os_home.to_path_buf());
    assert!(home != os_home, "tests must refuse the real OS user home");
}

pub fn assert_temporary_home(home: &Path) {
    assert_not_os_home(home, os_user_home());
    let home = home.canonicalize().expect("test HOME must exist");
    let temporary_root = std::env::temp_dir().canonicalize().unwrap();
    assert!(
        home.starts_with(&temporary_root) && home != temporary_root,
        "test HOME must be a private temporary directory: {}",
        home.display()
    );
}
