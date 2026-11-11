//! Snapshot revisions shared by the desktop and the MCP subscription.

use std::sync::{Arc, RwLock};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Monotonic snapshot revision.
///
/// Invariant: a subscriber accepts a snapshot only when its revision is
/// greater than the one it holds. Revisions restart at `0` per process.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(transparent)]
pub struct Revision(pub u64);

/// The current snapshot and its revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Versioned<T> {
    /// Revision of `value`.
    pub revision: Revision,
    /// The snapshot.
    pub value: T,
}

/// A cell that holds the latest snapshot and bumps the revision on publish.
///
/// Invariant: `publish` assigns `previous + 1` inside the write lock, so two
/// publishers never produce the same revision.
pub struct SnapshotCell<T> {
    inner: RwLock<Option<Arc<Versioned<T>>>>,
}

impl<T> Default for SnapshotCell<T> {
    fn default() -> Self {
        SnapshotCell {
            inner: RwLock::new(None),
        }
    }
}

impl<T> SnapshotCell<T> {
    /// An empty cell.
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores `value` and returns its revision.
    pub fn publish(&self, value: T) -> Revision {
        let mut slot = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let next = Revision(slot.as_ref().map_or(1, |v| v.revision.0 + 1));
        *slot = Some(Arc::new(Versioned {
            revision: next,
            value,
        }));
        next
    }

    /// The current snapshot, if any was published.
    pub fn current(&self) -> Option<Arc<Versioned<T>>> {
        self.inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}
