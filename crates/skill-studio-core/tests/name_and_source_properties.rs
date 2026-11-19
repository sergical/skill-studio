// Integration test binaries aren't covered by the lib crate's
// `cfg_attr(test, allow(...))`: this file compiles as its own crate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Property tests for the small parsers that turn user and ledger spellings
//! into identities: the agentskills.io skill name rule, harness spellings,
//! GitHub sources, and the repo key the update check joins on.

use proptest::prelude::*;
use skill_studio_core::dotagents_ledger::github_repo_from_source;
use skill_studio_core::frontmatter::{validate_skill, FrontmatterParseResult, SkillFrontmatter};
use skill_studio_core::identity::AgentId;
use skill_studio_core::skill_update_check::normalize_repo_key;

/// The agentskills.io name rule written another way: 1 to 64 bytes,
/// hyphen-separated words of `a-z0-9`, each word non-empty.
fn reference_name_rule(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name.split('-').all(|word| {
            !word.is_empty()
                && word
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

fn name_format_violation(name: &str) -> bool {
    let frontmatter = SkillFrontmatter {
        name: Some(name.to_string()),
        description: Some("d".to_string()),
        ..SkillFrontmatter::default()
    };
    validate_skill(name, &FrontmatterParseResult::Valid(frontmatter), 5)
        .iter()
        .any(|violation| violation.contains("must be 1-64 lowercase"))
}

fn candidate_name() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-z0-9-]{0,70}",
        "[a-z0-9]{1,8}(-[a-z0-9]{1,8}){0,4}",
        "[ -~]{0,12}",
        "[a-zA-Zé_.-]{1,6}",
    ]
}

/// Every spelling of `canonical` a CLI flag, an MCP call or a desktop id
/// may use: any case, and `-`, `_`, ` ` or nothing between its words.
fn harness_spelling() -> impl Strategy<Value = (String, &'static str)> {
    let canonical = prop::sample::select(vec![
        AgentId::CLAUDE_CODE,
        AgentId::CODEX,
        AgentId::OPEN_CODE,
        AgentId::PI,
        AgentId::CURSOR,
        AgentId::GROK_BUILD,
    ]);
    (canonical, any::<u64>()).prop_map(|(canonical, seed)| {
        // `opencode` is also spelled `open-code`.
        let words: Vec<&str> = if canonical == AgentId::OPEN_CODE {
            vec!["open", "code"]
        } else {
            canonical.split('-').collect()
        };
        let mut spelled = String::new();
        let mut bits = seed;
        for (at, word) in words.iter().enumerate() {
            if at > 0 {
                spelled.push_str(["-", "_", " ", ""][(bits % 4) as usize]);
                bits /= 4;
            }
            for c in word.chars() {
                spelled.push(if bits % 2 == 1 {
                    c.to_ascii_uppercase()
                } else {
                    c
                });
                bits /= 2;
            }
        }
        (spelled, canonical)
    })
}

/// A GitHub `owner/repo` slug. GitHub repo names never end in `.git`.
fn slug() -> impl Strategy<Value = String> {
    (
        "[A-Za-z0-9][A-Za-z0-9-]{0,10}",
        "[A-Za-z0-9_][A-Za-z0-9._-]{0,10}",
    )
        .prop_filter("GitHub drops a .git suffix", |(_, repo)| {
            !repo.to_ascii_lowercase().ends_with(".git")
        })
        .prop_map(|(owner, repo)| format!("{owner}/{repo}"))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn the_skill_name_rule_matches_the_spec_for_any_string(name in candidate_name()) {
        // An empty name is reported as missing, not as badly formed.
        prop_assume!(!name.is_empty());
        prop_assert_eq!(!name_format_violation(&name), reference_name_rule(&name), "{:?}", name);
    }

    #[test]
    fn every_spelling_of_a_harness_parses_to_its_wire_id((spelled, canonical) in harness_spelling()) {
        let parsed = AgentId::parse_harness(&spelled).unwrap();
        prop_assert_eq!(parsed.as_str(), canonical, "{:?}", spelled);
    }

    #[test]
    fn a_github_source_in_any_ledger_spelling_gives_its_slug(slug in slug(), host in "[a-z]{3,8}\\.(com|org)") {
        prop_assert_eq!(github_repo_from_source(&slug), Some(slug.clone()));
        for url in [
            format!("git:https://github.com/{slug}.git"),
            format!("git:https://github.com/{slug}"),
            format!("git:https://github.com/{slug}/"),
            format!("git:git@github.com/{slug}.git"),
        ] {
            prop_assert_eq!(github_repo_from_source(&url), Some(slug.clone()), "{}", url);
        }
        prop_assume!(host != "github.com");
        prop_assert_eq!(github_repo_from_source(&format!("git:https://{host}/{slug}.git")), None);
        prop_assert_eq!(github_repo_from_source(&format!("path:{slug}")), None);
        prop_assert_eq!(github_repo_from_source(&format!("https://github.com/{slug}")), None);
        prop_assert_eq!(github_repo_from_source(&format!("{slug}/extra")), None);
    }

    #[test]
    fn every_spelling_of_a_repo_gives_one_key_and_the_key_is_stable(slug in slug()) {
        let key = normalize_repo_key(&slug);
        prop_assert_eq!(&key, &slug.to_ascii_lowercase());
        prop_assert_eq!(normalize_repo_key(&key), key.clone(), "not idempotent");
        for spelling in [
            slug.to_ascii_uppercase(),
            format!("{slug}.git"),
            format!("github.com/{slug}"),
            format!("GitHub.com/{}.GIT", slug.to_ascii_uppercase()),
        ] {
            prop_assert_eq!(normalize_repo_key(&spelling), key.clone(), "{}", spelling);
        }
    }
}
