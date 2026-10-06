// ============================================================================
// Skill Studio - skill-upstream-note
// The "Forked from mattpocock/skills, which has 12 changes this fork doesn't." line under an
// installed skill's source line. Informational; never an update.
// ============================================================================

import type { Deployment, InstalledSkill, UpstreamAhead } from "@skill-studio/lib";

export interface SkillUpstreamNote {
  /** "Forked from mattpocock/skills, which has 12 changes this fork doesn't." */
  text: string;
  /** The GitHub compare page that lists those changes. */
  href: string;
}

/** The note for the page's deployment, when a dotagents or skills.sh fork behind its original owns it. */
export function skillUpstreamNote(
  skill: InstalledSkill,
  deployment: Deployment | undefined,
  upstreamAhead: UpstreamAhead[],
): SkillUpstreamNote | null {
  if (skill.source_kind !== "dotagents" && skill.source_kind !== "skills-sh") return null;
  // A global and a project copy of one name can come from different forks, so only the shown
  // copy's owner counts. A dotagents-only install has `source: "local"`, hence the owner id.
  const ownerId = deployment?.owner_id;
  if (ownerId == null) return null;
  const record = upstreamAhead.find((r) => r.owner_ids.includes(ownerId));
  if (!record) return null;
  const changes = record.behind_by === 1 ? "1 change" : `${record.behind_by} changes`;
  return {
    text: `Forked from ${record.upstream_repo}, which has ${changes} this fork doesn't.`,
    href: record.compare_url,
  };
}
