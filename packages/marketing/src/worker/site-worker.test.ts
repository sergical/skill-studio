// ============================================================================
// Skill Studio - Site worker tests
// Flow: an agent or a browser requests a docs page URL.
// Failure: a browser gets Markdown, an agent that asks for Markdown gets HTML,
// or a cache can hand one format to the other client.
// ============================================================================

import { afterEach, describe, expect, it, vi } from "vitest";

import * as Sentry from "@sentry/browser";

import { siteSentryInitOptions } from "../site-sentry";
import worker from "./site-worker";

const FILES = new Map([
  ["/docs/cli/", { type: "text/html", body: "<!doctype html><h1>CLI</h1>", etag: '"html"' }],
  ["/docs/cli.md", { type: "text/markdown", body: "# CLI", etag: '"md"' }],
]);

// Answers like the static assets layer: the file, a 304 for a matching ETag, or a 404.
const env = {
  ASSETS: {
    async fetch(request: Request) {
      const file = FILES.get(new URL(request.url).pathname);
      if (!file) return new Response("Not found", { status: 404 });
      if (request.headers.get("if-none-match") === file.etag) {
        return new Response(null, { status: 304, headers: { etag: file.etag } });
      }
      return new Response(file.body, { headers: { "content-type": file.type, etag: file.etag } });
    },
  },
};

function requestPage(headers: Record<string, string>) {
  return worker.fetch(new Request("https://useskillstudio.com/docs/cli/", { headers }), env);
}

describe("site worker", () => {
  it.each([
    ["Chrome", "text/html,application/xhtml+xml,*/*;q=0.8", "Mozilla/5.0", "text/html", "Accept"],
    ["Claude Code", "text/markdown, text/html, */*", "claude-code/2.1", "text/markdown", "Accept"],
    [
      "an HTML preference",
      "text/html, text/markdown;q=0.9",
      "claude-code/2.1",
      "text/html",
      "Accept",
    ],
    ["HTML ranked higher by a wildcard", "text/*, text/markdown;q=0.5", "", "text/html", "Accept"],
    ["Markdown refused", "text/markdown;q=0, */*", "codex_cli_rs/0.172", "text/html", "Accept"],
    [
      "Codex with no preference",
      "*/*",
      "codex_cli_rs/0.172",
      "text/markdown",
      "Accept, User-Agent",
    ],
    ["ClaudeBot with no Accept", "", "ClaudeBot/1.0", "text/markdown", "Accept, User-Agent"],
    ["curl with no preference", "*/*", "curl/8.7", "text/html", "Accept, User-Agent"],
  ])("answers %s with %s", async (_client, accept, userAgent, expectedType, expectedVary) => {
    const response = await requestPage({ accept, "user-agent": userAgent });

    expect(response.headers.get("content-type")).toContain(expectedType);
    expect(response.headers.get("vary")).toBe(expectedVary);
  });

  it("points the HTML page at its Markdown copy", async () => {
    const response = await requestPage({ accept: "text/html" });

    expect(response.headers.get("link")).toBe(
      '</docs/cli.md>; rel="alternate"; type="text/markdown"',
    );
  });

  it("serves the page's own Markdown file", async () => {
    const response = await requestPage({ accept: "text/markdown" });

    expect(await response.text()).toBe("# CLI");
    expect(response.headers.get("content-type")).toBe("text/markdown; charset=utf-8");
  });

  it("keeps an agent's cached Markdown when it revalidates", async () => {
    const first = await requestPage({ accept: "text/markdown" });
    const etag = first.headers.get("etag") ?? "";

    const again = await requestPage({ accept: "text/markdown", "if-none-match": etag });

    expect(again.status).toBe(304);
    expect(again.headers.get("vary")).toBe("Accept");
  });
});

// Flow: the browser SDK posts an envelope to the same-origin tunnel, which ad blockers allow.
// Failure: the envelope never reaches Sentry, or the tunnel forwards other projects' envelopes.
describe("site worker Sentry tunnel", () => {
  const SITE_DSN = "https://public@o1.ingest.us.sentry.io/42";

  afterEach(async () => {
    await Sentry.close();
    vi.unstubAllGlobals();
  });

  // The visitor's browser with an ad blocker: same-origin requests reach the site worker,
  // requests to sentry.io fail.
  async function browserFetch(input: RequestInfo | URL, init?: RequestInit) {
    const url = new URL(
      input instanceof Request ? input.url : input,
      "https://useskillstudio.com/",
    );
    if (url.hostname.endsWith("sentry.io")) throw new TypeError("net::ERR_BLOCKED_BY_CLIENT");
    const body = new Uint8Array(await new Response(init?.body).arrayBuffer());
    const request = new Request(url, {
      method: init?.method,
      body,
      headers: { "content-length": String(body.length) },
    });
    return worker.fetch(request, { ...env, SENTRY_DSN: SITE_DSN });
  }

  it("delivers a page error to Sentry while the browser blocks sentry.io", async () => {
    // The site worker's own network, where Sentry answers.
    const received: { url: string; body: string }[] = [];
    vi.stubGlobal("fetch", async (url: string, init: RequestInit) => {
      received.push({ url, body: await new Response(init.body).text() });
      return new Response("{}", { status: 200 });
    });
    const sdk = {
      ...Sentry,
      makeFetchTransport: (options: Parameters<typeof Sentry.makeFetchTransport>[0]) =>
        Sentry.makeFetchTransport(options, browserFetch),
    };
    const siteEnv = { dsn: SITE_DSN, release: "", branch: "main", mode: "production" };
    // Node has no DOM for the tracing integration.
    Sentry.init({ ...siteSentryInitOptions(siteEnv, sdk), defaultIntegrations: false });

    Sentry.captureException(new Error("thrown on the homepage"));
    await Sentry.flush(2000);

    expect(received).toHaveLength(1);
    expect(received[0].url).toMatch(/^https:\/\/o1\.ingest\.us\.sentry\.io\/api\/42\/envelope\//);
    expect(received[0].body).toContain("thrown on the homepage");
  });

  it("refuses an envelope for another DSN", async () => {
    const sentry = vi.fn();
    vi.stubGlobal("fetch", sentry);
    const body = `${JSON.stringify({ dsn: "https://public@o9.ingest.sentry.io/9" })}\n{}\n{}`;

    const response = await browserFetch("/m", { method: "POST", body });

    expect(response.status).toBe(403);
    expect(sentry).not.toHaveBeenCalled();
  });
});
