import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { z } from "zod";

const dist = process.argv[2];
const assets = join(dist, "assets");
const manifest = join(dist, "webview-bundle.json");
const debugIds = {};
for (const file of readdirSync(assets).sort()) {
  if (!file.endsWith(".js.map")) continue;
  const asset = file.slice(0, -4);
  const map = JSON.parse(readFileSync(join(assets, file), "utf8"));
  const script = readFileSync(join(assets, asset), "utf8");
  const id = z.uuid().parse(map.debugId ?? map.debug_id);
  if (!script.includes(`//# debugId=${id}`)) {
    throw new Error(`WebView source map and bundle debug IDs differ for ${asset}`);
  }
  debugIds[asset] = id;
}
if (Object.keys(debugIds).length === 0) {
  throw new Error("WebView build produced no JavaScript source maps");
}
const bundle = JSON.parse(readFileSync(manifest, "utf8"));
writeFileSync(manifest, JSON.stringify({ ...bundle, debugIds }));
