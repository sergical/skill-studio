// ============================================================================
// Skill Studio - skill scope model
// Where a skill lives across Global and projects, for the cross-scope notes
// on the Add skill sheet and the Locations card.
// ============================================================================

import type { Deployment, InstalledSkill, InstallScope } from "@skill-studio/lib";

export interface ScopePresence {
  global: boolean;
  /** Project folders holding a live copy, sorted. */
  projectPaths: string[];
}

/** Live copies only: a parked copy is switched off, so it does not count. A plugin copy counts as global, as on the Locations card. */
export function scopePresence(deployments: readonly Deployment[]): ScopePresence {
  let global = false;
  const projects = new Set<string>();
  for (const d of deployments) {
    if (d.scope === "parked") continue;
    if (d.scope === "project" && d.project_path) projects.add(d.project_path);
    else global = true;
  }
  return { global, projectPaths: [...projects].sort() };
}

/** Marker text for one scope group, or null when the skill is in that scope only. */
export function scopeMarker(
  presence: ScopePresence,
  group: { isGlobal: boolean; projectPath?: string },
): string | null {
  if (group.isGlobal) {
    const count = presence.projectPaths.length;
    if (count === 0) return null;
    return `Also in ${count} ${count === 1 ? "project" : "projects"}`;
  }
  const isLiveHere =
    group.projectPath !== undefined && presence.projectPaths.includes(group.projectPath);
  return isLiveHere && presence.global ? "Also global" : null;
}

function projectName(path: string): string {
  return path.split("/").filter(Boolean).pop() ?? path;
}

function joinNames(names: string[]): string {
  if (names.length <= 2) return names.join(" and ");
  return `${names[0]} and ${names.length - 1} more`;
}

/**
 * The note for installing `skillName` into `scope`, when a live copy already
 * sits in the other scope. Null when nothing needs saying. A different
 * project's copy does not matter to a project install.
 */
export function otherScopeNote(
  skills: readonly InstalledSkill[],
  skillName: string,
  scope: InstallScope,
  projectPath: string | null,
): string | null {
  const skill = skills.find((s) => s.name === skillName);
  if (!skill) return null;
  const presence = scopePresence(skill.deployments);

  if (scope === "project") {
    if (!presence.global) return null;
    return "Already installed globally. Installing here adds a second copy for this project.";
  }
  const others = presence.projectPaths.filter((p) => p !== projectPath);
  if (others.length === 0) return null;
  return `Already in ${joinNames(others.map(projectName))}. A global copy will apply to every project.`;
}
