// Integration test binaries aren't covered by the lib crate's
// `cfg_attr(test, allow(...))`: this file compiles as its own crate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Property test for the paged CLI and MCP answers (`view::render_document`).
//! For any list of rows, small or near the byte cap, following
//! `page.next_cursor` from the first page returns every row exactly once,
//! in order, and ends with no cursor. Without an explicit limit, every
//! answer fits `ANSWER_BYTES`.

use proptest::prelude::*;
use serde_json::{json, Value};
use skill_studio_core::ops::Outcome;
use skill_studio_core::view::{render_document, ViewList, ViewRequest, ANSWER_BYTES};

/// An outcome with one pageable list at `/rows`, like `library`.
struct Rows;

impl Outcome for Rows {
    const LISTS: &'static [ViewList<'static>] = &[ViewList::new("/rows", &["/name"])];
}

fn rows() -> impl Strategy<Value = Vec<Value>> {
    // Up to 4 KB per row, so even 60 rows run far past the 32 KB answer cap.
    prop::collection::vec(0usize..4096, 0..60).prop_map(|sizes| {
        sizes
            .into_iter()
            .enumerate()
            .map(|(i, size)| json!({"name": format!("s{i}"), "blob": "x".repeat(size)}))
            .collect()
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn following_cursors_returns_every_row_once_in_order_and_each_answer_fits(
        all in rows(),
        limit in prop::option::of(1usize..10),
    ) {
        let mut seen = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..=all.len() + 1 {
            let document = json!({"status": "ok", "data": {"rows": all.clone()}, "errors": []});
            let view = ViewRequest { limit, cursor: cursor.clone(), ..ViewRequest::default() };
            let rendered = render_document::<Rows>(document, 0, &view, &(), false).unwrap();
            if limit.is_none() {
                let bytes = rendered.json.to_string().len();
                prop_assert!(bytes <= ANSWER_BYTES, "answer of {} bytes", bytes);
            }
            seen.extend(rendered.json["data"]["rows"].as_array().unwrap().iter().cloned());
            cursor = rendered.json["page"]["next_cursor"].as_str().map(str::to_string);
            if cursor.is_none() {
                break;
            }
        }
        prop_assert!(cursor.is_none(), "the cursors never ended");
        prop_assert_eq!(seen, all);
    }
}
