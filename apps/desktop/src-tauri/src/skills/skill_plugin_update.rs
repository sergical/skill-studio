// ============================================================================
// Skills Module - skill_plugin_update
// Detects Claude Code plugin updates from two local files: the installs in
// `~/.claude/plugins/installed_plugins.json` and the marketplace copy Claude
// Code keeps fresh under `~/.claude/plugins/marketplaces/<marketplace>/`.
// No network. A missing, unreadable, or malformed file means "no claim",
// never an error.
// ============================================================================

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

/// Owner id a plugin update carries in `InstalledSkill::update_owner_ids`.
pub fn plugin_owner_id(plugin_id: &str) -> String {
    format!("plugin:{plugin_id}")
}

/// One entry of `installed_plugins.json`'s per-id install list.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PluginInstall {
    pub scope: Option<String>,
    pub project_path: Option<String>,
    pub version: Option<String>,
    pub git_commit_sha: Option<String>,
}

/// What a marketplace entry says about the latest release of a plugin.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MarketplaceRelease {
    pub version: Option<String>,
    pub source_sha: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginUpdateVerdict {
    Available,
    Current,
    /// Neither a pinned sha nor a version to compare: say nothing.
    NoClaim,
}

/// One install of a plugin that has an update, for the snapshot overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginInstallUpdate {
    pub scope: String,
    pub project_path: Option<String>,
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.filter(|v| !v.is_empty())
}

/// Pure decision for one install against its marketplace entry.
pub fn plugin_update_verdict(
    install: &PluginInstall,
    release: &MarketplaceRelease,
) -> PluginUpdateVerdict {
    if let (Some(latest), Some(installed)) = (
        non_empty(release.source_sha.as_deref()),
        non_empty(install.git_commit_sha.as_deref()),
    ) {
        // Claude Code records full shas, marketplaces sometimes abbreviate.
        let same = latest.starts_with(installed) || installed.starts_with(latest);
        return if same {
            PluginUpdateVerdict::Current
        } else {
            PluginUpdateVerdict::Available
        };
    }
    if let (Some(latest), Some(installed)) = (
        non_empty(release.version.as_deref()),
        non_empty(install.version.as_deref()).filter(|v| *v != "unknown"),
    ) {
        return if latest == installed {
            PluginUpdateVerdict::Current
        } else {
            PluginUpdateVerdict::Available
        };
    }
    PluginUpdateVerdict::NoClaim
}

