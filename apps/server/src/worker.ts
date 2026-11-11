// ============================================================================
// Skill Studio - Worker
// The Cloudflare Workers entry point for the skills.sh proxy: this URL is
// public (unlike the Node dev server, which only ever binds 127.0.0.1), so it
// wires the Workers Rate Limiting binding and the Cache API into
// `createSkillsProxyApp` before every request reaches skills.sh.
// ============================================================================

import * as Sentry from "@sentry/cloudflare";

import {
  createSkillsProxyApp,
  methodLabel,
  PROXY_ROUTE_TEMPLATES,
  proxyRouteFor,
  type RateLimiter,
  type ResponseCache,
} from "./skills-proxy-app";
import { scrubSentryEvent, scrubSentrySpan } from "./sentry-event-scrub";

import type { CloudflareOptions } from "@sentry/cloudflare";

/** The subset of Workers' `Fetcher.env` this proxy reads: the skills.sh key
 * (set once with `wrangler secret put SKILLS_SH_API_KEY`), the rate limit
 * binding declared in `wrangler.jsonc`, and the optional Sentry DSN (`wrangler
 * secret put SENTRY_DSN`). */
export interface Env {
  SKILLS_SH_API_KEY: string;
  RATE_LIMITER: RateLimiter;
  SENTRY_DSN?: string;
}

/** The Workers fetch handler's third argument - its `waitUntil` schedules the
 * Cache API write past the response, so a cache write never adds to the
 * caller's latency. Named narrowly instead of pulling in
 * `@cloudflare/workers-types` for one method. */
interface ExecutionContext {
  waitUntil(promise: Promise<unknown>): void;
  passThroughOnException(): void;
}

// `caches.default` is a Workers-only global (the edge Cache API) with no
// Node equivalent, so it isn't part of this project's `lib: ["ES2020"]`
// tsconfig - declared narrowly here instead of pulling in the full
// `@cloudflare/workers-types` package just for one global.
declare const caches: { default: ResponseCache };

/** The SDK names a streamed request span by its raw method only, so every
 * route would group as one `GET` and any invented method would open a new
 * group. The template keeps skill names out of the name. */
function nameRequestSpanByRoute(request: Request): void {
  const route = proxyRouteFor(new URL(request.url).pathname);
  const span = Sentry.getActiveSpan();
  if (!span) return;
  const path = route === "other" ? "other" : PROXY_ROUTE_TEMPLATES[route];
  Sentry.updateSpanName(Sentry.getRootSpan(span), `${methodLabel(request.method)} ${path}`);
}

/** A caller's `sentry-trace` and `baggage` would make the SDK continue their
 * trace and copy their baggage, such as a made-up transaction name, into the
 * envelope header, where `beforeSendSpan` cannot reach it. */
function withoutCallerTrace(request: Request): Request {
  if (!request.headers.has("sentry-trace") && !request.headers.has("baggage")) return request;
  const headers = new Headers(request.headers);
  headers.delete("sentry-trace");
  headers.delete("baggage");
  return new Request(request, { headers });
}

const handler = {
  fetch(request: Request, env: Env, ctx: ExecutionContext): Response | Promise<Response> {
    nameRequestSpanByRoute(request);
    const app = createSkillsProxyApp({
      apiKey: env.SKILLS_SH_API_KEY,
      limiter: env.RATE_LIMITER,
      cache: caches.default,
      waitUntil: (promise) => ctx.waitUntil(promise),
      reportServerError: (error, { kind }) =>
        Sentry.captureException(error, { tags: { error_kind: kind } }),
    });
    return app.fetch(request);
  },
};

/** Without `SENTRY_DSN` the SDK still initializes on every request but has no
 * transport, so it captures locally and sends nothing.
 *
 * Every request is traced; the proxy serves about a thousand requests a month.
 * SDK v11 streams spans, so `beforeSendTransaction` never runs. `dataCollection`
 * keeps the search text, client IP, cookies, headers and bodies off spans and
 * events at the source, and `beforeSendSpan` drops the two request details it
 * does not cover. `tracePropagationTargets: []` keeps `sentry-trace` and
 * `baggage` headers off the outgoing skills.sh requests. */
function sentryOptionsFor(env: Env): CloudflareOptions {
  return {
    dsn: env.SENTRY_DSN,
    environment: "production",
    tracesSampleRate: 1,
    dataCollection: {
      userInfo: false,
      cookies: false,
      httpHeaders: false,
      httpBodies: [],
      urlQueryParams: false,
    },
    beforeSend: scrubSentryEvent,
    beforeSendSpan: scrubSentrySpan,
    tracePropagationTargets: [],
  };
}

const sentryHandler = Sentry.withSentry(sentryOptionsFor, handler);

export default {
  fetch: (request: Request, env: Env, ctx: ExecutionContext) =>
    sentryHandler.fetch(withoutCallerTrace(request), env, ctx),
};
