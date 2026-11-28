// PROTOTYPE (throwaway): what "Ask in one line" says about each set-up agent. Deleted with
// src/prototype/home-library.

import { formatCount } from "@skill-studio/lib";
import type { HarnessListing, SkillInvocationStats } from "@skill-studio/lib";
import {
  hasAssumedClaudeWindow,
  hasBudgetEstimate,
  hasReadableHistory,
  listedEntries,
} from "../../components/Listing/client-allocation";

export const plural = (n: number, one: string, many = `${one}s`) =>
  `${formatCount(n)} ${n === 1 ? one : many}`;

/** Names in a prompt: all of them up to ten, then a count the agent can look up itself. */
export function names(list: string[]): string {
  const shown = list.slice(0, 10);
  const rest = list.length - shown.length;
  return rest > 0 ? `${shown.join(", ")} and ${formatCount(rest)} more` : shown.join(", ");
}

export interface AgentCell {
  id: string;
  label: string;
  /** Skills behind `seesUnit`; null until its listing is read. */
  sees: number | null;
  /** What `sees` counts: the skill list only when there is an estimate to stand behind. */
  seesUnit: string;
  /** Null when this agent has no budget estimate, so nothing can be called cut. */
  nameOnly: string[] | null;
  notListed: string[] | null;
  /** Uses this week; null when no history record names this agent. */
  uses: number | null;
}

export function agentCell(
  agent: { id: string; label: string },
  listing: HarnessListing | undefined,
  invocations: SkillInvocationStats[],
  weekUses: number[] | undefined,
): AgentCell {
  const estimated = listing !== undefined && hasBudgetEstimate(listing);
  const offered = (listing?.entries ?? []).filter((e) => e.state !== "hidden");
  // Not listed skills are out of the list; without an estimate, the count is what its folders hold.
  const sees = !listing ? null : estimated ? listedEntries(listing).length : offered.length;
  const seesUnit = estimated
    ? "in skill list"
    : listing && hasAssumedClaudeWindow(listing)
      ? "found, model window unknown"
      : "found in folders it reads";
  return {
    id: agent.id,
    label: agent.label,
    sees,
    seesUnit,
    nameOnly: estimated ? offered.filter((e) => e.state === "name_only").map((e) => e.name) : null,
    notListed: estimated ? offered.filter((e) => e.state === "omitted").map((e) => e.name) : null,
    uses: hasReadableHistory(invocations, agent.id)
      ? (weekUses ?? []).reduce((a, b) => a + b, 0)
      : null,
  };
}

export function cutLine(agent: AgentCell): string | null {
  const parts = [
    agent.nameOnly?.length ? `lists ${plural(agent.nameOnly.length, "skill")} by name only` : "",
    agent.notListed?.length ? `leaves ${plural(agent.notListed.length, "skill")} out` : "",
  ].filter(Boolean);
  if (!parts.length) return null;
  const cut = [...(agent.nameOnly ?? []), ...(agent.notListed ?? [])];
  return `Make room in ${agent.label}'s skill list. It ${parts.join(" and ")}: ${names(cut)}. Suggest which skills I use least to park.`;
}
