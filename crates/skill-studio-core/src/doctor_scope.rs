//! Scope-wide doctor checks that need the runtime's roots and history.
use crate::doctor::DoctorInvariant;
use crate::dto::{DoctorViolation, Severity};
use crate::harness::{RootRole, ScopeLevel};
use crate::ports::{FileKind, Runtime};
use std::collections::BTreeSet;
use std::path::PathBuf;

pub(crate) fn skill_links(rt: &Runtime) -> Vec<DoctorViolation> {
    let fs = rt.ports.fs.as_ref();
    let roots: BTreeSet<PathBuf> = rt
        .ports
        .catalog
        .facts
        .iter()
        .flat_map(|facts| &facts.roots)
        .filter(|root| !matches!(root.role, RootRole::PluginCache | RootRole::CrossHarness))
        .flat_map(|root| match root.level {
            ScopeLevel::Global => vec![rt
                .scope
                .global_root_path(std::path::Path::new(&root.relative_path))],
            ScopeLevel::Project => rt
                .scope
                .projects
                .iter()
                .map(|project| project.lexical.join(&root.relative_path))
                .collect(),
        })
        .collect();
    let mut warnings = Vec::new();
    for root in roots {
        for entry in fs.read_dir(&root).unwrap_or_default() {
            if entry.kind != FileKind::Symlink || entry.name.starts_with('.') {
                continue;
            }
            let link = root.join(&entry.name);
            let target = match fs.canonicalize(&link) {
                Ok(target) => target,
                Err(error) => {
                    warnings.push(violation(
                        DoctorInvariant::LinkResolvesInRoot,
                        link,
                        Severity::Error,
                        format!("skill link does not resolve: {error}"),
                    ));
                    continue;
                }
            };
            if target.starts_with(&rt.scope.home.canonical)
                || rt
                    .scope
                    .projects
                    .iter()
                    .any(|project| target.starts_with(&project.canonical))
            {
                continue;
            }
            warnings.push(DoctorViolation {
                severity: Severity::Warning,
                invariant: DoctorInvariant::LinkResolvesInRoot,
                path: link,
                detail: format!(
                    "skill link resolves to {} outside the home and every selected project",
                    target.display()
                ),
            });
        }
    }
    warnings
}

/// The lead's default. Doctor reports old retained entries and never removes them.
pub const RETAINED_ENTRY_MAX_AGE_DAYS: i64 = 30;

fn violation(
    invariant: DoctorInvariant,
    path: PathBuf,
    severity: Severity,
    detail: String,
) -> DoctorViolation {
    DoctorViolation {
        severity,
        invariant,
        path,
        detail,
    }
}

pub(crate) fn history_pending(rt: &Runtime) -> Vec<DoctorViolation> {
    let path = rt.scope.history_root.join("events.sqlite3");
    let result = rt
        .ports
        .history
        .open(&rt.scope, crate::ports::HistoryAccess::ReadIfExists)
        .and_then(|store| store.map(|store| store.pending()).transpose());
    match result {
        Ok(Some(rows)) => rows
            .into_iter()
            .map(|row| {
                violation(
                    DoctorInvariant::JournalHasNoOpenPlan,
                    path.clone(),
                    Severity::Error,
                    format!("history event {} ({}) is still pending", row.id.0, row.kind),
                )
            })
            .collect(),
        Ok(None) => Vec::new(),
        Err(error) => vec![violation(
            DoctorInvariant::JournalHasNoOpenPlan,
            path,
            Severity::Error,
            format!("history journal could not be read: {error}"),
        )],
    }
}

fn retained_entries(
    fs: &dyn crate::ports::ScopeFs,
    root: &std::path::Path,
    nested: bool,
    protected: &[PathBuf],
    entries: &mut Vec<PathBuf>,
) {
    for entry in fs.read_dir(root).unwrap_or_default() {
        if !matches!(
            entry.kind,
            FileKind::Dir | FileKind::File | FileKind::Symlink
        ) || entry.name == ".origin"
        {
            continue;
        }
        let path = root.join(entry.name);
        if protected.iter().any(|active| path.starts_with(active)) {
            continue;
        }
        let children = if entry.kind == FileKind::Dir {
            fs.read_dir(&path).unwrap_or_default()
        } else {
            Vec::new()
        };
        if nested
            && entry.kind == FileKind::Dir
            && fs.symlink_metadata(&path.join("SKILL.md")).is_err()
            && children
                .iter()
                .any(|child| child.kind == FileKind::Dir && !child.name.starts_with('.'))
        {
            retained_entries(fs, &path, true, protected, entries);
        } else {
            entries.push(path);
        }
    }
}

