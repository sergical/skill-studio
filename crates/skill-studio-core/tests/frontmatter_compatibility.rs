#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Baseline values and errors were recorded with `serde_yaml` 0.9.34 before migration.

use skill_studio_core::frontmatter::{parse_frontmatter, FrontmatterParseResult};
use std::collections::BTreeMap;

fn corpus() -> BTreeMap<String, String> {
    let mut cases = BTreeMap::new();
    fn visit(path: &std::path::Path, cases: &mut BTreeMap<String, String>) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path, cases);
            } else if path.file_name().unwrap() == "SKILL.md" {
                cases.insert(
                    path.canonicalize()
                        .unwrap()
                        .strip_prefix(
                            std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
                                .canonicalize()
                                .unwrap(),
                        )
                        .unwrap()
                        .display()
                        .to_string(),
                    std::fs::read_to_string(path).unwrap(),
                );
            }
        }
    }
    visit(
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures")),
        &mut cases,
    );
    visit(
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures")),
        &mut cases,
    );
    for (name, yaml) in [
        ("empty", ""),
        ("aliases-merge", "name: demo\ndescription: &text example\nmetadata: {copy: *text, base: &base {x: 1}, merged: {<<: *base, y: 2}}"),
        ("numeric-keys", "metadata: {nested: {42: answer, true: bool-key}}"),
        ("nonfinite", "metadata: {nan: .nan, infinity: .inf}"),
        ("boolean-words", "name: yes\ndescription: no\nmetadata: {on: on, off: off, yes: yes, no: no}"),
        ("numbers-dates", "name: 123\ndescription: 2026-11-11\nmetadata: {int: 42, float: 1.5, octal: 0o17, leading: 012, hex: 0xFF, date: 2026-11-11, null: null}"),
        ("multiline", "name: demo\ndescription: >-\n  first line\n  second line\nmetadata: {literal: |\n    first\n    second\n  }"),
        ("literal", "name: demo\ndescription: |\n  first\n  second"),
        ("quotes-comments", "'name': 'demo' # comment\n\"description\": \"text: # literal\"\nmetadata: {quoted: 'yes'}"),
        ("duplicate-field", "name: first\nname: second"),
        ("duplicate-metadata", "metadata: {key: first, key: second}"),
        ("tabs", "name: demo\ndescription:\n\tbad indent"),
        ("malformed", "name: demo\ndescription: unquoted: colon"),
        ("sequence", "name: demo\ndescription: [unterminated"),
        ("strict-bool", "name: demo\ndisable-model-invocation: yes"),
        ("metadata-types", "metadata: {bool: true, list: [1, two], nested: {a: b}}"),
    ] { cases.insert(name.into(), format!("---\n{yaml}\n---\nBody.\n")); }
    let quoted_null_key = cases["numbers-dates"].replace("null: null", "\"null\": null");
    cases.insert("numbers-dates-quoted-null-key".into(), quoted_null_key);
    cases
}

#[test]
fn frontmatter_corpus_keeps_values_and_rejects_invalid_yaml_instead_of_changing_skills() {
    let actual: BTreeMap<_, _> = corpus().into_iter().map(|(name, content)| {
        let result = match parse_frontmatter(&content) {
            FrontmatterParseResult::Absent => serde_json::json!({"absent": true}),
            FrontmatterParseResult::Valid(value) => serde_json::json!({"value": value}),
            FrontmatterParseResult::Invalid(error) => serde_json::json!({"invalid": true, "line": error.line, "column": error.column, "message": error.message}),
        };
        (name, result)
    }).collect();
    let expected_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/frontmatter-compatibility.json"
    );
    let mut expected: BTreeMap<String, serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(expected_path).unwrap()).unwrap();
    expected.insert(
        "numbers-dates-quoted-null-key".into(),
        expected["numbers-dates"].clone(),
    );
    assert_eq!(
        actual.keys().collect::<Vec<_>>(),
        expected.keys().collect::<Vec<_>>()
    );
    for (name, value) in actual {
        if value.get("invalid").is_some() {
            assert_eq!(expected[&name]["invalid"], true, "{name}: {value}");
            assert!(value["line"].as_u64().unwrap() > 1, "{name}");
            assert!(value["column"].as_u64().unwrap() > 0, "{name}");
            assert!(!value["message"].as_str().unwrap().is_empty(), "{name}");
        } else {
            assert_eq!(value, expected[&name], "{name}");
        }
    }
}

#[test]
fn yaml_dependency_graph_excludes_unsafe_libyaml_instead_of_using_an_advisory_exception() {
    let lock = include_str!("../../../Cargo.lock");
    assert!(
        !lock.contains("name = \"unsafe-libyaml\""),
        "unsafe-libyaml remains in Cargo.lock"
    );
}
