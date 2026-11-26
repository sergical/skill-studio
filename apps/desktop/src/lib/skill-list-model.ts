// ============================================================================
// skill-list-model - Sorts a skill list and buckets it into SkillListTable's
// state groups (Broken, Warnings, Updates, Healthy, Parked), in display
// order. The one pure computation between a SkillSnapshot's skills and what
// the Skills list renders.
// ============================================================================

import { ATTENTION_ORDER } from "@skill-studio/lib";
import type {
  AttentionFilter,
  InstalledSkill,
  SkillAttention,
  SkillInvocationStats,
} from "@skill-studio/lib";
import type { SortMode } from "./skill-list-sort";
import { sortRows } from "../components/SkillList/skill-row-format";
import { ROW_GROUP_ORDER, rowGroup, rowState } from "../components/SkillList/skill-row-state";
import type { RowGroup, RowState } from "../components/SkillList/skill-row-state";

interface GroupedSkillRows {
  buckets: Record<RowGroup, InstalledSkill[]>;
  statesBySkill: Map<string, RowState | null>;
  /** The grouped display order: shift-click and `aria-rowindex` refer to this array, not the
   * caller's original `skills` order. */
  rows: InstalledSkill[];
}

/** `attention` is `attentionBySkill` over the same skills; `filter` is the Skills filter's
 * attention pick, so a skill shows under the category the user picked. */
export function groupSkillRows(
  skills: InstalledSkill[],
  sort: SortMode,
  stats: SkillInvocationStats[],
  attention: ReadonlyMap<string, SkillAttention>,
  filter?: AttentionFilter,
): GroupedSkillRows {
  const statsBySkill = new Map(stats.map((s) => [s.skill, s]));
  const sorted = sortRows(skills, sort, statsBySkill);
  const statesBySkill = new Map<string, RowState | null>();
  // SAFETY: each bucket starts empty; the loop below only ever pushes `InstalledSkill` values into it.
  const buckets = {
    broken: [] as InstalledSkill[],
    warning: [] as InstalledSkill[],
    update: [] as InstalledSkill[],
    healthy: [] as InstalledSkill[],
    parked: [] as InstalledSkill[],
  } satisfies Record<RowGroup, InstalledSkill[]>;
  for (const skill of sorted) {
    const state = rowState(skill);
    statesBySkill.set(skill.name, state);
    buckets[rowGroup(skill, attention.get(skill.name), filter)].push(skill);
  }
  // Inside an attention group the most serious issue kind comes first, as Home lists it; the
  // sort orders skills with the same kind (the sort is stable).
  for (const category of ATTENTION_ORDER) {
    const rank = (skill: InstalledSkill) => attention.get(skill.name)?.[category] ?? 0;
    buckets[category].sort((a, b) => rank(a) - rank(b));
  }
  const rows = ROW_GROUP_ORDER.flatMap((group) => buckets[group]);
  return { buckets, statesBySkill, rows };
}
