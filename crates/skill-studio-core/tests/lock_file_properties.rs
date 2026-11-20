// Integration test binaries aren't covered by the lib crate's
// `cfg_attr(test, allow(...))`: this file compiles as its own crate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Property tests for [`restore_lock_entry`], the undo of a remove, which
//! writes a row back into the CLI-owned `~/.agents/.skill-lock.json`.
//! `npx skills` owns that file, so the write may change only
//! `skills[name]`: the version, every other row, unknown keys and key order
//! must come back byte for byte in the CLI's own pretty-printed shape.
//! Real temp folders, because the in-memory `FixtureFs` cannot write.

use std::path::PathBuf;
use std::time::Duration;

use proptest::prelude::*;
use serde_json::{Map, Value};

use skill_studio_core::lock_file::{read_lock_entry_value, restore_lock_entry};
use skill_studio_core::ports::{ExclusiveGuard, LeaseMode, LeaseProvider};
use skill_studio_core::scope::{NormalizedScope, RuntimeScope};
use skill_studio_core::testing::FakeLease;
use skill_studio_host::RealFs;

fn leaf() -> impl Strategy<Value = Value> {
    prop_oneof![
        any::<bool>().prop_map(Value::Bool),
        any::<i32>().prop_map(Value::from),
        "[ -~é]{0,12}".prop_map(Value::String),
        Just(Value::Null),
    ]
}

fn json_value() -> impl Strategy<Value = Value> {
    leaf().prop_recursive(2, 16, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..3).prop_map(Value::Array),
            prop::collection::vec(("[a-zA-Z]{1,8}", inner), 0..4)
                .prop_map(|pairs| Value::Object(pairs.into_iter().collect())),
        ]
    })
}

/// One lock row: the fields the CLI writes plus unknown ones.
fn row() -> impl Strategy<Value = Value> {
    (
        "[a-z]{1,6}/[a-z]{1,6}",
        "[0-9a-f]{8}",
        prop::collection::vec(("x[A-Za-z]{1,6}", json_value()), 0..3),
    )
        .prop_map(|(source, hash, extra)| {
            let mut row = Map::new();
            row.insert("source".into(), Value::String(source));
            row.insert("sourceType".into(), Value::String("github".into()));
            row.insert("skillFolderHash".into(), Value::String(hash));
            for (key, value) in extra {
                row.insert(key, value);
            }
            Value::Object(row)
        })
}

/// A whole lock document: unknown top-level keys around `skills`, in
/// random order, with rows in random order.
fn document() -> impl Strategy<Value = Value> {
    (
        1u32..5,
        prop::collection::vec(("[a-d]{1,2}", row()), 0..5),
        prop::collection::vec(("t[A-Za-z]{1,6}", json_value()), 0..3),
        any::<bool>(),
    )
        .prop_map(|(version, rows, extra, skills_first)| {
            let skills = Value::Object(rows.into_iter().collect());
            let mut doc = Map::new();
            if skills_first {
                doc.insert("skills".into(), skills.clone());
            }
            doc.insert("version".into(), Value::from(version));
            for (key, value) in extra {
                doc.insert(key, value);
            }
            if !skills_first {
                doc.insert("skills".into(), skills);
            }
            Value::Object(doc)
        })
}

/// The bytes `npx skills` writes: two-space pretty JSON and a newline.
fn cli_bytes(doc: &Value) -> Vec<u8> {
    let mut bytes = serde_json::to_vec_pretty(doc).unwrap();
    bytes.push(b'\n');
    bytes
}

struct Home {
    _dir: tempfile::TempDir,
    lock: PathBuf,
    fs: RealFs,
    scope: NormalizedScope,
    guard: ExclusiveGuard,
}

fn setup(doc: &Value) -> Home {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().canonicalize().unwrap();
    std::fs::create_dir_all(home.join(".agents")).unwrap();
    let lock = home.join(".agents/.skill-lock.json");
    std::fs::write(&lock, cli_bytes(doc)).unwrap();
    let fs = RealFs::new();
    let scope = NormalizedScope::normalize(&RuntimeScope::fixture(&home), &fs).unwrap();
    let handle = FakeLease::default()
        .acquire(&[], LeaseMode::Exclusive, Duration::from_secs(0))
        .unwrap();
    Home {
        _dir: dir,
        lock,
        fs,
        scope,
        guard: ExclusiveGuard::from_handle(handle),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn restoring_a_row_changes_only_that_row(
        doc in document(),
        name in "[a-d]{1,2}",
        entry in row(),
    ) {
        let home = setup(&doc);
        let had_row = doc["skills"].get(&name).is_some();
        restore_lock_entry(&home.guard, &home.fs, &home.scope, &home.lock, &name, &entry).unwrap();

        // A row a reinstall wrote first is kept; otherwise only skills[name]
        // is new, after the rows that were already there.
        let mut expected = doc.clone();
        if !had_row {
            expected["skills"].as_object_mut().unwrap().insert(name.clone(), entry.clone());
        }
        prop_assert_eq!(
            String::from_utf8(std::fs::read(&home.lock).unwrap()).unwrap(),
            String::from_utf8(cli_bytes(&expected)).unwrap()
        );
    }

    #[test]
    fn a_removed_last_row_comes_back_byte_for_byte(doc in document(), name in "[a-d]{1,2}", entry in row()) {
        // The remove flow: save the row as written, let the CLI drop it,
        // then undo. The row was the file's last, so undo restores every byte.
        let mut original = doc.clone();
        let skills = original["skills"].as_object_mut().unwrap();
        skills.shift_remove(&name);
        skills.insert(name.clone(), entry);
        let home = setup(&original);
        let saved = read_lock_entry_value(&home.fs, &home.lock, &name).unwrap().unwrap();

        let mut removed = original.clone();
        removed["skills"].as_object_mut().unwrap().shift_remove(&name);
        std::fs::write(&home.lock, cli_bytes(&removed)).unwrap();

        restore_lock_entry(&home.guard, &home.fs, &home.scope, &home.lock, &name, &saved).unwrap();
        prop_assert_eq!(std::fs::read(&home.lock).unwrap(), cli_bytes(&original));
    }
}
