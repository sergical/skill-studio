// ============================================================================
// Skill Studio - Site Sentry tests
// Flow: the site starts Sentry after the first render, only when the build has a DSN.
// ============================================================================

import { fileURLToPath } from "node:url";
import { build, type Rollup } from "vite";
import { afterEach, describe, expect, it, vi } from "vitest";

import * as Sentry from "@sentry/browser";

import {
  bufferEarlyErrors,
  scrubSentryEnvelope,
  siteSentryEnvironment,
  siteSentryInitOptions,
  startSiteSentry,
} from "./site-sentry";
import type { EarlyErrors, SiteSentryEnv, SiteSentrySdk } from "./site-sentry";

const ENV: SiteSentryEnv = {
  dsn: "https://key@example.invalid/1",
  release: "",
  branch: "",
  mode: "production",
};

const PAGE = "https://useskillstudio.com/docs";
const PRIVATE_URL = `${PAGE}?email=visitor%40mail.test#token=PRIVATE`;

type SentryEnvelope = Parameters<ReturnType<SiteSentrySdk["makeFetchTransport"]>["send"]>[0];

function fakeSdk() {
  return {
    ...Sentry,
    init: vi.fn<SiteSentrySdk["init"]>(),
    captureException: vi.fn<SiteSentrySdk["captureException"]>(),
    replayIntegration: vi.fn<SiteSentrySdk["replayIntegration"]>(Sentry.replayIntegration),
  };
}

function noEarlyErrors(): EarlyErrors {
  return { stop: () => [] };
}

function errorEvent(error: Error) {
  return Object.assign(new Event("error"), { error, message: error.message });
}

describe("startSiteSentry", () => {
  it.each([undefined, "", "   "])("never loads the SDK when the DSN is %j", async (dsn) => {
    // Failure: a build without a DSN still downloads the SDK or sends network calls.
    const loadSdk = vi.fn();
    await startSiteSentry(noEarlyErrors(), { ...ENV, dsn }, loadSdk);
    expect(loadSdk).not.toHaveBeenCalled();
  });

  it("loads the SDK once and initialises it when the DSN is set", async () => {
    // Failure: a configured build never starts monitoring.
    const sdk = fakeSdk();
    await startSiteSentry(noEarlyErrors(), ENV, async () => sdk);
    expect(sdk.init).toHaveBeenCalledTimes(1);
    expect(sdk.init.mock.calls[0][0]).toMatchObject({ dsn: ENV.dsn });
  });

  it("keeps the page working when the SDK chunk fails to load", async () => {
    // Failure: an ad blocker that stops the chunk turns into an unhandled rejection on the page.
    await expect(
      startSiteSentry(noEarlyErrors(), ENV, () => Promise.reject(new Error("blocked"))),
    ).resolves.toBeUndefined();
  });

  it("reports an error thrown before the SDK loaded, once, as unhandled", async () => {
    // Failure: a crash during the first render never reaches Sentry, because the SDK loads later.
    const page = new EventTarget();
    // SAFETY: the buffer only adds and removes listeners; an EventTarget has both.
    const earlyErrors = bufferEarlyErrors(page as Window);
    const crash = new Error("render failed");
    page.dispatchEvent(errorEvent(crash));
    const sdk = fakeSdk();

    await startSiteSentry(earlyErrors, ENV, async () => sdk);
    page.dispatchEvent(errorEvent(new Error("after init")));

    expect(sdk.captureException).toHaveBeenCalledTimes(1);
    expect(sdk.captureException).toHaveBeenCalledWith(crash, {
      mechanism: { type: "auto.browser.global_handlers.onerror", handled: false },
    });
  });

  it("stops listening for errors when the build has no DSN", async () => {
    // Failure: a build without monitoring keeps every page error in memory.
    const page = new EventTarget();
    // SAFETY: see above.
    const earlyErrors = bufferEarlyErrors(page as Window);
    await startSiteSentry(earlyErrors, { ...ENV, dsn: undefined }, vi.fn());
    page.dispatchEvent(errorEvent(new Error("later")));
    expect(earlyErrors.stop()).toEqual([]);
  });
});

describe("siteSentryInitOptions", () => {
  it("masks all text and inputs, blocks media, and sends no personal data", () => {
    // Failure: a replay records visitor text or media, or the SDK attaches IP, cookies, or headers.
    const sdk = fakeSdk();
    const options = siteSentryInitOptions(ENV, sdk);
    expect(options.sendDefaultPii).toBe(false);
    expect(options.dataCollection).toMatchObject({ cookies: false, httpHeaders: false });
    expect(sdk.replayIntegration).toHaveBeenCalledWith(
      expect.objectContaining({ maskAllText: true, maskAllInputs: true, blockAllMedia: true }),
    );
  });

  it("keeps replay recordings uncompressed, so the envelope scrub can read them", () => {
    // Failure: compressed recordings pass the scrub as bytes and send the full page URL.
    const sdk = fakeSdk();
    siteSentryInitOptions(ENV, sdk);
    expect(sdk.replayIntegration).toHaveBeenCalledWith(
      expect.objectContaining({ useCompression: false }),
    );
  });

  it("traces every page load and records every replay that has an error", () => {
    // Failure: Web Vitals samples are dropped, or an error session has no replay.
    const options = siteSentryInitOptions(ENV, fakeSdk());
    expect(options.tracesSampleRate).toBe(1);
    expect(options.replaysOnErrorSampleRate).toBe(1);
    expect(options.replaysSessionSampleRate).toBeGreaterThan(0);
    expect(options.replaysSessionSampleRate).toBeLessThan(1);
  });

  it("sends no trace headers to other origins", () => {
    // Failure: the site adds sentry-trace headers to third-party requests.
    expect(siteSentryInitOptions(ENV, fakeSdk()).tracePropagationTargets).toEqual([]);
  });

  it("leaves the release unset when the build has no commit SHA", () => {
    // Failure: an empty string becomes a release named "" in Sentry.
    expect(siteSentryInitOptions(ENV, fakeSdk()).release).toBeUndefined();
    expect(siteSentryInitOptions({ ...ENV, release: "abc123" }, fakeSdk()).release).toBe("abc123");
  });
});

