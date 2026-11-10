#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

#[path = "support/park_display.rs"]
mod fixture;

fn command(home: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_skill-studio"));
    cmd.env("HOME", home).env("SKILL_STUDIO_HOME", home);
    cmd
}
fn run(home: &Path, args: &[&str]) -> Value {
    let output = command(home)
        .args(args)
        .arg("--home")
        .arg(home)
        .arg("--json")
        .output()
        .unwrap();
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("{args:?}: {e}: {}", String::from_utf8_lossy(&output.stderr)))
}
fn deployment(inventory: &Value) -> Value {
    inventory["skills"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "foo")
        .unwrap()["deployments"][0]
        .clone()
}
fn golden(name: &str, value: &Value) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);
    let expected: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(value, &expected);
}
fn watch(home: &Path, project: &str) -> Value {
    let mut child = command(home)
        .args(["watch", "--project", project, "--json"])
        .arg("--home")
        .arg(home)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        let line = BufReader::new(stdout).lines().next();
        let _ = tx.send(line);
    });
    let first = rx.recv_timeout(std::time::Duration::from_mins(1));
    child.kill().ok();
    child.wait().unwrap();
    reader.join().unwrap();
    serde_json::from_str(
        &first
            .expect("watch did not emit an initial snapshot")
            .unwrap()
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn cli_park_scan_watch_show_project_and_original_type_or_fail_the_display_golden() {
    let mut rows = Vec::new();
    for owner in ["dotagents", "wildcard_dotagents", "in_repo"] {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().canonicalize().unwrap();
        let project = fixture::project(&home, owner);
        let project = project.to_str().unwrap();
        let original = deployment(&run(&home, &["scan", "--project", project])["data"]);
        let parked = run(&home, &["park", "foo", "--project", project]);
        assert_eq!(parked["status"], "ok", "{parked}");
        let current = deployment(&run(&home, &["scan", "--project", project])["data"]);
        let watched = deployment(&watch(&home, project)["inventory"]);
        let physically_in_git = Path::new(current["path"].as_str().unwrap())
            .ancestors()
            .any(|path| path.join(".git").exists());
        let output = command(&home)
            .args(["scan", "--project", project])
            .arg("--home")
            .arg(&home)
            .output()
            .unwrap();
        let human = String::from_utf8(output.stdout).unwrap();
        rows.push(json!({
            "owner": current["owner_kind"],
            "physical_git_location_matches": current["in_git_repo"] == json!(physically_in_git),
            "source_matches_original": current["source_kind"] == original["source_kind"],
            "owner_id_matches_original": current["owner_id"] == original["owner_id"],
            "project_matches_original": current["parked_origin"]["scope"] == original["root"]["scope"],
            "watch_matches_scan": watched["owner_kind"] == current["owner_kind"] && watched["parked_origin"] == current["parked_origin"],
            "human": human.replace(current["id"].as_str().unwrap(), "$ID").replace(current["path"].as_str().unwrap(), "$PARKED").replace(project, "$PROJECT"),
        }));
    }
    golden("park-project-display.json", &json!(rows));
}

#[test]
fn cli_turn_on_claude_copy_names_the_restored_id_or_fails_the_display_golden() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().canonicalize().unwrap();
    fixture::skill(&home.join(".claude/skills/foo"));
    let original = deployment(&run(&home, &["scan"])["data"]);
    assert_eq!(run(&home, &["park", "foo"])["status"], "ok");
    let result = run(&home, &["unpark", "foo"]);
    golden(
        "unpark-restored-display.json",
        &json!({"status":result["status"], "id_matches_restored_copy":result["data"]["deployment_id"] == original["id"], "path_matches_restored_copy":result["data"]["restored_path"] == original["path"]}),
    );
}

#[test]
fn cli_park_plugin_by_name_gives_the_id_guidance_or_fails_the_display_golden() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().canonicalize().unwrap();
    let (_, fixture) = skill_studio_core::testing::fixtures::all()
        .into_iter()
        .find(|(n, _)| *n == "plugin_cache")
        .unwrap();
    fixture.materialize(&home).unwrap();
    let scan = run(&home, &["scan"]);
    let plugin = &scan["data"]["skills"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "audit-code")
        .unwrap()["deployments"][0];
    let by_name = run(&home, &["park", "audit-code"]);
    let by_id = run(&home, &["park", "--id", plugin["id"].as_str().unwrap()]);
    golden(
        "park-plugin-display.json",
        &json!({"message":by_name["errors"][0]["message"], "matches_id":by_name["errors"][0]["message"] == by_id["errors"][0]["message"]}),
    );
}

#[test]
fn cli_copy_update_after_split_points_to_the_skill_page_or_fails_the_display_golden() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().canonicalize().unwrap();
    fixture::skill(&home.join(".claude/skills/foo"));
    fixture::skill(&home.join("source"));
    let result = run(
        &home,
        &[
            "update",
            "--skill",
            "foo",
            "--method",
            "copy",
            "--source-dir",
            home.join("source").to_str().unwrap(),
        ],
    );
    golden(
        "update-split-display.json",
        &json!({"message":result["errors"][0]["message"]}),
    );
}
