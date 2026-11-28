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
const SHARE_IMAGE_URL = "https://useskillstudio.com/og.jpg";
const SHARE_IMAGE_TAG = /<meta (?:property|name)="((?:og|twitter):image)" content="([^"]+)"/g;

// The 404 page is noindex and has no share tags on purpose.
it.each(SITE_ROUTES.filter((route) => route.file !== "404.html"))(
  "points $file at the share card",
  (route) => {
    const template = readFileSync(
      fileURLToPath(new URL(`../${route.template}`, import.meta.url)),
      "utf8",
    );
    const html = renderSitePage(route.file, template);

    expect(
      Object.fromEntries([...html.matchAll(SHARE_IMAGE_TAG)].map(([, name, url]) => [name, url])),
    ).toEqual({
      "og:image": SHARE_IMAGE_URL,
      "twitter:image": SHARE_IMAGE_URL,
    });
  },
);

it("keeps the share card under the size budget", () => {
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
