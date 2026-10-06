// ============================================================================
// Skill Studio - skill-upstream-note
// The "Forked from mattpocock/skills, which has 12 changes this fork doesn't." line under an
// installed skill's source line. Informational; never an update.
// ============================================================================

import type { InstalledSkill, UpstreamAhead } from "@skill-studio/lib";

export interface SkillUpstreamNote {
  /** "Forked from mattpocock/skills, which has 12 changes this fork doesn't." */
  text: string;
  /** The GitHub compare page that lists those changes. */
  href: string;
}

/** The note for a dotagents or skills.sh skill installed from a fork behind its original. */
export function skillUpstreamNote(
  skill: InstalledSkill,
  upstreamAhead: UpstreamAhead[],
): SkillUpstreamNote | null {
  if (skill.source_kind !== "dotagents" && skill.source_kind !== "skills-sh") return null;
  // A dotagents-only install has `source: "local"`, so match on the lifecycle owner instead.
  const record = upstreamAhead.find((r) =>
    skill.deployments.some((d) => d.owner_id != null && r.owner_ids.includes(d.owner_id)),
  );
  if (!record) return null;
  const changes = record.behind_by === 1 ? "1 change" : `${record.behind_by} changes`;
  return {
    text: `Forked from ${record.upstream_repo}, which has ${changes} this fork doesn't.`,
    href: record.compare_url,
  };
}
