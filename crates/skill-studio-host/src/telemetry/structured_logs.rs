//! Structured logs with fixed message bodies and bounded attributes.

use super::Surface;

/// Reports the first desktop inventory's counts.
pub fn record_desktop_startup(skills: usize, agents: usize, projects: usize) {
    sentry::logger_info!(
        surface = "desktop",
        skills = skills as u64,
        agents = agents as u64,
        projects = projects as u64,
        "Desktop inventory loaded"
    );
}

/// Reports whether startup opened the data folder and its event store.
pub fn record_desktop_data_folder(blocked: bool, event_store_open: bool) {
    let status = if blocked { "blocked" } else { "ready" };
    let event_store = if event_store_open {
        "open"
    } else {
        "unavailable"
    };
    sentry::logger_info!(
        surface = "desktop",
        status = status,
        event_store = event_store,
        "Desktop data folder checked"
    );
}

/// Reports the updater's fixed outcome and trigger.
pub fn record_desktop_update_check(manual: bool, outcome: &'static str) {
    let trigger = if manual { "manual" } else { "background" };
    sentry::logger_info!(
        surface = "desktop",
        trigger = trigger,
        outcome = outcome,
        "Desktop update checked"
    );
}

pub(super) fn record_refresh_finish(step: &'static str, elapsed_ms: u64, ok: bool) {
    let outcome = if ok { "ok" } else { "error" };
    sentry::logger_info!(
        surface = "desktop",
        step = step,
        elapsed_ms = elapsed_ms,
        outcome = outcome,
        "Snapshot refresh finished"
    );
}

pub(super) fn record_failure_log(surface: Surface, command: &str, code: &str) {
    sentry::logger_error!(
        surface = surface.as_str(),
        command = command,
        error_code = code,
        "Command failed"
    );
}
