//! Property test for the `agents.toml` `exclude` edits Park and Turn on make
//! (`add_exclude` then `lift_exclude`). The person owns the layout of that
//! file, so for any list layout - one line or many, trailing comma or not,
//! comments above or after items, a commented-out item - adding a name and
//! lifting it again must give back every byte. LF only: `toml_edit` reads a
//! CRLF file as LF, so CRLF never round-trips through it.

use std::fmt::Write as _;

use proptest::prelude::*;

use super::{add_exclude, lift_exclude};

#[derive(Debug, Clone)]
struct Item {
    name: String,
    comment_above: bool,
    tail_comment: bool,
}

fn item() -> impl Strategy<Value = Item> {
    ("[a-e]{1,3}", any::<bool>(), any::<bool>()).prop_map(|(name, comment_above, tail_comment)| {
        Item {
            name,
            comment_above,
            tail_comment,
        }
    })
}

/// One `exclude = [...]` line or block that never lists `foo`.
fn exclude_list() -> impl Strategy<Value = String> {
    let inline = prop::collection::vec("[a-e]{1,3}", 0..4).prop_map(|names| {
        let items: Vec<String> = names.iter().map(|name| format!("\"{name}\"")).collect();
        format!("exclude = [{}]\n", items.join(", "))
    });
    let block = (
        prop::collection::vec(item(), 1..5),
        any::<bool>(),
        any::<bool>(),
        prop_oneof![Just("  "), Just("    "), Just("\t")],
    )
        .prop_map(|(items, trailing_comma, commented_out_after, indent)| {
            let mut text = String::from("exclude = [\n");
            for (at, item) in items.iter().enumerate() {
                if item.comment_above {
                    let _ = writeln!(text, "{indent}# note");
                }
                let _ = write!(text, "{indent}\"{}\"", item.name);
                if at + 1 < items.len() || trailing_comma {
                    text.push(',');
                }
                if item.tail_comment {
                    text.push_str(" # tail");
                }
                text.push('\n');
            }
            if commented_out_after {
                let _ = writeln!(text, "{indent}# \"old\"");
            }
            text.push_str("]\n");
            text
        });
    prop_oneof![inline, block]
}

fn names_in(doc: &toml_edit::DocumentMut) -> Vec<String> {
    doc["skills"][0]["exclude"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item.as_str().map(str::to_string))
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn park_then_turn_on_gives_back_every_byte_of_the_exclude_list(
        exclude in exclude_list(),
        after in prop_oneof![Just(""), Just("\n[[skills]]\nname = \"other\"\n")],
    ) {
        let original = format!(
            "version = 1\n\n[[skills]]\nname = \"*\"\nsource = \"owner/pack\"\n{exclude}{after}"
        );
        let mut doc: toml_edit::DocumentMut = original.parse().unwrap();
        let before = names_in(&doc);
        let row = doc["skills"]
            .as_array_of_tables_mut()
            .unwrap()
            .get_mut(0)
            .unwrap();

        add_exclude(row, "foo");
        let parked = doc.to_string();
        let reparsed: toml_edit::DocumentMut = parked
            .parse()
            .map_err(|e| TestCaseError::fail(format!("park wrote invalid TOML: {e}\n{parked}")))?;
        let mut expected = before.clone();
        expected.push("foo".to_string());
        prop_assert_eq!(names_in(&reparsed), expected, "{}", parked);
        prop_assert_eq!(
            parked.matches('#').count(),
            original.matches('#').count(),
            "park moved or doubled a comment:\n{}",
            parked
        );

        let row = doc["skills"]
            .as_array_of_tables_mut()
            .unwrap()
            .get_mut(0)
            .unwrap();
        let exclude = row["exclude"].as_array_mut().unwrap();
        prop_assert!(lift_exclude(exclude, "foo"));
        prop_assert_eq!(doc.to_string(), original, "parked form:\n{}", parked);
    }
}
