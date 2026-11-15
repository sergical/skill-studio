//! Window echoes shared by library and usage answers.
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The counting window, without a rolling day count beside explicit dates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UsageWindow {
    /// Rolling days, only when no date bounds were supplied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days: Option<u32>,
    /// Inclusive start, including the inferred start for an explicit end.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    /// Exclusive end, when supplied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,
}

impl UsageWindow {
    /// Echoes the same lower bound used by the usage tally.
    pub fn new(days: u32, since: Option<DateTime<Utc>>, until: Option<DateTime<Utc>>) -> Self {
        Self {
            days: (since.is_none() && until.is_none()).then_some(days),
            since: since
                .or_else(|| until.map(|end| end - chrono::Duration::days(i64::from(days))))
                .map(|at| at.to_rfc3339()),
            until: until.map(|at| at.to_rfc3339()),
        }
    }
}

impl std::fmt::Display for UsageWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(days) = self.days {
            write!(f, "last {days} days")
        } else {
            write!(
                f,
                "since={} until={}",
                self.since.as_deref().unwrap_or("unspecified"),
                self.until.as_deref().unwrap_or("onwards")
            )
        }
    }
}
