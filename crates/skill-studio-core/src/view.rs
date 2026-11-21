//! Bounded views of a result envelope: every surface prints an envelope
//! through [`render`], so a read answer fits in an agent's context by
//! default and the caller can ask for one page, some fields, or counts.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::error::{ErrorCode, ErrorEntry};
use crate::ops::{OpStatus, Outcome, ResultEnvelope};

/// Bytes of the whole envelope when the caller sets no `limit`; the cut
/// lists share what the rest of the envelope leaves.
pub const ANSWER_BYTES: usize = 32 * 1024;

/// What part of a result's lists to return.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ViewRequest {
    /// Rows to return from the main list. `0` returns only `page` (totals
    /// and `counts`). Without it, rows until the answer is 32 KB.
    pub limit: Option<usize>,
    /// Continue one list with its opaque `next_cursor`. Repeat the same scope
    /// and filters. If the cursor is invalid or the list changed, retry
    /// without a cursor.
    pub cursor: Option<String>,
    /// Row fields to keep. Empty keeps every field.
    pub fields: Vec<String>,
    /// Count the rows of the whole main list by this field. A list field
    /// counts each item.
    pub count_by: Option<String>,
}

impl ViewRequest {
    fn is_set(&self) -> bool {
        *self != Self::default()
    }
}

/// How a list in `data` was cut. Present in the envelope only when a list
/// lost rows or the caller set a view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Page {
    /// JSON pointer into `data`; empty when `data` itself is the list.
    pub list: String,
    /// Rows in the whole list.
    pub total: usize,
    /// Rows returned.
    pub returned: usize,
    /// Pass as `cursor` to get the next rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Rows per value of `count_by`, over the whole list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counts: Option<BTreeMap<String, usize>>,
    /// Other lists in `data` that lost rows.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub also: Vec<Page>,
}

/// An envelope as JSON, with the exit status that matches it.
pub struct Rendered {
    /// The envelope document.
    pub json: Value,
    /// `0` ok, `1` issues, `4` partial, else the first error's code.
    pub exit_status: i32,
}

/// Serializes `envelope` with `view` applied to the lists in
/// [`Outcome::LISTS`]. `timings` keeps the envelope's `timings`.
///
/// `request` is the parsed operation input, excluding view controls.
/// An invalid view replaces the result with an `invalid_request` error.
pub fn render<T: Outcome + Serialize>(
    envelope: &ResultEnvelope<T>,
    view: &ViewRequest,
    request: &impl Serialize,
    timings: bool,
) -> Result<Rendered, serde_json::Error> {
    render_document::<T>(
        serde_json::to_value(envelope)?,
        envelope.exit_status(),
        view,
        request,
        timings,
    )
}

/// [`render`] for an envelope already serialized inside a larger document,
/// such as the MCP's labeled answer. Every field of `json` counts toward the
/// answer budget.
pub fn render_document<T: Outcome>(
    mut json: Value,
    mut exit_status: i32,
    view: &ViewRequest,
    request: &impl Serialize,
    timings: bool,
) -> Result<Rendered, serde_json::Error> {
    if !timings {
        if let Value::Object(doc) = &mut json {
            doc.remove("timings");
        }
    }
    let query = Sha256::new_with_prefix(serde_json::to_vec(&(
        &json["operation"],
        &json["scope"],
        request,
    ))?);
    if let Err(message) = fit_answer(&mut json, T::LISTS, T::NESTED_LIST, view, &query)? {
        if let Value::Object(doc) = &mut json {
            let error = ErrorEntry {
                code: ErrorCode::InvalidRequest,
                message,
                path: None,
                detail: None,
            };
            doc.insert("status".into(), serde_json::to_value(OpStatus::Error)?);
            doc.insert("data".into(), Value::Null);
            doc.remove("page");
            doc.insert("errors".into(), serde_json::to_value([error])?);
        }
        exit_status = ErrorCode::InvalidRequest.exit_status();
    }
    Ok(Rendered { json, exit_status })
}

