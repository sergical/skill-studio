//! A pack's original source. When `pack_switch_source` moves a skill to
//! another git source, usually the user's GitHub fork, it records the old
//! source as that skill's original in `~/.agents/skill-studio.json`
//! (`pack_originals`, by scope and skill name, one record per source the
//! skill moved to). A record counts only while the skill's lock row names
//! its source, so a failed step, an undo, or a later move picks the record
//! that matches the lock row, with no rollback and no lost record. A
//! remote pack check reads the original too and reports which skills differ
//! from it and which skills only it has.
//!
//! "Upstream" in a pack report stays the pack's own git source; the original
//! is the source the pack's source was forked from. Skill Studio owns this
//! map; no other tool reads or writes it. Taking the original's changes
//! stays with git: sync the fork, then `pack update`.
use std::collections::BTreeMap;
use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::identity::RootScope;
use crate::pack::{source_identity, PackKind, RemotePack, RemoteState};
use crate::ports::{acquire_exclusive, Runtime, ScopeFs};

/// The `skill-studio.json` key that holds every record.
const PACK_ORIGINALS_KEY: &str = "pack_originals";

/// The source a pack moved away from, and how the pack compares with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PackOriginal {
    /// The source the pack's skills came from before they moved to the
    /// pack's own source.
    #[schemars(extend("x-untrusted" = true))]
    pub source: String,
    /// `checked` only when the remote check read both the pack's own source
    /// and the original; otherwise the state of the read that did not happen
    /// or failed, and the two lists are empty.
    pub remote: RemoteState,
    /// Installed skills whose folder in the original differs from the
    /// folder in the pack's own source: one of the two changed it.
    #[schemars(extend("x-untrusted" = true))]
    pub differs: Vec<String>,
    /// Skills of the original that the pack's own source does not have.
    #[schemars(extend("x-untrusted" = true))]
    pub not_in_source: Vec<String>,
}

/// One skill's move, as recorded.
#[derive(Debug, Clone, Deserialize)]
struct Record {
    source: String,
    original: String,
}

/// Every record, by scope key and skill name: one per source the skill
/// moved to.
#[derive(Debug, Default)]
pub(crate) struct Records(BTreeMap<String, BTreeMap<String, Vec<Record>>>);

/// The key a scope's records sit under.
fn scope_key(scope: &RootScope) -> String {
    match scope {
        RootScope::Global => "global".to_string(),
        RootScope::Project(project) => project.0.to_string_lossy().into_owned(),
    }
}

/// A source without its `#ref`, for matching.
fn identity(source: &str) -> String {
    let base = source.split_once('#').map_or(source, |(base, _)| base);
    source_identity(PackKind::SkillsSh, base)
}

/// True when `a` and `b` name the same git source, ignoring a `#ref`.
pub(crate) fn same_source(a: &str, b: &str) -> bool {
    identity(a) == identity(b)
}

/// Reads every record. A missing, unreadable, or malformed file reads as
/// none: a pack report must not fail on Skill Studio's own bookkeeping.
pub(crate) fn read_records(fs: &dyn ScopeFs, home: &Path) -> Records {
    crate::ops_install::read_registry_document(fs, home)
        .ok()
        .and_then(|mut document| document.shift_remove(PACK_ORIGINALS_KEY))
        .and_then(|value| serde_json::from_value(value).ok())
        .map(Records)
        .unwrap_or_default()
}

impl Records {
    /// The original of a skill installed in `scope` from `source`, when a
    /// record for it is still current.
    pub(crate) fn original(&self, scope: &RootScope, skill: &str, source: &str) -> Option<&str> {
        self.0
            .get(&scope_key(scope))?
            .get(skill)?
            .iter()
            .find(|record| same_source(&record.source, source))
            .map(|record| record.original.as_str())
    }
}

/// Compares the pack's own source with its original at their newest
/// commits, by skill name and git tree hash. `installed` are the names of
/// the pack's installed skills.
pub(crate) fn compare(
    source: String,
    own: Result<&RemotePack, RemoteState>,
    original: Result<RemotePack, RemoteState>,
    installed: &[String],
) -> PackOriginal {
    let (mut differs, mut not_in_source) = (Vec::new(), Vec::new());
    let remote = match (own, original) {
        (Ok(own), Ok(original)) => {
            for skill in &original.skills {
                match own.skills.iter().find(|s| s.name == skill.name) {
                    Some(mine) if installed.contains(&skill.name) => {
                        if mine.tree_hash != skill.tree_hash {
                            differs.push(skill.name.clone());
                        }
                    }
                    Some(_) => {}
                    None => not_in_source.push(skill.name.clone()),
                }
            }
            RemoteState::Checked {
                commit: original.commit,
            }
        }
        (Err(state), _) | (_, Err(state)) => state,
    };
    for list in [&mut differs, &mut not_in_source] {
        list.sort();
        list.dedup();
    }
    PackOriginal {
        source,
        remote,
        differs,
        not_in_source,
    }
}

