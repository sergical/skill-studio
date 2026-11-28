// ============================================================================
// home-todos - What Home asks the user to do, most harmful first. Each item
// names the consequence, says why, and opens the place that fixes it: a
// filter on Home's own list, or an agent's What it sees tab.
// ============================================================================

import { deploymentLabelFromAgentId, joinNames } from "@skill-studio/lib";
import type { HarnessListing } from "@skill-studio/lib";
import { hasBudgetEstimate } from "../Listing/client-allocation";
import type { HomeFilter, HomeGroups } from "./home-inbox-data";

export type HomeTodoAction =
  | { kind: "filter"; filter: HomeFilter }
  | { kind: "agent"; harness: string };

export interface HomeTodo {
  id: string;
  tone: "error" | "warning" | "neutral";
  title: string;
  detail: string;
  action: string;
  to: HomeTodoAction;
}

const plural = (count: number, one: string, many = `${one}s`) =>
  `${count} ${count === 1 ? one : many}`;

export function homeTodos(
  groups: Pick<HomeGroups, "broken" | "warnings" | "updates" | "unused">,
  listings: HarnessListing[],
  historyAgents: string[],
  updateCaption: string,
): HomeTodo[] {
  const todos: HomeTodo[] = [];
  // Home lists one row per issue; a skill with two broken links still counts once, as on Skills.
  const skillsIn = (issues: Pick<HomeGroups, "broken">["broken"]) =>
    new Set(issues.map((issue) => issue.skill.name)).size;
  const broken = skillsIn(groups.broken);
  const warnings = skillsIn(groups.warnings);
  if (broken)
    todos.push({
      id: "broken",
      tone: "error",
      title: `${plural(broken, "skill")} ${broken === 1 ? "is" : "are"} broken`,
      detail:
        "A link to a folder that is gone, or a SKILL.md with no description an agent can read. The agent behind each one can’t pick it.",
      action: `Review ${plural(broken, "broken skill")}`,
      to: { kind: "filter", filter: "broken" },
    });
  if (warnings)
    todos.push({
      id: "warnings",
      tone: "warning",
      title: `${plural(warnings, "skill")} ${warnings === 1 ? "needs" : "need"} a look`,
      detail:
        "Copies that differ, a SKILL.md that some agents skip or name differently, or a lock entry with no folder.",
      action: "Review warnings",
      to: { kind: "filter", filter: "warn" },
    });
  for (const row of listings) {
    if (!hasBudgetEstimate(row)) continue;
    const label = deploymentLabelFromAgentId(row.harness);
    const count = (...states: string[]) =>
      row.entries.filter((e) => states.includes(e.state)).length;
    const left = count("name_only", "omitted");
    const cut = count("truncated");
    const full = count("described");
    if (left)
      todos.push({
        id: `space-${row.harness}`,
        tone: "warning",
        title: `${label} gets only the names of ${plural(left, "skill")}`,
        detail: "Its skill space is full, and it rarely picks a skill it knows only by name.",
        action: `See what ${label} sees`,
        to: { kind: "agent", harness: row.harness },
      });
    else if (cut > full)
      todos.push({
        id: `space-${row.harness}`,
        tone: "warning",
        title: `${label} shortens ${cut} of ${cut + full} descriptions`,
        detail: "It sees too little of most skills to pick them on its own.",
        action: `See what ${label} sees`,
        to: { kind: "agent", harness: row.harness },
      });
  }
  if (groups.updates.length)
    todos.push({
      id: "updates",
      tone: "neutral",
      title: plural(groups.updates.length, "update"),
      detail: `Newer versions upstream. ${updateCaption}.`,
      action: `Review ${plural(groups.updates.length, "update")}`,
      to: { kind: "filter", filter: "upd" },
    });
  // Without any readable history, "unused" would mean "not seen", so the item stays out. A skill
  // only you can run is not in the prompt, so it costs nothing while unused.
  const unused = groups.unused.filter((skill) => skill.invocation !== "user-only");
  if (unused.length && historyAgents.length)
    todos.push({
      id: "unused",
      tone: "neutral",
      title: `${plural(unused.length, "skill")} had no use in 30 days`,
      detail: `Counted from ${joinNames(historyAgents.map(deploymentLabelFromAgentId))} history only. Each is sent with every message; Park keeps it a click away.`,
      action: "Review unused",
      to: { kind: "filter", filter: "unused" },
    });
  return todos;
}
