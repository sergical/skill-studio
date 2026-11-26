// ============================================================================
// Skill Studio - Site worker tests
// Flow: an agent or a browser requests a docs page URL.
// Failure: a browser gets Markdown, an agent that asks for Markdown gets HTML,
// or a cache can hand one format to the other client.
// ============================================================================

import { describe, expect, it } from "vitest";

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
