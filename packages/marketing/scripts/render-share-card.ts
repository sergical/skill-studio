// Renders public/og.png from the OgCard still and records what it showed, so
// src/share-card.test.ts can tell when the card is older than the hero.
// Run from packages/marketing: bun scripts/render-share-card.ts
import { execFileSync } from "node:child_process";
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { SHARE_CARD_CONTENT } from "../src/share-card";

execFileSync(
  "npx",
  [
    "remotion",
    "still",
    "src/remotion/index.ts",
    "OgCard",
    "public/og.png",
    "--log=error",
    "--overwrite",
  ],
  { cwd: fileURLToPath(new URL("../", import.meta.url)), stdio: "inherit" },
);
writeFileSync(
  new URL("../src/share-card.rendered.json", import.meta.url),
  `${JSON.stringify(SHARE_CARD_CONTENT, null, 2)}\n`,
);
