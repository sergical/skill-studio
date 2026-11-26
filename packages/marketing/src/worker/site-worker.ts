// ============================================================================
// Skill Studio - Site worker
// Runs before the static files for /docs/* and the Sentry tunnel (see wrangler.jsonc).
// A docs page URL answers with the page's Markdown copy when the client prefers
// Markdown, and with the HTML page otherwise. The tunnel forwards the browser SDK's
// envelopes to Sentry. Everything else comes from the static files.
// ============================================================================

import { handleTunnelRequest } from "@sentry/core";

import { SITE_SENTRY_TUNNEL_PATH } from "../site-links";

interface Env {
  ASSETS: { fetch(request: Request): Promise<Response> };
  /** The browser SDK's DSN; the tunnel forwards envelopes for this DSN only. */
  SENTRY_DSN?: string;
}

const MARKDOWN_TYPES = ["text/markdown", "text/x-markdown"];
const MARKDOWN_CONTENT_TYPE = "text/markdown; charset=utf-8";

// AI fetchers and crawlers, after the families in @vercel/agent-readability's list. Many send
// no Accept preference (Codex's fetch, ClaudeBot); they get Markdown unless they ask for HTML.
// Search engine crawlers are not listed, so they keep the HTML page.
const AGENT_USER_AGENT =
  /claude|anthropic|chatgpt|gptbot|oai-searchbot|openai|codex|perplexity|cursor|gemini|google-extended|meta-external|mistralai|cohere-ai|amazonbot|deepseekbot|youbot/i;
const SITE_ORIGIN = "https://useskillstudio.com";
const MISSING_PAGE_MARKDOWN = `# Page not found\n\nEvery Skill Studio docs page is listed in ${SITE_ORIGIN}/llms.txt.\n`;

// Each media range the Accept header lists (text/html, text/*, */*), with its quality.
function acceptRanges(accept: string): Map<string, number> {
  const ranges = new Map<string, number>();
  for (const range of accept.split(",")) {
    const [type = "", ...params] = range.split(";").map((part) => part.trim().toLowerCase());
    const q = params.find((param) => param.startsWith("q="));
    const quality = q ? Number(q.slice(2)) : 1;
    if (type) ranges.set(type, Number.isNaN(quality) ? 0 : quality);
  }
  return ranges;
}

/** A type's quality comes from the most specific range that matches it (RFC 9110, 12.5.1). */
function qualityOf(ranges: Map<string, number>, type: string): number {
  return ranges.get(type) ?? ranges.get(`${type.split("/")[0]}/*`) ?? ranges.get("*/*") ?? 0;
}

interface FormatPreference {
  markdown: boolean;
  /** True when Accept ranks HTML and Markdown the same through wildcards alone, so the
   * User-Agent decided. */
  byUserAgent: boolean;
}

function prefersMarkdown(request: Request): FormatPreference {
  // A missing Accept header accepts anything.
  const ranges = acceptRanges(request.headers.get("accept")?.trim() || "*/*");
  const html = qualityOf(ranges, "text/html");
  // text/x-markdown is an alias, so a wildcard match on it must not outrank a listed text/markdown.
  const listedMarkdown = MARKDOWN_TYPES.find((type) => ranges.has(type));
  const markdown = qualityOf(ranges, listedMarkdown ?? "text/markdown");
  if (markdown !== html || markdown === 0) {
    return { markdown: markdown > html, byUserAgent: false };
  }
  // A tie where the client named Markdown itself, like Claude Code's
  // `text/markdown, text/html, */*`, is a request for Markdown.
  if (listedMarkdown) return { markdown: true, byUserAgent: false };
  const byUserAgent = AGENT_USER_AGENT.test(request.headers.get("user-agent") ?? "");
  return { markdown: byUserAgent, byUserAgent: true };
}

/** `/docs/` -> `/docs/index.md`, `/docs/cli/` or `/docs/cli` -> `/docs/cli.md`. */
function markdownPathFor(pathname: string): string | undefined {
  if (pathname === "/docs/" || pathname === "/docs") return "/docs/index.md";
  const slug = /^\/docs\/([a-z0-9-]+)\/?$/.exec(pathname)?.[1];
  return slug ? `/docs/${slug}.md` : undefined;
}

function withHeaders(response: Response, headers: Record<string, string>): Response {
  const result = new Response(response.body, response);
  for (const [name, value] of Object.entries(headers)) result.headers.set(name, value);
  return result;
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const { pathname } = new URL(request.url);

    if (pathname === SITE_SENTRY_TUNNEL_PATH && request.method === "POST") {
      return handleTunnelRequest(request, { allowedDsns: env.SENTRY_DSN ? [env.SENTRY_DSN] : [] });
    }

    // Static files serve .md as text/markdown with no charset.
    if (pathname.endsWith(".md")) {
      const response = await env.ASSETS.fetch(request);
      return response.ok
        ? withHeaders(response, { "content-type": MARKDOWN_CONTENT_TYPE })
        : response;
    }

    const markdownPath = markdownPathFor(pathname);
    if (!markdownPath) return env.ASSETS.fetch(request);

    const preference = prefersMarkdown(request);
    // The answer depends on Accept, and on User-Agent when Accept states no preference.
    const vary = preference.byUserAgent ? "Accept, User-Agent" : "Accept";

    if (preference.markdown) {
      const markdown = await env.ASSETS.fetch(
        new Request(new URL(markdownPath, request.url), request),
      );
      // 304: the client's cached Markdown copy is still current.
      if (markdown.ok || markdown.status === 304) {
        return withHeaders(markdown, {
          "content-type": MARKDOWN_CONTENT_TYPE,
          vary,
          link: `<${SITE_ORIGIN}${markdownPath.replace(/\/index\.md$|\.md$/, "/")}>; rel="canonical"`,
        });
      }
      if (markdown.status === 404) {
        return new Response(MISSING_PAGE_MARKDOWN, {
          status: 404,
          headers: { "content-type": MARKDOWN_CONTENT_TYPE, vary },
        });
      }
    }

    const page = await env.ASSETS.fetch(request);
    if (!page.ok && page.status !== 304) return page;
    return withHeaders(page, {
      vary,
      link: `<${markdownPath}>; rel="alternate"; type="text/markdown"`,
    });
  },
};
