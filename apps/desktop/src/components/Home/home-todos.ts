// ============================================================================
// home-todos - What Home asks the user to do, most harmful first. Each item
// names the consequence, says why, and opens the place that fixes it: a
// filter on Home's own list, or an agent's What it sees tab.
// ============================================================================

import { deploymentLabelFromAgentId } from "@skill-studio/lib";
import type { HarnessListing } from "@skill-studio/lib";
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

/** Agents whose history Skill Studio could read uses from, in catalog order. */
export function joinNames(names: string[]): string {
  if (names.length <= 1) return names.join("");
  return `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
}

export function homeTodos(
  groups: Pick<HomeGroups, "broken" | "warnings" | "updates" | "unused">,
  listings: HarnessListing[],
  historyAgents: string[],
  updateCaption: string,
): HomeTodo[] {
  const todos: HomeTodo[] = [];
  if (groups.broken.length)
    todos.push({
      id: "broken",
      tone: "error",
      title: `${plural(groups.broken.length, "skill")} don’t load`,
      detail: "Broken links or a SKILL.md the agents reject. No agent can use them.",
      action: "Show",
      to: { kind: "filter", filter: "broken" },
    });
  for (const row of listings) {
    if (row.evidence !== "preview" || row.budget === null) continue;
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
      action: "Show",
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
      detail: `Counted from ${joinNames(historyAgents.map(deploymentLabelFromAgentId))} history. Each is sent with every message; Park keeps it a click away.`,
      action: "Show",
      to: { kind: "filter", filter: "unused" },
    });
  if (groups.warnings.length)
    todos.push({
      id: "warnings",
      tone: "neutral",
      title: `${plural(groups.warnings.length, "skill")} need a look`,
      detail: "Copies that differ between agents, or a lock entry with no folder.",
      action: "Show",
      to: { kind: "filter", filter: "warn" },
    });
  return todos;
}
