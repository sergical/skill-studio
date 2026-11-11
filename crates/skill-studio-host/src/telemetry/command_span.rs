//! The root span of one desktop command or one background refresh step.
//!
//! [`super::record_command`] sends it as the `command.<name>` transaction, and
//! every core op transaction [`super::SentryTelemetry::record`] sends while it
//! is entered joins its trace as a child. The sampling decision lives on the
//! span, so a dropped root also drops every op under it.

use std::cell::Cell;

use sentry::protocol::{SpanId, TraceId};

/// A refresh step that runs on its own every few seconds. Recording each one
/// would bury the commands people actually ran.
pub const BACKGROUND_SAMPLE_RATE: f64 = 0.1;

/// A command that a click or a key press started.
pub const USER_SAMPLE_RATE: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// What started a root span, which sets its sample rate.
pub enum Trigger {
    /// A command the webview sent.
    User,
    /// A refresh step the app ran on its own.
    Background,
}

impl Trigger {
    /// The share of new traces this trigger keeps.
    pub fn sample_rate(self) -> f64 {
        match self {
            Trigger::User => USER_SAMPLE_RATE,
            Trigger::Background => BACKGROUND_SAMPLE_RATE,
        }
    }
}

/// The caller's span, read from a `sentry-trace` header
/// (`<32 hex trace id>-<16 hex span id>[-<0|1>]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceParent {
    /// The trace the caller is in.
    pub trace_id: TraceId,
    /// The caller's own span, which becomes the root's parent.
    pub span_id: SpanId,
    /// The caller's sampling decision, when the header carries one.
    pub sampled: Option<bool>,
}

impl TraceParent {
    /// `None` for anything but the exact header shape.
    pub fn from_sentry_trace(header: &str) -> Option<Self> {
        let mut parts = header.trim().split('-');
        let trace_id = parts.next()?.parse().ok()?;
        let span_id = parts.next()?.parse().ok()?;
        let sampled = match parts.next() {
            None => None,
            Some("1") => Some(true),
            Some("0") => Some(false),
            Some(_) => return None,
        };
        if parts.next().is_some() {
            return None;
        }
        Some(Self {
            trace_id,
            span_id,
            sampled,
        })
    }
}

/// One root span: its ids, its parent, and whether it is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSpan {
    /// Shared by the root and every op transaction under it.
    pub trace_id: TraceId,
    /// The root's own span id, the parent of each op transaction.
    pub span_id: SpanId,
    /// The webview span that sent the command, if any.
    pub parent_span_id: Option<SpanId>,
    /// `false` drops the root and every op under it.
    pub sampled: bool,
}

thread_local! {
    static CURRENT: Cell<Option<CommandSpan>> = const { Cell::new(None) };
}

impl CommandSpan {
    /// Continues `parent`'s trace and keeps its sampling decision when there
    /// is one, so a trace is never cut in half. Otherwise starts a new trace
    /// and samples it at `trigger`'s rate.
    pub fn start(trigger: Trigger, parent: Option<TraceParent>) -> Self {
        let trace_id = parent.map_or_else(TraceId::default, |parent| parent.trace_id);
        let sampled = parent
            .and_then(|parent| parent.sampled)
            .unwrap_or_else(|| sampled_at(trace_id, trigger.sample_rate()));
        Self {
            trace_id,
            span_id: SpanId::default(),
            parent_span_id: parent.map(|parent| parent.span_id),
            sampled,
        }
    }

    /// The span entered on this thread, if any.
    pub fn current() -> Option<Self> {
        CURRENT.with(Cell::get)
    }

    /// Runs `f` with `self` as this thread's current span, and puts the
    /// previous span back afterwards, also when `f` panics.
    pub fn enter<T>(self, f: impl FnOnce() -> T) -> T {
        struct Restore(Option<CommandSpan>);
        impl Drop for Restore {
            fn drop(&mut self) {
                CURRENT.with(|current| current.set(self.0));
            }
        }
        let _restore = Restore(CURRENT.with(|current| current.replace(Some(self))));
        f()
    }
}

