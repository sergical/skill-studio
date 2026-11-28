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

/**
 * Open the selected copy's physical file, or the broken link itself for repair.
 * `reader` is the skill with its plugin copies, which the list leaves out of
 * `skill`: with an agent filter, a row that agent reads only through a plugin
 * opens that plugin copy, not another agent's copy.
 */
export function deploymentForScope(
  skill: InstalledSkill,
  scope: SkillListFilter["scope"],
  harness?: string,
  reader: InstalledSkill = skill,
): string | undefined {
  const own = selectListDeployment(skill, scope, harness);
  const selected = own ?? (harness ? selectListDeployment(reader, scope, harness) : undefined);
  if (!selected || isUnresolvedDeployment(selected)) return selected?.path;
  const backing = selected.backing;
  if (backing.kind === "linked-to") {
    return reader.deployments.find((d) => d.id === backing.deployment_id)?.path ?? selected.path;
  }
  return selected.path;
}
