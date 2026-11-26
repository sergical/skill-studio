// ============================================================================
// Skill Studio - Share card test
// Flow: a hero change ships, and link previews show the card public/og.jpg.
// Failure: the homepage says one thing and the card another, because nobody
// rendered the card again, or the pages point at a card too heavy to load in
// a chat preview.
// ============================================================================

import { readFileSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, it } from "vitest";

import { renderSitePage, SITE_ROUTES } from "./prerender/site-prerender";
import { SHARE_CARD_CONTENT } from "./share-card";
import { hashShareCardFiles } from "./share-card-files";

const SHARE_IMAGE_BUDGET_BYTES = 300_000;
const SHARE_IMAGE_TAG =
  /<meta (?:property|name)="(?:og|twitter):image" content="https:\/\/useskillstudio\.com\/([^"]+)"/g;

it("points every page at a share card under the size budget", () => {
  const images = new Set<string>();
  for (const route of SITE_ROUTES) {
    const template = readFileSync(
      fileURLToPath(new URL(`../${route.template}`, import.meta.url)),
      "utf8",
    );
    for (const [, path] of renderSitePage(route.file, template).matchAll(SHARE_IMAGE_TAG))
      images.add(path);
  }

  expect([...images]).toEqual(["og.jpg"]);
  const bytes = statSync(fileURLToPath(new URL("../public/og.jpg", import.meta.url))).size;
  expect(bytes).toBeLessThan(SHARE_IMAGE_BUDGET_BYTES);
});

it("ships a share card rendered from the current hero", () => {
  const rendered: unknown = JSON.parse(
    readFileSync(fileURLToPath(new URL("./share-card.rendered.json", import.meta.url)), "utf8"),
  );

  expect(rendered, "Run `bun scripts/render-share-card.ts` in packages/marketing").toEqual({
    content: SHARE_CARD_CONTENT,
    files: hashShareCardFiles(),
  });
});
