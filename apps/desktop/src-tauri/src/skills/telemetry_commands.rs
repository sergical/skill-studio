// ============================================================================
// Skills Module - telemetry_commands
// The telemetry switch (unit 6.4): `telemetry_enabled` in
// `~/.agents/skill-studio.json`, alongside the other settings in
// `skill_fork_registry` - off by default, covering crash reports, operation
// timings, and WebView errors and screen timings. The welcome screen offers it on and
// `save_harnesses_choice` writes the user's explicit choice; Settings'
// "Telemetry" card keeps it in sync afterward. The actual Sentry client, its
// panic hook, and its consent gate live in `skill_studio_host::telemetry`;
// this module only owns the two Tauri commands that read and flip the
// persisted switch, the `TelemetryState` they flip alongside it, and the
// two commands the WebView's Sentry SDK uses: `get_webview_telemetry` and
// `send_webview_envelope`.
// ============================================================================

use std::path::Path;
use std::sync::Mutex;

use skill_studio_host::telemetry::{Consent, TelemetryGuard, WebviewTelemetry};
use tauri::Manager;

/// The Tauri-managed telemetry state: the live consent flag every command
/// below flips, and the Sentry client guard `run()`'s exit handler takes
/// out of the `Mutex` to flush and close on `RunEvent::Exit`.
pub struct TelemetryState {
    /// The gate `skill_studio_host::telemetry::ConsentTransport` checks
    /// before forwarding an envelope.
    pub consent: Consent,
    /// `None` when `telemetry::init` found no DSN (every build until
    /// `SKILL_STUDIO_SENTRY_DSN` is set) - there is nothing to flush.
    pub guard: Mutex<Option<TelemetryGuard>>,
    /// The client for webview envelopes. `None` when no
    /// `SKILL_STUDIO_SENTRY_WEBVIEW_DSN` was set at build or run time.
    pub webview: Option<WebviewTelemetry>,
}

/// The saved switch, straight off disk - like `get_discovery_sources`, this
/// doesn't change anything, so it reads the registry directly.
#[tauri::command]
pub async fn get_telemetry_enabled(
    trace: crate::timing_log::IpcTrace,
    app: tauri::AppHandle,
) -> Result<bool, String> {
    let timing_app = app.clone();
    crate::timing_log::time_command_blocking(
        &timing_app,
        trace,
        "get_telemetry_enabled",
        move || {
            let home = dirs::home_dir().ok_or("Could not find home directory")?;
            Ok(super::skill_fork_registry::read_fork_registry(&home)?.telemetry_enabled)
        },
    )
    .await
}

/// Saves the switch and flips the live `Consent` so it takes effect
/// without a restart. Returns the saved value.
#[tauri::command]
pub async fn set_telemetry_enabled(
    trace: crate::timing_log::IpcTrace,
    enabled: bool,
    app: tauri::AppHandle,
) -> Result<bool, String> {
    let timing_app = app.clone();
    crate::timing_log::time_command_blocking(
        &timing_app,
        trace,
        "set_telemetry_enabled",
        move || {
            let home = dirs::home_dir().ok_or("Could not find home directory")?;
            let consent = app.state::<TelemetryState>().consent.clone();
            set_telemetry_enabled_at(&home, enabled, &consent)?;
            Ok(enabled)
        },
    )
    .await
}

/// The persist-and-flip body of `set_telemetry_enabled`, kept apart so
/// a test can drive it with a plain `home` path and `Consent` - a
/// `tauri::AppHandle` can't be constructed outside a running app (see
/// `harness_first_run::save_harnesses_choice_at`'s own split for the same
/// reason). Saves the switch and flips the live `Consent` so it takes
/// effect without a restart.
pub(crate) fn set_telemetry_enabled_at(
    home: &Path,
    enabled: bool,
    consent: &Consent,
) -> Result<(), String> {
    set_telemetry_enabled_at_with(
        home,
        enabled,
        consent,
        std::env::var("SKILL_STUDIO_TELEMETRY").ok(),
    )
}

/// As [`set_telemetry_enabled_at`], but with the env override passed
/// in rather than read from the process - so a test can prove
/// `SKILL_STUDIO_TELEMETRY` still wins over the switch just saved, without
/// mutating process-wide env.
fn set_telemetry_enabled_at_with(
    home: &Path,
    enabled: bool,
    consent: &Consent,
    env_override: Option<String>,
) -> Result<(), String> {
    super::skill_fork_registry::update_fork_registry(home, |registry| {
        registry.telemetry_enabled = enabled;
    })?;
    consent.set(skill_studio_host::telemetry::resolve_consent(
        env_override,
        enabled,
    ));
    Ok(())
}

/// Whether the webview should start its Sentry SDK: a webview client
/// exists and the switch is on. Not timed, so it adds no command root.
#[allow(clippy::needless_pass_by_value)] // Tauri's command extractor requires owned arguments.
#[tauri::command(async)]
pub fn get_webview_telemetry(state: tauri::State<'_, TelemetryState>) -> bool {
    state.webview.is_some() && state.consent.enabled()
}

/// The webview Sentry SDK's only way out: it hands its envelope items here,
/// and `WebviewTelemetry::send_json` drops them while the switch is off and
/// scrubs them otherwise. Returns how many items were sent. Not timed, so
/// sending telemetry does not make more telemetry.
#[allow(clippy::needless_pass_by_value)] // Tauri's command extractor requires owned arguments.
#[tauri::command(async)]
pub fn send_webview_envelope(
    request: tauri::ipc::Request<'_>,
    state: tauri::State<'_, TelemetryState>,
) -> usize {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return 0;
    };
    let Ok(items) = std::str::from_utf8(bytes) else {
        return 0;
    };
    state
        .webview
        .as_ref()
        .map_or(0, |webview| webview.send_json(items))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `turning_the_settings_switch_off_stops_reports_before_restart`: the
    /// same persist-and-flip function called with `false` must leave both
    /// the registry and the live `Consent` off - the property that makes a
    /// telemetry opt-out take effect immediately rather than at next
    /// launch.
    #[test]
    fn turning_the_settings_switch_off_stops_reports_before_restart() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(home.join(".agents")).unwrap();
        let mut registry = super::super::skill_fork_registry::read_fork_registry(&home).unwrap();
        registry.telemetry_enabled = true;
        super::super::skill_fork_registry::write_fork_registry(&home, &mut registry).unwrap();
        let consent = Consent::new(true);

        set_telemetry_enabled_at_with(&home, false, &consent, None).unwrap();

        assert!(
            !consent.enabled(),
            "turning the switch off must flip the live Consent before restart"
        );
        let after = super::super::skill_fork_registry::read_fork_registry(&home).unwrap();
        assert!(
            !after.telemetry_enabled,
            "turning the switch off must persist false to the registry"
        );
    }

    /// `SKILL_STUDIO_TELEMETRY=0` must keep the live `Consent` off even
    /// though the switch itself is being turned on - the env override, not
    /// the just-saved switch, decides the live value.
    #[test]
    fn an_env_override_of_0_keeps_consent_false_after_enabling_the_switch() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(home.join(".agents")).unwrap();
        let consent = Consent::new(false);

        set_telemetry_enabled_at_with(&home, true, &consent, Some("0".to_string())).unwrap();

        assert!(
            !consent.enabled(),
            "the env override must win over the switch this call just turned on"
        );
        let after = super::super::skill_fork_registry::read_fork_registry(&home).unwrap();
        assert!(
            after.telemetry_enabled,
            "the registry still records the user's choice, independent of the env override"
        );
    }
}
