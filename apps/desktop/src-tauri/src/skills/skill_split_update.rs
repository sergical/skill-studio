// ============================================================================
// Skill Studio - Update for split copies
// Fetches a split skill's new version once into a scratch folder, then hands
// the files to `ops::update_split_copies`, which writes every live copy.
// ============================================================================

use std::path::Path;

use skill_studio_core::dto::InstallFile;
use skill_studio_core::identity::{RootScope, SkillName};
use skill_studio_core::lock_file;
use skill_studio_core::ops;
use skill_studio_core::ports::{OpContext, Runtime};
use skill_studio_core::{SplitCopiesOutcome, SplitCopiesUpdate};

use super::skill_deployment::SkillDestination;
use super::skill_dto::{ParsedSkillSource, ParsedSkillSourceKind};
use super::skill_fork::UpstreamFetch;
use super::skill_install::gather_copy_files;
use super::skill_update_check::{CommitLookup, TreeLookup};

/// The source of `name` as `npx skills` recorded it. A split leaves that
/// lock row alone, so it still names where the copies came from.
fn source_from_lock(home: &Path, name: &str) -> Result<ParsedSkillSource, String> {
    let fs = skill_studio_host::RealFs::new();
    let lock = lock_file::read_lock_file(&fs, &lock_file::lock_file_path(home))
        .map_err(|e| e.to_string())?;
    let entry = lock
        .skills
        .get(name)
        .ok_or_else(|| format!("`{name}` has no source on record, so it cannot be updated"))?;
    if entry.source_type != "github" {
        return Err(format!(
            "`{name}` is not hosted on GitHub; only GitHub sources can update split copies"
        ));
    }
    let repo = skill_studio_core::dotagents_ledger::github_repo_from_source(&entry.source)
        .ok_or_else(|| format!("Could not determine {name}'s GitHub repo from its source"))?;
    let skill_path = entry.skill_path.clone().unwrap_or_default();
    let path = skill_path
        .strip_suffix("/SKILL.md")
        .unwrap_or(&skill_path)
        .to_string();
    Ok(ParsedSkillSource {
        kind: ParsedSkillSourceKind::Github,
        repo: Some(repo),
        path: Some(path).filter(|p| !p.is_empty()),
        git_ref: None,
        skill_name: Some(name.to_string()),
        url: None,
        local_path: None,
    })
}

/// Only a global per-harness copy takes part in a split update; any other
/// copy has no split to update, so the user gets a plain reason.
pub(crate) fn check_split_deployment(
    scope: &str,
    destination: SkillDestination,
) -> Result<(), String> {
    if scope == "global" && destination == SkillDestination::PerHarness {
        return Ok(());
    }
    Err("Update works for split copies in your home folder only. \
         This copy is not one, so it was left as it is."
        .to_string())
}

/// Fetches `name`'s new version once, then writes it to every live split
/// copy. Nothing is written to an agent folder unless the fetch succeeded.
pub(crate) fn update_split_skill(
    rt: &Runtime,
    ctx: &OpContext,
    home: &Path,
    name: &str,
    fetch: &dyn UpstreamFetch,
    lookup: &dyn CommitLookup,
    tree: &dyn TreeLookup,
) -> Result<SplitCopiesOutcome, String> {
    let source = source_from_lock(home, name)?;
    // Read before the files, so a commit that lands in between leaves the
    // update offered again rather than hidden.
    let lock_folder_hash = {
        let repo = source.repo.as_deref().unwrap_or_default();
        let path = source.path.as_deref().unwrap_or_default();
        // Best effort: without it the copies still update and the lock row
        // stays as it is, so the next update check offers Update again.
        tree.tree_shas_at_head_uncached(repo)
            .ok()
            .and_then(|shas| shas.get(path).cloned())
    };
    let files: Vec<InstallFile> = gather_copy_files(&source, home, home, fetch, lookup, None)?;
    ops::update_split_copies(
        rt,
        ctx,
        &SplitCopiesUpdate {
            skill: SkillName(name.to_string()),
            scope: RootScope::Global,
            files,
            lock_folder_hash,
        },
    )
    .map_err(|e| e.message)
}

