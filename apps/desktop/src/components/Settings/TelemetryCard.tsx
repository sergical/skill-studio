// ============================================================================
// TelemetryCard - Settings' "Telemetry" section: the switch for crash reports,
// operation timings, WebView errors, and screen performance. Off in the registry by default; the
// welcome screen offers it on and writes the user's explicit choice; this
// card lets the user change it later - see the Rust `telemetry_commands` and
// `crates/skill-studio-host/src/telemetry.rs` for what a report can and
// can't carry.
// ============================================================================

import { useLoad } from "../../hooks/useLoad";
import { useAppStore } from "../../store/appStore";
import { SwitchControl } from "../ui/SwitchControl";
import { PageSection } from "../Shell/PageSection";

export function TelemetryCard() {
  const addToast = useAppStore((state) => state.addToast);
  const getTelemetryEnabled = useAppStore((state) => state.getTelemetryEnabled);
  const setTelemetryEnabled = useAppStore((state) => state.setTelemetryEnabled);
  const telemetry = useLoad("telemetry", getTelemetryEnabled, {
    onFailed: (message) =>
      addToast({ type: "error", title: "Couldn't read your Telemetry setting", message }),
  });
  const enabled = telemetry.data ?? false;

  const toggle = async (next: boolean) => {
    const previous = enabled;
    telemetry.setData(next);
    try {
      await setTelemetryEnabled(next);
    } catch (err) {
      telemetry.setData(previous);
      addToast({
        type: "error",
        title: "Couldn't save your Telemetry setting",
        message: err instanceof Error ? err.message : "Unknown error",
      });
    }
  };

  return (
    <PageSection
      title="Telemetry"
      meta={
        <SwitchControl
          checked={enabled}
          onCheckedChange={toggle}
          disabled={telemetry.isLoading}
          ariaLabel="Telemetry"
        />
      }
    >
      <p className="m-0 max-w-prose text-small text-pretty text-text-tertiary">
        Sends crash and error locations in Skill Studio's own code, how long each action took, and
        how fast each screen responds. Skill names, files, paths and the machine name are removed
        before it leaves this Mac.
      </p>
    </PageSection>
  );
}
