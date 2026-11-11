// ============================================================================
// Skill Studio - Site Sentry
// Real-user monitoring for the marketing site. The SDK loads in its own chunk
// after the first render, and only when the build has a DSN. The prerender
// must never import this file.
// ============================================================================

import type * as Sentry from "@sentry/browser";

export type SiteSentrySdk = Pick<
  typeof Sentry,
  | "init"
  | "captureException"
  | "browserTracingIntegration"
  | "replayIntegration"
  | "makeFetchTransport"
>;

type SentryTransport = ReturnType<SiteSentrySdk["makeFetchTransport"]>;
type SentryEnvelope = Parameters<SentryTransport["send"]>[0];
type SentryEnvelopeItem = SentryEnvelope[1][number];

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

const MAX_EARLY_ERRORS = 20;

// A JSON string value that starts with a URL or a path, cut at its first `?` or `#`.
const URL_QUERY_OR_FRAGMENT_IN_JSON =
  /((?:"|\s)(?:[a-z][a-z0-9+.-]*:\/\/|\/)[^"\s?#\\]*)[?#](?:[^"\s\\]|\\.)*/gi;

export function siteSentryEnvironment({ branch, mode }: Pick<SiteSentryEnv, "branch" | "mode">) {
  if (branch === "") return mode;
  return branch === "main" ? "production" : "preview";
}

function withoutUrlQueries(json: string): string {
  return json.replace(URL_QUERY_OR_FRAGMENT_IN_JSON, "$1");
}

function scrubEnvelopeItem(item: SentryEnvelopeItem): SentryEnvelopeItem {
  const [header, payload] = item;
  if (payload instanceof Uint8Array) return item;
  if (header.type === "replay_recording") {
    const recording = withoutUrlQueries(String(payload));
    const length = new TextEncoder().encode(recording).length;
    // SAFETY: the header and payload keep the replay_recording shape; only the text and its byte length change.
    return [{ ...header, length }, recording] as SentryEnvelopeItem;
  }
  // SAFETY: the payload is the SDK's own JSON; a string replace inside its values keeps the shape.
  return [header, JSON.parse(withoutUrlQueries(JSON.stringify(payload)))] as SentryEnvelopeItem;
}

// URLs can carry visitor data in the query or the fragment. The SDK's own filter keeps the
// fragment, and Replay sends the full page URL, so every envelope is scrubbed here.
export function scrubSentryEnvelope(envelope: SentryEnvelope): SentryEnvelope {
  const [headers, items] = envelope;
  // SAFETY: the headers are the SDK's own JSON; a string replace inside its values keeps the shape.
  const scrubbedHeaders = JSON.parse(withoutUrlQueries(JSON.stringify(headers))) as typeof headers;
  // SAFETY: each item keeps its own header and payload pairing.
  return [scrubbedHeaders, items.map(scrubEnvelopeItem)] as SentryEnvelope;
}

export function siteSentryInitOptions(env: SiteSentryEnv, sdk: SiteSentrySdk) {
  return {
    dsn: env.dsn,
    release: env.release || undefined,
    environment: siteSentryEnvironment(env),
    sendDefaultPii: false,
    dataCollection: {
      userInfo: false,
      cookies: false,
      httpHeaders: false,
      httpBodies: [],
      urlQueryParams: false,
    },
    transport: (options: Parameters<SiteSentrySdk["makeFetchTransport"]>[0]): SentryTransport => {
      const transport = sdk.makeFetchTransport(options);
      return { ...transport, send: (envelope) => transport.send(scrubSentryEnvelope(envelope)) };
    },
    tracesSampleRate: TRACES_SAMPLE_RATE,
    // The site calls no other origin, so no trace headers go anywhere.
    tracePropagationTargets: [],
    replaysSessionSampleRate: REPLAYS_SESSION_SAMPLE_RATE,
    replaysOnErrorSampleRate: REPLAYS_ON_ERROR_SAMPLE_RATE,
    integrations: [
      sdk.browserTracingIntegration(),
      sdk.replayIntegration({
        maskAllText: true,
        maskAllInputs: true,
        blockAllMedia: true,
        // The envelope scrub must read the recording text.
        useCompression: false,
      }),
    ],
  };
}

interface EarlyError {
  exception: ErrorEvent["error"];
  mechanism: string;
}

export interface EarlyErrors {
  stop(): EarlyError[];
}

// Errors thrown before the SDK chunk loads would be lost; this keeps them for the SDK.
export function bufferEarlyErrors(
  target: Pick<Window, "addEventListener" | "removeEventListener"> = window,
): EarlyErrors {
  const errors: EarlyError[] = [];
  const keep = (error: EarlyError) => {
    if (errors.length < MAX_EARLY_ERRORS) errors.push(error);
  };
  const onError = (event: ErrorEvent) =>
    keep({
      exception: event.error ?? event.message,
      mechanism: "auto.browser.global_handlers.onerror",
    });
  const onRejection = (event: PromiseRejectionEvent) =>
    keep({
      exception: event.reason,
      mechanism: "auto.browser.global_handlers.onunhandledrejection",
    });
  target.addEventListener("error", onError);
  target.addEventListener("unhandledrejection", onRejection);
  return {
    stop() {
      target.removeEventListener("error", onError);
      target.removeEventListener("unhandledrejection", onRejection);
      return errors.splice(0);
    },
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
  earlyErrors: EarlyErrors,
  env: SiteSentryEnv = readSiteSentryEnv(),
  loadSdk: () => Promise<SiteSentrySdk> = () => import("@sentry/browser"),
): Promise<void> {
  if (!env.dsn?.trim()) {
    earlyErrors.stop();
    return;
  }
  try {
    const sdk = await loadSdk();
    sdk.init(siteSentryInitOptions(env, sdk));
    for (const { exception, mechanism } of earlyErrors.stop()) {
      sdk.captureException(exception, { mechanism: { type: mechanism, handled: false } });
    }
  } catch {
    // Monitoring must never break the page, for example when an ad blocker stops the chunk.
    earlyErrors.stop();
  }
}
