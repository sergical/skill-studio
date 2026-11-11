//! Saved Dotagents entry policy for Remove and Restore.

use std::ops::Range;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, ErrorCode};
use crate::ports::{Runtime, ScopeFs};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SavedEntries {
    files: Vec<SavedEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum EntryKind {
    Manifest,
    Lock,
    Ignore,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum EntryPostState {
    Unrecorded,
    Recorded(SlotBytes),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SlotBytes {
    text: String,
    header_offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SavedEntry {
    path: PathBuf,
    source: PathBuf,
    kind: EntryKind,
    skill: String,
    previous: SlotBytes,
    shared_prefix: Option<String>,
    previous_file_absent: bool,
    expected: EntryPostState,
    preceding: Option<String>,
    following: Option<String>,
}

pub(crate) struct PreparedEntries {
    pub files: Vec<(PathBuf, Option<Vec<u8>>)>,
    pub inverse: serde_json::Value,
}

struct Header {
    start: usize,
    raw_start: usize,
    key: String,
    owned: bool,
}

fn invalid(path: &Path, message: impl Into<String>) -> CoreError {
    CoreError::new(ErrorCode::InvalidRequest, message).at(path)
}

fn conflict(path: &Path) -> CoreError {
    CoreError::new(
        ErrorCode::DriftConflict,
        "The saved Dotagents entry's place changed. Undo cannot restore it without changing another entry.",
    ).at(path)
}

fn read_text(fs: &dyn ScopeFs, path: &Path) -> Result<String, CoreError> {
    match fs.read_capped(path, crate::dotagents_ledger::DOTAGENTS_FILE_MAX_BYTES) {
        Ok(bytes) => String::from_utf8(bytes).map_err(|error| invalid(path, error.to_string())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(CoreError::io(path, error)),
    }
}

fn header_start(table: &toml_edit::Table, text: &str) -> Option<usize> {
    if table.is_implicit() {
        return None;
    }
    let header = table.span()?;
    let prefix = table.decor().prefix().and_then(toml_edit::RawString::span);
    let mut start = prefix.map_or(header.start, |range| range.start);
    for line in text[start..header.start].split_inclusive('\n') {
        if !line.trim().is_empty() {
            break;
        }
        start += line.len();
    }
    Some(start)
}

fn collect_headers(
    table: &toml_edit::Table,
    text: &str,
    path: &mut Vec<String>,
    owned: bool,
    manifest: bool,
    skill: &str,
    headers: &mut Vec<Header>,
) {
    if let Some(start) = (!path.is_empty())
        .then(|| header_start(table, text))
        .flatten()
    {
        headers.push(Header {
            start,
            raw_start: table.span().map_or(start, |span| span.start),
            key: format!("{path:?}"),
            owned,
        });
    }
    for (key, item) in table {
        path.push(key.to_string());
        if let Some(child) = item.as_table() {
            let own =
                owned || (!manifest && path.len() == 2 && path[0] == "skills" && key == skill);
            collect_headers(child, text, path, own, manifest, skill, headers);
        } else if let Some(children) = item.as_array_of_tables() {
            for (index, child) in children.iter().enumerate() {
                let name = child.get("name").and_then(toml_edit::Item::as_str);
                let own = owned
                    || (manifest && path.len() == 1 && key == "skills" && name == Some(skill));
                let identity = name.map_or_else(|| format!("#{index}"), str::to_string);
                path.push(identity);
                collect_headers(child, text, path, own, manifest, skill, headers);
                path.pop();
            }
        }
        path.pop();
    }
}

fn parse_headers(
    text: &str,
    path: &Path,
    kind: EntryKind,
    skill: &str,
) -> Result<Vec<Header>, CoreError> {
    if kind == EntryKind::Ignore {
        let mut start = 0;
        return Ok(text
            .split_inclusive('\n')
            .map(|line| {
                let key = line.trim_end_matches(['\r', '\n']).to_string();
                let owned = key == format!("/skills/{skill}") || key == format!("/skills/{skill}/");
                let header = Header {
                    start,
                    raw_start: start,
                    key,
                    owned,
                };
                start += line.len();
                header
            })
            .collect());
    }
    let manifest = kind == EntryKind::Manifest;
    let document =
        toml_edit::Document::parse(text).map_err(|error| invalid(path, error.to_string()))?;
    let mut headers = Vec::new();
    collect_headers(
        document.as_table(),
        text,
        &mut Vec::new(),
        false,
        manifest,
        skill,
        &mut headers,
    );
    headers.sort_by_key(|header| header.start);
    if manifest
        && document
            .get("skills")
            .and_then(toml_edit::Item::as_array_of_tables)
            .is_some_and(|entries| {
                entries
                    .iter()
                    .filter(|entry| {
                        entry.get("name").and_then(toml_edit::Item::as_str) == Some(skill)
                    })
                    .count()
                    > 1
            })
    {
        return Err(conflict(path));
    }
    Ok(headers)
}

fn owned_range(
    headers: &[Header],
    text_len: usize,
    path: &Path,
) -> Result<Option<Range<usize>>, CoreError> {
    let Some(first) = headers.iter().position(|header| header.owned) else {
        return Ok(None);
    };
    let last = headers
        .iter()
        .rposition(|header| header.owned)
        .ok_or_else(|| conflict(path))?;
    if headers[first..=last].iter().any(|header| !header.owned) {
        return Err(conflict(path));
    }
    Ok(Some(
        headers[first].start
            ..headers
                .get(last + 1)
                .map_or(text_len, |header| header.start),
    ))
}

impl SavedEntries {
    pub fn backup_paths(&self) -> Vec<PathBuf> {
        self.files.iter().map(|entry| entry.path.clone()).collect()
    }

    pub fn capture(
        rt: &Runtime,
        scope: &crate::identity::RootScope,
        skill: &str,
    ) -> Result<Self, CoreError> {
        let project = match scope {
            crate::identity::RootScope::Global => None,
            crate::identity::RootScope::Project(project) => Some(project.0.as_path()),
        };
        let dir = crate::dotagents_ledger::dotagents_dir(&rt.scope.home.lexical, project);
        let mut files = Vec::new();
        let mut paths = vec![
            (dir.join("agents.toml"), EntryKind::Manifest),
            (dir.join("agents.lock"), EntryKind::Lock),
        ];
        if let Some(project) = project {
            paths.push((project.join(".agents/.gitignore"), EntryKind::Ignore));
        }
        for (source, kind) in paths {
            let path = crate::ports::resolve_config_link(rt.ports.fs.as_ref(), &source)?;
            crate::ports::confine_write_through(&rt.scope, rt.ports.fs.as_ref(), &path)?;
            let text = read_text(rt.ports.fs.as_ref(), &path)?;
            let headers = parse_headers(&text, &path, kind, skill)?;
            let range = owned_range(&headers, text.len(), &path)?;
            if kind == EntryKind::Ignore && range.is_none() {
                continue;
            }
            let range = range.ok_or_else(|| {
                invalid(
                    &path,
                    "Dotagents Remove needs the skill's saved declaration and lock entry.",
                )
            })?;
            let preceding = headers
                .iter()
                .rev()
                .find(|header| header.start < range.start)
                .map(|header| header.key.clone());
            let following = headers
                .iter()
                .find(|header| header.start >= range.end)
                .map(|header| header.key.clone());
            files.push(SavedEntry {
                path,
                source,
                kind,
                skill: skill.into(),
                previous: SlotBytes {
                    text: text[range.clone()].to_string(),
                    header_offset: headers
                        .iter()
                        .find(|header| header.owned)
                        .map(|header| header.raw_start - range.start),
                },
                shared_prefix: None,
                previous_file_absent: false,
                expected: EntryPostState::Unrecorded,
                preceding,
                following,
            });
        }
        Ok(Self { files })
    }
    pub fn capture_after(&mut self, fs: &dyn ScopeFs) -> Result<(), CoreError> {
        for entry in &mut self.files {
            let text = read_text(fs, &entry.path)?;
            let headers = parse_headers(&text, &entry.path, entry.kind, &entry.skill)?;
            let range = owned_range(&headers, text.len(), &entry.path)?;
            let expected = if let Some(range) = range {
                SlotBytes {
                    text: text[range.clone()].to_string(),
                    header_offset: headers
                        .iter()
                        .find(|header| header.owned)
                        .map(|header| header.raw_start - range.start),
                }
            } else {
                let prefix_len = entry.previous.header_offset.unwrap_or(0);
                let prefix = &entry.previous.text[..prefix_len];
                let retained = if prefix.is_empty() {
                    None
                } else {
                    let mut matches = text.match_indices(prefix);
                    matches
                        .next()
                        .filter(|_| matches.next().is_none())
                        .map(|(position, _)| position + prefix.len())
                };
                let gap = if let Some(position) = retained {
                    entry.shared_prefix = Some(prefix.to_string());
                    entry.previous.text.drain(..prefix_len);
                    entry.previous.header_offset = Some(0);
                    text[position..]
                        .chars()
                        .take_while(|ch| ch.is_whitespace())
                        .collect()
                } else {
                    String::new()
                };
                SlotBytes {
                    text: gap,
                    header_offset: None,
                }
            };
            entry.expected = EntryPostState::Recorded(expected);
        }
        Ok(())
    }
}

/// This is the policy boundary for native saved-entry Undo. It makes no process calls.
pub(crate) fn prepare_saved_entry_restore(
    rt: &Runtime,
    target: &crate::events::EventRecord,
) -> Result<PreparedEntries, CoreError> {
    let inverse = target.inverse.as_ref().unwrap_or(&serde_json::Value::Null);
    if target.kind == crate::events::EventKind::Remove.as_str()
        && target
            .payload
            .get("owner_kind")
            .and_then(serde_json::Value::as_str)
            == Some("dotagents")
        && inverse.get("dotagents_saved_entries").is_none()
    {
        return Err(CoreError::new(ErrorCode::Unsupported, "This older Dotagents Remove did not save its declaration and lock entry. Undo cannot restore the exact prior state."));
    }
    let Some(saved) = inverse.get("dotagents_saved_entries") else {
        return Ok(PreparedEntries {
            files: Vec::new(),
            inverse: serde_json::Value::Null,
        });
    };
    let mut saved: SavedEntries = serde_json::from_value(saved.clone())
        .map_err(|error| invalid(Path::new("agents.toml"), error.to_string()))?;
    let mut files = Vec::new();
    for entry in &mut saved.files {
        let resolved = crate::ports::resolve_config_link(rt.ports.fs.as_ref(), &entry.source)
            .map_err(|_| conflict(&entry.source))?;
        if resolved != entry.path {
            return Err(conflict(&entry.source));
        }
        crate::ports::confine_write_through(&rt.scope, rt.ports.fs.as_ref(), &entry.path)?;
        let current_absent = match rt.ports.fs.symlink_metadata(&entry.path) {
            Ok(facts) if facts.kind == crate::ports::FileKind::File => false,
            Ok(_) => return Err(conflict(&entry.path)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
            Err(error) => return Err(CoreError::io(&entry.path, error)),
        };
        let text = read_text(rt.ports.fs.as_ref(), &entry.path)?;
        let headers = parse_headers(&text, &entry.path, entry.kind, &entry.skill)?;
        let range = owned_range(&headers, text.len(), &entry.path)?;
        let EntryPostState::Recorded(expected) = &entry.expected else {
            return Err(CoreError::new(ErrorCode::Incomplete, "Dotagents Remove did not record its final ledger state. Undo cannot safely restore the saved entries.").at(&entry.path));
        };
        let anchor = |key: &str| -> Result<usize, CoreError> {
            let mut matches = headers.iter().filter(|header| header.key == key);
            let first = matches.next().ok_or_else(|| conflict(&entry.path))?;
            if matches.next().is_some() {
                return Err(conflict(&entry.path));
            }
            Ok(first.start)
        };
        let preceding = entry.preceding.as_deref().map(anchor).transpose()?;
        let following = entry.following.as_deref().map(anchor).transpose()?;
        if preceding
            .zip(following)
            .is_some_and(|(before, after)| before >= after)
        {
            return Err(conflict(&entry.path));
        }
        let start = match (range.as_ref(), expected.header_offset) {
            (Some(_), Some(offset)) => headers
                .iter()
                .find(|header| header.owned)
                .and_then(|header| header.raw_start.checked_sub(offset))
                .ok_or_else(|| conflict(&entry.path))?,
            (None, None) => {
                if let Some(prefix) = &entry.shared_prefix {
                    let mut matches = text.match_indices(prefix);
                    let (position, _) = matches.next().ok_or_else(|| conflict(&entry.path))?;
                    if matches.next().is_some() {
                        return Err(conflict(&entry.path));
                    }
                    position + prefix.len()
                } else {
                    following.unwrap_or_else(|| {
                        if preceding.is_some() {
                            text.len()
                        } else {
                            headers
                                .iter()
                                .find(|header| header.key != "[\"skills\"]")
                                .map_or(text.len(), |header| header.start)
                        }
                    })
                }
            }
            _ => return Err(conflict(&entry.path)),
        };
        let end = start
            .checked_add(expected.text.len())
            .ok_or_else(|| conflict(&entry.path))?;
        if text.get(start..end) != Some(expected.text.as_str())
            || range.as_ref().is_some_and(|owned| owned.end > end)
            || preceding.is_some_and(|position| position >= start)
            || following.is_some_and(|position| position < end)
        {
            return Err(conflict(&entry.path));
        }
        let mut merged = text.clone();
        let replacement = &entry.previous.text;
        if !replacement.is_empty() && start > 0 && !merged[..start].ends_with('\n') {
            return Err(conflict(&entry.path));
        }
        merged.replace_range(start..end, replacement);
        parse_headers(&merged, &entry.path, entry.kind, &entry.skill)?;
        let desired = if merged.is_empty() && entry.previous_file_absent {
            None
        } else {
            Some(merged.into_bytes())
        };
        files.push((entry.path.clone(), desired));
        entry.previous_file_absent = current_absent;
        let previous = std::mem::replace(&mut entry.previous, expected.clone());
        entry.expected = EntryPostState::Recorded(previous);
    }
    Ok(PreparedEntries {
        files,
        inverse: serde_json::to_value(saved)
            .map_err(|error| invalid(Path::new("agents.toml"), error.to_string()))?,
    })
}