/// One skill that is about to move.
pub(crate) struct Move<'a> {
    pub scope: &'a RootScope,
    pub skill: &'a str,
}

/// Records, before the skills move from `from` to `to`, that each one's
/// original on `to` is `from`, replacing only an older record for `to`. A
/// skill with a record of moving from `to` to `from` is moving back, so it
/// gets no reverse record. Records for other sources stay, so a failed or
/// undone move finds the one its lock row names. Returns true when every
/// skill is moving back.
pub(crate) fn record_moves(
    rt: &Runtime,
    from: &str,
    to: &str,
    moves: &[Move<'_>],
) -> Result<bool, CoreError> {
    let fs = rt.ports.fs.as_ref();
    let home = &rt.scope.home.lexical;
    let guard = acquire_exclusive(rt.ports.leases.as_ref(), &rt.scope)?;
    let mut document = crate::ops_install::read_registry_document(fs, home)?;
    let mut records = match document.shift_remove(PACK_ORIGINALS_KEY) {
        Some(serde_json::Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    let linked_at = rt.ports.clock.now().to_rfc3339();
    let mut all_back = true;
    for step in moves {
        let scope = records
            .entry(scope_key(step.scope))
            .or_insert_with(|| serde_json::json!({}));
        if !scope.is_object() {
            *scope = serde_json::json!({});
        }
        let Some(scope) = scope.as_object_mut() else {
            continue;
        };
        let mut rows: Vec<serde_json::Value> = scope
            .get(step.skill)
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        let source_of = |row: &serde_json::Value, key: &str| {
            row.get(key)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let moving_back = rows.iter().any(|row| {
            same_source(&source_of(row, "source"), from)
                && same_source(&source_of(row, "original"), to)
        });
        if moving_back {
            continue;
        }
        all_back = false;
        rows.retain(|row| !same_source(&source_of(row, "source"), to));
        rows.push(serde_json::json!({ "source": to, "original": from, "linked_at": linked_at }));
        scope.insert(step.skill.to_string(), serde_json::Value::Array(rows));
    }
    document.insert(
        PACK_ORIGINALS_KEY.to_string(),
        serde_json::Value::Object(records),
    );
    crate::ops_install::write_registry_document(&guard, fs, home, document)?;
    Ok(all_back)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::RemoteSkill;

    fn remote(skills: &[(&str, &str)]) -> RemotePack {
        RemotePack {
            commit: "c".to_string(),
            skills: skills
                .iter()
                .map(|(name, tree)| RemoteSkill {
                    dir: format!("skills/{name}"),
                    name: (*name).to_string(),
                    tree_hash: (*tree).to_string(),
                    folder_hash: String::new(),
                })
                .collect(),
        }
    }

    /// Fails if a skill only the fork has, or a skill the user did not
    /// install, shows up as differing, or if a skill only the original has
    /// is missed.
    #[test]
    fn compare_names_installed_skills_that_differ_and_skills_only_the_original_has() {
        let own = remote(&[("a", "1"), ("b", "2"), ("mine", "3"), ("skipped", "4")]);
        let original = remote(&[("a", "1"), ("b", "9"), ("new", "5"), ("skipped", "8")]);
        let installed = ["a".to_string(), "b".to_string(), "mine".to_string()];

        let found = compare("o/r".into(), Ok(&own), Ok(original), &installed);

        assert_eq!(found.differs, ["b"]);
        assert_eq!(found.not_in_source, ["new"]);
        assert_eq!(found.remote, RemoteState::Checked { commit: "c".into() });
    }

    /// A failed read of the fork showed "0 differ, 0 only there", which
    /// reads as a measured result. Fails if a comparison that did not read
    /// both sources reports `checked`.
    #[test]
    fn compare_reports_the_failed_read_when_either_source_is_missing() {
        let failed = RemoteState::Failed { reason: "x".into() };
        let found = compare(
            "o/r".into(),
            Err(failed.clone()),
            Ok(remote(&[("new", "5")])),
            &[],
        );
        assert_eq!(found.remote, failed);
        assert!(found.differs.is_empty() && found.not_in_source.is_empty());

        let own = remote(&[("a", "1")]);
        let found = compare("o/r".into(), Ok(&own), Err(failed.clone()), &[]);
        assert_eq!(found.remote, failed);
    }

    /// Fails if a `#ref` makes the same repo look like another source.
    #[test]
    fn same_source_ignores_a_ref_and_github_spelling() {
        assert!(same_source(
            "me/fork#main",
            "https://github.com/Me/fork.git"
        ));
        assert!(!same_source("me/fork", "owner/pack"));
    }
}
