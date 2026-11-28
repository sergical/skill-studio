// ============================================================================
// Skill Studio - home-summary
// Pure helpers behind Home's summary line and "Recently used" block, kept
// separate from HomeView.tsx so they're unit-testable without React.
// ============================================================================

import { issueAttentionCategory } from "./skill-attention";
import { skillVisibleToAgent } from "./skill-coverage";
import { FIRST_CLASS_AGENTS, agentsCoveredByDeployment } from "./skill-health";
import type { HealthIssue } from "./skill-health";
import { ownSkillsView } from "./skill-plugin-partition";
import { HARNESS_CATALOG } from "./skill-types.generated";
import type { InstalledSkill, SkillInvocationStats, SkillSnapshot } from "./skill-types";

/**
 * Which of `FIRST_CLASS_AGENTS` at least one own deployment in the snapshot
 * gives coverage for, in `FIRST_CLASS_AGENTS` order. Coverage, not literal
 * deployments: a skill in the shared `.agents/skills` root is readable by
 * every shared-root reader (Codex, OpenCode, pi, ...) even with no
 * harness-specific deployment - see `agentsCoveredByDeployment`.
 */
export function harnessesPresent(snapshot: SkillSnapshot): string[] {
  const own = ownSkillsView(snapshot.skills);
  const present = new Set<string>();
  for (const skill of own) {
    for (const deployment of skill.deployments) {
      for (const agent of agentsCoveredByDeployment(deployment.agent)) {
        present.add(agent);
      }
    }
  }
  return FIRST_CLASS_AGENTS.filter((agent) => present.has(agent));
}

/** One row of Home's "Recently used" block. */
export interface RecentlyUsedSkill {
  skill: InstalledSkill;
  lastUsed: string;
  /** The project this skill was invoked in the most over the last 30 days, when any invocation carried a project path. */
  projectLabel?: string;
  usesIn30Days: number;
}

/** The basename of the project path with the highest invocation count in `byProject`, if any. */
function topProjectLabel(byProject: Record<string, number>): string | undefined {
  const entries = Object.entries(byProject);
  if (entries.length === 0) return undefined;
  const [path] = entries.sort((a, b) => b[1] - a[1])[0];
  return path.split("/").filter(Boolean).pop() ?? path;
}

/**
 * The `n` skills with the most recent invocation, newest first. Skills with
 * no recorded invocation, or whose stats no longer match an installed skill,
 * are excluded.
 */
export function recentlyUsedSkills(
  skills: InstalledSkill[],
  stats: SkillInvocationStats[],
  n: number,
): RecentlyUsedSkill[] {
  const skillsByName = new Map(skills.map((s) => [s.name, s]));

  const rows: RecentlyUsedSkill[] = [];
  for (const stat of stats) {
    const skill = skillsByName.get(stat.skill);
    if (!stat.last_used || !skill) continue;
    rows.push({
      skill,
      lastUsed: stat.last_used,
      projectLabel: topProjectLabel(stat.by_project_30_days),
      usesIn30Days: stat.last_30_days,
    });
  }

  return rows
    .sort((a, b) => new Date(b.lastUsed).getTime() - new Date(a.lastUsed).getTime())
    .slice(0, n);
}

/** How Home's "Invocation" segment breaks down `skills`' own (non-plugin) skills by policy. */
export interface HomeInvocationCounts {
  both: number;
  modelOnly: number;
  userOnly: number;
}

/** Counts of own skills by `invocation` policy - "both" vs "model-only" vs "user-only". */
export function homeInvocationCounts(skills: InstalledSkill[]): HomeInvocationCounts {
  const own = ownSkillsView(skills);
  const counts: HomeInvocationCounts = { both: 0, modelOnly: 0, userOnly: 0 };
  for (const skill of own) {
    if (skill.invocation === "both") counts.both += 1;
    else if (skill.invocation === "model-only") counts.modelOnly += 1;
    else counts.userOnly += 1;
  }
  return counts;
}

/** Home's per-turn prompt cost breakdown, split into skills used vs. idle in the last 30 days. */
export interface HomePromptCost {
  totalTokens: number;
  usedTokens: number;
  usedCount: number;
  idleTokens: number;
  idleCount: number;
}

/**
 * The prompt cost the model actually pays: only skills the model can invoke
 * (`invocation` is `"both"` or `"model-only"`) contribute their
 * `description_tokens` - a `"user-only"` skill is never in the model's
 * context to begin with. Split into "used" (invoked at least once in the
 * last 30 days) and "idle" (not).
 */
