// Integration test binaries aren't covered by the lib crate's
// `cfg_attr(test, allow(...))`: this file compiles as its own crate, so
// the same allow needs to be declared here too.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Real-disk integration tests for [`skill_studio_core::registry`]'s
//! lease-guarded write path.
//!
//! Uses `skill-studio-host`'s real `RealFs`/`FileLease` rather than the
//! in-memory `FixtureFs`, since `FixtureFs::write_atomic` refuses every
//! call (see its doc comment in `testing.rs`) - this module's whole job is
//! the write.

use std::path::Path;

use serde::{Deserialize, Serialize};
use skill_studio_core::registry::{write_registry_document, RegistryDocument};
use skill_studio_host::{FileLease, RealFs};

/// A minimal registry document: one known field plus a flatten catch-all,
/// standing in for the desktop's `ForkRegistry` without pulling its
/// adapter-specific types into a core test.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
struct TestDoc {
    #[serde(default)]
    write_version: u64,
    #[serde(default)]
    known_field: String,
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

impl RegistryDocument for TestDoc {
    fn write_version(&self) -> u64 {
        self.write_version
    }
    fn set_write_version(&mut self, version: u64) {
        self.write_version = version;
    }
}

fn write(home: &Path, path: &Path, doc: &mut TestDoc) {
    let fs = RealFs::new();
    let leases = FileLease::new(home.join(".leases"));
    write_registry_document(&leases, &fs, home, path, doc).unwrap();
}

/// Flow: a registry document already on disk carries a key
/// (`a_future_field`) the current schema does not know about (an older or
/// newer build wrote it). A caller reads it, changes a known field, and
/// writes it back through the lease-guarded path.
/// Expectation: `a_future_field` survives byte-identical.
/// Failure here (the key silently disappearing) would mean a lease-guarded
/// write and an older/newer build's registry can no longer share the file.
#[test]
fn registry_flatten_catch_all_round_trips_an_unknown_key_or_names_the_dropped_key() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let path = home.join(".agents").join("skill-studio.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        r#"{"write_version":0,"known_field":"a","a_future_field":{"nested":true}}"#,
    )
    .unwrap();

    let mut doc: TestDoc = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        doc.extra.get("a_future_field"),
        Some(&serde_json::json!({"nested": true}))
    );
    doc.known_field = "b".to_string();
    write(home, &path, &mut doc);

    let reloaded: TestDoc = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        reloaded.extra.get("a_future_field"),
        Some(&serde_json::json!({"nested": true}))
    );
    assert_eq!(reloaded.known_field, "b");
}

/// Flow: a document at write-version 3 goes through one lease-guarded
/// write.
/// Expectation: the write-version on disk (and on the in-memory value the
/// caller holds) becomes 4.
/// Failure here means a lease loser's stale-read check would
/// have nothing to compare against - the counter never moves.
#[test]
fn write_registry_document_bumps_write_version_by_one_or_names_the_stuck_value() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let path = home.join(".agents").join("skill-studio.json");

    let mut doc = TestDoc {
        write_version: 3,
        ..Default::default()
    };
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, serde_json::to_vec(&doc).unwrap()).unwrap();
    write(home, &path, &mut doc);
    assert_eq!(doc.write_version, 4);

    let reloaded: TestDoc = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(reloaded.write_version, 4);
}

/// Flow: a document written before this field existed (absent from the
/// JSON) is read through the trait's `#[serde(default)]`.
/// Expectation: it starts at write-version 0.
/// Failure here means an old registry file would panic or misbehave on
/// first read instead of starting the counter cleanly.
#[test]
fn missing_write_version_defaults_to_zero_or_names_the_thrown_error() {
    let json = r#"{"known_field":"a"}"#;
    let doc: TestDoc = serde_json::from_str(json).unwrap();
    assert_eq!(doc.write_version(), 0);
}

