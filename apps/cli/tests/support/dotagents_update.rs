#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::headless;

use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;

pub fn covered_update(surface: &str) {
    let home = tempfile::tempdir().unwrap();
    let agents = home.path().join(".agents");
    for name in ["alpha", "beta"] {
        let folder = agents.join("skills").join(name);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: fixture\n---\nOld {name}\n"),
        )
        .unwrap();
    }
    std::fs::write(agents.join("agents.toml"), "version = 1\n[[skills]]\nname = \"alpha\"\nsource = \"o/r\"\n[[skills]]\nname = \"beta\"\nsource = \"o/r\"\n").unwrap();
    let lock = |a: &str, b: &str| {
        format!("version = 1\n[skills.alpha]\nsource = \"o/r\"\nresolved_commit = \"{a}\"\n[skills.beta]\nsource = \"o/r\"\nresolved_commit = \"{b}\"\n")
    };
    std::fs::write(agents.join("agents.lock"), lock("old-alpha", "old-beta")).unwrap();
    let bin = home.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let npx = bin.join("npx");
    std::fs::write(&npx, format!("#!/bin/sh\nprintf 'install\\n' >> \"$HOME/installs\"\nfor name in alpha beta; do\n  printf '%s\\n' '---' \"name: $name\" 'description: fixture' '---' \"New $name\" > \"$HOME/.agents/skills/$name/SKILL.md\"\ndone\ncat > \"$HOME/.agents/agents.lock\" <<'LOCK'\n{}LOCK\n", lock("new-alpha", "new-beta"))).unwrap();
    std::fs::set_permissions(&npx, std::fs::Permissions::from_mode(0o755)).unwrap();
    let env = [(
        "PATH".to_string(),
        format!("{}:/usr/bin:/bin", bin.display()),
    )];
    let call = |args: &[&str], tool: &str, input: Value| {
        let response = match (surface, tool) {
            ("cli", "restore_event") => headless::cli(home.path(), args),
            ("cli", _) => headless::cli_with_env(home.path(), args, &env),
            (_, "restore_event") => {
                headless::mcp(home.path(), tool, &input)["result"]["structuredContent"].clone()
            }
            _ => headless::mcp_with_env(home.path(), tool, &input, &env)["result"]
                ["structuredContent"]
                .clone(),
        };
        assert_eq!(response["status"], "ok", "{response}");
        response["data"].clone()
    };
    let requests: Vec<_> = ["alpha", "beta"].map(|name| json!({"skill":name,"method":"dotagents","scope":{"scope":"global"},"source":"o/r"})).into();
    let updated = call(
        &[
            "update",
            "--method",
            "dotagents",
            "--source",
            "o/r",
            "--skill",
            "alpha",
            "--skill",
            "beta",
        ],
        "update_all",
        json!({"requests":requests}),
    );
    let items = updated["items"].as_array().unwrap();
    let first = items[0]["outcome"]["event_id"].as_str().unwrap();
    let covered = items[1]["outcome"]["event_id"].as_str().unwrap();
    assert_ne!(first, covered, "covered skill needs its own Undo");
    assert_eq!(
        std::fs::read_to_string(home.path().join("installs")).unwrap(),
        "install\n"
    );
    let events = call(&["events"], "list_events", json!({}));
    assert_eq!(
        events.as_array().unwrap().len(),
        2,
        "history exposes a whole-scope Undo"
    );
    call(
        &["restore", "--event-id", covered],
        "restore_event",
        json!({"event_id":covered}),
    );
    assert!(
        std::fs::read_to_string(agents.join("skills/alpha/SKILL.md"))
            .unwrap()
            .contains("New alpha")
    );
    assert!(std::fs::read_to_string(agents.join("skills/beta/SKILL.md"))
        .unwrap()
        .contains("Old beta"));
    let text = std::fs::read_to_string(agents.join("agents.lock")).unwrap();
    assert!(text.contains("new-alpha"));
    assert!(text.contains("old-beta"));
    assert!(!text.contains("new-beta"));
}
