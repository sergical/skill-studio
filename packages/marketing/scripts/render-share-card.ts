// Renders the share card from the OgCard still and records what it showed, so
// src/share-card.test.ts can tell when the card is older than the hero.
// public/og.jpg is the card the pages point at: as a JPEG it stays under the 300 KB
// budget share-card.test.ts checks. public/og.png keeps links shared before the switch working.
// Run from packages/marketing after formatting: bun scripts/render-share-card.ts
import { execFileSync } from "node:child_process";
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { SHARE_CARD_CONTENT } from "../src/share-card";
import { hashShareCardFiles } from "../src/share-card-files";

const STILL = ["remotion", "still", "src/remotion/index.ts", "OgCard"];
const FORMATS = [
  ["public/og.jpg", "--image-format=jpeg", "--jpeg-quality=92"],
  ["public/og.png", "--image-format=png"],
];

for (const [output, ...flags] of FORMATS) {
  execFileSync("npx", [...STILL, output, ...flags, "--log=error", "--overwrite"], {
    cwd: fileURLToPath(new URL("../", import.meta.url)),
    stdio: "inherit",
  });
}
writeFileSync(
  new URL("../src/share-card.rendered.json", import.meta.url),
  `${JSON.stringify({ content: SHARE_CARD_CONTENT, files: hashShareCardFiles() }, null, 2)}\n`,
);