/// Applies `view` to the lists of the envelope `json` and adds its `page`.
/// Rows go until the whole document, `page` included, is `ANSWER_BYTES`.
fn fit_answer(
    json: &mut Value,
    lists: &[&str],
    nested_list: Option<&str>,
    view: &ViewRequest,
    query: &Sha256,
) -> serde_json::Result<Result<(), String>> {
    let rest = bytes_without_lists(json, lists);
    let Value::Object(doc) = json else {
        return Ok(Ok(()));
    };
    let Some(data) = doc.get("data").filter(|data| !data.is_null()).cloned() else {
        return Ok(Ok(()));
    };
    let mut budget = ANSWER_BYTES.saturating_sub(rest);
    loop {
        let mut viewed = data.clone();
        let page = match apply(&mut viewed, lists, nested_list, view, budget, query) {
            Ok(page) => page,
            Err(message) => return Ok(Err(message)),
        };
        doc.insert("data".into(), viewed);
        match page {
            Some(page) => doc.insert("page".into(), serde_json::to_value(page)?),
            None => doc.remove("page"),
        };
        let over = serde_json::to_string(&*doc)?
            .len()
            .saturating_sub(ANSWER_BYTES);
        if over == 0 {
            return Ok(Ok(()));
        }
        if budget == 0 {
            return Ok(if nested_list.is_some() {
                Err(OVERSIZED_ROW.into())
            } else {
                Ok(())
            });
        }
        budget = budget.saturating_sub(over);
    }
}

/// Bytes of the envelope with every list in `lists` left empty.
fn bytes_without_lists(json: &mut Value, lists: &[&str]) -> usize {
    let taken: Vec<_> = lists
        .iter()
        .map(|pointer| format!("/data{pointer}"))
        .filter_map(|pointer| {
            let list = std::mem::take(json.pointer_mut(&pointer)?);
            Some((pointer, list))
        })
        .collect();
    let bytes = json.to_string().len();
    for (pointer, list) in taken {
        if let Some(slot) = json.pointer_mut(&pointer) {
            *slot = list;
        }
    }
    bytes
}

fn apply(
    data: &mut Value,
    lists: &[&str],
    nested_list: Option<&str>,
    view: &ViewRequest,
    mut budget: usize,
    query: &Sha256,
) -> Result<Option<Page>, String> {
    let Some(&main) = lists.first() else {
        return if view.cursor.is_some() {
            Err(INVALID_CURSOR.into())
        } else if view.is_set() {
            Err("this result has no list to page, project or count".into())
        } else {
            Ok(None)
        };
    };
    let (target, offset) = parse_cursor(view.cursor.as_deref(), main)?;
    if let Some(field) = nested_list {
        if target == main {
            let oversized = view.limit != Some(0)
                && data
                    .pointer(main)
                    .and_then(|rows| rows.get(offset))
                    .is_some_and(|row| {
                        let mut row = row.clone();
                        project_row(&mut row, &view.fields);
                        row.to_string().len() + 1 > budget
                    });
            if oversized {
                let nested = NestedListPage {
                    parent: main,
                    field,
                    row: offset,
                };
                return nested.render(data, view, budget, query, None).map(Some);
            }
        } else if let Some(row) = target
            .strip_prefix(main)
            .and_then(|tail| tail.strip_prefix('/'))
            .and_then(|tail| tail.strip_suffix(field))
            .and_then(|index| index.parse().ok())
        {
            let nested = NestedListPage {
                parent: main,
                field,
                row,
            };
            return nested
                .render(data, view, budget, query, Some(offset))
                .map(Some);
        }
    }
    if !lists.contains(&target) {
        return Err(INVALID_CURSOR.into());
    }
    let min_rows = usize::from(nested_list.is_none() || target != main);
    let Some(mut page) = cut(
        data.pointer_mut(target),
        target,
        offset,
        view,
        &mut budget,
        min_rows,
        query,
    )?
    else {
        return Ok(None);
    };
    let other_view = ViewRequest {
        limit: (view.limit == Some(0) || view.cursor.is_some()).then_some(0),
        ..ViewRequest::default()
    };
    for pointer in lists.iter().filter(|pointer| **pointer != target) {
        let other = cut(
            data.pointer_mut(pointer),
            pointer,
            0,
            &other_view,
            &mut budget,
            0,
            query,
        )?;
        if let Some(other) = other {
            if other.returned < other.total {
                page.also.push(other);
            }
        }
    }
    let cut_any = page.returned < page.total || !page.also.is_empty();
    Ok((cut_any || view.is_set()).then_some(page))
}

