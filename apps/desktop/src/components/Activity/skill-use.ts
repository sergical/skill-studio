// ============================================================================
// skill-use - One skill's uses on an Activity list, and whether the list's
// uses come from more than one agent.
// ============================================================================

export interface SkillUse {
  skill: string;
  count: number;
  /** Agents behind the uses, busiest first. */
  harnesses: string[];
  /** Absent on a day's list, where the day already says when. */
  lastUsed?: string | null;
  tooltip: string[];
}

/** True when the rows' uses come from two or more agents, so per-row agent marks tell them apart. */
export function manyAgents(rows: SkillUse[]): boolean {
  return new Set(rows.flatMap((row) => row.harnesses)).size > 1;
}
