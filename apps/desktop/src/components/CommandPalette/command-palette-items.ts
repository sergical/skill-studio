// ============================================================================
// command-palette-items - Builds and ranks the palette's flat item list:
// Needs attention (empty query only), Actions, Go to (places), and every
// installed skill. No fuzzy-match library - matching is case-insensitive
// substring/word-start/prefix, the same tier order a user's own typing
// intuition expects.
// ============================================================================

import { ATTENTION_ANY_LABEL } from "@skill-studio/lib";
import type { ReactNode } from "react";
import type { ShortcutDef } from "../../lib/app-shortcuts";
import type { InstalledSkill } from "@skill-studio/lib";

export type PaletteSection = "attention" | "actions" | "goto" | "skills";

export interface PaletteItem {
  id: string;
  section: PaletteSection;
  label: string;
  /** Secondary text after the label - a skill's state group ("Broken", "Parked"). */
  secondary?: string;
  icon?: ReactNode;
  shortcut?: ShortcutDef;
  /** Listed only once the person types: an action whose own keys are faster than the palette. */
  typedOnly?: boolean;
  run: () => void;
}

export const SECTION_LABEL = {
  attention: ATTENTION_ANY_LABEL,
  actions: "Actions",
  goto: "Go to",
  skills: "Skills",
} satisfies Record<PaletteSection, string>;

/** One item's match tier for a query - lower sorts first. `undefined` means no match. */
function matchTier(label: string, query: string): number | undefined {
  // Spaces, slashes, underscores, and hyphens are one word break, so "i have" finds `i-have-adhd`.
  const haystack = label.toLowerCase().replace(/[\s/_-]+/g, " ");
  const normalized = query.replace(/[\s/_-]+/g, " ");
  if (haystack.startsWith(normalized)) return 0;
  if (` ${haystack}`.includes(` ${normalized}`)) return 1;
  if (haystack.includes(normalized)) return 2;
  if (normalized.split(" ").every((word) => haystack.includes(word))) return 3;
  return undefined;
}

/**
 * The items to list for a query, in the order they are drawn and arrowed through. An empty query
 * lists what needs attention, the actions and the places; typing searches actions, places and
 * every skill. Matches rank by tier, then alphabetically, then by a stable index so equal-ranked
 * items don't reorder between keystrokes; each section then follows its best match, so the
 * first row drawn is the best match and arrow keys move down the list as drawn.
 */
export function rankItems(items: PaletteItem[], query: string): PaletteItem[] {
  const trimmed = query.trim().toLowerCase();
  if (trimmed === "") return items.filter((item) => item.section !== "skills" && !item.typedOnly);
  const ranked = items
    .filter((item) => item.section !== "attention")
    .map((item, index) => ({ item, index, tier: matchTier(item.label, trimmed) }))
    .filter(
      (entry): entry is { item: PaletteItem; index: number; tier: number } =>
        entry.tier !== undefined,
    )
    .sort(
      (a, b) => a.tier - b.tier || a.item.label.localeCompare(b.item.label) || a.index - b.index,
    )
    .map((entry) => entry.item);
  const sectionOrder = [...new Set(ranked.map((item) => item.section))];
  return sectionOrder.flatMap((section) => ranked.filter((item) => item.section === section));
}

/** One place in "Go to". `run` opens it the way the sidebar does. */
export interface GotoDef {
  id: string;
  label: string;
  icon: ReactNode;
  run: () => void;
}

/** Builds the "Go to" items from the app's places - the caller supplies which ones apply
 * (e.g. Plugins only when there are any, as in the sidebar). */
export function gotoItems(defs: GotoDef[]): PaletteItem[] {
  return defs.map((def) => ({ ...def, id: `goto-${def.id}`, section: "goto" }));
}

/** Builds one item per skill in `section`. A skill that needs attention or is parked shows its
 * state group label (e.g. "Broken"); a healthy skill shows none, since most skills are healthy. */
export function skillItems(
  skills: InstalledSkill[],
  section: "attention" | "skills",
  groupLabelFor: (skill: InstalledSkill) => string | undefined,
  glyphFor: (skill: InstalledSkill) => ReactNode,
  onOpen: (skill: InstalledSkill) => void,
): PaletteItem[] {
  return skills.map((skill) => ({
    id: `${section}-${skill.name}`,
    section,
    label: skill.name,
    secondary: groupLabelFor(skill),
    icon: glyphFor(skill),
    run: () => onOpen(skill),
  }));
}