const OVERSIZED_ROW: &str =
    "Pack answer exceeds 32 KB. Request fewer fields or omit count_by, then try again.";

struct NestedListPage<'a> {
    parent: &'a str,
    field: &'a str,
    row: usize,
}

impl NestedListPage<'_> {
    fn render(
        &self,
        data: &mut Value,
        view: &ViewRequest,
        mut budget: usize,
        query: &Sha256,
        continuation: Option<usize>,
    ) -> Result<Page, String> {
        let parent = data.pointer_mut(self.parent).ok_or(INVALID_CURSOR)?;
        let seed = list_cursor_seed(query, self.parent, parent);
        let mut parent_view = if continuation.is_some() {
            ViewRequest::default()
        } else {
            view.clone()
        };
        parent_view.limit = Some(1);
        let mut parent_page = cut(
            Some(&mut *parent),
            self.parent,
            self.row,
            &parent_view,
            &mut budget,
            1,
            query,
        )?
        .ok_or(INVALID_CURSOR)?;
        let row = parent.get_mut(0).ok_or(INVALID_CURSOR)?;

        // Count the pack metadata before fitting any of its skill rows.
        let missing_list = if continuation.is_some() {
            INVALID_CURSOR
        } else {
            OVERSIZED_ROW
        };
        let mut rows = std::mem::replace(
            row.pointer_mut(self.field)
                .filter(|list| list.is_array())
                .ok_or(missing_list)?,
            Value::Array(Vec::new()),
        );
        budget = budget.saturating_sub(row.to_string().len() + 1);

        let target = format!("{}/{}{}", self.parent, self.row, self.field);
        let default_view = ViewRequest::default();
        let nested_view = if continuation.is_some() {
            view
        } else {
            &default_view
        };
        let mut page = cut(
            Some(&mut rows),
            &target,
            continuation.unwrap_or_default(),
            nested_view,
            &mut budget,
            0,
            &seed,
        )?
        .ok_or(missing_list)?;
        if page.returned == 0 && page.next_cursor.is_some() && nested_view.limit != Some(0) {
            return Err(OVERSIZED_ROW.into());
        }
        *row.pointer_mut(self.field).ok_or(missing_list)? = rows;
        page.list = format!("{}/0{}", self.parent, self.field);
        if continuation.is_some() {
            return Ok(page);
        }
        parent_page.also.push(page);
        Ok(parent_page)
    }
}

const INVALID_CURSOR: &str = "Invalid or stale page cursor. Start again without a cursor.";

fn parse_cursor<'a>(cursor: Option<&'a str>, main: &'a str) -> Result<(&'a str, usize), String> {
    let Some(cursor) = cursor else {
        return Ok((main, 0));
    };
    let invalid = || INVALID_CURSOR.to_owned();
    let (position, _) = cursor
        .strip_prefix("v1@")
        .and_then(|s| s.rsplit_once('@'))
        .ok_or_else(invalid)?;
    let (list, offset) = position.rsplit_once('@').ok_or_else(invalid)?;
    Ok((list, offset.parse().map_err(|_| invalid())?))
}

fn list_cursor_seed(query: &Sha256, pointer: &str, list: &Value) -> Sha256 {
    let mut digest = query.clone();
    digest.update([0]);
    digest.update(pointer);
    digest.update([0]);
    digest.update(list.to_string());
    digest
}

fn page_cursor(seed: &Sha256, pointer: &str, offset: usize) -> String {
    let mut digest = seed.clone();
    digest.update([0]);
    digest.update(offset.to_string());
    let mut cursor = format!("v1@{pointer}@{offset}@");
    for byte in digest.finalize().iter().take(16) {
        let _ = write!(cursor, "{byte:02x}");
    }
    cursor
}

