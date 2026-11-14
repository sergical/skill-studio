#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Given a pack update that reached the library but not every agent copy,
//! when the user asks which copies are old, one scan names each agent copy
//! as link, same, differs or stale; fails if an old copy is missed or a
//! current one is flagged.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use skill_studio_core::dto::{CopyStatus, Inventory, ScanRequest};
use skill_studio_core::identity::AgentId;
use skill_studio_core::ops;
use skill_studio_core::ports::Runtime;
use skill_studio_core::scope::RuntimeScope;
use skill_studio_core::testing::golden::{ctx, materialized_ports, unique_temp_dir};
use skill_studio_host::{FileLease, RealFs};

const UPDATED_AT: &str = "2026-01-02T00:00:00Z";
const BEFORE_UPDATE: u64 = 1_767_225_600; // 2026-01-01T00:00:00Z

fn skill(home: &Path, folder: &str, name: &str, body: &str) {
    let dir = home.join(folder).join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: A pack skill.\n---\n{body}\n"),
    )
    .unwrap();
}

fn set_old(file: &Path) {
    std::fs::File::options()
        .write(true)
        .open(file)
        .unwrap()
        .set_modified(UNIX_EPOCH + Duration::from_secs(BEFORE_UPDATE))
        .unwrap();
}

fn scan(request: &ScanRequest) -> Inventory {
    let dir = unique_temp_dir("scan-copy-status");
    std::fs::create_dir_all(&dir).unwrap();
    let home = dir.canonicalize().unwrap();
    for name in ["linked", "copied", "old", "edited", "huge"] {
        skill(&home, ".agents/skills", name, "Version 2.");
    }
    std::fs::create_dir_all(home.join(".claude/skills")).unwrap();
    std::os::unix::fs::symlink(
        "../../.agents/skills/linked",
        home.join(".claude/skills/linked"),
    )
    .unwrap();
    skill(&home, ".codex/skills", "copied", "Version 2.");
    skill(&home, ".codex/skills", "old", "Version 1.");
    set_old(&home.join(".codex/skills/old/SKILL.md"));
    // More files than the scan reads: their times are unknown, so the copy
    // cannot be called stale even though every file read is old.
    skill(&home, ".codex/skills", "huge", "Version 1.");
    set_old(&home.join(".codex/skills/huge/SKILL.md"));
    for index in 0..2_000 {
        let file = home.join(format!(".codex/skills/huge/notes-{index:04}.md"));
        std::fs::write(&file, "old").unwrap();
        set_old(&file);
    }
    skill(
        &home,
        ".cursor/skills",
        "edited",
        "Version 2 with my own step.",
    );
    skill(&home, ".codex/skills", "agent-only", "Only here.");
    assert!(
        SystemTime::now() > UNIX_EPOCH + Duration::from_secs(BEFORE_UPDATE + 86_400),
        "the edited copy must be newer than the lock entry"
    );
    let entries: Vec<String> = ["linked", "copied", "old", "edited", "huge"]
        .iter()
        .map(|name| {
            format!(
                r#""{name}":{{"source":"acme/pack","sourceType":"github","sourceUrl":"https://github.com/acme/pack","skillFolderHash":"hash","installedAt":"{UPDATED_AT}","updatedAt":"{UPDATED_AT}"}}"#
            )
        })
        .collect();
    std::fs::write(
        home.join(".agents/.skill-lock.json"),
        format!(r#"{{"version":3,"skills":{{{}}}}}"#, entries.join(",")),
    )
    .unwrap();

    let ports = materialized_ports(
        Arc::new(RealFs::new()),
        Arc::new(FileLease::new(home.join(".leases"))),
    );
    let runtime = Runtime::new(&RuntimeScope::fixture(&home), ports).unwrap();
    let inventory = ops::scan(&runtime, &ctx(), request).unwrap();
    std::fs::remove_dir_all(&home).unwrap();
    inventory
}

fn statuses(inventory: &Inventory) -> Vec<(String, Option<AgentId>, Option<CopyStatus>)> {
    let mut rows: Vec<_> = inventory
        .skills
        .iter()
        .flat_map(|skill| {
            skill
                .deployments
                .iter()
                .map(|d| (skill.name.0.clone(), d.harness.clone(), d.copy_status))
        })
        .collect();
    rows.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    rows
}

fn agent(id: &str) -> Option<AgentId> {
    AgentId::parse(id).ok()
}

#[test]
fn scan_names_each_agent_copy_against_the_library_or_hides_an_old_copy() {
    let rows = statuses(&scan(&ScanRequest::default()));
    let agent_rows: Vec<_> = rows.iter().filter(|row| row.1.is_some()).cloned().collect();
    assert_eq!(
        agent_rows,
        vec![
            ("agent-only".into(), agent(AgentId::CODEX), None),
            (
                "copied".into(),
                agent(AgentId::CODEX),
                Some(CopyStatus::Same)
            ),
            (
                "edited".into(),
                agent(AgentId::CURSOR),
                Some(CopyStatus::Differs)
            ),
            (
                "huge".into(),
                agent(AgentId::CODEX),
                Some(CopyStatus::Differs)
            ),
            (
                "linked".into(),
                agent(AgentId::CLAUDE_CODE),
                Some(CopyStatus::Link)
            ),
            ("old".into(), agent(AgentId::CODEX), Some(CopyStatus::Stale)),
        ]
    );
    assert!(
        rows.iter()
            .filter(|row| row.1.is_none())
            .all(|row| row.2.is_none()),
        "library copies have no copy status: {rows:?}"
    );
}

#[test]
fn scan_with_a_copy_status_filter_keeps_only_those_copies_or_lists_the_rest() {
    let inventory = scan(&ScanRequest {
        copy_status: Some(CopyStatus::Stale),
        ..Default::default()
    });
    assert_eq!(
        statuses(&inventory),
        vec![("old".into(), agent(AgentId::CODEX), Some(CopyStatus::Stale))]
    );
    assert_eq!(inventory.skills[0].source, "acme/pack");
    assert!(inventory.lock_only.is_empty());
}