fn recorded_paths(
    value: &serde_json::Value,
    time: Option<chrono::DateTime<chrono::Utc>>,
    records: &mut std::collections::BTreeMap<PathBuf, Option<chrono::DateTime<chrono::Utc>>>,
) {
    match value {
        serde_json::Value::Object(fields) => {
            let recorded = [
                "trashed_at",
                "quarantined_at",
                "created_at",
                "parked_at",
                "forked_at",
            ]
            .into_iter()
            .find_map(|key| fields.get(key))
            .map(|value| {
                value
                    .as_str()
                    .and_then(|text| chrono::DateTime::parse_from_rfc3339(text).ok())
                    .map(|date| date.with_timezone(&chrono::Utc))
            });
            let time = recorded.unwrap_or(time);
            for (key, value) in fields {
                if matches!(
                    key.as_str(),
                    "quarantined"
                        | "to"
                        | "trash_path"
                        | "trashed_path"
                        | "backup_path"
                        | "parked_dir"
                        | "base_dir"
                        | "base_snapshot"
                ) {
                    if let Some(path) = value
                        .as_str()
                        .map(PathBuf::from)
                        .filter(|path| path.is_absolute())
                    {
                        record_earliest_creation(records, path, time);
                    }
                }
                recorded_paths(value, time, records);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                recorded_paths(value, time, records);
            }
        }
        _ => {}
    }
}

fn record_earliest_creation(
    records: &mut std::collections::BTreeMap<PathBuf, Option<chrono::DateTime<chrono::Utc>>>,
    path: PathBuf,
    time: Option<chrono::DateTime<chrono::Utc>>,
) {
    // A stored None marks an invalid creation record.
    records
        .entry(path)
        .and_modify(|recorded| *recorded = (*recorded).min(time))
        .or_insert(time);
}

fn creation_time(
    recorded: Option<&Option<chrono::DateTime<chrono::Utc>>>,
    modified: Option<chrono::DateTime<chrono::Utc>>,
) -> Option<chrono::DateTime<chrono::Utc>> {
    recorded.copied().unwrap_or(modified)
}

