// ============================================================================
// skill-list-deployment - The copy a skill-list row opens on for the current
// scope filter.
// ============================================================================

import {
  deploymentMatchesHarness,
  deploymentWithSpecViolations,
  isProjectScope,
  isUnresolvedDeployment,
  skillForCoverageScope,
} from "@skill-studio/lib";
import type { Deployment, InstalledSkill, SkillListFilter } from "@skill-studio/lib";

function selectListDeployment(
  skill: InstalledSkill,
  scope: SkillListFilter["scope"],
  harness?: string,
): Deployment | undefined {
  if (scope === "all" && !harness) return deploymentWithSpecViolations(skill);
  const { deployments } = skillForCoverageScope(skill, scope);
  if (!harness) return deployments[0];
  const matches = deployments.filter((d) => deploymentMatchesHarness(d, harness));
  const local = isProjectScope(scope)
    ? matches.filter((d) => d.project_path === scope.project)
    : matches;
  const choices = local.length ? local : matches;
  return choices.find((d) => d.agent === harness) ?? choices[0];
}

/** Open the selected copy's physical file, or the broken link itself for repair. */
export function deploymentForScope(
  skill: InstalledSkill,
  scope: SkillListFilter["scope"],
  harness?: string,
): string | undefined {
  const selected = selectListDeployment(skill, scope, harness);
  if (!selected || isUnresolvedDeployment(selected)) return selected?.path;
  const backing = selected.backing;
  if (backing.kind === "linked-to") {
    return skill.deployments.find((d) => d.id === backing.deployment_id)?.path ?? selected.path;
  }
  return selected.path;
}
