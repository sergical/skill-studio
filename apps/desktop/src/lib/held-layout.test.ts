import { describe, expect, it } from "vitest";
import { holdLayout } from "./held-layout";

type Group = "attention" | "healthy";
interface Item {
  name: string;
  v: number;
}
const item = (name: string, v = 0): Item => ({ name, v });
const key = (i: Item) => i.name;
const names = (layout: Readonly<Record<Group, readonly Item[]>>) => ({
  attention: layout.attention.map(key),
  healthy: layout.healthy.map(key),
});

describe("holdLayout", () => {
  it("keeps an updated skill in its old group and slot, with its fresh data", () => {
    // Flow: the user updates b in Needs attention; the rescan files b under Healthy.
    // Expect: b stays second in Needs attention and shows v1. Fails if the row jumps groups.
    const prev = { attention: [item("a"), item("b"), item("c")], healthy: [item("d")] };
    const next = { attention: [item("a"), item("c")], healthy: [item("b", 1), item("d")] };
    const out = holdLayout(prev, next, key, key, new Set(["b"]));
    expect(names(out)).toEqual({ attention: ["a", "b", "c"], healthy: ["d"] });
    expect(out.attention[1].v).toBe(1);
  });

  it("keeps a removed skill in its slot with its last data", () => {
    // Flow: the user removes b; the rescan drops it. Expect: the row stays until the list resets.
    const prev = { attention: [item("a"), item("b", 7)], healthy: [] };
    const next = { attention: [item("a")], healthy: [] };
    const out = holdLayout(prev, next, key, key, new Set(["b"]));
    expect(names(out).attention).toEqual(["a", "b"]);
    expect(out.attention[1].v).toBe(7);
  });

  it("gives a held row that left the data its fresh copy from the lookup", () => {
    // Flow: Park on b under a "not parked" filter; the rescan drops b from the filtered list but
    // the full list has b parked. Expect: b keeps its slot and shows the parked copy (v2), not the
    // copy from before the act. Fails if the held row keeps stale data.
    const prev = { attention: [item("a"), item("b")], healthy: [] };
    const next = { attention: [item("a")], healthy: [] };
    const out = holdLayout(prev, next, key, key, new Set(["b"]), (name) =>
      name === "b" ? item("b", 2) : undefined,
    );
    expect(names(out).attention).toEqual(["a", "b"]);
    expect(out.attention[1].v).toBe(2);
  });

  it("lets rows nobody acted on follow the new data", () => {
    // Flow: a background rescan adds e while b is held. Expect: e appears, b holds its slot.
    const prev = { attention: [item("a"), item("b")], healthy: [] };
    const next = { attention: [item("a"), item("e")], healthy: [item("b")] };
    const out = holdLayout(prev, next, key, key, new Set(["b"]));
    expect(names(out)).toEqual({ attention: ["a", "b", "e"], healthy: [] });
  });

  it("returns the next layout untouched when nothing is held or there is no earlier layout", () => {
    const next = { attention: [item("a")], healthy: [item("b")] };
    expect(holdLayout(null, next, key, key, new Set(["a"]))).toBe(next);
    expect(holdLayout(next, next, key, key, new Set())).toBe(next);
  });

  it("is stable when laid out again over its own result", () => {
    // The hook feeds each result back as `prev`; a second pass must not move anything.
    const prev = { attention: [item("a"), item("b"), item("c")], healthy: [item("d")] };
    const next = { attention: [item("c"), item("a")], healthy: [item("d"), item("b")] };
    const held = new Set(["b"]);
    const once = holdLayout(prev, next, key, key, held);
    expect(names(holdLayout(once, next, key, key, held))).toEqual(names(once));
  });

  it("leaves a held skill that was not on screen where the new data puts it", () => {
    // A bulk action can hold names the filter hid; they have no slot to keep.
    const prev = { attention: [item("a")], healthy: [] };
    const next = { attention: [item("a")], healthy: [item("z")] };
    expect(names(holdLayout(prev, next, key, key, new Set(["z"]))).healthy).toEqual(["z"]);
  });
});