pub(crate) fn retained_state(
    rt: &Runtime,
    inventory: &crate::dto::Inventory,
) -> Vec<DoctorViolation> {
    use crate::doctor::{QUARANTINE_DIR_NAME, QUARANTINE_RETENTION_CAP};
    use crate::identity::RootKind;
    use crate::ports::{HistoryAccess, Journal, PlanStep};
    let fs = rt.ports.fs.as_ref();
    let home = &rt.scope.home.lexical;
    let registry_path = home.join(".agents/skill-studio.json");
    let registry: serde_json::Value =
        match fs.read_capped(&registry_path, crate::ownership::OWNERSHIP_LEDGER_MAX_BYTES) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(value) => value,
                Err(_) => return Vec::new(),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
            Err(_) => return Vec::new(),
        };
    let mut protected: Vec<PathBuf> = inventory
        .skills
        .iter()
        .flat_map(|skill| &skill.deployments)
        .filter(|deployment| deployment.root.kind == RootKind::Parked)
        .map(|deployment| deployment.path.clone())
        .collect();
    protected.extend(inventory.unread_roots.iter().cloned());
    if let Some(parked) = registry
        .get("parked")
        .and_then(serde_json::Value::as_object)
    {
        for (name, record) in parked {
            if let Some(path) = record
                .get("skill_dir")
                .and_then(serde_json::Value::as_str)
                .filter(|path| !path.is_empty())
            {
                protected.push(path.into());
            }
            protected.push(home.join(".agents/skills-parked").join(name));
        }
    }
    let scope_roots: BTreeSet<_> = std::iter::once(home.clone())
        .chain(
            rt.scope
                .projects
                .iter()
                .map(|project| project.lexical.clone()),
        )
        .collect();
    let mut roots: Vec<_> = scope_roots
        .into_iter()
        .flat_map(|root| {
            [
                (root.join(".agents/skills").join(QUARANTINE_DIR_NAME), false),
                (root.join(".agents/skills-trash"), false),
                (root.join(".agents/skills-parked"), true),
            ]
        })
        .collect();
    if let Some(data) = &rt.scope.data_root {
        for forks in [data.join("forks"), data.join("skill-studio/forks")] {
            if let Some(active) = registry.get("forks").and_then(serde_json::Value::as_object) {
                for name in active.keys() {
                    protected.push(forks.join(name).join("base"));
                }
            }
            roots.push((forks, true));
        }
    }
    let mut records = std::collections::BTreeMap::new();
    recorded_paths(&registry, None, &mut records);
    let journal =
        crate::journal::FsJournal::new(crate::ops_install::journal_root(home), rt.ports.fs.clone());
    let Ok(plans) = journal.all() else {
        return Vec::new();
    };
    for plan in plans {
        for step in plan.steps {
            if let PlanStep::Swap {
                quarantined: Some(path),
                ..
            } = step
            {
                record_earliest_creation(&mut records, path, Some(plan.created_at));
            }
        }
    }
    match rt
        .ports
        .history
        .open(&rt.scope, HistoryAccess::ReadIfExists)
    {
        Ok(Some(store)) => match store.list(&crate::events::EventFilter {
            limit: u32::MAX,
            ..Default::default()
        }) {
            Ok(rows) => {
                for row in rows {
                    recorded_paths(&row.payload, Some(row.ts), &mut records);
                    if let Some(inverse) = row.inverse {
                        recorded_paths(&inverse, Some(row.ts), &mut records);
                    }
                }
            }
            Err(error) => {
                return vec![violation(
                    DoctorInvariant::QuarantineWithinCap,
                    rt.scope.history_root.clone(),
                    Severity::Error,
                    format!("retention history could not be read: {error}"),
                )]
            }
        },
        Ok(None) => {}
        Err(_) => return Vec::new(),
    }
    let now = rt.ports.clock.now();
    let mut warnings = Vec::new();
    let parked_names: BTreeSet<_> = inventory
        .skills
        .iter()
        .filter(|skill| {
            skill
                .deployments
                .iter()
                .any(|deployment| deployment.root.kind == RootKind::Parked)
        })
        .map(|skill| skill.name.0.clone())
        .collect();
    for (root, nested) in roots {
        let mut entries = Vec::new();
        retained_entries(fs, &root, nested, &protected, &mut entries);
        let mut per_skill = std::collections::BTreeMap::<String, usize>::new();
        for path in entries {
            let facts = fs.symlink_metadata(&path).ok();
            let name = facts
                .as_ref()
                .filter(|facts| facts.kind == FileKind::Dir)
                .and_then(|_| fs.read_capped(&path.join("SKILL.md"), 1024 * 1024).ok())
                .and_then(|bytes| {
                    crate::frontmatter::parse_frontmatter(&String::from_utf8_lossy(&bytes))
                        .as_frontmatter()
                        .and_then(|fm| fm.name.clone())
                })
                .unwrap_or_else(|| {
                    path.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                });
            *per_skill.entry(name.clone()).or_default() += 1;
            if root.ends_with("skills-trash") && parked_names.contains(&name) {
                warnings.push(violation(
                    DoctorInvariant::NoFolderInTwoStates,
                    path.clone(),
                    Severity::Warning,
                    format!(
                        "`{name}` is parked and also retained in trash; check the copies' origins"
                    ),
                ));
            }
            let modified = facts.and_then(|facts| facts.modified);
            if let Some(created) = creation_time(records.get(&path), modified) {
                if now.signed_duration_since(created)
                    > chrono::Duration::days(RETAINED_ENTRY_MAX_AGE_DAYS)
                {
                    warnings.push(violation(DoctorInvariant::QuarantineWithinCap, path.clone(), Severity::Warning, format!("retained entry created at {} is older than {RETAINED_ENTRY_MAX_AGE_DAYS} days", created.to_rfc3339())));
                }
            } else if records.get(&path).is_some_and(Option::is_none) {
                warnings.push(violation(
                    DoctorInvariant::QuarantineWithinCap,
                    path,
                    Severity::Warning,
                    "retained entry has an invalid recorded creation time".into(),
                ));
            }
        }
        for (name, count) in per_skill {
            if count > QUARANTINE_RETENTION_CAP {
                warnings.push(violation(DoctorInvariant::QuarantineWithinCap, root.clone(), Severity::Warning, format!("{count} retained entries for `{name}` exceed the per-skill cap of {QUARANTINE_RETENTION_CAP}")));
            }
        }
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retention_age_prefers_recorded_creation_and_uses_mtime_only_without_a_record_or_resets_age_on_touch(
    ) {
        let now = chrono::DateTime::from_timestamp(10_000_000, 0).unwrap();
        let old = now - chrono::Duration::days(RETAINED_ENTRY_MAX_AGE_DAYS + 1);
        let young = now - chrono::Duration::days(1);
        assert_eq!(creation_time(Some(&Some(old)), Some(young)), Some(old));
        assert_eq!(creation_time(Some(&Some(young)), Some(old)), Some(young));
        assert_eq!(creation_time(None, Some(old)), Some(old));
        assert_eq!(creation_time(Some(&None), Some(old)), None);
    }
    #[test]
    fn a_retained_removal_uses_its_history_creation_time_or_treats_a_touched_entry_as_new() {
        let path = PathBuf::from("/home/.agents/skills/.skill-studio-quarantine/removed");
        let created = chrono::DateTime::from_timestamp(100, 0).unwrap();
        let mut records = std::collections::BTreeMap::new();
        recorded_paths(
            &serde_json::json!({"from":"/home/.agents/skills/alpha", "to":path}),
            Some(created),
            &mut records,
        );
        assert_eq!(records.get(&path), Some(&Some(created)));
    }

    #[test]
    fn a_later_history_reference_keeps_the_original_retained_creation_time_or_hides_an_old_entry() {
        let path = PathBuf::from("/home/.agents/skills-trash/removed");
        let created = chrono::DateTime::from_timestamp(100, 0).unwrap();
        let referenced = created + chrono::Duration::days(RETAINED_ENTRY_MAX_AGE_DAYS + 1);
        let event = serde_json::json!({"to": path});
        let mut records = std::collections::BTreeMap::new();
        recorded_paths(&event, Some(referenced), &mut records);
        recorded_paths(&event, Some(created), &mut records);
        assert_eq!(records.get(&path), Some(&Some(created)));
    }
}
