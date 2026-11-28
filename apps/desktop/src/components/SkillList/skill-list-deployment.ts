// ============================================================================
// skill-list-deployment - The copy a skill-list row opens on for the current
// scope filter.
// ============================================================================

import {
  agentIdFromDeploymentLabel,
  deploymentMatchesHarness,
  deploymentWithSpecViolations,
  isCopyReadByAgent,
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
  const agent = agentIdFromDeploymentLabel(harness);
  const matches = deployments.filter(
    (d) =>
      deploymentMatchesHarness(d, harness) &&
      (agent === null || agent === "shared" || isCopyReadByAgent(skill, d, agent)),
  );
  const local = isProjectScope(scope)
    ? matches.filter((d) => d.project_path === scope.project)
    : matches;
  const choices = local.length ? local : matches;
  return choices.find((d) => d.agent === harness) ?? choices[0];
}

/**
 * Open the selected copy's physical file, or the broken link itself for repair.
 * `reader` is the skill with its plugin copies, which the list leaves out of
 * `skill`: with an agent filter, the row opens a copy that agent reads (the
 * reader rule, `isCopyReadByAgent`), so a disabled own copy gives way to its
 * live plugin copy.
 */
export function deploymentForScope(
  skill: InstalledSkill,
  scope: SkillListFilter["scope"],
  harness?: string,
  reader: InstalledSkill = skill,
): string | undefined {
  const selected =
    selectListDeployment(skill, scope, harness) ??
    (harness ? selectListDeployment(reader, scope, harness) : undefined);
  if (!selected || isUnresolvedDeployment(selected)) return selected?.path;
  const backing = selected.backing;
  if (backing.kind === "linked-to") {
    return reader.deployments.find((d) => d.id === backing.deployment_id)?.path ?? selected.path;
  }
  return selected.path;
}
