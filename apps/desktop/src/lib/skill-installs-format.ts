// ============================================================================
// Skill Studio - skill-installs-format
// How skills.sh install counts read in the UI, shared by Browse and the
// installed skill's source line.
// ============================================================================

/** 1,000+ installs show as e.g. "1.2k". */
export function formatInstalls(count: number): string {
  if (count >= 1000) {
    return `${(count / 1000).toFixed(1)}k`;
  }
  return count.toString();
}
