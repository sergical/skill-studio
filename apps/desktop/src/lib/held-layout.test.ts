import fc from "fast-check";
import { describe, expect, it } from "vitest";
import { holdLayout } from "./held-layout";

type Group = "update" | "healthy";
interface Item {
  name: string;
  v: number;
}
const item = (name: string, v = 0): Item => ({ name, v });
const key = (i: Item) => i.name;
const names = (layout: Readonly<Record<Group, readonly Item[]>>) => ({
  update: layout.update.map(key),
  healthy: layout.healthy.map(key),
});

describe("holdLayout", () => {
  it("keeps an updated skill in its old group and slot, with its fresh data", () => {
    // Flow: the user updates b in Updates; the rescan files b under Healthy.
    // Expect: b stays second in Updates and shows v1. Fails if the row jumps groups.
    const prev = { update: [item("a"), item("b"), item("c")], healthy: [item("d")] };
    const next = { update: [item("a"), item("c")], healthy: [item("b", 1), item("d")] };
    const out = holdLayout(prev, next, key, key, new Set(["b"]));
    expect(names(out)).toEqual({ update: ["a", "b", "c"], healthy: ["d"] });
    expect(out.update[1].v).toBe(1);
  });

  it("keeps a removed skill in its slot with its last data", () => {
    // Flow: the user removes b; the rescan drops it. Expect: the row stays until the list resets.
    const prev = { update: [item("a"), item("b", 7)], healthy: [] };
    const next = { update: [item("a")], healthy: [] };
    const out = holdLayout(prev, next, key, key, new Set(["b"]));
    expect(names(out).update).toEqual(["a", "b"]);
    expect(out.update[1].v).toBe(7);
  });

  it("gives a held row that left the data its fresh copy from the lookup", () => {
    // Flow: Park on b under a "not parked" filter; the rescan drops b from the filtered list but
    // the full list has b parked. Expect: b keeps its slot and shows the parked copy (v2), not the
    // copy from before the act. Fails if the held row keeps stale data.
    const prev = { update: [item("a"), item("b")], healthy: [] };
    const next = { update: [item("a")], healthy: [] };
    const out = holdLayout(prev, next, key, key, new Set(["b"]), (name) =>
      name === "b" ? item("b", 2) : undefined,
    );
    expect(names(out).update).toEqual(["a", "b"]);
    expect(out.update[1].v).toBe(2);
  });

  it("lets rows nobody acted on follow the new data", () => {
    // Flow: a background rescan adds e while b is held. Expect: e appears, b holds its slot.
    const prev = { update: [item("a"), item("b")], healthy: [] };
    const next = { update: [item("a"), item("e")], healthy: [item("b")] };
    const out = holdLayout(prev, next, key, key, new Set(["b"]));
    expect(names(out)).toEqual({ update: ["a", "b", "e"], healthy: [] });
  });

  it("returns the next layout untouched when nothing is held or there is no earlier layout", () => {
    const next = { update: [item("a")], healthy: [item("b")] };
    expect(holdLayout(null, next, key, key, new Set(["a"]))).toBe(next);
    expect(holdLayout(next, next, key, key, new Set())).toBe(next);
  });

  it("keeps every row once, holds each held row in its old group, and is stable when laid out again", () => {
    // The hook feeds each result back as `prev`; a second pass must not move anything.
    const NAMES = ["a", "b", "c", "d", "e", "f"];
    const layout = fc.shuffledSubarray(NAMES).chain((order) =>
      fc
        .array(fc.constantFrom<Group>("update", "healthy"), {
          minLength: order.length,
          maxLength: order.length,
        })
        .map((groups) => ({
          update: order.filter((_, i) => groups[i] === "update").map((n) => item(n)),
          healthy: order.filter((_, i) => groups[i] === "healthy").map((n) => item(n, 1)),
        })),
    );
    fc.assert(
      fc.property(layout, layout, fc.subarray(NAMES), (prev, next, heldNames) => {
        const held = new Set(heldNames);
        const once = holdLayout(prev, next, key, key, held);
        expect(names(holdLayout(once, next, key, key, held))).toEqual(names(once));

        const shown = [...once.update, ...once.healthy].map(key);
        const heldBefore = [...prev.update, ...prev.healthy].map(key).filter((n) => held.has(n));
        const expected = new Set([...next.update, ...next.healthy].map(key).concat(heldBefore));
        expect([...shown].sort()).toEqual([...expected].sort());
        for (const group of ["update", "healthy"] as const) {
          for (const row of prev[group].filter((r) => held.has(key(r)))) {
            expect(once[group].map(key), `${row.name} left ${group}`).toContain(row.name);
          }
        }
      }),
    );
  });

  it("leaves a held skill that was not on screen where the new data puts it", () => {
    // A bulk action can hold names the filter hid; they have no slot to keep.
    const prev = { update: [item("a")], healthy: [] };
    const next = { update: [item("a")], healthy: [item("z")] };
    expect(names(holdLayout(prev, next, key, key, new Set(["z"]))).healthy).toEqual(["z"]);
  });
});
