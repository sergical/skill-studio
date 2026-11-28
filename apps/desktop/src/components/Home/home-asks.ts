// ============================================================================
// home-asks - What needs the user on Home: the counts that open Skills
// filtered to them, and one prompt line per item for an agent with the Skill
// Studio MCP server. Counts and prompt come from the same lists, so they agree.
// ============================================================================

import {
  attentionGroups,
  collectDashboardIssues,
  formatCount,
  ownSkillsView,
  skillsWithUpdates,
  unusedSkills,
} from "@skill-studio/lib";
import type { HealthIssue, SkillListFilter, SkillSnapshot } from "@skill-studio/lib";
import type { HomeAgent } from "./home-agents";

export const plural = (n: number, one: string, many = `${one}s`) =>
  `${formatCount(n)} ${n === 1 ? one : many}`;

/** Names in a prompt: all of them up to ten, then a count the agent can look up itself. */
export function names(list: string[]): string {
  const shown = list.slice(0, 10);
  const rest = list.length - shown.length;
  return rest > 0 ? `${shown.join(", ")} and ${formatCount(rest)} more` : shown.join(", ");
}

/** The skill names behind each Home count. */
export interface HomeCounts {
  broken: string[];
  warnings: string[];
  updates: string[];
  unused: string[];
}

/**
 * Home's counts. `agents` are the agents the user keeps: an agent they turned off adds no use
 * and no history to "unused", as on the Skills filter the count opens.
 */
export function homeCounts(
  snapshot: SkillSnapshot,
  agents: readonly string[] | undefined,
): HomeCounts {
  const own = ownSkillsView(snapshot.skills);
  // Each count opens Skills, which lists no parked skill, so none is counted here either.
  const live = own.filter((skill) => !skill.parked);
  const liveIssues = collectDashboardIssues(own).filter((issue) => !issue.skill.parked);
  const { broken, warnings } = attentionGroups(liveIssues);
  // One skill with two broken links still counts once, as on Skills.
  const skillNames = (issues: HealthIssue[]) => [...new Set(issues.map((i) => i.skill.name))];
  return {
    broken: skillNames(broken),
    warnings: skillNames(warnings),
    updates: skillsWithUpdates(snapshot).flatMap((skill) => (skill.parked ? [] : [skill.name])),
    unused: unusedSkills(live, snapshot.invocations, agents).map((skill) => skill.name),
  };
}

export interface HomeTask {
  id: "broken" | "warnings" | "updates" | "unused" | `cut:${string}`;
  /** The prompt line, an instruction that starts with a verb. */
  line: string;
}

export interface HomeCount {
  id: "broken" | "warnings" | "updates" | "unused";
  count: number;
  word: string;
  tone: "error" | "warning" | "neutral";
  /** The Skills filter that lists exactly these skills. */
  filter: Partial<SkillListFilter>;
}

export function cutLine(agent: HomeAgent): string | null {
  const parts = [
    agent.nameOnly?.length ? `lists ${plural(agent.nameOnly.length, "skill")} by name only` : "",
    agent.notListed?.length ? `leaves ${plural(agent.notListed.length, "skill")} out` : "",
  ].filter(Boolean);
  if (!parts.length) return null;
  const cut = [...(agent.nameOnly ?? []), ...(agent.notListed ?? [])];
  return `Make room in ${agent.label}'s skill list. It ${parts.join(" and ")}: ${names(cut)}. Suggest which skills I use least to park.`;
}

/** The prompt lines and the counts, most harmful first. */
export function homeAsks(counts: HomeCounts, agents: HomeAgent[]) {
  const tasks: HomeTask[] = [];
  const chips: HomeCount[] = [];
  const { broken, warnings, updates, unused } = counts;
  if (broken.length) {
    tasks.push({
      id: "broken",
      line: `Fix the ${plural(broken.length, "broken skill")}: ${names(broken)}.`,
    });
    chips.push({
      id: "broken",
      count: broken.length,
      word: "broken",
      tone: "error",
      filter: { attention: "broken" },
    });
  }
  if (warnings.length) {
    tasks.push({
      id: "warnings",
      line: `Check the ${plural(warnings.length, "skill")} with warnings and fix what you can: ${names(warnings)}.`,
    });
    chips.push({
      id: "warnings",
      count: warnings.length,
      word: warnings.length === 1 ? "warning" : "warnings",
      tone: "warning",
      filter: { attention: "warning" },
    });
  }
  if (updates.length) {
    tasks.push({
      id: "updates",
      line: `Show me what changed in the ${plural(updates.length, "skill")} with updates, then update the ones I approve: ${names(updates)}.`,
    });
    chips.push({
      id: "updates",
      count: updates.length,
      word: updates.length === 1 ? "update" : "updates",
      tone: "neutral",
      filter: { attention: "update" },
    });
  }
  for (const agent of agents) {
    const line = cutLine(agent);
    if (line) tasks.push({ id: `cut:${agent.id}`, line });
  }
  if (unused.length) {
    tasks.push({
      id: "unused",
      line: `Park the ${plural(unused.length, "skill")} I have not used in 30 days, after I confirm the list: ${names(unused)}.`,
    });
    chips.push({
      id: "unused",
      count: unused.length,
      word: "unused",
      tone: "neutral",
      filter: { usage: "unused-30d" },
    });
  }
  return { tasks, chips };
}

const lowerFirst = (text: string) => text.charAt(0).toLowerCase() + text.slice(1);

export const PROMPT_OPENING = "Use the Skill Studio MCP tools to sort out my skills:";
export const PROMPT_CLOSING = "Tell me what you changed.";

/** One prompt for the given tasks: a single task reads as one sentence, more as a list. */
export function promptFor(tasks: HomeTask[]): string {
  if (tasks.length === 1)
    return `Use the Skill Studio MCP tools to ${lowerFirst(tasks[0].line)} ${PROMPT_CLOSING}`;
  return [PROMPT_OPENING, ...tasks.map((task) => `- ${task.line}`), PROMPT_CLOSING].join("\n");
}
