// ============================================================================
// skill-list-deployment - The copy a skill-list row opens on for the current
// scope filter.
// ============================================================================

import {
  deploymentMatchesHarness,
  deploymentWithSpecViolations,
  isProjectScope,
  skillForCoverageScope,
} from "@skill-studio/lib";
import type { InstalledSkill, SkillListFilter } from "@skill-studio/lib";

/** The deployment the current scope shows for `skill`, so the detail drawer opens on that copy.
 * With no scope, the copy carrying spec violations, so the row's badge and the page agree. */
export function deploymentForScope(
  skill: InstalledSkill,
  scope: SkillListFilter["scope"],
  harness?: string,
): string | undefined {
  if (scope === "all" && !harness) return deploymentWithSpecViolations(skill)?.path;
  const { deployments } = skillForCoverageScope(skill, scope);
  if (!harness) return deployments[0]?.path;
  const matches = deployments.filter((d) => deploymentMatchesHarness(d, harness));
  const local = isProjectScope(scope)
    ? matches.filter((d) => d.project_path === scope.project)
    : matches;
  const choices = local.length ? local : matches;
  return (choices.find((d) => d.agent === harness) ?? choices[0])?.path;
}
