// ============================================================================
// Skill Studio - skill-list-filter
// Pure filter over a skill list: scope (all/global/parked/project), one
// harness, one source kind, one attention category or issue kind, and a
// free-text query. Shared by SkillListFilterBar and SkillsView so the filter
// bar's controls and the list they drive never drift apart.
// ============================================================================

import { skillUsageIn30Days } from "./home-summary";
import { attentionBySkill, attentionCategoryOf } from "./skill-attention";
import type { AttentionFilter } from "./skill-attention";
import type { HealthIssueKind } from "./skill-health";
import { agentsCoveredByDeployment, collectDashboardIssues } from "./skill-health";
import { pluginDeployments } from "./skill-plugin-partition";
import type {
  Deployment,
  InstalledSkill,
  SkillInvocationStats,
  SkillSourceKind,
} from "./skill-types";

/** Which own skills `applySkillListFilter` considers before the other fields narrow it further. */
export type SkillListFilterScope = "all" | "global" | "parked" | { project: string };

/** The Skills view's filter bar state: scope, an optional harness/source/issue narrower, and a query. */
export interface SkillListFilter {
  scope: SkillListFilterScope;
  /** An `AgentId` display label (e.g. "Claude Code"), one at a time. */
  harness?: string;
  source?: SkillSourceKind;
  /** Keeps the skills in one attention category (Broken, Warnings, Updates), or in any of them. */
  attention?: AttentionFilter;
  /** Keeps the skills with one issue kind. */
  issue?: HealthIssueKind;
  /** Matches `skill.invocation` exactly. */
  invocation?: InstalledSkill["invocation"];
  /** Used in the last 30 days, or unused by `skillUsageIn30Days` (an agent that reads it has history). */
  usage?: "used-30d" | "unused-30d";
  query: string;
}

/** A filter with every field at its default: every non-parked skill, no query. */
export function defaultSkillListFilter(): SkillListFilter {
  return { scope: "all", query: "" };
}

/** True when `scope` is the `{ project: string }` variant, narrowing its type for callers. */
export function isProjectScope(scope: SkillListFilterScope): scope is { project: string } {
  return scope !== "all" && scope !== "global" && scope !== "parked";
}

function isGlobalDeployment(deployment: Deployment): boolean {
  return deployment.scope === "global" || deployment.scope === "plugin";
}

/** Project rows require a local install; their client coverage also includes inherited global copies.
 * Use this projection for display only. Lifecycle actions need the complete skill. */
export function skillForCoverageScope(
  skill: InstalledSkill,
  scope: SkillListFilterScope,
): InstalledSkill {
  if (scope === "all") return skill;
  let deployments: Deployment[];
  if (isProjectScope(scope)) {
    deployments = [
      ...skill.deployments.filter((d) => d.project_path === scope.project),
      ...skill.deployments.filter(isGlobalDeployment),
    ];
  } else if (scope === "global") {
    deployments = skill.deployments.filter(isGlobalDeployment);
  } else {
    deployments = skill.deployments.filter((d) => d.scope === "parked");
  }
  return { ...skill, deployments };
}

/** Whether `skill` belongs to `scope`. `all` and `global` never include parked skills. */
function matchesScope(skill: InstalledSkill, scope: SkillListFilterScope): boolean {
  if (scope === "parked") return skill.parked;
  if (skill.parked) return false;
  if (scope === "all") return true;
  if (scope === "global") {
    return skill.deployments.some(isGlobalDeployment);
  }
  return skill.deployments.some((d) => d.project_path === scope.project);
}

/** The client filter matches its own install folder and the shared folder for its native readers. */
export function deploymentMatchesHarness(deployment: Deployment, harness: string): boolean {
  return agentsCoveredByDeployment(deployment.agent).includes(harness);
}

/** Hyphens, underscores, slashes, dots, and spaces all count as one word break. */
const WORD_BREAK = /[\s\-_/.]+/;

function words(text: string): string[] {
  return text.toLowerCase().split(WORD_BREAK).filter(Boolean);
}

/**
 * Whether `skill` matches `query`, case-insensitive. The name matches when the query, breaks
 * removed, is inside the name with its breaks removed ("ihave", "have adhd"), or when every query
 * word starts a name word in any order ("adhd have"). The description and source match the query
 * as one phrase, with every word break treated alike. Word-level matching stays on the name only:
 * across descriptions, short words like "ask" hit inside "task" and flood the list.
 */
function matchesQuery(skill: InstalledSkill, query: string): boolean {
  const queryWords = words(query);
  if (queryWords.length === 0) return true;
  const nameWords = words(skill.name);
  if (nameWords.join("").includes(queryWords.join(""))) return true;
  if (queryWords.every((q) => nameWords.some((n) => n.startsWith(q)))) return true;
  const phrase = queryWords.join(" ");
  return [skill.description ?? "", skill.source].some((field) =>
    words(field).join(" ").includes(phrase),
  );
}

/**
 * Filters `skills` by every field of `filter` in turn: scope, then harness,
 * source kind, and issue kind (each only when set), then the free-text
 * query. `issues` should be `collectDashboardIssues(skills)` (or equivalent)
 * from the caller, computed once and shared - passed in rather than
 * recomputed here so a caller filtering a large list repeatedly doesn't pay
 * for it more than once per render. `invocations` is `SkillSnapshot.invocations`
 * (or equivalent), needed only when `filter.usage` is set; `agents` are the agents the user keeps,
 * so the unused list matches Home's count (see `skillUsageIn30Days`).
 */
export function applySkillListFilter(
  skills: InstalledSkill[],
  filter: SkillListFilter,
  issues = collectDashboardIssues(skills),
  invocations?: SkillInvocationStats[],
  agents?: readonly string[],
): InstalledSkill[] {
  const skillsWithIssue = filter.issue
    ? new Set(issues.filter((i) => i.kind === filter.issue).map((i) => i.skill.name))
    : null;
  const attention = filter.attention ? attentionBySkill(skills, issues) : null;
  const usage = filter.usage ? skillUsageIn30Days(invocations ?? [], agents) : null;

  return skills.filter((skill) => {
    if (!matchesScope(skill, filter.scope)) return false;
    const harness = filter.harness;
    if (
      harness &&
      !skillForCoverageScope(skill, filter.scope).deployments.some((d) =>
        deploymentMatchesHarness(d, harness),
      )
    ) {
      return false;
    }
    // "plugin" reaches outside a skill's own source_kind: it means "has at
    // least one plugin deployment", tested against whatever deployments the
    // caller passed in (the pluginSkillsView) rather than the aggregate
    // source_kind, which describes the skill's own (non-plugin) copies.
    if (filter.source === "plugin") {
      if (pluginDeployments(skill).length === 0) return false;
    } else if (filter.source && skill.source_kind !== filter.source) {
      return false;
    }
    if (skillsWithIssue && !skillsWithIssue.has(skill.name)) return false;
    if (attention && attentionCategoryOf(attention.get(skill.name), filter.attention) === null)
      return false;
    if (filter.invocation && skill.invocation !== filter.invocation) return false;
    if (usage) {
      const want = filter.usage === "used-30d" ? "used" : "unused";
      if (usage(skill) !== want) return false;
    }
    if (!matchesQuery(skill, filter.query)) return false;
    return true;
  });
}
