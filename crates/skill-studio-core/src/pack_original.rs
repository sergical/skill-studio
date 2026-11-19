//! A pack's original source. When `pack_switch_source` moves a pack's
//! skills to another git source, usually the user's GitHub fork, the old
//! source stays recorded as that pack's original in
//! `~/.agents/skill-studio.json` (`pack_originals`, keyed by the new source).
//! A remote pack check then reads the original too and reports which skills
//! differ from it and which skills only it has.
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
use crate::pack::{source_identity, PackKind, RemotePack, RemoteState};
use crate::ports::{acquire_exclusive, Runtime, ScopeFs};

/// The `skill-studio.json` key that holds every link.
const PACK_ORIGINALS_KEY: &str = "pack_originals";

/// The source a pack moved away from, and how the pack compares with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PackOriginal {
    /// The source the pack's skills came from before they moved to the
    /// pack's own source.
    #[schemars(extend("x-untrusted" = true))]
    pub source: String,
    /// Remote check result for the original.
    pub remote: RemoteState,
    /// Installed skills whose folder in the original differs from the
    /// folder in the pack's own source: one of the two changed it. Empty
    /// unless the remote check read both sources.
    #[schemars(extend("x-untrusted" = true))]
    pub differs: Vec<String>,
    /// Skills of the original that the pack's own source does not have.
    /// Empty unless the remote check read both sources.
    #[schemars(extend("x-untrusted" = true))]
    pub not_in_source: Vec<String>,
}

/// Every recorded link, pack source to original source, as written. A
/// missing, unreadable, or malformed file reads as no links: a pack report
/// must not fail on Skill Studio's own bookkeeping.
pub(crate) fn read_links(fs: &dyn ScopeFs, home: &Path) -> BTreeMap<String, String> {
    let Ok(document) = crate::ops_install::read_registry_document(fs, home) else {
        return BTreeMap::new();
    };
    document
        .get(PACK_ORIGINALS_KEY)
        .and_then(serde_json::Value::as_object)
        .map(|links| {
            links
                .iter()
                .filter_map(|(source, row)| {
                    let original = row.get("original")?.as_str()?;
                    Some((source.clone(), original.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The original recorded for a pack with this kind and source, if any.
pub(crate) fn original_of(
    links: &BTreeMap<String, String>,
    kind: PackKind,
    source: &str,
) -> Option<String> {
    let wanted = source_identity(kind, source);
    links
        .iter()
        .find(|(linked, _)| source_identity(kind, linked) == wanted)
        .map(|(_, original)| original.clone())
}

/// Compares the pack's own source with its original at their newest
/// commits, by skill name and git tree hash. `installed` are the names of
/// the pack's installed skills.
pub(crate) fn compare(
    source: String,
    own: Option<&RemotePack>,
    original: Result<RemotePack, RemoteState>,
    installed: &[String],
) -> PackOriginal {
    let (remote, original) = match original {
        Ok(fetched) => (
            RemoteState::Checked {
                commit: fetched.commit.clone(),
            },
            Some(fetched),
        ),
        Err(state) => (state, None),
    };
    let (mut differs, mut not_in_source) = (Vec::new(), Vec::new());
    if let (Some(own), Some(original)) = (own, original) {
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
    }
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

/// The links as they were before [`record_switch`], for
/// [`restore_links`] when no skill moved.
pub(crate) struct LinksBefore(Option<serde_json::Value>);

/// What [`record_switch`] wrote.
pub(crate) enum Switched {
    /// `from` is now recorded as the original of `to`.
    Linked,
    /// `to` was the original of `from`, so that link is gone.
    BackToOriginal,
}

/// Records that a pack moved from `from` to `to`. Moving back to the
/// original a pack was linked to drops that link instead of adding a
/// reverse one. Any older link keyed by `to` is replaced.
pub(crate) fn record_switch(
    rt: &Runtime,
    from: &str,
    to: &str,
) -> Result<(LinksBefore, Switched), CoreError> {
    let identity = |source: &str| source_identity(PackKind::SkillsSh, source);
    let linked_at = rt.ports.clock.now().to_rfc3339();
    edit_links(rt, |links| {
        let before = LinksBefore(links.clone().map(serde_json::Value::Object));
        let links = links.get_or_insert_with(serde_json::Map::new);
        let back_to_original = links.iter().any(|(source, row)| {
            identity(source) == identity(from)
                && row
                    .get("original")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|original| identity(original) == identity(to))
        });
        links.retain(|source, _| {
            let key = identity(source);
            key != identity(to) && !(back_to_original && key == identity(from))
        });
        if back_to_original {
            return (before, Switched::BackToOriginal);
        }
        links.insert(
            to.to_string(),
            serde_json::json!({ "original": from, "linked_at": linked_at }),
        );
        (before, Switched::Linked)
    })
}

/// Puts the links back as [`record_switch`] found them.
pub(crate) fn restore_links(rt: &Runtime, before: LinksBefore) -> Result<(), CoreError> {
    edit_links(rt, |links| {
        *links = before.0.and_then(|value| match value {
            serde_json::Value::Object(map) => Some(map),
            _ => None,
        });
    })
}

/// Reads `skill-studio.json` under the exclusive lease, lets `edit` change
/// its `pack_originals` map (`None` when the key is absent), and writes the
/// document back with every other key unchanged.
fn edit_links<T>(
    rt: &Runtime,
    edit: impl FnOnce(&mut Option<serde_json::Map<String, serde_json::Value>>) -> T,
) -> Result<T, CoreError> {
    let fs = rt.ports.fs.as_ref();
    let home = &rt.scope.home.lexical;
    let guard = acquire_exclusive(rt.ports.leases.as_ref(), &rt.scope)?;
    let mut document = crate::ops_install::read_registry_document(fs, home)?;
    let mut links = match document.shift_remove(PACK_ORIGINALS_KEY) {
        Some(serde_json::Value::Object(map)) => Some(map),
        _ => None,
    };
    let result = edit(&mut links);
    if let Some(links) = links.filter(|links| !links.is_empty()) {
        document.insert(
            PACK_ORIGINALS_KEY.to_string(),
            serde_json::Value::Object(links),
        );
    }
    crate::ops_install::write_registry_document(&guard, fs, home, document)?;
    Ok(result)
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

        let found = compare("o/r".into(), Some(&own), Ok(original), &installed);

        assert_eq!(found.differs, ["b"]);
        assert_eq!(found.not_in_source, ["new"]);
        assert_eq!(found.remote, RemoteState::Checked { commit: "c".into() });
    }

    /// Fails if a failed fork check still reports a comparison.
    #[test]
    fn compare_reports_nothing_without_both_sources() {
        let original = remote(&[("new", "5")]);
        let found = compare("o/r".into(), None, Ok(original), &[]);
        assert!(found.differs.is_empty() && found.not_in_source.is_empty());

        let failed = RemoteState::Failed { reason: "x".into() };
        let own = remote(&[("a", "1")]);
        let found = compare("o/r".into(), Some(&own), Err(failed.clone()), &[]);
        assert_eq!(found.remote, failed);
    }
}
