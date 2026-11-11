// ============================================================================
// Skill Studio - Site Sentry
// Real-user monitoring for the marketing site. The SDK loads in its own chunk
// after the first render, and only when the build has a DSN. The prerender
// must never import this file.
// ============================================================================

import type { BrowserOptions } from "@sentry/browser";

type SentryIntegration = Extract<BrowserOptions["integrations"], unknown[]>[number];

export interface SiteSentrySdk {
  init(options: BrowserOptions): void;
  browserTracingIntegration(): SentryIntegration;
  replayIntegration(options: {
    maskAllText: boolean;
    maskAllInputs: boolean;
    blockAllMedia: boolean;
  }): SentryIntegration;
}

export interface SiteSentryEnv {
  dsn: string | undefined;
  release: string;
  branch: string;
  mode: string;
}

// Traffic is a few hundred visits a month, so every page load is traced. That keeps
// Web Vitals complete. Replays are sampled to stay inside the free quota; any session
// that throws an error is always recorded.
const TRACES_SAMPLE_RATE = 1;
const REPLAYS_SESSION_SAMPLE_RATE = 0.1;
const REPLAYS_ON_ERROR_SAMPLE_RATE = 1;

export function siteSentryEnvironment({ branch, mode }: Pick<SiteSentryEnv, "branch" | "mode">) {
  if (branch === "") return mode;
  return branch === "main" ? "production" : "preview";
}

export function siteSentryInitOptions(env: SiteSentryEnv, sdk: SiteSentrySdk) {
  return {
    dsn: env.dsn,
    release: env.release || undefined,
    environment: siteSentryEnvironment(env),
    sendDefaultPii: false,
    tracesSampleRate: TRACES_SAMPLE_RATE,
    // The site calls no other origin, so no trace headers go anywhere.
    tracePropagationTargets: [],
    replaysSessionSampleRate: REPLAYS_SESSION_SAMPLE_RATE,
    replaysOnErrorSampleRate: REPLAYS_ON_ERROR_SAMPLE_RATE,
    integrations: [
      sdk.browserTracingIntegration(),
      sdk.replayIntegration({ maskAllText: true, maskAllInputs: true, blockAllMedia: true }),
    ],
  };
}

function readSiteSentryEnv(): SiteSentryEnv {
  return {
    dsn: import.meta.env.VITE_SENTRY_DSN,
    release: __SITE_RELEASE__,
    branch: __SITE_BRANCH__,
    mode: import.meta.env.MODE,
  };
}

export async function startSiteSentry(
  env: SiteSentryEnv = readSiteSentryEnv(),
  loadSdk: () => Promise<SiteSentrySdk> = () => import("@sentry/browser"),
): Promise<void> {
  if (!env.dsn?.trim()) return;
  try {
    const sdk = await loadSdk();
    sdk.init(siteSentryInitOptions(env, sdk));
  } catch {
    // Monitoring must never break the page, for example when an ad blocker stops the chunk.
  }
}
