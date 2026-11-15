// ============================================================================
// Skill Studio - Held layout
// Keeps the rows the user acted on where they were while the data under them changes
// ============================================================================

import { useState } from "react";
import { useAppStore } from "../store/appStore";

export type GroupedItems<G extends string, T> = Readonly<Record<G, readonly T[]>>;

const NO_NAMES: ReadonlySet<string> = new Set();

/**
 * Lays out `next` with every held item at its `prev` group and index. A held item that left its
 * group (an updated skill that no longer needs attention) or left the data (a skill an update took
 * out of the filter) stays in its old slot - with its fresh data from `next`, else from `latest`.
 * Items that are not held take the order `next` gives them.
 */
export function holdLayout<G extends string, T>(
  prev: GroupedItems<G, T> | null,
  next: GroupedItems<G, T>,
  keyOf: (item: T) => string,
  nameOf: (item: T) => string,
  held: ReadonlySet<string>,
  latest: (key: string) => T | undefined = () => undefined,
) {
  if (!prev || held.size === 0) return next;
  // SAFETY: both records have the same group keys; `G` is a closed string union.
  const groups = Object.keys(next) as G[];
  const prevKeys = new Set(groups.flatMap((g) => (prev[g] ?? []).map(keyOf)));
  const fresh = new Map(groups.flatMap((g) => next[g].map((item) => [keyOf(item), item] as const)));
  const isHeld = (item: T) => held.has(nameOf(item)) && prevKeys.has(keyOf(item));

  // SAFETY: every group key is assigned in the loop below.
  const result = {} as Record<G, T[]>;
  for (const g of groups) {
    const items = next[g].filter((item) => !isHeld(item));
    (prev[g] ?? []).forEach((item, index) => {
      if (!held.has(nameOf(item))) return;
      items.splice(
        Math.min(index, items.length),
        0,
        fresh.get(keyOf(item)) ?? latest(keyOf(item)) ?? item,
      );
    });
    result[g] = items;
  }
  return result;
}

function sameLayout<G extends string, T>(
  a: GroupedItems<G, T> | null,
  b: GroupedItems<G, T>,
  keyOf: (item: T) => string,
): boolean {
  if (!a) return false;
  // SAFETY: both records have the same group keys.
  return (Object.keys(b) as G[]).every(
    (g) => a[g]?.length === b[g].length && a[g].every((item, i) => keyOf(item) === keyOf(b[g][i])),
  );
}

/**
 * `holdLayout` over the store's `rowActs`, remembering the layout it last rendered. A change of
 * `resetKey` (the filter, the sort) lays the list out fresh, because the user asked for a new
 * order; the rows keep what they say ("Updated"), and a running act keeps its row busy, but a
 * held row the new order leaves out is gone. `latest` finds a held row's data outside `next`.
 */
export function useHeldLayout<G extends string, T>(
  next: GroupedItems<G, T>,
  keyOf: (item: T) => string,
  nameOf: (item: T) => string,
  resetKey: string,
  latest?: (key: string) => T | undefined,
): GroupedItems<G, T> {
  const rowActs = useAppStore((state) => state.rowActs);
  const [memory, setMemory] = useState<{ resetKey: string; layout: GroupedItems<G, T> | null }>(
    () => ({ resetKey, layout: null }),
  );
  const fresh = memory.resetKey !== resetKey;
  const held = fresh ? NO_NAMES : new Set(rowActs.keys());
  const layout = holdLayout(fresh ? null : memory.layout, next, keyOf, nameOf, held, latest);
  if (fresh || !sameLayout(memory.layout, layout, keyOf)) setMemory({ resetKey, layout });

  return layout;
}
