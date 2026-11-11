import React from "react";
import ReactDOM from "react-dom/client";
import { KitPreview } from "@skill-studio/ui";
import App from "./App";
import { AppErrorBoundary } from "./components/Shell/AppErrorBoundary";
import { stampInitialTheme } from "./lib/theme";
import {
  reactErrorHandler,
  setScreenSource,
  syncWebviewTelemetry,
  trackScreenChange,
} from "./lib/webview-telemetry";
import { useAppStore } from "./store/appStore";

// Stamped before React renders, so the app never flashes the wrong palette
// on first paint.
stampInitialTheme();

// Dev-only proof that @skill-studio/ui renders themed with the app's tokens.
// Not a real route: visit with `#kit` while running `npm run dev`.
const showKitPreview = import.meta.env.DEV && location.hash === "#kit";

// Dev-only perf HUD: visit with `?perf=1` while running `npm run dev`. See
// components/dev/PerfOverlay.tsx and lib/perf-marks.ts.
const showPerfOverlay =
  import.meta.env.DEV && new URLSearchParams(location.search).get("perf") === "1";

async function boot(): Promise<void> {
  const rootElement = document.getElementById("root");
  if (!rootElement) {
    throw new Error("Skill Studio root element #root is missing from index.html");
  }

  // No Tauri runtime behind this tab (a plain browser dev session): install
  // the mock IPC layer so an agent-browser session can drive the app from a
  // realistic in-memory skill estate. See `dev/harness/mock-tauri.ts`.
  if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
    const [{ buildHarnessSnapshot }, { installMockTauri }] = await Promise.all([
      import("./dev/harness/skill-fixture"),
      import("./dev/harness/mock-tauri"),
    ]);
    // `?n=378` pads the estate to a realistic size for performance runs.
    const skillCount = Number(new URLSearchParams(window.location.search).get("n") ?? 0);
    window.__harness = installMockTauri(buildHarnessSnapshot(skillCount));
    // eslint-disable-next-line no-console
    console.info("[harness] mock Tauri IPC installed");
  }

  setScreenSource(() => useAppStore.getState().activeView.kind);
  useAppStore.subscribe((state, previous) => {
    if (state.activeView.kind !== previous.activeView.kind) trackScreenChange();
  });
  await syncWebviewTelemetry();

  const PerfOverlay = showPerfOverlay
    ? (await import("./components/dev/PerfOverlay")).PerfOverlay
    : null;

  ReactDOM.createRoot(rootElement, {
    onUncaughtError: reactErrorHandler,
    onCaughtError: reactErrorHandler,
    onRecoverableError: reactErrorHandler,
  }).render(
    <React.StrictMode>
      <AppErrorBoundary>
        {showKitPreview ? <KitPreview /> : <App />}
        {PerfOverlay && <PerfOverlay />}
      </AppErrorBoundary>
    </React.StrictMode>,
  );
}

void boot();
