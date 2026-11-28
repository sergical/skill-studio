import type { HarnessListing, ListingEntry, SkillInvocationStats } from "@skill-studio/lib";

export function listedEntries(row: HarnessListing) {
  return row.entries.filter((e) => e.state !== "hidden" && e.state !== "omitted");
}

export function entrySource(entry: ListingEntry) {
  if (!entry.scanned) return "Session record";
  return entry.plugin ? "Plugin" : "Skill folder";
}

export function sourceCounts(entries: ListingEntry[]) {
  const counts = { "Skill folder": 0, Plugin: 0, "Session record": 0 };
  for (const entry of entries) counts[entrySource(entry)]++;
  return counts;
}

/** Skills per folder, the folder a skill's own directory sits in, largest first. */
export function folderCounts(entries: ListingEntry[]): [string, number][] {
  const counts = new Map<string, number>();
  for (const entry of entries) {
    if (!entry.path) continue;
    const folder = entry.path.replace(/\/SKILL\.md$/i, "").replace(/\/[^/]+\/?$/, "");
    counts.set(folder, (counts.get(folder) ?? 0) + 1);
  }
  return [...counts].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
}

// Codex can list two copies with the same name. Match one entry on each side at a time.
export function additionalEntries(entries: ListingEntry[], other: ListingEntry[]) {
  const remaining = new Map<string, number>();
  for (const e of other) remaining.set(e.name, (remaining.get(e.name) ?? 0) + 1);
  return entries.filter((e) => {
    const count = remaining.get(e.name) ?? 0;
    if (!count) return true;
    remaining.set(e.name, count - 1);
    return false;
  });
}

// Claude's budget depends on the model window. Without a catalog match the
// simulator still works, but its result cannot describe the user's configuration.
export function hasAssumedClaudeWindow(row: HarnessListing) {
  return row.harness === "claude-code" && row.uses_default_context_window;
}

/** True when the cuts in this skill list can be stated as facts: a preview with a budget and a
 * known model window. Name only, Not listed and park advice all need it. */
export function hasBudgetEstimate(row: HarnessListing) {
  return row.evidence === "preview" && row.budget !== null && !hasAssumedClaudeWindow(row);
}

/** True when any history record names the agent; only then can its zero mean "no uses". */
export function hasReadableHistory(invocations: SkillInvocationStats[], agent: string) {
  return invocations.some(
    (stats) =>
      agent in stats.by_harness_30_days || stats.by_hour.some((hour) => hour.harness === agent),
  );
}

/** Each skill's uses by this one agent in 30 days, or null when the agent has no readable history. */
export function agentUses30Days(
  invocations: SkillInvocationStats[],
  agent: string,
): Map<string, number> | null {
  if (!hasReadableHistory(invocations, agent)) return null;
  return new Map(
    invocations.map((stats) => [stats.skill, stats.by_harness_30_days[agent] ?? 0] as const),
  );
}

export function budgetRule(row: HarnessListing) {
  if (row.harness === "claude-code") {
    const fraction = Number(row.setting?.current ?? 0.01);
    return `${Number((fraction * 100).toFixed(4))}% of context`;
  }
  if (row.harness === "codex") {
    if (row.setting?.current != null)
      return `${Number(row.setting.current).toLocaleString()} tokens configured`;
    return row.context_window_tokens ? "2% of context" : "Default for an unknown context window";
  }
  return "Budget rule unknown";
}
