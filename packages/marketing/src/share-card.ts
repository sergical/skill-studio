// What the share card (public/og.jpg) shows, read from the homepage hero and its skill pile.
// scripts/render-share-card.ts renders the card and records this next to it, and
// share-card.test.ts fails when the hero changes without a new render.
import { HERO_TITLE } from "./home/listing-calc/hero-copy";
import { ACTIVE_PILE_CHIPS, PILE, PILE_CLEANUP } from "./home/listing-calc/pile/skill-pile-data";
import { SITE_URL } from "./site-links";

// The card keeps the centre for the headline, so the sorted side shows only a few skills.
const SORTED_SKILLS_SHOWN = 6;
const PILE_SKILLS_SHOWN = 96;

export const SHARE_CARD_CONTENT = {
  title: HERO_TITLE,
  host: new URL(SITE_URL).host,
  pile: PILE.slice(0, PILE_SKILLS_SHOWN).map(({ name, folder, status, inUse }) => ({
    name,
    folder,
    status,
    inUse,
  })),
  sorted: ACTIVE_PILE_CHIPS.slice(0, SORTED_SKILLS_SHOWN).map((chip) => chip.name),
  cleanup: PILE_CLEANUP,
};

export const SHARE_CARD_ALT = `Skill Studio: ${HERO_TITLE} The app icon and headline between a messy pile of skills and a short, tidy list, with counts of the skills it parked, fixed and updated.`;
