// ============================================================================
// skill-attention - What needs the user, in one order of priority: Broken
// (an agent does not load the skill), then Warnings, then Updates. Home's
// groups and the Skills page's groups and filter read the same categories
// from here, so the two pages list the same skills in the same order.
// ============================================================================

import { HEALTH_ISSUE_KIND_ORDER, HEALTH_ISSUE_SEVERITY } from "./skill-health";
import type { HealthIssue } from "./skill-health";
import type { InstalledSkill } from "./skill-types";
import { hasUpdate } from "./skill-updates";

/** One thing that needs the user. `ATTENTION_ORDER` ranks them. */
export type AttentionCategory = "broken" | "warning" | "update";

/** Most serious first: a skill no agent loads outranks a warning, which outranks an update. */
export const ATTENTION_ORDER = [
  "broken",
  "warning",
  "update",
] as const satisfies readonly AttentionCategory[];

/** The Skills filter's choice: one category, or `"any"` for all three in `ATTENTION_ORDER`. */
export type AttentionFilter = AttentionCategory | "any";

/** The categories one skill is in. Each value is the skill's place inside that category: the
 * `HEALTH_ISSUE_KIND_ORDER` rank of its first issue there (0 for an update). Home lists issues in
 * that order, so sorting by it puts a category's skills in Home's order. */
export type SkillAttention = Partial<Record<AttentionCategory, number>>;

/** The category an issue counts in: an error means an agent does not load the skill. */
export function issueAttentionCategory(issue: HealthIssue): "broken" | "warning" {
  return HEALTH_ISSUE_SEVERITY[issue.kind] === "error" ? "broken" : "warning";
}

/** Each skill's categories, keyed by skill name. `issues` is `collectDashboardIssues(skills)`, the
 * list Home's Broken and Warnings groups show; updates come from the background update check. */
export function attentionBySkill(
  skills: readonly InstalledSkill[],
  issues: readonly HealthIssue[],
): Map<string, SkillAttention> {
  const bySkill = new Map<string, SkillAttention>();
  const entry = (name: string) => {
    let found = bySkill.get(name);
    if (!found) bySkill.set(name, (found = {}));
    return found;
  };
  for (const issue of issues) {
    const category = issueAttentionCategory(issue);
    const rank = HEALTH_ISSUE_KIND_ORDER.indexOf(issue.kind);
    const attention = entry(issue.skill.name);
    attention[category] = Math.min(attention[category] ?? rank, rank);
  }
  for (const skill of skills) if (hasUpdate(skill)) entry(skill.name).update = 0;
  return bySkill;
}

/** The category a skill shows under: the one the filter picked, or with no single pick its most
 * serious one. `null` when the skill needs nothing the filter covers. */
export function attentionCategoryOf(
  attention: SkillAttention | undefined,
  filter?: AttentionFilter,
): AttentionCategory | null {
  if (!attention) return null;
  if (filter && filter !== "any") return attention[filter] === undefined ? null : filter;
  return ATTENTION_ORDER.find((category) => attention[category] !== undefined) ?? null;
}

/** How many skills each attention pick lists. */
export interface AttentionCounts {
  broken: number;
  warning: number;
  update: number;
  /** Skills in at least one category. */
  any: number;
}

/** How many skills each category holds. A skill counts in every category it is in, as on Home. */
export function attentionCounts(
  bySkill: ReadonlyMap<string, SkillAttention>,
  skills: readonly InstalledSkill[],
): AttentionCounts {
  const counts: AttentionCounts = { broken: 0, warning: 0, update: 0, any: 0 };
  for (const skill of skills) {
    const attention = bySkill.get(skill.name);
    if (!attention || attentionCategoryOf(attention) === null) continue;
    counts.any += 1;
    for (const category of ATTENTION_ORDER)
      if (attention[category] !== undefined) counts[category] += 1;
  }
  return counts;
}
