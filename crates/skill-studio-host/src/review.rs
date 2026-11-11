//! Usage cache adapter for the core review-date list.
use skill_studio_core::review::{ReviewDueReport, ReviewObservation, ReviewRow};
use skill_studio_core::{CoreError, OpContext, Runtime};

/// Refresh the CLI usage cache and attach session counts to due dates of installed skills.
pub fn review_due_report(
    rt: &Runtime,
    ctx: &OpContext,
    seed: Option<&std::path::Path>,
) -> Result<ReviewDueReport, CoreError> {
    let mut dates = skill_studio_core::ops::review_due(rt, ctx)?;
    let inventory = skill_studio_core::ops::scan_for_usage(rt, ctx)?;
    dates.retain(|name, _| inventory.skills.iter().any(|s| s.name.0 == *name));
    let cache = seed.map(crate::cli_usage_cache_path);
    let mut usage =
        crate::SkillUsage::load_read_only(cache.as_deref().filter(|path| path.exists()).or(seed));
    let report = usage.report_named(
        &rt.scope.home.canonical,
        &inventory,
        crate::DEFAULT_USAGE_DAYS,
        rt.ports.clock.now(),
        &crate::UsageFilter::default(),
        &dates.keys().cloned().collect(),
    );
    if let Some(path) = &cache {
        usage.save_cache(path).map_err(|_| {
            CoreError::new(
                skill_studio_core::ErrorCode::Io,
                "could not save usage cache",
            )
        })?;
    }
    Ok(ReviewDueReport {
        rows: dates
            .into_iter()
            .map(|(skill, date)| {
                let sessions = report
                    .rows
                    .iter()
                    .find(|r| r.skill == skill)
                    .map_or(0, |r| r.sessions);
                ReviewRow {
                    skill,
                    review_date: date.to_string(),
                    sessions,
                }
            })
            .collect(),
        partial: report.partial,
        observations: report
            .observations
            .into_iter()
            .map(|o| ReviewObservation {
                path: o.path,
                reason: o.reason,
            })
            .collect(),
    })
}
