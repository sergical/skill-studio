// ============================================================================
// listing-words - The words every page uses for what an agent sees of a
// skill, and for what needs the user. One map each, so Home, Skills, Agents,
// Sessions and the skill page never name the same state two ways. The
// glossary is in DESIGN.md.
// ============================================================================

import type { AttentionCategory } from "./skill-attention";
import type { ListingState, SessionSummary } from "./skill-types";

/** What the model sees of one skill in an agent's skill list. */
export const LISTING_STATE_LABEL = {
  described: "Full description",
  truncated: "Shortened",
  name_only: "Name only",
  omitted: "Not listed",
  hidden: "Hidden",
} satisfies Record<ListingState, string>;

type SessionListedAs = SessionSummary["used"][number]["listed_as"];

/** The same states as a session recorded them, plus the two a session can add. */
export const SESSION_LISTED_LABEL = {
  described: LISTING_STATE_LABEL.described,
  truncated: LISTING_STATE_LABEL.truncated,
  name_only: LISTING_STATE_LABEL.name_only,
  not_listed: LISTING_STATE_LABEL.omitted,
  unknown: "Not recorded",
} satisfies Record<SessionListedAs, string>;

/** What needs the user, as Home's groups and the Skills page's groups and filter name it. */
export const ATTENTION_LABEL = {
  broken: "Broken",
  warning: "Warnings",
  update: "Updates",
} satisfies Record<AttentionCategory, string>;

/** All three attention categories at once, in their order of priority. */
export const ATTENTION_ANY_LABEL = "Needs attention";
