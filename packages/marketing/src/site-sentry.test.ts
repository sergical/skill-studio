// ============================================================================
// Skill Studio - Site Sentry tests
// Flow: the site starts Sentry after the first render, only when the build has a DSN.
// ============================================================================

import { fileURLToPath } from "node:url";
import { build, type Rollup } from "vite";
import { describe, expect, it, vi } from "vitest";

import { siteSentryEnvironment, siteSentryInitOptions, startSiteSentry } from "./site-sentry";
import type { SiteSentryEnv, SiteSentrySdk } from "./site-sentry";

const ENV: SiteSentryEnv = {
  dsn: "https://key@example.invalid/1",
  release: "",
  branch: "",
  mode: "production",
};

function fakeSdk() {
  return {
    init: vi.fn<SiteSentrySdk["init"]>(),
    browserTracingIntegration: vi.fn<SiteSentrySdk["browserTracingIntegration"]>(() => ({
      name: "tracing",
    })),
    replayIntegration: vi.fn<SiteSentrySdk["replayIntegration"]>(() => ({ name: "replay" })),
  };
}

describe("startSiteSentry", () => {
  it.each([undefined, "", "   "])("never loads the SDK when the DSN is %j", async (dsn) => {
    // Failure: a build without a DSN still downloads the SDK or sends network calls.
    const loadSdk = vi.fn();
    await startSiteSentry({ ...ENV, dsn }, loadSdk);
    expect(loadSdk).not.toHaveBeenCalled();
  });

  it("loads the SDK once and initialises it when the DSN is set", async () => {
    // Failure: a configured build never starts monitoring.
    const sdk = fakeSdk();
    await startSiteSentry(ENV, async () => sdk);
    expect(sdk.init).toHaveBeenCalledTimes(1);
    expect(sdk.init.mock.calls[0][0]).toMatchObject({ dsn: ENV.dsn });
  });

  it("keeps the page working when the SDK chunk fails to load", async () => {
    // Failure: an ad blocker that stops the chunk turns into an unhandled rejection on the page.
    await expect(
      startSiteSentry(ENV, () => Promise.reject(new Error("blocked"))),
    ).resolves.toBeUndefined();
  });
});

describe("siteSentryInitOptions", () => {
  it("masks all text and inputs, blocks media, and sends no personal data", () => {
    // Failure: a replay records visitor text or media, or the SDK attaches IP and cookies.
    const sdk = fakeSdk();
    const options = siteSentryInitOptions(ENV, sdk);
    expect(options.sendDefaultPii).toBe(false);
    expect(sdk.replayIntegration).toHaveBeenCalledWith({
      maskAllText: true,
      maskAllInputs: true,
      blockAllMedia: true,
    });
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
