// ============================================================================
// Skill Studio - skill-source-line
// The "by owner/repo · 12.3k installs" line under an installed skill's title.
// Numbers only: no "official" or "verified" claim.
// ============================================================================

import type { InstalledSkill } from "@skill-studio/lib";
import { formatInstalls } from "../../lib/skill-installs-format";
import { sourceLedgerLabel } from "./installed-skill-source-ledger-model";

export interface SkillSourceLine {
  /** Text before the source label, e.g. "by "; empty for non-skills.sh sources. */
  prefix: string;
  label: string;
  /** Where the label links; null when the source has no page. */
  href: string | null;
  /** "12.3k installs", or null when the count is unknown. */
  installs: string | null;
}

/** "1 install" / "12.3k installs". */
function installsLabel(count: number): string {
  return `${formatInstalls(count)} ${count === 1 ? "install" : "installs"}`;
}

/**
 * skills.sh skills show who made them and how many installs skills.sh counts;
 * every other source (plugin, fork, in-repo, manual) shows its existing source
 * label with no count. `installs` is the cached skills.sh count, if any.
 */
export function skillSourceLine(
  skill: InstalledSkill,
  installs: number | null | undefined,
): SkillSourceLine {
  if (skill.source_kind !== "skills-sh" || !skill.source.includes("/")) {
    return { prefix: "", label: sourceLedgerLabel(skill), href: null, installs: null };
  }
  return {
    prefix: "by ",
    label: skill.source,
    href: skill.source_url ?? `https://github.com/${skill.source}`,
    installs: installs == null ? null : installsLabel(installs),
  };
}
