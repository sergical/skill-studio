//! pi v3 JSONL on the last saved branch. Only the structured skills field is evidence.
use super::{
    jsonl_files, newest, read_lines, subdirs, temporary_dir, ParsedSession, RecordedEntry,
};
use crate::error::CoreError;
use crate::identity::RootScope;
use crate::ports::{OpContext, Runtime};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

type Listing = Vec<(String, RecordedEntry)>;

// The formatter escapes all five XML entities. Reject other markup or entities as an
// unsupported section. No transcript string is interpreted as HTML.
fn xml_text(input: &mut &str, tag: &str) -> Option<String> {
    let body = input.trim_start().strip_prefix(&format!("<{tag}>"))?;
    let (text, rest) = body.split_once(&format!("</{tag}>"))?;
    if text.contains(['<', '>']) {
        return None;
    }
    let mut decoded = String::new();
    let mut remaining = text;
    while let Some((before, entity)) = remaining.split_once('&') {
        decoded.push_str(before);
        let (entity, tail) = entity.split_once(';')?;
        decoded.push(match entity {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ => return None,
        });
        remaining = tail;
    }
    decoded.push_str(remaining);
    *input = rest;
    Some(decoded)
}

fn saved_pi_listing(section: &str) -> Option<Listing> {
    let formatted = section
        .trim()
        .strip_prefix("<skills>")?
        .strip_suffix("</skills>")?
        .trim();
    let (_, body) = formatted.split_once("\n<available_skills>")?;
    let mut body = body.strip_suffix("</available_skills>")?.trim();
    let mut entries = Vec::new();
    while !body.is_empty() {
        body = body.strip_prefix("<skill>")?;
        let name = xml_text(&mut body, "name")?;
        let text = xml_text(&mut body, "description")?;
        let locator = xml_text(&mut body, "location")?;
        body = body.trim_start().strip_prefix("</skill>")?.trim();
        entries.push((name, RecordedEntry::Saved { text, locator }));
    }
    Some(entries)
}

enum SkillSectionPatch {
    Unchanged,
    Unavailable,
    Clear,
    Section(String),
    RemoveMessage(String),
}
impl SkillSectionPatch {
    fn from_record(row: &Value) -> Self {
        let msg = match row["type"].as_str() {
            Some("message") if row["message"]["role"] == "system" => &row["message"],
            Some("compaction") => &row["systemMessage"],
            Some("context_edit") if row["replacement"].is_null() => {
                return row["targetId"]
                    .as_str()
                    .map_or(Self::Unchanged, |id| Self::RemoveMessage(id.into()))
            }
            _ => return Self::Unchanged,
        };
        match msg["sections"].get("skills") {
            Some(Value::String(section)) => Self::Section(section.clone()),
            Some(Value::Null) => Self::Clear,
            None if row["type"] != "compaction"
                && msg["content"].as_str().unwrap_or("").is_empty() =>
            {
                Self::Unchanged
            }
            _ => Self::Unavailable,
        }
    }
}

fn read_pi_session(rt: &Runtime, path: &Path) -> Option<ParsedSession> {
    let mut session = ParsedSession {
        transcript: path.into(),
        ..ParsedSession::default()
    };
    let mut nodes = BTreeMap::new();
    let mut leaf = None;
    let mut supported = false;
    read_lines(rt, path, |line| {
        let Ok(row) = serde_json::from_slice::<Value>(line) else {
            return;
        };
        if row["type"] == "session" {
            session.id = row["id"].as_str().map(String::from);
            session.project_path = row["cwd"].as_str().map(String::from);
            session.at = row["timestamp"].as_str().map(String::from);
            supported = row["version"] == 3;
        } else if let Some(id) = row["id"].as_str() {
            nodes.insert(
                id.to_owned(),
                (
                    row["parentId"].as_str().map(String::from),
                    SkillSectionPatch::from_record(&row),
                ),
            );
            leaf = Some(id.to_owned());
        }
    });
    session.id.as_ref()?;
    let mut removed = BTreeSet::new();
    while let Some((parent, patch)) = leaf.as_ref().and_then(|id| nodes.remove(id)) {
        let skip = leaf.as_ref().is_some_and(|id| removed.contains(id));
        leaf = parent;
        if !supported {
            break;
        }
        if skip {
            continue;
        }
        match patch {
            SkillSectionPatch::Section(section) => {
                session.listing = saved_pi_listing(&section);
                break;
            }
            SkillSectionPatch::Clear => {
                session.listing = Some(Vec::new());
                break;
            }
            SkillSectionPatch::Unavailable => break,
            SkillSectionPatch::RemoveMessage(target) => {
                removed.insert(target);
            }
            SkillSectionPatch::Unchanged => {}
        }
    }
    Some(session)
}

pub(super) fn read_pi_sessions(
    rt: &Runtime,
    ctx: &OpContext,
    scope: &RootScope,
    since: DateTime<Utc>,
    limit: usize,
) -> Result<Vec<ParsedSession>, CoreError> {
    let dir = rt.scope.global_root_path(Path::new(".pi/agent/sessions"));
    let mut files = Vec::new();
    for folder in subdirs(rt, &dir) {
        jsonl_files(rt, &folder, &mut files);
    }
    let mut sessions = Vec::new();
    for path in newest(files, since, usize::MAX) {
        ctx.checkpoint()?;
        if let Some(session) = read_pi_session(rt, &path) {
            let cwd = session.project_path.as_deref();
            if cwd.is_some_and(|p| temporary_dir(rt, p))
                || matches!(scope, RootScope::Project(project) if cwd.map(Path::new) != Some(&project.0))
            {
                continue;
            }
            sessions.push(session);
            if sessions.len() == limit {
                break;
            }
        }
    }
    Ok(sessions)
}
