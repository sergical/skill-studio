//! [`EventSink`] adapters: discard notices.

use skill_studio_core::ports::{CoreNotice, EventSink};

/// Discards every notice. The default when nobody is listening.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopSink;

impl EventSink for NoopSink {
    fn notify(&self, _notice: CoreNotice) {}
}
