// ============================================================================
// Skill Studio - Share card test
// Flow: a hero change ships, and link previews show the card public/og.png.
// Failure: the homepage says one thing and the card another, because nobody
// rendered the card again.
// ============================================================================

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, it } from "vitest";

import { SHARE_CARD_CONTENT } from "./share-card";
import { hashShareCardFiles } from "./share-card-files";

it("ships a share card rendered from the current hero", () => {
  const rendered: unknown = JSON.parse(
    readFileSync(fileURLToPath(new URL("./share-card.rendered.json", import.meta.url)), "utf8"),
  );

  expect(rendered, "Run `bun scripts/render-share-card.ts` in packages/marketing").toEqual({
    content: SHARE_CARD_CONTENT,
    files: hashShareCardFiles(),
  });
});
