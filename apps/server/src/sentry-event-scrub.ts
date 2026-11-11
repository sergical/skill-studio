// ============================================================================
// Skill Studio - Sentry Event Scrub
// The Worker's Sentry `beforeSend` and `beforeSendSpan` hooks: this proxy is public, so an event it
// sends must never leak who called it or what they asked for.
// ============================================================================

import { methodLabel } from "./skills-proxy-app";

import type { CloudflareOptions, ErrorEvent } from "@sentry/cloudflare";

/** Strips request headers, cookies, query strings, user data, breadcrumbs, and the
 *  culture (timezone) context from a Sentry event before it leaves the Worker, so a
 *  caller's IP (cf-connecting-ip), Authorization header, or search text never reach
 *  Sentry - breadcrumbs and the exception message get the same query-string scrub as
 *  `request.url`, since the SDK's fetch instrumentation records the upstream skills.sh
 *  URL (query included) as a breadcrumb before the rejection reaches the proxy's catch. */
export function scrubSentryEvent<E extends ErrorEvent>(event: E): E {
  delete event.user;
  delete event.breadcrumbs;
  if (event.contexts) delete event.contexts.culture;
  if (event.request) {
    delete event.request.headers;
    delete event.request.cookies;
    delete event.request.query_string;
    delete event.request.data;
    if (event.request.url) {
      event.request.url = event.request.url.split("?")[0];
    }
  }
  for (const value of event.exception?.values ?? []) {
    if (value.value) {
      value.value = value.value.replace(/\?\S*/g, "");
    }
  }
  return event;
}

type StreamedSpan = Parameters<NonNullable<CloudflareOptions["beforeSendSpan"]>>[0];

const UNSCRUBBED_SPAN_ATTRIBUTES = [
  "user_agent.original",
  "culture.timezone",
  "culture.locale",
  "http.request.body.size",
  "http.response.body.size",
];

/** Drops the request details `dataCollection` cannot turn off: the caller's
 * user agent, the timezone Cloudflare derives from their location, and the
 * `Content-Length` header values the SDK copies into body sizes. A made-up
 * request method is reported as `OTHER`. */
export function scrubSentrySpan(span: StreamedSpan): StreamedSpan {
  for (const key of UNSCRUBBED_SPAN_ATTRIBUTES) delete span.attributes[key];
  const method = span.attributes["http.request.method"];
  if (method !== undefined) span.attributes["http.request.method"] = methodLabel(String(method));
  return span;
}
