//! A compact table of the shared library, joined from existing reports.
use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dto::{CopyStatus, Inventory};
use crate::identity::{RootKind, RootScope};
use crate::ops::{OpStatus, Outcome};
use crate::ops_invocation::{deployment_invocation, InvocationMode};
use crate::ports::{OpContext, Runtime};
use crate::CoreError;

/// Window and row selection for the library table.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct LibraryRequest {
    /// Include skills outside the shared library.
    pub all: bool,
    /// Restrict to owner/repo, plugin@marketplace, or manual.
    pub source: Option<String>,
    /// Restrict to these skill names; empty means all.
    pub skills: Vec<String>,
    /// Keep rows with an agent copy of this status.
    pub copy_status: Option<LibraryCopyStatus>,
    /// Rolling window in days; defaults to 30.
    pub days: Option<u32>,
    /// Inclusive start, instead of days before the end.
    pub since: Option<DateTime<Utc>>,
    /// Exclusive end; defaults to now.
    pub until: Option<DateTime<Utc>>,
    /// Sort by name (default) or uses descending.
    pub sort: Option<LibrarySort>,
    /// Keep the N most used skills.
    pub top: Option<usize>,
}

/// Supported library row order.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LibrarySort {
    /// Alphabetical skill names.
    Name,
    /// Most uses first, then name.
    Uses,
}

/// Windowed counts supplied by the history adapter, with no session counts.
#[derive(Debug, Clone)]
pub struct LibraryUses {
    /// Installed skill name.
    pub name: String,
    /// Counted uses in the window.
    pub uses: u32,
    /// Counted uses by agent.
    pub agents: BTreeMap<String, u32>,
}

/// Shared library table, before or after row selection.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LibraryReport {
    /// Number of real library skills, before top selection.
    pub count: usize,
    /// Number used in the window, before top selection.
    pub used: usize,
    /// Window used by the tally.
    #[serde(flatten)]
    pub window: crate::usage_window::UsageWindow,
    /// Selected skills.
    pub rows: Vec<LibraryRow>,
    /// Some inventory or history could not be read.
    pub partial: bool,
}

/// One real folder in the home's shared skill library.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LibraryRow {
    /// A real folder exists in the home's shared library.
    pub library: bool,
    /// Every deployment is parked.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub parked: bool,
    /// Installed name.
    pub name: String,
    /// Repository from the install ledgers, or null for a non-repository install.
    pub source: Option<String>,
    /// Uses in the requested window.
    pub uses: u32,
    /// Agents and their use counts in the window.
    pub agents: BTreeMap<String, u32>,
    /// Verified modes; agent copies override library defaults, and manual wins conflicts.
    pub modes: BTreeMap<String, InvocationMode>,
    /// Most severe agent copy: changed, old, same, then link.
    pub copies: BTreeMap<String, LibraryCopyStatus>,
}

/// Copy severity, from a link to a changed independent folder.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum LibraryCopyStatus {
    /// Resolves to the library.
    Link,
    /// Matches the library's files.
    Same,
    /// Missed the library's last update.
    #[serde(alias = "stale")]
    Old,
    /// Has independent changes.
    #[serde(alias = "differs")]
    Changed,
}

impl From<CopyStatus> for LibraryCopyStatus {
    fn from(status: CopyStatus) -> Self {
        match status {
            CopyStatus::Link => Self::Link,
            CopyStatus::Same => Self::Same,
            CopyStatus::Stale => Self::Old,
            CopyStatus::Differs => Self::Changed,
        }
    }
}

impl Outcome for LibraryReport {
    fn status(&self) -> OpStatus {
        if self.partial {
            OpStatus::Partial
        } else {
            OpStatus::Ok
        }
    }
}