/// Parses the `plugins[]` entry named `plugin` out of a `marketplace.json`
/// body. `None` when the body is not JSON or has no such entry.
pub fn marketplace_release(marketplace_json: &str, plugin: &str) -> Option<MarketplaceRelease> {
    let root: Value = serde_json::from_str(marketplace_json).ok()?;
    let entry = root
        .get("plugins")?
        .as_array()?
        .iter()
        .find(|entry| entry.get("name").and_then(Value::as_str) == Some(plugin))?;
    Some(MarketplaceRelease {
        version: entry
            .get("version")
            .and_then(Value::as_str)
            .map(str::to_string),
        source_sha: entry
            .get("source")
            .and_then(|source| source.get("sha"))
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

/// Parses `installed_plugins.json` into installs per plugin id, dropping an
/// id whose install list does not parse.
pub fn parse_installs(installed_json: &str) -> BTreeMap<String, Vec<PluginInstall>> {
    let Ok(root) = serde_json::from_str::<Value>(installed_json) else {
        return BTreeMap::new();
    };
    let Some(plugins) = root.get("plugins").and_then(Value::as_object) else {
        return BTreeMap::new();
    };
    plugins
        .iter()
        .filter_map(|(id, installs)| {
            let installs = serde_json::from_value::<Vec<PluginInstall>>(installs.clone()).ok()?;
            Some((id.clone(), installs))
        })
        .collect()
}

/// Every Claude Code plugin install under `home` with an update available,
/// keyed by `<plugin>@<marketplace>`. Two local file reads per marketplace.
pub fn read_plugin_updates(home: &Path) -> BTreeMap<String, Vec<PluginInstallUpdate>> {
    let plugins_dir = home.join(".claude").join("plugins");
    let Ok(installed) = std::fs::read_to_string(plugins_dir.join("installed_plugins.json")) else {
        return BTreeMap::new();
    };
    let mut marketplaces: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut updates = BTreeMap::new();
    for (id, installs) in parse_installs(&installed) {
        let Some((plugin, marketplace)) = id.split_once('@') else {
            continue;
        };
        // A marketplace name from the file must not walk out of the folder.
        if marketplace.contains(['/', '\\']) || marketplace == ".." {
            continue;
        }
        let body = marketplaces
            .entry(marketplace.to_string())
            .or_insert_with(|| {
                std::fs::read_to_string(
                    plugins_dir
                        .join("marketplaces")
                        .join(marketplace)
                        .join(".claude-plugin")
                        .join("marketplace.json"),
                )
                .ok()
            });
        let Some(release) = body
            .as_deref()
            .and_then(|body| marketplace_release(body, plugin))
        else {
            continue;
        };
        let outdated: Vec<PluginInstallUpdate> = installs
            .iter()
            .filter(|install| {
                plugin_update_verdict(install, &release) == PluginUpdateVerdict::Available
            })
            .map(|install| PluginInstallUpdate {
                scope: install.scope.clone().unwrap_or_else(|| "user".to_string()),
                project_path: install.project_path.clone(),
            })
            .collect();
        if !outdated.is_empty() {
            updates.insert(id, outdated);
        }
    }
    updates
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn install(version: Option<&str>, sha: Option<&str>) -> PluginInstall {
        PluginInstall {
            scope: Some("user".to_string()),
            version: version.map(str::to_string),
            git_commit_sha: sha.map(str::to_string),
            ..Default::default()
        }
    }

    fn release(version: Option<&str>, sha: Option<&str>) -> MarketplaceRelease {
        MarketplaceRelease {
            version: version.map(str::to_string),
            source_sha: sha.map(str::to_string),
        }
    }

    #[test]
    fn pinned_sha_differing_from_the_installed_sha_is_an_update() {
        let verdict = plugin_update_verdict(
            &install(None, Some("215e5f1b")),
            &release(None, Some("73e53541")),
        );
        assert_eq!(verdict, PluginUpdateVerdict::Available);
    }

    #[test]
    fn pinned_sha_equal_or_prefix_of_the_installed_sha_is_current() {
        let full = "73e53541aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        for latest in [full, "73e53541"] {
            let verdict =
                plugin_update_verdict(&install(None, Some(full)), &release(None, Some(latest)));
            assert_eq!(verdict, PluginUpdateVerdict::Current, "latest {latest}");
        }
        let verdict =
            plugin_update_verdict(&install(None, Some("73e53541")), &release(None, Some(full)));
        assert_eq!(verdict, PluginUpdateVerdict::Current);
    }

    #[test]
    fn a_different_marketplace_version_is_an_update_and_the_same_is_current() {
        assert_eq!(
            plugin_update_verdict(&install(Some("1.0.5"), None), &release(Some("1.0.6"), None)),
            PluginUpdateVerdict::Available
        );
        assert_eq!(
            plugin_update_verdict(&install(Some("1.0.6"), None), &release(Some("1.0.6"), None)),
            PluginUpdateVerdict::Current
        );
    }

    #[test]
    fn an_unknown_installed_version_makes_no_claim() {
        assert_eq!(
            plugin_update_verdict(
                &install(Some("unknown"), None),
                &release(Some("1.0.6"), None)
            ),
            PluginUpdateVerdict::NoClaim
        );
    }

    #[test]
    fn a_release_with_neither_sha_nor_version_makes_no_claim() {
        assert_eq!(
            plugin_update_verdict(&install(Some("1.0.0"), Some("abc")), &release(None, None)),
            PluginUpdateVerdict::NoClaim
        );
    }

    #[test]
    fn a_pinned_sha_with_no_installed_sha_falls_back_to_the_version() {
        assert_eq!(
            plugin_update_verdict(
                &install(Some("1.0.0"), None),
                &release(Some("1.1.0"), Some("73e53541"))
            ),
            PluginUpdateVerdict::Available
        );
    }

    #[test]
    fn a_malformed_marketplace_file_yields_no_release() {
        assert_eq!(marketplace_release("{ not json", "sentry"), None);
        assert_eq!(marketplace_release(r#"{"plugins": 3}"#, "sentry"), None);
        assert_eq!(marketplace_release(r#"{"plugins": []}"#, "sentry"), None);
    }

    #[test]
    fn marketplace_release_reads_the_three_seen_entry_shapes() {
        let body = r#"{"plugins":[
            {"name":"sentry","version":null,"source":{"source":"url","url":"u","sha":"73e5"}},
            {"name":"codex","version":"1.0.6","source":"./plugins/codex"},
            {"name":"plannotator","version":null,"source":"./apps/hook"}]}"#;
        assert_eq!(
            marketplace_release(body, "sentry"),
            Some(release(None, Some("73e5")))
        );
        assert_eq!(
            marketplace_release(body, "codex"),
            Some(release(Some("1.0.6"), None))
        );
        assert_eq!(
            marketplace_release(body, "plannotator"),
            Some(release(None, None))
        );
    }

    fn write_fixture(home: &Path, installed: &str, marketplace: Option<&str>) {
        let plugins = home.join(".claude/plugins");
        fs::create_dir_all(&plugins).unwrap();
        fs::write(plugins.join("installed_plugins.json"), installed).unwrap();
        if let Some(body) = marketplace {
            let dir = plugins.join("marketplaces/claude-plugins-official/.claude-plugin");
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("marketplace.json"), body).unwrap();
        }
    }

    const MARKETPLACE: &str = r#"{"plugins":[
        {"name":"sentry","version":null,"source":{"source":"url","sha":"73e53541"}},
        {"name":"codex","version":"1.0.6","source":"./plugins/codex"},
        {"name":"plugin-dev","version":"2.0.0","source":"./plugin-dev"}]}"#;

    #[test]
    fn reader_reports_each_outdated_install_with_its_scope_and_project() {
        let home = tempfile::tempdir().unwrap();
        write_fixture(
            home.path(),
            r#"{"version":2,"plugins":{
                "sentry@claude-plugins-official":[{"scope":"user","version":"1.4.0","gitCommitSha":"215e5f1b"}],
                "codex@claude-plugins-official":[
                    {"scope":"user","version":"1.0.6"},
                    {"scope":"project","projectPath":"/Users/x/a","version":"1.0.5"},
                    {"scope":"local","projectPath":"/Users/x/b","version":"1.0.4"}],
                "plugin-dev@claude-plugins-official":[{"scope":"project","projectPath":"/Users/x/src/skills","version":"unknown","gitCommitSha":"dbc4"}]
            }}"#,
            Some(MARKETPLACE),
        );
        let updates = read_plugin_updates(home.path());
        assert_eq!(
            updates.get("sentry@claude-plugins-official"),
            Some(&vec![PluginInstallUpdate {
                scope: "user".to_string(),
                project_path: None
            }])
        );
        assert_eq!(
            updates.get("codex@claude-plugins-official"),
            Some(&vec![
                PluginInstallUpdate {
                    scope: "project".to_string(),
                    project_path: Some("/Users/x/a".to_string())
                },
                PluginInstallUpdate {
                    scope: "local".to_string(),
                    project_path: Some("/Users/x/b".to_string())
                },
            ])
        );
        assert!(!updates.contains_key("plugin-dev@claude-plugins-official"));
    }

    #[test]
    fn reader_returns_nothing_when_files_are_missing_or_malformed() {
        let home = tempfile::tempdir().unwrap();
        assert!(read_plugin_updates(home.path()).is_empty());

        write_fixture(home.path(), "{ not json", Some(MARKETPLACE));
        assert!(read_plugin_updates(home.path()).is_empty());

        write_fixture(
            home.path(),
            r#"{"plugins":{"codex@claude-plugins-official":[{"scope":"user","version":"1.0.0"}]}}"#,
            Some("{ broken"),
        );
        assert!(read_plugin_updates(home.path()).is_empty());

        write_fixture(
            home.path(),
            r#"{"plugins":{"codex@claude-plugins-official":[{"scope":"user","version":"1.0.0"}]}}"#,
            None,
        );
        fs::remove_dir_all(home.path().join(".claude/plugins/marketplaces")).ok();
        assert!(read_plugin_updates(home.path()).is_empty());
    }
}
