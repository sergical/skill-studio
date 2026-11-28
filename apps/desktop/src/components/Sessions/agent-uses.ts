// ============================================================================
// agent-uses - One agent's uses per skill in the last 30 days, from the same
// use history as Home and Activity.
// ============================================================================

import type { SkillSnapshot } from "@skill-studio/lib";

/** Uses per skill by `harness` in the last 30 days, most first; null when the history holds no
 * record from `harness`, which is no readable history for it, not zero uses. */
export function agentUses(snapshot: SkillSnapshot, harness: string) {
  const rows = snapshot.invocations
    .flatMap((stats) => {
      const uses = stats.by_harness_30_days[harness] ?? 0;
      return uses > 0 ? [{ skill: stats.skill, uses }] : [];
    })
    .sort((a, b) => b.uses - a.uses || a.skill.localeCompare(b.skill));
  return rows.length ? rows : null;
}
