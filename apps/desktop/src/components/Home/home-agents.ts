// ============================================================================
// home-agents - What Home says about each set-up agent (what it sees, what its
// skill list cuts, its uses this week) and the week's uses per day.
// ============================================================================

import type { HarnessListing, SkillInvocationStats, SkillSnapshot } from "@skill-studio/lib";
import { hasBudgetEstimate, hasReadableHistory, listedEntries } from "../Listing/client-allocation";

export interface HomeAgent {
  id: string;
  label: string;
  /** Skills behind `seesUnit`; null until its listing is read. */
  sees: number | null;
  /** What `sees` counts: the skill list only when there is an estimate to stand behind. */
  seesUnit: "in skill list" | "found";
  /** True when the listing is read and has no budget estimate, so nothing can be called cut. */
  noEstimate: boolean;
  /** Null when this agent has no budget estimate. */
  nameOnly: string[] | null;
  notListed: string[] | null;
  /** Uses this week; null when no history record names this agent. */
  uses: number | null;
}

export function homeAgent(
  agent: { id: string; label: string },
  listing: HarnessListing | undefined,
  invocations: SkillInvocationStats[],
  weekUses: number[] | undefined,
): HomeAgent {
  const estimated = listing !== undefined && hasBudgetEstimate(listing);
  const offered = (listing?.entries ?? []).filter((e) => e.state !== "hidden");
  // Not listed skills are out of the list; without an estimate, the count is what its folders hold.
  const sees = !listing ? null : estimated ? listedEntries(listing).length : offered.length;
  return {
    id: agent.id,
    label: agent.label,
    sees,
    seesUnit: estimated ? "in skill list" : "found",
    noEstimate: listing !== undefined && !estimated,
    nameOnly: estimated ? offered.filter((e) => e.state === "name_only").map((e) => e.name) : null,
    notListed: estimated ? offered.filter((e) => e.state === "omitted").map((e) => e.name) : null,
    uses: hasReadableHistory(invocations, agent.id)
      ? (weekUses ?? []).reduce((a, b) => a + b, 0)
      : null,
  };
}

/** Uses per day for the last `days` days (oldest first), from the hourly history of `agents`
 * (every agent when undefined): in total and per agent, with a one-letter weekday label each. */
export function dailyUses(
  snapshot: SkillSnapshot,
  agents: readonly string[] | undefined,
  days = 7,
) {
  // Days are the user's local days, so an evening use lands on the day it happened.
  // Each moment uses its own offset, so a use before a daylight-saving change keeps its day.
  const localDay = (ms: number) =>
    Math.floor((ms - new Date(ms).getTimezoneOffset() * 60_000) / 86_400_000);
  const start = localDay(Date.parse(snapshot.scanned_at)) - days + 1;
  const total = Array.from({ length: days }, () => 0);
  const byAgent = new Map<string, number[]>();
  for (const stats of snapshot.invocations) {
    for (const hour of stats.by_hour) {
      if (agents && !agents.includes(hour.harness)) continue;
      const day = localDay(hour.hour * 3_600_000) - start;
      if (day < 0 || day >= days) continue;
      total[day] += hour.count;
      const agent = byAgent.get(hour.harness) ?? Array.from({ length: days }, () => 0);
      agent[day] += hour.count;
      byAgent.set(hour.harness, agent);
    }
  }
  const scan = new Date(snapshot.scanned_at);
  const labels = Array.from({ length: days }, (_, i) =>
    new Date(
      scan.getFullYear(),
      scan.getMonth(),
      scan.getDate() - (days - 1 - i),
      12,
    ).toLocaleDateString(undefined, { weekday: "narrow" }),
  );
  return { total, byAgent, labels };
}