/// The same trace id always gets the same answer, so every process that
/// sees a trace agrees on it, and a test can compute the expected answer.
pub fn sampled_at(trace_id: TraceId, rate: f64) -> bool {
    if rate >= 1.0 {
        return true;
    }
    let hex = trace_id.to_string();
    let Ok(head) = u64::from_str_radix(&hex[..16], 16) else {
        return false;
    };
    (head as f64) < rate * (u64::MAX as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trace_id_with_head(head: u64) -> TraceId {
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&head.to_be_bytes());
        TraceId::from(bytes)
    }

    /// Flow: the webview sends `sentry-trace` on `invoke`. Expectation: the
    /// header's trace id, span id, and sampled flag come back unchanged.
    /// Catches: a parser that swaps the ids or drops the flag, which splits
    /// the webview trace from the command root.
    #[test]
    fn a_sentry_trace_header_parses_into_its_three_parts() {
        let trace_id = "771a43a4192642f0b136d5159a501700";
        let span_id = "b0e6f15b45c36b12";
        let parent = TraceParent::from_sentry_trace(&format!("{trace_id}-{span_id}-1"))
            .expect("a well-formed header parses");
        assert_eq!(parent.trace_id.to_string(), trace_id);
        assert_eq!(parent.span_id.to_string(), span_id);
        assert_eq!(parent.sampled, Some(true));

        let unflagged = TraceParent::from_sentry_trace(&format!("{trace_id}-{span_id}"))
            .expect("the sampled flag is optional");
        assert_eq!(unflagged.sampled, None);
    }

    /// Flow: a header from the webview. Expectation: anything but the exact
    /// shape is ignored. Catches: a short id that `hex` pads, or a fourth
    /// part, becoming a parent the trace view cannot join.
    #[test]
    fn a_malformed_sentry_trace_header_is_ignored() {
        for header in [
            "",
            "771a43a4192642f0b136d5159a501700",
            "771a43a4-b0e6f15b45c36b12-1",
            "771a43a4192642f0b136d5159a501700-b0e6f15b-1",
            "771a43a4192642f0b136d5159a501700-b0e6f15b45c36b12-2",
            "771a43a4192642f0b136d5159a501700-b0e6f15b45c36b12-1-x",
            "zz1a43a4192642f0b136d5159a501700-b0e6f15b45c36b12-1",
        ] {
            assert_eq!(TraceParent::from_sentry_trace(header), None, "{header:?}");
        }
    }

    /// Flow: a command the webview started. Expectation: the root continues
    /// the webview trace, has the webview span as parent, and keeps its
    /// sampling flag. Catches: a root that starts a fresh trace.
    #[test]
    fn a_command_span_continues_the_callers_trace() {
        let parent = TraceParent {
            trace_id: TraceId::default(),
            span_id: SpanId::default(),
            sampled: Some(false),
        };
        let span = CommandSpan::start(Trigger::User, Some(parent));
        assert_eq!(span.trace_id, parent.trace_id);
        assert_eq!(span.parent_span_id, Some(parent.span_id));
        assert_ne!(span.span_id, parent.span_id);
        assert!(!span.sampled, "the caller's decision wins over the rate");
    }

    /// Flow: the background refresh loop and user commands. Expectation:
    /// background roots sample below 1.0 and user roots at 1.0, and each
    /// decision is the one `sampled_at` derives from the trace id.
    /// Catches: background refreshes sent at the user rate, or a sampler
    /// that ignores the trace id.
    #[test]
    fn background_refresh_samples_below_one_and_user_commands_at_one() {
        assert!(Trigger::Background.sample_rate() < 1.0);
        assert!(Trigger::Background.sample_rate() > 0.0);
        assert!(Trigger::User.sample_rate() >= 1.0);

        let rate = Trigger::Background.sample_rate();
        let boundary = (rate * u64::MAX as f64) as u64;
        assert!(sampled_at(trace_id_with_head(0), rate));
        assert!(sampled_at(trace_id_with_head(boundary / 2), rate));
        assert!(!sampled_at(
            trace_id_with_head(boundary.saturating_mul(2)),
            rate
        ));
        assert!(!sampled_at(trace_id_with_head(u64::MAX), rate));
        assert!(sampled_at(
            trace_id_with_head(u64::MAX),
            Trigger::User.sample_rate()
        ));

        for _ in 0..64 {
            let span = CommandSpan::start(Trigger::Background, None);
            assert_eq!(span.sampled, sampled_at(span.trace_id, rate));
            assert_eq!(span.parent_span_id, None);
            assert!(CommandSpan::start(Trigger::User, None).sampled);
        }
    }

    /// Flow: a command body runs an op inside `enter`. Expectation: the op
    /// sees the span, and the thread is clean again after `enter` returns
    /// or panics. Catches: a span that leaks into the next command on a
    /// reused pool thread.
    #[test]
    fn entering_a_span_sets_it_for_the_closure_only() {
        let outer = CommandSpan::start(Trigger::User, None);
        let inner = CommandSpan::start(Trigger::User, None);
        assert_eq!(CommandSpan::current(), None);
        outer.enter(|| {
            assert_eq!(CommandSpan::current(), Some(outer));
            inner.enter(|| assert_eq!(CommandSpan::current(), Some(inner)));
            assert_eq!(CommandSpan::current(), Some(outer));
            let panicked = std::panic::catch_unwind(|| {
                inner.enter(|| std::panic::resume_unwind(Box::new("op failed")));
            });
            assert!(panicked.is_err());
            assert_eq!(CommandSpan::current(), Some(outer));
        });
        assert_eq!(CommandSpan::current(), None);
    }
}