/// `Ok` when every live copy updated, otherwise the plain sentence the
/// toast shows: how many copies updated and why each other one was left.
pub(crate) fn split_update_result(name: &str, outcome: &SplitCopiesOutcome) -> Result<(), String> {
    if outcome.updated.is_empty() && outcome.refused.is_empty() {
        return Err(format!(
            "No copies of {name} are live to update. Each one is parked or turned off."
        ));
    }
    if outcome.refused.is_empty() {
        return Ok(());
    }
    let left: Vec<String> = outcome
        .refused
        .iter()
        .map(|(path, reason)| format!("{}: {reason}", path.display()))
        .collect();
    Err(format!(
        "Updated {} of {} copies of {name}. Left alone - {}",
        outcome.updated.len(),
        outcome.updated.len() + outcome.refused.len(),
        left.join("; ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use skill_studio_core::dto::{ScanRequest, SplitRequest};
    use skill_studio_core::identity::{AgentId, RootKind};

    const SKILL_V1: &str = "---\nname: gamma\ndescription: split me\n---\nBody at v1.\n";

    /// Writes the new version where the real fetch would, and counts calls.
    struct FixtureFetch(std::sync::atomic::AtomicUsize);
    impl UpstreamFetch for FixtureFetch {
        fn fetch_skill_dir(
            &self,
            repo: &str,
            path: &str,
            _commit: &str,
            into: &Path,
        ) -> Result<(), String> {
            assert_eq!((repo, path), ("acme/skills", "skills/gamma"));
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::fs::write(
                into.join("SKILL.md"),
                SKILL_V1.replace("Body at v1", "Body at v2"),
            )
            .map_err(|e| e.to_string())
        }
    }

    struct FixtureLookup;
    impl CommitLookup for FixtureLookup {
        fn latest_commit(
            &self,
            _repo: &str,
            _path: &str,
            _until: Option<&str>,
        ) -> Result<Option<(String, String)>, String> {
            Ok(Some(("c0ffee".into(), "2026-01-01T00:00:00Z".into())))
        }
    }

    struct FixtureTree;
    impl TreeLookup for FixtureTree {
        fn tree_shas_at_head_uncached(
            &self,
            repo: &str,
        ) -> Result<std::collections::HashMap<String, String>, String> {
            assert_eq!(repo, "acme/skills");
            Ok([("skills/gamma".to_string(), "tree-v2".to_string())].into())
        }
    }

    struct FailingTree;
    impl TreeLookup for FailingTree {
        fn tree_shas_at_head_uncached(
            &self,
            _: &str,
        ) -> Result<std::collections::HashMap<String, String>, String> {
            Err("rate limited".to_string())
        }
    }

    struct FailingFetch;
    impl UpstreamFetch for FailingFetch {
        fn fetch_skill_dir(&self, _: &str, _: &str, _: &str, _: &Path) -> Result<(), String> {
            Err("network down".to_string())
        }
    }

    /// A temp home whose `gamma` skill (lock row: github `acme/skills`) was
    /// split into Claude Code, Codex and pi copies.
    fn split_home() -> (tempfile::TempDir, Runtime) {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let dir = home.join(".agents/skills/gamma");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), SKILL_V1).unwrap();
        std::fs::write(
            home.join(".agents/.skill-lock.json"),
            serde_json::json!({
                "version": 3,
                "skills": {"gamma": {
                    "source": "acme/skills",
                    "sourceType": "github",
                    "sourceUrl": "https://github.com/acme/skills",
                    "skillPath": "skills/gamma/SKILL.md",
                    "skillFolderHash": "abc",
                    "installedAt": "2026-01-01T00:00:00Z",
                    "updatedAt": "2026-01-01T00:00:00Z",
                }}
            })
            .to_string(),
        )
        .unwrap();
        let rt =
            super::super::core_runtime::build_runtime_write_at(home, &home.join("data")).unwrap();
        let id = ops::scan(&rt, &ctx(), &ScanRequest::default())
            .unwrap()
            .skills
            .iter()
            .find(|s| s.name.0 == "gamma")
            .unwrap()
            .deployments
            .iter()
            .find(|d| d.root.kind == RootKind::Universal)
            .unwrap()
            .id
            .clone();
        ops::split(
            &rt,
            &ctx(),
            &SplitRequest {
                deployment_id: id,
                harnesses: ["claude-code", "codex", "pi"]
                    .iter()
                    .map(|h| AgentId::parse(h).unwrap())
                    .collect(),
            },
        )
        .unwrap();
        (tmp, rt)
    }

    fn body(path: &Path) -> String {
        std::fs::read_to_string(path.join("SKILL.md")).unwrap()
    }

    fn ctx() -> OpContext {
        OpContext::uncancellable(skill_studio_core::identity::CorrelationId("t".to_string()))
    }

    /// Flow: Update a split skill with one live copy parked. Expect one
    /// fetch, the two live copies at v2, the parked copy still v1, and no
    /// shared folder. Catches an Update that fetches per copy, writes into
    /// a parked copy's old place, or recreates `~/.agents/skills/gamma`.
    #[test]
    fn update_fetches_once_and_writes_live_copies_but_not_the_parked_one() {
        let (tmp, rt) = split_home();
        let home = tmp.path();
        let parked = home.join(".agents/skills-parked/universal/gamma");
        std::fs::create_dir_all(parked.parent().unwrap()).unwrap();
        std::fs::rename(home.join(".codex/skills/gamma"), &parked).unwrap();
        let fetch = FixtureFetch(Default::default());

        let outcome = update_split_skill(
            &rt,
            &ctx(),
            home,
            "gamma",
            &fetch,
            &FixtureLookup,
            &FixtureTree,
        )
        .unwrap();

        assert_eq!(fetch.0.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(outcome.updated.len(), 2, "{outcome:?}");
        assert!(body(&home.join(".claude/skills/gamma")).contains("Body at v2"));
        assert!(body(&home.join(".pi/agent/skills/gamma")).contains("Body at v2"));
        assert!(body(&parked).contains("Body at v1"));
        assert!(!home.join(".codex/skills/gamma").exists());
        assert!(!home.join(".agents/skills/gamma").exists());
        assert!(split_update_result("gamma", &outcome).is_ok());
    }

    /// Flow: a copy was edited after the split, then Update runs. Expect the
    /// edited copy kept, the rest at v2, and a message that names the copy
    /// and says why. Catches an Update that overwrites local edits or hides
    /// the refusal from the user.
    #[test]
    fn update_reports_an_edited_copy_in_the_message_and_updates_the_rest() {
        let (tmp, rt) = split_home();
        let home = tmp.path();
        let edited = home.join(".pi/agent/skills/gamma");
        std::fs::write(
            edited.join("SKILL.md"),
            "---\nname: gamma\ndescription: mine\n---\nEdit.\n",
        )
        .unwrap();

        let outcome = update_split_skill(
            &rt,
            &ctx(),
            home,
            "gamma",
            &FixtureFetch(Default::default()),
            &FixtureLookup,
            &FixtureTree,
        )
        .unwrap();

        assert!(body(&edited).contains("Edit."));
        assert!(body(&home.join(".claude/skills/gamma")).contains("Body at v2"));
        let message = split_update_result("gamma", &outcome).unwrap_err();
        assert!(message.contains("Updated 2 of 3"), "{message}");
        assert!(message.contains(".pi/agent/skills/gamma"), "{message}");
        assert!(message.contains("changes of its own"), "{message}");
    }

    /// Flow: the fetch fails. Expect an error and every copy still v1.
    /// Catches an Update that touches copies before the new files are in hand.
    #[test]
    fn a_failed_fetch_leaves_every_split_copy_untouched() {
        let (tmp, rt) = split_home();
        let home = tmp.path();

        let err = update_split_skill(
            &rt,
            &ctx(),
            home,
            "gamma",
            &FailingFetch,
            &FixtureLookup,
            &FixtureTree,
        )
        .unwrap_err();

        assert!(err.contains("network down"), "{err}");
        for dir in [".claude/skills", ".codex/skills", ".pi/agent/skills"] {
            assert!(body(&home.join(dir).join("gamma")).contains("Body at v1"));
        }
    }

    /// Flow: a real core split, then the desktop reads the home registry.
    /// Expect it to parse, with three per-harness copies that name the lock
    /// source. Catches a registry spelling the desktop rejects, which makes
    /// every registry read fail after one split.
    #[test]
    fn the_registry_a_real_split_wrote_reads_back_with_its_copies() {
        let (tmp, _rt) = split_home();

        let registry = super::super::skill_fork_registry::read_fork_registry(tmp.path()).unwrap();

        assert_eq!(registry.copies.len(), 3);
        for record in registry.copies.values() {
            assert_eq!(record.destination, SkillDestination::PerHarness);
            assert_eq!(record.split_source.as_deref(), Some("acme/skills"));
        }
    }

    /// Flow: a full update of a split skill. Expect the lock row to carry the
    /// tree SHA the tree lookup gave. Catches an Update that leaves the old
    /// hash, so the next update check offers the same update again.
    #[test]
    fn a_full_update_moves_the_lock_hash_to_the_fetched_tree() {
        let (tmp, rt) = split_home();
        let home = tmp.path();

        update_split_skill(
            &rt,
            &ctx(),
            home,
            "gamma",
            &FixtureFetch(Default::default()),
            &FixtureLookup,
            &FixtureTree,
        )
        .unwrap();

        let lock = std::fs::read_to_string(home.join(".agents/.skill-lock.json")).unwrap();
        assert!(lock.contains("tree-v2"), "{lock}");
    }

    /// Flow: the tree lookup fails but the fetch works. Expect every copy at
    /// v2 and the lock hash unchanged, so the next check offers Update again.
    /// Catches a lookup hiccup that blocks the whole update.
    #[test]
    fn a_failed_tree_lookup_still_updates_the_copies_and_keeps_the_lock_hash() {
        let (tmp, rt) = split_home();
        let home = tmp.path();

        let outcome = update_split_skill(
            &rt,
            &ctx(),
            home,
            "gamma",
            &FixtureFetch(Default::default()),
            &FixtureLookup,
            &FailingTree,
        )
        .unwrap();

        assert_eq!(outcome.updated.len(), 3, "{outcome:?}");
        let lock = std::fs::read_to_string(home.join(".agents/.skill-lock.json")).unwrap();
        assert!(
            lock.contains("\"abc\"") && !lock.contains("tree-v2"),
            "{lock}"
        );
    }

    /// Flow: every copy is parked, then Update. Expect an error that says no
    /// copy is live. Catches a silent success that clears the badge and
    /// toasts "updated" with nothing changed.
    #[test]
    fn updating_when_every_copy_is_parked_says_so_instead_of_succeeding() {
        let (tmp, rt) = split_home();
        let home = tmp.path();
        for (i, dir) in [".claude/skills", ".codex/skills", ".pi/agent/skills"]
            .iter()
            .enumerate()
        {
            let parked = home.join(format!(".agents/skills-parked/universal/gamma-{i}"));
            std::fs::create_dir_all(parked.parent().unwrap()).unwrap();
            std::fs::rename(home.join(dir).join("gamma"), parked).unwrap();
        }

        let outcome = update_split_skill(
            &rt,
            &ctx(),
            home,
            "gamma",
            &FixtureFetch(Default::default()),
            &FixtureLookup,
            &FixtureTree,
        )
        .unwrap();

        let message = split_update_result("gamma", &outcome).unwrap_err();
        assert!(message.contains("No copies of gamma are live"), "{message}");
    }

    /// Flow: Update on a copy that is not a global per-harness copy. Expect a
    /// plain-words error, and none for a global per-harness copy. Catches a
    /// project or Universal copy being updated as if it were a global split.
    #[test]
    fn only_a_global_per_harness_copy_is_routed_to_the_split_update() {
        assert!(check_split_deployment("global", SkillDestination::PerHarness).is_ok());
        for (scope, destination) in [
            ("project", SkillDestination::PerHarness),
            ("global", SkillDestination::Universal),
        ] {
            let message = check_split_deployment(scope, destination).unwrap_err();
            assert!(message.contains("home folder only"), "{message}");
        }
    }
}
