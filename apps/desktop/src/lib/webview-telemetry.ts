// ============================================================================
// Skill Studio - webview-telemetry
// The WebView's Sentry SDK: errors with stack traces and React component
// stacks, screen timings, long tasks, and INP. It never talks to sentry.io.
// Its transport hands each envelope's items to the `send_webview_envelope`
// Tauri command, and Rust decides whether to send them and scrubs them first
// (`crates/skill-studio-host/src/telemetry/webview.rs`). It starts only when
// `get_webview_telemetry` says the telemetry switch is on.
// Calls `invoke` directly, not `callCommand` (skill-api.ts): `callCommand`
// wraps each call in an `ipc` span, and a span around the transport would
// make every sent envelope start another one.
// ============================================================================

import * as Sentry from "@sentry/react";
import { invoke } from "@tauri-apps/api/core";

type Transport = ReturnType<NonNullable<Sentry.BrowserOptions["transport"]>>;
type Envelope = Parameters<Transport["send"]>[0];

/** The envelope item types Rust accepts. Replays, attachments, sessions,
 * and client reports never leave the WebView. */
const SENT_ITEM_TYPES = new Set(["event", "transaction", "span"]);

/** Never contacted: the transport below replaces the network. The SDK
 * needs a well-formed DSN to start. */
const PLACEHOLDER_DSN = "https://webview@telemetry.invalid/0";

/** SDK 11's replacement for `sendDefaultPii: false`, at its strictest. */
const NO_PERSONAL_DATA: Sentry.BrowserOptions["dataCollection"] = {
  userInfo: false,
  cookies: false,
  httpHeaders: false,
  httpBodies: [],
  urlQueryParams: false,
  stackFrameVariables: false,
  frameContextLines: 0,
};

let enabled = false;
let started = false;
let currentScreen: () => string = () => "unknown";

function screenName(): string {
  return `view.${currentScreen()}`;
}

/** Sends nothing while `isEnabled()` is false. Never throws: a failed
 * telemetry send must not surface as a second error. */
export function createIpcTransport(isEnabled: () => boolean): Transport {
  return {
    send(envelope: Envelope) {
      if (!isEnabled()) return Promise.resolve({});
      const items = envelope[1].flatMap(([header, payload]) =>
        SENT_ITEM_TYPES.has(header.type) ? [{ type: header.type, payload }] : [],
      );
      if (items.length === 0) return Promise.resolve({});
      return invoke("send_webview_envelope", { items }).then(
        () => ({}),
        () => ({}),
      );
    },
    flush: () => Promise.resolve(true),
  };
}

function start(): void {
  const keepWhileEnabled = <T>(item: T): T | null => (enabled ? item : null);
  Sentry.init({
    dsn: PLACEHOLDER_DSN,
    transport: () => createIpcTransport(() => enabled),
    defaultIntegrations: false,
    integrations: [
      Sentry.eventFiltersIntegration(),
      Sentry.functionToStringIntegration(),
      Sentry.browserApiErrorsIntegration(),
      Sentry.globalHandlersIntegration(),
      Sentry.linkedErrorsIntegration(),
      Sentry.dedupeIntegration(),
      Sentry.browserTracingIntegration({
        instrumentNavigation: false,
        traceFetch: false,
        traceXHR: false,
        enableHTTPTimings: false,
        linkPreviousTrace: "off",
        beforeStartSpan: (options) => ({ ...options, name: screenName() }),
      }),
    ],
    dataCollection: NO_PERSONAL_DATA,
    tracesSampleRate: 1,
    tracePropagationTargets: [],
    sendClientReports: false,
    maxBreadcrumbs: 0,
    beforeBreadcrumb: () => null,
    beforeSend: keepWhileEnabled,
    beforeSendTransaction: keepWhileEnabled,
  });
}

/** Reads the switch from Rust and starts the SDK the first time it is on.
 * Call at launch and after every save of the switch. Turning the switch off
 * leaves the SDK loaded but sending nothing. */
export async function syncWebviewTelemetry(): Promise<void> {
  enabled = await invoke<boolean>("get_webview_telemetry").catch(() => false);
  if (enabled && !started) {
    started = true;
    start();
  }
}

/** `screen` names the screen the user is on, such as `home` or `skills`.
 * Screen timings are named `view.<screen>`. */
export function setScreenSource(screen: () => string): void {
  currentScreen = screen;
}

/** Starts the timing for a screen change. */
export function trackScreenChange(): void {
  const client = Sentry.getClient();
  if (!enabled || !client) return;
  Sentry.startBrowserTracingNavigationSpan(client, { name: screenName() });
}

/** Runs one Tauri command inside an `ipc` span and gives `run` the
 * `sentry-trace` header for it, so the Rust command root continues the
 * WebView trace. With no active WebView span, `run` gets no header and the
 * Rust command root starts its own trace. */
export function withIpcSpan<T>(
  command: string,
  run: (headers: Record<string, string> | undefined) => Promise<T>,
): Promise<T> {
  if (!enabled || !Sentry.getActiveSpan()) return run(undefined);
  return Sentry.startSpan({ name: command, op: "ipc" }, (span) =>
    run({ "sentry-trace": Sentry.spanToTraceHeader(span) }),
  );
}

/** The handler for React's `onUncaughtError`, `onCaughtError`, and
 * `onRecoverableError`. It adds the component stack to the event. */
export const reactErrorHandler = Sentry.reactErrorHandler((error) => {
  // eslint-disable-next-line no-console
  if (import.meta.env.DEV) console.error(error);
});
