#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Real-disk tests for `ops::update_split_copies`: one fetched version
//! written to every live copy `ops::split` made.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use skill_studio_core::dto::{InstallFile, ScanRequest, SplitRequest};
use skill_studio_core::harness::HarnessCatalog;
use skill_studio_core::identity::{AgentId, RootKind, RootScope, SkillName};
use skill_studio_core::ops;
use skill_studio_core::ports::{Ports, Runtime, ScopeFs};
use skill_studio_core::scope::RuntimeScope;
use skill_studio_core::testing::golden::{ctx, unique_temp_dir};
use skill_studio_core::testing::{FailingFs, FakeClock, RecordingSink};
use skill_studio_core::SplitCopiesUpdate;

use skill_studio_host::{FileLease, RealFs, SqliteHistoryOpener, UlidIds};

const HARNESS_DIRS: [&str; 3] = [".claude/skills", ".codex/skills", ".pi/agent/skills"];

fn copy_of(home: &Path, dir: &str) -> PathBuf {
    home.join(dir).join("gamma")
}

fn skill_md(revision: &str) -> Vec<u8> {
    format!("---\nname: gamma\ndescription: a skill to split\n---\nBody at {revision}.\n")
        .into_bytes()
}

fn runtime_with(home: &Path, fs: Arc<dyn ScopeFs>) -> Runtime {
    let ports = Ports {
        fs,
        clock: Arc::new(FakeClock::at(0)),
        ids: Arc::new(UlidIds::new()),
        leases: Arc::new(FileLease::new(home.join(".leases"))),
        history: Arc::new(SqliteHistoryOpener::new(
            home.join(".history").join("events.sqlite3"),
        )),
        sink: Arc::new(RecordingSink::default()),
        spawner: None,
        discovery: None,
        tools: None,
        catalog: Arc::new(HarnessCatalog::builtin()),
        telemetry: Arc::new(skill_studio_core::ports::NoopTelemetry),
    };
    Runtime::new(&RuntimeScope::fixture(home), ports).unwrap()
}

/// A home whose Universal skill `gamma` (at revision v1) was split into
/// Claude Code, Codex and pi copies.
fn split_home(name: &str) -> PathBuf {
    let home = unique_temp_dir(name);
    let dir = home.join(".agents/skills/gamma");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("SKILL.md"), skill_md("v1")).unwrap();
    write_lock(&home);
    let rt = runtime_with(&home, Arc::new(RealFs::new()));
    let id = ops::scan(&rt, &ctx(), &ScanRequest::default())
        .unwrap()
        .skills
        .iter()
        .find(|s| s.name.0 == "gamma")
        .unwrap()
        .deployments
        .iter()
        .find(|d| d.root.kind == RootKind::Universal)
        .unwrap()
        .id
        .clone();
    ops::split(
        &rt,
        &ctx(),
        &SplitRequest {
            deployment_id: id,
            harnesses: ["claude-code", "codex", "pi"]
                .iter()
                .map(|h| AgentId::parse(h).unwrap())
                .collect(),
        },
    )
    .unwrap();
    home
}

fn v2() -> SplitCopiesUpdate {
    SplitCopiesUpdate {
        skill: SkillName("gamma".to_string()),
        scope: RootScope::Global,
        files: vec![InstallFile {
            relative_path: PathBuf::from("SKILL.md"),
            contents: skill_md("v2"),
            mode: None,
        }],
        lock_folder_hash: Some("tree-v2".to_string()),
    }
}

/// `v1` or `v2` when the copy holds one whole version, `None` when it is
/// missing or holds neither.
fn revision_of(path: &Path) -> Option<&'static str> {
    let text = std::fs::read_to_string(path.join("SKILL.md")).ok()?;
    if text.contains("Body at v2") {
        Some("v2")
    } else if text.contains("Body at v1") {
        Some("v1")
    } else {
        None
    }
}

