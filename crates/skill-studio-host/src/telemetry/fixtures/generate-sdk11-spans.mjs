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
const spans = ["navigation", "ipc", "ui.long-task", "ui.interaction.click"].map((op, index) => {
  const span = new SentrySpan({
    traceId: "0123456789abcdef0123456789abcdef",
    spanId: `0123456789abcde${index}`,
    parentSpanId: "fedcba9876543210",
    name: op === "ipc" ? "scan_skills" : "view.home",
    op,
    startTimestamp: 1700000000 + index,
    attributes: {
      "sentry.segment.name": "view.home",
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
