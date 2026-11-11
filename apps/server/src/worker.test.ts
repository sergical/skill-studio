// ============================================================================
// Skill Studio - Worker tests
// Runs the deployed Worker export and records what its Sentry transport posts
// to the ingest URL, so a test reads exactly what would reach Sentry: request
// spans must arrive, and no caller detail (search text, IP, user agent,
// cookie, timezone) or the skills.sh key may be in them.
// ============================================================================

import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import worker from "./worker";

import type { Env } from "./worker";

const SEARCH_TEXT = "private-search-text";
const CALLER_IP = "203.0.113.7";
const CALLER_AGENT = "CallerAgent/9.9";
const CALLER_COOKIE = "session=caller-cookie";
const CALLER_TIMEZONE = "Pacific/Chatham";
const API_KEY = "skills-sh-test-key";
const CALLER_DETAILS = [SEARCH_TEXT, CALLER_IP, CALLER_AGENT, CALLER_COOKIE, CALLER_TIMEZONE];

const sentToSentry: string[] = [];
const upstreamCalls: Request[] = [];
let upstreamStatus = 200;

async function fakeFetch(input: string | URL | Request, init?: RequestInit): Promise<Response> {
  const request = new Request(input, init);
  if (request.url.includes("ingest.sentry.io")) {
    sentToSentry.push(await request.text());
    return new Response(null, { status: 200 });
  }
  upstreamCalls.push(request);
  return Response.json({ skills: [] }, { status: upstreamStatus });
}

async function callWorker(): Promise<string> {
  const request = new Request(`https://api.example.test/api/v1/skills/search?q=${SEARCH_TEXT}`, {
    headers: {
      "cf-connecting-ip": CALLER_IP,
      "x-forwarded-for": CALLER_IP,
      "user-agent": CALLER_AGENT,
      cookie: CALLER_COOKIE,
    },
  });
  Object.defineProperty(request, "cf", {
    value: { timezone: CALLER_TIMEZONE, httpProtocol: "HTTP/2" },
  });
  const pending: Promise<unknown>[] = [];
  const env: Env = {
    SKILLS_SH_API_KEY: API_KEY,
    RATE_LIMITER: { limit: async () => ({ success: true }) },
    SENTRY_DSN: "https://examplePublicKey@o0.ingest.sentry.io/0",
  };
  const ctx = {
    waitUntil: (promise: Promise<unknown>) => pending.push(promise),
    passThroughOnException: () => {},
  };

  await worker.fetch(request, env, ctx);
  for (let settled = 0; settled < pending.length; settled = pending.length) {
    await Promise.all(pending);
  }
  return sentToSentry.join("\n");
}

beforeAll(() => {
  vi.stubGlobal("fetch", fakeFetch);
  vi.stubGlobal("caches", {
    default: { match: async () => undefined, put: async () => {} },
  });
});

beforeEach(() => {
  sentToSentry.length = 0;
  upstreamCalls.length = 0;
  upstreamStatus = 200;
});

afterAll(() => {
  vi.unstubAllGlobals();
});

describe("Worker tracing", () => {
  it("sends the search request as a span named by its path, so traces reach Sentry", async () => {
    const payload = await callWorker();

    expect(payload).toContain("GET /api/v1/skills/search");
  });

  it("keeps the caller's search text, IP, user agent, cookie and timezone out of every span", async () => {
    const payload = await callWorker();

    for (const detail of CALLER_DETAILS) {
      expect(payload).not.toContain(detail);
    }
  });

  it("keeps the caller's details and the skills.sh key out of the error a 5xx upstream reports", async () => {
    upstreamStatus = 503;
    const payload = await callWorker();

    expect(payload).toContain("skills.sh answered 503");
    for (const detail of [...CALLER_DETAILS, API_KEY]) {
      expect(payload).not.toContain(detail);
    }
  });

  it("adds no sentry-trace or baggage header to the skills.sh request", async () => {
    await callWorker();

    expect(upstreamCalls).toHaveLength(1);
    expect(upstreamCalls[0].headers.has("sentry-trace")).toBe(false);
    expect(upstreamCalls[0].headers.has("baggage")).toBe(false);
  });
});