/// Flow: split into three copies, then update from one fetched version.
/// Expect all three at v2 and no shared `~/.agents/skills/gamma`. Catches an
/// update that skips a copy or recreates the shared folder.
#[test]
fn update_writes_the_new_version_to_all_three_split_copies_and_no_shared_folder() {
    let home = split_home("split_update_all");
    let rt = runtime_with(&home, Arc::new(RealFs::new()));

    let outcome = ops::update_split_copies(&rt, &ctx(), &v2()).unwrap();

    assert_eq!(outcome.updated.len(), 3, "{outcome:?}");
    for dir in HARNESS_DIRS {
        assert_eq!(revision_of(&copy_of(&home, dir)), Some("v2"), "{dir}");
    }
    assert!(!home.join(".agents/skills/gamma").exists());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: park the Codex copy (its folder moves away), then update. Expect
/// the parked folder unchanged and nothing recreated at the Codex place,
/// the other two at v2. Catches an update that writes into a parked copy's
/// old place and turns the agent back on.
#[test]
fn update_leaves_a_parked_split_copy_untouched() {
    let home = split_home("split_update_parked");
    let parked = home.join(".agents/skills-parked/universal/gamma");
    std::fs::create_dir_all(parked.parent().unwrap()).unwrap();
    std::fs::rename(copy_of(&home, ".codex/skills"), &parked).unwrap();
    let rt = runtime_with(&home, Arc::new(RealFs::new()));

    let outcome = ops::update_split_copies(&rt, &ctx(), &v2()).unwrap();

    assert_eq!(outcome.updated.len(), 2, "{outcome:?}");
    assert_eq!(revision_of(&parked), Some("v1"));
    assert!(!copy_of(&home, ".codex/skills").exists());
    assert_eq!(revision_of(&copy_of(&home, ".claude/skills")), Some("v2"));
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: edit the pi copy after the split, then update. Expect the pi copy
/// refused with a reason and keeping its edit, the other two at v2. Catches
/// an update that overwrites a user's local edits.
#[test]
fn update_refuses_a_split_copy_with_local_edits_and_updates_the_rest() {
    let home = split_home("split_update_edited");
    let pi = copy_of(&home, ".pi/agent/skills");
    std::fs::write(
        pi.join("SKILL.md"),
        b"---\nname: gamma\ndescription: mine\n---\nEdited.\n",
    )
    .unwrap();
    let rt = runtime_with(&home, Arc::new(RealFs::new()));

    let outcome = ops::update_split_copies(&rt, &ctx(), &v2()).unwrap();

    assert_eq!(outcome.updated.len(), 2, "{outcome:?}");
    assert_eq!(outcome.refused.len(), 1, "{outcome:?}");
    assert_eq!(outcome.refused[0].0, pi);
    assert!(std::fs::read_to_string(pi.join("SKILL.md"))
        .unwrap()
        .contains("Edited."));
    assert_eq!(revision_of(&copy_of(&home, ".claude/skills")), Some("v2"));
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: a rename fails at each step of the three-copy update. Expect every
/// copy to hold wholly v1 or wholly v2 (never missing or mixed), and a
/// second update to bring all three to v2 with nothing refused. Catches a
/// failure that leaves a copy half-written, or skips recording the hash of a
/// copy that did swap so the retry refuses it as edited.
#[test]
fn update_failing_at_any_step_leaves_each_copy_wholly_old_or_new_and_a_retry_finishes() {
    for n in 1..=14 {
        let home = split_home(&format!("split_update_fail_{n}"));
        let failing = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
        let rt = runtime_with(&home, failing.clone());
        failing.fail_nth_fsops_rename(n);

        let result = ops::update_split_copies(&rt, &ctx(), &v2());

        for dir in HARNESS_DIRS {
            assert!(
                revision_of(&copy_of(&home, dir)).is_some(),
                "step {n}: {dir} is missing or mixed after {result:?}"
            );
        }
        if let Ok(outcome) = &result {
            for (_, reason) in &outcome.refused {
                assert!(
                    reason.starts_with("could not write: "),
                    "step {n}: a write failure reads like an edit refusal: {reason}"
                );
            }
        }
        let retry = runtime_with(&home, Arc::new(RealFs::new()));
        let retried = ops::update_split_copies(&retry, &ctx(), &v2()).unwrap();
        assert!(
            retried.refused.is_empty(),
            "step {n}: a copy that did update lost its recorded hash: {retried:?}"
        );
        for dir in HARNESS_DIRS {
            assert_eq!(
                revision_of(&copy_of(&home, dir)),
                Some("v2"),
                "step {n} {dir}"
            );
        }
        std::fs::remove_dir_all(&home).ok();
    }
}

/// The lock file `npx skills` left, with fields the update must not touch.
fn lock_doc() -> serde_json::Value {
    serde_json::json!({
        "version": 3,
        "skills": {"gamma": {
            "source": "acme/skills", "sourceType": "github",
            "sourceUrl": "https://github.com/acme/skills",
            "skillPath": "skills/gamma/SKILL.md", "skillFolderHash": "tree-v1",
            "installedAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-02T00:00:00Z",
            "futureField": {"kept": true},
        }},
        "dismissed": {"tip": true},
    })
}

fn write_lock(home: &Path) {
    std::fs::write(
        home.join(".agents/.skill-lock.json"),
        lock_doc().to_string(),
    )
    .unwrap();
}

fn read_lock(home: &Path) -> serde_json::Value {
    let text = std::fs::read_to_string(home.join(".agents/.skill-lock.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

/// Rewrites every `copies` row of the home registry through `edit`.
fn edit_rows(home: &Path, edit: impl Fn(&str, &mut serde_json::Value)) {
    let path = home.join(".agents/skill-studio.json");
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    for (_, row) in doc["copies"].as_object_mut().unwrap() {
        let slot = row["slot"].as_str().unwrap().to_string();
        edit(&slot, row);
    }
    std::fs::write(&path, doc.to_string()).unwrap();
}

/// Flow: every live copy updates and the lock row has a tree SHA. Expect the
/// row's `skillFolderHash` to become the fetched SHA and every other key of
/// the lock file to stay as written. Catches an Update that
/// leaves the old hash, so the 6-hour update check offers the same update
/// again.
#[test]
fn a_full_split_update_writes_the_fetched_tree_hash_to_the_lock_row() {
    let home = split_home("split_update_lock_full");
    let rt = runtime_with(&home, Arc::new(RealFs::new()));

    ops::update_split_copies(&rt, &ctx(), &v2()).unwrap();

    let mut expected = lock_doc();
    expected["skills"]["gamma"]["skillFolderHash"] = serde_json::json!("tree-v2");
    assert_eq!(read_lock(&home), expected);
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: one copy was edited, so the update is partial. Expect the lock hash
/// to stay at the old value. Catches a partial update that marks the skill
/// current and hides the Update the edited copy still needs.
#[test]
fn a_partial_split_update_leaves_the_lock_hash_alone() {
    let home = split_home("split_update_lock_partial");
    std::fs::write(
        copy_of(&home, ".pi/agent/skills").join("SKILL.md"),
        b"---\nname: gamma\ndescription: mine\n---\nEdited.\n",
    )
    .unwrap();
    let rt = runtime_with(&home, Arc::new(RealFs::new()));

    ops::update_split_copies(&rt, &ctx(), &v2()).unwrap();

    assert_eq!(read_lock(&home), lock_doc());
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: a registry row points outside the agent's own skill folders. Expect
/// it refused with a reason and its folder untouched. Catches an Update that
/// swaps into a parent folder taken from an unchecked registry path.
#[test]
fn a_row_outside_the_agents_own_skill_folder_is_refused_and_not_written() {
    let home = split_home("split_update_stray_row");
    let stray = home.join("stray/gamma");
    std::fs::create_dir_all(stray.parent().unwrap()).unwrap();
    std::fs::rename(copy_of(&home, ".pi/agent/skills"), &stray).unwrap();
    edit_rows(&home, |slot, row| {
        if slot == "pi" {
            row["path"] = serde_json::json!(stray);
        }
    });
    let rt = runtime_with(&home, Arc::new(RealFs::new()));

    let outcome = ops::update_split_copies(&rt, &ctx(), &v2()).unwrap();

    assert_eq!(outcome.updated.len(), 2, "{outcome:?}");
    assert_eq!(outcome.refused.len(), 1, "{outcome:?}");
    assert!(outcome.refused[0].1.contains("own skill folders"));
    assert_eq!(revision_of(&stray), Some("v1"));
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: a copy's row is marked disabled. Expect it neither updated nor
/// listed as refused. Catches an Update that writes into a copy the user
/// turned off.
#[test]
fn a_disabled_split_row_is_skipped_without_a_refusal() {
    let home = split_home("split_update_disabled_row");
    edit_rows(&home, |slot, row| {
        if slot == "codex" {
            row["disabled"] = serde_json::json!(true);
        }
    });
    let rt = runtime_with(&home, Arc::new(RealFs::new()));

    let outcome = ops::update_split_copies(&rt, &ctx(), &v2()).unwrap();

    assert_eq!(outcome.updated.len(), 2, "{outcome:?}");
    assert!(outcome.refused.is_empty(), "{outcome:?}");
    assert_eq!(revision_of(&copy_of(&home, ".codex/skills")), Some("v1"));
    std::fs::remove_dir_all(&home).ok();
}

/// Flow: a same-name copy was installed from another repo (its row names a
/// different source, or none) and its bytes still match its recorded hash.
/// Expect it refused with a reason and untouched, the split copies updated,
/// and the lock row left as is. Catches an Update that overwrites a copy of
/// a different skill that only shares the name.
#[test]
fn a_same_name_copy_from_another_source_is_refused_and_not_overwritten() {
    let home = split_home("split_update_other_source");
    edit_rows(&home, |slot, row| match slot {
        "pi" => row["split_source"] = serde_json::json!("other/repo"),
        "codex" => {
            row.as_object_mut().unwrap().remove("split_source");
        }
        _ => {}
    });
    let rt = runtime_with(&home, Arc::new(RealFs::new()));

    let outcome = ops::update_split_copies(&rt, &ctx(), &v2()).unwrap();

    assert_eq!(outcome.updated.len(), 1, "{outcome:?}");
    assert_eq!(outcome.refused.len(), 2, "{outcome:?}");
    assert_eq!(revision_of(&copy_of(&home, ".pi/agent/skills")), Some("v1"));
    assert_eq!(revision_of(&copy_of(&home, ".codex/skills")), Some("v1"));
    assert_eq!(revision_of(&copy_of(&home, ".claude/skills")), Some("v2"));
    assert_eq!(read_lock(&home), lock_doc());
    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn updating_split_copies_records_an_undo_that_restores_files_and_registry_or_leaves_new_bytes() {
    let home = split_home("split_update_undo");
    let registry = home.join(".agents/skill-studio.json");
    let previous: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&registry).unwrap()).unwrap();
    let rt = runtime_with(&home, Arc::new(RealFs::new()));
    ops::update_split_copies(&rt, &ctx(), &v2()).unwrap();
    let event = ops::list_events(
        &rt,
        &ctx(),
        &skill_studio_core::dto::ListEventsRequest::default(),
    )
    .unwrap()
    .into_iter()
    .find(|event| event.kind == "update")
    .expect("split update has no Activity row to undo");
    let restore = skill_studio_core::dto::RestoreRequest {
        event_id: event.id,
        force: false,
    };
    let lock_path = home.join(".agents/.skill-lock.json");
    let updated_lock = std::fs::read(&lock_path).unwrap();
    let mut other_install = read_lock(&home);
    other_install["skills"]["gamma"]["skillFolderHash"] = serde_json::json!("edited");
    std::fs::write(&lock_path, other_install.to_string()).unwrap();
    let refused = ops::restore_event(&rt, &ctx(), &restore).unwrap_err();
    assert_eq!(
        refused.code,
        skill_studio_core::error::ErrorCode::DriftConflict
    );
    for dir in HARNESS_DIRS {
        assert_eq!(revision_of(&copy_of(&home, dir)), Some("v2"));
    }
    std::fs::write(&lock_path, updated_lock).unwrap();
    ops::restore_event(&rt, &ctx(), &restore).unwrap();
    for dir in HARNESS_DIRS {
        assert_eq!(revision_of(&copy_of(&home, dir)), Some("v1"), "{dir}");
    }
    let restored: serde_json::Value =
        serde_json::from_slice(&std::fs::read(registry).unwrap()).unwrap();
    assert_eq!(restored["copies"], previous["copies"]);
    assert_eq!(read_lock(&home), lock_doc());
    assert!(!home.join(".agents/skills/gamma").exists());
    std::fs::remove_dir_all(home).ok();
}

#[path = "support/edit_on_read.rs"]
mod edit_on_read;

fn latest_update(rt: &Runtime) -> skill_studio_core::identity::EventId {
    ops::list_events(
        rt,
        &ctx(),
        &skill_studio_core::dto::ListEventsRequest::default(),
    )
    .unwrap()
    .into_iter()
    .find(|e| e.kind == "update")
    .unwrap()
    .id
}

#[test]
fn split_update_then_undo_keeps_a_copy_edited_after_backup_or_overwrites_the_refused_edit() {
    let home = split_home("split_refused_undo");
    let path = copy_of(&home, ".claude/skills").join("SKILL.md");
    let edited = skill_md("editor");
    let fs = edit_on_read::EditOnRead {
        path: path.clone(),
        backups: home.join(".history/backups"),
        bytes: edited.clone(),
        reads: Default::default(),
        crash_after: None,
        after_v2: None,
    };
    let rt = runtime_with(&home, Arc::new(fs));
    let outcome = ops::update_split_copies(&rt, &ctx(), &v2()).unwrap();
    assert_eq!(outcome.refused.len(), 1, "{outcome:?}");
    assert_eq!(outcome.updated.len(), 2, "{outcome:?}");
    ops::restore_event(
        &rt,
        &ctx(),
        &skill_studio_core::dto::RestoreRequest {
            event_id: latest_update(&rt),
            force: false,
        },
    )
    .unwrap();
    assert_eq!(
        std::fs::read(&path).unwrap(),
        edited,
        "Undo overwrote the refused edit"
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn split_update_undo_redo_allows_the_next_update_or_the_registry_hash_refuses_unedited_copies() {
    let home = split_home("split_redo_registry");
    let rt = runtime_with(&home, Arc::new(RealFs::new()));
    ops::update_split_copies(&rt, &ctx(), &v2()).unwrap();
    let undo = ops::restore_event(
        &rt,
        &ctx(),
        &skill_studio_core::dto::RestoreRequest {
            event_id: latest_update(&rt),
            force: false,
        },
    )
    .unwrap();
    ops::restore_event(
        &rt,
        &ctx(),
        &skill_studio_core::dto::RestoreRequest {
            event_id: undo.restore_event_id,
            force: false,
        },
    )
    .unwrap();
    let next = ops::update_split_copies(&rt, &ctx(), &v2()).unwrap();
    assert!(
        next.refused.is_empty(),
        "redo left stale registry hashes: {next:?}"
    );
    assert_eq!(next.updated.len(), 3);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn split_update_undo_refuses_an_edit_while_other_copies_finish_or_overwrites_user_bytes() {
    let home = split_home("split_late_edit");
    let path = copy_of(&home, ".claude/skills").join("SKILL.md");
    let edited = skill_md("editor");
    let fs = edit_on_read::EditOnRead {
        path: path.clone(),
        backups: home.join(".history/backups"),
        bytes: edited.clone(),
        reads: Default::default(),
        crash_after: None,
        after_v2: Some(path.clone()),
    };
    let rt = runtime_with(&home, Arc::new(fs));
    assert_eq!(
        ops::update_split_copies(&rt, &ctx(), &v2())
            .unwrap()
            .updated
            .len(),
        3
    );
    assert_eq!(
        std::fs::read(&path).unwrap(),
        edited,
        "late edit was not injected"
    );
    let error = ops::restore_event(
        &rt,
        &ctx(),
        &skill_studio_core::dto::RestoreRequest {
            event_id: latest_update(&rt),
            force: false,
        },
    )
    .expect_err("Undo overwrote a late user edit");
    assert_eq!(
        error.code,
        skill_studio_core::error::ErrorCode::DriftConflict
    );
    assert_eq!(std::fs::read(&path).unwrap(), edited);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn split_update_undo_preserves_an_install_added_during_update_or_deletes_its_lock_row() {
    let home = split_home("split_concurrent_install");
    let mut installed = lock_doc();
    installed["skills"]["other"] =
        serde_json::json!({"source": "other/repo", "skillFolderHash": "other-hash"});
    let fs = edit_on_read::EditOnRead {
        path: home.join(".agents/.skill-lock.json"),
        backups: home.join(".history/backups"),
        bytes: serde_json::to_vec(&installed).unwrap(),
        reads: Default::default(),
        crash_after: None,
        after_v2: Some(copy_of(&home, ".claude/skills").join("SKILL.md")),
    };
    let rt = runtime_with(&home, Arc::new(fs));
    ops::update_split_copies(&rt, &ctx(), &v2()).unwrap();
    assert_eq!(
        read_lock(&home)["skills"]["other"],
        installed["skills"]["other"]
    );
    ops::restore_event(
        &rt,
        &ctx(),
        &skill_studio_core::dto::RestoreRequest {
            event_id: latest_update(&rt),
            force: false,
        },
    )
    .unwrap();
    assert_eq!(
        read_lock(&home),
        installed,
        "Undo deleted another install's lock row"
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn split_update_undo_force_retry_then_redo_keeps_hashes_matching_or_next_update_refuses() {
    let home = split_home("split_registry_retry");
    let rt = runtime_with(&home, Arc::new(RealFs::new()));
    let mut request = v2();
    request.lock_folder_hash = None;
    ops::update_split_copies(&rt, &ctx(), &request).unwrap();
    let event_id = latest_update(&rt);
    let failing = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
    let failed_rt = runtime_with(&home, failing.clone());
    failing.fail_next_write_atomic();
    let error = ops::restore_event(
        &failed_rt,
        &ctx(),
        &skill_studio_core::dto::RestoreRequest {
            event_id: event_id.clone(),
            force: false,
        },
    )
    .expect_err("Undo reported success after the registry write failed");
    assert_eq!(error.code, skill_studio_core::error::ErrorCode::Io);
    let events = ops::list_events(&rt, &ctx(), &Default::default()).unwrap();
    assert!(events
        .iter()
        .any(|e| e.kind == "restore" && e.status == "failed"));
    for dir in HARNESS_DIRS {
        assert_eq!(revision_of(&copy_of(&home, dir)), Some("v1"));
    }
    let undo = ops::restore_event(
        &rt,
        &ctx(),
        &skill_studio_core::dto::RestoreRequest {
            event_id,
            force: true,
        },
    )
    .unwrap();
    ops::restore_event(
        &rt,
        &ctx(),
        &skill_studio_core::dto::RestoreRequest {
            event_id: undo.restore_event_id,
            force: false,
        },
    )
    .unwrap();
    for dir in HARNESS_DIRS {
        assert_eq!(
            revision_of(&copy_of(&home, dir)),
            Some("v2"),
            "Redo lost the original v2 backup"
        );
    }
    let next = ops::update_split_copies(&rt, &ctx(), &v2()).unwrap();
    assert!(
        next.refused.is_empty(),
        "Redo left stale registry hashes: {next:?}"
    );
    std::fs::remove_dir_all(home).unwrap();
}

fn interrupted_update_flow(test: &str, boundary: &'static str) {
    if let Ok(home) = std::env::var("SPLIT_UPDATE_CRASH_HOME") {
        let home = PathBuf::from(home);
        let fs = edit_on_read::EditOnRead {
            path: copy_of(&home, HARNESS_DIRS[0]).join("SKILL.md"),
            backups: home.join(".history"),
            bytes: skill_md("v2"),
            reads: Default::default(),
            after_v2: None,
            crash_after: Some(boundary),
        };
        let rt = runtime_with(&home, Arc::new(fs));
        ops::update_split_copies(&rt, &ctx(), &v2()).unwrap();
        panic!("update did not reach the crash boundary");
    }
    let home = split_home(boundary);
    let before: serde_json::Value =
        serde_json::from_slice(&std::fs::read(home.join(".agents/skill-studio.json")).unwrap())
            .unwrap();
    let lock_before = read_lock(&home);
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture"])
        .env("SPLIT_UPDATE_CRASH_HOME", &home)
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(86), "child missed the crash boundary");
    let rt = runtime_with(&home, Arc::new(RealFs::new()));
    let event_id = latest_update(&rt);
    let undo = ops::restore_event(
        &rt,
        &ctx(),
        &skill_studio_core::dto::RestoreRequest {
            event_id,
            force: false,
        },
    )
    .unwrap();
    for dir in HARNESS_DIRS {
        assert_eq!(
            revision_of(&copy_of(&home, dir)),
            Some("v1"),
            "interrupted Undo left v2 bytes"
        );
    }
    let after: serde_json::Value =
        serde_json::from_slice(&std::fs::read(home.join(".agents/skill-studio.json")).unwrap())
            .unwrap();
    assert_eq!(
        after["copies"], before["copies"],
        "Undo did not restore the registry rows"
    );
    assert_eq!(
        read_lock(&home),
        lock_before,
        "Undo did not restore the source lock row"
    );
    if boundary != "swap" {
        ops::restore_event(
            &rt,
            &ctx(),
            &skill_studio_core::dto::RestoreRequest {
                event_id: undo.restore_event_id,
                force: false,
            },
        )
        .unwrap();
        assert!(HARNESS_DIRS
            .iter()
            .any(|dir| revision_of(&copy_of(&home, dir)) == Some("v2")));
    }
    let next = ops::update_split_copies(&rt, &ctx(), &v2()).unwrap();
    assert!(
        next.refused.is_empty(),
        "interrupted Update Undo Redo left stale registry hashes: {next:?}"
    );
    assert_eq!(next.updated.len(), 3);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn split_update_crashes_after_swap_then_undo_restores_v1_or_next_update_refuses() {
    interrupted_update_flow(
        "split_update_crashes_after_swap_then_undo_restores_v1_or_next_update_refuses",
        "swap",
    );
}

#[test]
fn split_update_crashes_after_lock_write_undo_redo_allows_update_or_stale_hash_refuses() {
    interrupted_update_flow(
        "split_update_crashes_after_lock_write_undo_redo_allows_update_or_stale_hash_refuses",
        "lock",
    );
}

fn history_row(
    rt: &Runtime,
    id: &skill_studio_core::identity::EventId,
) -> skill_studio_core::events::EventRecord {
    rt.ports
        .history
        .open(
            &rt.scope,
            skill_studio_core::ports::HistoryAccess::ReadIfExists,
        )
        .unwrap()
        .unwrap()
        .get(id)
        .unwrap()
        .unwrap()
}

#[test]
fn split_update_undo_crash_force_retry_redo_restores_matching_hashes_or_next_update_refuses() {
    let home = split_home("undo_crash_retry");
    let rt = runtime_with(&home, Arc::new(RealFs::new()));
    ops::update_split_copies(&rt, &ctx(), &v2()).unwrap();
    let event_id = latest_update(&rt);
    let crash_rt = runtime_with(
        &home,
        Arc::new(edit_on_read::EditOnRead {
            path: home.join("unused"),
            backups: home.join(".history/backups"),
            bytes: vec![],
            reads: Default::default(),
            after_v2: None,
            crash_after: Some("undo-registry"),
        }),
    );
    let crash = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ops::restore_event(
            &crash_rt,
            &ctx(),
            &skill_studio_core::dto::RestoreRequest {
                event_id: event_id.clone(),
                force: false,
            },
        )
    }));
    assert!(
        crash.is_err(),
        "fault port did not reach Undo metadata boundary"
    );
    for dir in HARNESS_DIRS {
        assert_eq!(revision_of(&copy_of(&home, dir)), Some("v1"));
    }
    let undo = ops::restore_event(
        &rt,
        &ctx(),
        &skill_studio_core::dto::RestoreRequest {
            event_id,
            force: true,
        },
    )
    .unwrap();
    ops::restore_event(
        &rt,
        &ctx(),
        &skill_studio_core::dto::RestoreRequest {
            event_id: undo.restore_event_id,
            force: false,
        },
    )
    .unwrap();
    for dir in HARNESS_DIRS {
        assert_eq!(
            revision_of(&copy_of(&home, dir)),
            Some("v2"),
            "Redo lost pre-crash files"
        );
    }
    assert_eq!(
        read_lock(&home)["skills"]["gamma"]["skillFolderHash"],
        "tree-v2"
    );
    assert!(ops::update_split_copies(&rt, &ctx(), &v2())
        .unwrap()
        .refused
        .is_empty());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn split_update_failed_undo_force_preserves_intervening_files_and_metadata_or_discards_edits() {
    let home = split_home("undo_displaced_retry");
    let rt = runtime_with(&home, Arc::new(RealFs::new()));
    ops::update_split_copies(&rt, &ctx(), &v2()).unwrap();
    let event_id = latest_update(&rt);
    let failed_rt = runtime_with(
        &home,
        Arc::new(edit_on_read::EditOnRead {
            path: home.join("unused"),
            backups: home.join(".history/backups"),
            bytes: vec![],
            reads: Default::default(),
            after_v2: None,
            crash_after: Some("fail-undo-registry"),
        }),
    );
    ops::restore_event(
        &failed_rt,
        &ctx(),
        &skill_studio_core::dto::RestoreRequest {
            event_id: event_id.clone(),
            force: false,
        },
    )
    .unwrap_err();
    let mut edited_lock = read_lock(&home);
    edited_lock["skills"]["gamma"]["source"] = serde_json::json!("edited/source");
    std::fs::write(
        home.join(".agents/.skill-lock.json"),
        edited_lock.to_string(),
    )
    .unwrap();
    let edited_path = copy_of(&home, ".claude/skills").join("SKILL.md");
    std::fs::write(&edited_path, skill_md("editor")).unwrap();
    let undo = ops::restore_event(
        &rt,
        &ctx(),
        &skill_studio_core::dto::RestoreRequest {
            event_id: event_id.clone(),
            force: true,
        },
    )
    .unwrap();
    let row = history_row(&rt, &undo.restore_event_id);
    let displaced = &row.payload["displaced_data"];
    assert_eq!(
        displaced["inverse"]["lock_row_undo"]["previous"], edited_lock["skills"]["gamma"],
        "Force discarded intervening lock edit"
    );
    let store = rt
        .ports
        .history
        .open(
            &rt.scope,
            skill_studio_core::ports::HistoryAccess::ReadIfExists,
        )
        .unwrap()
        .unwrap();
    let backup = displaced["backup_dir"]
        .as_str()
        .expect("Force did not save displaced files");
    let manifest = store.read_manifest(backup).unwrap();
    let entry = manifest
        .entries
        .iter()
        .find(|entry| entry.original == edited_path.parent().unwrap())
        .unwrap();
    let files = store.read_backup_files(backup, &entry.relative).unwrap();
    assert!(
        files.iter().any(|file| file.bytes == skill_md("editor")),
        "Force discarded intervening file edit"
    );
    assert_ne!(
        row.backup_dir.as_deref(),
        Some(backup),
        "Redo and displaced data share an inconsistent snapshot"
    );
    ops::restore_event(
        &rt,
        &ctx(),
        &skill_studio_core::dto::RestoreRequest {
            event_id: undo.restore_event_id,
            force: false,
        },
    )
    .unwrap();
    assert!(ops::update_split_copies(&rt, &ctx(), &v2())
        .unwrap()
        .refused
        .is_empty());
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

fn split_update_force_detects_metadata_edits_during_restore_or_loses_the_new_row(registry: bool) {
    let home = split_home("force_metadata_race");
    let rt = runtime_with(&home, Arc::new(RealFs::new()));
    ops::update_split_copies(&rt, &ctx(), &v2()).unwrap();
    let event_id = latest_update(&rt);
    let path = home.join(if registry {
        ".agents/skill-studio.json"
    } else {
        ".agents/.skill-lock.json"
    });
    let mut edited: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    if registry {
        for row in edited["copies"].as_object_mut().unwrap().values_mut() {
            row["concurrent_edit"] = serde_json::json!(true);
        }
    } else {
        edited["skills"]["gamma"]["concurrent_edit"] = serde_json::json!(true);
    }
    let race_rt = runtime_with(
        &home,
        Arc::new(edit_on_read::EditOnRead {
            path: path.clone(),
            backups: home.join(".history/backups"),
            bytes: serde_json::to_vec(&edited).unwrap(),
            reads: Default::default(),
            after_v2: None,
            crash_after: Some("edit-during-restore"),
        }),
    );
    let error = ops::restore_event(
        &race_rt,
        &ctx(),
        &skill_studio_core::dto::RestoreRequest {
            event_id: event_id.clone(),
            force: true,
        },
    )
    .expect_err("Force overwrote metadata edited during folder restoration");
    assert_eq!(
        error.code,
        skill_studio_core::error::ErrorCode::DriftConflict
    );
    let live: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(live, edited);
    let undo = ops::restore_event(
        &rt,
        &ctx(),
        &skill_studio_core::dto::RestoreRequest {
            event_id,
            force: true,
        },
    )
    .unwrap();
    let row = history_row(&rt, &undo.restore_event_id);
    let displaced = &row.payload["displaced_data"]["inverse"];
    if registry {
        assert!(displaced["registry_undo"]
            .as_array()
            .unwrap()
            .iter()
            .all(|edit| edit["previous"]["concurrent_edit"] == true));
    } else {
        assert_eq!(
            displaced["lock_row_undo"]["previous"],
            edited["skills"]["gamma"]
        );
    }
    std::fs::remove_dir_all(home).unwrap();
}

fn split_update_undo_force_restores_deleted_lock_before_undo_or_retry_instead_of_not_found(
    before_retry: bool,
) {
    let home = split_home("force_deleted_lock");
    let rt = runtime_with(&home, Arc::new(RealFs::new()));
    ops::update_split_copies(&rt, &ctx(), &v2()).unwrap();
    let event_id = latest_update(&rt);
    if before_retry {
        let failing = Arc::new(FailingFs::wrap(Arc::new(RealFs::new())));
        let failed_rt = runtime_with(&home, failing.clone());
        failing.fail_next_write_atomic();
        ops::restore_event(
            &failed_rt,
            &ctx(),
            &skill_studio_core::dto::RestoreRequest {
                event_id: event_id.clone(),
                force: false,
            },
        )
        .unwrap_err();
    }
    std::fs::remove_file(home.join(".agents/.skill-lock.json")).unwrap();
    let request = skill_studio_core::dto::RestoreRequest {
        event_id,
        force: false,
    };
    assert_eq!(
        ops::restore_event(&rt, &ctx(), &request).unwrap_err().code,
        skill_studio_core::error::ErrorCode::DriftConflict
    );
    ops::restore_event(
        &rt,
        &ctx(),
        &skill_studio_core::dto::RestoreRequest {
            force: true,
            ..request
        },
    )
    .unwrap();
    assert_eq!(
        read_lock(&home)["skills"]["gamma"],
        lock_doc()["skills"]["gamma"]
    );
    for dir in HARNESS_DIRS {
        assert_eq!(revision_of(&copy_of(&home, dir)), Some("v1"));
    }
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn split_update_crashes_before_registry_undo_redo_allows_update_or_stale_hash_refuses() {
    interrupted_update_flow(
        "split_update_crashes_before_registry_undo_redo_allows_update_or_stale_hash_refuses",
        "next-stage",
    );
}

#[test]
fn split_update_force_detects_lock_edits_during_restore_or_loses_the_new_row() {
    split_update_force_detects_metadata_edits_during_restore_or_loses_the_new_row(false);
}

#[test]
fn split_update_force_detects_registry_edits_during_restore_or_loses_the_new_row() {
    split_update_force_detects_metadata_edits_during_restore_or_loses_the_new_row(true);
}

#[test]
fn split_update_undo_force_restores_a_lock_deleted_before_undo_or_fails_with_not_found() {
    split_update_undo_force_restores_deleted_lock_before_undo_or_retry_instead_of_not_found(false);
}

#[test]
fn split_update_undo_force_restores_a_lock_deleted_before_retry_or_fails_with_not_found() {
    split_update_undo_force_restores_deleted_lock_before_undo_or_retry_instead_of_not_found(true);
}

fn deleted_registry_force_restore_keeps_next_update(only_copies: bool, delete_before_redo: bool) {
    let home = split_home("deleted_registry_force_restore");
    let rt = runtime_with(&home, Arc::new(RealFs::new()));
    let registry = home.join(".agents/skill-studio.json");
    let before: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&registry).unwrap()).unwrap();
    ops::update_split_copies(&rt, &ctx(), &v2()).unwrap();
    let updated: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&registry).unwrap()).unwrap();
    let delete = || {
        if only_copies {
            let mut doc: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&registry).unwrap()).unwrap();
            doc.as_object_mut().unwrap().remove("copies");
            doc["unrelated"] = serde_json::json!("keep");
            std::fs::write(&registry, doc.to_string()).unwrap();
        } else {
            std::fs::remove_file(&registry).unwrap();
        }
    };
    if !delete_before_redo {
        delete();
    }
    let undo = ops::restore_event(
        &rt,
        &ctx(),
        &skill_studio_core::dto::RestoreRequest {
            event_id: latest_update(&rt),
            force: true,
        },
    )
    .unwrap();
    let restored: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&registry).unwrap()).unwrap();
    assert_eq!(restored["copies"], before["copies"]);
    for dir in HARNESS_DIRS {
        assert_eq!(revision_of(&copy_of(&home, dir)), Some("v1"));
    }
    if delete_before_redo {
        delete();
    }
    ops::restore_event(
        &rt,
        &ctx(),
        &skill_studio_core::dto::RestoreRequest {
            event_id: undo.restore_event_id,
            force: delete_before_redo,
        },
    )
    .unwrap();
    let redone: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&registry).unwrap()).unwrap();
    assert_eq!(redone["copies"], updated["copies"]);
    if only_copies {
        assert_eq!(redone["unrelated"], "keep");
    }
    for dir in HARNESS_DIRS {
        assert_eq!(revision_of(&copy_of(&home, dir)), Some("v2"));
    }
    let next = ops::update_split_copies(&rt, &ctx(), &v2()).unwrap();
    assert!(
        next.refused.is_empty(),
        "registry hashes refused unchanged copies: {next:?}"
    );
    assert_eq!(next.updated.len(), 3);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn split_update_deleted_registry_force_undo_redo_allows_update_or_loses_copy_rows() {
    deleted_registry_force_restore_keeps_next_update(false, false);
}

#[test]
fn split_update_deleted_copies_force_undo_redo_allows_update_or_loses_copy_rows() {
    deleted_registry_force_restore_keeps_next_update(true, false);
}

#[test]
fn split_update_undo_deleted_registry_force_redo_allows_update_or_loses_copy_rows() {
    deleted_registry_force_restore_keeps_next_update(false, true);
}

#[test]
fn split_update_undo_deleted_copies_force_redo_allows_update_or_loses_copy_rows() {
    deleted_registry_force_restore_keeps_next_update(true, true);
}

fn force_restore_rejects_non_object_copies(redo: bool) {
    for invalid in [
        serde_json::json!(null),
        serde_json::json!([]),
        serde_json::json!("invalid"),
        serde_json::json!(1),
        serde_json::json!(false),
    ] {
        let home = split_home("invalid_registry_copies");
        let rt = runtime_with(&home, Arc::new(RealFs::new()));
        ops::update_split_copies(&rt, &ctx(), &v2()).unwrap();
        let event_id = if redo {
            ops::restore_event(
                &rt,
                &ctx(),
                &skill_studio_core::dto::RestoreRequest {
                    event_id: latest_update(&rt),
                    force: false,
                },
            )
            .unwrap()
            .restore_event_id
        } else {
            latest_update(&rt)
        };
        let registry = home.join(".agents/skill-studio.json");
        let mut doc: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&registry).unwrap()).unwrap();
        doc["copies"] = invalid;
        std::fs::write(&registry, doc.to_string()).unwrap();
        let error = ops::restore_event(
            &rt,
            &ctx(),
            &skill_studio_core::dto::RestoreRequest {
                event_id,
                force: true,
            },
        )
        .expect_err("Force silently skipped saved copy rows");
        assert!(
            error.message.contains("copies") && error.message.contains("object"),
            "{error:?}"
        );
        let unchanged: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&registry).unwrap()).unwrap();
        assert_eq!(unchanged["copies"], doc["copies"]);
        std::fs::remove_dir_all(home).unwrap();
    }
}

#[test]
fn split_update_force_undo_rejects_non_object_copies_or_reports_success_without_rows() {
    force_restore_rejects_non_object_copies(false);
}

#[test]
fn split_update_undo_force_redo_rejects_non_object_copies_or_reports_success_without_rows() {
    force_restore_rejects_non_object_copies(true);
}

fn metadata_loss_restore_chain_keeps_next_update(store: &str) {
    for first_force in [0, 1] {
        let home = split_home("metadata_loss_restore_chain");
        let rt = runtime_with(&home, Arc::new(RealFs::new()));
        let registry = home.join(".agents/skill-studio.json");
        let read_registry = || -> serde_json::Value {
            serde_json::from_slice(&std::fs::read(&registry).unwrap()).unwrap()
        };
        let before = read_registry();
        let lock_before = read_lock(&home)["skills"]["gamma"].clone();
        ops::update_split_copies(&rt, &ctx(), &v2()).unwrap();
        let updated = read_registry();
        let lock_updated = read_lock(&home)["skills"]["gamma"].clone();
        let mut event_id = latest_update(&rt);
        for step in 0..6 {
            let force = step % 2 == first_force;
            if force {
                match store {
                    "registry" => std::fs::remove_file(&registry).unwrap(),
                    "copies" => {
                        let mut doc = read_registry();
                        doc.as_object_mut().unwrap().remove("copies");
                        std::fs::write(&registry, doc.to_string()).unwrap();
                    }
                    "lock" => std::fs::remove_file(home.join(".agents/.skill-lock.json")).unwrap(),
                    "lock-row" => {
                        let mut doc = read_lock(&home);
                        doc["skills"].as_object_mut().unwrap().remove("gamma");
                        std::fs::write(home.join(".agents/.skill-lock.json"), doc.to_string())
                            .unwrap();
                    }
                    _ => unreachable!(),
                }
            }
            event_id = ops::restore_event(
                &rt,
                &ctx(),
                &skill_studio_core::dto::RestoreRequest { event_id, force },
            )
            .unwrap()
            .restore_event_id;
            let (expected_registry, expected_lock, revision) = if step % 2 == 0 {
                (&before, &lock_before, "v1")
            } else {
                (&updated, &lock_updated, "v2")
            };
            assert_eq!(
                read_registry()["copies"],
                expected_registry["copies"],
                "{store}, phase {first_force}, step {step}"
            );
            assert_eq!(
                &read_lock(&home)["skills"]["gamma"],
                expected_lock,
                "{store}, phase {first_force}, step {step}"
            );
            for dir in HARNESS_DIRS {
                assert_eq!(revision_of(&copy_of(&home, dir)), Some(revision));
            }
        }
        let next = ops::update_split_copies(&rt, &ctx(), &v2()).unwrap();
        assert!(next.refused.is_empty(), "{store}: {next:?}");
        assert_eq!(next.updated.len(), 3, "{store}: {next:?}");
        std::fs::remove_dir_all(home).unwrap();
    }
}

#[test]
fn split_update_registry_loss_alternating_force_restores_keep_copies_or_next_update_loses_rows() {
    metadata_loss_restore_chain_keeps_next_update("registry");
}

#[test]
fn split_update_copies_loss_alternating_force_restores_keep_hashes_or_next_update_refuses() {
    metadata_loss_restore_chain_keeps_next_update("copies");
}

#[test]
fn split_update_lock_loss_alternating_force_restores_keep_source_or_next_update_refuses() {
    metadata_loss_restore_chain_keeps_next_update("lock");
}

#[test]
fn split_update_lock_row_loss_alternating_force_restores_keep_source_or_next_update_refuses() {
    metadata_loss_restore_chain_keeps_next_update("lock-row");
}

#[test]
fn split_update_undo_redo_four_times_restores_a_changed_existing_lock_row_and_keeps_an_unchanged_one_or_a_null_inverse_skips_the_row(
) {
    for changes_lock in [false, true] {
        let home = split_home("split_update_lock_inverse_cycle");
        let rt = runtime_with(&home, Arc::new(RealFs::new()));
        let before = read_lock(&home);
        let mut request = v2();
        if !changes_lock {
            request.lock_folder_hash = None;
        }
        ops::update_split_copies(&rt, &ctx(), &request).unwrap();
        let after = read_lock(&home);
        let mut id = latest_update(&rt);
        let inverse = history_row(&rt, &id).inverse.unwrap();
        if changes_lock {
            assert!(inverse["lock_row_undo"].is_object());
            assert_eq!(inverse["lock_row_undo"]["version"], 3);
            assert_eq!(
                inverse["lock_row_undo"]["path"],
                home.join(".agents/.skill-lock.json").to_str().unwrap()
            );
            assert_eq!(
                inverse["lock_row_undo"]["previous"],
                before["skills"]["gamma"]
            );
            assert_eq!(
                inverse["lock_row_undo"]["expected"],
                after["skills"]["gamma"]
            );
            assert_ne!(before, after);
        } else {
            assert!(inverse["lock_row_undo"].is_null());
            assert_eq!(before, after);
        }
        for step in 0..4 {
            id = ops::restore_event(
                &rt,
                &ctx(),
                &skill_studio_core::dto::RestoreRequest {
                    event_id: id,
                    force: false,
                },
            )
            .unwrap()
            .restore_event_id;
            assert_eq!(
                read_lock(&home),
                if step % 2 == 0 {
                    before.clone()
                } else {
                    after.clone()
                }
            );
            let mirror = history_row(&rt, &id).inverse.unwrap();
            assert_eq!(mirror["lock_row_undo"].is_object(), changes_lock);
            if !changes_lock {
                assert_eq!(mirror.get("lock_row_undo"), Some(&serde_json::Value::Null));
            }
        }
        std::fs::remove_dir_all(home).unwrap();
    }
}
