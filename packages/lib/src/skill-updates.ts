// ============================================================================
// Skill Studio - skill-updates
// `update-available` moved out of HealthIssue (an update isn't a problem to
// fix, just something to act on) into its own list, shared by Home's
// "Updates" section and the skill page.
// ============================================================================

import type { InstalledSkill, SkillSnapshot } from "./skill-types";

/** Whether the background update check found a newer commit upstream for `skill`. */
export function hasUpdate(skill: InstalledSkill): boolean {
  return skill.update_owner_ids.length > 0;
}

/** Every skill with a newer commit available upstream, per the background update check. */
export function skillsWithUpdates(snapshot: SkillSnapshot): InstalledSkill[] {
  return snapshot.skills.filter(hasUpdate);
}
