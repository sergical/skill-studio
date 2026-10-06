// ============================================================================
// Skill Studio - skill-upstream-note
// The "mattpocock/skills has 12 changes your fork doesn't." line under an
// installed skill's source line. Informational; never an update.
// ============================================================================

import type { InstalledSkill, UpstreamAhead } from "@skill-studio/lib";

export interface SkillUpstreamNote {
  /** "mattpocock/skills has 12 changes your fork doesn't." */
  text: string;
  /** The GitHub compare page that lists those changes. */
  href: string;
}

/** The note for a dotagents or skills.sh skill whose source repo is a fork behind its original. */
export function skillUpstreamNote(
  skill: InstalledSkill,
  upstreamAhead: UpstreamAhead[],
): SkillUpstreamNote | null {
  if (skill.source_kind !== "dotagents" && skill.source_kind !== "skills-sh") return null;
  const source = skill.source.toLowerCase();
  const record = upstreamAhead.find((r) => r.repo.toLowerCase() === source);
  if (!record) return null;
  const changes = record.behind_by === 1 ? "1 change" : `${record.behind_by} changes`;
  return {
    text: `${record.upstream_repo} has ${changes} your fork doesn't.`,
    href: record.compare_url,
  };
}
