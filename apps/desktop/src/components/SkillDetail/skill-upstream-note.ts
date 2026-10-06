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

/**
 * Reduces a lock-file source (`git:https://github.com/o/r.git`, `https://github.com/o/r`,
 * `o/r#ref`, `o/r@ref`) to lowercase `owner/repo`; null for anything else, such as a GitLab URL.
 */
export function githubRepoOfSource(source: string): string | null {
  const rest = source
    .trim()
    .replace(/^git:/, "")
    .replace(/^(?:https?:\/\/github\.com\/|git@github\.com:)/i, "")
    .replace(/[#@].*$/, "")
    .replace(/\/+$/, "")
    .replace(/\.git$/, "");
  return /^[\w.-]+\/[\w.-]+$/.test(rest) ? rest.toLowerCase() : null;
}

/** The note for a dotagents or skills.sh skill whose source repo is a fork behind its original. */
export function skillUpstreamNote(
  skill: InstalledSkill,
  upstreamAhead: UpstreamAhead[],
): SkillUpstreamNote | null {
  if (skill.source_kind !== "dotagents" && skill.source_kind !== "skills-sh") return null;
  const source = githubRepoOfSource(skill.source);
  if (!source) return null;
  const record = upstreamAhead.find((r) => r.repo.toLowerCase() === source);
  if (!record) return null;
  const changes = record.behind_by === 1 ? "1 change" : `${record.behind_by} changes`;
  return {
    text: `Forked from ${record.upstream_repo}, which has ${changes} this fork doesn't.`,
    href: record.compare_url,
  };
}
