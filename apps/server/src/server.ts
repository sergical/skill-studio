// ============================================================================
// Skill Studio - Server
// The Node entry point (`pnpm run dev:server`) for the skills.sh proxy: builds
// `createSkillsProxyApp` with no rate limiter and no cache (local dev has no
// public abuse surface to guard against) and serves it with
// `@hono/node-server`. See `src/worker.ts` for the public Cloudflare Workers
// entry, and `src/skills-proxy-app.ts` for the shared app both build.
// ============================================================================

import { serve } from "@hono/node-server";

import { createSkillsProxyApp, isAllowedRawRequestTarget } from "./skills-proxy-app";

const DEFAULT_PORT = 8787;
const HOST = "127.0.0.1";

interface RawNodeRequestEnvironment {
  incoming: {
    url?: string;
  };
}

/** Reads and validates `SKILLS_SH_API_KEY` - the one thing this server
 * refuses to start without. Never logged. */
export function requireApiKey(env: NodeJS.ProcessEnv): string {
  const key = env.SKILLS_SH_API_KEY?.trim();
  if (!key) {
    throw new Error(
      "SKILLS_SH_API_KEY is not set. Add it to the repo-root .env, then run `pnpm run dev:server`.",
    );
  }
  return key;
}

/** Builds the Hono app for `apiKey`, with no rate limiter and no cache - kept
 * for tests that exercise routes directly without a real listener. */
export function createApp(apiKey: string) {
  return createSkillsProxyApp({ apiKey });
}

/** Creates the production Node fetch seam that rejects unsafe raw targets before Hono routing. */
export function createNodeRequestHandler(apiKey: string) {
  const app = createApp(apiKey);
  return (request: Request, env: RawNodeRequestEnvironment): Response | Promise<Response> => {
    if (!isAllowedRawRequestTarget(env.incoming.url)) {
      return Response.json({ error: "Invalid request path" }, { status: 400 });
    }
    return app.fetch(request, env);
  };
}

function main() {
  const apiKey = requireApiKey(process.env);
  const port = Number(process.env.PORT) || DEFAULT_PORT;
  serve({ fetch: createNodeRequestHandler(apiKey), port, hostname: HOST }, (info) => {
    process.stdout.write(`Skill Studio server listening on http://${HOST}:${info.port}\n`);
  });
}

// Only start the server when this file is run directly (`tsx src/server.ts`),
// not when a test imports its exports.
if (import.meta.url === `file://${process.argv[1]}`) {
  main();
}