/// Joins inventory, invocation, lock provenance and adapter-counted uses.
/// Selection is applied after computing the library totals.
pub fn library(
    rt: &Runtime,
    ctx: &OpContext,
    inventory: &Inventory,
    usage: &[LibraryUses],
    request: &LibraryRequest,
    partial: bool,
) -> Result<LibraryReport, CoreError> {
    let root = rt.scope.home.lexical.join(".agents/skills");
    let mut partial = partial;
    let lock = crate::lock_file::read_lock_file(
        rt.ports.fs.as_ref(),
        &crate::lock_file::lock_file_path(&rt.scope.home.lexical),
    )
    .map_err(|_| {
        partial = true;
    })
    .ok();
    ctx.checkpoint()?;
    let mut rows = Vec::new();
    for skill in &inventory.skills {
        if !request.skills.is_empty() && !request.skills.contains(&skill.name.0) {
            continue;
        }
        let folder = skill.deployments.iter().find(|d| {
            d.root.kind == RootKind::Universal
                && d.path.parent() == Some(root.as_path())
                && !d.is_symlink
        });
        if folder.is_none()
            && (!request.all
                || skill
                    .deployments
                    .iter()
                    .all(|d| d.path.parent() == Some(root.as_path())))
        {
            continue;
        }
        let counts = usage.iter().find(|u| u.name == skill.name.0);
        let mut row = LibraryRow {
            library: folder.is_some(),
            parked: skill
                .deployments
                .iter()
                .all(|d| d.root.kind == RootKind::Parked),
            name: skill.name.0.clone(),
            source: lock
                .as_ref()
                .and_then(|lock| lock.skills.get(&skill.name.0))
                .filter(|entry| matches!(entry.source_type.as_str(), "github" | "gitlab" | "git"))
                .map(|entry| entry.source.clone())
                .or_else(|| {
                    let local_lock = lock
                        .as_ref()
                        .and_then(|lock| lock.skills.get(&skill.name.0))
                        .is_some_and(|entry| entry.source == skill.source);
                    if local_lock || skill.source.contains('@') {
                        None
                    } else {
                        crate::dotagents_ledger::github_repo_from_source(&skill.source)
                    }
                }),
            uses: counts.map_or(0, |u| u.uses),
            agents: counts.map(|u| u.agents.clone()).unwrap_or_default(),
            modes: BTreeMap::new(),
            copies: BTreeMap::new(),
        };
        if request
            .source
            .as_ref()
            .is_some_and(|source| match source.as_str() {
                "manual" => skill.source != "manual",
                _ if source.contains('@') => source != &skill.source,
                _ => row.source.as_ref() != Some(source),
            })
        {
            continue;
        }
        let copies: Vec<_> = skill
            .deployments
            .iter()
            .filter(|d| {
                !d.symlink_is_broken
                    && d.root.kind != RootKind::Parked
                    && if folder.is_some() {
                        d.root.scope == RootScope::Global
                            && matches!(d.root.kind, RootKind::Harness(_) | RootKind::Legacy(_))
                    } else {
                        true
                    }
            })
            .collect();
        if request.copy_status.is_some_and(|status| {
            !copies
                .iter()
                .any(|d| d.copy_status.map(LibraryCopyStatus::from) == Some(status))
        }) {
            continue;
        }
        for library_copy in [true, false] {
            let mut modes = BTreeMap::new();
            for deployment in folder
                .into_iter()
                .chain(copies.iter().copied())
                .filter(|d| folder.is_some_and(|folder| d.path == folder.path) == library_copy)
            {
                ctx.checkpoint()?;
                match deployment_invocation(rt, deployment) {
                    Ok(reads) => {
                        for read in reads {
                            if read.mode == InvocationMode::Unsupported {
                                continue;
                            }
                            modes
                                .entry(read.harness.to_string())
                                .and_modify(|mode| {
                                    if read.mode == InvocationMode::Manual {
                                        *mode = InvocationMode::Manual;
                                    }
                                })
                                .or_insert(read.mode);
                        }
                    }
                    Err(_) => partial = true,
                }
            }
            row.modes.extend(modes);
        }
        for deployment in copies {
            if let (Some(agent), Some(status)) = (&deployment.harness, deployment.copy_status) {
                let status = LibraryCopyStatus::from(status);
                row.copies
                    .entry(agent.to_string())
                    .and_modify(|current| *current = (*current).max(status))
                    .or_insert(status);
            }
        }
        rows.push(row);
    }
    let count = rows.len();
    let used = rows.iter().filter(|row| row.uses > 0).count();
    if request.top.is_some() || matches!(request.sort, Some(LibrarySort::Uses)) {
        rows.sort_by(|a, b| b.uses.cmp(&a.uses).then_with(|| a.name.cmp(&b.name)));
    } else {
        rows.sort_by(|a, b| a.name.cmp(&b.name));
    }
    if let Some(top) = request.top {
        rows.retain(|row| row.uses > 0);
        rows.truncate(top);
    }
    Ok(LibraryReport {
        count,
        used,
        window: crate::usage_window::UsageWindow::new(
            request.days.unwrap_or(30),
            request.since,
            request.until,
        ),
        rows,
        partial,
    })
}
