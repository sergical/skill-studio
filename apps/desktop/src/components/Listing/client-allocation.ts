import type { HarnessListing, ListingEntry } from "@skill-studio/lib";

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

// Claude's allocation depends on the model window. Without a catalog match the
// simulator still works, but its result cannot describe the user's configuration.
export function hasAssumedClaudeWindow(row: HarnessListing) {
  return row.harness === "claude-code" && row.uses_default_context_window;
}

export function budgetRule(row: HarnessListing) {
  if (row.harness === "claude-code") {
    const fraction = Number(row.setting?.current ?? 0.01);
    return `${Number((fraction * 100).toFixed(4))}% of context`;
  }
  if (row.harness === "codex") {
    if (row.setting?.current != null)
      return `${Number(row.setting.current).toLocaleString()} tokens configured`;
    return row.context_window_tokens ? "2% of context" : "Fallback for unknown context";
  }
  return "Allocation rule unknown";
}