#[test]
fn registry_write_with_a_stale_write_version_is_rejected_and_names_the_current_version() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(".agents/skill-studio.json");
    let mut winner = TestDoc::default();
    write(tmp.path(), &path, &mut winner);
    let mut stale = winner.clone();
    winner.known_field = "newer change".into();
    write(tmp.path(), &path, &mut winner);
    let committed = std::fs::read(&path).unwrap();
    let read_version = stale.write_version;
    stale.known_field = "stale change".into();
    let error = write_registry_document(
        &FileLease::new(tmp.path().join(".leases")),
        &RealFs::new(),
        tmp.path(),
        &path,
        &mut stale,
    )
    .expect_err("a stale snapshot must not overwrite the newer writer");
    assert_eq!(error.code, skill_studio_core::ErrorCode::StaleProposal);
    assert!(
        error
            .message
            .contains(&format!("current version {}", winner.write_version)),
        "{error}"
    );
    assert_eq!(std::fs::read(&path).unwrap(), committed);
    assert_eq!(stale.write_version, read_version);
}

#[test]
fn a_failed_registry_write_keeps_the_callers_version_or_makes_its_retry_stale() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(".agents/skill-studio.json");
    let fs = skill_studio_core::testing::FailingFs::wrap(std::sync::Arc::new(RealFs::new()));
    fs.fail_next_write_atomic();
    let mut doc = TestDoc::default();
    let before = doc.clone();
    assert!(write_registry_document(
        &FileLease::new(tmp.path().join(".leases")),
        &fs,
        tmp.path(),
        &path,
        &mut doc
    )
    .is_err());
    assert_eq!(
        doc, before,
        "an unsuccessful write must leave its input version reusable"
    );
    assert!(!path.exists());
}

#[test]
fn two_registry_writers_under_one_lease_serialize_and_neither_write_is_lost() {
    use std::sync::{mpsc, Arc, Barrier, Mutex};
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(".agents/skill-studio.json");
    let barrier = Arc::new(Barrier::new(2));
    let (sender, receiver) = mpsc::channel();
    let receiver = Arc::new(Mutex::new(receiver));
    let names = ["one", "two"];
    let handles: Vec<_> = names
        .iter()
        .map(|name| {
            let name = name.to_string();
            let path = path.clone();
            let home = tmp.path().to_path_buf();
            let barrier = barrier.clone();
            let sender = sender.clone();
            let receiver = receiver.clone();
            std::thread::spawn(move || {
                let leases = FileLease::new(home.join(".leases"));
                let fs = RealFs::new();
                let mut doc = TestDoc::default();
                doc.extra.insert(name.clone(), serde_json::json!(true));
                barrier.wait();
                match write_registry_document(&leases, &fs, &home, &path, &mut doc) {
                    Ok(()) => sender.send(()).unwrap(),
                    Err(error)
                        if matches!(
                            error.code,
                            skill_studio_core::ErrorCode::StaleProposal
                                | skill_studio_core::ErrorCode::ScopeBusy
                        ) =>
                    {
                        drop(sender);
                        receiver
                            .lock()
                            .unwrap()
                            .recv()
                            .expect("the other writer must commit before retry");
                        let mut fresh: TestDoc =
                            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
                        fresh.extra.insert(name, serde_json::json!(true));
                        write_registry_document(&leases, &fs, &home, &path, &mut fresh).unwrap();
                    }
                    Err(error) => panic!("unexpected registry failure: {error:?}"),
                }
            })
        })
        .collect();
    drop(sender);
    for handle in handles {
        handle.join().unwrap();
    }
    let committed: TestDoc = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        committed.write_version,
        names.len() as u64,
        "each successful writer must advance the version once"
    );
    for name in names {
        assert_eq!(
            committed.extra.get(name),
            Some(&serde_json::json!(true)),
            "writer {name} was lost"
        );
    }
}

#[test]
fn registry_write_without_the_lease_root_names_the_missing_directory() {
    let temp = tempfile::tempdir().unwrap();
    let blocked = temp.path().join("blocked");
    std::fs::write(&blocked, b"not a directory").unwrap();
    let lease_root = blocked.join("leases");
    let path = temp.path().join(".agents/skill-studio.json");
    let mut document = TestDoc::default();
    let error = write_registry_document(
        &FileLease::new(lease_root.clone()),
        &RealFs::new(),
        temp.path(),
        &path,
        &mut document,
    )
    .expect_err("an unusable lease root must fail before writing the registry");
    assert_eq!(error.path.as_ref(), Some(&lease_root));
    assert_eq!(document.write_version, 0);
    assert!(!path.exists());
}
