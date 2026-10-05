// ============================================================================
// Skill Studio - skill scope model
// Where a skill lives across Global and projects, for the cross-scope notes
// on the Add skill sheet and the Locations card.
// ============================================================================

import type { Deployment, InstalledSkill, InstallScope } from "@skill-studio/lib";

export interface ScopePresence {
  /** A live copy the user installed globally. */
  global: boolean;
  /** A live copy shipped by a plugin. The Locations card lists it under Global, but nobody installed it. */
  plugin: boolean;
  /** Project folders holding a live copy, sorted. */
  projectPaths: string[];
}

/** Live copies only: a parked copy is switched off, so it does not count. */
export function scopePresence(deployments: readonly Deployment[]): ScopePresence {
  let global = false;
  let plugin = false;
  const projects = new Set<string>();
  for (const d of deployments) {
    if (d.scope === "parked") continue;
    if (d.scope === "plugin") plugin = true;
    else if (d.scope === "project" && d.project_path) projects.add(d.project_path);
    else global = true;
  }
  return { global, plugin, projectPaths: [...projects].sort() };
}

/** Marker text for one scope group, or null when the skill is live in that scope only. */
export function scopeMarker(
  presence: ScopePresence,
  group: { isGlobal: boolean; projectPath?: string },
): string | null {
  const liveGlobally = presence.global || presence.plugin;
  if (group.isGlobal) {
    const count = presence.projectPaths.length;
    if (!liveGlobally || count === 0) return null;
    return `Also in ${count} ${count === 1 ? "project" : "projects"}`;
  }
  const isLiveHere =
    group.projectPath !== undefined && presence.projectPaths.includes(group.projectPath);
  return isLiveHere && liveGlobally ? "Also global" : null;
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
 * sits in the other scope. Null when nothing needs saying. A project install
 * ignores other projects and plugin copies, which Claude Code namespaces.
 * `named` puts the skill name in the note, for an install of several skills.
 */
export function otherScopeNote(
  skills: readonly InstalledSkill[],
  skillName: string,
  scope: InstallScope,
  named = false,
): string | null {
  const skill = skills.find((s) => s.name === skillName);
  if (!skill) return null;
  const presence = scopePresence(skill.deployments);
  const already = named ? `${skillName} is already` : "Already";

  if (scope === "project") {
    if (!presence.global) return null;
    return `${already} installed globally. Installing here adds a second copy for this project.`;
  }
  if (presence.projectPaths.length === 0) return null;
  return `${already} in ${joinNames(presence.projectPaths.map(projectName))}. A global copy will apply to every project.`;
}
