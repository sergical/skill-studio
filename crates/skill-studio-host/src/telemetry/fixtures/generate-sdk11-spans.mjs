import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const require = createRequire(new URL("../../../../../apps/desktop/package.json", import.meta.url));
const sdkRequire = createRequire(require.resolve("@sentry/react"));
const root = dirname(sdkRequire.resolve("@sentry/core/package.json"));
if (require(join(root, "package.json")).version !== "11.11.0") throw new Error("Use the pinned SDK 11.11.0");
const { createStreamedSpanEnvelope } = await import(pathToFileURL(join(root, "build/esm/tracing/spans/envelope.js")));
const { SentrySpan } = await import(pathToFileURL(join(root, "build/esm/tracing/sentrySpan.js")));
const { streamedSpanJsonToSerializedSpan } = await import(pathToFileURL(join(root, "build/esm/utils/spanUtils.js")));
const browserRequire = createRequire(sdkRequire.resolve("@sentry/browser"));
const utilsRequire = createRequire(browserRequire.resolve("@sentry/browser-utils"));
const op = await import(pathToFileURL(utilsRequire.resolve("@sentry/conventions/op")));
// Each op with the origin the SDK's own integration gives it.
const opsAndOrigins = [
  [op.NAVIGATION, "auto.navigation.browser"],
  ["ipc", "manual"],
  [op.UI_LONG_TASK, "auto.ui.browser.metrics"],
  [op.UI_INTERACTION_CLICK, "auto.ui.browser.inp"],
  [op.PAGELOAD, "auto.pageload.browser"],
  [op.UI_LONG_ANIMATION_FRAME, "auto.ui.browser.metrics"],
  [op.UI_INTERACTION_PRESS, "auto.ui.browser.inp"],
  [op.UI_INTERACTION_DRAG, "auto.ui.browser.inp"],
  [op.UI_INTERACTION_HOVER, "auto.ui.browser.inp"],
  [op.UI_WEBVITAL_LCP, "auto.ui.browser.lcp"],
  [op.UI_WEBVITAL_CLS, "auto.ui.browser.cls"],
];
const spans = opsAndOrigins.map(([op, origin], index) => {
  const span = new SentrySpan({
    traceId: "0123456789abcdef0123456789abcdef",
    spanId: `0123456789abcd${index.toString(16).padStart(2, "0")}`,
    parentSpanId: "fedcba9876543210",
    name: op === "ipc" ? "scan_skills" : "view.home",
    op,
    startTimestamp: 1700000000 + index,
    attributes: {
      "sentry.segment.name": "view.home",
      "sentry.origin": origin,
    },
  });
  if (index === 3) span.setAttribute("browser.web_vital.inp.value", 100);
  span.end(1700000000.1 + index);
  return streamedSpanJsonToSerializedSpan(span.getSpanJSON());
});
const envelope = createStreamedSpanEnvelope(spans, {}, {
  getOptions: () => ({}), getDsn: () => undefined,
  getDataCollectionOptions: () => ({ userInfo: false }),
});
writeFileSync(new URL("sdk11-spans.json", import.meta.url), `${JSON.stringify(envelope, null, 2)}\n`);
