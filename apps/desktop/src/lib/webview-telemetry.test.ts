// @vitest-environment happy-dom
// ============================================================================
// Skill Studio - webview-telemetry tests
// Guards the two promises the WebView SDK makes to Rust: with the switch off
// nothing reaches `send_webview_envelope`, and a Tauri command started in a
// WebView span carries that span's trace in its `sentry-trace` header.
// `mockIPC` stands in for the Tauri bridge.
// ============================================================================

import { afterEach, describe, expect, it } from "vitest";
import * as Sentry from "@sentry/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { createIpcTransport, syncWebviewTelemetry, withIpcSpan } from "./webview-telemetry";

type Envelope = Parameters<ReturnType<typeof createIpcTransport>["send"]>[0];

/** The arguments of each `send_webview_envelope` call. */
function recordIpc(webviewTelemetry: boolean): unknown[] {
  const sent: unknown[] = [];
  mockIPC((command, payload) => {
    if (command === "get_webview_telemetry") return webviewTelemetry;
    if (command === "send_webview_envelope") {
      sent.push(payload);
      return 1;
    }
    return undefined;
  });
  return sent;
}

const EVENT_ID = "0".repeat(32);

const ENVELOPE_HEADERS = { event_id: EVENT_ID, sent_at: "2026-11-10T00:00:00.000Z" };

const ERROR_AND_TRANSACTION: Envelope = [
  ENVELOPE_HEADERS,
  [
    [{ type: "event" }, { event_id: EVENT_ID }],
    [{ type: "transaction" }, { event_id: EVENT_ID, type: "transaction" }],
  ],
];

const MIXED_ITEMS: Envelope = [
  ENVELOPE_HEADERS,
  [
    [{ type: "event" }, { event_id: EVENT_ID }],
    [{ type: "attachment", length: 1, filename: "state.json" }, "x"],
    [{ type: "transaction" }, { event_id: EVENT_ID, type: "transaction" }],
  ],
];

afterEach(() => {
  clearMocks();
});

describe("the WebView IPC transport", () => {
  // Flow: the user turned telemetry off. Expectation: an envelope the SDK
  // hands the transport never reaches Rust. Failure caught: the WebView
  // calling `send_webview_envelope` while the switch is off.
  it("sends nothing to Rust while the switch is off", async () => {
    const sent = recordIpc(false);

    await createIpcTransport(() => false).send(ERROR_AND_TRANSACTION);

    expect(sent).toEqual([]);
  });

  // Flow: the switch is on and the SDK sends an envelope that also holds an
  // attachment. Expectation: only the event and the
  // transaction go to Rust. Failure caught: an attachment, which can hold
  // any bytes, leaving the WebView.
  it("hands Rust only events, transactions, and spans", async () => {
    const sent = recordIpc(true);

    await createIpcTransport(() => true).send(MIXED_ITEMS);

    expect(sent).toEqual([
      {
        items: [
          expect.objectContaining({ type: "event" }),
          expect.objectContaining({ type: "transaction" }),
        ],
      },
    ]);
  });
});

describe("WebView telemetry", () => {
  // Flow: the switch is on and a screen span is active when the app calls a
  // Tauri command. Expectation: the command's `sentry-trace` header has the
  // screen span's trace id and a sampled flag, so the Rust command root
  // continues the WebView trace. Failure caught: the WebView and Rust
  // traces being cut apart, or an unsampled header making Rust drop the
  // user's command.
  it("gives a Tauri command the trace of the active WebView span", async () => {
    recordIpc(true);
    await syncWebviewTelemetry();

    let screenTraceId = "";
    const headers = await Sentry.startSpan({ name: "view.home" }, (screen) => {
      screenTraceId = screen.spanContext().traceId;
      return withIpcSpan("scan_skills", (sentHeaders) => Promise.resolve(sentHeaders));
    });

    const [traceId, , sampled] = headers?.["sentry-trace"]?.split("-") ?? [];
    expect(traceId).toBe(screenTraceId);
    expect(sampled).toBe("1");
  });

  // Flow: the switch was on, then the user turned it off. Expectation: a
  // later error is not sent to Rust, and commands get no trace header.
  // Failure caught: the already-started SDK still sending after opt-out.
  it("sends nothing after the switch is turned off", async () => {
    recordIpc(true);
    await syncWebviewTelemetry();
    const sent = recordIpc(false);
    await syncWebviewTelemetry();

    Sentry.captureException(new TypeError("boom"));
    await Sentry.flush(100);
    const headers = await Sentry.startSpan({ name: "view.home" }, () =>
      withIpcSpan("scan_skills", (sentHeaders) => Promise.resolve(sentHeaders)),
    );

    expect(sent).toEqual([]);
    expect(headers).toBeUndefined();
  });
});
