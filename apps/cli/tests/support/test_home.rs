use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

pub struct TestHome(tempfile::TempDir);

impl Default for TestHome {
    fn default() -> Self {
        let home = Self(tempfile::tempdir().unwrap());
        assert_temporary_home(home.0.path());
        home
    }
}

impl TestHome {
    pub fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut command = Command::new(program);
        command
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", self.0.path())
            .env("SKILL_STUDIO_TELEMETRY", "0");
        command
    }
}

pub fn assert_temporary_home(home: &Path) {
    static OS_HOME: OnceLock<PathBuf> = OnceLock::new();
    let os_home = OS_HOME.get_or_init(|| {
        // With HOME unset, the shell resolves ~ from the OS user record.
        let output = Command::new("/bin/sh")
            .env_clear()
            .args(["-c", "printf '%s' ~"])
            .output()
            .expect("resolve OS user home");
        assert!(output.status.success(), "resolve OS user home");
        PathBuf::from(String::from_utf8(output.stdout).unwrap())
            .canonicalize()
            .expect("canonical OS user home")
    });
    let home = home.canonicalize().expect("test HOME must exist");
    assert_ne!(&home, os_home, "tests must refuse the real OS user home");
    let temporary_root = std::env::temp_dir().canonicalize().unwrap();
    assert!(
        home.starts_with(&temporary_root) && home != temporary_root,
        "test HOME must be a private temporary directory: {}",
        home.display()
    );
}

#[test]
fn setup_refuses_the_os_user_home() {
    let home = Command::new("/bin/sh")
        .env_clear()
        .args(["-c", "printf '%s' ~"])
        .output()
        .unwrap();
    let home = PathBuf::from(String::from_utf8(home.stdout).unwrap());
    assert!(std::panic::catch_unwind(|| assert_temporary_home(&home)).is_err());
}

#[test]
fn setup_accepts_a_temporary_home() {
    let home = tempfile::tempdir().unwrap();
    assert_temporary_home(home.path());
}

#[test]
fn child_uses_a_temporary_home_without_inherited_config() {
    let home = TestHome::default();
    let output = home
        .command("/bin/sh")
        .args([
            "-c",
            "printf '%s' \"$HOME\"; test -z \"$CODEX_HOME$XDG_CONFIG_HOME$XDG_DATA_HOME\"",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let child_home = PathBuf::from(String::from_utf8(output.stdout).unwrap());
    assert_eq!(child_home, home.0.path());
    assert_temporary_home(&child_home);
}