fn cut(
    slot: Option<&mut Value>,
    pointer: &str,
    offset: usize,
    view: &ViewRequest,
    budget: &mut usize,
    min_rows: usize,
    query: &Sha256,
) -> Result<Option<Page>, String> {
    let Some(slot) = slot.filter(|slot| slot.is_array() || slot.is_object()) else {
        return if view.cursor.is_some() {
            Err(INVALID_CURSOR.into())
        } else {
            Ok(None)
        };
    };
    let cursor_seed = list_cursor_seed(query, pointer, slot);
    let (is_map, rows): (bool, Vec<(Option<String>, Value)>) = match std::mem::take(slot) {
        Value::Array(items) => (false, items.into_iter().map(|row| (None, row)).collect()),
        Value::Object(map) => (
            true,
            map.into_iter().map(|(k, row)| (Some(k), row)).collect(),
        ),
        other => {
            *slot = other;
            return Ok(None);
        }
    };
    let total = rows.len();
    if let Some(cursor) = &view.cursor {
        if offset >= total || *cursor != page_cursor(&cursor_seed, pointer, offset) {
            return Err(INVALID_CURSOR.into());
        }
    }
    check_fields(&rows, view)?;
    let counts = view.count_by.as_deref().map(|key| count(&rows, key));
    let mut kept: Vec<_> = rows.into_iter().skip(offset).collect();
    if !view.fields.is_empty() {
        for (_, row) in &mut kept {
            project_row(row, &view.fields);
        }
    }
    if let Some(limit) = view.limit {
        kept.truncate(limit);
    }
    // A zero minimum lets the caller split an oversized primary row.
    let returned = if view.limit.is_none() || min_rows == 0 {
        fit(&kept, budget, min_rows)
    } else {
        kept.len()
    };
    kept.truncate(returned);
    let next = offset.saturating_add(returned);
    *slot = if is_map {
        Value::Object(
            kept.into_iter()
                .filter_map(|(key, row)| Some((key?, row)))
                .collect::<Map<_, _>>(),
        )
    } else {
        Value::Array(kept.into_iter().map(|(_, row)| row).collect())
    };
    Ok(Some(Page {
        list: pointer.to_owned(),
        total,
        returned,
        next_cursor: (next < total).then(|| page_cursor(&cursor_seed, pointer, next)),
        counts,
        also: Vec::new(),
    }))
}

/// Rows that fit in `budget`, and at least `min`; takes their bytes from it.
fn fit(rows: &[(Option<String>, Value)], budget: &mut usize, min: usize) -> usize {
    let mut fitting = 0;
    for (key, row) in rows {
        let bytes = key.as_ref().map_or(0, |key| key.len() + 3) + row.to_string().len() + 1;
        if bytes > *budget && fitting >= min {
            break;
        }
        *budget = budget.saturating_sub(bytes);
        fitting += 1;
    }
    fitting
}

fn project_row(row: &mut Value, fields: &[String]) {
    if !fields.is_empty() {
        if let Value::Object(row) = row {
            row.retain(|key, _| fields.contains(key));
        }
    }
}

fn check_fields(rows: &[(Option<String>, Value)], view: &ViewRequest) -> Result<(), String> {
    if rows.is_empty() {
        return Ok(());
    }
    let known: std::collections::BTreeSet<&str> = rows
        .iter()
        .filter_map(|(_, row)| row.as_object())
        .flat_map(|row| row.keys().map(String::as_str))
        .collect();
    let unknown = view
        .fields
        .iter()
        .chain(&view.count_by)
        .find(|field| !known.contains(field.as_str()));
    match unknown {
        Some(field) => Err(format!(
            "`{field}` is not a row field; fields: {}",
            known.into_iter().collect::<Vec<_>>().join(", ")
        )),
        None => Ok(()),
    }
}

