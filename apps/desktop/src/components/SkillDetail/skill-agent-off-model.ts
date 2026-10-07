// ============================================================================
// Skill Studio - skill-agent-off-model
// Pure helpers for "Turn off for <Agent>": which Locations rows offer it, the
// confirm text, and what the dialog shows once the backend check answers.
// ============================================================================

import type { AgentOffCheck } from "@skill-studio/lib";
import { SPLIT_HARNESSES } from "./skill-split-model";
import type { LocationAction, LocationRow, ScopeGroup } from "./skill-location-status";

export type TurnOffAction = Extract<LocationAction, { kind: "turn-off-agent" }>;

/**
 * The "Turn off for <Agent>" action for one row, or `null`. Only an agent row
 * that reads a live shared folder qualifies: a synthesized reader or a link.
 * A copy already belongs to that agent alone, a plugin has its own switch, and
 * an agent hidden by its own setting is already off.
 */
export function turnOffActionFor(group: ScopeGroup, row: LocationRow): TurnOffAction | null {
  const { shared } = group;
  const folder = shared?.deployment;
  if (!shared || !folder) return null;
  if (folder.disabled || folder.backing.kind !== "canonical" || shared.leftBehindLive) return null;
  if (row.kind !== "reader" && row.kind !== "link") return null;
  if (!SPLIT_HARNESSES.includes(row.harness)) return null;
  if (row.conditions.some((condition) => condition.level === "off")) return null;
  return {
    kind: "turn-off-agent",
    target: shared.lifecycleTarget,
    agent: row.harness,
    agentLabel: row.harnessLabel,
    shared: folder,
    scopeLabel: group.label,
    projectPath: group.projectPath ?? null,
  };
}

/** The confirm text: what the backend does, in the words the user sees. */
export function turnOffConfirmText(skillName: string, agentLabel: string): string {
  return `${skillName} is shared by every agent. To turn it off for ${agentLabel} only, Skill Studio gives each agent its own copy, then parks the ${agentLabel} copy. You will see one copy per agent from now on.`;
}

/** What the dialog shows for a backend check: the normal confirm, or the refusal reason. */
export type TurnOffView =
  | { kind: "confirm" }
  | { kind: "refused"; reason: string; offEverywhere: boolean };

export function turnOffView(check: AgentOffCheck): TurnOffView {
  if (!check.refusal) return { kind: "confirm" };
  return {
    kind: "refused",
    reason: check.refusal.reason,
    offEverywhere: check.refusal.off_everywhere,
  };
}

/** The toast after a successful turn-off. Activity holds the one undo. */
export function turnOffSuccessMessage(agentLabel: string): string {
  return `Parked the ${agentLabel} copy. Every other agent keeps its own copy. Undo it from Activity.`;
}
