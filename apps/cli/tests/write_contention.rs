#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[allow(dead_code)]
#[path = "support/test_home.rs"]
mod test_home;

use serde_json::Value;
use skill_studio_core::ports::{LeaseKey, LeaseMode, LeaseProvider};
use skill_studio_host::FileLease;
use std::path::Path;
use std::time::Duration;

fn run(home: &Path, args: &[&str]) -> (i32, Value) {
    let output = test_home::command(env!("CARGO_BIN_EXE_skill-studio"), home)
        .args(args)
        .args([
            "--home",
            home.to_str().unwrap(),
            "--json",
            "--write-timeout-ms",
            "100",
            "--read-timeout-ms",
            "100",
        ])
        .output()
        .unwrap();
    (
        output.status.code().unwrap(),
        serde_json::from_slice(&output.stdout).unwrap(),
    )
}

#[test]
fn copy_and_undo_refuse_another_process_lease_then_succeed_after_release() {
    let directory = tempfile::tempdir().unwrap();
    let home = directory.path().canonicalize().unwrap();
    let source = home.join(".agents/skills/solo");
    std::fs::create_dir_all(&source).unwrap();
    let bytes = "---\nname: solo\ndescription: Fixture\n---\nKeep these bytes.\n";
    std::fs::write(source.join("SKILL.md"), bytes).unwrap();
    let (status, scan) = run(&home, &["scan"]);
    assert_eq!(status, 0, "{scan}");
    let id = scan["data"]["skills"][0]["deployments"][0]["id"]
        .as_str()
        .unwrap();
    let copy_args = ["split", "--deployment-id", id, "--harness", "codex"];
    let lease = FileLease::new(home.join(".skill-studio/leases"));
    let key = LeaseKey {
        canonical_root: home.clone(),
    };
    let guard = lease
        .acquire(
            std::slice::from_ref(&key),
            LeaseMode::Exclusive,
            Duration::ZERO,
        )
        .unwrap();
    let (status, busy) = run(&home, &copy_args);
    assert_eq!(status, 3, "{busy}");
    assert_eq!(busy["errors"][0]["code"], "scope_busy");
    assert_eq!(
        std::fs::read_to_string(source.join("SKILL.md")).unwrap(),
        bytes
    );
    let copy = home.join(".codex/skills/solo");
    assert!(!copy.exists());
    drop(guard);
    let (status, copied) = run(&home, &copy_args);
    assert_eq!(status, 0, "{copied}");
    assert!(!source.exists());
    assert_eq!(
        std::fs::read_to_string(copy.join("SKILL.md")).unwrap(),
        bytes
    );
    let (status, before) = run(&home, &["events"]);
    assert_eq!(status, 0, "{before}");
    let guard = lease
        .acquire(&[key], LeaseMode::Exclusive, Duration::ZERO)
        .unwrap();
    let (status, busy) = run(&home, &["undo"]);
    assert_eq!(status, 3, "{busy}");
    assert_eq!(busy["errors"][0]["code"], "scope_busy");
    assert!(!source.exists());
    assert_eq!(
        std::fs::read_to_string(copy.join("SKILL.md")).unwrap(),
        bytes
    );
    drop(guard);
    let (status, after) = run(&home, &["events"]);
    assert_eq!(status, 0, "{after}");
    assert_eq!(
        after["data"], before["data"],
        "busy undo must not add an event"
    );
    let (status, undone) = run(&home, &["undo"]);
    assert_eq!(status, 0, "{undone}");
    assert_eq!(
        std::fs::read_to_string(source.join("SKILL.md")).unwrap(),
        bytes
    );
    assert!(!copy.exists());
}