fn count(rows: &[(Option<String>, Value)], key: &str) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    let label = |value: &Value| match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    for (_, row) in rows {
        match row.get(key) {
            Some(Value::Array(items)) => {
                for item in items {
                    *counts.entry(label(item)).or_default() += 1;
                }
            }
            Some(value) => *counts.entry(label(value)).or_default() += 1,
            None => {}
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn apply(
        data: &mut Value,
        lists: &[&str],
        view: &ViewRequest,
        budget: usize,
    ) -> Result<Option<Page>, String> {
        super::apply(data, lists, None, view, budget, &Sha256::new())
    }

    fn fit_answer(
        json: &mut Value,
        lists: &[&str],
        view: &ViewRequest,
    ) -> serde_json::Result<Result<(), String>> {
        super::fit_answer(json, lists, None, view, &Sha256::new())
    }

    fn rows(n: usize) -> Value {
        json!({
            "rows": (0..n)
                .map(|i| json!({"name": format!("s{i}"), "source": if i % 2 == 0 { json!("a/b") } else { Value::Null }, "agents": ["x", "y"]}))
                .collect::<Vec<_>>(),
            "unused": (0..n).map(|i| format!("s{i}")).collect::<Vec<_>>(),
        })
    }

    fn view(
        limit: Option<usize>,
        cursor: Option<&str>,
        fields: &[&str],
        count_by: Option<&str>,
    ) -> ViewRequest {
        ViewRequest {
            limit,
            cursor: cursor.map(str::to_owned),
            fields: fields.iter().map(|f| (*f).to_owned()).collect(),
            count_by: count_by.map(str::to_owned),
        }
    }

    #[test]
    fn no_view_and_a_small_list_leaves_data_alone() {
        let mut data = rows(3);
        let before = data.clone();
        assert_eq!(
            apply(&mut data, &["/rows"], &ViewRequest::default(), ANSWER_BYTES),
            Ok(None)
        );
        assert_eq!(data, before);
    }

    #[test]
    fn a_list_over_the_byte_cap_keeps_the_rows_that_fit_and_a_cursor() {
        let mut data = rows(2_000);
        let page = apply(
            &mut data,
            &["/rows", "/unused"],
            &ViewRequest::default(),
            ANSWER_BYTES,
        )
        .unwrap()
        .unwrap();
        let kept = data["rows"].as_array().unwrap();
        assert_eq!(page.total, 2_000);
        assert_eq!(page.returned, kept.len());
        assert!(kept.len() < 2_000);
        assert!(data["rows"].to_string().len() <= ANSWER_BYTES);
        assert!(page.next_cursor.is_some());
        assert_eq!(page.also[0].list, "/unused", "{:?}", page.also);
    }

    #[test]
    fn every_cut_list_shares_one_answer_budget() {
        let mut doc = json!({"status": "ok", "data": {"rows": rows(2_000)["rows"], "unused": rows(2_000)["rows"]}});
        fit_answer(&mut doc, &["/rows", "/unused"], &ViewRequest::default())
            .unwrap()
            .unwrap();
        let page = &doc["page"];
        assert!(doc.to_string().len() <= ANSWER_BYTES, "{page}");
        assert_eq!(page["also"].as_array().map(Vec::len), Some(1), "{page}");
        assert!(
            page["returned"].as_u64() > page["also"][0]["returned"].as_u64(),
            "{page}"
        );
    }

    #[test]
    fn the_page_object_counts_toward_the_answer_budget() {
        let mut doc = json!({"status": "ok", "data": rows(1_500)});
        fit_answer(
            &mut doc,
            &["/rows", "/unused"],
            &view(None, None, &[], Some("name")),
        )
        .unwrap()
        .unwrap();
        assert_eq!(doc["page"]["counts"].as_object().map(Map::len), Some(1_500));
        assert!(
            doc.to_string().len() <= ANSWER_BYTES,
            "{}",
            doc.to_string().len()
        );
    }

    #[test]
    fn a_cursor_continues_its_own_list_and_no_list_repeats_rows() {
        let lists = ["/rows", "/unused"];
        let mut data = rows(2_000);
        let first = apply(&mut data, &lists, &ViewRequest::default(), ANSWER_BYTES)
            .unwrap()
            .unwrap();
        let unused = &first.also[0];
        let cursor = unused.next_cursor.as_deref().unwrap();

        let mut data = rows(2_000);
        let next = apply(
            &mut data,
            &lists,
            &view(None, Some(cursor), &[], None),
            ANSWER_BYTES,
        )
        .unwrap()
        .unwrap();
        assert_eq!(next.list, "/unused");
        assert_eq!(data["unused"][0], format!("s{}", unused.returned));
        assert_eq!(data["rows"], json!([]));
        assert_eq!(next.also[0].returned, 0);
        assert!(next.also[0].next_cursor.is_some());

        let mut data = rows(2_000);
        let main = apply(
            &mut data,
            &lists,
            &view(None, first.next_cursor.as_deref(), &[], None),
            ANSWER_BYTES,
        )
        .unwrap()
        .unwrap();
        assert_eq!(data["rows"][0]["name"], format!("s{}", first.returned));
        assert_eq!(data["unused"], json!([]));
        assert_eq!(main.also[0].returned, 0);
        assert!(main.also[0].next_cursor.is_some());
    }

    #[test]
    fn fields_project_rows_and_counts_cover_the_whole_list() {
        let mut data = rows(5);
        let page = apply(
            &mut data,
            &["/rows"],
            &view(Some(1), None, &["name"], Some("source")),
            ANSWER_BYTES,
        )
        .unwrap()
        .unwrap();
        assert_eq!(data["rows"], json!([{"name": "s0"}]));
        assert_eq!(
            page.counts,
            Some(BTreeMap::from([("a/b".into(), 3), ("null".into(), 2)]))
        );
        let mut data = rows(5);
        let page = apply(
            &mut data,
            &["/rows"],
            &view(Some(0), None, &[], Some("agents")),
            ANSWER_BYTES,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            page.counts,
            Some(BTreeMap::from([("x".into(), 5), ("y".into(), 5)]))
        );
    }

    #[test]
    fn limit_zero_empties_every_list_and_reports_the_others() {
        let mut data = rows(4);
        let page = apply(
            &mut data,
            &["/rows", "/unused"],
            &view(Some(0), None, &[], None),
            ANSWER_BYTES,
        )
        .unwrap()
        .unwrap();
        assert_eq!((page.total, page.returned), (4, 0));
        assert_eq!(page.also.len(), 1);
        assert_eq!(
            (page.also[0].list.as_str(), page.also[0].total),
            ("/unused", 4)
        );
        assert_eq!(data["unused"], json!([]));
    }

    #[test]
    fn a_map_list_pages_by_key_order() {
        let mut data = json!({"a": {"v": 1}, "b": {"v": 2}, "c": {"v": 3}});
        let first = apply(
            &mut data.clone(),
            &[""],
            &view(Some(1), None, &[], None),
            ANSWER_BYTES,
        )
        .unwrap()
        .unwrap();
        let page = apply(
            &mut data,
            &[""],
            &view(Some(1), first.next_cursor.as_deref(), &[], None),
            ANSWER_BYTES,
        )
        .unwrap()
        .unwrap();
        assert_eq!(data, json!({"b": {"v": 2}}));
        assert!(page.next_cursor.is_some());
    }

    #[test]
    fn an_unknown_field_a_bad_cursor_or_no_list_is_an_invalid_request() {
        let err = apply(
            &mut rows(2),
            &["/rows"],
            &view(None, None, &["nmae"], None),
            ANSWER_BYTES,
        )
        .unwrap_err();
        assert!(err.contains("`nmae`") && err.contains("name"), "{err}");
        assert!(apply(
            &mut rows(2),
            &["/rows"],
            &view(None, None, &[], Some("x")),
            ANSWER_BYTES
        )
        .is_err());
        assert!(apply(
            &mut rows(2),
            &["/rows"],
            &view(None, Some("x"), &[], None),
            ANSWER_BYTES
        )
        .is_err());
        assert!(apply(
            &mut rows(2),
            &["/rows"],
            &view(None, Some("/nope@0"), &[], None),
            ANSWER_BYTES
        )
        .is_err());
        assert!(apply(
            &mut rows(2),
            &[],
            &view(Some(1), None, &[], None),
            ANSWER_BYTES
        )
        .is_err());
        assert_eq!(
            apply(&mut rows(2), &[], &ViewRequest::default(), ANSWER_BYTES),
            Ok(None)
        );
    }
}
