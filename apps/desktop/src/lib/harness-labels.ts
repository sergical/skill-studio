// ============================================================================
// harness-labels - Shared harness-label data (Claude Code / Codex / OpenCode
// / pi), consumed by SkillHistorySection.
// ============================================================================

/** Display label for each first-class harness, in the order the control shows them. */
export const HARNESS_LABELS = [
  ["claude-code", "Claude Code"],
  ["codex", "Codex"],
  ["opencode", "OpenCode"],
  ["pi", "pi"],
] satisfies [string, string][];

const AGENT_LABELS = new Map([
  ["claude-code", "Claude Code"],
  ["codex", "Codex"],
  ["opencode", "OpenCode"],
  ["pi", "pi"],
  ["cursor", "Cursor"],
  ["grok-build", "Grok Build"],
]);

/** Display label for any agent id, or the id itself when it is not first-class. */
export function harnessLabel(id: string): string {
  return AGENT_LABELS.get(id) ?? id;
}
