// ============================================================================
// skill-list-model tests
// Guards that grouping skills into the three state buckets accounts for every
// skill exactly once, before the virtualizer trims what's painted.
// ============================================================================

import fc from "fast-check";
import { describe, expect, it } from "vitest";
import { buildHarnessSnapshot } from "../dev/harness/skill-fixture";
import { groupSkillRows } from "./skill-list-model";

const snapshot = buildHarnessSnapshot(400);

describe("groupSkillRows", () => {
  it("puts every skill in exactly one state bucket, and rows lists the buckets in order", () => {
    fc.assert(
      fc.property(
        fc.subarray(snapshot.skills),
        fc.constantFrom("name", "used", "size"),
        (skills, sort) => {
          const { buckets, rows } = groupSkillRows(skills, sort, snapshot.invocations);
          const bucketed = [...buckets.attention, ...buckets.healthy, ...buckets.parked];
          expect(rows).toEqual(bucketed);
          expect([...bucketed].sort(byName)).toEqual([...skills].sort(byName));
        },
      ),
    );
  });
});

function byName(a: { name: string }, b: { name: string }): number {
  return a.name < b.name ? -1 : a.name > b.name ? 1 : 0;
}
