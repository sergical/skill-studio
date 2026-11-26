// What the share card (public/og.png) shows, read from the homepage hero.
// scripts/render-share-card.ts renders the card and records this next to it, and
// share-card.test.ts fails when the hero changes without a new render.
import { HERO_SUBLINE, HERO_TITLE } from "./home/listing-calc/hero-copy";
import { ACTIVE_PILE_CHIPS, PILE_CLEANUP } from "./home/listing-calc/pile/skill-pile-data";
import { SITE_URL, TRUST_LINE } from "./site-links";

export const SHARE_CARD_CONTENT = {
  title: HERO_TITLE,
  subline: HERO_SUBLINE,
  host: new URL(SITE_URL).host,
  surfaces: `Mac app · CLI · MCP server · ${TRUST_LINE}`,
  skills: ACTIVE_PILE_CHIPS.map((chip) => chip.name),
  cleanup: PILE_CLEANUP,
};

export const SHARE_CARD_ALT = `Skill Studio: ${HERO_TITLE} The app icon beside a sorted list of skills, with counts of the ones it parked, fixed and updated.`;
