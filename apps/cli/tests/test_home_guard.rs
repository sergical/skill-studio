#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The guard every CLI and MCP test binary uses for its temporary HOME:
//! it refuses the OS user's own home and runs children without inherited
//! agent config. These tests live here once, not in the shared support
//! module that 24 test binaries include.

#[path = "support/test_home.rs"]
mod test_home;

use std::path::PathBuf;

fn assert_home_refused(result: std::thread::Result<()>) {
    let panic = result.unwrap_err();
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap();
    assert!(
        message.contains("tests must refuse the real OS user home"),
        "{message}"
    );
}

#[test]
fn setup_refuses_the_os_user_home() {
    assert_home_refused(std::panic::catch_unwind(|| {
        test_home::assert_temporary_home(test_home::os_user_home());
    }));
}

#[test]
fn setup_accepts_a_temporary_home() {
    let home = tempfile::tempdir().unwrap();
    test_home::assert_temporary_home(home.path());
}

#[test]
fn child_uses_a_temporary_home_without_inherited_config() {
    let home = test_home::TestHome::default();
    let output = home
        .command("/bin/sh")
        .args([
            "-c",
            "printf '%s' \"$HOME\"; test -z \"$CODEX_HOME$CLAUDE_CONFIG_DIR$PI_CODING_AGENT_DIR$GROK_HOME$XDG_CONFIG_HOME$XDG_DATA_HOME$XDG_STATE_HOME$DOTAGENTS_HOME$SKILL_STUDIO_HOME$SKILL_STUDIO_FIXTURE\"",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let child_home = PathBuf::from(String::from_utf8(output.stdout).unwrap());
    assert_eq!(child_home, home.0.path());
    test_home::assert_temporary_home(&child_home);
}

#[test]
fn setup_refuses_a_missing_os_home_for_its_identity() {
    let temporary = tempfile::tempdir().unwrap();
    let missing = temporary.path().join("missing-user-home");
    assert_home_refused(std::panic::catch_unwind(|| {
        test_home::assert_not_os_home(&missing, &missing);
    }));
}
