// Integration test binaries aren't covered by the lib crate's
// `cfg_attr(test, allow(...))`: this file compiles as its own crate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Property tests for the SKILL.md frontmatter repairs. For random
//! frontmatter (key order, extra keys, nested blocks, LF or CRLF, a final
//! newline or not, any body) with one repairable fault:
//! - the repaired text parses and keeps the repaired value;
//! - every byte before and after the repaired line is unchanged, so the
//!   body and the other keys survive and CRLF stays CRLF;
//! - repairing the repaired text proposes nothing (the repair is idempotent).

use proptest::prelude::*;
use skill_studio_core::frontmatter::{parse_frontmatter, FrontmatterParseResult};
use skill_studio_core::frontmatter_repair::{
    propose_colon_scalar_repair, propose_repair, FrontmatterRepairKind,
};

fn eol() -> impl Strategy<Value = &'static str> {
    prop_oneof![Just("\n"), Just("\r\n")]
}

fn skill_name() -> impl Strategy<Value = String> {
    "[a-z0-9]{1,8}(-[a-z0-9]{1,8}){0,2}"
}

/// A plain scalar with at least one `: ` inside: the fault the colon repair
/// fixes. Starts with a letter, has no ` #` and does not end in `:`.
fn colon_scalar() -> impl Strategy<Value = String> {
    (
        "[A-Za-z][A-Za-z0-9]{0,6}",
        prop::collection::vec(
            (
                "[A-Za-z0-9(),.\"'%-]{1,6}",
                prop_oneof![Just(" "), Just(": ")],
            ),
            1..5,
        ),
    )
        .prop_map(|(first, rest)| {
            let mut value = format!("{first}: ");
            for (word, gap) in &rest {
                value.push_str(word);
                value.push_str(gap);
            }
            value.trim_end_matches([' ', ':']).to_string()
        })
        .prop_filter("needs text after the colon", |value| {
            !value.ends_with(':') && value.contains(": ") && !value.trim_end().ends_with(": ")
        })
}

/// Other top-level keys, each at most once (a repeated key is a YAML
/// error of its own), in random order.
fn extra_lines() -> impl Strategy<Value = Vec<String>> {
    let lines = vec![
        "license: MIT",
        "# a comment",
        "metadata:\n  author: someone",
        "allowed-tools: Read, Grep",
    ];
    prop::sample::subsequence(lines, 0..=4)
        .prop_shuffle()
        .prop_map(|lines| lines.into_iter().map(str::to_string).collect())
}

fn body() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec("[ -~]{0,16}", 0..4)
}

struct Doc {
    content: String,
    /// Byte range of the line the repair may rewrite, ending before its eol.
    target: std::ops::Range<usize>,
    eol: &'static str,
}

fn build(
    eol: &'static str,
    first: &str,
    second: &str,
    target_first: bool,
    extra: &[String],
    body: &[String],
    final_newline: bool,
) -> Doc {
    let mut content = format!("---{eol}");
    let [first_range, second_range] = [first, second].map(|line| {
        let start = content.len();
        content.push_str(line);
        let range = start..content.len();
        content.push_str(eol);
        range
    });
    for line in extra {
        content.push_str(&line.replace('\n', eol));
        content.push_str(eol);
    }
    content.push_str("---");
    for line in body {
        content.push_str(eol);
        content.push_str(line);
    }
    if final_newline {
        content.push_str(eol);
    }
    Doc {
        content,
        target: if target_first {
            first_range
        } else {
            second_range
        },
        eol,
    }
}

fn assert_only_target_changed(doc: &Doc, proposed: &str) -> Result<(), TestCaseError> {
    let before = &doc.content[..doc.target.start];
    let after = &doc.content[doc.target.end..];
    prop_assert!(
        proposed.starts_with(before),
        "bytes before the line changed"
    );
    prop_assert!(proposed.ends_with(after), "bytes after the line changed");
    if doc.eol == "\r\n" {
        prop_assert!(
            !proposed.replace("\r\n", "").contains('\n'),
            "a CRLF file gained a bare LF"
        );
    }
    Ok(())
}

fn parsed(content: &str) -> skill_studio_core::frontmatter::SkillFrontmatter {
    match parse_frontmatter(content) {
        FrontmatterParseResult::Valid(parsed) => parsed,
        other => panic!("does not parse: {other:?}\n{content}"),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn a_colon_scalar_repair_parses_keeps_the_value_and_touches_only_its_line(
        eol in eol(),
        name in skill_name(),
        broken in colon_scalar(),
        broken_is_description in any::<bool>(),
        name_first in any::<bool>(),
        extra in extra_lines(),
        body in body(),
        final_newline in any::<bool>(),
    ) {
        let (name_line, description_line) = if broken_is_description {
            (format!("name: {name}"), format!("description: {broken}"))
        } else {
            (format!("name: {broken}"), "description: A plain description".to_string())
        };
        let (first, second) = if name_first {
            (name_line, description_line)
        } else {
            (description_line, name_line)
        };
        let doc = build(eol, &first, &second, name_first != broken_is_description, &extra, &body, final_newline);

        let (proposed, _) = propose_colon_scalar_repair(&doc.content)
            .map_err(|e| TestCaseError::fail(format!("{e}\n{:?}", doc.content)))?;
        assert_only_target_changed(&doc, &proposed)?;
        let repaired = parsed(&proposed);
        if broken_is_description {
            prop_assert_eq!(repaired.description.as_deref(), Some(broken.as_str()));
            prop_assert_eq!(repaired.name.as_deref(), Some(name.as_str()));
        } else {
            prop_assert_eq!(repaired.name.as_deref(), Some(broken.as_str()));
            prop_assert_eq!(repaired.description.as_deref(), Some("A plain description"));
        }
        prop_assert!(propose_colon_scalar_repair(&proposed).is_err(), "a second repair proposed a change");
    }

    #[test]
    fn a_name_repair_sets_the_folder_name_and_touches_only_the_name_line(
        eol in eol(),
        old_name in skill_name(),
        folder in skill_name(),
        quote in prop_oneof![Just(""), Just("\""), Just("'")],
        comment in prop_oneof![Just(""), Just(" # why")],
        name_first in any::<bool>(),
        extra in extra_lines(),
        body in body(),
        final_newline in any::<bool>(),
    ) {
        prop_assume!(old_name != folder);
        let name_line = format!("name: {quote}{old_name}{quote}{comment}");
        let description_line = "description: Use it when needed".to_string();
        let (first, second) = if name_first {
            (name_line, description_line)
        } else {
            (description_line, name_line)
        };
        let doc = build(eol, &first, &second, name_first, &extra, &body, final_newline);

        let (proposed, _) =
            propose_repair(FrontmatterRepairKind::NameMismatch, &doc.content, &folder, None)
                .map_err(|e| TestCaseError::fail(format!("{e}\n{:?}", doc.content)))?;
        assert_only_target_changed(&doc, &proposed)?;
        let line_start = doc.target.start;
        let new_line = proposed[line_start..].split(eol).next().unwrap();
        prop_assert_eq!(new_line, format!("name: {quote}{folder}{quote}{comment}"));
        prop_assert_eq!(parsed(&proposed).name, Some(folder.clone()));
        prop_assert!(
            propose_repair(FrontmatterRepairKind::NameMismatch, &proposed, &folder, None).is_err(),
            "a second repair proposed a change"
        );
    }
}
