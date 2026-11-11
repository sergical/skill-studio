//! Review dates in Skill Studio's own lease-guarded data file.
use crate::ops::{Operation, Outcome};
use crate::registry::{write_registry_document, RegistryDocument};
use crate::{CoreError, ErrorCode, OpContext, Runtime};
use chrono::NaiveDate;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A named skill and its optional review date. No date clears the saved date.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReviewRequest {
    /// Installed skill name.
    pub skill: String,
    /// Calendar date in YYYY-MM-DD form.
    pub date: Option<String>,
}
/// Saved review date after a set or clear.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReviewOutcome {
    /// Installed skill name.
    pub skill: String,
    /// None after clear.
    pub review_date: Option<String>,
}
impl Outcome for ReviewOutcome {}

#[derive(Default, Serialize, Deserialize)]
struct ReviewStore {
    #[serde(default)]
    write_version: u64,
    #[serde(default)]
    dates: BTreeMap<String, NaiveDate>,
}
impl RegistryDocument for ReviewStore {
    fn write_version(&self) -> u64 {
        self.write_version
    }
    fn set_write_version(&mut self, version: u64) {
        self.write_version = version;
    }
}
fn load(rt: &Runtime) -> Result<ReviewStore, CoreError> {
    let path = rt.scope.home.canonical.join(".agents/review-dates.json");
    match rt.ports.fs.read_capped(&path, 8 * 1024 * 1024) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| {
            CoreError::new(ErrorCode::InvalidRequest, "review dates file is malformed").at(&path)
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ReviewStore::default()),
        Err(e) => Err(CoreError::io(&path, e)),
    }
}
/// Set or clear one existing skill's review date.
pub fn review(
    rt: &Runtime,
    ctx: &OpContext,
    req: &ReviewRequest,
) -> Result<ReviewOutcome, CoreError> {
    rt.run(Operation::Review, ctx, || {
        let mut store = load(rt)?;
        let clears_saved = req.date.is_none() && store.dates.contains_key(&req.skill);
        let inventory = crate::ops::scan_for_usage(rt, ctx)?;
        if !clears_saved && !inventory.skills.iter().any(|s| s.name.0 == req.skill) {
            return Err(CoreError::new(
                ErrorCode::InvalidRequest,
                format!("skill '{}' does not exist", req.skill),
            ));
        }
        let date = req
            .date
            .as_ref()
            .map(|date| {
                NaiveDate::parse_from_str(date, "%Y-%m-%d")
                    .ok()
                    .filter(|parsed| parsed.format("%Y-%m-%d").to_string() == *date)
                    .ok_or_else(|| {
                        CoreError::new(
                            ErrorCode::InvalidRequest,
                            "use a valid date in YYYY-MM-DD form",
                        )
                    })
            })
            .transpose()?;
        if let Some(date) = date {
            store.dates.insert(req.skill.clone(), date);
        } else {
            store.dates.remove(&req.skill);
        }
        let home = &rt.scope.home.canonical;
        write_registry_document(
            rt.ports.leases.as_ref(),
            rt.ports.fs.as_ref(),
            home,
            &home.join(".agents/review-dates.json"),
            &mut store,
        )?;
        Ok(ReviewOutcome {
            skill: req.skill.clone(),
            review_date: req.date.clone(),
        })
    })
}
/// Saved dates strictly before today's UTC date from the runtime clock.
pub fn review_due(rt: &Runtime, ctx: &OpContext) -> Result<BTreeMap<String, NaiveDate>, CoreError> {
    rt.run(Operation::ReviewDue, ctx, || {
        let today = rt.ports.clock.now().date_naive();
        Ok(load(rt)?
            .dates
            .into_iter()
            .filter(|(_, date)| *date < today)
            .collect())
    })
}

/// One skill past its review date.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReviewRow {
    /// Skill name.
    pub skill: String,
    /// Saved date in YYYY-MM-DD form.
    pub review_date: String,
    /// Distinct sessions across all available history.
    pub sessions: usize,
}
/// History that could not be counted.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReviewObservation {
    /// Skipped file or root.
    pub path: std::path::PathBuf,
    /// Why this history is unavailable.
    pub reason: String,
}
/// Due skills with usage completeness facts.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReviewDueReport {
    /// Skills sorted by name.
    pub rows: Vec<ReviewRow>,
    /// Counts may miss sessions.
    pub partial: bool,
    /// All usage observations.
    pub observations: Vec<ReviewObservation>,
}
impl Outcome for ReviewDueReport {
    fn status(&self) -> crate::ops::OpStatus {
        if self.partial {
            crate::ops::OpStatus::Partial
        } else {
            crate::ops::OpStatus::Ok
        }
    }
}
