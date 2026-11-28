// The share card and the files that decide how it looks. scripts/render-share-card.ts
// records their hashes after a render, so a new logo, a layout edit, or a card left out
// of a commit fails share-card.test.ts.
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";

const PACKAGE_ROOT = new URL("../", import.meta.url);
const SHARE_CARD_FILES = [
  "public/og.jpg",
  "public/og.png",
  "public/skill-studio-logo-512.webp",
  "src/remotion/OgCard.tsx",
  "src/home/dither-colors.ts",
];

export function hashShareCardFiles(): Record<string, string> {
  return Object.fromEntries(
    SHARE_CARD_FILES.map((file) => [
      file,
      createHash("sha256")
        .update(readFileSync(new URL(file, PACKAGE_ROOT)))
        .digest("hex"),
    ]),
  );
}