describe("sent data", () => {
  afterEach(async () => {
    await Sentry.close();
    vi.unstubAllGlobals();
  });

  it("drops the page query and fragment from a real error event and its breadcrumbs", async () => {
    // Failure: an email or token in the page URL reaches Sentry through the request or a breadcrumb.
    vi.stubGlobal("location", new URL(PRIVATE_URL));
    vi.stubGlobal("document", { location: new URL(PRIVATE_URL), referrer: PRIVATE_URL });
    const sent: SentryEnvelope[] = [];
    const sdk = {
      ...fakeSdk(),
      makeFetchTransport: () => ({
        send: async (envelope: SentryEnvelope) => {
          sent.push(envelope);
          return {};
        },
        flush: async () => true,
      }),
    };
    // Node has no DOM for the tracing and replay integrations; the page URL comes from HttpContext.
    Sentry.init({
      ...siteSentryInitOptions(ENV, sdk),
      defaultIntegrations: false,
      integrations: [Sentry.httpContextIntegration()],
    });
    Sentry.addBreadcrumb({
      category: "navigation",
      data: { from: PRIVATE_URL, to: "/?ref=PRIVATE" },
    });

    Sentry.captureException(new Error("boom"));
    await Sentry.flush(2000);

    const body = JSON.stringify(sent);
    expect(body).toContain(`"url":"${PAGE}"`);
    expect(body).not.toContain("PRIVATE");
    expect(body).not.toContain("visitor");
  });

  it("drops the query and fragment from replay URLs and keeps the recording length exact", () => {
    // Failure: Replay sends the full page URL, or a wrong length makes Sentry reject the recording.
    const recording = `{"segment_id":0}\n${JSON.stringify([
      { type: 4, data: { href: PRIVATE_URL, width: 800, height: 600 } },
      {
        type: 5,
        data: {
          tag: "breadcrumb",
          payload: { category: "navigation", data: { to: "/docs#PRIVATE" } },
        },
      },
    ])}`;
    const envelope: SentryEnvelope = [
      { event_id: "1", trace: { trace_id: "1", public_key: "key", transaction: "/docs" } },
      [
        [
          { type: "replay_event" },
          {
            type: "replay_event",
            urls: [PRIVATE_URL],
            error_ids: [],
            trace_ids: [],
            segment_names: [],
            replay_id: "1",
            segment_id: 0,
            replay_type: "session",
          },
        ],
        [{ type: "replay_recording", length: recording.length }, recording],
      ],
    ];

    const scrubbed = scrubSentryEnvelope(envelope);
    const [, [, [recordingHeader, recordingText]]] = scrubbed;

    expect(JSON.stringify(scrubbed)).not.toContain("PRIVATE");
    expect(String(recordingText)).toContain(`"href":"${PAGE}"`);
    expect(recordingHeader).toMatchObject({
      length: new TextEncoder().encode(String(recordingText)).length,
    });
  });

  it("keeps text that is not a URL", () => {
    // Failure: an error message with a question mark loses its words.
    const envelope: SentryEnvelope = [
      { event_id: "1", sent_at: "2026-11-10T00:00:00.000Z" },
      [[{ type: "event" }, { message: "Why? #1 failed", transaction: "/docs" }]],
    ];
    expect(JSON.stringify(scrubSentryEnvelope(envelope))).toContain("Why? #1 failed");
  });
});

describe("siteSentryEnvironment", () => {
  it.each([
    ["main", "production", "production"],
    ["feature/x", "production", "preview"],
    ["", "development", "development"],
  ])("maps branch %j in mode %j to %j", (branch, mode, expected) => {
    // Failure: preview builds of pull requests count as production traffic.
    expect(siteSentryEnvironment({ branch, mode })).toBe(expected);
  });
});

describe("prerender build", () => {
  it("does not bundle or import the SDK or the Sentry start file", async () => {
    // Failure: the server-side prerender runs Sentry code in Node, or pulls browser code into it.
    const root = fileURLToPath(new URL("..", import.meta.url));
    const result = await build({
      root,
      logLevel: "silent",
      build: {
        ssr: "src/prerender/site-prerender.tsx",
        write: false,
      },
    });
    const chunks = [result]
      .flat()
      .filter((entry): entry is Rollup.RollupOutput => "output" in entry)
      .flatMap((entry) => entry.output)
      .filter((output): output is Rollup.OutputChunk => output.type === "chunk");
    expect(chunks.length).toBeGreaterThan(0);
    for (const chunk of chunks) {
      expect(chunk.code).not.toContain("@sentry");
      expect(chunk.moduleIds.filter((id) => id.includes("site-sentry"))).toEqual([]);
    }
  }, 60_000);
});
