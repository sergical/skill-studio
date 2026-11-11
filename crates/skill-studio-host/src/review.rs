//! Usage cache adapter for the core review-date list.
use skill_studio_core::review::{ReviewDueReport, ReviewObservation, ReviewRow};
use skill_studio_core::{CoreError, OpContext, Runtime};

/// Refresh the saved usage index and attach session counts to due dates.
pub fn review_due_report(
    rt: &Runtime,
    ctx: &OpContext,
    seed: Option<&std::path::Path>,
) -> Result<ReviewDueReport, CoreError> {
    let dates = skill_studio_core::ops::review_due(rt, ctx)?;
    let inventory = skill_studio_core::ops::scan_for_usage(rt, ctx)?;
    let cache = rt.scope.home.canonical.join(".agents/review-usage.json");
    let mut usage =
        crate::SkillUsage::load_read_only(cache.exists().then_some(cache.as_path()).or(seed));
    let report = usage.report_named(
        &rt.scope.home.canonical,
        &inventory,
        crate::DEFAULT_USAGE_DAYS,
        rt.ports.clock.now(),
        &crate::UsageFilter::default(),
        &dates.keys().cloned().collect(),
    );
    usage.save_cache(&cache).map_err(|_| {
        CoreError::new(
            skill_studio_core::ErrorCode::Io,
            "could not save review usage cache",
        )
    })?;
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
