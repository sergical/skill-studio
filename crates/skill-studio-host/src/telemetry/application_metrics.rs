//! Application metrics emitted from the native command and scan seams.

use sentry::metrics;
use sentry::protocol::Unit;

use super::{CommandFailure, Surface};

pub(super) fn record_command_metrics(
    surface: Surface,
    command: &str,
    elapsed_ms: u64,
    failure: Option<CommandFailure>,
) {
    metrics::distribution("skill_studio.command.duration", elapsed_ms as f64)
        .unit(Unit::Millisecond)
        .attribute("command", command.to_owned())
        .attribute("surface", surface.as_str())
        .capture();
    if let Some(failure) = failure.filter(|failure| failure.opens_issue()) {
        record_command_failure(surface, command, failure);
    }
    if let Some(count) = open_file_descriptor_count() {
        metrics::gauge("skill_studio.command.open_file_descriptors", count as f64)
            .attribute("command", command.to_owned())
            .attribute("surface", surface.as_str())
            .capture();
    }
}

pub(super) fn record_command_failure(surface: Surface, command: &str, failure: CommandFailure) {
    metrics::counter("skill_studio.command.failures", 1)
        .attribute("command", command.to_owned())
        .attribute("surface", surface.as_str())
        .attribute("error_code", failure.as_str())
        .capture();
}

pub(super) fn record_snapshot_duration(step: &'static str, elapsed_ms: u64) {
    metrics::distribution("skill_studio.snapshot_refresh.duration", elapsed_ms as f64)
        .unit(Unit::Millisecond)
        .attribute("surface", "desktop")
        .attribute("step", step)
        .capture();
}

pub(super) fn record_scan_count(surface: Surface, count: usize) {
    metrics::distribution("skill_studio.scan.skills", count as f64)
        .attribute("surface", surface.as_str())
        .capture();
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn open_file_descriptor_count() -> Option<usize> {
    let pid = i32::try_from(std::process::id()).ok()?;
    // A size probe and read through libproc don't themselves open a descriptor.
    // SAFETY: the null buffer and zero length request the required byte count.
    let capacity =
        unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, std::ptr::null_mut(), 0) };
    if capacity <= 0 {
        return None;
    }
    let mut buffer = vec![0_u8; usize::try_from(capacity).ok()?];
    // SAFETY: the writable buffer has exactly capacity bytes and outlives the call.
    let written = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDLISTFDS,
            0,
            buffer.as_mut_ptr().cast(),
            capacity,
        )
    };
    (written > 0).then(|| written as usize / std::mem::size_of::<libc::proc_fdinfo>())
}

#[cfg(target_os = "linux")]
fn open_file_descriptor_count() -> Option<usize> {
    let entries = std::fs::read_dir("/proc/self/fd").ok()?;
    // Exclude the directory stream's own descriptor.
    Some(entries.filter_map(Result::ok).count().saturating_sub(1))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn open_file_descriptor_count() -> Option<usize> {
    None
}