export function homePromptCost(
  skills: InstalledSkill[],
  invocations: SkillInvocationStats[],
): HomePromptCost {
  const last30DaysBySkill = new Map(invocations.map((stat) => [stat.skill, stat.last_30_days]));
  const cost: HomePromptCost = {
    totalTokens: 0,
    usedTokens: 0,
    usedCount: 0,
    idleTokens: 0,
    idleCount: 0,
  };

  for (const skill of skills) {
    if (skill.invocation !== "both" && skill.invocation !== "model-only") continue;
    cost.totalTokens += skill.description_tokens;
    const usedIn30Days = (last30DaysBySkill.get(skill.name) ?? 0) > 0;
    if (usedIn30Days) {
      cost.usedTokens += skill.description_tokens;
      cost.usedCount += 1;
    } else {
      cost.idleTokens += skill.description_tokens;
      cost.idleCount += 1;
    }
  }

  return cost;
}

/**
 * Each skill's uses in the last 30 days, by `agents` only when given: the agents the user keeps.
 * The Skills row count, its "Most used" sort and the unused rule all read these numbers, so a
 * skill listed as unused never shows a use.
 */
export function usesIn30DaysBySkill(
  invocations: SkillInvocationStats[],
  agents?: readonly string[],
): Map<string, number> {
  return new Map(
    invocations.map((stat) => [
      stat.skill,
      agents
        ? Object.entries(stat.by_harness_30_days).reduce(
            (sum, [agent, count]) => (agents.includes(agent) ? sum + count : sum),
            0,
          )
        : stat.last_30_days,
    ]),
  );
}

/** What the 30-day history can say about one skill. */
export type SkillUsageIn30Days = "used" | "unused" | "no-readable-history";

/**
 * The one rule for "unused": a skill is unused only when at least one agent that reads a copy of
 * it has readable history (any record) and the skill has no use in 30 days. With history from
 * Claude Code only, a skill only Cursor reads has no readable history, not no use. `agents`, when
 * given, are the agents the user keeps: an agent they turned off adds no use and no history, even
 * while its records are still in the snapshot. Returns a classifier so a caller checking many
 * skills reads `invocations` once.
 */
export function skillUsageIn30Days(
  invocations: SkillInvocationStats[],
  agents?: readonly string[],
): (skill: InstalledSkill) => SkillUsageIn30Days {
  const counted = (agent: string) => !agents || agents.includes(agent);
  const usesBySkill = usesIn30DaysBySkill(invocations, agents);
  const withHistory = HARNESS_CATALOG.filter(
    (row) =>
      counted(row.id) && invocations.some((stat) => (stat.by_harness_30_days[row.id] ?? 0) > 0),
  ).map((row) => row.id);
  return (skill) => {
    if ((usesBySkill.get(skill.name) ?? 0) > 0) return "used";
    return withHistory.some((agent) => skillVisibleToAgent(skill, agent) !== "none")
      ? "unused"
      : "no-readable-history";
  };
}

/**
 * Own skills with no use in the last 30 days by `skillUsageIn30Days` (from the history of
 * `agents` only, when given), model-invocable (`"both"` or `"model-only"`) first, then
 * `"user-only"`, alphabetical within each group - the order Home's park prompt and Learn's
 * "unused" section list them in.
 */
export function unusedSkills(
  skills: InstalledSkill[],
  invocations: SkillInvocationStats[],
  agents?: readonly string[],
): InstalledSkill[] {
  const usage = skillUsageIn30Days(invocations, agents);
  const own = ownSkillsView(skills).filter((skill) => usage(skill) === "unused");

  return own.sort((a, b) => {
    const aModelInvocable = a.invocation !== "user-only";
    const bModelInvocable = b.invocation !== "user-only";
    if (aModelInvocable !== bModelInvocable) return aModelInvocable ? -1 : 1;
    return a.name.localeCompare(b.name);
  });
}

/** Home's Broken and Warnings issues, split by `issueAttentionCategory`. */
export interface HomeAttentionGroups {
  broken: HealthIssue[];
  warnings: HealthIssue[];
}

/** Splits `issues` into Broken and Warnings, by the category the Skills page groups them under. */
export function attentionGroups(issues: HealthIssue[]): HomeAttentionGroups {
  const groups: HomeAttentionGroups = { broken: [], warnings: [] };
  for (const issue of issues) {
    if (issueAttentionCategory(issue) === "broken") groups.broken.push(issue);
    else groups.warnings.push(issue);
  }
  return groups;
}
