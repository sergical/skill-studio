// ============================================================================
// TelemetryCard - Settings' "Telemetry" section: the switch for crash reports,
// operation timings, WebView errors, and screen performance. Off in the registry by default; the
// welcome screen offers it on and writes the user's explicit choice; this
// card lets the user change it later - see the Rust `telemetry_commands` and
// `crates/skill-studio-host/src/telemetry.rs` for what a report can and
// can't carry.
// ============================================================================

import { useEffect, useState } from "react";
import { getTelemetryEnabled, setTelemetryEnabled } from "../../lib/skill-api";
import { useAppStore } from "../../store/appStore";
import { SwitchControl } from "../ui/SwitchControl";
import { PageSection } from "../Shell/PageSection";

export function TelemetryCard() {
  const addToast = useAppStore((state) => state.addToast);
  const [enabled, setEnabled] = useState(false);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    getTelemetryEnabled()
      .then((result) => {
        if (!cancelled) setEnabled(result);
      })
      .catch((err) => {
        addToast({
          type: "error",
          title: "Couldn't read your Telemetry setting",
          message: err instanceof Error ? err.message : "Unknown error",
        });
      })
      .finally(() => {
        if (!cancelled) setIsLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [addToast]);

  const toggle = async (next: boolean) => {
    const previous = enabled;
    setEnabled(next);
    try {
      await setTelemetryEnabled(next);
    } catch (err) {
      setEnabled(previous);
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
          disabled={isLoading}
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
