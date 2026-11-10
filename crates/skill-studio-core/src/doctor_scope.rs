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
        .open(&rt.scope, crate::ports::HistoryAccess::Inspect)
        .and_then(|store| {
            store
                .map(|store| {
                    let mut after = 0;
                    let mut total = 0_u64;
                    let mut examples = Vec::new();
                    loop {
                        let rows = store.doctor_pending(after)?;
                        if rows.is_empty() {
                            break;
                        }
                        for (rowid, id, kind) in rows {
                            after = rowid;
                            total += 1;
                            if examples.len() < 20 {
                                examples.push(format!("{id} ({kind})"));
                            }
                        }
                    }
                    Ok::<_, crate::error::CoreError>((total, examples))
                })
                .transpose()
        });
    match result {
        Ok(Some((0, _)) | None) => Vec::new(),
        Ok(Some((total, examples))) => vec![violation(
            DoctorInvariant::JournalHasNoOpenPlan,
            path.clone(),
            Severity::Error,
            format!(
                "{total} history events still pending; examples: {}",
                examples.join(", ")
            ),
        )],
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
    depth: usize,
    protected: &[PathBuf],
    visit: &mut dyn FnMut(PathBuf),
) {
    let _ = fs.visit_dir(root, &mut |entry| {
        if !matches!(
            entry.kind,
            FileKind::Dir | FileKind::File | FileKind::Symlink
        ) || entry.name == ".origin"
        {
            return;
        }
        let path = root.join(entry.name);
        if protected.iter().any(|active| path.starts_with(active)) {
            return;
        }
        let mut has_children = false;
        if depth > 0 && entry.kind == FileKind::Dir {
            let _ = fs.visit_dir(&path, &mut |child| {
                has_children |= child.kind == FileKind::Dir && !child.name.starts_with('.');
            });
        }
        if depth > 0
            && entry.kind == FileKind::Dir
            && fs.symlink_metadata(&path.join("SKILL.md")).is_err()
            && has_children
        {
            retained_entries(fs, &path, depth - 1, protected, visit);
        } else {
            visit(path);
        }
    });
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

fn read_retention_history(
    mut list: impl FnMut(i64) -> Result<Vec<crate::ports::DoctorHistoryRow>, crate::error::CoreError>,
    candidates: &BTreeSet<PathBuf>,
    records: &mut std::collections::BTreeMap<PathBuf, Option<chrono::DateTime<chrono::Utc>>>,
) -> Result<(), crate::error::CoreError> {
    let mut after = 0;
    loop {
        let rows = list(after)?;
        let Some(last) = rows.last() else {
            break;
        };
        after = last.rowid;
        for row in rows {
            let mut paths = std::collections::BTreeMap::new();
            recorded_paths(&row.payload, Some(row.ts), &mut paths);
            if let Some(inverse) = row.inverse {
                recorded_paths(&inverse, Some(row.ts), &mut paths);
            }
            for (path, time) in paths {
                if candidates.contains(&path) {
                    record_earliest_creation(records, path, time);
                }
            }
        }
    }
    Ok(())
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
        for forks in [
            data.join("forks"),
            data.join("skill-studio/forks"),
            crate::scope::desktop_fork_storage_root(home, data),
        ] {
            if let Some(active) = registry.get("forks").and_then(serde_json::Value::as_object) {
                for name in active.keys() {
                    protected.push(forks.join(name).join("base"));
                }
            }
            roots.push((forks, true));
        }
    }
    let mut warnings = Vec::new();
    let history = std::cell::OnceCell::new();
    let mut history_reported = false;
    let now = rt.ports.clock.now();
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
        let boundary = std::iter::once(home)
            .chain(rt.scope.projects.iter().map(|project| &project.lexical))
            .chain(rt.scope.data_root.iter())
            .find(|boundary| root.starts_with(boundary))
            .map_or_else(
                || {
                    rt.scope
                        .data_root
                        .as_deref()
                        .and_then(std::path::Path::parent)
                        .unwrap_or(&root)
                },
                |path| path.as_path(),
            );
        if let Some(link) = root
            .ancestors()
            .take_while(|path| path.starts_with(boundary))
            .find(|path| {
                fs.symlink_metadata(path)
                    .is_ok_and(|facts| facts.kind == FileKind::Symlink)
            })
        {
            warnings.push(violation(
                DoctorInvariant::QuarantineWithinCap,
                link.to_path_buf(),
                Severity::Warning,
                "retained root has a linked ancestor; skipped".into(),
            ));
            continue;
        }

        let mut per_skill = std::collections::BTreeMap::<String, usize>::new();
        retained_entries(
            fs,
            &root,
            if nested { 32 } else { 0 },
            &protected,
            &mut |path| {
                let paths = BTreeSet::from([path.clone()]);
                let mut records = std::collections::BTreeMap::new();
                recorded_paths(&registry, None, &mut records);
                records.retain(|key, _| paths.contains(key));
                let journal = crate::journal::FsJournal::new(
                    crate::ops_install::journal_root(home),
                    rt.ports.fs.clone(),
                );
                let _ = journal.visit(&mut |plan| {
                    for step in plan.steps {
                        if let PlanStep::Swap {
                            quarantined: Some(retained),
                            ..
                        } = step
                        {
                            if retained == path {
                                record_earliest_creation(
                                    &mut records,
                                    retained,
                                    Some(plan.created_at),
                                );
                            }
                        }
                    }
                });
                let store = history
                    .get_or_init(|| rt.ports.history.open(&rt.scope, HistoryAccess::Inspect));
                if let Err(error) = store {
                    if !history_reported {
                        warnings.push(violation(
                            DoctorInvariant::QuarantineWithinCap,
                            rt.scope.history_root.clone(),
                            Severity::Error,
                            format!("retention history could not be read: {error}"),
                        ));
                        history_reported = true;
                    }
                }
                if let Ok(Some(store)) = store {
                    if let Err(error) = read_retention_history(
                        |after| store.doctor_retention(after),
                        &paths,
                        &mut records,
                    ) {
                        warnings.push(violation(
                            DoctorInvariant::QuarantineWithinCap,
                            rt.scope.history_root.clone(),
                            Severity::Error,
                            format!("retention history could not be read: {error}"),
                        ));
                    }
                }
                let metadata_path = path.join("SKILL.md");
                let metadata = fs.symlink_metadata(&metadata_path).ok();
                if metadata
                    .as_ref()
                    .is_some_and(|facts| facts.kind == FileKind::Symlink)
                {
                    warnings.push(violation(
                        DoctorInvariant::QuarantineWithinCap,
                        metadata_path.clone(),
                        Severity::Warning,
                        "retained SKILL.md is linked; target was not read".into(),
                    ));
                }
                let facts = fs.symlink_metadata(&path).ok();
                let name = facts
                    .as_ref()
                    .filter(|facts| facts.kind == FileKind::Dir)
                    .filter(|_| {
                        metadata
                            .as_ref()
                            .is_some_and(|facts| facts.kind == FileKind::File)
                    })
                    .and_then(|_| fs.read_capped(&metadata_path, 1024 * 1024).ok())
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
            },
        );
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
    use crate::ports::ScopedPath;
    use crate::ports::{DirEntryFacts, ExclusiveGuard, FileFacts};
    use std::path::Path;

    struct StreamingFs {
        inner: std::sync::Arc<dyn crate::ports::ScopeFs>,
        pending: std::sync::atomic::AtomicUsize,
        plan_reads: std::sync::atomic::AtomicUsize,
    }
    impl crate::ports::ScopeFs for StreamingFs {
        fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
            self.inner.canonicalize(path)
        }
        fn symlink_metadata(&self, path: &Path) -> std::io::Result<FileFacts> {
            self.inner.symlink_metadata(path)
        }
        fn read_link(&self, path: &Path) -> std::io::Result<PathBuf> {
            self.inner.read_link(path)
        }
        fn visit_dir(
            &self,
            path: &Path,
            visit: &mut dyn FnMut(DirEntryFacts),
        ) -> std::io::Result<()> {
            self.inner.visit_dir(path, &mut |entry| {
                if path.ends_with("skills-trash") {
                    assert_eq!(
                        self.pending.swap(1, std::sync::atomic::Ordering::SeqCst),
                        0,
                        "retained paths accumulated before inspection"
                    );
                }
                visit(entry);
            })
        }
        fn read_dir(&self, path: &Path) -> std::io::Result<Vec<DirEntryFacts>> {
            {
                assert!(
                    !path.ends_with("skills-trash") && !path.ends_with("plans"),
                    "inspection collected a complete directory"
                );
                self.inner.read_dir(path)
            }
        }
        fn ancestor_holds(&self, start: &Path, name: &str) -> std::io::Result<bool> {
            crate::ports::ancestor_holds(self, start, name)
        }
        fn read_capped(&self, path: &Path, max_bytes: u64) -> std::io::Result<Vec<u8>> {
            {
                if path.ends_with("SKILL.md") {
                    self.pending.store(0, std::sync::atomic::Ordering::SeqCst);
                }
                if path.ends_with("plan.json") {
                    self.plan_reads
                        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
                self.inner.read_capped(path, max_bytes)
            }
        }
        fn read_prefix(&self, path: &Path, limit: u64) -> std::io::Result<(Vec<u8>, bool)> {
            self.inner.read_prefix(path, limit)
        }
        fn write_atomic(
            &self,
            guard: &ExclusiveGuard,
            path: &ScopedPath,
            bytes: &[u8],
        ) -> std::io::Result<()> {
            self.inner.write_atomic(guard, path, bytes)
        }
        fn rename(
            &self,
            guard: &ExclusiveGuard,
            from: &ScopedPath,
            to: &ScopedPath,
        ) -> std::io::Result<()> {
            self.inner.rename(guard, from, to)
        }
        fn remove_file(&self, guard: &ExclusiveGuard, path: &ScopedPath) -> std::io::Result<()> {
            self.inner.remove_file(guard, path)
        }
        fn create_dir_all(&self, guard: &ExclusiveGuard, path: &ScopedPath) -> std::io::Result<()> {
            self.inner.create_dir_all(guard, path)
        }
        fn symlink(
            &self,
            guard: &ExclusiveGuard,
            target: &ScopedPath,
            link: &ScopedPath,
        ) -> std::io::Result<()> {
            self.inner.symlink(guard, target, link)
        }
        fn symlink_relative(
            &self,
            guard: &ExclusiveGuard,
            target: &ScopedPath,
            relative_target: &Path,
            link: &ScopedPath,
        ) -> std::io::Result<()> {
            self.inner
                .symlink_relative(guard, target, relative_target, link)
        }
        fn fsops_device_inode(&self, path: &Path) -> std::io::Result<(u64, u64)> {
            self.inner.fsops_device_inode(path)
        }
        fn fsops_fsync_file(&self, path: &Path) -> std::io::Result<()> {
            self.inner.fsops_fsync_file(path)
        }
        fn fsops_fsync_dir(&self, path: &Path) -> std::io::Result<()> {
            self.inner.fsops_fsync_dir(path)
        }
        fn fsops_create_dir(&self, path: &Path) -> std::io::Result<()> {
            self.inner.fsops_create_dir(path)
        }
        fn fsops_write_new_file(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
            self.inner.fsops_write_new_file(path, bytes)
        }
        fn fsops_write_new_file_with_mode(
            &self,
            path: &Path,
            bytes: &[u8],
            mode: u32,
        ) -> std::io::Result<()> {
            self.inner.fsops_write_new_file_with_mode(path, bytes, mode)
        }
        fn fsops_rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
            self.inner.fsops_rename(from, to)
        }
        fn fsops_symlink(&self, target: &Path, link: &Path) -> std::io::Result<()> {
            self.inner.fsops_symlink(target, link)
        }
        fn fsops_remove_dir(&self, path: &Path) -> std::io::Result<()> {
            self.inner.fsops_remove_dir(path)
        }
        fn fsops_remove_file(&self, path: &Path) -> std::io::Result<()> {
            self.inner.fsops_remove_file(path)
        }
        fn fsops_exchange(&self, a: &Path, b: &Path) -> std::io::Result<()> {
            self.inner.fsops_exchange(a, b)
        }
    }

    #[test]
    fn doctor_retention_inspects_each_entry_before_next_delivery_or_accumulates_paths_and_plans() {
        let mut fixture = crate::testing::FixtureBuilder::new().dir("/home");
        for n in 0..300 {
            fixture = crate::testing::fixtures::skill(
                fixture,
                &format!("/home/.agents/skills-trash/copy-{n}"),
                "alpha",
            );
        }
        let fs = std::sync::Arc::new(StreamingFs {
            inner: std::sync::Arc::new(fixture.build_fs()),
            pending: std::sync::atomic::AtomicUsize::new(0),
            plan_reads: std::sync::atomic::AtomicUsize::new(0),
        });
        let rt = crate::ports::Runtime::new(
            &crate::testing::golden::scope_for("streaming", std::path::Path::new("/home")),
            crate::ports::Ports {
                fs: fs.clone(),
                clock: std::sync::Arc::new(crate::testing::FakeClock::at(0)),
                ids: std::sync::Arc::new(crate::testing::FakeIds::default()),
                leases: std::sync::Arc::new(crate::testing::FakeLease::default()),
                history: std::sync::Arc::new(crate::testing::NoHistory),
                sink: std::sync::Arc::new(crate::testing::RecordingSink::default()),
                spawner: None,
                discovery: None,
                tools: None,
                catalog: std::sync::Arc::new(crate::harness::HarnessCatalog::builtin()),
                telemetry: std::sync::Arc::new(crate::ports::NoopTelemetry),
            },
        )
        .unwrap();
        let inventory = crate::ops::scan(
            &rt,
            &crate::testing::golden::ctx(),
            &crate::dto::ScanRequest::default(),
        )
        .unwrap();
        let report = retained_state(&rt, &inventory);
        assert!(
            report
                .iter()
                .any(|v| v.detail.contains("300 retained entries")),
            "{report:?}"
        );
        assert_eq!(fs.pending.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn doctor_journal_delivers_each_plan_before_reading_next_or_loads_all_steps_and_backups() {
        use crate::ports::Journal;
        let mut fixture = crate::testing::FixtureBuilder::new().dir("/home/plans");
        for n in 0..300 {
            let root = format!("/home/plans/{n}");
            let record = crate::ports::PlanRecord {
                id: crate::identity::PlanId(n.to_string()),
                created_at: chrono::DateTime::from_timestamp(0, 0).unwrap(),
                label: "fixture".into(),
                root: "/home".into(),
                backups: Vec::new(),
                steps: Vec::new(),
                status: crate::ports::PlanStatus::Done,
            };
            fixture = fixture.dir(&root).file(
                &format!("{root}/plan.json"),
                &serde_json::to_vec(&record).unwrap(),
            );
        }
        let fs = std::sync::Arc::new(StreamingFs {
            inner: std::sync::Arc::new(fixture.build_fs()),
            pending: std::sync::atomic::AtomicUsize::new(0),
            plan_reads: std::sync::atomic::AtomicUsize::new(0),
        });
        let journal = crate::journal::FsJournal::new("/home".into(), fs.clone());
        let mut delivered = 0;
        journal
            .visit(&mut |_| {
                delivered += 1;
                assert_eq!(
                    fs.plan_reads.load(std::sync::atomic::Ordering::SeqCst),
                    delivered
                );
            })
            .unwrap();
        assert_eq!(delivered, 300);
    }

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
    #[test]
    fn doctor_retention_pages_large_history_and_keeps_earliest_time_or_loads_unbounded_rows() {
        let path = PathBuf::from("/home/.agents/skills-trash/old");
        let mut records = std::collections::BTreeMap::new();
        let mut pages = 0;
        let mut returned = 0;
        read_retention_history(
            |after| {
                let start = usize::try_from(after).unwrap();
                pages += 1;
                let rows: Vec<_> = (start..(start + 256).min(4097))
                    .map(|n| crate::ports::DoctorHistoryRow {
                        rowid: i64::try_from(n + 1).unwrap(),
                        ts: chrono::DateTime::from_timestamp(10000 - i64::try_from(n).unwrap(), 0)
                            .unwrap(),
                        payload: serde_json::json!({"to": path}),
                        inverse: None,
                    })
                    .collect();
                returned += rows.len();
                Ok(rows)
            },
            &BTreeSet::from([path.clone()]),
            &mut records,
        )
        .unwrap();
        assert_eq!(returned, 4097);
        assert_eq!(pages, 18);
        assert_eq!(
            records.get(&path),
            Some(&chrono::DateTime::from_timestamp(5904, 0))
        );
    }

    #[test]
    fn doctor_retained_tree_stops_at_depth_limit_or_recurses_without_bound() {
        let mut builder = crate::testing::FixtureBuilder::new().dir("/home");
        let mut path = PathBuf::from("/home");
        for _ in 0..40 {
            path.push("nested");
            builder = builder.dir(path.to_str().unwrap());
        }
        let fs = builder.build_fs();
        let mut entries = Vec::new();
        retained_entries(&fs, std::path::Path::new("/home"), 32, &[], &mut |path| {
            entries.push(path);
        });
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].components().count(), 35);
    }
}
