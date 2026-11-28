// Integration test crate: `unwrap` and `expect` are the assertions here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Skill link detection on real packs: the links a person read line by line
//! in `docs/research/skill-links-evidence.md`, checked against the pinned
//! copies in `tests/fixtures/real-packs` (refresh with
//! `tools/refresh-real-packs.sh`).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use proptest::prelude::*;
use skill_studio_core::frontmatter::frontmatter_fields;
use skill_studio_core::skill_links::{detect_links, DetectedLinks, LinkStrength, SkillText};

fn packs_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/real-packs")
}

fn markdown_files(dir: &Path, root: &Path, out: &mut Vec<(String, String)>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            markdown_files(&path, root, out);
        } else if path.extension().is_some_and(|ext| ext == "md") {
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, fs::read_to_string(&path).unwrap()));
        }
    }
}

fn skill_dirs(dir: &Path, out: &mut Vec<PathBuf>) {
    if dir.join("SKILL.md").is_file() {
        out.push(dir.to_path_buf());
        return;
    }
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for path in entries.into_iter().filter(|p| p.is_dir()) {
        skill_dirs(&path, out);
    }
}

fn pack(folder: &str) -> Vec<SkillText> {
    let mut dirs = Vec::new();
    skill_dirs(&packs_root().join(folder).join("skills"), &mut dirs);
    dirs.into_iter()
        .map(|dir| {
            let mut files = Vec::new();
            markdown_files(&dir, &dir, &mut files);
            let skill_md = fs::read_to_string(dir.join("SKILL.md")).unwrap();
            let folder_name = dir.file_name().unwrap().to_string_lossy().to_string();
            let name = frontmatter_fields(&skill_md)
                .get("name")
                .cloned()
                .unwrap_or(folder_name.clone());
            SkillText {
                name,
                folder: folder_name,
                pack: Some(folder.to_string()),
                files,
            }
        })
        .collect()
}

fn strength(found: &DetectedLinks, from: &str, to: &str) -> Option<LinkStrength> {
    found
        .links
        .iter()
        .find(|l| l.from == from && l.to == to)
        .map(|l| l.strength)
}

fn strong_users(found: &DetectedLinks, to: &str) -> BTreeSet<String> {
    found
        .links
        .iter()
        .filter(|l| {
            l.to == to && matches!(l.strength, LinkStrength::Needs | LinkStrength::HandsOff)
        })
        .map(|l| l.from.clone())
        .collect()
}

#[test]
fn real_packs_give_the_links_a_person_checked_by_hand() {
    let matt = detect_links(&pack("mattpocock_skills"), &BTreeSet::new());
    let grill_me = matt
        .links
        .iter()
        .find(|l| l.from == "grill-me" && l.to == "grilling")
        .unwrap();
    assert_eq!(grill_me.strength, LinkStrength::HandsOff);
    assert_eq!(
        (grill_me.evidence.file.as_str(), grill_me.evidence.line),
        ("SKILL.md", 7)
    );
    assert_eq!(
        strong_users(&matt, "grilling"),
        [
            "grill-me",
            "grill-with-docs",
            "improve-codebase-architecture",
            "loop-me",
            "triage",
            "wayfinder"
        ]
        .map(String::from)
        .into(),
    );
    assert_eq!(
        strong_users(&matt, "setup-matt-pocock-skills"),
        [
            "ask-matt",
            "code-review",
            "implement-spec",
            "to-spec",
            "to-tickets",
            "triage",
            "wayfinder"
        ]
        .map(String::from)
        .into(),
    );
    assert_eq!(
        strength(&matt, "chief-of-staff", "implement-spec"),
        Some(LinkStrength::Needs)
    );
    assert_eq!(matt.indexes, vec!["ask-matt".to_string()]);
    assert_eq!(strength(&matt, "ask-matt", "pr"), Some(LinkStrength::Lists));

    let pstack = detect_links(
        &pack("cursor_plugins_pstack"),
        &BTreeSet::from(["deslop".to_string()]),
    );
    assert_eq!(
        strength(&pstack, "poteto-mode", "deslop"),
        Some(LinkStrength::HandsOff)
    );
    assert_eq!(pstack.indexes, vec!["poteto-help".to_string()]);

    let superpowers = detect_links(&pack("obra_superpowers"), &BTreeSet::new());
    assert_eq!(
        strength(&superpowers, "brainstorming", "writing-plans"),
        Some(LinkStrength::HandsOff)
    );
    assert!(!superpowers.indexes.contains(&"executing-plans".to_string()));

    let emil = detect_links(&pack("emilkowalski_skills"), &BTreeSet::new());
    assert_eq!(
        strength(&emil, "find-animation-opportunities", "choreograph"),
        Some(LinkStrength::Needs)
    );
    assert_eq!(emil.indexes, vec!["ask-emil".to_string()]);
}

fn synthetic(name: &str, files: &[(&str, &str)]) -> SkillText {
    SkillText {
        name: name.to_string(),
        folder: name.to_string(),
        pack: None,
        files: files
            .iter()
            .map(|(p, t)| (p.to_string(), t.to_string()))
            .collect(),
    }
}

#[test]
fn lines_that_only_look_like_links_are_not_links() {
    let skills = [
        synthetic(
            "skill-a",
            &[
                ("SKILL.md", "Do not call `skill-b` for approvals.\nRun `npx skill-b render`.\nThis is called by `skill-c`.\nA teach moment."),
                ("skill-b/REFERENCE.md", "See [skill-b](../skill-b/REFERENCE.md) and `skill-b`."),
            ],
        ),
        synthetic("skill-b", &[("SKILL.md", "body")]),
        synthetic("skill-c", &[("SKILL.md", "body")]),
        synthetic("teach", &[("SKILL.md", "body")]),
    ];
    let found = detect_links(&skills, &BTreeSet::new());
    assert!(found.links.is_empty(), "{:#?}", found.links);
}

#[test]
fn optional_names_are_mentions_even_when_they_read_a_path() {
    let skills = [
        synthetic(
            "skill-a",
            &[("SKILL.md", "If available, read ../skill-b/SKILL.md first.")],
        ),
        synthetic("skill-b", &[("SKILL.md", "body")]),
    ];
    let found = detect_links(&skills, &BTreeSet::new());
    assert_eq!(
        strength(&found, "skill-a", "skill-b"),
        Some(LinkStrength::Mentions)
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]
    /// Links do not depend on the order the scan hands the skills over.
    #[test]
    fn links_are_the_same_whatever_order_the_skills_come_in(
        order in Just((0..pack("mattpocock_skills").len()).collect::<Vec<_>>()).prop_shuffle()
    ) {
        let skills = pack("mattpocock_skills");
        let shuffled: Vec<SkillText> = order.iter().map(|&i| skills[i].clone()).collect();
        prop_assert_eq!(detect_links(&shuffled, &BTreeSet::new()), detect_links(&skills, &BTreeSet::new()));
    }
}
